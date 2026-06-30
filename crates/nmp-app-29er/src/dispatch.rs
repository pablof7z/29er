//! NIP-29 typed dispatch-payload encoding — ported from the deleted C-ABI
//! `ffi.rs` (`encode_payload_for_namespace` / `encode_chat_send_payload` /
//! `nmp_app_29er_dispatch_action_bytes`).
//!
//! [`dispatch_nip29_action`] is a plain Rust function, not yet exported on
//! [`crate::TwentyNinerApp`] via `#[uniffi::export]` — wiring it onto the
//! facade as a Swift-callable `dispatchNip29Action` verb is PR-3's job (see
//! the crate module docs and `docs/builder-guide/15-codegen-and-ffi.md`
//! "App-owned UniFFI facades (#2494)" worked example). It is ported now,
//! rather than deleted, because the native Rust TUI (`29er-tui`) dispatches
//! every NIP-29 action — join/leave/create-group/chat-send/etc. — through
//! this exact encoding, and leaving it out would regress a working feature
//! for no PR-1 benefit.

use std::sync::atomic::{AtomicU64, Ordering};

use nmp_core::dispatch_envelope::{encode_dispatch_envelope, DISPATCH_ENVELOPE_SCHEMA_VERSION};
use nmp_core::substrate::ActionPayload;
use nmp_native_runtime::NmpApp;
use nmp_nip29::group_id::GroupId;

use crate::app::DispatchOutcome;
use crate::kinds::KIND_CHAT_MESSAGE;

/// 29er's app-level chat-send doorway namespace. NOT a NIP-29 action
/// namespace — it takes raw `{group, content, mention_pubkeys}`, is composed
/// by [`encode_chat_send_payload`], and re-emitted under
/// [`PUBLISH_GROUP_EVENT_NAMESPACE`].
const CHAT_SEND_NAMESPACE: &str = "nmp.nip29.post_chat_message";
/// The real NIP-29 generic-publish action namespace the composed chat send is
/// dispatched under.
const PUBLISH_GROUP_EVENT_NAMESPACE: &str = "nmp.nip29.publish_group_event";

/// Build the typed payload + open `DispatchEnvelope` for a NIP-29 action and
/// dispatch it through the byte lane, returning the typed [`DispatchOutcome`].
///
/// Used by the native Rust TUI. The `nmp.nip29.post_chat_message` doorway
/// runs the shared composer ([`crate::compose::compose_chat_message`]) and
/// re-emits under `nmp.nip29.publish_group_event`. D6 fail-closed: an unknown
/// namespace or a body that does not deserialize into the namespace's typed
/// action returns a populated `DispatchOutcome.error` — never a panic.
pub fn dispatch_nip29_action(app: &NmpApp, namespace: &str, body_json: &str) -> DispatchOutcome {
    let (dispatch_ns, payload): (String, Vec<u8>) = if namespace == CHAT_SEND_NAMESPACE {
        match encode_chat_send_payload(body_json) {
            Some(p) => (PUBLISH_GROUP_EVENT_NAMESPACE.to_string(), p),
            None => {
                return DispatchOutcome::error("could not compose chat message from body");
            }
        }
    } else {
        match encode_payload_for_namespace(namespace, body_json) {
            Some(p) => (namespace.to_string(), p),
            None => {
                return DispatchOutcome::error(format!(
                    "no typed payload encoder for action namespace '{namespace}'"
                ));
            }
        }
    };
    let correlation_id = mint_correlation_id();
    let envelope = encode_dispatch_envelope(
        &correlation_id,
        &dispatch_ns,
        DISPATCH_ENVELOPE_SCHEMA_VERSION,
        &payload,
    );
    nmp_uniffi_support::dispatch_action(app, &envelope).into()
}

/// Process-local correlation-id source for 29er's byte-doorway dispatches.
///
/// A monotone atomic counter satisfies the "unique within one running process
/// for the lifetime of an in-flight operation" contract with zero extra deps.
/// The `29er-` prefix namespaces it so it never collides with the kernel's hex
/// correlation ids.
static NEXT_CORRELATION_ID: AtomicU64 = AtomicU64::new(1);

fn mint_correlation_id() -> String {
    let n = NEXT_CORRELATION_ID.fetch_add(1, Ordering::Relaxed);
    format!("29er-{n}")
}

/// Raw chat-send body the shell hands to [`CHAT_SEND_NAMESPACE`]: the target
/// group, the user's verbatim text (carrying `@<pubkey>` placeholders), and
/// the pubkeys they `@mentioned`. The app composes; the shell holds no
/// NIP-21 / tag knowledge.
#[derive(serde::Deserialize)]
struct ChatSendBody {
    group: GroupId,
    #[serde(default)]
    content: String,
    #[serde(default)]
    mention_pubkeys: Vec<String>,
}

/// Compose a raw chat-send body into the typed [`ActionPayload`] bytes for a
/// kind:9 `PublishGroupEventInput`. NIP-21 mention rewriting + `["p", …]` tags
/// are produced by the shared [`crate::compose::compose_chat_message`] (the
/// one place that composition lives). Returns `None` on a malformed body (D6).
fn encode_chat_send_payload(json: &str) -> Option<Vec<u8>> {
    let body: ChatSendBody = serde_json::from_str(json).ok()?;
    let composed = crate::compose::compose_chat_message(&body.content, &body.mention_pubkeys);
    let input = nmp_nip29::action::PublishGroupEventInput {
        group: body.group,
        kind: KIND_CHAT_MESSAGE,
        content: composed.content,
        tags: composed.tags,
    };
    Some(input.encode())
}

/// Encode `json` into the typed [`ActionPayload`] FlatBuffers bytes for
/// `namespace`. Returns `None` for an unknown namespace (D6 fail-closed — no
/// JSON fallback; the byte doorway has no JSON path).
fn encode_payload_for_namespace(namespace: &str, json: &str) -> Option<Vec<u8>> {
    use serde::de::DeserializeOwned;
    fn encode<P>(json: &str) -> Option<Vec<u8>>
    where
        P: ActionPayload + DeserializeOwned,
    {
        let action: P = serde_json::from_str(json).ok()?;
        Some(action.encode())
    }
    match namespace {
        "nmp.publish" => encode::<nmp_core::publish::PublishAction>(json),
        "nmp.nip29.discover" => encode::<nmp_nip29::action::DiscoverGroupsInput>(json),
        "nmp.nip29.join" => encode::<nmp_nip29::action::JoinGroupInput>(json),
        "nmp.nip29.leave" => encode::<nmp_nip29::action::LeaveGroupInput>(json),
        "nmp.nip29.create_public_group" => {
            encode::<nmp_nip29::action::CreatePublicGroupInput>(json)
        }
        "nmp.nip29.put_user" => encode::<nmp_nip29::action::PutUserInput>(json),
        "nmp.nip29.create_invite" => encode::<nmp_nip29::action::CreateInviteInput>(json),
        "nmp.nip29.set_parent" => encode::<nmp_nip29::action::SetParentInput>(json),
        "nmp.nip29.publish_group_event" => {
            encode::<nmp_nip29::action::PublishGroupEventInput>(json)
        }
        // "nmp.nip29.react_in_group" is gone — NIP-29 is kind-blind transport
        // (no react-in-group / reaction-aggregate verb upstream). Reactions
        // are NIP-25 (kind:7) built by the caller and handed to NIP-29's
        // generic `publish_group_event` doorway like any other group event.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn m002_membership_and_admin_namespaces_encode_typed_payloads() {
        let leave = encode_payload_for_namespace(
            "nmp.nip29.leave",
            r#"{"group":{"host_relay_url":"wss://groups.example.com","local_id":"room"},"reason":"done"}"#,
        )
        .expect("leave payload encodes");
        let leave =
            <nmp_nip29::action::LeaveGroupInput as ActionPayload>::decode(&leave).expect("decodes");
        assert_eq!(leave.group.local_id, "room");
        assert_eq!(leave.reason.as_deref(), Some("done"));

        let create = encode_payload_for_namespace(
            "nmp.nip29.create_public_group",
            r#"{"group":{"host_relay_url":"wss://groups.example.com","local_id":"child-room"},"name":"Child Room","about":"Admin work","visibility":"private","access":"closed","parent":"root"}"#,
        )
        .expect("create payload encodes");
        let create = <nmp_nip29::action::CreatePublicGroupInput as ActionPayload>::decode(&create)
            .expect("decodes");
        assert_eq!(create.group.local_id, "child-room");
        assert_eq!(create.name, "Child Room");
        assert_eq!(create.parent.as_deref(), Some("root"));
        assert_eq!(create.about.as_deref(), Some("Admin work"));

        let put_user = encode_payload_for_namespace(
            "nmp.nip29.put_user",
            r#"{"group":{"host_relay_url":"wss://groups.example.com","local_id":"room"},"target_pubkey":"0000000000000000000000000000000000000000000000000000000000000001","role":"admin","reason":"trusted"}"#,
        )
        .expect("put-user payload encodes");
        let put_user =
            <nmp_nip29::action::PutUserInput as ActionPayload>::decode(&put_user).expect("decodes");
        assert_eq!(
            put_user.target_pubkey,
            "0000000000000000000000000000000000000000000000000000000000000001"
        );
        assert_eq!(put_user.role.as_deref(), Some("admin"));

        let invite = encode_payload_for_namespace(
            "nmp.nip29.create_invite",
            r#"{"group":{"host_relay_url":"wss://groups.example.com","local_id":"room"},"codes":["alpha","beta"]}"#,
        )
        .expect("invite payload encodes");
        let invite = <nmp_nip29::action::CreateInviteInput as ActionPayload>::decode(&invite)
            .expect("decodes");
        assert_eq!(invite.codes, vec!["alpha".to_string(), "beta".to_string()]);

        let set_parent = encode_payload_for_namespace(
            "nmp.nip29.set_parent",
            r#"{"group":{"host_relay_url":"wss://groups.example.com","local_id":"child-room"},"parent":"root"}"#,
        )
        .expect("set-parent payload encodes");
        let set_parent = <nmp_nip29::action::SetParentInput as ActionPayload>::decode(&set_parent)
            .expect("decodes");
        assert_eq!(set_parent.group.local_id, "child-room");
        assert_eq!(set_parent.parent.as_deref(), Some("root"));
    }

    #[test]
    fn chat_send_doorway_composes_kind9_publish_group_event() {
        const HEX: &str = "3bf0c63fcb93463407af97a5e5ee64fa883d107ef9e558472c4eb9aaaefa459d";
        let npub = nmp_nostr_id::encode_npub(HEX).expect("valid hex");
        let bytes = encode_chat_send_payload(&format!(
            r#"{{"group":{{"host_relay_url":"wss://groups.example.com","local_id":"room"}},"content":"hi @{HEX}","mention_pubkeys":["{HEX}"]}}"#
        ))
        .expect("chat-send composes");
        let input = <nmp_nip29::action::PublishGroupEventInput as ActionPayload>::decode(&bytes)
            .expect("decodes");
        assert_eq!(input.group.local_id, "room");
        assert_eq!(input.kind, KIND_CHAT_MESSAGE);
        assert_eq!(input.content, format!("hi nostr:{npub}"));
        assert_eq!(input.tags, vec![vec!["p".to_string(), HEX.to_string()]]);
        // The envelope tags (`h` / `previous`) must NOT be present — nmp-nip29
        // injects them and rejects caller-supplied copies.
        assert!(input.tags.iter().all(|t| t.first().map(String::as_str) != Some("h")));
        assert!(input
            .tags
            .iter()
            .all(|t| t.first().map(String::as_str) != Some("previous")));
    }

    #[test]
    fn chat_send_doorway_fails_closed_on_malformed_body() {
        assert!(encode_chat_send_payload(r#"{"content":"no group"}"#).is_none());
    }

    #[test]
    fn action_encoder_fails_closed_for_unknown_or_malformed_payloads() {
        assert!(encode_payload_for_namespace("nmp.nip29.remove_everyone", "{}").is_none());
        assert!(encode_payload_for_namespace("nmp.nip29.leave", r#"{"group":42}"#).is_none());
    }
}
