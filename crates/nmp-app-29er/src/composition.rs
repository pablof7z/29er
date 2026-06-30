//! 29er composition root (ADR-0069 explicit composition).
//!
//! The single Rust entry point that turns a fresh, unstarted NMP app host into
//! a fully-composed 29er app: the substrate floor (routing, NIP-65 mailbox
//! cache, coverage gate, blocked-relay handling — everything `nmp-substrate`
//! owns), the NIP-29 action namespaces, and the NIP-29 group-create defaults
//! projection seeded with 29er's operator-policy public-group relay.
//!
//! It is shared by BOTH 29er shells:
//! * [`crate::TwentyNinerApp`] (the UniFFI facade) calls it on its own,
//!   unstarted `nmp-native-runtime` app inside its constructor;
//! * the native Rust TUI calls it on its own `nmp-native-runtime` app before
//!   `start`.
//!
//! Composition lives here (not upstream in NMP) because NIP-29 is NOT part of
//! the substrate floor every Nostr app inherits — a notes-only app would not
//! register it. This is the per-app boundary D0 keeps outside `nmp-core`.
//!
//! ## Why not `nmp-defaults::register_defaults_with_handles`
//!
//! The old (pre-#2483) `nmp-app-29er::ffi::nmp_app_29er_register` called
//! `nmp_defaults::register_defaults_with_handles` for "the canonical NMP
//! default action set" (NIP-02/17/57/65, DM-inbox + zap-receipts runtimes).
//! `nmp-defaults` is deleted upstream (ADR-0069 kills hidden default bundles
//! in favor of explicit, named composition; the crate and every
//! `register_defaults*` entry point are banned doctrine-lint tokens as of the
//! pin in this workspace's `Cargo.toml`). That bundle also contradicted 29er's
//! own stated S01 scope — this crate's module docs have always said 29er
//! "carries no DM / Marmot / Wallet / Search / Embed projections in S01" —
//! so composing only the substrate floor + NIP-29 here is both the only
//! available option and the architecturally correct one: 29er now declares
//! exactly what it uses instead of inheriting an all-in bundle it never
//! wanted. A future PR can add explicit, named feature installs (e.g.
//! `nmp_nip17::register_actions` for DMs) the same way, if/when 29er's product
//! scope grows to need them.

use nmp_core::substrate::AppHost;

/// Compose the full 29er protocol surface onto a pre-start app host.
///
/// `app` is any [`AppHost`] — the live, unstarted `nmp_native_runtime::NmpApp`
/// owned by [`crate::TwentyNinerApp`], or the native Rust TUI's own app
/// instance. MUST be called before the runtime is started so the action
/// registry and the group-defaults snapshot are in place for the first tick.
///
/// Steps:
/// 1. The NMP substrate floor (`nmp_substrate::install`) — routing action,
///    shared profile/contacts/mailbox cache + parser wiring, blocked-relay
///    parser/actions, publish resolver, raw-event forwarding, D2 coverage
///    trimming, NIP-77 interceptors, and native NIP-11 relay metadata. Every
///    NMP app/runtime root needs this; it carries no product-specific
///    features (no DMs, no follows, no zaps — see the module doc above).
/// 2. The NIP-29 action namespaces (`nmp.nip29.discover` / `join` / etc.).
/// 3. The NIP-29 group-create defaults projection, pre-filled with 29er's
///    suggested public-group host relay ([`crate::config::public_group_relay_url`])
///    so the shell reads it from the projection instead of hardcoding it (D7).
pub fn compose_29er_runtime(app: &mut impl AppHost) {
    let _substrate_handles =
        nmp_substrate::install(app, nmp_substrate::SubstrateConfig::default());

    // 29er-specific: register the NIP-29 action namespaces against the action
    // registry. Lives in this crate (not NMP) because NIP-29 is not part of
    // the canonical substrate floor every Nostr app inherits.
    let registered = nmp_nip29::register::register_actions(app);
    debug_assert!(
        registered.is_ok(),
        "nmp-nip29 register_actions reported a namespace collision: {registered:?}"
    );
    let _ = registered;

    // Output-only projection (pure function of the supplied URL) — safe to
    // wire pre-start. `&*app` reborrows the `&mut impl AppHost` as the shared
    // `&impl SnapshotProjectionRegistrar` the wiring helper expects.
    nmp_nip29::register::wire_group_defaults_with_relay(
        &*app,
        crate::config::public_group_relay_url(),
    );
}
