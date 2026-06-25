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
pub struct StatusBar { connected: bool, focus_label: &'static str, identity: String }
impl StatusBar {
    pub fn new() -> Self { Self::default() }
    pub fn update(&mut self, s: &TuiSnapshot) {
        self.connected = matches!(s.relay_state, RelayState::Connected);
        self.focus_label = match s.focus { Focus::ChannelList => "channels", Focus::Chat => "chat", Focus::Composer => "compose" };
        self.identity = match &s.identity_state { IdentityState::LoggedIn { npub } => ui::short_pubkey(npub), IdentityState::LoggingIn => "signing in\u{2026}".to_string(), IdentityState::LoggedOut => "offline".to_string() };
    }
}
impl Component for StatusBar {
    fn draw(&mut self, f: &mut Frame, area: Rect) {
        let dot = if self.connected { ui::GREEN } else { ui::RED };
        let line = Line::from(vec![
            Span::raw(" "), Span::styled("\u{25cf}", Style::default().fg(dot)), Span::raw(" "),
            Span::styled(self.identity.clone(), Style::default().fg(ui::TEXT)),
            Span::raw("  |  "), Span::styled(format!("focus: {}", self.focus_label), Style::default().fg(ui::SUBTEXT)),
            Span::raw("  |  "), Span::styled("Tab switch  Enter open  /  palette  q quit", Style::default().fg(ui::OVERLAY).add_modifier(Modifier::DIM)),
        ]);
        f.render_widget(Paragraph::new(line).style(Style::default().bg(ui::SURFACE0)), area);
    }
    fn handle_event(&mut self, _event: &Event) -> Option<Action> { None }
}
