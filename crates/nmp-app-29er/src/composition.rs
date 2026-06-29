//! 29er composition root (ADR-0069 explicit composition).
//!
//! The single Rust entry point that turns a fresh, unstarted NMP app host into
//! a fully-composed 29er app: the canonical NMP default set, the NIP-29 action
//! namespaces, and the NIP-29 group-create defaults projection seeded with
//! 29er's operator-policy public-group relay.
//!
//! It is shared by BOTH 29er shells:
//! * [`crate::TwentyNinerApp`] (the UniFFI object) calls it on its owned
//!   `nmp-native-runtime` app before `start`;
//! * the native Rust TUI calls it on its `NmpAppBuilder` before `start`.
//!
//! Composition lives here (not in `nmp-defaults`) because NIP-29 is NOT part of
//! the canonical composition every Nostr app inherits — a notes-only app would
//! not register it. This is the per-app boundary D0 keeps outside `nmp-core`.

use nmp_core::substrate::AppHost;

/// Compose the full 29er protocol surface onto a pre-start app host.
///
/// `app` is any [`AppHost`] — an `NmpAppBuilder` (TUI) or the live, unstarted
/// `NmpApp` owned by [`crate::TwentyNinerApp`]. MUST be called before the
/// runtime is started so the action registry and the group-defaults snapshot
/// are in place for the first tick.
///
/// Steps:
/// 1. Canonical NMP default composition (NIP-02 / NIP-17 / NIP-25 / NIP-65
///    action modules, routing substrate, DM-inbox + WOT + mute + search
///    runtimes, the D2 coverage hook). 29er is a full client, so the full
///    default set is the right baseline.
/// 2. The NIP-29 action namespaces (`nmp.nip29.discover` / `join` / etc.).
/// 3. The NIP-29 group-create defaults projection, pre-filled with 29er's
///    suggested public-group host relay ([`crate::config::public_group_relay_url`])
///    so the shell reads it from the projection instead of hardcoding it (D7).
pub fn compose_29er_runtime(app: &mut impl AppHost) {
    let _handles =
        nmp_defaults::register_defaults_with_handles(app, nmp_defaults::NmpDefaults::default());

    // 29er-specific: register the NIP-29 action namespaces. A collision means a
    // double-init of the same app (caller bug) — surface it loudly in debug.
    let registered = nmp_nip29::register::register_actions(app);
    debug_assert!(
        registered.is_ok(),
        "nmp-nip29 register_actions reported a namespace collision: {registered:?}"
    );
    let _ = registered;

    // Output-only projection (pure function of the supplied URL) — safe to wire
    // pre-start. `&*app` reborrows the `&mut impl AppHost` as the shared
    // `&impl SnapshotProjectionRegistrar` the wiring helper expects.
    nmp_nip29::register::wire_group_defaults_with_relay(
        &*app,
        crate::config::public_group_relay_url(),
    );
}
