//! NMP composition root + projection-backed view-model for the TUI.
//!
//! Ported off the deleted `nmp-ffi` C-ABI (#2483) onto `nmp-native-runtime`
//! directly. The TUI is a plain Rust consumer with no Swift/UniFFI boundary,
//! so it composes 29er via [`nmp_app_29er::compose_29er_runtime`] (the same
//! composition root the `TwentyNinerApp` UniFFI facade uses) and talks to
//! `nmp_native_runtime::NmpApp`'s NIP-29 session methods directly, instead of
//! going through the facade object.
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use nmp_app_29er::group_tree::{GroupTreeMessageState, GroupTreeProjection};
use nmp_app_29er::kinds::{KIND_CHAT_MESSAGE, KIND_DISCUSSION_OR_ARTIFACT};
use nmp_app_29er::{compose_29er_runtime, dispatch_nip29_action, DispatchOutcome};
use nmp_core::refs::{RefProfileStore, REFS_PROFILE_KEY};
use nmp_core::substrate::{ObservedProjection, ObservedProjectionRegistrar};
use nmp_core::{ObservedProjectionId, ObservedProjectionSink};
use nmp_native_runtime::{
    new_app, Nip29GroupDiscoveryHandle, Nip29GroupDiscoverySession, Nip29GroupEventsHandle,
    Nip29GroupEventsSession, Nip29JoinedGroupsHandle, Nip29JoinedGroupsSession, NmpApp,
};
use nmp_nip29::projection::{
    DiscoveredGroup, DiscoveredGroupsProjection, DiscoveredGroupsSnapshot,
    GroupEvent as GroupChatMessage, GroupEventsProjection, JoinedGroupsProjection,
};
use nmp_nip29::GroupId;
use tokio::sync::watch;

type ActiveAccountSlot = Arc<Mutex<Option<String>>>;

/// A single member row rendered by the composer `@mention` popup and the
/// members panel.
///
/// v0.8.0 removed `nmp_nip29::projection::GroupMembersProjection` /
/// `GroupMemberRow` (NIP-29 no longer surfaces a per-member roster; membership
/// now derives from [`JoinedGroupsProjection`] and `DiscoveredGroup` counts).
/// 29er keeps this view-model shape locally so the roster UI compiles and is
/// ready to be repopulated once a custom roster observer lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupMemberRow {
    pub pubkey: String,
    pub display_name: Option<String>,
    pub admin: bool,
    pub role: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Login,
    App,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    RoomList, // primary sidebar (was ChannelList)
    Chat,
    Composer,
    Palette, // command palette overlay
    Modal,   // form/dialog overlay
}
impl Focus {
    /// Forward Tab cycle: only cycles through the three base panels.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Focus::RoomList => Focus::Chat,
            Focus::Chat => Focus::Composer,
            Focus::Composer => Focus::RoomList,
            other => other, // Palette/Modal don't participate
        }
    }
    /// Reverse Shift+Tab cycle.
    #[must_use]
    pub fn prev(self) -> Self {
        match self {
            Focus::RoomList => Focus::Composer,
            Focus::Chat => Focus::RoomList,
            Focus::Composer => Focus::Chat,
            other => other,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IdentityState {
    LoggedOut,
    LoggingIn,
    LoggedIn { npub: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelayState {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutboxStatus {
    Pending,
    Confirmed,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormKind {
    JoinWithCode(GroupId),
    CreateInvite(GroupId),
    CreateChild(GroupId),
    PutUser(GroupId),
    MoveChannel(GroupId),
}

/// Priority tier for a channel in the hotlist / room-list sidebar.
/// Determines badge style and sort weight.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ChannelTier {
    /// Unread message that contains `@<my_pubkey>`.
    Mention,
    /// At least one unread message (no mention).
    Unread,
    /// No unread, but last message within the past hour.
    Activity,
    /// No recent activity.
    Normal,
}

#[derive(Clone, Debug)]
pub struct ChannelListItem {
    pub group_id: GroupId,
    pub local_id: String,
    pub name: String,
    pub depth: usize,
    pub unread: u32,
    pub member_count: u32,
    pub admin_count: u32,
    pub is_branch: bool,
    pub last_preview: Option<String>,
    pub last_timestamp: Option<u64>,
    /// Computed hotlist tier: drives badge rendering and Alt+A cycling.
    pub tier: ChannelTier,
}

#[derive(Clone, Debug)]
pub struct PublishOutboxItem {
    pub correlation_id: String,
    pub group_local_id: String,
    pub content: String,
    pub status: OutboxStatus,
    pub error: Option<String>,
    /// Pubkeys of @mentioned users (hex-encoded). Stored so retry can re-send
    /// the same `p` tags without re-parsing the message text.
    pub mention_pubkeys: Vec<String>,
    /// Nostr event-id echoed back by NMP after a successful dispatch.
    /// When present, confirmation is matched on this id alone (precise match).
    pub event_id: Option<String>,
    /// Wall-clock instant the item was dispatched. Used to enforce
    /// a 30-second content-match window and a 60-second timeout.
    pub dispatched_at: std::time::Instant,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TuiProfile {
    pub pubkey: String,
    pub display_name: Option<String>,
    pub npub: Option<String>,
    pub picture_url: Option<String>,
}

/// The immutable per-frame view-model. Contains ZERO Ratatui types (issue #3).
#[derive(Clone, Debug)]
pub struct TuiSnapshot {
    pub channel_tree: Vec<ChannelListItem>,
    pub selected_channel_id: Option<GroupId>,
    pub selected_messages: Vec<GroupChatMessage>,
    pub selected_members: Vec<GroupMemberRow>,
    pub profiles: HashMap<String, TuiProfile>,
    pub is_admin: bool,
    pub my_pubkey: Option<String>,
    pub publish_outbox: Vec<PublishOutboxItem>,
    pub identity_state: IdentityState,
    pub relay_state: RelayState,
    pub errors: Vec<String>,
    pub selected_index: usize,
    pub focus: Focus,
    pub message_scroll: u16,
    pub palette_open: bool,
    pub active_form: Option<FormKind>,
    pub login_error: Option<String>,
    pub screen: Screen,
    pub help_open: bool,
    /// Transient command-acknowledgment line (e.g. "Joining room…" → "Joined ✓").
    /// `None` when nothing to show; cleared after 2 seconds.
    pub status_message: Option<String>,
    /// The id of the last message the user read in the selected channel.
    /// Used to render the "── You've read to here ──" separator in chat.
    pub last_read_message_id: Option<String>,
    /// Monotonically incrementing tick counter; drives the spinner frame selector.
    pub spinner_tick: u64,
    /// When the NMP relay connection attempt started (set at login / reconnect).
    pub connecting_since: Option<std::time::Instant>,
    /// When we first observed `relay_connected = true` (used for the 2-second flash).
    pub connected_at: Option<std::time::Instant>,
}

/// Projection-derived fields delivered through the pushed update channel.
#[derive(Clone, Debug, Default)]
pub struct ProjectionView {
    pub channel_tree: Vec<ChannelListItem>,
    pub selected_messages: Vec<GroupChatMessage>,
    pub selected_members: Vec<GroupMemberRow>,
    pub profiles: HashMap<String, TuiProfile>,
    pub is_admin: bool,
    pub my_pubkey: Option<String>,
    pub identity_state: IdentityState,
    /// Last time any snapshot data was observed; used for heartbeat relay-state inference.
    pub last_data_at: Option<Instant>,
}
impl Default for IdentityState {
    fn default() -> Self {
        IdentityState::LoggedOut
    }
}

/// Bridges NMP update-frame pushes into the TUI's latest projection channel.
///
/// The update listener is the explicit Rust->shell reactivity seam: whenever
/// NMP emits an update frame, the TUI re-reads its Rust-owned projection readers
/// once and replaces the latest UI projection. There is no timer-driven refresh.
struct ProjectionUpdateBridge {
    shared: Arc<SharedProjections>,
    projection_tx: watch::Sender<ProjectionView>,
}

/// Send+Sync read-side state shared between App and the NMP update listener.
pub struct SharedProjections {
    pub group_tree: Arc<GroupTreeProjection>,
    pub discovered: Mutex<Option<Arc<DiscoveredGroupsProjection>>>,
    /// Active-account joined/admin state (v0.8.2 replacement for the removed
    /// `GroupMembersProjection`). Wired lazily once the active pubkey resolves
    /// (the projection captures the pubkey at construction).
    pub joined: Mutex<Option<Arc<JoinedGroupsProjection>>>,
    /// The handle for the currently-open joined-groups session, opened (and
    /// replaced) by the identity-change observer in [`App::init_nmp`]. Tracked
    /// here (rather than on `App`) because it is written from the observer
    /// closure, which runs on the actor thread.
    pub joined_handle: Mutex<Option<Nip29JoinedGroupsHandle>>,
    pub active_account: Mutex<ActiveAccountSlot>,
    pub selected_chat: Mutex<Option<Arc<GroupEventsProjection>>>,
    pub selected_group: Mutex<Option<GroupId>>,
    pub profile_refs: Mutex<RefProfileStore>,
    /// Set on each pushed snapshot that returns non-empty data; drives relay-state indicator.
    pub last_update_at: Mutex<Option<std::time::Instant>>,
}
impl SharedProjections {
    pub fn project(&self) -> ProjectionView {
        let discovered = self
            .discovered
            .lock()
            .ok()
            .and_then(|slot| slot.as_ref().map(|projection| projection.snapshot()))
            .unwrap_or_default();
        let tree_state = self.group_tree.snapshot();
        // Resolve my_pubkey first so it can be passed to tier computation.
        let me = self
            .active_account
            .lock()
            .ok()
            .and_then(|s| s.lock().ok().and_then(|v| v.clone()));
        let channel_tree = derive_channel_tree(&discovered, &tree_state, me.as_deref());
        let selected_messages = self
            .selected_chat
            .lock()
            .ok()
            .and_then(|c| c.as_ref().map(|c| c.snapshot().events))
            .unwrap_or_default();
        // The per-member roster API was removed in v0.8.0; no source yet.
        let members: Vec<GroupMemberRow> = Vec::new();
        // Derive admin status for the selected group from the joined-groups
        // projection (relay-signed 39002), the canonical v0.8.0 source.
        let selected_local = self
            .selected_group
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|g| g.local_id.clone()));
        let is_admin = match (
            selected_local,
            self.joined.lock().ok().and_then(|j| j.clone()),
        ) {
            (Some(local), Some(joined)) => joined
                .snapshot()
                .groups
                .iter()
                .any(|g| g.group_id == local && g.is_admin),
            _ => false,
        };
        let identity_state = match &me {
            Some(pk) => IdentityState::LoggedIn {
                npub: nmp_core::display::to_npub(pk),
            },
            None => IdentityState::LoggingIn,
        };
        let has_data = !channel_tree.is_empty() || !selected_messages.is_empty();
        if has_data {
            if let Ok(mut ts) = self.last_update_at.lock() {
                *ts = Some(std::time::Instant::now());
            }
        }
        let profiles = self
            .profile_refs
            .lock()
            .ok()
            .map(|store| {
                store
                    .profiles()
                    .into_iter()
                    .map(|(pubkey, card)| (pubkey.clone(), TuiProfile::from_card(pubkey, card)))
                    .collect()
            })
            .unwrap_or_default();
        let last_data_at = self.last_update_at.lock().ok().and_then(|ts| *ts);
        ProjectionView {
            channel_tree,
            selected_messages,
            selected_members: members,
            profiles,
            is_admin,
            my_pubkey: me,
            identity_state,
            last_data_at,
        }
    }
}

pub struct App {
    /// `None` until [`Self::init_nmp`] succeeds. `Arc`-shared (not the old
    /// `*mut NmpApp`) so the identity-change observer closure — which runs on
    /// the actor thread and is `'static` — can hold its own safe clone instead
    /// of recovering a raw pointer's address.
    app: Option<Arc<NmpApp>>,
    relay_url: String,
    shared: Arc<SharedProjections>,
    projection_tx: watch::Sender<ProjectionView>,
    /// The currently-open group-events (chat) session. Singleton: opening a
    /// new one (in [`Self::select_channel`]) replaces this at the kernel
    /// level, so there is no per-group cache to maintain here.
    group_events_handle: Option<Nip29GroupEventsHandle>,
    discovery_handle: Option<Nip29GroupDiscoveryHandle>,
    /// Pubkeys of authors of currently-visible chat messages whose profile
    /// ref is currently claimed (resolved) via [`Self::resolve_profile_ref`].
    /// Diffed against the visible-author set on every projection ingest
    /// ([`Self::sync_visible_profile_refs`]) to claim newly-visible authors
    /// and release no-longer-visible ones.
    claimed_profile_authors: HashSet<String>,
    group_tree_observer_id: Option<ObservedProjectionId>,
    latest: ProjectionView,
    screen: Screen,
    focus: Focus,
    /// Focus history stack — push on modal/palette open, pop on Esc.
    focus_stack: Vec<Focus>,
    selected_index: usize,
    selected_channel: Option<GroupId>,
    outbox: Vec<PublishOutboxItem>,
    errors: Vec<String>,
    palette_open: bool,
    active_form: Option<FormKind>,
    message_scroll: u16,
    login_error: Option<String>,
    help_open: bool,
    should_quit: bool,
    /// Transient status line: (message, when_set). Cleared by `tick()` after 2s.
    status_message: Option<(String, Instant)>,
    /// Per-channel read marker: local_id → last-seen message id.
    /// Frozen when the user switches away from a channel so the separator
    /// shows on next visit for messages that arrived in the interim.
    read_markers: HashMap<String, String>,
    spinner_tick: u64,
    connecting_since: Option<std::time::Instant>,
    connected_at: Option<std::time::Instant>,
    relay_error: Option<String>,
}

impl App {
    pub fn new(relay_url: impl Into<String>, projection_tx: watch::Sender<ProjectionView>) -> Self {
        let relay_url = relay_url.into();
        let shared = Arc::new(SharedProjections {
            group_tree: Arc::new(GroupTreeProjection::new()),
            discovered: Mutex::new(None),
            joined: Mutex::new(None),
            joined_handle: Mutex::new(None),
            active_account: Mutex::new(Arc::new(Mutex::new(None))),
            selected_chat: Mutex::new(None),
            selected_group: Mutex::new(None),
            profile_refs: Mutex::new(RefProfileStore::new()),
            last_update_at: Mutex::new(None),
        });
        Self {
            app: None,
            relay_url,
            shared,
            projection_tx,
            group_events_handle: None,
            latest: ProjectionView::default(),
            discovery_handle: None,
            claimed_profile_authors: HashSet::new(),
            group_tree_observer_id: None,
            screen: Screen::Login,
            focus: Focus::RoomList,
            focus_stack: Vec::new(),
            selected_index: 0,
            selected_channel: None,
            outbox: Vec::new(),
            errors: Vec::new(),
            palette_open: false,
            active_form: None,
            message_scroll: 0,
            login_error: None,
            help_open: false,
            should_quit: false,
            status_message: None,
            read_markers: HashMap::new(),
            spinner_tick: 0,
            connecting_since: None,
            connected_at: None,
            relay_error: None,
        }
    }

    /// Validate + hand the nsec straight to NMP, never storing it (issue #10).
    /// `relay` overrides the relay URL configured at startup (collected in Step 2).
    pub fn login(&mut self, nsec: String, relay: String) {
        let nsec = nsec.trim().to_string();
        if !nsec.starts_with("nsec1") {
            self.login_error = Some("Secret key must start with nsec1\u{2026}".to_string());
            return;
        }
        if !relay.is_empty() {
            self.relay_url = relay;
        }
        match self.init_nmp(&nsec) {
            Ok(()) => {
                self.screen = Screen::App;
                self.focus = Focus::RoomList;
                self.focus_stack.clear();
                self.login_error = None;
                self.connecting_since = Some(std::time::Instant::now());
                self.connected_at = None;
                self.relay_error = None;
            }
            Err(e) => {
                self.login_error = Some(format!("Sign-in failed: {e}"));
            }
        }
        // `nsec` is dropped here.
    }

    fn init_nmp(&mut self, nsec: &str) -> anyhow::Result<()> {
        let relay = self.relay_url.clone();
        let storage = std::env::temp_dir().join("29er-tui-store");
        std::fs::create_dir_all(&storage).ok();
        let storage_str = storage.to_string_lossy().into_owned();

        let mut app = new_app();
        if !matches!(
            app.set_storage_path(Some(storage_str)),
            nmp_native_runtime::NmpConfigStatus::Ok
        ) {
            anyhow::bail!("set_storage_path failed");
        }
        compose_29er_runtime(&mut app);
        app.consume_all_builtin_projections();

        let (discovery_handle, discovered) = app.open_nip29_group_discovery_session_with_reader(
            Nip29GroupDiscoverySession::new(relay.clone()),
        );
        if let Ok(mut slot) = self.shared.discovered.lock() {
            *slot = Some(discovered);
        }

        let mut tree_shape = nmp_planner::InterestShape::from_filter_json(&format!(
            r#"{{"kinds":[{KIND_CHAT_MESSAGE}]}}"#
        ))
        .ok_or_else(|| anyhow::anyhow!("failed to build group-tree interest shape"))?;
        tree_shape.relay_pin = Some(relay.clone());
        let tree_observer_id = app.open_observed_projection(ObservedProjection::from_shape(
            Arc::clone(&self.shared.group_tree) as Arc<dyn ObservedProjectionSink>,
            format!("29er.tui.group_tree.kind9:{relay}"),
            1,
            tree_shape,
            80,
        ));
        if tree_observer_id.0 == 0 {
            app.close_nip29_group_discovery_session(discovery_handle);
            anyhow::bail!("failed to open group-tree observer");
        }

        // `nmp-native-runtime` owns the app by value now (no `*mut NmpApp`), so
        // it is wrapped in an `Arc` here purely so the `'static` identity-change
        // observer closure below (which runs on the actor thread) can hold a
        // safe, owned clone instead of recovering a raw pointer's address.
        let app = Arc::new(app);
        if let Ok(mut slot) = self.shared.active_account.lock() {
            *slot = app.active_account_handle();
        }

        // The `JoinedGroupsProjection` captures the active pubkey at
        // construction, which is not known until sign-in resolves. Wire it
        // from an identity-change observer (registered BEFORE sign-in so we
        // never miss the first frame). Best-effort (D6): a poisoned slot or
        // an already-wired projection makes this a no-op. The joined-groups
        // session is itself a singleton (re-opening replaces the prior one at
        // the kernel level), so each fresh open here simply tracks the latest
        // handle for final teardown.
        let joined_app = Arc::clone(&app);
        let joined_shared = Arc::clone(&self.shared);
        let joined_relay = relay.clone();
        let joined_projection_tx = self.projection_tx.clone();
        app.register_identity_change_observer(move |pubkey| {
            let Some(pk) = pubkey.filter(|p| !p.is_empty()) else {
                return;
            };
            let Ok(mut slot) = joined_shared.joined.lock() else {
                return;
            };
            if slot
                .as_ref()
                .map(|projection| projection.snapshot().active_pubkey == pk)
                .unwrap_or(false)
            {
                return;
            }
            let opened = joined_app.open_nip29_joined_groups_session_with_reader(
                Nip29JoinedGroupsSession::new(pk, joined_relay.clone()),
            );
            let (handle, reader) = match opened {
                Some((handle, reader)) => (Some(handle), Some(reader)),
                None => (None, None),
            };
            if let Ok(mut handle_slot) = joined_shared.joined_handle.lock() {
                *handle_slot = handle;
            }
            *slot = reader;
            let _ = joined_projection_tx.send(joined_shared.project());
        });

        // Bridge every NMP update frame into the latest projection channel.
        // The same callback also applies `refs.profile` sidecars, replacing the
        // deleted `nmp-ffi` C-ABI update callback without reintroducing polling.
        // Ownership of the boxed sink moves into the runtime's update-listener
        // slot; `App` does not need to hold it.
        let update_bridge = Box::new(ProjectionUpdateBridge {
            shared: Arc::clone(&self.shared),
            projection_tx: self.projection_tx.clone(),
        });
        nmp_uniffi_support::set_update_sink(&app, Some(update_bridge), on_nmp_update);

        app.add_relay(relay.clone(), "read".to_string());
        nmp_uniffi_support::start_runtime(&app, 80, 4);
        app.add_signer(
            nmp_core::SignerSource::LocalNsec(zeroize::Zeroizing::new(nsec.to_string())),
            true,
        );
        self.app = Some(app);
        self.discovery_handle = Some(discovery_handle);
        self.group_tree_observer_id = Some(tree_observer_id);

        let discover_body = serde_json::json!({ "relay_url": relay }).to_string();
        self.dispatch_json("nmp.nip29.discover", &discover_body);
        self.refresh_projection();
        Ok(())
    }

    /// Store the latest projection view; clamp selection; reconcile the outbox.
    pub fn ingest_projection(&mut self, view: ProjectionView) {
        let was_connected = self
            .latest
            .last_data_at
            .map(|t| t.elapsed() < Duration::from_secs(30))
            .unwrap_or(false);
        self.latest = view;
        self.sync_visible_profile_refs();
        // Record the first moment we see fresh relay data (drives the 2s Connected flash).
        let is_connected_now = self
            .latest
            .last_data_at
            .map(|t| t.elapsed() < Duration::from_secs(30))
            .unwrap_or(false);
        if !was_connected && is_connected_now && self.connected_at.is_none() {
            self.connected_at = Some(std::time::Instant::now());
        }
        if self.latest.channel_tree.is_empty() {
            self.selected_index = 0;
        } else if self.selected_index >= self.latest.channel_tree.len() {
            self.selected_index = self.latest.channel_tree.len() - 1;
        }
        self.reconcile_outbox();
    }

    fn reconcile_outbox(&mut self) {
        // Guard: never confirm outbox items if my_pubkey is not yet known.
        let me = match self.latest.my_pubkey.as_deref() {
            Some(pk) if !pk.is_empty() => pk.to_string(),
            _ => return,
        };
        let now = Instant::now();
        for item in self.outbox.iter_mut() {
            if !matches!(item.status, OutboxStatus::Pending) {
                continue;
            }
            let elapsed = now.duration_since(item.dispatched_at);
            // Items pending for more than 60 s are timed out (treated as Failed).
            if elapsed >= Duration::from_secs(60) {
                item.status = OutboxStatus::Failed;
                item.error = Some("timed out waiting for confirmation".to_string());
                continue;
            }
            // Only attempt matching within the 30-second dispatch window.
            if elapsed >= Duration::from_secs(30) {
                continue;
            }
            let confirmed = self.latest.selected_messages.iter().any(|m| {
                if m.pubkey != me {
                    return false;
                }
                // Prefer precise event-id match when NMP returned one.
                if let Some(ref eid) = item.event_id {
                    return m.id == *eid;
                }
                // Fallback: trimmed content equality (same pubkey already verified).
                m.content.trim() == item.content.trim()
            });
            if confirmed {
                item.status = OutboxStatus::Confirmed;
            }
        }
    }

    pub fn snapshot(&self) -> TuiSnapshot {
        TuiSnapshot {
            channel_tree: self.latest.channel_tree.clone(),
            selected_channel_id: self.selected_channel.clone(),
            selected_messages: self.latest.selected_messages.clone(),
            selected_members: self.latest.selected_members.clone(),
            profiles: self.latest.profiles.clone(),
            is_admin: self.latest.is_admin,
            my_pubkey: self.latest.my_pubkey.clone(),
            publish_outbox: self.outbox.clone(),
            identity_state: self.latest.identity_state.clone(),
            relay_state: if let Some(ref err) = self.relay_error {
                RelayState::Error(err.clone())
            } else if self.app.is_none() {
                RelayState::Disconnected
            } else {
                // Heartbeat: Connected if last data arrived within 30 s; otherwise
                // Connecting (covers initial, unstable-30-120 s, and reconnecting->120 s).
                match self.latest.last_data_at {
                    Some(t) if t.elapsed() < Duration::from_secs(30) => RelayState::Connected,
                    _ => RelayState::Connecting,
                }
            },
            errors: self.errors.clone(),
            selected_index: self.selected_index,
            focus: self.focus,
            message_scroll: self.message_scroll,
            palette_open: self.palette_open,
            active_form: self.active_form.clone(),
            login_error: self.login_error.clone(),
            screen: self.screen,
            help_open: self.help_open,
            status_message: self.status_message.as_ref().map(|(msg, _)| msg.clone()),
            last_read_message_id: self
                .selected_channel
                .as_ref()
                .and_then(|ch| self.read_markers.get(&ch.local_id).cloned()),
            spinner_tick: self.spinner_tick,
            connecting_since: self.connecting_since,
            connected_at: self.connected_at,
        }
    }

    fn sync_visible_profile_refs(&mut self) {
        if self.app.is_none() {
            return;
        }
        let visible: HashSet<String> = self
            .latest
            .selected_messages
            .iter()
            .map(|message| message.pubkey.clone())
            .filter(|pubkey| !pubkey.is_empty())
            .collect();
        for pubkey in visible.difference(&self.claimed_profile_authors) {
            self.resolve_profile_ref(pubkey);
        }
        for pubkey in self.claimed_profile_authors.difference(&visible) {
            self.release_profile_ref(pubkey);
        }
        self.claimed_profile_authors = visible;
    }

    /// Mirrors the deleted `nmp_ffi::nmp_app_resolve_profile_ref` typed
    /// adapter: namespace=Profile, shape=Ref, liveness=CacheOk — the
    /// feed-avatar shape, suitable for a chat-message author byline.
    fn resolve_profile_ref(&self, pubkey: &str) {
        let Some(app) = self.app.as_ref() else {
            return;
        };
        app.resolve_ref(
            nmp_core::RefNamespace::Profile,
            pubkey.to_string(),
            "29er-tui.chat-author".to_string(),
            nmp_core::RefShape::Profile(nmp_core::ProfileShape::Ref),
            nmp_core::RefLiveness::CacheOk,
        );
    }

    /// Mirrors the deleted `nmp_ffi::nmp_app_release_profile_ref`. Idempotent.
    fn release_profile_ref(&self, pubkey: &str) {
        let Some(app) = self.app.as_ref() else {
            return;
        };
        app.release_ref(
            nmp_core::RefNamespace::Profile,
            pubkey.to_string(),
            "29er-tui.chat-author".to_string(),
        );
    }

    pub fn select_channel(&mut self, group: GroupId) {
        // Freeze the read marker for the channel we are leaving so that messages
        // arriving while the user is away will appear after the separator on return.
        if let Some(old_ch) = &self.selected_channel {
            if let Some(last_msg) = self.latest.selected_messages.first() {
                self.read_markers
                    .insert(old_ch.local_id.clone(), last_msg.id.clone());
            }
        }
        // The group-events session is a kernel-level singleton: opening a new
        // one replaces (and closes) the prior one, so there is no per-group
        // cache to maintain — just track the latest handle for teardown.
        if let Some(app) = &self.app {
            let (handle, reader) =
                app.open_nip29_group_events_session_with_reader(Nip29GroupEventsSession::new(
                    group.clone(),
                    vec![KIND_CHAT_MESSAGE, KIND_DISCUSSION_OR_ARTIFACT],
                ));
            self.group_events_handle = Some(handle);
            if let Ok(mut slot) = self.shared.selected_chat.lock() {
                *slot = Some(reader);
            }
        }
        if let Ok(mut g) = self.shared.selected_group.lock() {
            *g = Some(group.clone());
        }
        self.shared.group_tree.mark_read(&group.local_id);
        self.selected_channel = Some(group);
        self.message_scroll = 0;
        self.focus = Focus::Chat;
        // Explicitly pull the projection for the newly selected channel so
        // the first frame shows whatever messages are already cached.
        self.refresh_projection();
    }

    pub fn send_message(&mut self, body: String, mention_pubkeys: Vec<String>) {
        let trimmed = body.trim().to_string();
        if trimmed.is_empty() {
            return;
        }
        let Some(group) = self.selected_channel.clone() else {
            self.errors.push("No channel selected".to_string());
            return;
        };
        // Assign an optimistic local id immediately so the item is identifiable
        // before NMP echoes back a correlation_id.
        let optimistic_id = generate_local_id();
        let json = serde_json::json!({
            "group": group,
            "content": trimmed,
            "mention_pubkeys": mention_pubkeys,
        })
        .to_string();
        let result = self.dispatch_json("nmp.nip29.post_chat_message", &json);
        let mut item = PublishOutboxItem {
            correlation_id: optimistic_id,
            group_local_id: group.local_id.clone(),
            content: trimmed,
            status: OutboxStatus::Pending,
            error: None,
            mention_pubkeys: mention_pubkeys.clone(),
            event_id: None,
            dispatched_at: Instant::now(),
        };
        Self::apply_dispatch_result(&mut item, result);
        self.outbox.push(item);
        // Explicitly pull fresh projection data so the outbox strip renders
        // before the runtime echoes the published event back.
        self.refresh_projection();
    }

    pub fn retry_outbox(&mut self, correlation_id: String) {
        let Some(idx) = self
            .outbox
            .iter()
            .position(|i| i.correlation_id == correlation_id)
        else {
            return;
        };
        let (content, local_id, mention_pubkeys) = {
            let it = &self.outbox[idx];
            (
                it.content.clone(),
                it.group_local_id.clone(),
                it.mention_pubkeys.clone(),
            )
        };
        let Some(group) = self
            .selected_channel
            .clone()
            .filter(|g| g.local_id == local_id)
        else {
            return;
        };
        let json = serde_json::json!({
            "group": group,
            "content": content,
            "mention_pubkeys": mention_pubkeys,
        })
        .to_string();
        let result = self.dispatch_json("nmp.nip29.post_chat_message", &json);
        let item = &mut self.outbox[idx];
        item.status = OutboxStatus::Pending;
        item.error = None;
        item.event_id = None;
        item.dispatched_at = Instant::now();
        Self::apply_dispatch_result(item, result);
    }

    /// Note: [`DispatchOutcome`] (the same outcome shape PR-3 will wire onto
    /// the `TwentyNinerApp` facade) carries `correlation_id` /
    /// `error` / `code` only — no `event_id`. Outbox confirmation therefore
    /// always falls back to the existing trimmed-content match in
    /// [`Self::reconcile_outbox`] rather than the old precise event-id match;
    /// a future PR can thread an event-id back through if NMP's dispatch
    /// outcome grows one.
    fn apply_dispatch_result(item: &mut PublishOutboxItem, result: Option<DispatchOutcome>) {
        match result {
            Some(outcome) => {
                if let Some(cid) = outcome.correlation_id {
                    item.correlation_id = cid;
                } else if let Some(err) = outcome.error {
                    item.status = OutboxStatus::Failed;
                    item.error = Some(err);
                }
            }
            None => {
                item.status = OutboxStatus::Failed;
                item.error = Some("dispatch failed".to_string());
            }
        }
    }

    pub fn join(&mut self, group: GroupId, invite_code: Option<String>) {
        let body = serde_json::json!({ "group": group, "invite_code": invite_code }).to_string();
        self.set_status_message("Joining room\u{2026}".to_string());
        self.dispatch_json("nmp.nip29.join", &body);
        self.close_form();
    }
    pub fn leave(&mut self, group: GroupId) {
        let body = serde_json::json!({ "group": group }).to_string();
        self.set_status_message("Leaving room\u{2026}".to_string());
        self.dispatch_json("nmp.nip29.leave", &body);
    }
    pub fn create_invite(&mut self, group: GroupId, codes: Vec<String>) {
        let body = serde_json::json!({ "group": group, "codes": codes }).to_string();
        self.set_status_message("Creating invite\u{2026}".to_string());
        self.dispatch_json("nmp.nip29.create_invite", &body);
        self.close_form();
    }
    pub fn put_user(&mut self, group: GroupId, target_pubkey: String, role: Option<String>) {
        let body =
            serde_json::json!({ "group": group, "target_pubkey": target_pubkey, "role": role })
                .to_string();
        self.set_status_message("Updating member\u{2026}".to_string());
        self.dispatch_json("nmp.nip29.put_user", &body);
        self.close_form();
    }
    pub fn create_child(&mut self, parent: GroupId, name: String) {
        let local_id = generate_local_id();
        let body = serde_json::json!({
            "group": { "host_relay_url": parent.host_relay_url, "local_id": local_id },
            "name": name, "visibility": "public", "access": "open", "parent": parent.local_id,
        })
        .to_string();
        self.set_status_message("Creating channel\u{2026}".to_string());
        self.dispatch_json("nmp.nip29.create_public_group", &body);
        self.close_form();
    }
    pub fn move_channel(&mut self, group: GroupId, parent: Option<String>) {
        let body = serde_json::json!({ "group": group, "parent": parent }).to_string();
        self.set_status_message("Moving channel\u{2026}".to_string());
        self.dispatch_json("nmp.nip29.set_parent", &body);
        self.close_form();
    }
    pub fn show_members(&mut self, group: GroupId) {
        let _ = group;
    }

    fn dispatch_json(&mut self, namespace: &str, body: &str) -> Option<DispatchOutcome> {
        let app = self.app.as_ref()?;
        Some(dispatch_nip29_action(app, namespace, body))
    }

    /// Alt+A: cycle to the next channel that has a Mention-tier unread notification.
    /// If a mention channel is found it is immediately selected.
    pub fn jump_to_next_mention(&mut self) {
        let len = self.latest.channel_tree.len();
        if len == 0 {
            return;
        }
        for i in 1..=len {
            let idx = (self.selected_index + i) % len;
            if self.latest.channel_tree[idx].tier == ChannelTier::Mention {
                self.selected_index = idx;
                let group = self.latest.channel_tree[idx].group_id.clone();
                self.select_channel(group);
                return;
            }
        }
    }

    pub fn navigate(&mut self, delta: isize) {
        let len = self.latest.channel_tree.len();
        if len == 0 {
            return;
        }
        self.selected_index =
            (self.selected_index as isize + delta).rem_euclid(len as isize) as usize;
    }
    pub fn navigate_top(&mut self) {
        if !self.latest.channel_tree.is_empty() {
            self.selected_index = 0;
        }
    }
    pub fn navigate_bottom(&mut self) {
        let len = self.latest.channel_tree.len();
        if len > 0 {
            self.selected_index = len - 1;
        }
    }
    pub fn channel_at_cursor(&self) -> Option<GroupId> {
        self.latest
            .channel_tree
            .get(self.selected_index)
            .map(|c| c.group_id.clone())
    }
    pub fn scroll_messages(&mut self, delta: i32) {
        let n = self.message_scroll as i32 + delta;
        self.message_scroll = n.max(0) as u16;
    }
    pub fn focus(&self) -> Focus {
        self.focus
    }
    pub fn set_focus(&mut self, f: Focus) {
        self.focus = f;
    }
    /// Forward Tab cycle (RoomList → Chat → Composer → RoomList). No-op in Palette/Modal.
    pub fn cycle_focus(&mut self) {
        if matches!(self.focus, Focus::RoomList | Focus::Chat | Focus::Composer) {
            self.focus = self.focus.next();
        }
    }
    /// Reverse Shift+Tab cycle. No-op in Palette/Modal.
    pub fn reverse_cycle_focus(&mut self) {
        if matches!(self.focus, Focus::RoomList | Focus::Chat | Focus::Composer) {
            self.focus = self.focus.prev();
        }
    }
    /// Push current focus onto the stack (e.g. before opening palette or modal).
    pub fn push_focus(&mut self) {
        self.focus_stack.push(self.focus);
    }
    /// Pop previous focus from the stack. Returns `true` if something was restored.
    pub fn pop_focus(&mut self) -> bool {
        if let Some(f) = self.focus_stack.pop() {
            self.focus = f;
            true
        } else {
            false
        }
    }
    pub fn screen(&self) -> Screen {
        self.screen
    }
    /// Open the command palette, pushing current focus onto the stack.
    pub fn set_palette(&mut self, open: bool) {
        if open && self.focus != Focus::Palette {
            self.focus_stack.push(self.focus);
            self.focus = Focus::Palette;
            self.palette_open = true;
        } else if !open && self.palette_open {
            self.palette_open = false;
            if self.focus == Focus::Palette {
                self.focus = self.focus_stack.pop().unwrap_or(Focus::RoomList);
            }
        }
    }
    pub fn palette_open(&self) -> bool {
        self.palette_open
    }
    /// Open a form, collapsing any open palette and pushing focus onto the stack.
    pub fn open_form(&mut self, f: FormKind) {
        // If palette is the current focus, close it and restore underlying focus first.
        if self.focus == Focus::Palette {
            self.palette_open = false;
            self.focus = self.focus_stack.pop().unwrap_or(Focus::RoomList);
        }
        if self.focus != Focus::Modal {
            self.focus_stack.push(self.focus);
            self.focus = Focus::Modal;
        }
        self.active_form = Some(f);
    }
    /// Close the current form, restoring the previous focus from the stack.
    pub fn close_form(&mut self) {
        self.active_form = None;
        if self.focus == Focus::Modal {
            self.focus = self.focus_stack.pop().unwrap_or(Focus::RoomList);
        }
    }
    pub fn active_form(&self) -> Option<&FormKind> {
        self.active_form.as_ref()
    }
    pub fn open_help(&mut self) {
        self.help_open = true;
    }
    pub fn close_help(&mut self) {
        self.help_open = false;
    }
    pub fn is_help_open(&self) -> bool {
        self.help_open
    }
    pub fn quit(&mut self) {
        self.should_quit = true;
    }
    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    // ── optimistic UX helpers ────────────────────────────────────────────────

    /// Set a transient command-acknowledgment status message.
    /// It is cleared automatically by [`Self::tick`] after 2 seconds.
    pub fn set_status_message(&mut self, msg: String) {
        self.status_message = Some((msg, Instant::now()));
    }

    /// Called on every UI timer tick (~120 ms). Expires status messages older
    /// than 2 seconds so the status bar returns to normal hints.
    pub fn tick(&mut self) {
        self.spinner_tick = self.spinner_tick.wrapping_add(1);
        if let Some((_, set_at)) = &self.status_message {
            if set_at.elapsed() >= Duration::from_secs(2) {
                self.status_message = None;
            }
        }
    }

    /// Pull a fresh projection snapshot inline after an explicit local state
    /// change (selection, command dispatch, or cached command result).
    pub fn refresh_projection(&mut self) {
        let view = self.shared.project();
        self.ingest_projection(view);
    }

    /// Clear any relay error and re-dispatch discover to attempt reconnection.
    pub fn reconnect(&mut self) {
        self.relay_error = None;
        self.connecting_since = Some(std::time::Instant::now());
        self.connected_at = None;
        if self.app.is_some() {
            let relay = self.relay_url.clone();
            let discover_body = serde_json::json!({ "relay_url": relay }).to_string();
            self.dispatch_json("nmp.nip29.discover", &discover_body);
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        // Release claimed profile refs while `self.app` is still alive — the
        // session is about to shut down anyway, but this mirrors the
        // resolve/release symmetry the rest of the bridge relies on and keeps
        // kernel-side refcounts honest if `Drop` ever runs without a full
        // `shutdown()` (e.g. a future early-return path).
        let claimed: Vec<String> = self.claimed_profile_authors.drain().collect();
        for pubkey in claimed {
            self.release_profile_ref(&pubkey);
        }
        let Some(app) = self.app.take() else { return };
        if let Some(id) = self.group_tree_observer_id.take() {
            app.close_observed_projection(id);
        }
        if let Some(handle) = self.discovery_handle.take() {
            app.close_nip29_group_discovery_session(handle);
        }
        if let Some(handle) = self.group_events_handle.take() {
            app.close_nip29_group_events_session(handle);
        }
        if let Ok(mut slot) = self.shared.joined_handle.lock() {
            if let Some(handle) = slot.take() {
                app.close_nip29_joined_groups_session(handle);
            }
        }
        // `shutdown()` also clears the update listener (dropping the
        // `ProjectionUpdateBridge` registered in `init_nmp`), so there is no
        // separate sink-teardown step here.
        app.shutdown();
    }
}

impl TuiProfile {
    fn from_card(pubkey: String, card: nmp_core::typed_projections::ProfileCardModel) -> Self {
        let display_name = card
            .display_name
            .or(card.name)
            .filter(|value| !value.is_empty());
        Self {
            pubkey: pubkey.clone(),
            display_name,
            npub: Some(nmp_core::display::to_npub(&pubkey)),
            picture_url: card.picture_url.filter(|value| !value.is_empty()),
        }
    }
}

/// Update-sink callback registered via [`nmp_uniffi_support::set_update_sink`]
/// in [`App::init_nmp`]. Replaces the deleted `nmp-ffi` C-ABI's
/// `extern "C" fn(*mut c_void, *const u8, usize)` callback shape — the
/// runtime now delivers owned update-frame bytes straight to a safe Rust
/// closure/fn over the sink type, no raw pointers involved.
fn on_nmp_update(bridge: &ProjectionUpdateBridge, bytes: Vec<u8>) {
    let decoded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        (
            nmp_core::decode_snapshot_envelope(&bytes),
            nmp_core::decode_snapshot_typed_projections(&bytes),
        )
    }));
    if let Ok((Ok(envelope), Ok(typed))) = decoded {
        if let Some(entry) = typed.iter().find(|entry| entry.key == REFS_PROFILE_KEY) {
            if let Ok(mut store) = bridge.shared.profile_refs.lock() {
                store.apply_sidecar(&entry.payload, envelope.session_id, envelope.snapshot_epoch);
            }
        }
    }
    let _ = bridge.projection_tx.send(bridge.shared.project());
}

fn generate_local_id() -> String {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("ch-{n:x}")
}

fn name_for(d: &DiscoveredGroupsSnapshot, id: &str) -> String {
    d.groups
        .iter()
        .find(|g| g.group_id == id)
        .and_then(|g| g.name.clone())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| id.to_string())
}

fn make_item(
    g: &DiscoveredGroup,
    depth: usize,
    tree: &GroupTreeMessageState,
    is_branch: bool,
    _my_pubkey: Option<&str>,
) -> ChannelListItem {
    let last = tree.last_message_for(&g.group_id);
    let unread = tree.unread_for(&g.group_id);
    let tier = if unread > 0 {
        // NMP does not yet surface per-message `p` tags in GroupTreeProjection,
        // so we cannot reliably detect @mentions here (the preview contains the
        // display-name token, not the pubkey). Use Unread for all unread channels;
        // Mention tier will be promoted when NMP exposes a dedicated field.
        ChannelTier::Unread
    } else {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let recently_active = last
            .map(|m| m.created_at >= now.saturating_sub(3600))
            .unwrap_or(false);
        if recently_active {
            ChannelTier::Activity
        } else {
            ChannelTier::Normal
        }
    };
    ChannelListItem {
        group_id: GroupId::new(g.host_relay_url.clone(), g.group_id.clone()),
        local_id: g.group_id.clone(),
        name: g
            .name
            .clone()
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| g.group_id.clone()),
        depth,
        unread,
        member_count: g.member_count,
        admin_count: g.admin_count,
        is_branch,
        last_preview: last.map(|m| m.preview.clone()),
        last_timestamp: last.map(|m| m.created_at),
        tier,
    }
}

/// Map NMP discovery + group-tree projections into a flattened, depth-annotated
/// channel list (issue #3). Roots and children are ordered alphabetically.
/// `my_pubkey` is forwarded to `make_item` for mention-tier detection.
#[must_use]
pub fn derive_channel_tree(
    discovered: &DiscoveredGroupsSnapshot,
    tree_state: &GroupTreeMessageState,
    my_pubkey: Option<&str>,
) -> Vec<ChannelListItem> {
    use std::collections::{BTreeMap, BTreeSet};
    let known: BTreeSet<&str> = discovered
        .groups
        .iter()
        .map(|g| g.group_id.as_str())
        .collect();
    let mut parent_of: BTreeMap<&str, &str> = BTreeMap::new();
    let mut children_of: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for g in &discovered.groups {
        if let Some(p) = g.parent.as_deref() {
            if p != g.group_id && known.contains(p) {
                parent_of.insert(&g.group_id, p);
                children_of
                    .entry(p)
                    .or_default()
                    .insert(&g.group_id as &str);
            }
        }
        for c in &g.children {
            let c = c.as_str();
            if c != g.group_id && known.contains(c) {
                children_of
                    .entry(&g.group_id as &str)
                    .or_default()
                    .insert(c);
                parent_of.entry(c).or_insert(&g.group_id as &str);
            }
        }
    }
    let by_id: BTreeMap<&str, &DiscoveredGroup> = discovered
        .groups
        .iter()
        .map(|g| (g.group_id.as_str(), g))
        .collect();
    let mut roots: Vec<&str> = discovered
        .groups
        .iter()
        .map(|g| g.group_id.as_str())
        .filter(|id| !parent_of.contains_key(id))
        .collect();
    roots.sort_by_key(|id| name_for(discovered, id).to_lowercase());
    let mut out = Vec::new();
    let mut stack: Vec<(&str, usize)> = roots.iter().rev().map(|id| (*id, 0usize)).collect();
    let mut visited: BTreeSet<&str> = BTreeSet::new();
    while let Some((id, depth)) = stack.pop() {
        if !visited.insert(id) {
            continue;
        }
        if let Some(g) = by_id.get(id) {
            let branch = children_of.get(id).map(|c| !c.is_empty()).unwrap_or(false);
            out.push(make_item(g, depth, tree_state, branch, my_pubkey));
            if let Some(children) = children_of.get(id) {
                let mut kids: Vec<&str> = children.iter().copied().collect();
                kids.sort_by_key(|cid| name_for(discovered, cid).to_lowercase());
                for cid in kids.into_iter().rev() {
                    stack.push((cid, depth + 1));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use nmp_app_29er::kinds::KIND_CHAT_MESSAGE;
    use nmp_core::substrate::KernelEvent;
    use nmp_nip29::projection::GroupEvent as GroupChatMessage;

    fn group(id: &str, parent: Option<&str>, children: &[&str]) -> DiscoveredGroup {
        DiscoveredGroup {
            group_id: id.to_string(),
            host_relay_url: "wss://h".to_string(),
            name: Some(id.to_string()),
            picture: None,
            about: None,
            member_count: 3,
            admin_count: 1,
            public: true,
            open: true,
            parent: parent.map(str::to_string),
            children: children.iter().map(|s| s.to_string()).collect(),
        }
    }
    fn snap() -> DiscoveredGroupsSnapshot {
        DiscoveredGroupsSnapshot {
            host_relay_url: "wss://h".to_string(),
            groups: vec![
                group("root", None, &["child"]),
                group("child", Some("root"), &[]),
                group("alpha", None, &[]),
            ],
        }
    }
    fn evt(id: &str, g: &str, ts: u64, c: &str) -> KernelEvent {
        KernelEvent {
            id: id.to_string(),
            author: "pk".to_string(),
            kind: KIND_CHAT_MESSAGE,
            created_at: ts,
            tags: vec![vec!["h".to_string(), g.to_string()]],
            content: c.to_string(),
            relay_provenance: Vec::new(),
        }
    }
    fn make_app() -> (App, watch::Receiver<ProjectionView>) {
        let (tx, rx) = watch::channel(ProjectionView::default());
        (App::new("wss://relay.example.com", tx), rx)
    }

    // --- existing tests (issue #11 baseline) ---

    #[test]
    fn tree_is_depth_annotated_and_alpha_ordered() {
        let items = derive_channel_tree(&snap(), &GroupTreeProjection::new().snapshot(), None);
        let ids: Vec<_> = items
            .iter()
            .map(|i| (i.local_id.as_str(), i.depth, i.is_branch))
            .collect();
        assert_eq!(
            ids,
            vec![("alpha", 0, false), ("root", 0, true), ("child", 1, false)]
        );
    }

    #[test]
    fn item_carries_unread_and_preview_from_group_tree() {
        let proj = GroupTreeProjection::new();
        proj.on_kernel_event(&evt("a", "child", 10, "hello"));
        proj.on_kernel_event(&evt("b", "child", 20, "newest"));
        let items = derive_channel_tree(&snap(), &proj.snapshot(), None);
        let child = items.iter().find(|i| i.local_id == "child").unwrap();
        assert_eq!(child.unread, 2);
        assert_eq!(child.last_preview.as_deref(), Some("newest"));
        assert_eq!(child.member_count, 3);
    }

    // --- new comprehensive tests ---

    /// T1: derive_channel_tree with multi-level parent/child nesting produces
    /// correct depth values and alphabetical ordering at each level.
    #[test]
    fn test_derive_channel_tree_depth_ordering() {
        // Three-node tree: root -> child; plus an independent alpha root.
        // Expected order: alpha(d=0) < root(d=0) < child(d=1)
        let items = derive_channel_tree(&snap(), &GroupTreeProjection::new().snapshot(), None);
        let result: Vec<_> = items
            .iter()
            .map(|i| (i.local_id.as_str(), i.depth, i.is_branch))
            .collect();
        assert_eq!(
            result,
            vec![("alpha", 0, false), ("root", 0, true), ("child", 1, false),]
        );
    }

    /// T2: derive_channel_tree surfaces unread count and last-preview text
    /// from the GroupTreeProjection; preview is the verbatim event content.
    #[test]
    fn test_derive_channel_tree_unread_preview() {
        let proj = GroupTreeProjection::new();
        // Two messages on "child"; second is newer.
        proj.on_kernel_event(&evt(
            "e1",
            "child",
            100,
            "first message that is kind of long text here",
        ));
        proj.on_kernel_event(&evt("e2", "child", 200, "short"));
        let items = derive_channel_tree(&snap(), &proj.snapshot(), None);
        let child = items.iter().find(|i| i.local_id == "child").unwrap();
        // unread count covers both messages
        assert_eq!(child.unread, 2);
        // last_preview is the content of the most recent event (latest timestamp)
        assert_eq!(child.last_preview.as_deref(), Some("short"));
        assert_eq!(child.last_timestamp, Some(200));
        // member_count flows from DiscoveredGroup, not GroupTreeProjection
        assert_eq!(child.member_count, 3);
    }

    /// T3: TuiSnapshot carries every field without loss — this is a compile-time
    /// regression guard that construction with all fields present succeeds and
    /// read-back matches what was written.
    #[test]
    fn test_tui_snapshot_fields() {
        let gid = GroupId::new("wss://h", "grp1");
        let snap = TuiSnapshot {
            channel_tree: vec![],
            selected_channel_id: Some(gid.clone()),
            selected_messages: vec![GroupChatMessage {
                id: "e1".to_string(),
                pubkey: "pk1".to_string(),
                content: "hello".to_string(),
                created_at: 1000,
                kind: 9,
            }],
            selected_members: vec![GroupMemberRow {
                pubkey: "pk1".to_string(),
                display_name: Some("Alice".to_string()),
                admin: true,
                role: None,
            }],
            profiles: HashMap::new(),
            is_admin: true,
            my_pubkey: Some("pk1".to_string()),
            publish_outbox: vec![PublishOutboxItem {
                correlation_id: "cid".to_string(),
                group_local_id: "grp1".to_string(),
                content: "hi".to_string(),
                status: OutboxStatus::Pending,
                error: None,
                mention_pubkeys: Vec::new(),
                event_id: None,
                dispatched_at: Instant::now(),
            }],
            identity_state: IdentityState::LoggedIn {
                npub: "npub1test".to_string(),
            },
            relay_state: RelayState::Connected,
            errors: vec!["oops".to_string()],
            selected_index: 3,
            focus: Focus::Composer,
            message_scroll: 7,
            palette_open: true,
            active_form: Some(FormKind::JoinWithCode(gid)),
            login_error: None,
            screen: Screen::App,
            help_open: false,
            status_message: Some("Joining room\u{2026}".to_string()),
            last_read_message_id: Some("e1".to_string()),
            spinner_tick: 42,
            connecting_since: None,
            connected_at: None,
        };

        assert_eq!(snap.selected_index, 3);
        assert_eq!(snap.spinner_tick, 42);
        assert!(snap.is_admin);
        assert_eq!(snap.focus, Focus::Composer);
        assert_eq!(snap.message_scroll, 7);
        assert!(snap.palette_open);
        assert_eq!(snap.screen, Screen::App);
        assert_eq!(snap.relay_state, RelayState::Connected);
        assert_eq!(snap.my_pubkey.as_deref(), Some("pk1"));
        assert_eq!(snap.errors, vec!["oops"]);
        assert_eq!(snap.selected_messages.len(), 1);
        assert_eq!(snap.selected_members.len(), 1);
        assert_eq!(snap.publish_outbox.len(), 1);
        assert_eq!(snap.publish_outbox[0].status, OutboxStatus::Pending);
        assert!(
            matches!(snap.identity_state, IdentityState::LoggedIn { ref npub } if npub == "npub1test")
        );
        assert!(matches!(snap.active_form, Some(FormKind::JoinWithCode(_))));
        assert!(snap.login_error.is_none());
        assert_eq!(snap.status_message.as_deref(), Some("Joining room\u{2026}"));
    }

    /// T4: IdentityState state-machine — Default is LoggedOut; transitions
    /// between variants are correctly distinguishable.
    #[test]
    fn test_identity_state_transitions() {
        // Default is LoggedOut
        assert_eq!(IdentityState::default(), IdentityState::LoggedOut);
        // ProjectionView also defaults to LoggedOut
        assert_eq!(
            ProjectionView::default().identity_state,
            IdentityState::LoggedOut
        );

        // LoggedOut != LoggingIn != LoggedIn
        let logged_out = IdentityState::LoggedOut;
        let logging_in = IdentityState::LoggingIn;
        let logged_in = IdentityState::LoggedIn {
            npub: "npub1abc".to_string(),
        };
        assert_ne!(logged_out, logging_in);
        assert_ne!(logging_in, logged_in);
        assert_ne!(logged_out, logged_in);

        // LoggedIn exposes the npub
        if let IdentityState::LoggedIn { ref npub } = logged_in {
            assert_eq!(npub, "npub1abc");
        } else {
            panic!("expected LoggedIn");
        }

        // A freshly constructed App starts in Login screen with LoggedOut identity
        let (app, _rx) = make_app();
        let snap = app.snapshot();
        assert_eq!(snap.screen, Screen::Login);
        assert_eq!(snap.identity_state, IdentityState::LoggedOut);
    }

    /// T5: select_channel resets message_scroll to 0 regardless of prior value.
    #[test]
    fn test_selected_channel_change_clears_scroll() {
        let (mut app, _rx) = make_app();

        // Scroll down first
        app.scroll_messages(15);
        assert_eq!(app.snapshot().message_scroll, 15);

        // Selecting a channel must reset scroll to 0
        let group = GroupId::new("wss://relay.example.com", "channel-a");
        app.select_channel(group.clone());
        assert_eq!(app.snapshot().message_scroll, 0);

        // Scroll again, select a different channel — scroll resets again
        app.scroll_messages(5);
        let group2 = GroupId::new("wss://relay.example.com", "channel-b");
        app.select_channel(group2);
        assert_eq!(app.snapshot().message_scroll, 0);

        // selected_channel_id in the snapshot reflects the last selection
        assert_eq!(
            app.snapshot()
                .selected_channel_id
                .as_ref()
                .map(|g| g.local_id.as_str()),
            Some("channel-b"),
        );
    }

    /// T6: reconcile_outbox promotes a Pending item to Confirmed when a
    /// matching message (same trimmed content, same pubkey as my_pubkey) arrives.
    #[test]
    fn test_reconcile_outbox_pending_becomes_confirmed() {
        let (mut app, _rx) = make_app();

        // Inject a Pending item directly (private field visible from child mod).
        app.outbox.push(PublishOutboxItem {
            correlation_id: "cid-pending".to_string(),
            group_local_id: "grp".to_string(),
            content: "hello world".to_string(),
            status: OutboxStatus::Pending,
            error: None,
            mention_pubkeys: Vec::new(),
            event_id: None,
            dispatched_at: Instant::now(),
        });

        // The matching message MUST carry the same pubkey as my_pubkey.
        let matching_msg = GroupChatMessage {
            id: "event-1".to_string(),
            pubkey: "my-pk".to_string(),
            content: "hello world".to_string(),
            created_at: 12345,
            kind: 9,
        };
        let mut view = ProjectionView::default();
        view.my_pubkey = Some("my-pk".to_string());
        view.selected_messages = vec![matching_msg];

        app.ingest_projection(view);

        let snap = app.snapshot();
        assert_eq!(snap.publish_outbox.len(), 1);
        assert_eq!(
            snap.publish_outbox[0].status,
            OutboxStatus::Confirmed,
            "pending item must become Confirmed once matching message with correct pubkey arrives"
        );
    }

    /// T6b_guard: reconcile_outbox does NOT confirm a Pending item when my_pubkey
    /// is unknown — the guard must return early and leave all items as Pending.
    #[test]
    fn test_reconcile_outbox_no_pubkey_stays_pending() {
        let (mut app, _rx) = make_app();

        app.outbox.push(PublishOutboxItem {
            correlation_id: "cid-pending".to_string(),
            group_local_id: "grp".to_string(),
            content: "hello world".to_string(),
            status: OutboxStatus::Pending,
            error: None,
            mention_pubkeys: Vec::new(),
            event_id: None,
            dispatched_at: Instant::now(),
        });

        // Matching content, but my_pubkey is None — must NOT confirm.
        let matching_msg = GroupChatMessage {
            id: "event-1".to_string(),
            pubkey: "any-pubkey".to_string(),
            content: "hello world".to_string(),
            created_at: 12345,
            kind: 9,
        };
        let mut view = ProjectionView::default();
        // Intentionally NOT setting view.my_pubkey.
        view.selected_messages = vec![matching_msg];

        app.ingest_projection(view);

        let snap = app.snapshot();
        assert_eq!(
            snap.publish_outbox[0].status,
            OutboxStatus::Pending,
            "item must remain Pending when my_pubkey is not known"
        );
    }

    /// T6b: a Failed outbox item is NOT promoted even if a matching message arrives.
    #[test]
    fn test_reconcile_outbox_failed_stays_failed() {
        let (mut app, _rx) = make_app();

        app.outbox.push(PublishOutboxItem {
            correlation_id: "cid-failed".to_string(),
            group_local_id: "grp".to_string(),
            content: "oops".to_string(),
            status: OutboxStatus::Failed,
            error: Some("dispatch failed".to_string()),
            mention_pubkeys: Vec::new(),
            event_id: None,
            dispatched_at: Instant::now(),
        });

        let msg = GroupChatMessage {
            id: "event-2".to_string(),
            pubkey: "pk".to_string(),
            content: "oops".to_string(),
            created_at: 9999,
            kind: 9,
        };
        let mut view = ProjectionView::default();
        view.selected_messages = vec![msg];
        app.ingest_projection(view);

        let snap = app.snapshot();
        assert_eq!(
            snap.publish_outbox[0].status,
            OutboxStatus::Failed,
            "a Failed item must NOT be re-confirmed by reconcile"
        );
    }

    /// T7: Focus::next/prev cycle RoomList -> Chat -> Composer -> RoomList.
    #[test]
    fn test_focus_cycle() {
        // Forward cycle
        assert_eq!(Focus::RoomList.next(), Focus::Chat);
        assert_eq!(Focus::Chat.next(), Focus::Composer);
        assert_eq!(Focus::Composer.next(), Focus::RoomList);
        // Reverse cycle
        assert_eq!(Focus::RoomList.prev(), Focus::Composer);
        assert_eq!(Focus::Composer.prev(), Focus::Chat);
        assert_eq!(Focus::Chat.prev(), Focus::RoomList);
        // Palette/Modal don't participate
        assert_eq!(Focus::Palette.next(), Focus::Palette);
        assert_eq!(Focus::Modal.prev(), Focus::Modal);

        let (mut app, _rx) = make_app();
        assert_eq!(app.focus(), Focus::RoomList);
        app.cycle_focus();
        assert_eq!(app.focus(), Focus::Chat);
        app.cycle_focus();
        assert_eq!(app.focus(), Focus::Composer);
        app.cycle_focus();
        assert_eq!(app.focus(), Focus::RoomList);
    }

    /// T8: navigate() wraps around at both ends of the channel list.
    #[test]
    fn test_navigate_wraps() {
        let (mut app, _rx) = make_app();

        // Build a fake channel tree with 3 items.
        let make_channel = |id: &str| ChannelListItem {
            group_id: GroupId::new("wss://h", id),
            local_id: id.to_string(),
            name: id.to_string(),
            depth: 0,
            unread: 0,
            member_count: 0,
            admin_count: 0,
            is_branch: false,
            last_preview: None,
            last_timestamp: None,
            tier: ChannelTier::Normal,
        };
        let mut view = ProjectionView::default();
        view.channel_tree = vec![make_channel("a"), make_channel("b"), make_channel("c")];
        app.ingest_projection(view);

        // Starts at index 0
        assert_eq!(app.snapshot().selected_index, 0);

        // Navigate forward
        app.navigate(1);
        assert_eq!(app.snapshot().selected_index, 1);
        app.navigate(1);
        assert_eq!(app.snapshot().selected_index, 2);

        // Wrap around forward (3 -> 0)
        app.navigate(1);
        assert_eq!(app.snapshot().selected_index, 0);

        // Wrap around backward (0 -> 2)
        app.navigate(-1);
        assert_eq!(app.snapshot().selected_index, 2);
    }

    /// T9: set_status_message appears immediately in snapshot; tick() expires it
    /// after the 2-second window.
    #[test]
    fn test_status_message_set_and_expires() {
        let (mut app, _rx) = make_app();

        // Initially no status message.
        assert!(app.snapshot().status_message.is_none());

        // Set a message — should appear in the very next snapshot.
        app.set_status_message("Joining room\u{2026}".to_string());
        assert_eq!(
            app.snapshot().status_message.as_deref(),
            Some("Joining room\u{2026}"),
        );

        // tick() before expiry must NOT clear it.
        app.tick();
        assert!(
            app.snapshot().status_message.is_some(),
            "message cleared too early"
        );

        // Back-date the timestamp to simulate 2+ seconds having passed.
        if let Some((_, ref mut ts)) = app.status_message {
            *ts = Instant::now() - Duration::from_secs(3);
        }
        app.tick();
        assert!(
            app.snapshot().status_message.is_none(),
            "expired message must be cleared by tick()"
        );
    }

    /// T10: refresh_projection() updates `latest` immediately from an explicit
    /// local state-change hook.
    #[test]
    fn test_refresh_projection_is_immediate() {
        let (mut app, _rx) = make_app();

        // Default latest has an empty channel_tree.
        assert!(app.snapshot().channel_tree.is_empty());

        // Manually inject state into the shared projections via ingest_projection
        // (refresh_projection calls shared.project() which will still be empty here
        //  because no NMP is running; this test just verifies the call does NOT panic
        //  and the method is reachable).
        app.refresh_projection(); // must not panic
        assert!(app.snapshot().channel_tree.is_empty()); // still empty — no NMP running
    }

    #[test]
    fn nmp_update_push_publishes_projection_without_profile_sidecar() {
        let (app, mut rx) = make_app();
        let bridge = ProjectionUpdateBridge {
            shared: Arc::clone(&app.shared),
            projection_tx: app.projection_tx.clone(),
        };

        on_nmp_update(&bridge, Vec::new());

        assert!(matches!(rx.has_changed(), Ok(true)));
        let view = rx.borrow_and_update().clone();
        assert_eq!(view.identity_state, IdentityState::LoggingIn);
    }
}
