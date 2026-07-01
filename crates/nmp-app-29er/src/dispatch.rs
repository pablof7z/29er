//! NIP-29 typed dispatch-payload encoding.
//!
//! [`dispatch_nip29_action`] is a plain Rust function so the native Rust TUI
//! (`29er-tui`) — which dispatches every NIP-29 action (join/leave/
//! create-group/chat-send/etc.) through this exact encoding — can call it
//! directly without going through UniFFI. [`crate::group_sessions`] wraps it
//! as the Swift-callable `dispatchNip29Action` facade verb.

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
/// 29er's app-level child-channel doorway. The shell supplies only the parent
/// group and display fields; this Rust app crate owns the child local-id policy
/// and re-emits the typed NIP-29 create-public-group action.
const CREATE_CHILD_GROUP_NAMESPACE: &str = "nmp.29er.create_child_group";
/// The real NIP-29 generic-publish action namespace the composed chat send is
/// dispatched under.
const PUBLISH_GROUP_EVENT_NAMESPACE: &str = "nmp.nip29.publish_group_event";
/// The real NIP-29 create action namespace used by the app-owned child doorway.
const CREATE_PUBLIC_GROUP_NAMESPACE: &str = "nmp.nip29.create_public_group";

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
    } else if namespace == CREATE_CHILD_GROUP_NAMESPACE {
        match encode_child_group_payload(body_json) {
            Some(p) => (CREATE_PUBLIC_GROUP_NAMESPACE.to_string(), p),
            None => {
                return DispatchOutcome::error("could not compose child channel create action");
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

pub(crate) fn mint_correlation_id() -> String {
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

#[derive(serde::Deserialize)]
struct CreateChildGroupBody {
    parent: GroupId,
    #[serde(default)]
    name: String,
    #[serde(default)]
    about: Option<String>,
    #[serde(default)]
    picture: Option<String>,
    #[serde(default)]
    visibility: Option<nmp_nip29::action::GroupVisibility>,
    #[serde(default)]
    access: Option<nmp_nip29::action::GroupAccess>,
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

fn encode_child_group_payload(json: &str) -> Option<Vec<u8>> {
    let body: CreateChildGroupBody = serde_json::from_str(json).ok()?;
    let name = body.name.trim().to_string();
    if name.is_empty() {
        return None;
    }
    let input = nmp_nip29::action::CreatePublicGroupInput {
        group: GroupId::new(body.parent.host_relay_url.clone(), mint_group_local_id()),
        name,
        about: body.about.filter(|value| !value.trim().is_empty()),
        picture: body.picture.filter(|value| !value.trim().is_empty()),
        visibility: body.visibility.unwrap_or_default(),
        access: body.access.unwrap_or_default(),
        parent: Some(body.parent.local_id),
    };
    Some(input.encode())
}

fn mint_group_local_id() -> String {
    format!("ch-{}", uuid::Uuid::new_v4().simple())
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
        assert!(input
            .tags
            .iter()
            .all(|t| t.first().map(String::as_str) != Some("h")));
        assert!(input
            .tags
            .iter()
            .all(|t| t.first().map(String::as_str) != Some("previous")));
    }

    #[test]
    fn child_group_doorway_mints_local_id_in_rust() {
        let bytes = encode_child_group_payload(
            r#"{"parent":{"host_relay_url":"wss://groups.example.com","local_id":"root"},"name":"Child Room"}"#,
        )
        .expect("child group composes");
        let input = <nmp_nip29::action::CreatePublicGroupInput as ActionPayload>::decode(&bytes)
            .expect("decodes");

        assert_eq!(input.group.host_relay_url, "wss://groups.example.com");
        assert!(input.group.local_id.starts_with("ch-"));
        assert_ne!(input.group.local_id, "root");
        assert!(input
            .group
            .local_id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'));
        assert_eq!(input.name, "Child Room");
        assert_eq!(input.parent.as_deref(), Some("root"));
    }

    #[test]
    fn child_group_doorway_fails_closed_without_name_or_parent() {
        assert!(encode_child_group_payload(r#"{"name":"Child"}"#).is_none());
        assert!(encode_child_group_payload(
            r#"{"parent":{"host_relay_url":"wss://groups.example.com","local_id":"root"},"name":"   "}"#
        )
        .is_none());
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
