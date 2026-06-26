//! Bottom status + context hint bar (issue #5).
use crossterm::event::Event;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use crate::actions::Action;
use crate::app::{Focus, IdentityState, RelayState, TuiSnapshot};
use crate::ui;
use crate::Component;

#[derive(Default)]
pub struct StatusBar { connected: bool, focus_label: &'static str, identity: String, hint: &'static str }
impl StatusBar {
    pub fn new() -> Self { Self::default() }
    pub fn update(&mut self, s: &TuiSnapshot) {
        self.connected = matches!(s.relay_state, RelayState::Connected);
        self.identity = match &s.identity_state { IdentityState::LoggedIn { npub } => ui::short_pubkey(npub), IdentityState::LoggingIn => "signing in\u{2026}".to_string(), IdentityState::LoggedOut => "offline".to_string() };
        self.focus_label = match s.focus {
            Focus::RoomList => "channels",
            Focus::Chat => "chat",
            Focus::Composer => "compose",
            Focus::Palette => "palette",
            Focus::Modal => "dialog",
        };
        self.hint = if s.help_open { "? or Esc closes help" }
            else if s.active_form.is_some() { "Enter submit  Tab next field  Esc cancel" }
            else if s.palette_open { "type to filter  Enter run  Esc close" }
            else { match s.focus {
                Focus::RoomList => "j/k/g/G move  Enter open  / palette  n compose  ? help  q quit",
                Focus::Chat => "j/k scroll  Tab next  n compose  ? help  Esc back",
                Focus::Composer => "Enter send  Shift+Enter newline  Esc back",
                Focus::Palette => "type to filter  Enter run  Esc close",
                Focus::Modal => "Enter submit  Tab next field  Esc cancel",
            } };
    }
}
impl Component for StatusBar {
    fn draw(&mut self, f: &mut Frame, area: Rect) {
        let dot = if self.connected { ui::GREEN } else { ui::RED };
        let line = Line::from(vec![
            Span::raw(" "), Span::styled("\u{25cf}", Style::default().fg(dot)), Span::raw(" "),
            Span::styled(self.identity.clone(), Style::default().fg(ui::TEXT)),
            Span::raw("  |  "), Span::styled(format!("focus: {}", self.focus_label), Style::default().fg(ui::SUBTEXT0)),
            Span::raw("  |  "), Span::styled(self.hint.to_string(), Style::default().fg(ui::OVERLAY0).add_modifier(Modifier::DIM)),
        ]);
        f.render_widget(Paragraph::new(line).style(Style::default().bg(ui::SURFACE0)), area);
    }
    fn handle_event(&mut self, _event: &Event) -> Option<Action> { None }
}
