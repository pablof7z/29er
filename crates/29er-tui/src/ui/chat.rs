//! Right pane (foundation pass). Lists messages newest-at-bottom; full author
//! coloring/scroll/loading land in Wave 2 Task B.
use crossterm::event::Event;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};
use ratatui::Frame;
use nmp_nip29::projection::GroupChatMessage;
use crate::actions::Action;
use crate::app::{Focus, TuiSnapshot};
use crate::ui;
use crate::Component;

pub struct ChatComponent { messages: Vec<GroupChatMessage>, title: String, has_room: bool, focused: bool }
impl Default for ChatComponent { fn default() -> Self { Self::new() } }
impl ChatComponent {
    pub fn new() -> Self { Self { messages: Vec::new(), title: " chat ".to_string(), has_room: false, focused: false } }
    pub fn update(&mut self, s: &TuiSnapshot) {
        self.messages = s.selected_messages.clone();
        self.focused = s.focus == Focus::Chat;
        self.has_room = s.selected_channel_id.is_some();
        self.title = s.selected_channel_id.as_ref().map(|g| format!(" {} ", g.local_id)).unwrap_or_else(|| " chat ".to_string());
    }
}
impl Component for ChatComponent {
    fn draw(&mut self, f: &mut Frame, area: Rect) {
        let bs = if self.focused { Style::default().fg(ui::MAUVE) } else { Style::default().fg(ui::OVERLAY) };
        let block = Block::default().title(self.title.clone()).borders(Borders::ALL).border_type(BorderType::Rounded).border_style(bs);
        let inner = block.inner(area);
        let mut lines: Vec<Line> = Vec::new();
        if !self.has_room { lines.push(Line::from(Span::styled("Select a channel to view its messages.", Style::default().fg(ui::SUBTEXT)))); }
        else if self.messages.is_empty() { lines.push(Line::from(Span::styled("No messages yet \u{2014} be the first to say something", Style::default().fg(ui::SUBTEXT)))); }
        else {
            for m in self.messages.iter().rev() {
                lines.push(Line::from(vec![
                    Span::styled(ui::short_pubkey(&m.pubkey), Style::default().fg(ui::author_color(&m.pubkey)).add_modifier(Modifier::BOLD)),
                    Span::raw("  "), Span::styled(ui::clock_time(m.created_at), Style::default().fg(ui::OVERLAY)),
                ]));
                lines.push(Line::from(Span::styled(m.content.clone(), Style::default().fg(ui::TEXT))));
                lines.push(Line::from(""));
            }
        }
        let total = lines.len() as u16; let scroll = total.saturating_sub(inner.height);
        f.render_widget(Paragraph::new(lines).block(block).wrap(Wrap { trim: false }).scroll((scroll, 0)), area);
    }
    fn handle_event(&mut self, _event: &Event) -> Option<Action> { None }
}
