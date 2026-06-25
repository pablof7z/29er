//! Left sidebar (foundation pass). Renders the derived channel tree flat with
//! indentation; full styling lands in Wave 2 Task A.
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState};
use ratatui::Frame;
use crate::actions::Action;
use crate::app::{ChannelListItem, Focus, TuiSnapshot};
use crate::ui;
use crate::Component;

#[derive(Default)]
pub struct RoomListComponent { items: Vec<ChannelListItem>, selected: usize, focused: bool, state: ListState }
impl RoomListComponent {
    pub fn new() -> Self { Self::default() }
    pub fn update(&mut self, s: &TuiSnapshot) {
        self.items = s.channel_tree.clone();
        self.selected = s.selected_index;
        self.focused = s.focus == Focus::ChannelList;
        self.state.select(if self.items.is_empty() { None } else { Some(self.selected) });
    }
}
impl Component for RoomListComponent {
    fn draw(&mut self, f: &mut Frame, area: Rect) {
        let bs = if self.focused { Style::default().fg(ui::MAUVE) } else { Style::default().fg(ui::OVERLAY) };
        let block = Block::default().title(" channels ").borders(Borders::ALL).border_type(BorderType::Rounded).border_style(bs);
        let rows: Vec<ListItem> = if self.items.is_empty() {
            vec![ListItem::new(Line::from(Span::styled("discovering\u{2026}", Style::default().fg(ui::SUBTEXT))))]
        } else {
            self.items.iter().map(|it| {
                let indent = "  ".repeat(it.depth);
                let mut spans = vec![Span::raw(indent), Span::styled(it.name.clone(), Style::default().fg(ui::TEXT))];
                if it.unread > 0 { spans.push(Span::raw(" ")); spans.push(Span::styled(format!("{}", it.unread), Style::default().fg(ui::RED).add_modifier(Modifier::BOLD))); }
                ListItem::new(Line::from(spans))
            }).collect()
        };
        let list = List::new(rows).block(block).highlight_style(Style::default().bg(ui::SURFACE0).add_modifier(Modifier::BOLD)).highlight_symbol("> ");
        f.render_stateful_widget(list, area, &mut self.state);
    }
    fn handle_event(&mut self, event: &Event) -> Option<Action> {
        let Event::Key(key) = event else { return None };
        if key.kind != KeyEventKind::Press { return None; }
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => Some(Action::NavigateDown),
            KeyCode::Up | KeyCode::Char('k') => Some(Action::NavigateUp),
            KeyCode::Enter => self.items.get(self.selected).map(|it| Action::SelectChannel(it.group_id.clone())),
            _ => None,
        }
    }
}
