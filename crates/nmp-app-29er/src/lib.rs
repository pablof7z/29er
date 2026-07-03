//! `nmp-app-29er` — 29er per-app glue.
//!
//! 29er's S01 surface: NIP-29 group discovery + chat. No DM / Marmot / Wallet
//! / Search / Embed projections.
//!
//! ## Shape
//!
//! UniFFI is NMP's native FFI surface. This crate exposes:
//!
//! * [`TwentyNinerApp`] — a `uniffi::Object` that OWNS an
//!   `nmp-native-runtime` app, composes 29er in its constructor, and exposes
//!   29er's lifecycle, relay selector, refs, reactions,
//!   and NIP-29 group-read sessions through UniFFI. The iOS shell consumes
//!   generated Swift bindings (see `src/bin/uniffi_bindgen.rs`).
//! * [`group_sessions`] — the NIP-29 group-discovery / group-chat /
//!   group-roster typed read sessions and `dispatchNip29Action`, exported as
//!   a second `impl TwentyNinerApp` block.
//! * [`composition::compose_29er_runtime`] — the shared composition root,
//!   also called directly by the native Rust TUI on its own app instance.
//! * Pure modules ([`compose`], [`config`], [`group_chat`], [`group_tree`],
//!   [`relay_seeding`], [`dispatch`]) reused by both the facade and the TUI.
//!
//! ## Doctrine
//!
//! * **D0** — kernel emits, this crate composes. No business logic in the shell.
//! * **D6** — verbs degrade silently / fail-closed on malformed input; dispatch
//!   surfaces failures as a populated [`DispatchOutcome::error`] equivalent
//!   ([`app::DispatchOutcome`]).
//! * **D7** — seeding + relay policy live in Rust ([`config`] / [`relay_seeding`]).

uniffi::setup_scaffolding!();

pub mod app;
pub mod capability;
pub mod compose;
pub mod composition;
pub mod config;
pub mod dispatch;
pub mod group_chat;
mod group_sessions;
pub mod group_tree;
pub mod kinds;
pub mod media_upload;
pub mod reactions;
pub mod refs;
pub mod relay_seeding;
pub mod relay_selector;

pub use app::{DispatchOutcome, TwentyNinerApp, UpdateSink};
pub use capability::CapabilitySink;
pub use compose::{compose_chat_message, ComposedGroupMessage};
pub use composition::compose_29er_runtime;
pub use dispatch::dispatch_nip29_action;
