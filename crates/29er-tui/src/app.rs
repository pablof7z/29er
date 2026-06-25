//! NMP composition root + read-model snapshotting for the TUI shell.
//!
//! `App` owns the raw `*mut NmpApp` (the kernel runtime), the reusable
//! projections registered as `KernelEventObserver`s, and the navigation
//! state. `snapshot()` folds the projections into a plain [`AppState`] that
//! the render layer consumes. All writes are dispatched as typed actions.
//!
//! `App` is intentionally `!Send` (it holds raw pointers); it lives entirely
//! inside the single-threaded runtime loop and is never spawned onto another
//! thread.

use std::collections::HashMap;
use std::ffi::CString;
use std::sync::Arc;

use nmp_app_29er::group_tree::GroupTreeProjection;
use nmp_app_29er::{
    nmp_app_29er_declare_consumed_projections, nmp_app_29er_dispatch_action_bytes,
    nmp_app_29er_register, nmp_app_29er_register_group_chat, nmp_app_29er_unregister,
    TwentyNinerHandle,
};
use nmp_core::KernelEventObserver;
use nmp_ffi::{nmp_free_string, NmpApp};
use nmp_nip29::action::PostChatMessageInput;
use nmp_nip29::projection::{DiscoveredGroupsProjection, GroupChatMessage, GroupChatProjection};
use nmp_nip29::GroupId;

/// Which pane currently receives key input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    RoomList,
    Input,
}

/// One renderable room row, derived from discovery + group-tree projections.
#[derive(Clone, Debug)]
pub struct RoomEntry {
    pub group_id: GroupId,
    pub name: String,
    pub unread: u32,
    pub last_preview: Option<String>,
}

/// The immutable per-tick view-model handed to the UI. Contains zero behavior.
#[derive(Clone, Debug)]
pub struct AppState {
    pub rooms: Vec<RoomEntry>,
    pub selected_index: usize,
    pub selected_room: Option<GroupId>,
    pub messages: Vec<GroupChatMessage>,
    pub status: String,
    pub connected: bool,
    pub focus: Focus,
}

pub struct App {
    app_ptr: *mut NmpApp,
    handle: *mut TwentyNinerHandle,
    relay_url: String,
    group_tree: Arc<GroupTreeProjection>,
    discovered: Arc<DiscoveredGroupsProjection>,
    chats: HashMap<String, Arc<GroupChatProjection>>,
    rooms: Vec<RoomEntry>,
    selected_index: usize,
    selected_room: Option<GroupId>,
    focus: Focus,
    status: String,
    should_quit: bool,
}

impl App {
    pub fn new(relay_url: impl Into<String>) -> Self {
        let relay_url = relay_url.into();
        Self {
            app_ptr: std::ptr::null_mut(),
            handle: std::ptr::null_mut(),
            relay_url: relay_url.clone(),
            group_tree: Arc::new(GroupTreeProjection::new()),
            discovered: Arc::new(DiscoveredGroupsProjection::new(relay_url)),
            chats: HashMap::new(),
            rooms: Vec::new(),
            selected_index: 0,
            selected_room: None,
            focus: Focus::RoomList,
            status: "offline".to_string(),
            should_quit: false,
        }
    }

    /// Mirror `ffi.rs`'s init sequence natively: new -> set storage -> register
    /// (defaults + NIP-29 actions) -> declare consumed projections -> register
    /// our read-side projections as observers -> add relay -> start -> sign in
    /// -> kick off discovery. Returns `Err` (and leaves the app in demo mode)
    /// when `NMP_NSEC` is absent, so the binary still launches without secrets.
    pub fn init_nmp(&mut self) -> anyhow::Result<()> {
        let nsec = std::env::var("NMP_NSEC")
            .map_err(|_| anyhow::anyhow!("NMP_NSEC unset; running in demo mode"))?;
        let relay = self.relay_url.clone();

        let storage = std::env::temp_dir().join("29er-tui-store");
        std::fs::create_dir_all(&storage).ok();
        let storage_str = storage.to_string_lossy().into_owned();

        unsafe {
            let app = nmp_ffi::nmp_app_new();
            if app.is_null() {
                anyhow::bail!("nmp_app_new returned null");
            }

            let c_storage = CString::new(storage_str)?;
            nmp_ffi::nmp_app_set_storage_path(app, c_storage.as_ptr());

            let mut handle: *mut TwentyNinerHandle = std::ptr::null_mut();
            let status = nmp_app_29er_register(app, &mut handle as *mut *mut TwentyNinerHandle);
            if status != 0 {
                nmp_ffi::nmp_app_free(app);
                anyhow::bail!("nmp_app_29er_register failed (status {status})");
            }
            nmp_app_29er_declare_consumed_projections(app);

            // Register the read-side projections as kernel event observers.
            let app_ref = &*app;
            let _ = app_ref.register_event_observer(
                Arc::clone(&self.group_tree) as Arc<dyn KernelEventObserver>
            );
            let _ = app_ref.register_event_observer(
                Arc::clone(&self.discovered) as Arc<dyn KernelEventObserver>
            );

            let c_relay = CString::new(relay.clone())?;
            let c_role = CString::new("read")?;
            nmp_ffi::nmp_app_add_relay(app, c_relay.as_ptr(), c_role.as_ptr());
            nmp_ffi::nmp_app_start(app, 80, 4);

            let c_nsec = CString::new(nsec)?;
            nmp_ffi::nmp_app_signin_nsec(app, c_nsec.as_ptr(), 1);

            self.app_ptr = app;
            self.handle = handle;
        }

        let discover_body = serde_json::json!({ "relay_url": relay }).to_string();
        self.dispatch_json("nmp.nip29.discover", &discover_body);
        self.status = format!("connected: {relay}");
        Ok(())
    }

    /// Recompute the room list from the discovery + group-tree projections.
    fn refresh_rooms(&mut self) {
        let tree = self.group_tree.snapshot();
        let discovered = self.discovered.snapshot();
        let mut rooms: Vec<RoomEntry> = discovered
            .groups
            .into_iter()
            .map(|g| {
                let unread = tree.unread_for(&g.group_id);
                let last_preview = tree.last_message_for(&g.group_id).map(|m| m.preview.clone());
                let name = g
                    .name
                    .clone()
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| g.group_id.clone());
                let group_id = GroupId::new(g.host_relay_url, g.group_id);
                RoomEntry { group_id, name, unread, last_preview }
            })
            .collect();
        rooms.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        self.rooms = rooms;
        if self.rooms.is_empty() {
            self.selected_index = 0;
        } else if self.selected_index >= self.rooms.len() {
            self.selected_index = self.rooms.len() - 1;
        }
    }

    /// Produce the per-tick view-model. Pure read: snapshots projections only.
    pub fn snapshot(&mut self) -> AppState {
        self.refresh_rooms();
        let messages = self
            .selected_room
            .as_ref()
            .and_then(|r| self.chats.get(&r.local_id))
            .map(|c| c.snapshot().messages)
            .unwrap_or_default();
        AppState {
            rooms: self.rooms.clone(),
            selected_index: self.selected_index,
            selected_room: self.selected_room.clone(),
            messages,
            status: self.status.clone(),
            connected: !self.app_ptr.is_null(),
            focus: self.focus,
        }
    }

    /// Select a room: lazily build + register its chat projection and push the
    /// room's history/live interests through the per-app entry point.
    pub fn select_room(&mut self, group_id: GroupId) {
        let key = group_id.local_id.clone();
        if !self.chats.contains_key(&key) {
            let chat = Arc::new(GroupChatProjection::new(group_id.clone()));
            if !self.app_ptr.is_null() {
                unsafe {
                    let _ = (&*self.app_ptr).register_event_observer(
                        Arc::clone(&chat) as Arc<dyn KernelEventObserver>
                    );
                }
                if let Ok(json) = serde_json::to_string(&group_id) {
                    if let Ok(c) = CString::new(json) {
                        unsafe {
                            nmp_app_29er_register_group_chat(self.app_ptr, c.as_ptr());
                        }
                    }
                }
            }
            self.chats.insert(key, chat);
        }
        self.selected_room = Some(group_id);
        self.focus = Focus::Input;
    }

    /// Dispatch a kind:9 chat message into the selected room via the typed
    /// NIP-29 action namespace.
    pub fn send_message(&self, body: String) {
        let trimmed = body.trim();
        if trimmed.is_empty() {
            return;
        }
        let Some(room) = self.selected_room.clone() else {
            return;
        };
        let input = PostChatMessageInput {
            group: room,
            content: trimmed.to_string(),
            previous_event_id_prefixes: Vec::new(),
            reply_to_event_id: None,
            mention_pubkeys: Vec::new(),
        };
        if let Ok(json) = serde_json::to_string(&input) {
            self.dispatch_json("nmp.nip29.post_chat_message", &json);
        }
    }

    fn dispatch_json(&self, namespace: &str, body: &str) {
        if self.app_ptr.is_null() {
            return;
        }
        let (Ok(ns), Ok(body)) = (CString::new(namespace), CString::new(body)) else {
            return;
        };
        unsafe {
            let res = nmp_app_29er_dispatch_action_bytes(self.app_ptr, ns.as_ptr(), body.as_ptr());
            if !res.is_null() {
                nmp_free_string(res);
            }
        }
    }

    pub fn navigate(&mut self, delta: isize) {
        if self.rooms.is_empty() {
            return;
        }
        let len = self.rooms.len() as isize;
        self.selected_index = (self.selected_index as isize + delta).rem_euclid(len) as usize;
    }

    pub fn focus(&self) -> Focus {
        self.focus
    }

    pub fn set_focus(&mut self, focus: Focus) {
        self.focus = focus;
    }

    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::RoomList => Focus::Input,
            Focus::Input => Focus::RoomList,
        };
    }

    pub fn set_status(&mut self, status: impl Into<String>) {
        self.status = status.into();
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }
}

impl Drop for App {
    fn drop(&mut self) {
        unsafe {
            if !self.handle.is_null() {
                nmp_app_29er_unregister(self.handle);
            }
            if !self.app_ptr.is_null() {
                nmp_ffi::nmp_app_free(self.app_ptr);
            }
        }
    }
}
