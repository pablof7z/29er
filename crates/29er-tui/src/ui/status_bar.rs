//! Bottom status bar: connection dot, status text, active focus, key hints.

use crossterm::event::Event;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::actions::Action;
use crate::app::{AppState, Focus};
use crate::ui;
use crate::Component;

#[derive(Default)]
pub struct StatusBar {
    status: String,
    connected: bool,
    focus_label: &'static str,
}

impl StatusBar {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self, state: &AppState) {
        self.status = state.status.clone();
        self.connected = state.connected;
        self.focus_label = match state.focus {
            Focus::RoomList => "rooms",
            Focus::Input => "compose",
        };
    }
}

impl Component for StatusBar {
    fn draw(&mut self, f: &mut Frame, area: Rect) {
        let dot_color = if self.connected { ui::GREEN } else { ui::RED };
        let line = Line::from(vec![
            Span::raw(" "),
            Span::styled("\u{25cf}", Style::default().fg(dot_color)),
            Span::raw(" "),
            Span::styled(self.status.clone(), Style::default().fg(ui::TEXT)),
            Span::raw("  |  "),
            Span::styled(
                format!("focus: {}", self.focus_label),
                Style::default().fg(ui::SUBTEXT),
            ),
            Span::raw("  |  "),
            Span::styled(
                "Tab switch  \u{2191}/\u{2193} move  Enter open  Esc back/quit",
                Style::default().fg(ui::OVERLAY).add_modifier(Modifier::DIM),
            ),
        ]);
        f.render_widget(
            Paragraph::new(line).style(Style::default().bg(ui::SURFACE0)),
            area,
        );
    }

    fn handle_event(&mut self, _event: &Event) -> Option<Action> {
        None
    }
}
