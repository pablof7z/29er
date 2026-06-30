//! `nmp-app-29er` — 29er per-app glue.
//!
//! Mirrors `nmp-app-chirp`'s minimal surface but strips to what 29er's S01
//! slice needs: NIP-29 group discovery + the canonical NMP default action set.
//! No DM / Marmot / Wallet / Search / Embed / home-feed projections — those
//! land in later milestones.
//!
//! ## Wiring
//!
//! The iOS shell links this one aggregate static library for 29er. Keeping
//! `nmp-ffi`, the NIP-46 broker adapter, and the 29er registration in one Rust
//! archive gives the process exactly one copy of the native C-ABI state.
//!
//! The shell calls [`nmp_app_29er_register`] once after [`nmp_ffi::nmp_app_new`]
//! and before [`nmp_ffi::nmp_app_start`]. The registration:
//!
//! 1. Calls [`nmp_defaults::register_defaults_with_handles`] for the canonical
//!    NMP composition (NIP-02 / NIP-17 / NIP-57 / NIP-65 action modules, the
//!    production routing substrate, the DM-inbox + zap-receipts runtimes, the
//!    D2 coverage hook).
//! 2. Calls [`nmp_nip29::register::register_actions`] for the NIP-29 action
//!    namespaces (`nmp.nip29.discover` / `nmp.nip29.join` / etc).
//! 3. Wires the NIP-29 group-create defaults projection so the suggested
//!    public-group relay URL surfaces under `"nmp.nip29.group_defaults"`.
//!
//! ## Doctrine
//!
//! * **D0** — kernel emits, this crate composes. No business logic in Swift.
//! * **D6** — runtime FFI symbols degrade silently on null pointers, lock
//!   poisoning, or serialization failure; init-only config symbols return
//!   explicit status codes for ordering errors.

pub mod compose;
pub mod config;
pub mod ffi;
pub mod group_tree;
pub mod relay_seeding;
pub mod relay_selector;

pub use compose::{compose_chat_message, ComposedGroupMessage};

pub use ffi::{
    nmp_app_29er_close_group_discovery, nmp_app_29er_declare_consumed_projections,
    nmp_app_29er_dispatch_action_bytes, nmp_app_29er_mark_group_read,
    nmp_app_29er_open_group_discovery, nmp_app_29er_refresh_group_discovery, nmp_app_29er_register,
    nmp_app_29er_register_group_chat, nmp_app_29er_relay_selector_add_relay,
    nmp_app_29er_relay_selector_remove_relay, nmp_app_29er_relay_selector_select_relay,
    nmp_app_29er_unregister, NmpRegisterStatus, TwentyNinerHandle,
};

// Relay-seeding C-ABI surface (D7 — seeding policy lives in Rust, not the
// shell). Mirrors Chirp's `nmp_app_chirp_seed_default_relays` /
// `nmp_app_chirp_seed_relays_from_json`.
pub use relay_seeding::{nmp_app_29er_seed_default_relays, nmp_app_29er_seed_relays_from_json};
// Re-export `nmp_free_string` so the 29er shell links it through this archive
// the same way Chirp links it through `libnmp_app_chirp.a`.
pub use nmp_ffi::nmp_free_string;
