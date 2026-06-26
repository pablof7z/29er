//! Right pane message history (issue #7): newest-at-bottom, per-author color,
//! own-message alignment, wrapping, scroll gutter, empty + loading states.
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};
use ratatui::Frame;
use nmp_nip29::projection::GroupChatMessage;
use crate::actions::Action;
use crate::app::{Focus, RelayState, TuiSnapshot};
use crate::ui;
use crate::Component;

pub struct ChatComponent {
    messages: Vec<GroupChatMessage>,
    my_pubkey: Option<String>,
    title: String,
    has_room: bool,
    focused: bool,
    connected: bool,
    scroll: u16,
    tick: usize,
}
impl Default for ChatComponent { fn default() -> Self { Self::new() } }
impl ChatComponent {
    pub fn new() -> Self {
        Self { messages: Vec::new(), my_pubkey: None, title: " chat ".to_string(), has_room: false, focused: false, connected: false, scroll: 0, tick: 0 }
    }
    pub fn update(&mut self, s: &TuiSnapshot) {
        self.messages = s.selected_messages.clone();
        self.my_pubkey = s.my_pubkey.clone();
        self.focused = matches!(s.focus, Focus::Chat | Focus::Composer);
        self.has_room = s.selected_channel_id.is_some();
        self.connected = matches!(s.relay_state, RelayState::Connected);
        self.scroll = s.message_scroll;
        self.title = s.selected_channel_id.as_ref().map(|g| format!(" {} ", g.local_id)).unwrap_or_else(|| " chat ".to_string());
    }
    fn is_own(&self, pubkey: &str) -> bool { self.my_pubkey.as_deref().map(|me| me == pubkey).unwrap_or(false) }
    fn render_message(&self, m: &GroupChatMessage) -> Vec<Line<'static>> {
        let own = self.is_own(&m.pubkey);
        let header = if own {
            Line::from(vec![
                Span::styled(ui::clock_time(m.created_at), Style::default().fg(ui::OVERLAY0)),
                Span::raw("  "),
                Span::styled("you", Style::default().fg(ui::GREEN).add_modifier(Modifier::BOLD)),
            ]).alignment(Alignment::Right)
        } else {
            Line::from(vec![
                Span::styled(ui::short_pubkey(&m.pubkey), Style::default().fg(ui::author_color(&m.pubkey)).add_modifier(Modifier::BOLD)),
                Span::raw("  "),
                Span::styled(ui::clock_time(m.created_at), Style::default().fg(ui::OVERLAY0)),
            ])
        };
        let body_color = if own { ui::SUBTEXT0 } else { ui::TEXT };
        let mut body = Line::from(Span::styled(m.content.clone(), Style::default().fg(body_color)));
        if own { body = body.alignment(Alignment::Right); }
        vec![header, body, Line::from("")]
    }
}
impl Component for ChatComponent {
    fn draw(&mut self, f: &mut Frame, area: Rect) {
        self.tick = self.tick.wrapping_add(1);
        let border_style = if self.focused { Style::default().fg(ui::MAUVE) } else { Style::default().fg(ui::OVERLAY0) };
        let block = Block::default().title(self.title.clone()).borders(Borders::ALL).border_type(BorderType::Rounded).border_style(border_style);
        let inner = block.inner(area);
        if !self.has_room {
            let p = Paragraph::new(Line::from(Span::styled("Select a channel to view its messages.", Style::default().fg(ui::SUBTEXT0)))).block(block).wrap(Wrap { trim: false });
            f.render_widget(p, area); return;
        }
        if self.messages.is_empty() {
            let line = if self.connected {
                Line::from(Span::styled("No messages yet \u{2014} be the first to say something", Style::default().fg(ui::SUBTEXT0)))
            } else {
                Line::from(vec![
                    Span::styled(format!("{} ", ui::spinner_frame(self.tick)), Style::default().fg(ui::MAUVE)),
                    Span::styled("Connecting to relay\u{2026}", Style::default().fg(ui::SUBTEXT0)),
                ])
            };
            let p = Paragraph::new(line).block(block).alignment(Alignment::Center).wrap(Wrap { trim: false });
            f.render_widget(p, area); return;
        }
        let mut lines: Vec<Line> = Vec::new();
        for m in self.messages.iter().rev() { lines.extend(self.render_message(m)); }
        let total = lines.len() as u16;
        let base = total.saturating_sub(inner.height);
        let scroll = base.saturating_sub(self.scroll);
        let hidden_above = scroll;
        if hidden_above > 0 {
            lines.insert(0, Line::from(Span::styled(format!("\u{2191} {hidden_above} more"), Style::default().fg(ui::OVERLAY0))));
        }
        let p = Paragraph::new(lines).block(block).wrap(Wrap { trim: false }).scroll((scroll, 0));
        f.render_widget(p, area);
    }
    fn handle_event(&mut self, event: &Event) -> Option<Action> {
        let Event::Key(key) = event else { return None };
        if key.kind != KeyEventKind::Press { return None; }
        // j/k mirror PageDown/PageUp for vim-style scrolling in the chat pane.
        match key.code {
            KeyCode::PageUp | KeyCode::Char('k') => Some(Action::ScrollUp),
            KeyCode::PageDown | KeyCode::Char('j') => Some(Action::ScrollDown),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    fn msg(pk: &str, ts: u64, c: &str) -> GroupChatMessage { GroupChatMessage { id: format!("{pk}{ts}"), pubkey: pk.to_string(), content: c.to_string(), created_at: ts, kind: 9 } }
    fn render(c: &mut ChatComponent, w: u16, h: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| c.draw(f, f.area())).unwrap();
        let buf = t.backend().buffer().clone();
        (0..h).map(|y| (0..w).map(|x| buf[(x, y)].symbol().to_string()).collect::<String>()).collect::<Vec<_>>().join("\n")
    }
    #[test]
    fn empty_state_shown_when_connected_and_no_messages() {
        let mut c = ChatComponent::new();
        c.update(&snap(vec![], true, Some("wss://h".into())));
        assert!(render(&mut c, 60, 12).contains("No messages yet"));
    }
    #[test]
    fn loading_spinner_shown_when_disconnected_and_empty() {
        let mut c = ChatComponent::new();
        c.update(&snap(vec![], false, Some("wss://h".into())));
        assert!(render(&mut c, 60, 12).contains("Connecting to relay"));
    }
    #[test]
    fn long_message_wraps_to_multiple_lines() {
        let mut c = ChatComponent::new();
        let long = "x ".repeat(80);
        c.update(&snap(vec![msg("abcd", 100, &long)], true, Some("wss://h".into())));
        let out = render(&mut c, 40, 12);
        assert!(out.matches('x').count() > 40);
    }
    fn snap(messages: Vec<GroupChatMessage>, connected: bool, room: Option<String>) -> TuiSnapshot {
        use crate::app::{IdentityState, RelayState, Screen};
        TuiSnapshot {
            channel_tree: vec![], selected_channel_id: room.map(|r| nmp_nip29::GroupId::new("wss://h", r)),
            selected_messages: messages, selected_members: vec![], is_admin: false, my_pubkey: None,
            publish_outbox: vec![], identity_state: IdentityState::LoggedOut,
            relay_state: if connected { RelayState::Connected } else { RelayState::Connecting },
            errors: vec![], selected_index: 0, focus: Focus::Chat, message_scroll: 0,
            palette_open: false, active_form: None, login_error: None, screen: Screen::App,
            help_open: false,
        }
    }
}
