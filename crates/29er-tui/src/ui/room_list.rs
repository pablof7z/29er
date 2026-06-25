//! Left sidebar: hierarchical NIP-29 channel list with indentation, unread
//! badges, member counts, and last-message preview + timestamp (issue #4).
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState};
use ratatui::Frame;
use nmp_nip29::GroupId;
use crate::actions::Action;
use crate::app::{ChannelListItem, Focus, TuiSnapshot};
use crate::ui;
use crate::Component;

#[derive(Default)]
pub struct RoomListComponent {
    items: Vec<ChannelListItem>,
    selected: usize,
    selected_channel: Option<GroupId>,
    focused: bool,
    state: ListState,
}

impl RoomListComponent {
    pub fn new() -> Self { Self::default() }
    pub fn update(&mut self, s: &TuiSnapshot) {
        self.items = s.channel_tree.clone();
        self.selected = s.selected_index;
        self.selected_channel = s.selected_channel_id.clone();
        self.focused = s.focus == Focus::ChannelList;
        self.state.select(if self.items.is_empty() { None } else { Some(self.selected) });
    }
    fn row(&self, it: &ChannelListItem) -> ListItem<'static> {
        let indent = "  ".repeat(it.depth);
        let is_selected_channel = self.selected_channel.as_ref().map(|g| g.local_id == it.local_id).unwrap_or(false);
        let name_color = if is_selected_channel { ui::LAVENDER } else { ui::TEXT };
        let glyph = if it.is_branch { "\u{25be} " } else { "  " };
        let mut header = vec![
            Span::raw(indent.clone()),
            Span::styled(glyph, Style::default().fg(ui::OVERLAY)),
            Span::styled(it.name.clone(), Style::default().fg(name_color).add_modifier(Modifier::BOLD)),
        ];
        if it.unread > 0 {
            header.push(Span::raw(" "));
            header.push(Span::styled(format!("({})", it.unread), Style::default().fg(ui::RED).add_modifier(Modifier::BOLD)));
        }
        let mut lines = vec![Line::from(header)];
        let mut meta = vec![
            Span::raw(indent.clone()),
            Span::raw("  "),
            Span::styled(format!("\u{1f465}{}", it.member_count), Style::default().fg(ui::SUBTEXT0)),
        ];
        if let Some(preview) = &it.last_preview {
            let trimmed: String = preview.chars().take(24).collect();
            meta.push(Span::raw("  "));
            meta.push(Span::styled(trimmed, Style::default().fg(ui::OVERLAY0)));
        }
        if let Some(ts) = it.last_timestamp {
            meta.push(Span::raw("  "));
            meta.push(Span::styled(ui::relative_time(ts), Style::default().fg(ui::OVERLAY0)));
        }
        lines.push(Line::from(meta));
        ListItem::new(lines)
    }
}

impl Component for RoomListComponent {
    fn draw(&mut self, f: &mut Frame, area: Rect) {
        let border_style = if self.focused { Style::default().fg(ui::MAUVE) } else { Style::default().fg(ui::OVERLAY0) };
        let block = Block::default().title(" channels ").borders(Borders::ALL).border_type(BorderType::Rounded).border_style(border_style);
        let items: Vec<ListItem> = if self.items.is_empty() {
            vec![ListItem::new(Line::from(Span::styled("discovering channels\u{2026}", Style::default().fg(ui::SUBTEXT0))))]
        } else {
            self.items.iter().map(|it| self.row(it)).collect()
        };
        let highlight = if self.focused {
            Style::default().bg(ui::SURFACE0).add_modifier(Modifier::BOLD)
        } else {
            Style::default().bg(ui::MANTLE)
        };
        let symbol = if self.focused { "> " } else { "  " };
        let list = List::new(items).block(block).highlight_style(highlight).highlight_symbol(symbol);
        f.render_stateful_widget(list, area, &mut self.state);
    }
    fn handle_event(&mut self, event: &Event) -> Option<Action> {
        let Event::Key(key) = event else { return None };
        if key.kind != KeyEventKind::Press { return None; }
        match key.code {
            KeyCode::Down | KeyCode::Char('j') | KeyCode::PageDown => Some(Action::NavigateDown),
            KeyCode::Up | KeyCode::Char('k') | KeyCode::PageUp => Some(Action::NavigateUp),
            KeyCode::Enter => self.items.get(self.selected).map(|it| Action::SelectChannel(it.group_id.clone())),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};
    fn key(code: KeyCode) -> Event { Event::Key(KeyEvent::new(code, KeyModifiers::NONE)) }
    #[test]
    fn arrows_and_vim_keys_map_to_navigation() {
        let mut c = RoomListComponent::new();
        assert!(matches!(c.handle_event(&key(KeyCode::Char('j'))), Some(Action::NavigateDown)));
        assert!(matches!(c.handle_event(&key(KeyCode::Up)), Some(Action::NavigateUp)));
    }
    #[test]
    fn enter_selects_channel_at_cursor() {
        let mut c = RoomListComponent::new();
        c.items = vec![ChannelListItem { group_id: GroupId::new("wss://h", "a"), local_id: "a".into(), name: "A".into(), depth: 0, unread: 0, member_count: 1, admin_count: 0, is_branch: false, last_preview: None, last_timestamp: None }];
        c.selected = 0;
        assert!(matches!(c.handle_event(&key(KeyCode::Enter)), Some(Action::SelectChannel(_))));
    }
}
