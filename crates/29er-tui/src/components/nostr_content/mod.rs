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

/// Tokenize raw Nostr event content into the renderable [`ContentTreeWire`]
/// using the shared `nmp-content` substrate. `RenderMode::Auto` dispatches
/// markdown vs. plain by `kind` (kind 9/11 chat → plain). This is the single
/// place the shell turns wire content into a render tree — no shell-side
/// parsing. Returns `None` when projection fails; callers fall back to raw text.
#[must_use]
pub fn tokenize_message(content: &str, tags: &[Vec<String>], kind: u32) -> Option<ContentTreeWire> {
    let wire =
        nmp_content::tokenize_with_kind(content, tags, nmp_content::RenderMode::Auto, kind).to_wire();
    let value = serde_json::to_value(&wire).ok()?;
    ContentTreeWire::from_value(&value)
}
