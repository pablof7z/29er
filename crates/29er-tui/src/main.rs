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
use twentyniner_tui::ui::help::HelpOverlay;
use twentyniner_tui::ui::login::LoginComponent;
use twentyniner_tui::ui::membership::Membership;
use twentyniner_tui::ui::palette::Palette;
use twentyniner_tui::ui::room_list::RoomListComponent;
use twentyniner_tui::ui::status_bar::StatusBar;
use twentyniner_tui::Component;

struct Ui {
    login: LoginComponent,
    room_list: RoomListComponent,
    chat: ChatComponent,
    composer: Composer,
    palette: Palette,
    membership: Membership,
    status_bar: StatusBar,
    help: HelpOverlay,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> { run().await }

async fn run() -> Result<()> {
    let mut terminal = TerminalHandle::new()?;
    let relay = std::env::var("NMP_RELAY").unwrap_or_else(|_| "wss://nip29.f7z.io".to_string());
    let (poll_tx, mut poll_rx) = mpsc::unbounded_channel::<ProjectionView>();
    let mut app = App::new(relay, poll_tx);
    let mut ui = Ui {
        login: LoginComponent::new(),
        room_list: RoomListComponent::new(),
        chat: ChatComponent::new(),
        composer: Composer::new(),
        palette: Palette::new(),
        membership: Membership::new(),
        status_bar: StatusBar::new(),
        help: HelpOverlay::new(),
    };
    let mut reader = EventStream::new();
    let mut ticker = tokio::time::interval(Duration::from_millis(120));
    loop {
        let state = app.snapshot();
        ui.login.update(&state);
        ui.room_list.update(&state);
        ui.chat.update(&state);
        ui.composer.update(&state);
        ui.palette.update(&state);
        ui.membership.update(&state);
        ui.status_bar.update(&state);
        ui.help.update(&state);
        terminal.draw(|f| draw(f, &state, &mut ui))?;
        tokio::select! {
            _ = ticker.tick() => { app.tick(); }
            Some(view) = poll_rx.recv() => app.ingest_projection(view),
            maybe = reader.next() => match maybe {
                Some(Ok(event)) => {
                    handle_event(&event, &mut app, &mut ui);
                    // After any user action, immediately pull a fresh projection
                    // so the next frame reflects the latest state without waiting
                    // for the 4 Hz background poller.
                    app.refresh_projection();
                }
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
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(composer_h), Constraint::Length(1)])
        .split(f.area());
    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(28), Constraint::Min(1)])
        .split(root[0]);
    ui.room_list.draw(f, main[0]);
    ui.chat.draw(f, main[1]);
    if composer_h > 0 { ui.composer.draw(f, root[1]); }
    ui.status_bar.draw(f, root[2]);
    // Overlay layers: palette > form > help
    if s.palette_open { ui.palette.draw(f, f.area()); }
    if s.active_form.is_some() { ui.membership.draw(f, f.area()); }
    if s.help_open { ui.help.draw(f, f.area()); }
}

fn handle_event(event: &Event, app: &mut App, ui: &mut Ui) {
    // Ctrl+C and Ctrl+Q always quit, regardless of any overlay or focus.
    if let Event::Key(key) = event {
        if key.kind == KeyEventKind::Press && key.modifiers.contains(KeyModifiers::CONTROL) {
            if matches!(key.code, KeyCode::Char('c') | KeyCode::Char('q')) {
                app.quit();
                return;
            }
        }
    }

    // Login screen: forward everything to the login component.
    if app.screen() == Screen::Login {
        if let Some(a) = ui.login.handle_event(event) { apply(a, app); }
        return;
    }

    // Help overlay: only Esc / ? dismisses it; all other input is swallowed.
    if app.is_help_open() {
        if let Some(a) = ui.help.handle_event(event) { apply(a, app); }
        return;
    }

    // Route by current focus: Modal > Palette > base panels.
    match app.focus() {
        Focus::Modal => {
            // Form/modal: membership component handles everything; Esc closes.
            if let Some(a) = ui.membership.handle_event(event) { apply(a, app); }
            return;
        }
        Focus::Palette => {
            // Palette: handle its input; after each action check if still open.
            if let Some(a) = ui.palette.handle_event(event) {
                apply(a, app);
                // If the action didn't close the palette (e.g. navigating within it),
                // close it now only if it emitted a non-navigation action.
                // ClosePalette/OpenForm both close via apply → set_palette(false) /
                // open_form() respectively, so the focus is already updated.
            }
            return;
        }
        _ => {}
    }

    // --- Base-panel global keys ---
    if let Event::Key(key) = event {
        if key.kind == KeyEventKind::Press {
            // Alt+A: jump to next mention channel.
            if key.modifiers.contains(KeyModifiers::ALT) && key.code == KeyCode::Char('a') {
                app.jump_to_next_mention();
                return;
            }
            // Ctrl+K or '/' (outside Composer) opens palette.
            let open_palette = (key.modifiers.contains(KeyModifiers::CONTROL)
                && key.code == KeyCode::Char('k'))
                || (key.code == KeyCode::Char('/') && app.focus() != Focus::Composer);
            if open_palette {
                app.set_palette(true);
                return;
            }
            match key.code {
                // '?' opens help (outside Composer to avoid conflicting with typing).
                KeyCode::Char('?') if app.focus() != Focus::Composer => {
                    app.open_help();
                    return;
                }
                // 'n' focuses the composer from any base panel.
                KeyCode::Char('n') if app.focus() != Focus::Composer => {
                    app.set_focus(Focus::Composer);
                    return;
                }
                // Tab / Shift+Tab cycle through base panels.
                KeyCode::Tab => { app.cycle_focus(); return; }
                KeyCode::BackTab => { app.reverse_cycle_focus(); return; }
                // 'q' quits from the room list.
                KeyCode::Char('q') if app.focus() == Focus::RoomList => {
                    app.quit();
                    return;
                }
                // 'g' / 'G' jump to top / bottom of room list.
                KeyCode::Char('g') if app.focus() == Focus::RoomList => {
                    app.navigate_top();
                    return;
                }
                KeyCode::Char('G') if app.focus() == Focus::RoomList => {
                    app.navigate_bottom();
                    return;
                }
                // Esc: pop the focus stack, or quit from RoomList.
                KeyCode::Esc => {
                    if app.focus() == Focus::RoomList {
                        app.quit();
                    } else if !app.pop_focus() {
                        app.set_focus(Focus::RoomList);
                    }
                    return;
                }
                _ => {}
            }
        }
    }

    // Delegate to the focused panel component.
    let action = match app.focus() {
        Focus::RoomList => ui.room_list.handle_event(event),
        Focus::Chat => ui.chat.handle_event(event),
        Focus::Composer => ui.composer.handle_event(event),
        Focus::Palette | Focus::Modal => None, // already handled above
    };
    if let Some(a) = action { apply(a, app); }
}

fn apply(action: Action, app: &mut App) {
    match action {
        Action::Quit => app.quit(),
        Action::LoginSubmit(nsec) => app.login(nsec),
        // navigation
        Action::NavigateUp => app.navigate(-1),
        Action::NavigateDown => app.navigate(1),
        Action::NavigateTop => app.navigate_top(),
        Action::NavigateBottom => app.navigate_bottom(),
        Action::SelectChannel(g) => app.select_channel(g),
        Action::CycleFocus => app.cycle_focus(),
        Action::ReverseCycleFocus => app.reverse_cycle_focus(),
        Action::SetFocus(f) => app.set_focus(f),
        // chat scroll
        Action::ScrollUp => app.scroll_messages(1),
        Action::ScrollDown => app.scroll_messages(-1),
        // chat / outbox
        Action::SendMessage(b) => app.send_message(b),
        Action::RetryOutbox(id) => app.retry_outbox(id),
        // palette
        Action::OpenPalette => app.set_palette(true),
        Action::ClosePalette => app.set_palette(false),
        // help
        Action::OpenHelp => app.open_help(),
        Action::CloseHelp => app.close_help(),
        // membership / admin
        Action::Join { group, invite_code } => app.join(group, invite_code),
        Action::Leave { group } => app.leave(group),
        Action::ShowMembers(g) => app.show_members(g),
        Action::CreateInvite { group, codes } => app.create_invite(group, codes),
        Action::PutUser { group, target_pubkey, role } => app.put_user(group, target_pubkey, role),
        Action::CreateChild { parent, name } => app.create_child(parent, name),
        Action::MoveChannel { group, parent } => app.move_channel(group, parent),
        // forms — open_form handles palette collapse + focus stack internally.
        Action::OpenForm(f) => app.open_form(f),
        Action::CloseForm => app.close_form(),
        Action::JumpToNextMention => app.jump_to_next_mention(),
        Action::Noop => {}
    }
}
