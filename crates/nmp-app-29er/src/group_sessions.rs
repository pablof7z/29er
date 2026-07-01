//! NIP-29 group-read sessions + the `dispatchNip29Action` convenience verb on
//! [`TwentyNinerApp`].
//!
//! Mirrors the session-opening pattern the native Rust TUI
//! (`crates/29er-tui/src/app.rs`) already drives directly against
//! `nmp-native-runtime`'s `Nip29*Session` doors — this module is the same
//! composition, exposed through `#[uniffi::export]` so the iOS shell can call
//! it via generated Swift.
//!
//! ## State ownership
//!
//! [`TwentyNinerApp`] keeps session handles in Rust. Minting a second UniFFI
//! object per session is unnecessary ceremony for a facade that only ever has
//! one discovery / chat / roster session open at a time (each door is itself a
//! kernel-level singleton — re-opening replaces the prior session).
//! So [`GroupSessions`] holds the live handles behind `Mutex`es, owned by
//! `TwentyNinerApp` itself (mirrors how `inner: NmpApp` is already the single
//! source of truth for the runtime) — Swift just calls `open_group_discovery`
//! / `close_group_discovery` etc. with no handle of its own to manage.
//!
//! ## The group-tree composite (`"nmp.29er.group_tree"`)
//!
//! `open_group_discovery` layers ONE 29er-owned read on top of NMP's
//! canonical discovery + joined-groups doors: a hydrating, relay-pinned kind:9
//! observed projection feeding [`crate::group_tree::GroupTreeProjection`]
//! (per-group unread + last-message preview), composed with the discovery
//! door's catalog and the joined-groups door's membership truth into the
//! app-owned `"nmp.29er.group_tree"` typed snapshot. This is the same
//! composition 29er wants for its discover screen.
//!
//! ## Joined-groups tracking is reactive, not a one-shot snapshot
//!
//! This module registers an identity-change observer (the same pattern
//! `29er-tui::init_nmp` uses) once, in [`GroupSessions::new`], so the
//! joined-groups session — and therefore the `is_member`/`is_admin` flags
//! folded into `"nmp.29er.group_tree"` — stays correct across a later sign-in
//! or account switch, not just whatever account happened to be active when
//! discovery was opened.

use std::sync::{Arc, Mutex};

use nmp_core::substrate::{ObservedProjection, ObservedProjectionRegistrar};
use nmp_core::{ObservedProjectionId, ObservedProjectionSink, TypedProjectionData};
use nmp_native_runtime::{
    Nip29GroupDiscoveryHandle, Nip29GroupDiscoverySession, Nip29GroupEventsHandle,
    Nip29GroupEventsSession, Nip29GroupRosterHandle, Nip29GroupRosterSession,
    Nip29JoinedGroupsHandle, Nip29JoinedGroupsSession, NmpApp,
};
use nmp_nip29::{GroupEventsProjection, GroupId, JoinedGroupsProjection};

use crate::group_chat::{
    encode_group_chat_snapshot, GROUP_CHAT_FILE_IDENTIFIER, GROUP_CHAT_SCHEMA_ID,
    GROUP_CHAT_SCHEMA_VERSION,
};
use crate::group_tree::{
    encode_group_tree_snapshot, membership_from_joined, GroupTreeProjection,
    GROUP_TREE_FILE_IDENTIFIER, GROUP_TREE_SCHEMA_ID, GROUP_TREE_SCHEMA_VERSION,
};
use crate::kinds::{KIND_CHAT_MESSAGE, KIND_DISCUSSION_OR_ARTIFACT};
use crate::DispatchOutcome;
use crate::TwentyNinerApp;

/// `1` = `Global` scope (account-agnostic) — matches the discovery door's own
/// scope for the 29er-owned kind:9 tree observer (`nmp-native-runtime`'s
/// `group_feed` module keeps the same constant private, so it is re-declared
/// here for this one consumer).
const SCOPE_GLOBAL: u32 = 1;

/// Bounded read-cache replay budget for the 29er kind:9 group-tree observed
/// projection. Mirrors `nmp_feed::DEFAULT_FEED_WINDOW_LIMIT` (80) — kept as a
/// local const so this crate does not take an `nmp-feed` dependency just for
/// one number.
const GROUP_TREE_REPLAY_LIMIT: usize = 80;

/// The live `"nmp.29er.group_tree"` composition opened by
/// [`GroupSessions::open_discovery`].
struct DiscoverySession {
    handle: Nip29GroupDiscoveryHandle,
    tree_observer_id: ObservedProjectionId,
    tree_projection: Arc<GroupTreeProjection>,
}

/// The live `"nmp.29er.group_chat"` composition opened by
/// [`GroupSessions::open_chat`].
struct ChatSession {
    handle: Nip29GroupEventsHandle,
}

/// All NIP-29 group-read session state owned by one [`TwentyNinerApp`].
///
/// Each field is an independent singleton session — opening a new discovery
/// (or chat, or roster) session always replaces the prior one, matching the
/// kernel-level singleton semantics each `Nip29*Session` door already
/// implements.
pub(crate) struct GroupSessions {
    discovery: Mutex<Option<DiscoverySession>>,
    chat: Mutex<Option<ChatSession>>,
    roster: Mutex<Option<Nip29GroupRosterHandle>>,
    /// The relay the joined-groups session should be scoped to. `Some` only
    /// while a discovery session is open; read by the identity-change
    /// observer on every active-account switch.
    joined_relay: Arc<Mutex<Option<String>>>,
    joined: Arc<Mutex<Option<Nip29JoinedGroupsHandle>>>,
    /// The live joined-groups projection reader, shared with the
    /// `"nmp.29er.group_tree"` composition closure so membership stays
    /// reactive across an account switch (re-read every snapshot tick).
    joined_reader: Arc<Mutex<Option<Arc<JoinedGroupsProjection>>>>,
}

impl GroupSessions {
    /// Build empty session state and register the identity-change observer
    /// that keeps the joined-groups session in sync with the active account.
    /// Registered once, before any sign-in — mirrors `29er-tui::init_nmp`
    /// registering its own identity-change observer "before sign-in so we
    /// never miss the first frame".
    pub(crate) fn new(app: &Arc<NmpApp>) -> Self {
        let joined_relay: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let joined: Arc<Mutex<Option<Nip29JoinedGroupsHandle>>> = Arc::new(Mutex::new(None));
        let joined_reader: Arc<Mutex<Option<Arc<JoinedGroupsProjection>>>> =
            Arc::new(Mutex::new(None));

        let observer_app = Arc::clone(app);
        let observer_relay = Arc::clone(&joined_relay);
        let observer_joined = Arc::clone(&joined);
        let observer_reader = Arc::clone(&joined_reader);
        app.register_identity_change_observer(move |pubkey| {
            let relay = observer_relay.lock().ok().and_then(|slot| slot.clone());
            sync_joined_session(
                &observer_app,
                pubkey,
                relay,
                &observer_joined,
                &observer_reader,
            );
        });

        Self {
            discovery: Mutex::new(None),
            chat: Mutex::new(None),
            roster: Mutex::new(None),
            joined_relay,
            joined,
            joined_reader,
        }
    }

    /// Open a fresh discovery + group-tree composition for `host_relay_url`.
    /// Callers MUST close any existing discovery session first (both
    /// `open_group_discovery` and `refresh_group_discovery` do so). `false`
    /// when the discovery composition could not be opened (D6).
    pub(crate) fn open_discovery(&self, app: &NmpApp, host_relay_url: String) -> bool {
        let Some(session) =
            build_discovery_session(app, host_relay_url.clone(), &self.joined_reader)
        else {
            return false;
        };
        if let Ok(mut relay_slot) = self.joined_relay.lock() {
            *relay_slot = Some(host_relay_url);
        }
        if let Ok(mut slot) = self.discovery.lock() {
            *slot = Some(session);
        }
        // Retroactively sync the joined-groups session: an account may
        // already be active (sign-in happened before this discovery open),
        // in which case the identity-change observer already fired against a
        // `None` relay and skipped. Re-run the same sync now that the relay
        // is known.
        let active_pubkey = app
            .active_account_handle()
            .lock()
            .ok()
            .and_then(|slot| slot.clone());
        let relay = self.joined_relay.lock().ok().and_then(|slot| slot.clone());
        sync_joined_session(app, active_pubkey, relay, &self.joined, &self.joined_reader);
        true
    }

    /// Tear down the discovery + group-tree + joined-groups composition.
    /// Idempotent (D6) — closing with nothing open is a silent no-op.
    pub(crate) fn close_discovery(&self, app: &NmpApp) {
        if let Ok(mut relay_slot) = self.joined_relay.lock() {
            *relay_slot = None;
        }
        sync_joined_session(app, None, None, &self.joined, &self.joined_reader);
        if let Ok(mut slot) = self.discovery.lock() {
            if let Some(session) = slot.take() {
                app.close_observed_projection(session.tree_observer_id);
                app.remove_snapshot_projection(GROUP_TREE_SCHEMA_ID);
                app.close_nip29_group_discovery_session(session.handle);
            }
        }
    }

    /// Fold a direct kind:9 message read into the open group-tree
    /// composition's unread accounting. A no-op when no discovery session is
    /// open (D6).
    pub(crate) fn mark_group_read(&self, local_id: &str) {
        if let Ok(slot) = self.discovery.lock() {
            if let Some(session) = slot.as_ref() {
                session.tree_projection.mark_read(local_id);
            }
        }
    }

    /// Open the group-chat (kind:9 + kind:11) read session for `group_id`,
    /// replacing any previously open chat session.
    pub(crate) fn open_chat(&self, app: &NmpApp, group_id: GroupId) {
        self.close_chat(app);
        let (handle, reader) =
            app.open_nip29_group_events_session_with_reader(Nip29GroupEventsSession::new(
                group_id,
                vec![KIND_CHAT_MESSAGE, KIND_DISCUSSION_OR_ARTIFACT],
            ));
        register_group_chat_snapshot(app, reader);
        if let Ok(mut slot) = self.chat.lock() {
            *slot = Some(ChatSession { handle });
        }
    }

    /// Close the open group-chat session, if any (D6 idempotent).
    pub(crate) fn close_chat(&self, app: &NmpApp) {
        if let Ok(mut slot) = self.chat.lock() {
            if let Some(session) = slot.take() {
                app.remove_snapshot_projection(GROUP_CHAT_SCHEMA_ID);
                app.close_nip29_group_events_session(session.handle);
            }
        }
    }

    /// Open the member-roster read session for `group_id`, replacing any
    /// previously open roster session.
    pub(crate) fn open_roster(&self, app: &NmpApp, group_id: GroupId) {
        let handle = app.open_nip29_group_roster_session(Nip29GroupRosterSession::new(group_id));
        if let Ok(mut slot) = self.roster.lock() {
            *slot = Some(handle);
        }
    }

    /// Close the open roster session, if any (D6 idempotent).
    pub(crate) fn close_roster(&self, app: &NmpApp) {
        if let Ok(mut slot) = self.roster.lock() {
            if let Some(handle) = slot.take() {
                app.close_nip29_group_roster_session(handle);
            }
        }
    }
}

/// Reconcile the joined-groups session against `pubkey`/`relay`.
///
/// `(Some(pubkey), Some(relay))` opens (or, if the active pubkey already
/// matches the live reader, leaves alone) a joined-groups session scoped to
/// `relay`. Anything else (signed out, or no discovery relay known yet) tears
/// down a stale session rather than leak a previous account's/relay's
/// membership truth — same fail-safe rationale as
/// `crate::group_tree::membership_from_joined`'s active-pubkey mismatch
/// guard.
fn sync_joined_session(
    app: &NmpApp,
    pubkey: Option<String>,
    relay: Option<String>,
    joined: &Arc<Mutex<Option<Nip29JoinedGroupsHandle>>>,
    joined_reader: &Arc<Mutex<Option<Arc<JoinedGroupsProjection>>>>,
) {
    let pubkey = pubkey.filter(|p| !p.is_empty());
    let (Some(pubkey), Some(relay)) = (pubkey, relay) else {
        if let Ok(mut handle_slot) = joined.lock() {
            if let Some(handle) = handle_slot.take() {
                app.close_nip29_joined_groups_session(handle);
            }
        }
        if let Ok(mut reader_slot) = joined_reader.lock() {
            *reader_slot = None;
        }
        return;
    };

    let already_current = joined_reader
        .lock()
        .ok()
        .and_then(|slot| slot.clone())
        .map(|projection| projection.snapshot().active_pubkey == pubkey)
        .unwrap_or(false);
    if already_current {
        return;
    }

    let opened = app
        .open_nip29_joined_groups_session_with_reader(Nip29JoinedGroupsSession::new(pubkey, relay));
    let (handle, reader) = match opened {
        Some((handle, reader)) => (Some(handle), Some(reader)),
        None => (None, None),
    };
    if let Ok(mut handle_slot) = joined.lock() {
        if let Some(old) = handle_slot.take() {
            app.close_nip29_joined_groups_session(old);
        }
        *handle_slot = handle;
    }
    if let Ok(mut reader_slot) = joined_reader.lock() {
        *reader_slot = reader;
    }
}

/// Open NMP's canonical discovery door and layer the 29er-owned kind:9
/// group-tree composite on top. Returns `None` (D6 fail-closed) when the
/// kernel refuses the kind:9 observed projection.
fn build_discovery_session(
    app: &NmpApp,
    relay_url: String,
    joined_reader: &Arc<Mutex<Option<Arc<JoinedGroupsProjection>>>>,
) -> Option<DiscoverySession> {
    let (discovery_handle, discovered) = app.open_nip29_group_discovery_session_with_reader(
        Nip29GroupDiscoverySession::new(relay_url.clone()),
    );

    let tree_messages = Arc::new(GroupTreeProjection::new());
    let filter_json = format!(r#"{{"kinds":[{KIND_CHAT_MESSAGE}]}}"#);
    let Some(mut shape) = nmp_planner::InterestShape::from_filter_json(&filter_json) else {
        app.close_nip29_group_discovery_session(discovery_handle);
        return None;
    };
    shape.relay_pin = Some(relay_url.clone());
    let tree_observer_id = app.open_observed_projection(ObservedProjection::from_shape(
        Arc::clone(&tree_messages) as Arc<dyn ObservedProjectionSink>,
        format!("29er.facade.group_tree.kind9:{relay_url}"),
        SCOPE_GLOBAL,
        shape,
        GROUP_TREE_REPLAY_LIMIT,
    ));
    if tree_observer_id.0 == 0 {
        app.close_nip29_group_discovery_session(discovery_handle);
        return None;
    }

    let tree_discovered = Arc::clone(&discovered);
    let tree_messages_for_sidecar = Arc::clone(&tree_messages);
    let active_account = app.active_account_handle();
    let joined_reader_for_sidecar = Arc::clone(joined_reader);
    app.register_typed_snapshot_projection(GROUP_TREE_SCHEMA_ID, move || {
        let snapshot = tree_discovered.snapshot();
        let messages = tree_messages_for_sidecar.snapshot();
        // Re-read the live active pubkey + joined reader every tick so
        // membership is recomputed on an account switch and never leaks a
        // previous account's truth.
        let active_pubkey = active_account
            .lock()
            .ok()
            .and_then(|slot| slot.clone())
            .unwrap_or_default();
        let joined_snapshot = joined_reader_for_sidecar
            .lock()
            .ok()
            .and_then(|slot| slot.clone())
            .map(|projection| projection.snapshot())
            .unwrap_or_default();
        let membership = membership_from_joined(&joined_snapshot, &active_pubkey);
        Some(TypedProjectionData {
            key: GROUP_TREE_SCHEMA_ID.to_string(),
            schema_id: GROUP_TREE_SCHEMA_ID.to_string(),
            schema_version: GROUP_TREE_SCHEMA_VERSION,
            file_identifier: String::from_utf8_lossy(GROUP_TREE_FILE_IDENTIFIER).into_owned(),
            payload: encode_group_tree_snapshot(&snapshot, &messages, &membership),
            ..Default::default()
        })
    });

    Some(DiscoverySession {
        handle: discovery_handle,
        tree_observer_id,
        tree_projection: tree_messages,
    })
}

fn register_group_chat_snapshot(app: &NmpApp, reader: Arc<GroupEventsProjection>) {
    app.register_typed_snapshot_projection(GROUP_CHAT_SCHEMA_ID, move || {
        let snapshot = reader.snapshot();
        Some(TypedProjectionData {
            key: GROUP_CHAT_SCHEMA_ID.to_string(),
            schema_id: GROUP_CHAT_SCHEMA_ID.to_string(),
            schema_version: GROUP_CHAT_SCHEMA_VERSION,
            file_identifier: String::from_utf8_lossy(GROUP_CHAT_FILE_IDENTIFIER).into_owned(),
            payload: encode_group_chat_snapshot(&snapshot),
            ..Default::default()
        })
    });
}

// ── TwentyNinerApp UniFFI surface ───────────────────────────────────────────

#[uniffi::export]
impl TwentyNinerApp {
    /// Dispatch a NIP-29 action through the typed per-namespace byte doorway
    /// (join/leave/create-group/post-chat-message/etc.). Thin wrapper over
    /// [`crate::dispatch::dispatch_nip29_action`] — the same encoder the
    /// native Rust TUI dispatches every NIP-29 action through. D6
    /// fail-closed: an unknown namespace or a malformed body returns a
    /// populated [`DispatchOutcome::error`], never a panic.
    pub fn dispatch_nip29_action(&self, namespace: String, body_json: String) -> DispatchOutcome {
        crate::dispatch::dispatch_nip29_action(self.app(), &namespace, &body_json)
    }

    /// Open a NIP-29 group-discovery session for one host relay: NMP's
    /// canonical discovery + joined-groups doors, plus the 29er-owned kind:9
    /// group-tree composite (per-group unread + last-message preview +
    /// viewer membership), folded into the `"nmp.29er.group_tree"` typed
    /// snapshot the iOS shell reads through [`crate::UpdateSink`].
    ///
    /// Replaces any previously open discovery session. `false` (D6) on an
    /// empty `host_relay_url` or if the kernel refuses the kind:9 observed
    /// projection. Does NOT dispatch `nmp.nip29.discover` itself — call
    /// [`Self::dispatch_nip29_action`] for that: open is a pure read-session
    /// open, discovery is a separate action.
    pub fn open_group_discovery(&self, host_relay_url: String) -> bool {
        let relay = host_relay_url.trim().to_string();
        if relay.is_empty() {
            return false;
        }
        self.sessions().close_discovery(self.app());
        self.sessions().open_discovery(self.app(), relay)
    }

    /// Close the open group-discovery session (idempotent, D6).
    pub fn close_group_discovery(&self) {
        self.sessions().close_discovery(self.app());
    }

    /// Refresh the group-discovery session after a local store reset.
    ///
    /// Rust owns the read-model lifecycle: tears down the previous
    /// discovery/tree/joined composition unconditionally, opens a fresh
    /// composition for
    /// `host_relay_url`, and re-dispatches `nmp.nip29.discover` for that
    /// relay. `false` on an empty relay or if the fresh composition or the
    /// re-dispatch fails.
    pub fn refresh_group_discovery(&self, host_relay_url: String) -> bool {
        self.sessions().close_discovery(self.app());
        let relay = host_relay_url.trim().to_string();
        if relay.is_empty() {
            return false;
        }
        if !self.sessions().open_discovery(self.app(), relay.clone()) {
            return false;
        }
        let outcome = crate::dispatch::dispatch_nip29_action(
            self.app(),
            "nmp.nip29.discover",
            &serde_json::json!({ "relay_url": relay }).to_string(),
        );
        outcome.error.is_none()
    }

    /// Mark a group's direct kind:9 messages read inside the open group-tree
    /// composition. `local_id` is the group's bare local id (NOT a
    /// `GroupId` JSON object — mirrors `GroupTreeProjection::mark_read`).
    /// The next tree snapshot folds this into the group's aggregate unread
    /// count. No-op when no discovery session is open (D6).
    pub fn mark_group_read(&self, local_id: String) {
        self.sessions().mark_group_read(&local_id);
    }

    /// Open the group-chat (kind:9 + kind:11) read session for one group.
    /// `group_id_json` is a JSON [`GroupId`] object:
    /// `{"host_relay_url":"wss://groups.example.com","local_id":"room"}`.
    /// Replaces any previously open chat session. `false` (D6) on malformed
    /// JSON.
    pub fn open_group_chat(&self, group_id_json: String) -> bool {
        let Ok(group_id) = serde_json::from_str::<GroupId>(&group_id_json) else {
            return false;
        };
        self.sessions().open_chat(self.app(), group_id);
        true
    }

    /// Close the open group-chat session (idempotent, D6).
    pub fn close_group_chat(&self) {
        self.sessions().close_chat(self.app());
    }

    /// Open the member-roster read session for one group. `group_id_json` is
    /// a JSON [`GroupId`] object (same shape as [`Self::open_group_chat`]).
    /// Uses the dedicated roster door
    /// (`nmp_native_runtime::open_nip29_group_roster_session`). `false` (D6)
    /// on malformed JSON.
    pub fn open_group_roster(&self, group_id_json: String) -> bool {
        let Ok(group_id) = serde_json::from_str::<GroupId>(&group_id_json) else {
            return false;
        };
        self.sessions().open_roster(self.app(), group_id);
        true
    }

    /// Close the open roster session (idempotent, D6).
    pub fn close_group_roster(&self) {
        self.sessions().close_roster(self.app());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group_id_json() -> String {
        r#"{"host_relay_url":"wss://groups.example.com","local_id":"room"}"#.to_string()
    }

    #[test]
    fn open_group_discovery_rejects_empty_relay() {
        let app = TwentyNinerApp::new();
        assert!(!app.open_group_discovery(String::new()));
        assert!(!app.open_group_discovery("   ".to_string()));
    }

    #[test]
    fn open_group_discovery_registers_group_tree_snapshot() {
        let app = TwentyNinerApp::new();
        assert!(app.open_group_discovery("wss://groups.example.com".to_string()));
        assert!(app
            .app()
            .registered_typed_projection_keys()
            .iter()
            .any(|key| key == GROUP_TREE_SCHEMA_ID));
    }

    #[test]
    fn close_group_discovery_removes_group_tree_snapshot() {
        let app = TwentyNinerApp::new();
        assert!(app.open_group_discovery("wss://groups.example.com".to_string()));
        app.close_group_discovery();
        assert!(!app
            .app()
            .registered_typed_projection_keys()
            .iter()
            .any(|key| key == GROUP_TREE_SCHEMA_ID));
    }

    #[test]
    fn reopening_discovery_replaces_the_prior_session() {
        let app = TwentyNinerApp::new();
        assert!(app.open_group_discovery("wss://groups.example.com".to_string()));
        assert!(app.open_group_discovery("wss://other-groups.example.com".to_string()));
        // Only ONE "nmp.29er.group_tree" registration should be live — the
        // first session's observer/sidecar must have been torn down, not
        // leaked alongside the second.
        let count = app
            .app()
            .registered_typed_projection_keys()
            .iter()
            .filter(|key| key.as_str() == GROUP_TREE_SCHEMA_ID)
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn refresh_group_discovery_rejects_empty_relay_and_tears_down_prior_session() {
        let app = TwentyNinerApp::new();
        assert!(app.open_group_discovery("wss://groups.example.com".to_string()));
        assert!(!app.refresh_group_discovery(String::new()));
        // The prior handle is consumed regardless of whether the refresh's
        // own open succeeds.
        assert!(!app
            .app()
            .registered_typed_projection_keys()
            .iter()
            .any(|key| key == GROUP_TREE_SCHEMA_ID));
    }

    #[test]
    fn refresh_group_discovery_reopens_for_a_fresh_relay() {
        let app = TwentyNinerApp::new();
        assert!(app.open_group_discovery("wss://groups.example.com".to_string()));
        assert!(app.refresh_group_discovery("wss://groups.example.com".to_string()));
        assert!(app
            .app()
            .registered_typed_projection_keys()
            .iter()
            .any(|key| key == GROUP_TREE_SCHEMA_ID));
    }

    #[test]
    fn mark_group_read_with_no_open_session_is_a_silent_noop() {
        let app = TwentyNinerApp::new();
        // Must not panic.
        app.mark_group_read("room".to_string());
    }

    #[test]
    fn open_group_chat_rejects_malformed_json() {
        let app = TwentyNinerApp::new();
        assert!(!app.open_group_chat("not json".to_string()));
        assert!(!app.open_group_chat(r#"{"content":"missing group fields"}"#.to_string()));
    }

    #[test]
    fn open_group_chat_accepts_a_valid_group_id() {
        let app = TwentyNinerApp::new();
        assert!(app.open_group_chat(group_id_json()));
        assert!(app
            .app()
            .registered_typed_projection_keys()
            .iter()
            .any(|key| key == GROUP_CHAT_SCHEMA_ID));
        app.close_group_chat();
    }

    #[test]
    fn close_group_chat_removes_group_chat_snapshot() {
        let app = TwentyNinerApp::new();
        assert!(app.open_group_chat(group_id_json()));
        app.close_group_chat();
        assert!(!app
            .app()
            .registered_typed_projection_keys()
            .iter()
            .any(|key| key == GROUP_CHAT_SCHEMA_ID));
    }

    #[test]
    fn reopening_group_chat_replaces_the_prior_session() {
        let app = TwentyNinerApp::new();
        assert!(app.open_group_chat(group_id_json()));
        assert!(app.open_group_chat(
            r#"{"host_relay_url":"wss://groups.example.com","local_id":"other"}"#.to_string()
        ));
        let count = app
            .app()
            .registered_typed_projection_keys()
            .iter()
            .filter(|key| key.as_str() == GROUP_CHAT_SCHEMA_ID)
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn open_group_roster_rejects_malformed_json_and_accepts_valid() {
        let app = TwentyNinerApp::new();
        assert!(!app.open_group_roster("not json".to_string()));
        assert!(app.open_group_roster(group_id_json()));
        app.close_group_roster();
    }

    #[test]
    fn dispatch_nip29_action_fails_closed_for_unknown_namespace() {
        let app = TwentyNinerApp::new();
        let outcome =
            app.dispatch_nip29_action("nmp.nip29.unknown_namespace".to_string(), "{}".to_string());
        assert!(outcome.error.is_some());
        assert!(outcome.correlation_id.is_none());
    }
}
