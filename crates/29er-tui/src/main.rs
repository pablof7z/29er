//! 29er-tui runtime loop. Separated from layout/rendering: this file owns the
//! async event loop and input routing only; all rendering lives in the
//! `ui::*` components and all NMP/business logic lives in `App`.

use std::time::Duration;

use anyhow::Result;
use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind, KeyModifiers};
use futures::StreamExt;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::Frame;

use twentyniner_tui::actions::Action;
use twentyniner_tui::app::{App, Focus};
use twentyniner_tui::terminal::TerminalHandle;
use twentyniner_tui::ui::chat::ChatComponent;
use twentyniner_tui::ui::room_list::RoomListComponent;
use twentyniner_tui::ui::status_bar::StatusBar;
use twentyniner_tui::Component;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    run().await
}

async fn run() -> Result<()> {
    // RAII: constructing this installs the panic hook + enters the alternate
    // screen; dropping it (normal or early return) restores the terminal.
    let mut terminal = TerminalHandle::new()?;

    let relay = std::env::var("NMP_RELAY").unwrap_or_else(|_| "wss://groups.0xchat.com".to_string());
    let mut app = App::new(relay);
    if let Err(err) = app.init_nmp() {
        app.set_status(format!("offline ({err})"));
    }

    let mut room_list = RoomListComponent::new();
    let mut chat = ChatComponent::new();
    let mut status_bar = StatusBar::new();

    let mut reader = EventStream::new();
    let mut ticker = tokio::time::interval(Duration::from_millis(120));

    loop {
        // Refresh view-models from the latest projection snapshots, then draw.
        let state = app.snapshot();
        room_list.update(&state);
        chat.update(&state);
        status_bar.update(&state);
        terminal.draw(|f| draw(f, &mut room_list, &mut chat, &mut status_bar))?;

        tokio::select! {
            _ = ticker.tick() => {}
            maybe_event = reader.next() => match maybe_event {
                Some(Ok(event)) => handle_event(&event, &mut app, &mut room_list, &mut chat),
                Some(Err(_)) | None => app.quit(),
            },
        }

        if app.should_quit() {
            break;
        }
    }

    terminal.clear().ok();
    Ok(())
}

fn draw(
    f: &mut Frame,
    room_list: &mut RoomListComponent,
    chat: &mut ChatComponent,
    status_bar: &mut StatusBar,
) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(f.area());

    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(24), Constraint::Min(1)])
        .split(root[0]);

    room_list.draw(f, main[0]);
    chat.draw(f, main[1]);
    status_bar.draw(f, root[1]);
}

fn handle_event(
    event: &Event,
    app: &mut App,
    room_list: &mut RoomListComponent,
    chat: &mut ChatComponent,
) {
    if let Event::Key(key) = event {
        if key.kind != KeyEventKind::Press {
            return;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
            app.quit();
            return;
        }
        match key.code {
            KeyCode::Tab => {
                app.toggle_focus();
                return;
            }
            KeyCode::Esc => {
                if app.focus() == Focus::Input {
                    app.set_focus(Focus::RoomList);
                } else {
                    app.quit();
                }
                return;
            }
            KeyCode::Char('q') if app.focus() == Focus::RoomList => {
                app.quit();
                return;
            }
            _ => {}
        }
    }

    let action = match app.focus() {
        Focus::RoomList => room_list.handle_event(event),
        Focus::Input => chat.handle_event(event),
    };
    if let Some(action) = action {
        apply(action, app);
    }
}

fn apply(action: Action, app: &mut App) {
    match action {
        Action::Quit => app.quit(),
        Action::NavigateUp => app.navigate(-1),
        Action::NavigateDown => app.navigate(1),
        Action::ToggleFocus => app.toggle_focus(),
        Action::SelectRoom(group_id) => app.select_room(group_id),
        Action::SendMessage(body) => app.send_message(body),
        Action::Tick => {}
    }
}
