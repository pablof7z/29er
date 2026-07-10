//! NIP-29 group-read sessions + the `dispatchNip29Action` convenience verb on
//! [`TwentyNinerApp`].
//!
//! Mirrors the session-opening pattern the native Rust TUI
//! (`crates/29er-tui/src/app.rs`) already drives directly against
//! `nmp-nip29`'s `Nip29*Session` doors — this module is the same
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
//! ## The group-tree composite (`"app.29er.group_tree"`)
//!
//! `open_group_discovery` layers ONE 29er-owned read on top of NMP's
//! canonical discovery + joined-groups doors: an observed feed-source session
//! over the active user's hosted groups feeding
//! [`crate::group_tree::GroupTreeProjection`] (per-group unread + last-message
//! preview), composed with the discovery door's catalog and the joined-groups
//! door's membership truth into the app-owned `"app.29er.group_tree"` typed
//! snapshot. This is the same composition 29er wants for its discover screen.
//!
//! ## Joined-groups tracking is reactive, not a one-shot snapshot
//!
//! This module registers an identity-change observer (the same pattern
//! `29er-tui::init_nmp` uses) once, in [`GroupSessions::new`], so the
//! joined-groups session — and therefore the `is_member`/`is_admin` flags
//! folded into `"app.29er.group_tree"` — stays correct across a later sign-in
//! or account switch, not just whatever account happened to be active when
//! discovery was opened.
//!
//! ## `preview`/`presence` reconcile is driven by an update-frame observer
//!
//! [`GroupSessions::open_discovery`] registers ONE
//! `NmpApp::register_update_frame_observer` callback (#3127) alongside the
//! discovery session: it fires on the update-listener thread on EVERY
//! emitted update frame (never the actor thread), re-derives the current
//! desired group-set from the live `discovered` reader, and reconciles both
//! `preview`/`presence` `KeyedReadCollection`s against it. This ONE trigger
//! subsumes what used to be two separate call sites (an open-time one-shot
//! sync, and a second sync from the identity-change observer): a frame is
//! emitted after every kernel state change, including discovery replay
//! landing and an identity/active-account switch (the doc comment on
//! `IdentityChangeRegistrar` guarantees the active-keys slot is written
//! BEFORE the frame that follows it is built), so reading the live
//! `active_pubkey` inside the SAME callback picks up an account switch with
//! no separate observer needed. `KeyedReadCollection::reconcile` is a no-op
//! diff when nothing changed, so firing on every frame (not just
//! discovery-relevant ones) is cheap and correct (#3115's own contract).
//!
//! Since #3131, the callback filters via the frame's `UpdateFrameInfo`
//! instead of unconditionally reconciling on every frame: it skips the
//! reconcile unless the `nmp.nip29.discovered_groups` typed sidecar
//! projection changed on this frame OR the live `active_pubkey` diverged
//! from the value observed on the previous frame (tracked locally — identity
//! switches are not a typed sidecar entry, so they cannot be read off
//! `info` and must be diffed directly to avoid silently missing an account
//! switch). Those two inputs are the entirety of what
//! `reconcile_group_tree_sessions` reads, so this is a lossless filter, not
//! an approximation.
//! [`GroupSessions::close_discovery`] unregisters the observer.

use std::sync::{Arc, Mutex};

use nmp_core::TypedProjectionData;
use nmp_native_runtime::{NmpApp, ProjectionKey};
use nmp_nip25::ReactionAggregateProjection;
use nmp_nip29::{
    close_nip29_group_discovery_session, close_nip29_group_events_session,
    close_nip29_group_roster_session, close_nip29_joined_groups_session,
    open_nip29_group_discovery_session_with_reader, open_nip29_group_events_session_with_reader,
    open_nip29_group_roster_session, open_nip29_joined_groups_session_with_reader,
    DiscoveredGroupsProjection, GroupEventsProjection, GroupId, JoinedGroupsProjection,
    Nip29GroupDiscoveryHandle, Nip29GroupDiscoverySession, Nip29GroupEventsHandle,
    Nip29GroupEventsSession, Nip29GroupRosterHandle, Nip29GroupRosterSession,
    Nip29JoinedGroupsHandle, Nip29JoinedGroupsSession,
};
use nmp_reactions::{
    close_nip25_group_reactions_session, open_nip25_group_reactions_session_with_reader,
    Nip25GroupReactionsHandle, Nip25GroupReactionsSession,
};

use crate::group_chat::{
    encode_group_chat_snapshot_with_reactions, GROUP_CHAT_FILE_IDENTIFIER, GROUP_CHAT_SCHEMA_ID,
    GROUP_CHAT_SCHEMA_VERSION,
};
use crate::group_presence::GroupPresenceSessions;
use crate::group_preview::GroupPreviewSessions;
use crate::group_tree::{
    encode_group_tree_snapshot, membership_from_joined, GroupTreeProjection,
    GROUP_TREE_FILE_IDENTIFIER, GROUP_TREE_SCHEMA_ID, GROUP_TREE_SCHEMA_VERSION,
};
use crate::kinds::{KIND_CHAT_MESSAGE, KIND_DISCUSSION_OR_ARTIFACT};
use crate::DispatchOutcome;
use crate::TwentyNinerApp;

/// The live `"app.29er.group_tree"` composition opened by
/// [`GroupSessions::open_discovery`].
struct DiscoverySession {
    handle: Nip29GroupDiscoveryHandle,
    tree_projection: Arc<GroupTreeProjection>,
    preview: Arc<GroupPreviewSessions>,
    presence: Arc<GroupPresenceSessions>,
    /// The live discovery reader — the "discovered-groups reactive source"
    /// the update-frame observer (registered in
    /// [`GroupSessions::open_discovery`]) reconciles `preview`/`presence`
    /// against on every emitted frame. Never read from inside the
    /// snapshot-tick closure's own registration (see that closure's doc
    /// note) — only from the observer's off-actor-thread callback.
    discovered: Arc<DiscoveredGroupsProjection>,
    /// Handle for [`NmpApp::unregister_update_frame_observer`], revoked in
    /// [`GroupSessions::close_discovery`].
    update_frame_observer_id: nmp_native_runtime::UpdateFrameObserverId,
}

/// The live `"app.29er.group_chat"` composition opened by
/// [`GroupSessions::open_chat`].
struct ChatSession {
    handle: Nip29GroupEventsHandle,
    reactions_handle: Nip25GroupReactionsHandle,
}

/// All NIP-29 group-read session state owned by one [`TwentyNinerApp`].
///
/// Each field is an independent singleton session — opening a new discovery
/// (or chat, or roster) session always replaces the prior one, matching the
/// kernel-level singleton semantics each `Nip29*Session` door already
/// implements.
pub(crate) struct GroupSessions {
    /// `Arc`-wrapped so the identity-change observer closure (registered once
    /// in [`Self::new`], before any discovery session exists) can look up
    /// whichever discovery session is live at the time identity changes and
    /// reconcile its `preview`/`presence` collections — the off-tick lane
    /// [`Self::open_discovery`] also uses. See `DiscoverySession::discovered`.
    discovery: Arc<Mutex<Option<DiscoverySession>>>,
    chat: Mutex<Option<ChatSession>>,
    roster: Mutex<Option<Nip29GroupRosterHandle>>,
    /// The relay the joined-groups session should be scoped to. `Some` only
    /// while a discovery session is open; read by the identity-change
    /// observer on every active-account switch.
    joined_relay: Arc<Mutex<Option<String>>>,
    joined: Arc<Mutex<Option<Nip29JoinedGroupsHandle>>>,
    /// The live joined-groups projection reader, shared with the
    /// `"app.29er.group_tree"` composition closure so membership stays
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
        let discovery: Arc<Mutex<Option<DiscoverySession>>> = Arc::new(Mutex::new(None));
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
            sync_joined_session(&observer_app, pubkey, relay, &observer_joined, &observer_reader);
            // `preview`/`presence` are NOT re-synced here: the update-frame
            // observer registered in `Self::open_discovery` already re-reads
            // the live active_pubkey and reconciles both on every emitted
            // frame — including the frame that follows this very identity
            // change (`IdentityChangeRegistrar`'s own contract: the
            // active-keys slot is written BEFORE that frame is built). A
            // second trigger here would be redundant, not incorrect.
        });

        Self {
            discovery,
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
    pub(crate) fn open_discovery(&self, app: Arc<NmpApp>, host_relay_url: String) -> bool {
        let Some(mut session) = build_discovery_session(
            Arc::clone(&app),
            host_relay_url.clone(),
            &self.joined_reader,
        ) else {
            return false;
        };
        if let Ok(mut relay_slot) = self.joined_relay.lock() {
            *relay_slot = Some(host_relay_url);
        }

        // THE sole reactive driver for `preview`/`presence` (module docs):
        // fires on every emitted update frame, on the update-listener
        // thread — never the actor thread (the 29er#60 deadlock class this
        // whole migration removes by construction; see
        // `build_discovery_session`'s doc note on the tick closure).
        // Re-derives the desired group set from the live discovery reader
        // and the live active_pubkey on every call, so ONE trigger covers
        // both discovery replay landing and a later identity switch.
        //
        // `reconcile_group_tree_sessions` reads exactly two inputs:
        // `session.discovered.snapshot()` (backed by the
        // `nmp.nip29.discovered_groups` typed sidecar projection, #3131) and
        // the live `active_pubkey`. A frame that touched neither cannot
        // change the reconcile's desired output, so the observer skips the
        // (idempotent, but not free) reconcile call for it. `active_pubkey`
        // has no projection-key representation in `info` (identity switches
        // are a separate observer, not a typed sidecar entry) — it is
        // diffed locally against the previous frame's value instead of
        // guessed at, so an account switch is never missed even on a frame
        // where the discovered-groups set itself did not change.
        let observer_app = Arc::clone(&app);
        let observer_discovery = Arc::clone(&self.discovery);
        let last_active_pubkey: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        session.update_frame_observer_id = app.register_update_frame_observer(
            move |info: &nmp_native_runtime::UpdateFrameInfo| {
                if let Ok(guard) = observer_discovery.lock() {
                    if let Some(session) = guard.as_ref() {
                        let groups_changed = info.changed_projection_keys.is_empty()
                            || info.changed(nmp_nip29::DISCOVERED_GROUPS_KEY);
                        let current_pubkey = observer_app
                            .active_account_handle()
                            .lock()
                            .ok()
                            .and_then(|slot| slot.clone());
                        let pubkey_changed = last_active_pubkey
                            .lock()
                            .map(|mut last| {
                                let changed = *last != current_pubkey;
                                *last = current_pubkey;
                                changed
                            })
                            .unwrap_or(true);
                        if groups_changed || pubkey_changed {
                            reconcile_group_tree_sessions(&observer_app, session);
                        }
                    }
                }
            },
        );

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
        sync_joined_session(
            &app,
            active_pubkey,
            relay,
            &self.joined,
            &self.joined_reader,
        );
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
                app.unregister_update_frame_observer(session.update_frame_observer_id);
                session.presence.close_all();
                session.preview.close_all(&session.tree_projection);
                app.remove_snapshot_projection(GROUP_TREE_SCHEMA_ID);
                let _ = close_nip29_group_discovery_session(app, session.handle);
            }
        }
    }

    /// Advance the NMP chat-presence read marker to the latest known direct
    /// message for the group. A no-op when no discovery/presence session is
    /// open or no latest message has been observed yet (D6).
    pub(crate) fn mark_group_read(&self, local_id: &str) {
        if let Ok(slot) = self.discovery.lock() {
            if let Some(session) = slot.as_ref() {
                let messages = session.tree_projection.snapshot();
                let latest = messages.last_message_for(local_id);
                let _ = session.presence.mark_read_to_latest(local_id, latest);
            }
        }
    }

    /// Open the group-chat (kind:9 + kind:11) read session for `group_id`,
    /// replacing any previously open chat session.
    pub(crate) fn open_chat(&self, app: &NmpApp, group_id: GroupId) {
        self.close_chat(app);
        let active_pubkey = app
            .active_account_handle()
            .lock()
            .ok()
            .and_then(|slot| slot.clone())
            .unwrap_or_default();
        let (handle, reader) = open_nip29_group_events_session_with_reader(
            app,
            Nip29GroupEventsSession::new(
                group_id.clone(),
                vec![KIND_CHAT_MESSAGE, KIND_DISCUSSION_OR_ARTIFACT],
            ),
        );
        let (reactions_handle, reactions_reader) = open_nip25_group_reactions_session_with_reader(
            app,
            Nip25GroupReactionsSession::new(group_id, active_pubkey),
        );
        register_group_chat_snapshot(app, reader, reactions_reader);
        if let Ok(mut slot) = self.chat.lock() {
            *slot = Some(ChatSession {
                handle,
                reactions_handle,
            });
        }
    }

    /// Close the open group-chat session, if any (D6 idempotent).
    pub(crate) fn close_chat(&self, app: &NmpApp) {
        if let Ok(mut slot) = self.chat.lock() {
            if let Some(session) = slot.take() {
                app.remove_snapshot_projection(GROUP_CHAT_SCHEMA_ID);
                let _ = close_nip29_group_events_session(app, session.handle);
                let _ = close_nip25_group_reactions_session(app, session.reactions_handle);
            }
        }
    }

    /// Open the member-roster read session for `group_id`, replacing any
    /// previously open roster session.
    pub(crate) fn open_roster(&self, app: &NmpApp, group_id: GroupId) {
        let handle = open_nip29_group_roster_session(app, Nip29GroupRosterSession::new(group_id));
        if let Ok(mut slot) = self.roster.lock() {
            *slot = Some(handle);
        }
    }

    /// Close the open roster session, if any (D6 idempotent).
    pub(crate) fn close_roster(&self, app: &NmpApp) {
        if let Ok(mut slot) = self.roster.lock() {
            if let Some(handle) = slot.take() {
                let _ = close_nip29_group_roster_session(app, handle);
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
                let _ = close_nip29_joined_groups_session(app, handle);
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

    let opened = open_nip29_joined_groups_session_with_reader(
        app,
        Nip29JoinedGroupsSession::new(pubkey, relay),
    );
    let (handle, reader) = match opened {
        Some((handle, reader)) => (Some(handle), Some(reader)),
        None => (None, None),
    };
    if let Ok(mut handle_slot) = joined.lock() {
        if let Some(old) = handle_slot.take() {
            let _ = close_nip29_joined_groups_session(app, old);
        }
        *handle_slot = handle;
    }
    if let Ok(mut reader_slot) = joined_reader.lock() {
        *reader_slot = reader;
    }
}

/// Reconciles `session.preview`/`session.presence` against `session
/// .discovered`'s CURRENT snapshot and the CURRENT live `active_pubkey`.
///
/// This is the ONE reconcile driver (module docs): the update-frame observer
/// registered in [`GroupSessions::open_discovery`] calls this on every
/// emitted frame, on the update-listener thread — never the actor thread, so
/// it never re-enters the 29er#60 deadlock class. `KeyedReadCollection::
/// reconcile` (which `preview.sync`/`presence.sync` call) is a no-op diff
/// when nothing changed, so calling this unconditionally on every frame
/// (not just discovery-relevant ones) is cheap and correct.
fn reconcile_group_tree_sessions(app: &NmpApp, session: &DiscoverySession) {
    let snapshot = session.discovered.snapshot();
    let active_pubkey = app
        .active_account_handle()
        .lock()
        .ok()
        .and_then(|slot| slot.clone())
        .unwrap_or_default();
    session.preview.sync(&snapshot);
    session.presence.sync(&snapshot, &active_pubkey);
}

/// Open NMP's canonical discovery door and layer the 29er-owned kind:9
/// group-tree composite on top. Returns `None` (D6 fail-closed) when the
/// kernel refuses the observed feed-source session.
///
/// Only CONSTRUCTS the `preview`/`presence` collections here — never
/// reconciles them ([`GroupSessions::open_discovery`] registers the
/// update-frame observer that does, via [`reconcile_group_tree_sessions`],
/// after this returns). The registered snapshot-tick closure below only
/// READS their current outputs on every tick, exactly like
/// `tree_messages`/`joined_reader`. Calling `KeyedReadCollection::
/// reconcile`/`close` (which `preview.sync`/`presence.sync`/`close_all` do)
/// from inside this closure is the 29er#60 deadlock class — the whole point
/// of the #3115 migration is that this closure structurally cannot do that
/// anymore (there is no `app`/collection handle captured for it to call
/// `.sync()` on).
fn build_discovery_session(
    app: Arc<NmpApp>,
    relay_url: String,
    joined_reader: &Arc<Mutex<Option<Arc<JoinedGroupsProjection>>>>,
) -> Option<DiscoverySession> {
    let (discovery_handle, discovered) = open_nip29_group_discovery_session_with_reader(
        &*app,
        Nip29GroupDiscoverySession::new(vec![relay_url.clone()]),
    );

    let tree_messages = Arc::new(GroupTreeProjection::new());
    let preview = Arc::new(GroupPreviewSessions::new(
        Arc::clone(&app),
        Arc::clone(&tree_messages),
    ));
    let presence = Arc::new(GroupPresenceSessions::new(Arc::clone(&app)));

    let tree_discovered = Arc::clone(&discovered);
    let tree_messages_for_sidecar = Arc::clone(&tree_messages);
    let presence_for_sidecar = Arc::clone(&presence);
    let active_account = app.active_account_handle();
    let joined_reader_for_sidecar = Arc::clone(joined_reader);
    let registration_key = ProjectionKey::app_owned(GROUP_TREE_SCHEMA_ID)
        .expect("29er group-tree projection key must stay app-owned")
        .dynamic_token();
    app.register_typed_snapshot_projection(registration_key, move || {
        let snapshot = tree_discovered.snapshot();
        let messages = tree_messages_for_sidecar.snapshot();
        // Re-read the live active pubkey + joined reader every tick so
        // membership is recomputed on an account switch and never leaks a
        // previous account's truth. Reading `presence_for_sidecar`'s current
        // per-key outputs (`snapshot_state`) is a read, not a reconcile — it
        // does not open/close anything.
        let active_pubkey = active_account
            .lock()
            .ok()
            .and_then(|slot| slot.clone())
            .unwrap_or_default();
        let presence_state = presence_for_sidecar.snapshot_state();
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
            payload: encode_group_tree_snapshot(&snapshot, &messages, &presence_state, &membership),
            ..Default::default()
        })
    });

    Some(DiscoverySession {
        handle: discovery_handle,
        tree_projection: tree_messages,
        preview,
        presence,
        discovered,
        // Placeholder — `GroupSessions::open_discovery` overwrites this with
        // the real id right after this function returns (registering the
        // observer needs `&self.discovery`, which this free function does
        // not have access to). `0` is never a real id
        // (`next_update_frame_observer_id` starts at 1).
        update_frame_observer_id: 0,
    })
}

fn register_group_chat_snapshot(
    app: &NmpApp,
    reader: Arc<GroupEventsProjection>,
    reactions: Arc<ReactionAggregateProjection>,
) {
    let registration_key = ProjectionKey::app_owned(GROUP_CHAT_SCHEMA_ID)
        .expect("29er group-chat projection key must stay app-owned")
        .dynamic_token();
    app.register_typed_snapshot_projection(registration_key, move || {
        let snapshot = reader.snapshot();
        let reaction_snapshot = reactions.snapshot();
        Some(TypedProjectionData {
            key: GROUP_CHAT_SCHEMA_ID.to_string(),
            schema_id: GROUP_CHAT_SCHEMA_ID.to_string(),
            schema_version: GROUP_CHAT_SCHEMA_VERSION,
            file_identifier: String::from_utf8_lossy(GROUP_CHAT_FILE_IDENTIFIER).into_owned(),
            payload: encode_group_chat_snapshot_with_reactions(&snapshot, &reaction_snapshot),
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
    /// canonical discovery + joined-groups doors, plus the 29er group-tree
    /// composite (NMP-owned unread/typing, 29er-owned last-message preview,
    /// and viewer membership), folded into the `"app.29er.group_tree"` typed
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
        self.sessions().open_discovery(self.app_arc(), relay)
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
        if !self
            .sessions()
            .open_discovery(self.app_arc(), relay.clone())
        {
            return false;
        }
        let outcome = crate::dispatch::dispatch_nip29_action(
            self.app(),
            "nmp.nip29.discover",
            &serde_json::json!({ "relay_url": relay }).to_string(),
        );
        outcome.error.is_none()
    }

    /// Mark a group's direct chat messages read inside the open NMP
    /// chat-presence session. `local_id` is the group's bare local id (NOT a
    /// `GroupId` JSON object). The next tree snapshot folds NMP's updated
    /// unread count into the app group tree. No-op when no discovery/presence
    /// session is open (D6).
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
    /// (`nmp_nip29::open_nip29_group_roster_session`). `false` (D6)
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
    use nmp_core::ObservedProjectionSink;

    fn group_id_json() -> String {
        r#"{"host_relay_url":"wss://groups.example.com","local_id":"room"}"#.to_string()
    }

    /// A kind:39000 group-metadata event `DiscoveredGroupsProjection::
    /// on_kernel_event` accepts: a `["d", local_id]` tag, attributed via
    /// `relay_provenance` to a relay the discovery session tracks.
    fn metadata_event(local_id: &str, relay: &str) -> nmp_core::substrate::KernelEvent {
        nmp_core::substrate::KernelEvent {
            id: format!("meta-{local_id}"),
            author: "author-pubkey".to_string(),
            kind: nmp_nip29::kinds::KIND_GROUP_METADATA,
            created_at: 100,
            tags: vec![vec!["d".to_string(), local_id.to_string()]],
            content: String::new(),
            relay_provenance: vec![relay.to_string()],
        }
    }

    // Regression for the exact gap flagged in #63: a group discovered LIVE
    // (after `open_group_discovery`, with no identity change involved) must
    // still get a preview/presence row — not just groups known at open time.
    // The update-frame observer (`reconcile_group_tree_sessions`, registered
    // in `GroupSessions::open_discovery`) is what NMP's own
    // `register_update_frame_observer` test suite proves fires on every
    // emitted frame; this test proves OUR registered callback body correctly
    // reconciles the live `discovered` snapshot when invoked, which is
    // exactly what that callback does on each firing.
    #[test]
    fn newly_discovered_group_reconciles_via_the_update_frame_driver_without_identity_change() {
        let app = TwentyNinerApp::new();
        assert!(app.open_group_discovery("wss://groups.example.com".to_string()));

        // No signer/identity ever added in this test — proves the pickup is
        // independent of any identity-change trigger.
        {
            let guard = app.sessions().discovery.lock().unwrap();
            let session = guard.as_ref().expect("discovery session is open");
            assert_eq!(session.preview.live_count_for_test(), 0);
            assert_eq!(session.presence.live_count_for_test(), 0);

            // Simulate a group discovered live, streamed in after open —
            // directly on the shared reader, exactly as the actor would
            // fold a real relay-signed kind:39000 event in.
            session
                .discovered
                .on_kernel_event(&metadata_event("new-room", "wss://groups.example.com"));
        }

        // What the update-frame observer's callback runs on every emitted
        // frame (module docs) — re-derive from the CURRENT discovered
        // snapshot, no open-time/identity-change trigger involved.
        {
            let guard = app.sessions().discovery.lock().unwrap();
            let session = guard.as_ref().expect("discovery session is open");
            reconcile_group_tree_sessions(app.app(), session);
        }

        let guard = app.sessions().discovery.lock().unwrap();
        let session = guard.as_ref().expect("discovery session is open");
        assert_eq!(
            session.preview.live_count_for_test(),
            1,
            "the live-discovered group must mount a preview source"
        );
        // `presence.sync` closes fail-safe on an empty active_pubkey (no
        // signer was added), matching production behavior — the preview
        // side (which has no such account gate) is the meaningful
        // assertion here.
        assert_eq!(session.presence.live_count_for_test(), 0);
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
        // Only ONE "app.29er.group_tree" registration should be live — the
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
