//! Content composition for NIP-29 group messages — the **single source of
//! truth** for turning raw user input + `@mentions` into the `(content, tags)`
//! an event carries.
//!
//! ## Doctrine
//!
//! `nmp-nip29` owns ONLY the group *envelope* (the `["h", local_id]` tag, the
//! `["previous", …]` timeline references, host-relay routing). It is
//! kind-agnostic: "chat" is just `kind:9`, one event kind among many. The
//! *content* and the kind-specific *tags* are the **app's** concern.
//!
//! So this module is where 29er composes a chat message. Both shells reuse it:
//! the TUI routes raw input through the `nmp-app-29er` dispatch (which calls
//! [`compose_chat_message`]); the iOS shell hits the same dispatch through the
//! generated facade. Neither shell contains any NIP-21 / nostr knowledge — it
//! all lives here.
//!
//! ## Mention contract
//!
//! The shell collects raw text and the list of pubkeys the user `@mentioned`.
//! It inserts each mention into the text as an `@<identifier>` placeholder
//! where `<identifier>` is the **raw pubkey** (hex or `npub1…`) — NOT a display
//! name. [`compose_chat_message`] then, for every mention pubkey:
//!
//! 1. replaces the `@<identifier>` token in the content with the NIP-21
//!    `nostr:npub1…` URI for that pubkey, and
//! 2. emits a deduplicated `["p", <hex>]` tag.
//!
//! A mention that does not appear as a token in the text contributes only its
//! `["p", …]` tag — the URI is never appended to the end of the message.

use std::collections::BTreeSet;

use nmp_nostr_id::{decode_npub, encode_npub};

/// The composed body of a NIP-29 group event: the rewritten `content` (with
/// `@mention` placeholders turned into `nostr:npub1…` URIs) and the
/// kind-specific `tags` (here, the `["p", <hex>]` mention tags).
///
/// These map straight onto `PublishGroupEventInput { content, tags, .. }`; the
/// `["h", …]` / `["previous", …]` envelope tags are injected by `nmp-nip29` and
/// are deliberately absent here.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComposedGroupMessage {
    pub content: String,
    pub tags: Vec<Vec<String>>,
}

/// Compose a NIP-29 chat message body + tags from raw user text and the
/// pubkeys the user `@mentioned`.
///
/// See the [module docs](self) for the mention contract. Best-effort (D6): a
/// mention pubkey that is neither valid hex nor a valid `npub1…` is skipped
/// (no token rewrite, no `p` tag) rather than aborting the whole message.
#[must_use]
pub fn compose_chat_message(raw_text: &str, mention_pubkeys: &[String]) -> ComposedGroupMessage {
    let mut content = raw_text.to_string();
    let mut tags: Vec<Vec<String>> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();

    for mention in mention_pubkeys {
        let mention = mention.trim();
        if mention.is_empty() {
            continue;
        }

        // Normalise the mention to lowercase 64-char hex.
        let hex = if mention.starts_with("npub1") {
            match decode_npub(mention) {
                Ok(h) => h,
                Err(_) => continue,
            }
        } else {
            mention.to_lowercase()
        };

        // The canonical NIP-21 URI uses the bech32 `npub`. Skip on encode
        // failure (malformed hex) -- fail-closed.
        let Ok(npub) = encode_npub(&hex) else {
            continue;
        };
        let uri = format!("nostr:{npub}");

        // Rewrite the `@<identifier>` placeholder the shell inserted. Accept the
        // verbatim form the caller passed plus both canonical forms (hex / npub)
        // so the helper is robust to either insertion style.
        content = content.replace(&format!("@{mention}"), &uri);
        if mention != hex {
            content = content.replace(&format!("@{hex}"), &uri);
        }
        if mention != npub {
            content = content.replace(&format!("@{npub}"), &uri);
        }

        // One `["p", <hex>]` tag per distinct mentioned pubkey.
        if seen.insert(hex.clone()) {
            tags.push(vec!["p".to_string(), hex]);
        }
    }

    ComposedGroupMessage { content, tags }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Canonical NIP-19 test vector (valid secp256k1 x-only pubkey).
    const HEX: &str = "3bf0c63fcb93463407af97a5e5ee64fa883d107ef9e558472c4eb9aaaefa459d";

    #[test]
    fn rewrites_hex_mention_to_nip21_uri_and_adds_p_tag() {
        let npub = encode_npub(HEX).expect("valid hex");
        let composed = compose_chat_message(&format!("hey @{HEX} how are you"), &[HEX.to_string()]);
        assert_eq!(composed.content, format!("hey nostr:{npub} how are you"));
        assert_eq!(composed.tags, vec![vec!["p".to_string(), HEX.to_string()]]);
    }

    #[test]
    fn accepts_npub_mention_and_normalises_p_tag_to_hex() {
        let npub = encode_npub(HEX).expect("valid hex");
        // The shell passed the npub form both in the text token and the list.
        let composed = compose_chat_message(&format!("yo @{npub}!"), &[npub.clone()]);
        assert_eq!(composed.content, format!("yo nostr:{npub}!"));
        // The p tag is always normalised to lowercase hex.
        assert_eq!(composed.tags, vec![vec!["p".to_string(), HEX.to_string()]]);
    }

    #[test]
    fn dedups_repeated_mentions_into_one_p_tag() {
        let npub = encode_npub(HEX).expect("valid hex");
        let composed = compose_chat_message(
            &format!("@{HEX} and again @{HEX}"),
            &[HEX.to_string(), HEX.to_string()],
        );
        assert_eq!(
            composed.content,
            format!("nostr:{npub} and again nostr:{npub}")
        );
        assert_eq!(composed.tags, vec![vec!["p".to_string(), HEX.to_string()]]);
    }

    #[test]
    fn uppercase_hex_mention_is_normalised_before_rewrite() {
        let npub = encode_npub(HEX).expect("valid hex");
        let upper = HEX.to_uppercase();
        // The placeholder uses the uppercase form the caller passed…
        let composed = compose_chat_message(&format!("hi @{upper}"), &[upper.clone()]);
        // …and is rewritten to the canonical URI; the p tag is lowercase hex.
        assert_eq!(composed.content, format!("hi nostr:{npub}"));
        assert_eq!(composed.tags, vec![vec!["p".to_string(), HEX.to_string()]]);
    }

    #[test]
    fn mention_without_token_contributes_only_p_tag() {
        // No `@…` token in the text — the URI is NOT appended; only the tag.
        let composed = compose_chat_message("plain message", &[HEX.to_string()]);
        assert_eq!(composed.content, "plain message");
        assert_eq!(composed.tags, vec![vec!["p".to_string(), HEX.to_string()]]);
    }

    #[test]
    fn no_mentions_passes_text_through_untouched() {
        let composed = compose_chat_message("just text", &[]);
        assert_eq!(composed.content, "just text");
        assert!(composed.tags.is_empty());
    }

    #[test]
    fn invalid_mention_is_skipped_without_panicking() {
        let composed = compose_chat_message("hello @not-a-key", &["not-a-key".to_string()]);
        // Malformed pubkey: no rewrite, no tag — fail-closed.
        assert_eq!(composed.content, "hello @not-a-key");
        assert!(composed.tags.is_empty());
    }
}
