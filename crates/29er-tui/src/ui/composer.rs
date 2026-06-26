//! Composer (issue #8): tui-textarea input, @mention popup sourced from the
//! selected group's members, and a publish-outbox strip with retry.
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;
use ratatui_textarea::{CursorMove, DataCursor, TextArea};
use nmp_nip29::projection::GroupMemberRow;
use crate::actions::Action;
use crate::app::{Focus, OutboxStatus, PublishOutboxItem, TuiSnapshot};
use crate::ui;
use crate::Component;

pub struct Composer {
    textarea: TextArea<'static>,
    members: Vec<GroupMemberRow>,
    outbox: Vec<PublishOutboxItem>,
    focused: bool,
    mention_open: bool,
    mention_sel: usize,
    mention_state: ListState,
}
impl Default for Composer { fn default() -> Self { Self::new() } }
impl Composer {
    pub fn new() -> Self {
        Self { textarea: Self::fresh(), members: Vec::new(), outbox: Vec::new(), focused: false, mention_open: false, mention_sel: 0, mention_state: ListState::default() }
    }
    fn fresh() -> TextArea<'static> {
        let mut ta = TextArea::default();
        ta.set_placeholder_text("Type a message \u{2014} Enter to send, Shift+Enter for newline");
        ta
    }
    fn reset(&mut self) { self.textarea = Self::fresh(); self.mention_open = false; self.mention_sel = 0; }
    pub fn update(&mut self, s: &TuiSnapshot) {
        self.members = s.selected_members.clone();
        self.outbox = s.publish_outbox.clone();
        self.focused = s.focus == Focus::Composer;
    }
    fn current_word(&self) -> String {
        let DataCursor(row, col) = self.textarea.cursor();
        let line = self.textarea.lines().get(row).cloned().unwrap_or_default();
        let upto: String = line.chars().take(col).collect();
        upto.rsplit(|c: char| c.is_whitespace()).next().unwrap_or("").to_string()
    }
    fn mention_query(&self) -> Option<String> {
        let w = self.current_word();
        w.strip_prefix('@').map(|s| s.to_lowercase())
    }
    fn matches(&self, query: &str) -> Vec<GroupMemberRow> {
        self.members.iter().filter(|m| {
            let name = m.display_name.clone().unwrap_or_default().to_lowercase();
            name.contains(query) || m.pubkey.to_lowercase().contains(query)
        }).take(6).cloned().collect()
    }
    fn label(m: &GroupMemberRow) -> String { m.display_name.clone().filter(|n| !n.is_empty()).unwrap_or_else(|| ui::short_pubkey(&m.pubkey)) }
    fn accept_mention(&mut self, m: &GroupMemberRow) {
        // delete the @partial token then insert the resolved handle
        let word = self.current_word();
        for _ in 0..word.chars().count() { self.textarea.delete_char(); }
        let handle = format!("@{} ", Self::label(m));
        for ch in handle.chars() { self.textarea.insert_char(ch); }
        self.mention_open = false; self.mention_sel = 0;
    }
    fn refresh_mention(&mut self) {
        match self.mention_query() {
            Some(q) if !self.matches(&q).is_empty() => {
                self.mention_open = true;
                let n = self.matches(&q).len();
                if self.mention_sel >= n { self.mention_sel = n.saturating_sub(1); }
                self.mention_state.select(Some(self.mention_sel));
            }
            _ => { self.mention_open = false; self.mention_sel = 0; }
        }
    }
    fn latest_failed(&self) -> Option<String> {
        self.outbox.iter().rev().find(|i| matches!(i.status, OutboxStatus::Failed)).map(|i| i.correlation_id.clone())
    }
    fn outbox_lines(&self) -> Vec<Line<'static>> {
        self.outbox.iter().rev().take(3).map(|it| {
            let (glyph, color) = match it.status {
                OutboxStatus::Pending => ("\u{23f3}", ui::PEACH),
                OutboxStatus::Confirmed => ("\u{2713}", ui::GREEN),
                OutboxStatus::Failed => ("\u{2717}", ui::RED),
            };
            let preview: String = it.content.chars().take(32).collect();
            let mut spans = vec![ Span::styled(format!("{glyph} "), Style::default().fg(color)), Span::styled(preview, Style::default().fg(ui::SUBTEXT0)) ];
            if matches!(it.status, OutboxStatus::Failed) { spans.push(Span::raw("  ")); spans.push(Span::styled("[Ctrl-R retry]", Style::default().fg(ui::RED).add_modifier(Modifier::BOLD))); }
            Line::from(spans)
        }).collect()
    }
}
impl Component for Composer {
    fn draw(&mut self, f: &mut Frame, area: Rect) {
        let outbox_lines = self.outbox_lines();
        let outbox_h = outbox_lines.len() as u16;
        let chunks = Layout::default().direction(Direction::Vertical).constraints([
            Constraint::Min(3), Constraint::Length(outbox_h),
        ]).split(area);
        let border_style = if self.focused { Style::default().fg(ui::MAUVE) } else { Style::default().fg(ui::OVERLAY0) };
        let block = Block::default().title(" message ").borders(Borders::ALL).border_type(BorderType::Rounded).border_style(border_style);
        self.textarea.set_block(block);
        f.render_widget(&self.textarea, chunks[0]);
        if outbox_h > 0 { f.render_widget(Paragraph::new(outbox_lines), chunks[1]); }
        if self.mention_open {
            if let Some(q) = self.mention_query() {
                let matches = self.matches(&q);
                if !matches.is_empty() {
                    let h = (matches.len() as u16 + 2).min(8);
                    let popup = Rect { x: chunks[0].x, y: chunks[0].y.saturating_sub(h), width: chunks[0].width.min(40), height: h };
                    let items: Vec<ListItem> = matches.iter().map(|m| {
                        let admin = if m.admin { " \u{2605}" } else { "" };
                        ListItem::new(Line::from(vec![ Span::styled(Self::label(m), Style::default().fg(ui::TEXT)), Span::styled(admin.to_string(), Style::default().fg(ui::YELLOW)) ]))
                    }).collect();
                    let list = List::new(items)
                        .block(Block::default().title(" mention ").borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(ui::LAVENDER)))
                        .highlight_style(Style::default().bg(ui::SURFACE0).add_modifier(Modifier::BOLD)).highlight_symbol("> ");
                    f.render_widget(Clear, popup);
                    self.mention_state.select(Some(self.mention_sel.min(matches.len().saturating_sub(1))));
                    f.render_stateful_widget(list, popup, &mut self.mention_state);
                }
            }
        }
    }
    fn handle_event(&mut self, event: &Event) -> Option<Action> {
        let Event::Key(key) = event else { return None };
        if key.kind != KeyEventKind::Press { return None; }
        if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('r') | KeyCode::Char('R')) {
            return self.latest_failed().map(Action::RetryOutbox);
        }
        if self.mention_open {
            match key.code {
                KeyCode::Up => { self.mention_sel = self.mention_sel.saturating_sub(1); return None; }
                KeyCode::Down => { self.mention_sel = self.mention_sel.saturating_add(1); self.refresh_mention(); return None; }
                KeyCode::Tab | KeyCode::Enter => {
                    if let Some(q) = self.mention_query() {
                        let matches = self.matches(&q);
                        if let Some(m) = matches.get(self.mention_sel.min(matches.len().saturating_sub(1))).cloned() { self.accept_mention(&m); }
                    }
                    return None;
                }
                KeyCode::Esc => { self.mention_open = false; return None; }
                _ => {}
            }
        }
        match key.code {
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => { self.textarea.insert_newline(); self.refresh_mention(); None }
            KeyCode::Enter => {
                let text = self.textarea.lines().join("\n").trim().to_string();
                if text.is_empty() { None } else { self.reset(); Some(Action::SendMessage(text)) }
            }
            KeyCode::Char(c) => { self.textarea.insert_char(c); self.refresh_mention(); None }
            KeyCode::Backspace => { self.textarea.delete_char(); self.refresh_mention(); None }
            KeyCode::Left => { self.textarea.move_cursor(CursorMove::Back); self.refresh_mention(); None }
            KeyCode::Right => { self.textarea.move_cursor(CursorMove::Forward); self.refresh_mention(); None }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};
    fn ch(c: char) -> Event { Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)) }
    fn enter() -> Event { Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)) }
    fn member(name: &str, pk: &str, admin: bool) -> GroupMemberRow { GroupMemberRow { pubkey: pk.to_string(), display_name: Some(name.to_string()), admin, role: None } }
    #[test]
    fn enter_emits_send_with_trimmed_text() {
        let mut c = Composer::new();
        c.handle_event(&ch('h')); c.handle_event(&ch('i'));
        assert!(matches!(c.handle_event(&enter()), Some(Action::SendMessage(t)) if t == "hi"));
    }
    #[test]
    fn empty_enter_does_not_send() {
        let mut c = Composer::new();
        assert!(c.handle_event(&enter()).is_none());
    }
    #[test]
    fn at_token_opens_mention_and_filters_members() {
        let mut c = Composer::new();
        c.members = vec![member("alice", "aa", false), member("bob", "bb", true)];
        c.handle_event(&ch('@')); c.handle_event(&ch('a'));
        assert!(c.mention_open);
        assert_eq!(c.matches("a").len(), 1);
    }
    #[test]
    fn ctrl_r_retries_latest_failed() {
        let mut c = Composer::new();
        c.outbox = vec![PublishOutboxItem { correlation_id: "29er-1".into(), group_local_id: "g".into(), content: "x".into(), status: OutboxStatus::Failed, error: None }];
        let ev = Event::Key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL));
        assert!(matches!(c.handle_event(&ev), Some(Action::RetryOutbox(id)) if id == "29er-1"));
    }
}
