//! NMP Nostr content renderer (registry component `tui/content-view` and its
//! dependencies). The `ContentTreeWire` mirror, render-data, ratatui wrap
//! helpers, mention chip, media grid, kind-dispatch registry, and the full
//! `NostrContentView` widget. App-owned: `nmp update component tui/content-view`
//! refreshes these; local edits are reported as conflicts.
pub mod content_kind_registry;
pub mod content_render_data;
pub mod content_tree_wire;
pub mod nostr_media_grid;
pub mod nostr_mention_chip;
pub mod ratatui_text_wrap;

// `nostr_content_view.rs` declares a private child `mod nostr_content_widget;`.
// The registry ships that widget as a *sibling* file (flat layout). Declaring
// the view module with an explicit `#[path]` switches child-module discovery to
// the path file's own directory, so `nostr_content_widget` resolves to the
// sibling `nostr_content_widget.rs` instead of a `nostr_content_view/` subdir —
// this is the same mechanism the upstream reference app uses, and keeps the
// vendored sources byte-identical so `nmp update component` stays clean.
#[path = "nostr_content_view.rs"]
pub mod nostr_content_view;

use content_tree_wire::ContentTreeWire;

/// Decode canonical NFCT `ContentTreeWire` bytes produced by Rust projections
/// into the TUI renderer's local mirror.
#[must_use]
pub fn content_tree_from_nfct_bytes(bytes: &[u8]) -> Option<ContentTreeWire> {
    let wire = nmp_content::wire::decode_content_tree(bytes).ok()?;
    let value = serde_json::to_value(&wire).ok()?;
    ContentTreeWire::from_value(&value)
}
