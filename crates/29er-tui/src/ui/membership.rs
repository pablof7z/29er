//! Membership + admin forms (issue #9). Forms emit typed Actions; App dispatches.
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;
use nmp_nip29::projection::GroupMemberRow;
use crate::actions::Action;
use crate::app::{FormKind, TuiSnapshot};
use crate::ui;
use crate::Component;

#[derive(Default)]
pub struct Membership {
    form: Option<FormKind>,
    is_admin: bool,
    fields: Vec<String>,
    field: usize,
}
impl Membership {
    pub fn new() -> Self { Self::default() }
    pub fn update(&mut self, s: &TuiSnapshot) {
        let changed = match (&self.form, &s.active_form) {
            (None, Some(_)) => true,
            (Some(_), None) => true,
            (Some(a), Some(b)) => std::mem::discriminant(a) != std::mem::discriminant(b),
            (None, None) => false,
        };
        self.form = s.active_form.clone();
        self.is_admin = s.is_admin;
        if changed { self.fields = self.empty_fields(); self.field = 0; }
    }
    fn empty_fields(&self) -> Vec<String> {
        match &self.form {
            Some(FormKind::PutUser(_)) => vec![String::new(), String::new()],
            Some(_) => vec![String::new()],
            None => Vec::new(),
        }
    }
    pub fn is_open(&self) -> bool { self.form.is_some() }
    fn labels(&self) -> (&'static str, Vec<&'static str>) {
        match &self.form {
            Some(FormKind::JoinWithCode(_)) => ("Join channel", vec!["invite code (optional)"]),
            Some(FormKind::CreateInvite(_)) => ("Create invite", vec!["codes (comma-separated)"]),
            Some(FormKind::CreateChild(_)) => ("Create child channel", vec!["channel name"]),
            Some(FormKind::PutUser(_)) => ("Add role / put user", vec!["target pubkey (hex)", "role (optional)"]),
            Some(FormKind::MoveChannel(_)) => ("Move channel", vec!["new parent id (empty = root)"]),
            None => ("", vec![]),
        }
    }
    /// Pure mapping of the current form + buffers to a typed Action (issue #9 AC).
    fn submit(&self) -> Option<Action> {
        let f0 = self.fields.first().cloned().unwrap_or_default();
        let f0 = f0.trim().to_string();
        match &self.form {
            Some(FormKind::JoinWithCode(g)) => Some(Action::Join { group: g.clone(), invite_code: if f0.is_empty() { None } else { Some(f0) } }),
            Some(FormKind::CreateInvite(g)) if self.is_admin => {
                let codes: Vec<String> = f0.split(',').map(|c| c.trim().to_string()).filter(|c| !c.is_empty()).collect();
                Some(Action::CreateInvite { group: g.clone(), codes })
            }
            Some(FormKind::CreateChild(g)) if self.is_admin => { if f0.is_empty() { None } else { Some(Action::CreateChild { parent: g.clone(), name: f0 }) } }
            Some(FormKind::PutUser(g)) if self.is_admin => {
                if f0.is_empty() { return None; }
                let role = self.fields.get(1).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
                Some(Action::PutUser { group: g.clone(), target_pubkey: f0, role })
            }
            Some(FormKind::MoveChannel(g)) if self.is_admin => Some(Action::MoveChannel { group: g.clone(), parent: if f0.is_empty() { None } else { Some(f0) } }),
            _ => None,
        }
    }
    pub fn draw_members(&self, f: &mut Frame, area: Rect, members: &[GroupMemberRow]) {
        let block = Block::default().title(" members ").borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(ui::OVERLAY0));
        let items: Vec<ListItem> = if members.is_empty() {
            vec![ListItem::new(Line::from(Span::styled("no members loaded", Style::default().fg(ui::SUBTEXT0))))]
        } else {
            members.iter().map(|m| {
                let mut spans = vec![Span::styled(m.display_name.clone().filter(|n| !n.is_empty()).unwrap_or_else(|| ui::short_pubkey(&m.pubkey)), Style::default().fg(ui::TEXT))];
                if m.admin { spans.push(Span::styled(" \u{2605}", Style::default().fg(ui::YELLOW))); }
                if let Some(role) = &m.role { spans.push(Span::styled(format!("  {role}"), Style::default().fg(ui::OVERLAY0))); }
                ListItem::new(Line::from(spans))
            }).collect()
        };
        f.render_widget(List::new(items).block(block), area);
    }
}
impl Component for Membership {
    fn draw(&mut self, f: &mut Frame, area: Rect) {
        if self.form.is_none() { return; }
        let (title, labels) = self.labels();
        let admin_blocked = matches!(self.form, Some(FormKind::CreateInvite(_)) | Some(FormKind::CreateChild(_)) | Some(FormKind::PutUser(_)) | Some(FormKind::MoveChannel(_))) && !self.is_admin;
        let rows = labels.len() as u16 * 3 + 4;
        let v = Layout::default().direction(Direction::Vertical).constraints([Constraint::Percentage(30), Constraint::Length(rows), Constraint::Min(0)]).split(area);
        let h = Layout::default().direction(Direction::Horizontal).constraints([Constraint::Percentage(20), Constraint::Percentage(60), Constraint::Min(0)]).split(v[1]);
        let modal = h[1];
        f.render_widget(Clear, modal);
        let block = Block::default().title(format!(" {title} ")).borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(ui::MAUVE));
        let inner = block.inner(modal);
        f.render_widget(block, modal);
        let mut constraints: Vec<Constraint> = labels.iter().map(|_| Constraint::Length(3)).collect();
        constraints.push(Constraint::Length(1));
        let field_areas = Layout::default().direction(Direction::Vertical).constraints(constraints).split(inner);
        for (i, label) in labels.iter().enumerate() {
            let focused = i == self.field;
            let bs = if focused { Style::default().fg(ui::LAVENDER) } else { Style::default().fg(ui::OVERLAY0) };
            let val = self.fields.get(i).cloned().unwrap_or_default();
            let fb = Block::default().title(format!(" {label} ")).borders(Borders::ALL).border_style(bs);
            f.render_widget(Paragraph::new(Line::from(Span::styled(val, Style::default().fg(ui::TEXT)))).block(fb), field_areas[i]);
        }
        let hint = if admin_blocked { Span::styled("admin only \u{2014} you are not an admin", Style::default().fg(ui::RED)) } else { Span::styled("Enter submit \u{2022} Tab next field \u{2022} Esc cancel", Style::default().fg(ui::SUBTEXT0)) };
        if let Some(last) = field_areas.last() { f.render_widget(Paragraph::new(Line::from(hint)), *last); }
    }
    fn handle_event(&mut self, event: &Event) -> Option<Action> {
        if self.form.is_none() { return None; }
        let Event::Key(key) = event else { return None };
        if key.kind != KeyEventKind::Press { return None; }
        match key.code {
            KeyCode::Esc => Some(Action::CloseForm),
            KeyCode::Enter => self.submit(),
            KeyCode::Tab | KeyCode::Down => { if !self.fields.is_empty() { self.field = (self.field + 1) % self.fields.len(); } None }
            KeyCode::Up => { if !self.fields.is_empty() { self.field = (self.field + self.fields.len() - 1) % self.fields.len(); } None }
            KeyCode::Char(c) => { if let Some(buf) = self.fields.get_mut(self.field) { buf.push(c); } None }
            KeyCode::Backspace => { if let Some(buf) = self.fields.get_mut(self.field) { buf.pop(); } None }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nmp_nip29::GroupId;
    fn g() -> GroupId { GroupId::new("wss://h", "room") }
    #[test]
    fn join_form_emits_join_with_optional_code() {
        let mut m = Membership::new();
        m.form = Some(FormKind::JoinWithCode(g())); m.fields = vec!["code1".into()];
        match m.submit() { Some(Action::Join { group, invite_code }) => { assert_eq!(group.local_id, "room"); assert_eq!(invite_code.as_deref(), Some("code1")); } other => panic!("{other:?}") }
    }
    #[test]
    fn create_child_requires_admin_and_name() {
        let mut m = Membership::new();
        m.form = Some(FormKind::CreateChild(g())); m.fields = vec!["Sub".into()];
        assert!(m.submit().is_none(), "non-admin must not submit");
        m.is_admin = true;
        match m.submit() { Some(Action::CreateChild { parent, name }) => { assert_eq!(parent.local_id, "room"); assert_eq!(name, "Sub"); } other => panic!("{other:?}") }
    }
    #[test]
    fn put_user_maps_pubkey_and_role() {
        let mut m = Membership::new();
        m.form = Some(FormKind::PutUser(g())); m.is_admin = true; m.fields = vec!["deadbeef".into(), "admin".into()];
        match m.submit() { Some(Action::PutUser { target_pubkey, role, .. }) => { assert_eq!(target_pubkey, "deadbeef"); assert_eq!(role.as_deref(), Some("admin")); } other => panic!("{other:?}") }
    }
}
