//! S01 integration check: connect to a NIP-29 relay, decode discovered groups,
//! and log them. Proves the nmp-nip29 DiscoveredGroupsProjection against live
//! relay data before any UI work.
//!
//! Run:
//!   SHAKEOUT_NSEC=nsec1... cargo run -p shakeout -- --relay wss://nip29.f7z.io
//!
//! The CLI boots an NMP kernel, signs in with the supplied nsec, opens group
//! discovery on the target relay (registering the typed projection AND pushing
//! the tailing interest), waits for snapshots to land, then reads the typed
//! DiscoveredGroups FlatBuffers sidecar and logs each row.
//!
//! Clean-break note: `nmp-ffi` is deleted (#2483). This shell now consumes
//! `nmp-native-runtime` directly — `NmpAppBuilder` composes + starts the app,
//! `set_update_listener` replaces the C update callback, `add_signer` replaces
//! `nmp_app_signin_nsec`, and `open_nip29_group_discovery_session` replaces the
//! old `open_group_discovery` read door.

use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

use nmp_native_runtime::{Nip29GroupDiscoverySession, NmpApp, NmpAppBuilder, RunConfig};
use nmp_nip29::wire::discovered_groups_fb::decode_discovered_groups_snapshot;

const DEFAULT_RELAY: &str = "wss://nip29.f7z.io";
const WAIT_SECS: u64 = 12;

fn main() {
    let relay = std::env::args()
        .nth(1)
        .or_else(|| {
            std::env::args()
                .position(|a| a == "--relay")
                .and_then(|i| std::env::args().nth(i + 1))
        })
        .unwrap_or_else(|| DEFAULT_RELAY.to_string());

    let nsec = std::env::var("SHAKEOUT_NSEC").unwrap_or_else(|_| {
        eprintln!("shakeout: SHAKEOUT_NSEC env var is required (NIP-42 AUTH on {relay})");
        std::process::exit(2);
    });

    println!("shakeout: relay={relay}");
    println!("shakeout: booting NMP kernel (in-memory store, no initial relays)");

    let mut builder = NmpAppBuilder::new();
    nmp_defaults::register_defaults(&mut builder);
    let app = builder
        .in_memory()
        .consume_all_builtin_projections()
        .without_initial_relays()
        .start(RunConfig::default());
    if app.is_null() {
        eprintln!("shakeout: builder.start() returned null");
        std::process::exit(1);
    }
    // SAFETY: `app` is a non-null `*mut NmpApp` leaked by `builder.start()`; it
    // stays valid until we reclaim the box at the end of `main`.
    let app_ref: &NmpApp = unsafe { &*app };

    // The update listener is `Arc<dyn Fn(&[u8]) + Send + Sync>` — a snapshot
    // tap. We only need a wakeup signal, so forward each frame as a unit tick.
    let (tx, ticks) = channel::<()>();
    let tick_tx = tx.clone();
    app_ref.set_update_listener(Some(Arc::new(move |_bytes: &[u8]| {
        let _ = tick_tx.send(());
    })));

    println!("shakeout: adding relay {relay} (role=both)");
    app_ref.add_relay(relay.clone(), "both".to_string());

    println!("shakeout: signing in with supplied nsec (active)");
    app_ref.add_signer(
        nmp_core::SignerSource::LocalNsec(zeroize::Zeroizing::new(nsec)),
        true,
    );

    wait_for_active_account(app_ref, &ticks);

    println!("shakeout: opening group discovery projection on {relay}");
    println!("shakeout: (open_nip29_group_discovery_session registers the typed projection AND opens the tailing interest internally)");
    let _discovery_handle =
        app_ref.open_nip29_group_discovery_session(Nip29GroupDiscoverySession::new(relay.clone()));

    println!("shakeout: waiting {WAIT_SECS}s for relay to stream metadata...");
    let deadline = Instant::now() + Duration::from_secs(WAIT_SECS);
    let mut last_count: usize = 0;
    while Instant::now() < deadline {
        if ticks.recv_timeout(Duration::from_secs(1)).is_ok() {
            let count = current_group_count(app_ref);
            if count != last_count {
                println!("shakeout: tick — discovered_groups rows = {count}");
                last_count = count;
            }
        }
    }

    println!("shakeout: ---- final discovered-groups snapshot ----");
    let typed = app_ref.run_typed_snapshot_projections();
    let mut logged = 0;
    for entry in &typed {
        if entry.key != "nmp.nip29.discovered_groups" {
            continue;
        }
        match decode_discovered_groups_snapshot(&entry.payload) {
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
    app_ref.set_update_listener(None);
    app_ref.shutdown();
    // SAFETY: `app` was produced by `builder.start()` (a leaked `Box<NmpApp>`);
    // reclaim it exactly once now that the actor is shut down and no listener
    // can still borrow it.
    unsafe { drop(Box::from_raw(app)) };
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
            if let Ok(snapshot) = decode_discovered_groups_snapshot(&entry.payload) {
                return snapshot.groups.len();
            }
        }
    }
    0
}
