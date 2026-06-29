//! `nmp-app-29er` — 29er per-app glue (clean-break architecture).
//!
//! 29er's S01 surface: NIP-29 group discovery + the canonical NMP default
//! action set. No DM / Marmot / Wallet / Search / Embed / home-feed projections.
//!
//! ## Shape
//!
//! `nmp-ffi` is deleted (#2483). 29er no longer ships a hand-written C-ABI;
//! instead it exposes:
//!
//! * [`TwentyNinerApp`] — a `uniffi::Object` that OWNS an
//!   `nmp-native-runtime` app, composes 29er in its constructor, and exposes
//!   29er's lifecycle + NIP-29 verbs to Swift/Kotlin. The iOS/Android shells
//!   consume generated bindings (see `src/bin/uniffi_bindgen.rs`).
//! * [`composition::compose_29er_runtime`] — the shared composition root, also
//!   called by the native Rust TUI on its `NmpAppBuilder`.
//! * Pure modules ([`compose`], [`config`], [`group_tree`], [`relay_selector`],
//!   [`relay_seeding`]) reused verbatim by both shells.
//!
//! ## Doctrine
//!
//! * **D0** — kernel emits, this crate composes. No business logic in the shell.
//! * **D6** — verbs degrade silently / fail-closed on malformed input; dispatch
//!   surfaces failures as a populated [`DispatchOutcome::error`].
//! * **D7** — seeding + relay policy live in Rust ([`config`] / [`relay_seeding`]).

uniffi::setup_scaffolding!();

pub mod app;
pub mod capability;
pub mod composition;
pub mod compose;
pub mod config;
pub mod content;
pub mod group_tree;
pub mod refs;
pub mod relay_seeding;
pub mod relay_selector;

pub use app::{dispatch_nip29_action, DispatchOutcome, TwentyNinerApp, UpdateSink};
pub use capability::CapabilitySink;
pub use composition::compose_29er_runtime;
pub use compose::{compose_chat_message, ComposedGroupMessage};
pub use content::tokenize_content;
