//! Command palette (issue #6): fuzzy filter over channels + typed actions.
use crossterm::event::{Event, KeyCode, KeyEventKind};
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;
use crate::actions::Action;
use crate::app::{Focus, FormKind, TuiSnapshot};
use crate::ui;
use crate::Component;

#[derive(Clone)]
struct Entry {
    label: String,
    subtitle: String,
    action: Action,
}

#[derive(Default)]
pub struct Palette {
    query: String,
    entries: Vec<Entry>,
    filtered: Vec<usize>,
    selected: usize,
    state: ListState,
}

impl Palette {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self, s: &TuiSnapshot) {
        let mut entries: Vec<Entry> = Vec::new();

        // Channel entries from the channel tree
        for it in &s.channel_tree {
            entries.push(Entry {
                label: it.name.clone(),
                subtitle: format!("channel \u{2022} {}", it.last_preview.clone().unwrap_or_default()),
                action: Action::SelectChannel(it.group_id.clone()),
            });
        }

        // Action entries for the selected channel
        if let Some(g) = s.selected_channel_id.clone() {
            entries.push(Entry {
                label: "Compose message".into(),
                subtitle: "action".into(),
                action: Action::SetFocus(Focus::Composer),
            });
            entries.push(Entry {
                label: "Show members".into(),
                subtitle: "action".into(),
                action: Action::ShowMembers(g.clone()),
            });
            entries.push(Entry {
                label: "Join channel".into(),
                subtitle: "action".into(),
                action: Action::OpenForm(FormKind::JoinWithCode(g.clone())),
            });
            entries.push(Entry {
                label: "Leave channel".into(),
                subtitle: "action".into(),
                action: Action::Leave { group: g.clone() },
            });

            // Admin-only entries (issue #6 gating requirement)
            if s.is_admin {
                entries.push(Entry {
                    label: "Create invite".into(),
                    subtitle: "admin".into(),
                    action: Action::OpenForm(FormKind::CreateInvite(g.clone())),
                });
                entries.push(Entry {
                    label: "Create child channel".into(),
                    subtitle: "admin".into(),
                    action: Action::OpenForm(FormKind::CreateChild(g.clone())),
                });
                entries.push(Entry {
                    label: "Move channel".into(),
                    subtitle: "admin".into(),
                    action: Action::OpenForm(FormKind::MoveChannel(g.clone())),
                });
                entries.push(Entry {
                    label: "Add role / put user".into(),
                    subtitle: "admin".into(),
                    action: Action::OpenForm(FormKind::PutUser(g.clone())),
                });
            }
        }

        self.entries = entries;
        self.recompute();
    }

    fn recompute(&mut self) {
        self.filtered = self.filter();
        if self.filtered.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.filtered.len() {
            self.selected = self.filtered.len() - 1;
        }
        self.state
            .select(if self.filtered.is_empty() { None } else { Some(self.selected) });
    }

    /// Pure fuzzy filter (issue #6 AC). Empty query keeps original order.
    fn filter(&self) -> Vec<usize> {
        if self.query.trim().is_empty() {
            return (0..self.entries.len()).collect();
        }
        let mut matcher = Matcher::new(Config::DEFAULT);
        let pattern = Pattern::parse(self.query.trim(), CaseMatching::Ignore, Normalization::Smart);
        let mut scored: Vec<(u32, usize)> = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(i, e)| {
                let hay = format!("{} {}", e.label, e.subtitle);
                let mut buf = Vec::new();
                let utf32 = Utf32Str::new(&hay, &mut buf);
                pattern.score(utf32, &mut matcher).map(|score| (score, i))
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0));
        scored.into_iter().map(|(_, i)| i).collect()
    }

    fn selected_action(&self) -> Option<Action> {
        self.filtered
            .get(self.selected)
            .and_then(|&i| self.entries.get(i))
            .map(|e| e.action.clone())
    }
}

impl Component for Palette {
    fn draw(&mut self, f: &mut Frame, area: Rect) {
        // Center the modal: 70% width and height within the terminal area
        let v = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(15),
                Constraint::Percentage(70),
                Constraint::Min(0),
            ])
            .split(area);
        let h = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(15),
                Constraint::Percentage(70),
                Constraint::Min(0),
            ])
            .split(v[1]);
        let modal = h[1];

        // Clear the modal area first
        f.render_widget(Clear, modal);

        // Split modal into query bar and results list
        let inner = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(modal);

        // Query input line
        let qblock = Block::default()
            .title(" command palette ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(ui::MAUVE));
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("> ", Style::default().fg(ui::MAUVE)),
                Span::styled(self.query.clone(), Style::default().fg(ui::TEXT)),
            ]))
            .block(qblock),
            inner[0],
        );

        // Results list
        let items: Vec<ListItem> = self
            .filtered
            .iter()
            .filter_map(|&i| self.entries.get(i))
            .map(|e| {
                ListItem::new(Line::from(vec![
                    Span::styled(
                        e.label.clone(),
                        Style::default().fg(ui::TEXT).add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("  "),
                    Span::styled(e.subtitle.clone(), Style::default().fg(ui::OVERLAY0)),
                ]))
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(ui::OVERLAY0)),
            )
            .highlight_style(
                Style::default()
                    .bg(ui::SURFACE0)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");

        f.render_stateful_widget(list, inner[1], &mut self.state);
    }

    fn handle_event(&mut self, event: &Event) -> Option<Action> {
        let Event::Key(key) = event else {
            return None;
        };
        if key.kind != KeyEventKind::Press {
            return None;
        }
        match key.code {
            KeyCode::Esc => Some(Action::ClosePalette),
            KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
                self.state.select(Some(self.selected));
                None
            }
            KeyCode::Down => {
                if self.selected + 1 < self.filtered.len() {
                    self.selected += 1;
                }
                self.state.select(Some(self.selected));
                None
            }
            KeyCode::Enter => self.selected_action(),
            KeyCode::Backspace => {
                self.query.pop();
                self.recompute();
                None
            }
            KeyCode::Char(c) => {
                self.query.push(c);
                self.recompute();
                None
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(label: &str) -> Entry {
        Entry {
            label: label.to_string(),
            subtitle: "channel".to_string(),
            action: Action::Noop,
        }
    }

    #[test]
    fn empty_query_keeps_all_entries() {
        let mut p = Palette::new();
        p.entries = vec![entry("general"), entry("rust"), entry("random")];
        assert_eq!(p.filter().len(), 3);
    }

    #[test]
    fn fuzzy_query_filters_and_ranks() {
        let mut p = Palette::new();
        p.entries = vec![entry("general"), entry("rust"), entry("random")];
        p.query = "rst".to_string();
        let out = p.filter();
        assert!(!out.is_empty());
        assert_eq!(p.entries[out[0]].label, "rust");
    }

    #[test]
    fn esc_requests_close() {
        use crossterm::event::{KeyEvent, KeyModifiers};
        let mut p = Palette::new();
        let ev = Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(p.handle_event(&ev), Some(Action::ClosePalette)));
    }

    /// Channel entries produce a SelectChannel action when confirmed with Enter.
    #[test]
    fn test_palette_channel_result() {
        use crossterm::event::{KeyEvent, KeyModifiers};
        use nmp_nip29::GroupId;
        let mut p = Palette::new();
        let gid = GroupId::new("wss://h", "chan");
        p.entries = vec![Entry {
            label: "general".into(),
            subtitle: "channel".into(),
            action: Action::SelectChannel(gid.clone()),
        }];
        p.recompute();
        let ev = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        match p.handle_event(&ev) {
            Some(Action::SelectChannel(g)) => assert_eq!(g.local_id, "chan"),
            other => panic!("expected SelectChannel, got {other:?}"),
        }
    }

    /// Admin-only palette entries are hidden for non-admin users and visible for admins.
    #[test]
    fn test_palette_admin_gating() {
        use nmp_nip29::GroupId;
        use crate::app::{IdentityState, RelayState, Screen};
        let gid = GroupId::new("wss://h", "room");
        let base_snap = TuiSnapshot {
            channel_tree: vec![],
            selected_channel_id: Some(gid.clone()),
            selected_messages: vec![],
            selected_members: vec![],
            is_admin: false,
            my_pubkey: None,
            publish_outbox: vec![],
            identity_state: IdentityState::LoggedOut,
            relay_state: RelayState::Connected,
            errors: vec![],
            selected_index: 0,
            focus: Focus::ChannelList,
            message_scroll: 0,
            palette_open: false,
            active_form: None,
            login_error: None,
            screen: Screen::App,
        };
        let mut p = Palette::new();
        p.update(&base_snap);
        assert!(
            !p.entries.iter().any(|e| e.subtitle == "admin"),
            "non-admin should not see admin-only entries"
        );
        // Verify admin entries appear when the user is promoted to admin.
        let mut admin_snap = base_snap.clone();
        admin_snap.is_admin = true;
        p.update(&admin_snap);
        assert!(
            p.entries.iter().any(|e| e.subtitle == "admin"),
            "admin should see admin-only entries"
        );
    }
}
