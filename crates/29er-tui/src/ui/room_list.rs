//! Left sidebar: the discovered/joined room list with unread badges and a
//! one-line last-message preview. Renders with ratatui's stateful `List`.

use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState};
use ratatui::Frame;

use crate::actions::Action;
use crate::app::{AppState, Focus, RoomEntry};
use crate::ui;
use crate::Component;

#[derive(Default)]
pub struct RoomListComponent {
    rooms: Vec<RoomEntry>,
    selected: usize,
    focused: bool,
    state: ListState,
}

impl RoomListComponent {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self, state: &AppState) {
        self.rooms = state.rooms.clone();
        self.selected = state.selected_index;
        self.focused = state.focus == Focus::RoomList;
        self.state
            .select(if self.rooms.is_empty() { None } else { Some(self.selected) });
    }
}

impl Component for RoomListComponent {
    fn draw(&mut self, f: &mut Frame, area: Rect) {
        let border_style = if self.focused {
            Style::default().fg(ui::MAUVE)
        } else {
            Style::default().fg(ui::OVERLAY)
        };
        let block = Block::default()
            .title(" rooms ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(border_style);

        let items: Vec<ListItem> = if self.rooms.is_empty() {
            vec![ListItem::new(Line::from(Span::styled(
                "no rooms yet\u{2026}",
                Style::default().fg(ui::SUBTEXT),
            )))]
        } else {
            self.rooms
                .iter()
                .map(|room| {
                    let mut header = vec![Span::styled(
                        room.name.clone(),
                        Style::default().fg(ui::TEXT),
                    )];
                    if room.unread > 0 {
                        header.push(Span::raw(" "));
                        header.push(Span::styled(
                            format!("({})", room.unread),
                            Style::default().fg(ui::GREEN).add_modifier(Modifier::BOLD),
                        ));
                    }
                    let mut lines = vec![Line::from(header)];
                    if let Some(preview) = &room.last_preview {
                        let trimmed: String = preview.chars().take(28).collect();
                        lines.push(Line::from(Span::styled(
                            trimmed,
                            Style::default().fg(ui::SUBTEXT),
                        )));
                    }
                    ListItem::new(lines)
                })
                .collect()
        };

        let list = List::new(items)
            .block(block)
            .highlight_style(
                Style::default()
                    .bg(ui::SURFACE0)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");

        f.render_stateful_widget(list, area, &mut self.state);
    }

    fn handle_event(&mut self, event: &Event) -> Option<Action> {
        let Event::Key(key) = event else {
            return None;
        };
        if key.kind != KeyEventKind::Press {
            return None;
        }
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => Some(Action::NavigateDown),
            KeyCode::Up | KeyCode::Char('k') => Some(Action::NavigateUp),
            KeyCode::Enter => self
                .rooms
                .get(self.selected)
                .map(|room| Action::SelectRoom(room.group_id.clone())),
            _ => None,
        }
    }
}
