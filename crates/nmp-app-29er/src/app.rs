//! `TwentyNinerApp` — the 29er UniFFI object.
//!
//! Clean-break replacement for the deleted hand-written C-ABI (`nmp-ffi` +
//! this crate's old `#[no_mangle] extern "C"` surface). 29er owns its own
//! `nmp-native-runtime` app (composed in [`TwentyNinerApp::new`] via
//! [`crate::composition::compose_29er_runtime`]) and exposes its lifecycle +
//! NIP-29 verbs through UniFFI, so the iOS/Android shells consume generated
//! Swift/Kotlin instead of a hand-maintained header.
//!
//! Why a 29er-owned object (and not stock `nmp-uniffi::NmpApp`): NIP-29 is not
//! part of the canonical composition `nmp-uniffi` exposes, and a `uniffi::Object`
//! cannot be extended across crates. 29er therefore owns the composition root
//! and the runtime, mirroring `nmp-uniffi`'s lifecycle/dispatch surface and
//! adding the group-discovery / group-chat / discover-join verbs on top.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use nmp_core::dispatch_envelope::{encode_dispatch_envelope, DISPATCH_ENVELOPE_SCHEMA_VERSION};
use nmp_core::substrate::{ActionPayload, ObservedProjection, ObservedProjectionRegistrar};
use nmp_core::{ObservedProjectionId, ObservedProjectionSink, TypedProjectionData};
use nmp_native_runtime::{
    dispatch_action_bytes_typed, new_app, Nip29GroupDiscoveryHandle, Nip29GroupDiscoverySession,
    Nip29GroupEventsSession, Nip29JoinedGroupsHandle, Nip29JoinedGroupsSession, NmpApp,
    UpdateListener, DEFAULT_EMIT_HZ, DEFAULT_VISIBLE_LIMIT,
};
use nmp_nip29::group_id::GroupId;
use nmp_nip29::kinds::{KIND_CHAT_MESSAGE, KIND_DISCUSSION_OR_ARTIFACT};

use crate::group_tree::{
    encode_group_tree_snapshot, membership_from_joined, GroupMembershipMap, GroupTreeProjection,
    GROUP_TREE_FILE_IDENTIFIER, GROUP_TREE_SCHEMA_ID, GROUP_TREE_SCHEMA_VERSION,
};
use crate::relay_selector::{register_relay_selector_runtime, RelaySelectorProjection};

/// The 29er app-owned composite group-tree snapshot key.
const GROUP_TREE_KEY: &str = "nmp.29er.group_tree";

/// Bounded read-cache replay budget for the 29er kind:9 group-tree observed
/// projection (mirrors `nmp_feed::DEFAULT_FEED_WINDOW_LIMIT` = 80).
const GROUP_TREE_REPLAY_LIMIT: usize = 80;

/// 29er's app-level chat-send doorway namespace. NOT a NIP-29 action namespace
/// — it takes raw `{group, content, mention_pubkeys}`, is composed by
/// [`encode_chat_send_payload`], and re-emitted under
/// [`PUBLISH_GROUP_EVENT_NAMESPACE`].
const CHAT_SEND_NAMESPACE: &str = "nmp.nip29.post_chat_message";
/// The real NIP-29 generic-publish action namespace the composed chat send is
/// dispatched under.
const PUBLISH_GROUP_EVENT_NAMESPACE: &str = "nmp.nip29.publish_group_event";

// ── Typed dispatch outcome ───────────────────────────────────────────────────

/// Typed outcome of a dispatch. Exactly one of `correlation_id` (accepted) or
/// `error` (rejected/failed) is `Some`; `code` is present only for coded
/// rejections (issue #1734). Mirrors `nmp-uniffi::DispatchOutcome`.
#[derive(uniffi::Record, Debug, Clone)]
pub struct DispatchOutcome {
    pub correlation_id: Option<String>,
    pub error: Option<String>,
    pub code: Option<String>,
}

impl DispatchOutcome {
    fn error(msg: impl Into<String>) -> Self {
        DispatchOutcome {
            correlation_id: None,
            error: Some(msg.into()),
            code: None,
        }
    }
}

// ── Update sink callback interface ───────────────────────────────────────────

/// Rust→shell push interface: receives NMPU FlatBuffers update frames.
///
/// Implementations MUST NOT call back into any [`TwentyNinerApp`] method from
/// within `on_update` (the quiescence gate would deadlock). Mirrors
/// `nmp-uniffi::UpdateSink`.
#[uniffi::export(callback_interface)]
pub trait UpdateSink: Send + Sync {
    fn on_update(&self, frame: Vec<u8>);
}

// ── App-owned group-discovery session (replaces the opaque C handle) ─────────

/// The runtime state of one open group-discovery screen: the canonical NMP
/// discovery + joined-groups read sessions, plus 29er's own kind:9 group-tree
/// observed projection and the `nmp.29er.group_tree` composite snapshot.
struct GroupDiscoverySession {
    tree_observer_id: ObservedProjectionId,
    tree_projection: Arc<GroupTreeProjection>,
    discovery_handle: Nip29GroupDiscoveryHandle,
    joined_handle: Option<Nip29JoinedGroupsHandle>,
}

// ── The object ───────────────────────────────────────────────────────────────

/// Arc-wrapped 29er native runtime + NIP-29 verbs.
#[derive(uniffi::Object)]
pub struct TwentyNinerApp {
    inner: NmpApp,
    /// The NIP-51 relay selector projection (wired pre-start). Lives behind a
    /// lock for the relay-selector verbs.
    relay_selector: Mutex<Option<Arc<RelaySelectorProjection>>>,
    /// The single open group-discovery session (the discover screen is a
    /// singleton). `None` when no discover screen is open.
    discovery: Mutex<Option<GroupDiscoverySession>>,
}

#[uniffi::export]
impl TwentyNinerApp {
    /// Construct + compose 29er. No IO; the actor is NOT started. Call
    /// configuration setters then [`Self::start`].
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        let mut inner = new_app();
        crate::composition::compose_29er_runtime(&mut inner);
        // Pre-start: wire the NIP-51 relay selector (opens an account-scoped
        // observed projection + its typed snapshot). Same phase as the old
        // `nmp_app_29er_register`.
        let relay_selector = register_relay_selector_runtime(&inner);
        Arc::new(Self {
            inner,
            relay_selector: Mutex::new(Some(relay_selector)),
            discovery: Mutex::new(None),
        })
    }

    /// Set the LMDB storage directory (pre-start). Empty clears it. Returns
    /// `true` when accepted (`NmpConfigStatus::Ok`).
    pub fn set_storage_path(&self, path: String) -> bool {
        let arg = if path.is_empty() { None } else { Some(path) };
        matches!(
            self.inner.set_storage_path(arg),
            nmp_native_runtime::NmpConfigStatus::Ok
        )
    }

    /// Declare that 29er consumes every kernel-owned built-in Tier-2 projection
    /// (full client). Pre-start; idempotent.
    pub fn declare_consumed_projections(&self) {
        self.inner.consume_all_builtin_projections();
    }

    /// Start the runtime actor. Clamp parity with `nmp-uniffi`: `visible_limit
    /// == 0` → default; else clamp(1..=500). `emit_hz == 0` → default; else
    /// clamp(1..=12).
    pub fn start(&self, visible_limit: u32, emit_hz: u32) {
        self.inner
            .start_runtime(clamp_visible(visible_limit), clamp_emit_hz(emit_hz));
    }

    /// Reconfigure rendering limits without restarting (same clamps as `start`).
    pub fn configure(&self, visible_limit: u32, emit_hz: u32) {
        self.inner
            .configure_runtime(clamp_visible(visible_limit), clamp_emit_hz(emit_hz));
    }

    /// Pause event processing (no data loss).
    pub fn stop(&self) {
        self.inner.stop_runtime();
    }

    /// Reset transient kernel state.
    pub fn reset(&self) {
        self.inner.reset_runtime();
    }

    /// Idempotent teardown: clears the sink, sends Shutdown, joins threads.
    pub fn shutdown(&self) {
        self.inner.shutdown();
    }

    /// Actor-liveness probe (ADR-0028). `true` while the actor thread runs.
    pub fn is_alive(&self) -> bool {
        self.inner.is_alive()
    }

    /// Report iOS scenePhase = `.active`. Fire-and-forget.
    pub fn lifecycle_foreground(&self) {
        self.inner.lifecycle_foreground();
    }

    /// Report iOS scenePhase = `.background`. Fire-and-forget.
    pub fn lifecycle_background(&self) {
        self.inner.lifecycle_background();
    }

    /// Register (or clear) the NMPU frame observer. After return the previous
    /// sink is neither registered nor mid-invocation (quiescence). Mirrors
    /// `nmp-uniffi::NmpApp::set_update_sink`.
    pub fn set_update_sink(&self, sink: Option<Box<dyn UpdateSink>>) {
        let listener: Option<UpdateListener> = sink.map(|s| {
            let s: Arc<dyn UpdateSink> = Arc::from(s);
            Arc::new(move |bytes: &[u8]| {
                let frame = bytes.to_vec();
                let s = Arc::clone(&s);
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                    s.on_update(frame)
                }));
            }) as UpdateListener
        });
        self.inner.set_update_listener(listener);
    }

    /// Sign in with a local nsec and (when `make_active`) activate it. The nsec
    /// is wiped on drop (`Zeroizing`). D004: handed to NMP once.
    pub fn signin_nsec(&self, nsec: String, make_active: bool) {
        self.inner.add_signer(
            nmp_core::SignerSource::LocalNsec(zeroize::Zeroizing::new(nsec)),
            make_active,
        );
    }

    /// Remove an identity; the actor owns the active-account transition.
    pub fn remove_account(&self, identity_id: String) {
        self.inner.remove_account(identity_id);
    }

    /// Add a relay. `role` is an NMP relay-role token (e.g. `"both"`).
    pub fn add_relay(&self, url: String, role: String) {
        self.inner.add_relay(url, role);
    }

    /// Retry a parked publish-outbox row by its handle.
    pub fn retry_publish(&self, handle: String) {
        self.inner.retry_publish(handle);
    }

    /// Seed 29er's Rust-owned default relay set (D7). `true` when ≥1 relay was
    /// handed to the kernel.
    pub fn seed_default_relays(&self) -> bool {
        crate::relay_seeding::seed_default_relays(&self.inner)
    }

    /// Seed relays from a `[["url","role"],…]` JSON array (the `NMP_TEST_RELAYS`
    /// override). `false` on malformed/empty so the caller falls back.
    pub fn seed_relays_from_json(&self, json: String) -> bool {
        crate::relay_seeding::seed_relays_from_json_str(&self.inner, &json)
    }

    /// Dispatch a pre-built `DispatchEnvelope` (the generic byte lane, ADR-0071).
    pub fn dispatch_action(&self, envelope: Vec<u8>) -> DispatchOutcome {
        let out = dispatch_action_bytes_typed(&self.inner, &envelope);
        DispatchOutcome {
            correlation_id: out.correlation_id,
            error: out.error,
            code: out.code,
        }
    }

    /// Dispatch a NIP-29 action by `(namespace, body_json)`. 29er builds the
    /// typed payload + envelope in Rust so the shell hand-assembles no
    /// FlatBuffers. The `nmp.nip29.post_chat_message` doorway runs the shared
    /// composer and re-emits under `nmp.nip29.publish_group_event`. D6
    /// fail-closed: an unknown namespace / malformed body surfaces as
    /// `DispatchOutcome.error`.
    pub fn dispatch_nip29_action(&self, namespace: String, body_json: String) -> DispatchOutcome {
        dispatch_nip29_action(&self.inner, &namespace, &body_json)
    }

    /// Wire a single NIP-29 group's chat-message read view (kinds 9 + 11).
    /// Singleton: re-opening replaces the prior group-events view.
    pub fn register_group_chat(&self, group_id_json: String) {
        let Ok(group_id) = serde_json::from_str::<GroupId>(&group_id_json) else {
            return;
        };
        let _handle = self
            .inner
            .open_nip29_group_events_session(Nip29GroupEventsSession::new(
                group_id,
                vec![KIND_CHAT_MESSAGE, KIND_DISCUSSION_OR_ARTIFACT],
            ));
    }

    /// Open a NIP-29 group-discovery session for one host relay (singleton).
    /// Closes any prior session first. `true` on success.
    pub fn open_group_discovery(&self, host_relay_url: String) -> bool {
        if host_relay_url.is_empty() {
            return false;
        }
        let Ok(mut slot) = self.discovery.lock() else {
            return false;
        };
        if let Some(prev) = slot.take() {
            self.teardown_discovery_session(prev);
        }
        match self.open_discovery_session(host_relay_url) {
            Some(session) => {
                *slot = Some(session);
                true
            }
            None => false,
        }
    }

    /// Close the current NIP-29 group-discovery session (if any).
    pub fn close_group_discovery(&self) {
        if let Ok(mut slot) = self.discovery.lock() {
            if let Some(session) = slot.take() {
                self.teardown_discovery_session(session);
            }
        }
    }

    /// Refresh group discovery after a local store reset: tear down the current
    /// session, open a fresh one for `host_relay_url`, and re-dispatch
    /// `nmp.nip29.discover`. `true` when the new session opened.
    pub fn refresh_group_discovery(&self, host_relay_url: String) -> bool {
        let opened = self.open_group_discovery(host_relay_url.clone());
        if opened {
            let body = serde_json::json!({ "relay_url": host_relay_url }).to_string();
            let _ = self.dispatch_nip29_action("nmp.nip29.discover".to_string(), body);
        }
        opened
    }

    /// Mark a group's direct kind:9 messages read inside the open group-tree
    /// session. The next tree snapshot folds the read state into unread counts.
    pub fn mark_group_read(&self, group_id: String) {
        if group_id.is_empty() {
            return;
        }
        if let Ok(slot) = self.discovery.lock() {
            if let Some(session) = slot.as_ref() {
                session.tree_projection.mark_read(&group_id);
            }
        }
    }

    /// Select the active NIP-29 relay. `true` when a known relay was selected.
    pub fn relay_selector_select_relay(&self, relay_url: String) -> bool {
        let Some(selector) = self.relay_selector_arc() else {
            return false;
        };
        let Some(selected) = selector.select_relay(&relay_url) else {
            return false;
        };
        self.inner.add_relay(selected, "both".to_string());
        true
    }

    /// Add a relay to the user's NIP-51 relay set + select it.
    pub fn relay_selector_add_relay(&self, relay_url: String) -> bool {
        let Some(selector) = self.relay_selector_arc() else {
            return false;
        };
        let tx = self.inner.actor_sender();
        let Some(added) = selector.add_relay(&relay_url, &tx) else {
            return false;
        };
        self.inner.add_relay(added, "both".to_string());
        true
    }

    /// Remove a relay from the user's NIP-51 relay set.
    pub fn relay_selector_remove_relay(&self, relay_url: String) -> bool {
        let Some(selector) = self.relay_selector_arc() else {
            return false;
        };
        let tx = self.inner.actor_sender();
        let Some(removed) = selector.remove_relay(&relay_url, &tx) else {
            return false;
        };
        if removed != crate::config::public_group_relay_url() {
            self.inner.remove_relay(removed);
        }
        true
    }
}

// ── Internal (non-exported) helpers ──────────────────────────────────────────

impl TwentyNinerApp {
    fn relay_selector_arc(&self) -> Option<Arc<RelaySelectorProjection>> {
        self.relay_selector.lock().ok().and_then(|g| g.clone())
    }

    /// Open the canonical discovery + joined read sessions and layer 29er's
    /// kind:9 group-tree composite (`nmp.29er.group_tree`) on top.
    fn open_discovery_session(&self, relay_url: String) -> Option<GroupDiscoverySession> {
        let app = &self.inner;

        // 1. Canonical discovery door — owns `DiscoveredGroupsProjection`, the
        //    `nmp.nip29.discovered_groups` sidecar, the relay-pinned interest,
        //    and #2088 hydration. Retain the reader to compose the tree.
        let (discovery_handle, discovered) = app.open_nip29_group_discovery_session_with_reader(
            Nip29GroupDiscoverySession::new(relay_url.clone()),
        );

        // 2. Viewer membership/admin truth from the account-scoped joined door.
        let active_account = app.active_account_handle();
        let active_pubkey = active_account
            .lock()
            .ok()
            .and_then(|slot| slot.clone())
            .unwrap_or_default();
        let joined_session = if active_pubkey.is_empty() {
            None
        } else {
            app.open_nip29_joined_groups_session_with_reader(Nip29JoinedGroupsSession::new(
                active_pubkey,
                relay_url.clone(),
            ))
        };
        let (joined_handle, joined): (Option<Nip29JoinedGroupsHandle>, _) = match joined_session {
            Some((handle, reader)) => (Some(handle), Some(reader)),
            None => (None, None),
        };

        // 3. 29er-owned kind:9 unread/preview reader — a hydrating, relay-pinned
        //    observed projection with explicit lifecycle ownership.
        let tree_messages = Arc::new(GroupTreeProjection::new());
        let filter_json = format!(r#"{{"kinds":[{KIND_CHAT_MESSAGE}]}}"#);
        let replay_shapes: Vec<nmp_planner::InterestShape> =
            nmp_planner::InterestShape::from_filter_json(&filter_json)
                .map(|mut shape| {
                    shape.relay_pin = Some(relay_url.clone());
                    shape
                })
                .into_iter()
                .collect();
        let tree_observer_id = app.open_observed_projection(ObservedProjection {
            observer: Arc::clone(&tree_messages) as Arc<dyn ObservedProjectionSink>,
            filter_json,
            consumer_id: format!("29er.nip29.group_tree.kind9:{relay_url}"),
            scope: 1,
            relay_pin: Some(relay_url.clone()),
            replay_shapes,
            replay_limit: GROUP_TREE_REPLAY_LIMIT,
        });
        if tree_observer_id.0 == 0 {
            app.close_nip29_group_discovery_session(discovery_handle);
            if let Some(handle) = joined_handle {
                app.close_nip29_joined_groups_session(handle);
            }
            return None;
        }

        // 4. 29er composite — derive `nmp.29er.group_tree` from the canonical
        //    door snapshots + the app-owned kind:9 summaries.
        let tree_discovered = Arc::clone(&discovered);
        let tree_joined = joined.clone();
        let tree_messages_for_sidecar = Arc::clone(&tree_messages);
        let active_account_for_tree = Arc::clone(&active_account);
        app.register_typed_snapshot_projection(GROUP_TREE_KEY, move || {
            let snapshot = tree_discovered.snapshot();
            let messages = tree_messages_for_sidecar.snapshot();
            let active_pubkey = active_account_for_tree
                .lock()
                .ok()
                .and_then(|slot| slot.clone())
                .unwrap_or_default();
            let joined_snapshot = tree_joined
                .as_ref()
                .map(|projection| projection.snapshot())
                .unwrap_or_default();
            let membership: GroupMembershipMap =
                membership_from_joined(&joined_snapshot, &active_pubkey);
            Some(TypedProjectionData {
                key: GROUP_TREE_KEY.to_string(),
                schema_id: GROUP_TREE_SCHEMA_ID.to_string(),
                schema_version: GROUP_TREE_SCHEMA_VERSION,
                file_identifier: String::from_utf8_lossy(GROUP_TREE_FILE_IDENTIFIER).into_owned(),
                payload: encode_group_tree_snapshot(&snapshot, &messages, &membership),
                ..Default::default()
            })
        });

        Some(GroupDiscoverySession {
            tree_observer_id,
            tree_projection: tree_messages,
            discovery_handle,
            joined_handle,
        })
    }

    /// Tear down a group-discovery session: the 29er kind:9 observer + composite
    /// snapshot, then the canonical discovery + joined read sessions (which own
    /// and reclaim their sidecars + interests).
    fn teardown_discovery_session(&self, session: GroupDiscoverySession) {
        let app = &self.inner;
        app.close_observed_projection(session.tree_observer_id);
        app.remove_snapshot_projection(GROUP_TREE_KEY);
        app.close_nip29_group_discovery_session(session.discovery_handle);
        if let Some(handle) = session.joined_handle {
            app.close_nip29_joined_groups_session(handle);
        }
    }
}

// ── Clamp helpers (parity with nmp-uniffi / nmp_app_start) ───────────────────

fn clamp_visible(visible_limit: u32) -> usize {
    if visible_limit == 0 {
        DEFAULT_VISIBLE_LIMIT
    } else {
        visible_limit.clamp(1, 500) as usize
    }
}

fn clamp_emit_hz(emit_hz: u32) -> u32 {
    if emit_hz == 0 {
        DEFAULT_EMIT_HZ
    } else {
        emit_hz.clamp(1, 12)
    }
}

// ── Dispatch payload encoders (ported verbatim from the old C-ABI ffi.rs) ────

/// Build the typed payload + open `DispatchEnvelope` for a NIP-29 action and
/// dispatch it through the byte lane, returning the typed [`DispatchOutcome`].
///
/// Shared by [`TwentyNinerApp::dispatch_nip29_action`] (iOS/Android) and the
/// native Rust TUI, so the typed-payload encoding lives in exactly one place.
/// The `nmp.nip29.post_chat_message` doorway runs the shared composer and
/// re-emits under `nmp.nip29.publish_group_event`. D6 fail-closed.
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
    let out = dispatch_action_bytes_typed(app, &envelope);
    DispatchOutcome {
        correlation_id: out.correlation_id,
        error: out.error,
        code: out.code,
    }
}

/// Process-local correlation-id source for 29er's byte-doorway dispatches.
static NEXT_CORRELATION_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) fn mint_correlation_id() -> String {
    let n = NEXT_CORRELATION_ID.fetch_add(1, Ordering::Relaxed);
    format!("29er-{n}")
}

/// Raw chat-send body the shell hands to [`CHAT_SEND_NAMESPACE`].
#[derive(serde::Deserialize)]
struct ChatSendBody {
    group: GroupId,
    #[serde(default)]
    content: String,
    #[serde(default)]
    mention_pubkeys: Vec<String>,
}

/// Compose a raw chat-send body into the typed [`ActionPayload`] bytes for a
/// kind:9 `PublishGroupEventInput`. Returns `None` on a malformed body (D6).
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

/// Encode `json` into typed [`ActionPayload`] FlatBuffers bytes for `namespace`.
/// `None` for an unknown namespace (D6 fail-closed — no JSON fallback).
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
        "nmp.nip29.react_in_group" => encode::<nmp_nip29::action::ReactInGroupInput>(json),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_send_doorway_composes_kind9_publish_group_event() {
        const HEX: &str = "3bf0c63fcb93463407af97a5e5ee64fa883d107ef9e558472c4eb9aaaefa459d";
        let npub = nmp_core::nip19::encode_npub(HEX).expect("valid hex");
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
