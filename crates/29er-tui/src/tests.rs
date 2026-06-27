//! Ratatui TestBackend layout snapshot tests (issue #11).
//!
//! Each test constructs a minimal `TuiSnapshot`, updates the relevant
//! component, renders to a `TestBackend`, and asserts that key strings are
//! present in the cell buffer — verifying both "no panic" and correct output.

use ratatui::backend::TestBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::Terminal;

use crate::app::{
    ChannelListItem, ChannelTier, Focus, IdentityState, RelayState, Screen, TuiSnapshot,
};
use crate::ui::chat::ChatComponent;
use crate::ui::login::LoginComponent;
use crate::ui::room_list::RoomListComponent;
use crate::ui::status_bar::StatusBar;
use crate::Component;
use nmp_nip29::projection::GroupChatMessage;
use nmp_nip29::GroupId;

// ── helpers ──────────────────────────────────────────────────────────────────

/// Render `component` into a fresh `w×h` TestBackend and return all cell
/// symbols concatenated row-by-row (rows separated by `\n`).
fn render_component<C: Component>(c: &mut C, w: u16, h: u16) -> String {
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    t.draw(|f| c.draw(f, f.area())).unwrap();
    let buf = t.backend().buffer().clone();
    (0..h)
        .map(|y| {
            (0..w)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn fake_msg(pk: &str, ts: u64, content: &str) -> GroupChatMessage {
    GroupChatMessage {
        id: format!("{pk}-{ts}"),
        pubkey: pk.to_string(),
        content: content.to_string(),
        created_at: ts,
        kind: 9,
    }
}

fn fake_channel(id: &str) -> ChannelListItem {
    ChannelListItem {
        group_id: GroupId::new("wss://relay.test", id),
        local_id: id.to_string(),
        name: id.to_string(),
        depth: 0,
        unread: 0,
        member_count: 5,
        admin_count: 1,
        is_branch: false,
        last_preview: None,
        last_timestamp: None,
        tier: ChannelTier::Normal,
    }
}

fn base_snapshot() -> TuiSnapshot {
    TuiSnapshot {
        channel_tree: vec![],
        selected_channel_id: None,
        selected_messages: vec![],
        selected_members: vec![],
        profiles: Default::default(),
        is_admin: false,
        my_pubkey: None,
        publish_outbox: vec![],
        identity_state: IdentityState::LoggedOut,
        relay_state: RelayState::Disconnected,
        errors: vec![],
        selected_index: 0,
        focus: Focus::RoomList,
        message_scroll: 0,
        palette_open: false,
        active_form: None,
        login_error: None,
        screen: Screen::App,
        help_open: false,
        status_message: None,
        last_read_message_id: None,
        spinner_tick: 0,
        connecting_since: None,
        connected_at: None,
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

/// `RoomListComponent` renders channel items without panicking and the border
/// title "channels" appears in the output.
#[test]
fn test_room_list_renders_without_panic() {
    let mut c = RoomListComponent::new();
    let mut snap = base_snapshot();
    snap.channel_tree = vec![fake_channel("general"), fake_channel("dev")];
    c.update(&snap);
    let out = render_component(&mut c, 80, 24);
    assert!(
        out.chars().any(|ch| !ch.is_whitespace()),
        "buffer should not be blank"
    );
    assert!(
        out.contains("channels"),
        "border title 'channels' missing from room list"
    );
    assert!(
        out.contains("general"),
        "channel name 'general' missing from room list"
    );
}

/// `ChatComponent` renders `GroupChatMessage` content into the cell buffer.
#[test]
fn test_chat_renders_messages() {
    let mut c = ChatComponent::new();
    let mut snap = base_snapshot();
    snap.selected_channel_id = Some(GroupId::new("wss://relay.test", "general"));
    snap.relay_state = RelayState::Connected;
    snap.selected_messages = vec![fake_msg("aabbccdd1122", 1_000_000, "hello from test")];
    c.update(&snap);
    let out = render_component(&mut c, 80, 24);
    assert!(
        out.contains("hello from test"),
        "message content missing from chat buffer"
    );
}

/// Rendering both panels side-by-side using the main horizontal split
/// produces content for the room-list (left) and chat (right) panels.
#[test]
fn test_layout_both_panels_present() {
    let mut room_list = RoomListComponent::new();
    let mut chat = ChatComponent::new();
    let mut snap = base_snapshot();
    snap.channel_tree = vec![fake_channel("lobby")];
    snap.selected_channel_id = Some(GroupId::new("wss://relay.test", "lobby"));
    snap.relay_state = RelayState::Connected;
    room_list.update(&snap);
    chat.update(&snap);

    let mut t = Terminal::new(TestBackend::new(180, 48)).unwrap();
    t.draw(|f| {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(28), Constraint::Min(1)])
            .split(f.area());
        room_list.draw(f, cols[0]);
        chat.draw(f, cols[1]);
    })
    .unwrap();

    let buf = t.backend().buffer().clone();
    let out: String = (0..48_u16)
        .flat_map(|y| (0..180_u16).map(move |x| (x, y)))
        .map(|(x, y)| buf[(x, y)].symbol().to_string())
        .collect();

    assert!(
        out.contains("channels"),
        "left panel (room list) missing 'channels' title"
    );
    // The right panel renders the chat border; when no messages are present it
    // shows the "No messages yet" placeholder or the channel id.
    assert!(
        out.contains("chat") || out.contains("lobby") || out.contains("No messages"),
        "right panel (chat) produced no recognisable content"
    );
}

/// `StatusBar` shows key-hint text when rendered to a single-row terminal.
#[test]
fn test_status_bar_shows_keybinds() {
    let mut bar = StatusBar::new();
    let snap = base_snapshot();
    bar.update(&snap);
    // StatusBar is 1 row tall — use a wide single-row backend.
    let out = render_component(&mut bar, 200, 1);
    assert!(
        out.contains("j/k") || out.contains("move") || out.contains("quit"),
        "key-hint text missing from status bar: {out:?}"
    );
}

/// `LoginComponent` renders a form that contains the word "nsec".
#[test]
fn test_login_screen_renders() {
    let mut c = LoginComponent::new();
    let mut snap = base_snapshot();
    snap.screen = Screen::Login;
    c.update(&snap);
    let out = render_component(&mut c, 120, 30);
    assert!(
        out.contains("nsec"),
        "nsec prompt missing from login screen buffer"
    );
}
