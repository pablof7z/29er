//! Pure content tokenizer — the Layer A `nmp-content` render-tree producer the
//! SwiftUI `NostrContentView` consumes.
//!
//! Ported verbatim from the deleted `nmp-ffi` `content_ffi.rs` C-ABI
//! (`nmp_content_tokenize_text`) to a UniFFI free function. Pure: resolves no
//! entities and mutates no kernel state (mentions/embeds resolve via the
//! separate `resolve_*_ref` seams in [`crate::refs`]). Returns the same
//! `{"ok":true,"tree":{…}}` / `{"ok":false,"error":"…"}` JSON the shell already
//! decodes, so the FlatBuffers-free `ContentTreeWire` shape is unchanged.

use nmp_content::{tokenize, tokenize_with_kind, RenderMode};
use serde::Serialize;

#[derive(Serialize)]
struct TokenizeSuccess {
    ok: bool,
    tree: nmp_content::ContentTreeWire,
}

#[derive(Serialize)]
struct TokenizeError {
    ok: bool,
    error: &'static str,
}

const MODE_PLAIN: i32 = 0;
const MODE_MARKDOWN: i32 = 1;
const MODE_AUTO: i32 = 2;

/// Tokenize Nostr event content into the FFI-stable `ContentTreeWire` JSON.
///
/// `mode`: `0` = plain · `1` = markdown · `2` = auto (markdown vs plain by
/// `kind`). `tags_json`, when present, is a JSON `[[string]]` event-tag array
/// used for NIP-30 emoji resolution.
///
/// D6: never fails the call. Invalid input returns
/// `{"ok":false,"error":"…"}`.
#[uniffi::export]
pub fn tokenize_content(content: String, tags_json: Option<String>, mode: i32, kind: u32) -> String {
    match tokenize_text_json(&content, tags_json.as_deref(), mode, kind) {
        Ok(json) => json,
        Err(error) => error_json(error),
    }
}

fn tokenize_text_json(
    content: &str,
    tags_json: Option<&str>,
    mode: i32,
    kind: u32,
) -> Result<String, &'static str> {
    let tags = decode_tags(tags_json)?;
    let mode = decode_mode(mode).ok_or("invalid-mode")?;
    let tree = if mode == RenderMode::Auto {
        tokenize_with_kind(content, &tags, mode, kind)
    } else {
        tokenize(content, &tags, mode)
    }
    .to_wire();
    serde_json::to_string(&TokenizeSuccess { ok: true, tree }).map_err(|_| "serialization-failed")
}

fn decode_tags(tags_json: Option<&str>) -> Result<Vec<Vec<String>>, &'static str> {
    let Some(raw) = tags_json else {
        return Ok(Vec::new());
    };
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(raw).map_err(|_| "invalid-tags")
}

fn decode_mode(mode: i32) -> Option<RenderMode> {
    match mode {
        MODE_PLAIN => Some(RenderMode::Plain),
        MODE_MARKDOWN => Some(RenderMode::Markdown),
        MODE_AUTO => Some(RenderMode::Auto),
        _ => None,
    }
}

fn error_json(error: &'static str) -> String {
    serde_json::to_string(&TokenizeError { ok: false, error })
        .unwrap_or_else(|_| r#"{"ok":false,"error":"serialization-failed"}"#.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_plain_ok() {
        let out = tokenize_content("hello world".to_string(), None, MODE_PLAIN, 9);
        assert!(out.contains(r#""ok":true"#), "got: {out}");
    }

    #[test]
    fn tokenize_auto_kind9_is_plain() {
        let out = tokenize_content("hi @nostr".to_string(), None, MODE_AUTO, 9);
        assert!(out.contains(r#""ok":true"#), "got: {out}");
    }

    #[test]
    fn tokenize_invalid_mode_errs() {
        let out = tokenize_content("x".to_string(), None, 99, 9);
        assert!(out.contains(r#""ok":false"#), "got: {out}");
        assert!(out.contains("invalid-mode"), "got: {out}");
    }

    #[test]
    fn tokenize_invalid_tags_errs() {
        let out = tokenize_content("x".to_string(), Some("{not json".to_string()), MODE_PLAIN, 1);
        assert!(out.contains("invalid-tags"), "got: {out}");
    }
}
