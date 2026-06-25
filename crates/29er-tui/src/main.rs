//! Runtime loop + full wiring (issues #5, #10). Owns the Screen state machine,
//! the 4Hz projection mpsc, modal-aware input routing, and the only `apply`.
use std::time::Duration;
use anyhow::Result;
use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind, KeyModifiers};
use futures::StreamExt;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::Frame;
use tokio::sync::mpsc;
use twentyniner_tui::actions::Action;
use twentyniner_tui::app::{App, Focus, ProjectionView, Screen, TuiSnapshot};
use twentyniner_tui::terminal::TerminalHandle;
use twentyniner_tui::ui::chat::ChatComponent;
use twentyniner_tui::ui::composer::Composer;
use twentyniner_tui::ui::login::LoginComponent;
use twentyniner_tui::ui::membership::Membership;
use twentyniner_tui::ui::palette::Palette;
use twentyniner_tui::ui::room_list::RoomListComponent;
use twentyniner_tui::ui::status_bar::StatusBar;
use twentyniner_tui::Component;

struct Ui { login: LoginComponent, room_list: RoomListComponent, chat: ChatComponent, composer: Composer, palette: Palette, membership: Membership, status_bar: StatusBar }

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> { run().await }

async fn run() -> Result<()> {
    let mut terminal = TerminalHandle::new()?;
    let relay = std::env::var("NMP_RELAY").unwrap_or_else(|_| "wss://nip29.f7z.io".to_string());
    let (poll_tx, mut poll_rx) = mpsc::unbounded_channel::<ProjectionView>();
    let mut app = App::new(relay, poll_tx);
    let mut ui = Ui { login: LoginComponent::new(), room_list: RoomListComponent::new(), chat: ChatComponent::new(), composer: Composer::new(), palette: Palette::new(), membership: Membership::new(), status_bar: StatusBar::new() };
    let mut reader = EventStream::new();
    let mut ticker = tokio::time::interval(Duration::from_millis(120));
    loop {
        let state = app.snapshot();
        ui.login.update(&state); ui.room_list.update(&state); ui.chat.update(&state); ui.composer.update(&state); ui.palette.update(&state); ui.membership.update(&state); ui.status_bar.update(&state);
        terminal.draw(|f| draw(f, &state, &mut ui))?;
        tokio::select! {
            _ = ticker.tick() => {}
            Some(view) = poll_rx.recv() => app.ingest_projection(view),
            maybe = reader.next() => match maybe {
                Some(Ok(event)) => handle_event(&event, &mut app, &mut ui),
                Some(Err(_)) | None => app.quit(),
            },
        }
        if app.should_quit() { break; }
    }
    terminal.clear().ok();
    Ok(())
}

fn draw(f: &mut Frame, s: &TuiSnapshot, ui: &mut Ui) {
    match s.screen {
        Screen::Login => { ui.login.draw(f, f.area()); return; }
        Screen::App => {}
    }
    let composer_h: u16 = if s.selected_channel_id.is_some() { 6 } else { 0 };
    let root = Layout::default().direction(Direction::Vertical).constraints([Constraint::Min(1), Constraint::Length(composer_h), Constraint::Length(1)]).split(f.area());
    let main = Layout::default().direction(Direction::Horizontal).constraints([Constraint::Length(28), Constraint::Min(1)]).split(root[0]);
    ui.room_list.draw(f, main[0]);
    ui.chat.draw(f, main[1]);
    if composer_h > 0 { ui.composer.draw(f, root[1]); }
    ui.status_bar.draw(f, root[2]);
    if s.palette_open { ui.palette.draw(f, f.area()); }
    if s.active_form.is_some() { ui.membership.draw(f, f.area()); }
}

fn handle_event(event: &Event, app: &mut App, ui: &mut Ui) {
    if let Event::Key(key) = event {
        if key.kind == KeyEventKind::Press && key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) { app.quit(); return; }
    }
    if app.screen() == Screen::Login { if let Some(a) = ui.login.handle_event(event) { apply(a, app); } return; }
    // Modal precedence: form first, then palette.
    let form_open = app.active_form().is_some();
    if form_open { if let Some(a) = ui.membership.handle_event(event) { apply(a, app); } return; }
    if app.palette_open() {
        if let Some(a) = ui.palette.handle_event(event) {
            let keep_for_form = matches!(a, Action::OpenForm(_));
            apply(a, app);
            if !keep_for_form { app.set_palette(false); }
        }
        return;
    }
    if let Event::Key(key) = event {
        if key.kind == KeyEventKind::Press {
            if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('k')) { app.set_palette(true); return; }
            match key.code {
                KeyCode::Char('/') if app.focus() != Focus::Composer => { app.set_palette(true); return; }
                KeyCode::Char('n') if app.focus() != Focus::Composer => { app.set_focus(Focus::Composer); return; }
                KeyCode::Tab => { app.cycle_focus(); return; }
                KeyCode::Char('q') if app.focus() == Focus::ChannelList => { app.quit(); return; }
                KeyCode::Esc => { if app.focus() == Focus::ChannelList { app.quit(); } else { app.set_focus(Focus::ChannelList); } return; }
                _ => {}
            }
        }
    }
    let action = match app.focus() {
        Focus::ChannelList => ui.room_list.handle_event(event),
        Focus::Chat => ui.chat.handle_event(event),
        Focus::Composer => ui.composer.handle_event(event),
    };
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
        Action::OpenForm(f) => { app.set_palette(false); app.open_form(f); }
        Action::CloseForm => app.close_form(),
        Action::Noop => {}
    }
}
