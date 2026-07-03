//! Membership + admin forms (issue #9). Forms emit typed Actions; App dispatches.
//! Modal forms are rendered as tui-popup overlays with focus-trapped fields
//! and inline error display.
use crate::actions::Action;
use crate::app::GroupMemberRow;
use crate::app::{FormKind, TuiSnapshot};
use crate::ui;
use crate::Component;
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Widget};
use ratatui::Frame;
use tui_popup::{KnownSize, Popup};

// ── FormBody: popup inner widget ──────────────────────────────────────────────

/// Body widget rendered inside the tui-popup overlay for all membership forms.
/// Implements both [`KnownSize`] (so the popup can auto-size itself) and
/// [`Widget`] (so it can be rendered by the popup).
struct FormBody {
    labels: Vec<&'static str>,
    fields: Vec<String>,
    focused_field: usize,
    admin_blocked: bool,
    /// Inline error set by the caller after a failed submission.
    error: Option<String>,
}

impl FormBody {
    /// Height of the body region (excluding the outer popup border).
    fn body_height(&self) -> usize {
        let error_lines = if self.error.is_some() { 1 } else { 0 };
        // each field = 3 rows (border top + content + border bottom)
        // +1 for the hint / error-blocked hint line
        self.labels.len() * 3 + error_lines + 1
    }
}

impl KnownSize for FormBody {
    /// Fixed inner width (popup adds 2 more for its border).
    fn width(&self) -> usize {
        54
    }
    fn height(&self) -> usize {
        self.body_height()
    }
}

impl Widget for FormBody {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let has_error = self.error.is_some();
        let mut constraints: Vec<Constraint> =
            self.labels.iter().map(|_| Constraint::Length(3)).collect();
        if has_error {
            constraints.push(Constraint::Length(1));
        }
        constraints.push(Constraint::Length(1)); // hint row

        let areas = Layout::vertical(constraints).split(area);

        // ── field inputs ──────────────────────────────────────────────────────
        for (i, label) in self.labels.iter().enumerate() {
            let focused = i == self.focused_field;
            let border_style = if focused {
                Style::default().fg(ui::LAVENDER)
            } else {
                Style::default().fg(ui::OVERLAY0)
            };
            let val = self.fields.get(i).cloned().unwrap_or_default();
            let block = Block::default()
                .title(format!(" {label} "))
                .borders(Borders::ALL)
                .border_style(border_style);
            Paragraph::new(Line::from(Span::styled(val, Style::default().fg(ui::TEXT))))
                .block(block)
                .render(areas[i], buf);
        }

        let mut next_row = self.labels.len();

        // ── inline error (red ✗ glyph) ────────────────────────────────────────
        if let Some(err) = self.error {
            Paragraph::new(Line::from(vec![
                Span::styled("\u{2717} ", Style::default().fg(ui::RED)),
                Span::styled(err, Style::default().fg(ui::RED)),
            ]))
            .render(areas[next_row], buf);
            next_row += 1;
        }

        // ── hint / blocked notice ─────────────────────────────────────────────
        let hint = if self.admin_blocked {
            Span::styled(
                "admin only \u{2014} you are not an admin",
                Style::default().fg(ui::RED),
            )
        } else {
            Span::styled(
                "Enter submit \u{2022} Tab next field \u{2022} Esc cancel",
                Style::default().fg(ui::SUBTEXT0),
            )
        };
        Paragraph::new(Line::from(hint)).render(areas[next_row], buf);
    }
}

// ── Membership component ──────────────────────────────────────────────────────

#[derive(Default)]
pub struct Membership {
    form: Option<FormKind>,
    is_admin: bool,
    fields: Vec<String>,
    field: usize,
    members: Vec<GroupMemberRow>,
    /// Inline error shown inside the popup after a failed submission.
    pub error: Option<String>,
}

impl Membership {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self, s: &TuiSnapshot) {
        let changed = match (&self.form, &s.active_form) {
            (None, Some(_)) => true,
            (Some(_), None) => true,
            (Some(a), Some(b)) => std::mem::discriminant(a) != std::mem::discriminant(b),
            (None, None) => false,
        };
        self.form = s.active_form.clone();
        self.is_admin = s.is_admin;
        self.members = s.selected_members.clone();
        if changed {
            self.fields = self.empty_fields();
            self.field = 0;
            self.error = None; // clear stale error when switching forms
        }
    }

    fn empty_fields(&self) -> Vec<String> {
        match &self.form {
            Some(FormKind::EditMetadata(_)) => {
                vec![String::new(), String::new(), String::new()]
            }
            Some(FormKind::PutUser(_)) => vec![String::new(), String::new()],
            Some(FormKind::AttachMedia(_)) => vec![
                String::new(),
                String::new(),
                nmp_app_29er::config::default_blossom_upload_servers().join(", "),
            ],
            Some(FormKind::ShowMembers(_)) => Vec::new(),
            Some(_) => vec![String::new()],
            None => Vec::new(),
        }
    }

    pub fn is_open(&self) -> bool {
        self.form.is_some()
    }

    fn labels(&self) -> (&'static str, Vec<&'static str>) {
        match &self.form {
            Some(FormKind::JoinWithCode(_)) => ("Join channel", vec!["invite code (optional)"]),
            Some(FormKind::CreateInvite(_)) => ("Create invite", vec!["codes (comma-separated)"]),
            Some(FormKind::CreateChild(_)) => ("Create child channel", vec!["channel name"]),
            Some(FormKind::EditMetadata(_)) => (
                "Edit room metadata",
                vec![
                    "name (optional)",
                    "description (optional)",
                    "picture URL (optional)",
                ],
            ),
            Some(FormKind::PutUser(_)) => (
                "Add role / put user",
                vec!["target pubkey (hex)", "role (optional)"],
            ),
            Some(FormKind::MoveChannel(_)) => {
                ("Move channel", vec!["new parent id (empty = root)"])
            }
            Some(FormKind::AttachMedia(_)) => (
                "Attach media",
                vec![
                    "local file path",
                    "content type (optional)",
                    "Blossom server URLs",
                ],
            ),
            Some(FormKind::ShowMembers(_)) => ("Members", vec![]),
            None => ("", vec![]),
        }
    }

    /// Pure mapping of the current form + buffers to a typed Action (issue #9 AC).
    fn submit(&self) -> Option<Action> {
        let f0 = self.fields.first().cloned().unwrap_or_default();
        let f0 = f0.trim().to_string();
        let optional_field = |index: usize| {
            self.fields
                .get(index)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        };
        match &self.form {
            Some(FormKind::JoinWithCode(g)) => Some(Action::Join {
                group: g.clone(),
                invite_code: if f0.is_empty() { None } else { Some(f0) },
            }),
            Some(FormKind::CreateInvite(g)) if self.is_admin => {
                let codes: Vec<String> = f0
                    .split(',')
                    .map(|c| c.trim().to_string())
                    .filter(|c| !c.is_empty())
                    .collect();
                Some(Action::CreateInvite {
                    group: g.clone(),
                    codes,
                })
            }
            Some(FormKind::CreateChild(g)) if self.is_admin => {
                if f0.is_empty() {
                    None
                } else {
                    Some(Action::CreateChild {
                        parent: g.clone(),
                        name: f0,
                    })
                }
            }
            Some(FormKind::EditMetadata(g)) if self.is_admin => {
                let name = optional_field(0);
                let about = optional_field(1);
                let picture = optional_field(2);
                if name.is_none() && about.is_none() && picture.is_none() {
                    return None;
                }
                Some(Action::EditMetadata {
                    group: g.clone(),
                    name,
                    about,
                    picture,
                })
            }
            Some(FormKind::PutUser(g)) if self.is_admin => {
                if f0.is_empty() {
                    return None;
                }
                let role = self
                    .fields
                    .get(1)
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                Some(Action::PutUser {
                    group: g.clone(),
                    target_pubkey: f0,
                    role,
                })
            }
            Some(FormKind::MoveChannel(g)) if self.is_admin => Some(Action::MoveChannel {
                group: g.clone(),
                parent: if f0.is_empty() { None } else { Some(f0) },
            }),
            Some(FormKind::AttachMedia(_)) => {
                if f0.is_empty() {
                    return None;
                }
                let content_type = self
                    .fields
                    .get(1)
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                let servers = self
                    .fields
                    .get(2)
                    .map(|s| parse_server_list(s))
                    .unwrap_or_default();
                if servers.is_empty() {
                    return None;
                }
                Some(Action::AttachMedia {
                    file_path: f0,
                    content_type,
                    servers,
                })
            }
            Some(FormKind::ShowMembers(_)) => None,
            _ => None,
        }
    }

    pub fn draw_members(&self, f: &mut Frame, area: Rect, members: &[GroupMemberRow]) {
        let max_w = area.width.saturating_sub(4).max(1);
        let max_h = area.height.saturating_sub(4).max(1);
        let modal_w = 66u16.min(max_w);
        let desired_h = (members.len() as u16).saturating_add(4).min(18);
        let modal_h = desired_h.max(8u16.min(max_h)).min(max_h);
        let vertical = Layout::vertical([
            Constraint::Length((area.height.saturating_sub(modal_h)) / 2),
            Constraint::Length(modal_h),
            Constraint::Min(0),
        ])
        .split(area);
        let horizontal = Layout::horizontal([
            Constraint::Length((area.width.saturating_sub(modal_w)) / 2),
            Constraint::Length(modal_w),
            Constraint::Min(0),
        ])
        .split(vertical[1]);
        let modal = horizontal[1];
        f.render_widget(Clear, modal);
        let block = Block::default()
            .title(" members  Esc close ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(ui::OVERLAY0));
        let items: Vec<ListItem> = if members.is_empty() {
            vec![ListItem::new(Line::from(Span::styled(
                "no members loaded",
                Style::default().fg(ui::SUBTEXT0),
            )))]
        } else {
            members
                .iter()
                .map(|m| {
                    let mut spans = vec![Span::styled(
                        m.display_name
                            .clone()
                            .filter(|n| !n.is_empty())
                            .unwrap_or_else(|| ui::short_pubkey(&m.pubkey)),
                        Style::default().fg(ui::TEXT),
                    )];
                    if m.admin {
                        spans.push(Span::styled(" \u{2605}", Style::default().fg(ui::YELLOW)));
                    }
                    if let Some(role) = &m.role {
                        spans.push(Span::styled(
                            format!("  {role}"),
                            Style::default().fg(ui::OVERLAY0),
                        ));
                    }
                    ListItem::new(Line::from(spans))
                })
                .collect()
        };
        f.render_widget(List::new(items).block(block), modal);
    }
}

fn parse_server_list(input: &str) -> Vec<String> {
    input
        .split(|c: char| c == ',' || c.is_whitespace())
        .map(str::trim)
        .filter(|server| !server.is_empty())
        .map(str::to_string)
        .collect()
}

impl Component for Membership {
    fn draw(&mut self, f: &mut Frame, area: Rect) {
        if self.form.is_none() {
            return;
        }
        if matches!(self.form, Some(FormKind::ShowMembers(_))) {
            self.draw_members(f, area, &self.members);
            return;
        }
        let (title, labels) = self.labels();
        let admin_blocked = matches!(
            self.form,
            Some(FormKind::CreateInvite(_))
                | Some(FormKind::CreateChild(_))
                | Some(FormKind::EditMetadata(_))
                | Some(FormKind::PutUser(_))
                | Some(FormKind::MoveChannel(_))
        ) && !self.is_admin;

        let body = FormBody {
            labels,
            fields: self.fields.clone(),
            focused_field: self.field,
            admin_blocked,
            error: self.error.clone(),
        };

        // tui-popup auto-centers, auto-clears background, and draws the border/title.
        let popup = Popup::new(body)
            .title(Line::from(format!(" {title} ")))
            .border_style(Style::default().fg(ui::MAUVE));

        f.render_widget(popup, area);
    }

    fn handle_event(&mut self, event: &Event) -> Option<Action> {
        if self.form.is_none() {
            return None;
        }
        let Event::Key(key) = event else { return None };
        if key.kind != KeyEventKind::Press {
            return None;
        }
        // Focus trap: all key events are consumed while a popup is open.
        match key.code {
            KeyCode::Esc => Some(Action::CloseForm),
            KeyCode::Enter => self.submit(),
            // Tab / Down cycle forward within popup fields only.
            KeyCode::Tab | KeyCode::Down => {
                if !self.fields.is_empty() {
                    self.field = (self.field + 1) % self.fields.len();
                }
                None
            }
            // Up / Shift+Tab cycle backward within popup fields.
            KeyCode::Up => {
                if !self.fields.is_empty() {
                    self.field = (self.field + self.fields.len() - 1) % self.fields.len();
                }
                None
            }
            KeyCode::Char(c) => {
                if let Some(buf) = self.fields.get_mut(self.field) {
                    buf.push(c);
                }
                None
            }
            KeyCode::Backspace => {
                if let Some(buf) = self.fields.get_mut(self.field) {
                    buf.pop();
                }
                None
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nmp_nip29::GroupId;
    fn g() -> GroupId {
        GroupId::new("wss://h", "room")
    }

    #[test]
    fn join_form_emits_join_with_optional_code() {
        let mut m = Membership::new();
        m.form = Some(FormKind::JoinWithCode(g()));
        m.fields = vec!["code1".into()];
        match m.submit() {
            Some(Action::Join { group, invite_code }) => {
                assert_eq!(group.local_id, "room");
                assert_eq!(invite_code.as_deref(), Some("code1"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn create_child_requires_admin_and_name() {
        let mut m = Membership::new();
        m.form = Some(FormKind::CreateChild(g()));
        m.fields = vec!["Sub".into()];
        assert!(m.submit().is_none(), "non-admin must not submit");
        m.is_admin = true;
        match m.submit() {
            Some(Action::CreateChild { parent, name }) => {
                assert_eq!(parent.local_id, "room");
                assert_eq!(name, "Sub");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn put_user_maps_pubkey_and_role() {
        let mut m = Membership::new();
        m.form = Some(FormKind::PutUser(g()));
        m.is_admin = true;
        m.fields = vec!["deadbeef".into(), "admin".into()];
        match m.submit() {
            Some(Action::PutUser {
                target_pubkey,
                role,
                ..
            }) => {
                assert_eq!(target_pubkey, "deadbeef");
                assert_eq!(role.as_deref(), Some("admin"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn edit_metadata_maps_room_fields() {
        let mut m = Membership::new();
        m.form = Some(FormKind::EditMetadata(g()));
        m.is_admin = true;
        m.fields = vec![
            "Renamed Room".into(),
            "Updated description".into(),
            "https://example.com/room.png".into(),
        ];
        match m.submit() {
            Some(Action::EditMetadata {
                group,
                name,
                about,
                picture,
            }) => {
                assert_eq!(group.local_id, "room");
                assert_eq!(name.as_deref(), Some("Renamed Room"));
                assert_eq!(about.as_deref(), Some("Updated description"));
                assert_eq!(picture.as_deref(), Some("https://example.com/room.png"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn attach_media_form_emits_path_mime_and_servers() {
        let mut m = Membership::new();
        m.form = Some(FormKind::AttachMedia(g()));
        m.fields = vec![
            "/tmp/photo.png".into(),
            "image/png".into(),
            "https://a.example, https://b.example".into(),
        ];
        match m.submit() {
            Some(Action::AttachMedia {
                file_path,
                content_type,
                servers,
            }) => {
                assert_eq!(file_path, "/tmp/photo.png");
                assert_eq!(content_type.as_deref(), Some("image/png"));
                assert_eq!(
                    servers,
                    vec![
                        "https://a.example".to_string(),
                        "https://b.example".to_string()
                    ]
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn edit_metadata_requires_admin_and_a_field() {
        let mut m = Membership::new();
        m.form = Some(FormKind::EditMetadata(g()));
        m.fields = vec!["Renamed Room".into(), String::new(), String::new()];
        assert!(m.submit().is_none(), "non-admin must not submit");
        m.is_admin = true;
        m.fields = vec![String::new(), String::new(), String::new()];
        assert!(m.submit().is_none(), "empty edit must not submit");
    }

    #[test]
    fn error_clears_on_form_change() {
        let mut m = Membership::new();
        m.form = Some(FormKind::JoinWithCode(g()));
        m.error = Some("something went wrong".into());
        // Simulate switching to a different form via update
        let mut snap = TuiSnapshot {
            channel_tree: vec![],
            selected_channel_id: None,
            selected_messages: vec![],
            selected_members: vec![],
            profiles: std::collections::HashMap::new(),
            event_envelopes: Default::default(),
            is_admin: false,
            my_pubkey: None,
            publish_outbox: vec![],
            identity_state: crate::app::IdentityState::LoggedOut,
            relay_state: crate::app::RelayState::Disconnected,
            errors: vec![],
            selected_index: 0,
            focus: crate::app::Focus::Modal,
            message_scroll: 0,
            palette_open: false,
            active_form: Some(FormKind::CreateInvite(g())),
            login_error: None,
            screen: crate::app::Screen::App,
            help_open: false,
            status_message: None,
            last_read_message_id: None,
            spinner_tick: 0,
            connecting_since: None,
            connected_at: None,
        };
        m.update(&snap);
        assert!(m.error.is_none(), "error must be cleared when form changes");
        // Switching to None also clears
        snap.active_form = None;
        m.update(&snap);
        assert!(m.error.is_none());
    }
}
