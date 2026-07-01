//! S01 integration check: connect to a NIP-29 relay, decode discovered groups,
//! and log them. Proves the nmp-nip29 DiscoveredGroupsProjection against live
//! relay data before any UI work.
//!
//! Run:
//!   SHAKEOUT_NSEC=nsec1... cargo run -p shakeout -- --relay <wss://relay.example>
//!
//! The CLI boots an NMP runtime via 29er's own composition root
//! ([`nmp_app_29er::compose_29er_runtime`]) — the same composition the
//! `TwentyNinerApp` UniFFI facade and the native Rust TUI use — signs in with
//! the supplied nsec, opens group discovery on the target relay (registering
//! the typed projection AND pushing the tailing interest), waits for
//! snapshots to land, then reads the typed DiscoveredGroups FlatBuffers
//! sidecar and logs each row.
//!
//! Ported off the deleted `nmp-ffi` C-ABI (#2483) onto `nmp-native-runtime` +
//! `nmp-uniffi-support` directly — this CLI is a plain Rust consumer with no
//! Swift/UniFFI boundary, so it talks to the runtime types directly rather
//! than going through the `TwentyNinerApp` facade object.

use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use nmp_core::SignerSource;
use nmp_native_runtime::{new_app, Nip29GroupDiscoverySession, NmpApp};

const WAIT_SECS: u64 = 12;

static UPDATE_TX: OnceLock<Mutex<Option<Sender<()>>>> = OnceLock::new();

/// Update-sink target for [`nmp_uniffi_support::set_update_sink`]. Every
/// pushed `NMPU` frame just signals the wait loop to re-check the discovered
/// group count — the frame bytes themselves are not decoded here.
struct TickSink;

fn main() {
    let relay = relay_arg().unwrap_or_else(|| {
        eprintln!(
            "shakeout: --relay <relay-url> or SHAKEOUT_RELAY is required; relay selection is explicit input"
        );
        std::process::exit(2);
    });

    let nsec = std::env::var("SHAKEOUT_NSEC").unwrap_or_else(|_| {
        eprintln!("shakeout: SHAKEOUT_NSEC env var is required (NIP-42 AUTH on {relay})");
        std::process::exit(2);
    });

    println!("shakeout: relay={relay}");
    println!("shakeout: booting NMP runtime (29er composition, in-memory store)");

    let mut app = new_app();
    nmp_app_29er::compose_29er_runtime(&mut app);
    app.consume_all_builtin_projections();
    nmp_uniffi_support::start_runtime(&app, 0, 0);

    let (tx, ticks) = channel::<()>();
    let slot = UPDATE_TX.get_or_init(|| Mutex::new(None));
    *slot.lock().unwrap() = Some(tx);
    nmp_uniffi_support::set_update_sink(&app, Some(Box::new(TickSink)), |_sink, _frame| {
        if let Some(slot) = UPDATE_TX.get() {
            if let Ok(guard) = slot.lock() {
                if let Some(tx) = guard.as_ref() {
                    let _ = tx.send(());
                }
            }
        }
    });

    println!("shakeout: adding relay {relay} (role=both)");
    app.add_relay(relay.clone(), "both".to_string());

    println!("shakeout: signing in with supplied nsec (active)");
    app.add_signer(SignerSource::LocalNsec(zeroize::Zeroizing::new(nsec)), true);

    wait_for_active_account(&app, &ticks);

    println!("shakeout: opening group discovery session on {relay}");
    println!("shakeout: (open_nip29_group_discovery_session_with_reader registers the typed projection AND opens the tailing interest internally)");
    let (_discovery_handle, _reader) = app.open_nip29_group_discovery_session_with_reader(
        Nip29GroupDiscoverySession::new(relay.clone()),
    );

    println!("shakeout: waiting {WAIT_SECS}s for relay to stream metadata...");
    let deadline = Instant::now() + Duration::from_secs(WAIT_SECS);
    let mut last_count: usize = 0;
    while Instant::now() < deadline {
        if ticks.recv_timeout(Duration::from_secs(1)).is_ok() {
            let count = current_group_count(&app);
            if count != last_count {
                println!("shakeout: tick — discovered_groups rows = {count}");
                last_count = count;
            }
        }
    }

    println!("shakeout: ---- final discovered-groups snapshot ----");
    let typed = app.run_typed_snapshot_projections();
    let mut logged = 0;
    for entry in &typed {
        if entry.key != "nmp.nip29.discovered_groups" {
            continue;
        }
        match nmp_nip29::decode_discovered_groups_snapshot(&entry.payload) {
            Ok(snapshot) => {
                println!(
                    "shakeout: host_relay_url={} groups={}",
                    snapshot.host_relay_url,
                    snapshot.groups.len()
                );
                for g in &snapshot.groups {
                    println!(
                        "shakeout:   id={:<24} name={:?} members={} admins={} public={} open={} parent={:?} children={}",
                        g.group_id,
                        g.name,
                        g.member_count,
                        g.admin_count,
                        g.public,
                        g.open,
                        g.parent,
                        g.children.len(),
                    );
                    logged += 1;
                }
            }
            Err(e) => {
                eprintln!("shakeout: decode error: {e}");
            }
        }
    }
    if logged == 0 {
        eprintln!("shakeout: no discovered_groups rows decoded (relay may require AUTH, or no groups present)");
    }

    println!("shakeout: shutting down");
    if let Some(slot) = UPDATE_TX.get() {
        *slot.lock().unwrap() = None;
    }
    app.shutdown();
}

fn relay_arg() -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--relay" {
            return args.next().filter(|relay| !relay.trim().is_empty());
        }
        if !arg.starts_with('-') && !arg.trim().is_empty() {
            return Some(arg);
        }
    }
    std::env::var("SHAKEOUT_RELAY")
        .ok()
        .filter(|relay| !relay.trim().is_empty())
}

fn wait_for_active_account(app: &NmpApp, ticks: &Receiver<()>) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if ticks.recv_timeout(Duration::from_secs(1)).is_err() {
            continue;
        }
        if let Ok(guard) = app.active_account_handle().lock() {
            if guard.is_some() {
                println!("shakeout: active account slot populated");
                return;
            }
        }
    }
    eprintln!("shakeout: WARN — active account slot still empty after 5s (continuing)");
}

fn current_group_count(app: &NmpApp) -> usize {
    let typed = app.run_typed_snapshot_projections();
    for entry in &typed {
        if entry.key == "nmp.nip29.discovered_groups" {
            if let Ok(snapshot) = nmp_nip29::decode_discovered_groups_snapshot(&entry.payload) {
                return snapshot.groups.len();
            }
        }
    }
    0
}
