//! Runtime loop: Screen state machine (Login/App), 4Hz projection mpsc, input
//! routing, and the only `apply` that mutates `App`.
use std::time::Duration;
use anyhow::Result;
use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind, KeyModifiers};
use futures::StreamExt;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::Frame;
use tokio::sync::mpsc;
use twentyniner_tui::actions::Action;
use twentyniner_tui::app::{App, Focus, ProjectionView, Screen};
use twentyniner_tui::terminal::TerminalHandle;
use twentyniner_tui::ui::chat::ChatComponent;
use twentyniner_tui::ui::login::LoginComponent;
use twentyniner_tui::ui::room_list::RoomListComponent;
use twentyniner_tui::ui::status_bar::StatusBar;
use twentyniner_tui::Component;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> { run().await }

async fn run() -> Result<()> {
    let mut terminal = TerminalHandle::new()?;
    let relay = std::env::var("NMP_RELAY").unwrap_or_else(|_| "wss://nip29.f7z.io".to_string());
    let (poll_tx, mut poll_rx) = mpsc::unbounded_channel::<ProjectionView>();
    let mut app = App::new(relay, poll_tx);
    let mut login = LoginComponent::new();
    let mut room_list = RoomListComponent::new();
    let mut chat = ChatComponent::new();
    let mut status_bar = StatusBar::new();
    let mut reader = EventStream::new();
    let mut ticker = tokio::time::interval(Duration::from_millis(120));
    loop {
        let state = app.snapshot();
        login.update(&state); room_list.update(&state); chat.update(&state); status_bar.update(&state);
        terminal.draw(|f| draw(f, &state.screen, &mut login, &mut room_list, &mut chat, &mut status_bar))?;
        tokio::select! {
            _ = ticker.tick() => {}
            Some(view) = poll_rx.recv() => app.ingest_projection(view),
            maybe = reader.next() => match maybe {
                Some(Ok(event)) => handle_event(&event, &mut app, &mut login, &mut room_list, &mut chat),
                Some(Err(_)) | None => app.quit(),
            },
        }
        if app.should_quit() { break; }
    }
    terminal.clear().ok();
    Ok(())
}

fn draw(f: &mut Frame, screen: &Screen, login: &mut LoginComponent, room_list: &mut RoomListComponent, chat: &mut ChatComponent, status_bar: &mut StatusBar) {
    match screen {
        Screen::Login => login.draw(f, f.area()),
        Screen::App => {
            let root = Layout::default().direction(Direction::Vertical).constraints([Constraint::Min(1), Constraint::Length(1)]).split(f.area());
            let main = Layout::default().direction(Direction::Horizontal).constraints([Constraint::Length(28), Constraint::Min(1)]).split(root[0]);
            room_list.draw(f, main[0]); chat.draw(f, main[1]); status_bar.draw(f, root[1]);
        }
    }
}

fn handle_event(event: &Event, app: &mut App, login: &mut LoginComponent, room_list: &mut RoomListComponent, chat: &mut ChatComponent) {
    if let Event::Key(key) = event {
        if key.kind != KeyEventKind::Press { return; }
        if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) { app.quit(); return; }
    }
    if app.screen() == Screen::Login { if let Some(a) = login.handle_event(event) { apply(a, app); } return; }
    if let Event::Key(key) = event {
        if key.kind == KeyEventKind::Press {
            match key.code {
                KeyCode::Tab => { app.cycle_focus(); return; }
                KeyCode::Char('q') if app.focus() == Focus::ChannelList => { app.quit(); return; }
                KeyCode::Esc => { if app.focus() == Focus::ChannelList { app.quit(); } else { app.set_focus(Focus::ChannelList); } return; }
                _ => {}
            }
        }
    }
    let action = match app.focus() { Focus::ChannelList => room_list.handle_event(event), Focus::Chat | Focus::Composer => chat.handle_event(event) };
    if let Some(a) = action { apply(a, app); }
}

fn apply(action: Action, app: &mut App) {
    match action {
        Action::Quit => app.quit(),
        Action::LoginSubmit(nsec) => app.login(nsec),
        Action::NavigateUp => app.navigate(-1),
        Action::NavigateDown => app.navigate(1),
        Action::SelectChannel(g) => app.select_channel(g),
        Action::CycleFocus => app.cycle_focus(),
        Action::SetFocus(f) => app.set_focus(f),
        Action::ScrollUp => app.scroll_messages(1),
        Action::ScrollDown => app.scroll_messages(-1),
        Action::SendMessage(b) => app.send_message(b),
        Action::RetryOutbox(id) => app.retry_outbox(id),
        Action::OpenPalette => app.set_palette(true),
        Action::ClosePalette => app.set_palette(false),
        Action::Join { group, invite_code } => app.join(group, invite_code),
        Action::Leave { group } => app.leave(group),
        Action::ShowMembers(g) => app.show_members(g),
        Action::CreateInvite { group, codes } => app.create_invite(group, codes),
        Action::PutUser { group, target_pubkey, role } => app.put_user(group, target_pubkey, role),
        Action::CreateChild { parent, name } => app.create_child(parent, name),
        Action::MoveChannel { group, parent } => app.move_channel(group, parent),
        Action::OpenForm(f) => app.open_form(f),
        Action::CloseForm => app.close_form(),
        Action::Noop => {}
    }
}
