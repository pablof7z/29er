//! NMP composition root + projection-backed view-model for the TUI.
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nmp_app_29er::group_tree::{GroupTreeMessageState, GroupTreeProjection};
use nmp_app_29er::{
    nmp_app_29er_declare_consumed_projections, nmp_app_29er_dispatch_action_bytes,
    nmp_app_29er_register, nmp_app_29er_register_group_chat, nmp_app_29er_unregister,
    TwentyNinerHandle,
};
use nmp_core::KernelEventObserver;
use nmp_ffi::{nmp_free_string, NmpApp};
use nmp_nip29::projection::{
    DiscoveredGroup, DiscoveredGroupsProjection, DiscoveredGroupsSnapshot, GroupChatMessage,
    GroupChatProjection, GroupMemberRow, GroupMembersProjection,
};
use nmp_nip29::GroupId;
use tokio::sync::mpsc::UnboundedSender;

type ActiveAccountSlot = Arc<Mutex<Option<String>>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen { Login, App }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    RoomList,   // primary sidebar (was ChannelList)
    Chat,
    Composer,
    Palette,    // command palette overlay
    Modal,      // form/dialog overlay
}
impl Focus {
    /// Forward Tab cycle: only cycles through the three base panels.
    #[must_use] pub fn next(self) -> Self {
        match self {
            Focus::RoomList => Focus::Chat,
            Focus::Chat => Focus::Composer,
            Focus::Composer => Focus::RoomList,
            other => other, // Palette/Modal don't participate
        }
    }
    /// Reverse Shift+Tab cycle.
    #[must_use] pub fn prev(self) -> Self {
        match self {
            Focus::RoomList => Focus::Composer,
            Focus::Chat => Focus::RoomList,
            Focus::Composer => Focus::Chat,
            other => other,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IdentityState { LoggedOut, LoggingIn, LoggedIn { npub: String } }

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelayState { Disconnected, Connecting, Connected, Error(String) }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutboxStatus { Pending, Confirmed, Failed }

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormKind {
    JoinWithCode(GroupId),
    CreateInvite(GroupId),
    CreateChild(GroupId),
    PutUser(GroupId),
    MoveChannel(GroupId),
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
}

#[derive(Clone, Debug)]
pub struct PublishOutboxItem {
    pub correlation_id: String,
    pub group_local_id: String,
    pub content: String,
    pub status: OutboxStatus,
    pub error: Option<String>,
}

/// The immutable per-frame view-model. Contains ZERO Ratatui types (issue #3).
#[derive(Clone, Debug)]
pub struct TuiSnapshot {
    pub channel_tree: Vec<ChannelListItem>,
    pub selected_channel_id: Option<GroupId>,
    pub selected_messages: Vec<GroupChatMessage>,
    pub selected_members: Vec<GroupMemberRow>,
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
    /// Monotonically incrementing tick counter; drives the spinner frame selector.
    pub spinner_tick: u64,
    /// When the NMP relay connection attempt started (set at login / reconnect).
    pub connecting_since: Option<std::time::Instant>,
    /// When we first observed `relay_connected = true` (used for the 2-second flash).
    pub connected_at: Option<std::time::Instant>,
}

/// Projection-derived fields produced on the poller thread and sent over mpsc.
#[derive(Clone, Debug, Default)]
pub struct ProjectionView {
    pub channel_tree: Vec<ChannelListItem>,
    pub selected_messages: Vec<GroupChatMessage>,
    pub selected_members: Vec<GroupMemberRow>,
    pub is_admin: bool,
    pub my_pubkey: Option<String>,
    pub identity_state: IdentityState,
    pub relay_connected: bool,
}
impl Default for IdentityState { fn default() -> Self { IdentityState::LoggedOut } }

/// Send+Sync read-side state shared between App (main thread) and the poller.
pub struct SharedProjections {
    pub group_tree: Arc<GroupTreeProjection>,
    pub discovered: Arc<DiscoveredGroupsProjection>,
    pub members: Arc<GroupMembersProjection>,
    pub active_account: Mutex<ActiveAccountSlot>,
    pub selected_chat: Mutex<Option<Arc<GroupChatProjection>>>,
    pub selected_group: Mutex<Option<GroupId>>,
    /// Set on each snapshot poll that returns non-empty data; drives relay-state indicator.
    pub last_update_at: Mutex<Option<std::time::Instant>>,
}
impl SharedProjections {
    pub fn project(&self) -> ProjectionView {
        let discovered = self.discovered.snapshot();
        let tree_state = self.group_tree.snapshot();
        let channel_tree = derive_channel_tree(&discovered, &tree_state);
        let selected_messages = self.selected_chat.lock().ok()
            .and_then(|c| c.as_ref().map(|c| c.snapshot().messages)).unwrap_or_default();
        let members = self.members.snapshot().members;
        let me = self.active_account.lock().ok().and_then(|s| s.lock().ok().and_then(|v| v.clone()));
        let is_admin = me.as_ref().map(|pk| members.iter().any(|m| &m.pubkey == pk && m.admin)).unwrap_or(false);
        let identity_state = match &me {
            Some(pk) => IdentityState::LoggedIn { npub: nmp_core::display::to_npub(pk) },
            None => IdentityState::LoggingIn,
        };
        let has_data = !channel_tree.is_empty() || !selected_messages.is_empty();
        if has_data {
            if let Ok(mut ts) = self.last_update_at.lock() { *ts = Some(std::time::Instant::now()); }
        }
        let relay_connected = self.last_update_at.lock().ok().and_then(|ts| *ts).is_some();
        ProjectionView { channel_tree, selected_messages, selected_members: members, is_admin, my_pubkey: me, identity_state, relay_connected }
    }
}

/// Spawn the 4Hz background poller (issue #10). Captures only Send Arcs.
pub fn spawn_poller(shared: Arc<SharedProjections>, tx: UnboundedSender<ProjectionView>) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_millis(250));
        loop {
            ticker.tick().await;
            if tx.send(shared.project()).is_err() { break; }
        }
    });
}

pub struct App {
    app_ptr: *mut NmpApp,
    handle: *mut TwentyNinerHandle,
    relay_url: String,
    shared: Arc<SharedProjections>,
    poll_tx: UnboundedSender<ProjectionView>,
    chats: HashMap<String, Arc<GroupChatProjection>>,
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
    spinner_tick: u64,
    connecting_since: Option<std::time::Instant>,
    connected_at: Option<std::time::Instant>,
    relay_error: Option<String>,
}

impl App {
    pub fn new(relay_url: impl Into<String>, poll_tx: UnboundedSender<ProjectionView>) -> Self {
        let relay_url = relay_url.into();
        let shared = Arc::new(SharedProjections {
            group_tree: Arc::new(GroupTreeProjection::new()),
            discovered: Arc::new(DiscoveredGroupsProjection::new(relay_url.clone())),
            members: Arc::new(GroupMembersProjection::new(relay_url.clone())),
            active_account: Mutex::new(Arc::new(Mutex::new(None))),
            selected_chat: Mutex::new(None),
            selected_group: Mutex::new(None),
            last_update_at: Mutex::new(None),
        });
        Self {
            app_ptr: std::ptr::null_mut(), handle: std::ptr::null_mut(), relay_url,
            shared, poll_tx, chats: HashMap::new(), latest: ProjectionView::default(),
            screen: Screen::Login, focus: Focus::RoomList, focus_stack: Vec::new(),
            selected_index: 0, selected_channel: None, outbox: Vec::new(), errors: Vec::new(),
            palette_open: false, active_form: None, message_scroll: 0,
            login_error: None, help_open: false, should_quit: false,
            spinner_tick: 0, connecting_since: None, connected_at: None, relay_error: None,
        }
    }

    /// Validate + hand the nsec straight to NMP, never storing it (issue #10).
    pub fn login(&mut self, nsec: String) {
        let nsec = nsec.trim().to_string();
        if !nsec.starts_with("nsec1") {
            self.login_error = Some("Secret key must start with nsec1\u{2026}".to_string());
            return;
        }
        match self.init_nmp(&nsec) {
            Ok(()) => {
                self.screen = Screen::App; self.focus = Focus::RoomList;
                self.focus_stack.clear(); self.login_error = None;
                self.connecting_since = Some(std::time::Instant::now());
                self.connected_at = None; self.relay_error = None;
            }
            Err(e) => { self.login_error = Some(format!("Sign-in failed: {e}")); }
        }
        // `nsec` is dropped here.
    }

    fn init_nmp(&mut self, nsec: &str) -> anyhow::Result<()> {
        let relay = self.relay_url.clone();
        let storage = std::env::temp_dir().join("29er-tui-store");
        std::fs::create_dir_all(&storage).ok();
        let storage_str = storage.to_string_lossy().into_owned();
        unsafe {
            let app = nmp_ffi::nmp_app_new();
            if app.is_null() { anyhow::bail!("nmp_app_new returned null"); }
            let c_storage = CString::new(storage_str)?;
            nmp_ffi::nmp_app_set_storage_path(app, c_storage.as_ptr());
            let mut handle: *mut TwentyNinerHandle = std::ptr::null_mut();
            let status = nmp_app_29er_register(app, &mut handle as *mut *mut TwentyNinerHandle);
            if status != 0 { nmp_ffi::nmp_app_free(app); anyhow::bail!("register failed ({status})"); }
            nmp_app_29er_declare_consumed_projections(app);
            let app_ref = &*app;
            let _ = app_ref.register_event_observer(Arc::clone(&self.shared.group_tree) as Arc<dyn KernelEventObserver>);
            let _ = app_ref.register_event_observer(Arc::clone(&self.shared.discovered) as Arc<dyn KernelEventObserver>);
            let _ = app_ref.register_event_observer(Arc::clone(&self.shared.members) as Arc<dyn KernelEventObserver>);
            let c_relay = CString::new(relay.clone())?;
            let c_role = CString::new("read")?;
            nmp_ffi::nmp_app_add_relay(app, c_relay.as_ptr(), c_role.as_ptr());
            nmp_ffi::nmp_app_start(app, 80, 4);
            let c_nsec = CString::new(nsec)?;
            nmp_ffi::nmp_app_signin_nsec(app, c_nsec.as_ptr(), 1);
            if let Ok(mut slot) = self.shared.active_account.lock() { *slot = app_ref.active_account_handle(); }
            self.app_ptr = app;
            self.handle = handle;
        }
        let discover_body = serde_json::json!({ "relay_url": relay }).to_string();
        self.dispatch_json("nmp.nip29.discover", &discover_body);
        spawn_poller(Arc::clone(&self.shared), self.poll_tx.clone());
        Ok(())
    }

    /// Store the latest poller view; clamp selection; reconcile the outbox.
    pub fn ingest_projection(&mut self, view: ProjectionView) {
        let was_connected = self.latest.relay_connected;
        self.latest = view;
        // Record the first moment relay_connected flips to true (drives the 2s flash).
        if !was_connected && self.latest.relay_connected && self.connected_at.is_none() {
            self.connected_at = Some(std::time::Instant::now());
        }
        if self.latest.channel_tree.is_empty() { self.selected_index = 0; }
        else if self.selected_index >= self.latest.channel_tree.len() {
            self.selected_index = self.latest.channel_tree.len() - 1;
        }
        self.reconcile_outbox();
    }

    fn reconcile_outbox(&mut self) {
        let me = self.latest.my_pubkey.clone();
        for item in self.outbox.iter_mut() {
            if matches!(item.status, OutboxStatus::Pending) {
                let confirmed = self.latest.selected_messages.iter().any(|m| {
                    m.content.trim() == item.content.trim()
                        && me.as_deref().map(|pk| m.pubkey == pk).unwrap_or(true)
                });
                if confirmed { item.status = OutboxStatus::Confirmed; }
            }
        }
    }

    pub fn snapshot(&self) -> TuiSnapshot {
        TuiSnapshot {
            channel_tree: self.latest.channel_tree.clone(),
            selected_channel_id: self.selected_channel.clone(),
            selected_messages: self.latest.selected_messages.clone(),
            selected_members: self.latest.selected_members.clone(),
            is_admin: self.latest.is_admin,
            my_pubkey: self.latest.my_pubkey.clone(),
            publish_outbox: self.outbox.clone(),
            identity_state: self.latest.identity_state.clone(),
            relay_state: if let Some(ref err) = self.relay_error {
                RelayState::Error(err.clone())
            } else if !self.app_ptr.is_null() && self.latest.relay_connected {
                RelayState::Connected
            } else if !self.app_ptr.is_null() {
                RelayState::Connecting
            } else {
                RelayState::Disconnected
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
            spinner_tick: self.spinner_tick,
            connecting_since: self.connecting_since,
            connected_at: self.connected_at,
        }
    }

    pub fn select_channel(&mut self, group: GroupId) {
        let key = group.local_id.clone();
        if !self.chats.contains_key(&key) {
            let chat = Arc::new(GroupChatProjection::new(group.clone()));
            if !self.app_ptr.is_null() {
                unsafe { let _ = (&*self.app_ptr).register_event_observer(Arc::clone(&chat) as Arc<dyn KernelEventObserver>); }
                if let Ok(json) = serde_json::to_string(&group) {
                    if let Ok(c) = CString::new(json) { nmp_app_29er_register_group_chat(self.app_ptr, c.as_ptr()); }
                }
            }
            self.chats.insert(key.clone(), chat);
        }
        if let Some(chat) = self.chats.get(&key) {
            if let Ok(mut slot) = self.shared.selected_chat.lock() { *slot = Some(Arc::clone(chat)); }
        }
        if let Ok(mut g) = self.shared.selected_group.lock() { *g = Some(group.clone()); }
        self.shared.members.select_group(group.local_id.clone());
        self.shared.group_tree.mark_read(&group.local_id);
        self.selected_channel = Some(group);
        self.message_scroll = 0;
        self.focus = Focus::Chat;
    }

    pub fn send_message(&mut self, body: String) {
        let trimmed = body.trim().to_string();
        if trimmed.is_empty() { return; }
        let Some(group) = self.selected_channel.clone() else { self.errors.push("No channel selected".to_string()); return; };
        let json = serde_json::json!({ "group": group, "content": trimmed }).to_string();
        let result = self.dispatch_json("nmp.nip29.post_chat_message", &json);
        let mut item = PublishOutboxItem { correlation_id: String::new(), group_local_id: group.local_id.clone(), content: trimmed, status: OutboxStatus::Pending, error: None };
        Self::apply_dispatch_result(&mut item, result);
        self.outbox.push(item);
    }

    pub fn retry_outbox(&mut self, correlation_id: String) {
        let Some(idx) = self.outbox.iter().position(|i| i.correlation_id == correlation_id) else { return; };
        let (content, local_id) = { let it = &self.outbox[idx]; (it.content.clone(), it.group_local_id.clone()) };
        let Some(group) = self.selected_channel.clone().filter(|g| g.local_id == local_id) else { return; };
        let json = serde_json::json!({ "group": group, "content": content }).to_string();
        let result = self.dispatch_json("nmp.nip29.post_chat_message", &json);
        let item = &mut self.outbox[idx];
        item.status = OutboxStatus::Pending; item.error = None;
        Self::apply_dispatch_result(item, result);
    }

    fn apply_dispatch_result(item: &mut PublishOutboxItem, result: Option<String>) {
        match result {
            Some(r) => match serde_json::from_str::<serde_json::Value>(&r) {
                Ok(v) => {
                    if let Some(cid) = v.get("correlation_id").and_then(|c| c.as_str()) { item.correlation_id = cid.to_string(); }
                    else if let Some(err) = v.get("error").and_then(|c| c.as_str()) { item.status = OutboxStatus::Failed; item.error = Some(err.to_string()); }
                }
                Err(_) => { item.status = OutboxStatus::Failed; item.error = Some("bad dispatch reply".to_string()); }
            },
            None => { item.status = OutboxStatus::Failed; item.error = Some("dispatch failed".to_string()); }
        }
    }

    pub fn join(&mut self, group: GroupId, invite_code: Option<String>) {
        let body = serde_json::json!({ "group": group, "invite_code": invite_code }).to_string();
        self.dispatch_json("nmp.nip29.join", &body); self.close_form();
    }
    pub fn leave(&mut self, group: GroupId) {
        let body = serde_json::json!({ "group": group }).to_string();
        self.dispatch_json("nmp.nip29.leave", &body);
    }
    pub fn create_invite(&mut self, group: GroupId, codes: Vec<String>) {
        let body = serde_json::json!({ "group": group, "codes": codes }).to_string();
        self.dispatch_json("nmp.nip29.create_invite", &body); self.close_form();
    }
    pub fn put_user(&mut self, group: GroupId, target_pubkey: String, role: Option<String>) {
        let body = serde_json::json!({ "group": group, "target_pubkey": target_pubkey, "role": role }).to_string();
        self.dispatch_json("nmp.nip29.put_user", &body); self.close_form();
    }
    pub fn create_child(&mut self, parent: GroupId, name: String) {
        let local_id = generate_local_id();
        let body = serde_json::json!({
            "group": { "host_relay_url": parent.host_relay_url, "local_id": local_id },
            "name": name, "visibility": "public", "access": "open", "parent": parent.local_id,
        }).to_string();
        self.dispatch_json("nmp.nip29.create_public_group", &body); self.close_form();
    }
    pub fn move_channel(&mut self, group: GroupId, parent: Option<String>) {
        let body = serde_json::json!({ "group": group, "parent": parent }).to_string();
        self.dispatch_json("nmp.nip29.set_parent", &body); self.close_form();
    }
    pub fn show_members(&mut self, group: GroupId) { self.shared.members.select_group(group.local_id); }

    fn dispatch_json(&mut self, namespace: &str, body: &str) -> Option<String> {
        if self.app_ptr.is_null() { return None; }
        let (Ok(ns), Ok(b)) = (CString::new(namespace), CString::new(body)) else { return None; };
        let res = nmp_app_29er_dispatch_action_bytes(self.app_ptr, ns.as_ptr(), b.as_ptr());
        if res.is_null() { return None; }
        let out = unsafe { CStr::from_ptr(res) }.to_string_lossy().into_owned();
        nmp_free_string(res);
        Some(out)
    }

    pub fn navigate(&mut self, delta: isize) {
        let len = self.latest.channel_tree.len();
        if len == 0 { return; }
        self.selected_index = (self.selected_index as isize + delta).rem_euclid(len as isize) as usize;
    }
    pub fn navigate_top(&mut self) {
        if !self.latest.channel_tree.is_empty() { self.selected_index = 0; }
    }
    pub fn navigate_bottom(&mut self) {
        let len = self.latest.channel_tree.len();
        if len > 0 { self.selected_index = len - 1; }
    }
    pub fn channel_at_cursor(&self) -> Option<GroupId> { self.latest.channel_tree.get(self.selected_index).map(|c| c.group_id.clone()) }
    pub fn scroll_messages(&mut self, delta: i32) { let n = self.message_scroll as i32 + delta; self.message_scroll = n.max(0) as u16; }
    pub fn focus(&self) -> Focus { self.focus }
    pub fn set_focus(&mut self, f: Focus) { self.focus = f; }
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
    pub fn push_focus(&mut self) { self.focus_stack.push(self.focus); }
    /// Pop previous focus from the stack. Returns `true` if something was restored.
    pub fn pop_focus(&mut self) -> bool {
        if let Some(f) = self.focus_stack.pop() { self.focus = f; true } else { false }
    }
    pub fn screen(&self) -> Screen { self.screen }
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
    pub fn palette_open(&self) -> bool { self.palette_open }
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
    pub fn active_form(&self) -> Option<&FormKind> { self.active_form.as_ref() }
    pub fn open_help(&mut self) { self.help_open = true; }
    pub fn close_help(&mut self) { self.help_open = false; }
    pub fn is_help_open(&self) -> bool { self.help_open }
    pub fn quit(&mut self) { self.should_quit = true; }
    pub fn should_quit(&self) -> bool { self.should_quit }

    /// Advance the spinner counter by one. Called on every UI tick (~8 Hz).
    pub fn tick(&mut self) { self.spinner_tick = self.spinner_tick.wrapping_add(1); }

    /// Clear any relay error and re-dispatch discover to attempt reconnection.
    pub fn reconnect(&mut self) {
        self.relay_error = None;
        self.connecting_since = Some(std::time::Instant::now());
        self.connected_at = None;
        if !self.app_ptr.is_null() {
            let relay = self.relay_url.clone();
            let discover_body = serde_json::json!({ "relay_url": relay }).to_string();
            self.dispatch_json("nmp.nip29.discover", &discover_body);
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        if !self.handle.is_null() { nmp_app_29er_unregister(self.handle); }
        if !self.app_ptr.is_null() { nmp_ffi::nmp_app_free(self.app_ptr); }
    }
}

fn generate_local_id() -> String {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("ch-{n:x}")
}

fn name_for(d: &DiscoveredGroupsSnapshot, id: &str) -> String {
    d.groups.iter().find(|g| g.group_id == id).and_then(|g| g.name.clone())
        .filter(|n| !n.is_empty()).unwrap_or_else(|| id.to_string())
}

fn make_item(g: &DiscoveredGroup, depth: usize, tree: &GroupTreeMessageState, is_branch: bool) -> ChannelListItem {
    let last = tree.last_message_for(&g.group_id);
    ChannelListItem {
        group_id: GroupId::new(g.host_relay_url.clone(), g.group_id.clone()),
        local_id: g.group_id.clone(),
        name: g.name.clone().filter(|n| !n.is_empty()).unwrap_or_else(|| g.group_id.clone()),
        depth,
        unread: tree.unread_for(&g.group_id),
        member_count: g.member_count,
        admin_count: g.admin_count,
        is_branch,
        last_preview: last.map(|m| m.preview.clone()),
        last_timestamp: last.map(|m| m.created_at),
    }
}

/// Map NMP discovery + group-tree projections into a flattened, depth-annotated
/// channel list (issue #3). Roots and children are ordered alphabetically.
#[must_use]
pub fn derive_channel_tree(discovered: &DiscoveredGroupsSnapshot, tree_state: &GroupTreeMessageState) -> Vec<ChannelListItem> {
    use std::collections::{BTreeMap, BTreeSet};
    let known: BTreeSet<&str> = discovered.groups.iter().map(|g| g.group_id.as_str()).collect();
    let mut parent_of: BTreeMap<&str, &str> = BTreeMap::new();
    let mut children_of: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for g in &discovered.groups {
        if let Some(p) = g.parent.as_deref() {
            if p != g.group_id && known.contains(p) {
                parent_of.insert(&g.group_id, p);
                children_of.entry(p).or_default().insert(&g.group_id as &str);
            }
        }
        for c in &g.children {
            let c = c.as_str();
            if c != g.group_id && known.contains(c) {
                children_of.entry(&g.group_id as &str).or_default().insert(c);
                parent_of.entry(c).or_insert(&g.group_id as &str);
            }
        }
    }
    let by_id: BTreeMap<&str, &DiscoveredGroup> = discovered.groups.iter().map(|g| (g.group_id.as_str(), g)).collect();
    let mut roots: Vec<&str> = discovered.groups.iter().map(|g| g.group_id.as_str())
        .filter(|id| !parent_of.contains_key(id)).collect();
    roots.sort_by_key(|id| name_for(discovered, id).to_lowercase());
    let mut out = Vec::new();
    let mut stack: Vec<(&str, usize)> = roots.iter().rev().map(|id| (*id, 0usize)).collect();
    let mut visited: BTreeSet<&str> = BTreeSet::new();
    while let Some((id, depth)) = stack.pop() {
        if !visited.insert(id) { continue; }
        if let Some(g) = by_id.get(id) {
            let branch = children_of.get(id).map(|c| !c.is_empty()).unwrap_or(false);
            out.push(make_item(g, depth, tree_state, branch));
            if let Some(children) = children_of.get(id) {
                let mut kids: Vec<&str> = children.iter().copied().collect();
                kids.sort_by_key(|cid| name_for(discovered, cid).to_lowercase());
                for cid in kids.into_iter().rev() { stack.push((cid, depth + 1)); }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use nmp_core::substrate::KernelEvent;
    use nmp_nip29::kinds::KIND_CHAT_MESSAGE;
    use nmp_nip29::projection::{GroupChatMessage, GroupMemberRow};

    fn group(id: &str, parent: Option<&str>, children: &[&str]) -> DiscoveredGroup {
        DiscoveredGroup {
            group_id: id.to_string(), host_relay_url: "wss://h".to_string(),
            name: Some(id.to_string()), picture: None, about: None,
            member_count: 3, admin_count: 1, public: true, open: true,
            parent: parent.map(str::to_string), children: children.iter().map(|s| s.to_string()).collect(),
        }
    }
    fn snap() -> DiscoveredGroupsSnapshot {
        DiscoveredGroupsSnapshot { host_relay_url: "wss://h".to_string(),
            groups: vec![group("root", None, &["child"]), group("child", Some("root"), &[]), group("alpha", None, &[])] }
    }
    fn evt(id: &str, g: &str, ts: u64, c: &str) -> KernelEvent {
        KernelEvent { id: id.to_string(), author: "pk".to_string(), kind: KIND_CHAT_MESSAGE, created_at: ts,
            tags: vec![vec!["h".to_string(), g.to_string()]], content: c.to_string(), relay_provenance: Vec::new() }
    }
    fn make_app() -> (App, tokio::sync::mpsc::UnboundedReceiver<ProjectionView>) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<ProjectionView>();
        (App::new("wss://relay.example.com", tx), rx)
    }

    // --- existing tests (issue #11 baseline) ---

    #[test]
    fn tree_is_depth_annotated_and_alpha_ordered() {
        let items = derive_channel_tree(&snap(), &GroupTreeProjection::new().snapshot());
        let ids: Vec<_> = items.iter().map(|i| (i.local_id.as_str(), i.depth, i.is_branch)).collect();
        assert_eq!(ids, vec![("alpha", 0, false), ("root", 0, true), ("child", 1, false)]);
    }

    #[test]
    fn item_carries_unread_and_preview_from_group_tree() {
        let proj = GroupTreeProjection::new();
        proj.on_kernel_event(&evt("a", "child", 10, "hello"));
        proj.on_kernel_event(&evt("b", "child", 20, "newest"));
        let items = derive_channel_tree(&snap(), &proj.snapshot());
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
        let items = derive_channel_tree(&snap(), &GroupTreeProjection::new().snapshot());
        let result: Vec<_> = items.iter().map(|i| (i.local_id.as_str(), i.depth, i.is_branch)).collect();
        assert_eq!(result, vec![
            ("alpha", 0, false),
            ("root",  0, true),
            ("child", 1, false),
        ]);
    }

    /// T2: derive_channel_tree surfaces unread count and last-preview text
    /// from the GroupTreeProjection; preview is the verbatim event content.
    #[test]
    fn test_derive_channel_tree_unread_preview() {
        let proj = GroupTreeProjection::new();
        // Two messages on "child"; second is newer.
        proj.on_kernel_event(&evt("e1", "child", 100, "first message that is kind of long text here"));
        proj.on_kernel_event(&evt("e2", "child", 200, "short"));
        let items = derive_channel_tree(&snap(), &proj.snapshot());
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
            is_admin: true,
            my_pubkey: Some("pk1".to_string()),
            publish_outbox: vec![PublishOutboxItem {
                correlation_id: "cid".to_string(),
                group_local_id: "grp1".to_string(),
                content: "hi".to_string(),
                status: OutboxStatus::Pending,
                error: None,
            }],
            identity_state: IdentityState::LoggedIn { npub: "npub1test".to_string() },
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
        assert!(matches!(snap.identity_state, IdentityState::LoggedIn { ref npub } if npub == "npub1test"));
        assert!(matches!(snap.active_form, Some(FormKind::JoinWithCode(_))));
        assert!(snap.login_error.is_none());
    }

    /// T4: IdentityState state-machine — Default is LoggedOut; transitions
    /// between variants are correctly distinguishable.
    #[test]
    fn test_identity_state_transitions() {
        // Default is LoggedOut
        assert_eq!(IdentityState::default(), IdentityState::LoggedOut);
        // ProjectionView also defaults to LoggedOut
        assert_eq!(ProjectionView::default().identity_state, IdentityState::LoggedOut);

        // LoggedOut != LoggingIn != LoggedIn
        let logged_out = IdentityState::LoggedOut;
        let logging_in = IdentityState::LoggingIn;
        let logged_in = IdentityState::LoggedIn { npub: "npub1abc".to_string() };
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
            app.snapshot().selected_channel_id.as_ref().map(|g| g.local_id.as_str()),
            Some("channel-b"),
        );
    }

    /// T6: reconcile_outbox promotes a Pending item to Confirmed when a
    /// matching message (same trimmed content) appears in selected_messages.
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
        });

        // A message with matching content; no my_pubkey means the pubkey guard
        // defaults to `true`, so only the content needs to match.
        let matching_msg = GroupChatMessage {
            id: "event-1".to_string(),
            pubkey: "any-pubkey".to_string(),
            content: "hello world".to_string(),
            created_at: 12345,
            kind: 9,
        };
        let mut view = ProjectionView::default();
        view.selected_messages = vec![matching_msg];

        app.ingest_projection(view);

        let snap = app.snapshot();
        assert_eq!(snap.publish_outbox.len(), 1);
        assert_eq!(snap.publish_outbox[0].status, OutboxStatus::Confirmed,
            "pending item must become Confirmed once matching message arrives");
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
        assert_eq!(snap.publish_outbox[0].status, OutboxStatus::Failed,
            "a Failed item must NOT be re-confirmed by reconcile");
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
            depth: 0, unread: 0, member_count: 0, admin_count: 0,
            is_branch: false, last_preview: None, last_timestamp: None,
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
}
