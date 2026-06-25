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

use std::ffi::CString;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use nmp_defaults::{NmpAppBuilder, RunConfig};
use nmp_ffi::{
    nmp_app_free, nmp_app_set_update_callback, nmp_app_signin_nsec, nmp_app_stop, NmpApp,
};
use nmp_nip29::interest::relay_discovery_interest;
use nmp_nip29::register::open_group_discovery;
use nmp_nip29::wire::discovered_groups_fb::decode_discovered_groups_snapshot;

const DEFAULT_RELAY: &str = "wss://nip29.f7z.io";
const WAIT_SECS: u64 = 12;

static UPDATE_TX: OnceLock<Mutex<Option<Sender<()>>>> = OnceLock::new();

extern "C" fn update_signal_callback(_ctx: *mut std::ffi::c_void, _ptr: *const u8, _len: usize) {
    if let Some(slot) = UPDATE_TX.get() {
        if let Ok(guard) = slot.lock() {
            if let Some(tx) = guard.as_ref() {
                let _ = tx.send(());
            }
        }
    }
}

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
        eprintln!("shakeout: nmp_app_new returned null");
        std::process::exit(1);
    }

    let (tx, ticks) = channel::<()>();
    let slot = UPDATE_TX.get_or_init(|| Mutex::new(None));
    *slot.lock().unwrap() = Some(tx);
    nmp_app_set_update_callback(app, std::ptr::null_mut(), Some(update_signal_callback));

    println!("shakeout: adding relay {relay} (role=both)");
    let url_c = CString::new(relay.as_str()).unwrap();
    let role_c = CString::new("both").unwrap();
    nmp_ffi::nmp_app_add_relay(app, url_c.as_ptr(), role_c.as_ptr());

    println!("shakeout: signing in with supplied nsec (active)");
    let secret = CString::new(nsec).expect("nsec has no interior NUL");
    nmp_app_signin_nsec(app, secret.as_ptr(), 1);

    let app_ref: &NmpApp = unsafe { &*app };
    wait_for_active_account(app_ref, &ticks);

    println!("shakeout: opening group discovery projection on {relay}");
    let _discovery_handle = open_group_discovery(app_ref, relay.clone());
    println!("shakeout: pushing tailing discovery interest (39000/39001/39002)");
    app_ref.push_interest(relay_discovery_interest(&relay));

    println!("shakeout: waiting {WAIT_SECS}s for relay to stream metadata...");
    let deadline = Instant::now() + Duration::from_secs(WAIT_SECS);
    let mut last_count: usize = 0;
    while Instant::now() < deadline {
        match ticks.recv_timeout(Duration::from_secs(1)) {
            Ok(()) => {
                let count = current_group_count(app_ref);
                if count != last_count {
                    println!("shakeout: tick — discovered_groups rows = {count}");
                    last_count = count;
                }
            }
            Err(_) => {}
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
    nmp_app_set_update_callback(app, std::ptr::null_mut(), None);
    if let Some(slot) = UPDATE_TX.get() {
        *slot.lock().unwrap() = None;
    }
    nmp_app_stop(app);
    nmp_app_free(app);
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
