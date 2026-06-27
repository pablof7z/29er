//! Command palette (issue #6): fuzzy filter over channels + typed actions.
//!
//! Improvements:
//! * nucleo Matcher and scratch buffer hoisted out of the hot path.
//! * Contextual empty-state: recent → channels-by-recency → actions.
//! * Inline badge ([n], [Admin], …) on action entries.
use crate::actions::Action;
use crate::app::{Focus, FormKind, TuiSnapshot};
use crate::ui;
use crate::Component;
use crossterm::event::{Event, KeyCode, KeyEventKind};
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;
use std::collections::{HashSet, VecDeque};

/// Maximum number of recently-confirmed entries to remember across openings.
const RECENT_CAP: usize = 5;

#[derive(Clone)]
struct Entry {
    label: String,
    subtitle: String,
    /// Optional inline badge shown before the label: "[n]", "[Admin]", etc.
    badge: Option<String>,
    action: Action,
}

pub struct Palette {
    query: String,
    entries: Vec<Entry>,
    filtered: Vec<usize>,
    selected: usize,
    state: ListState,
    // ── hoisted nucleo resources ─────────────────────────────────────────────
    /// Reused across every filter() call — avoids reallocating the Matcher.
    matcher: Matcher,
    /// Scratch buffer for Utf32Str::new; reused to avoid per-entry allocations.
    scratch: Vec<char>,
    // ── contextual empty-state bookkeeping ───────────────────────────────────
    /// Labels of the last RECENT_CAP confirmed entries, most recent first.
    recent: VecDeque<String>,
    /// How many of entries[] are channel entries (always the leading slice).
    channel_count: usize,
    /// last_timestamp for entries[0..channel_count] (parallel array).
    channel_timestamps: Vec<Option<u64>>,
}

impl Palette {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            entries: Vec::new(),
            filtered: Vec::new(),
            selected: 0,
            state: ListState::default(),
            matcher: Matcher::new(Config::DEFAULT),
            scratch: Vec::<char>::new(),
            recent: VecDeque::new(),
            channel_count: 0,
            channel_timestamps: Vec::new(),
        }
    }

    pub fn update(&mut self, s: &TuiSnapshot) {
        let mut entries: Vec<Entry> = Vec::new();
        let mut timestamps: Vec<Option<u64>> = Vec::new();

        // Channel entries from the channel tree
        for it in &s.channel_tree {
            entries.push(Entry {
                label: it.name.clone(),
                subtitle: format!(
                    "channel \u{2022} {}",
                    it.last_preview.clone().unwrap_or_default()
                ),
                badge: None,
                action: Action::SelectChannel(it.group_id.clone()),
            });
            timestamps.push(it.last_timestamp);
        }
        let channel_count = entries.len();

        // Action entries for the selected channel
        if let Some(g) = s.selected_channel_id.clone() {
            entries.push(Entry {
                label: "Compose message".into(),
                subtitle: "action".into(),
                badge: Some("[n]".into()),
                action: Action::SetFocus(Focus::Composer),
            });
            entries.push(Entry {
                label: "Show members".into(),
                subtitle: "action".into(),
                badge: None,
                action: Action::ShowMembers(g.clone()),
            });
            entries.push(Entry {
                label: "Join channel".into(),
                subtitle: "action".into(),
                badge: None,
                action: Action::OpenForm(FormKind::JoinWithCode(g.clone())),
            });
            entries.push(Entry {
                label: "Leave channel".into(),
                subtitle: "action".into(),
                badge: None,
                action: Action::Leave { group: g.clone() },
            });

            // Admin-only entries (issue #6 gating requirement)
            if s.is_admin {
                entries.push(Entry {
                    label: "Create invite".into(),
                    subtitle: "admin".into(),
                    badge: Some("[Admin]".into()),
                    action: Action::OpenForm(FormKind::CreateInvite(g.clone())),
                });
                entries.push(Entry {
                    label: "Create child channel".into(),
                    subtitle: "admin".into(),
                    badge: Some("[Admin]".into()),
                    action: Action::OpenForm(FormKind::CreateChild(g.clone())),
                });
                entries.push(Entry {
                    label: "Move channel".into(),
                    subtitle: "admin".into(),
                    badge: Some("[Admin]".into()),
                    action: Action::OpenForm(FormKind::MoveChannel(g.clone())),
                });
                entries.push(Entry {
                    label: "Add role / put user".into(),
                    subtitle: "admin".into(),
                    badge: Some("[Admin]".into()),
                    action: Action::OpenForm(FormKind::PutUser(g.clone())),
                });
            }
        }

        self.entries = entries;
        self.channel_count = channel_count;
        self.channel_timestamps = timestamps;
        self.recompute();
    }

    fn recompute(&mut self) {
        self.filtered = self.filter();
        if self.filtered.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.filtered.len() {
            self.selected = self.filtered.len() - 1;
        }
        self.state.select(if self.filtered.is_empty() {
            None
        } else {
            Some(self.selected)
        });
    }

    /// Fuzzy filter. Empty query uses contextual ordering (see `filter_empty`).
    /// Non-empty query runs nucleo fuzzy-match over label+subtitle combined.
    fn filter(&mut self) -> Vec<usize> {
        if self.query.trim().is_empty() {
            return self.filter_empty();
        }
        let pattern = Pattern::parse(
            self.query.trim(),
            CaseMatching::Ignore,
            Normalization::Smart,
        );

        // Collect haystack strings first so we hold no borrow over `self.matcher`.
        let hays: Vec<String> = self
            .entries
            .iter()
            .map(|e| format!("{} {}", e.label, e.subtitle))
            .collect();

        let mut scored = Vec::with_capacity(hays.len());
        for (i, hay) in hays.iter().enumerate() {
            self.scratch.clear();
            let utf32 = Utf32Str::new(hay.as_str(), &mut self.scratch);
            if let Some(score) = pattern.score(utf32, &mut self.matcher) {
                scored.push((score, i));
            }
        }
        scored.sort_by(|a, b| b.0.cmp(&a.0));
        scored.into_iter().map(|(_, i)| i).collect()
    }

    /// Contextual ordering when the query is blank:
    ///
    /// 1. Up to `RECENT_CAP` recently confirmed entries (most recent first).
    /// 2. All channel entries sorted by `last_timestamp` descending.
    /// 3. All action entries in their natural order.
    fn filter_empty(&self) -> Vec<usize> {
        let mut seen: HashSet<usize> = HashSet::new();
        let mut result = Vec::with_capacity(self.entries.len());

        // 1. Recent
        for label in &self.recent {
            if let Some(idx) = self.entries.iter().position(|e| &e.label == label) {
                if seen.insert(idx) {
                    result.push(idx);
                }
            }
        }

        // 2. Channels — sorted by timestamp descending (most-active first)
        let mut channel_indices: Vec<usize> = (0..self.channel_count)
            .filter(|i| !seen.contains(i))
            .collect();
        channel_indices.sort_by(|&a, &b| {
            let ta = self
                .channel_timestamps
                .get(a)
                .copied()
                .flatten()
                .unwrap_or(0);
            let tb = self
                .channel_timestamps
                .get(b)
                .copied()
                .flatten()
                .unwrap_or(0);
            tb.cmp(&ta)
        });
        for idx in channel_indices {
            seen.insert(idx);
            result.push(idx);
        }

        // 3. Actions (everything after the channel slice)
        for idx in self.channel_count..self.entries.len() {
            if !seen.contains(&idx) {
                result.push(idx);
            }
        }

        result
    }

    fn selected_action(&self) -> Option<Action> {
        self.filtered
            .get(self.selected)
            .and_then(|&i| self.entries.get(i))
            .map(|e| e.action.clone())
    }

    /// Push the currently-selected entry into the recent list (FIFO, capped at RECENT_CAP).
    fn record_recent(&mut self) {
        if let Some(&i) = self.filtered.get(self.selected) {
            if let Some(e) = self.entries.get(i) {
                let label = e.label.clone();
                self.recent.retain(|l| l != &label); // remove duplicate
                self.recent.push_front(label);
                if self.recent.len() > RECENT_CAP {
                    self.recent.pop_back();
                }
            }
        }
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self::new()
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

        // Results list — each item may carry an inline badge before the label.
        let items: Vec<ListItem> = self
            .filtered
            .iter()
            .filter_map(|&i| self.entries.get(i))
            .map(|e| {
                let mut spans: Vec<Span> = Vec::new();
                if let Some(badge) = &e.badge {
                    let badge_style = if badge == "[Admin]" {
                        // Dim for privilege-gated items
                        Style::default()
                            .fg(ui::OVERLAY0)
                            .add_modifier(Modifier::DIM)
                    } else {
                        // Accent color for keyboard shortcuts
                        Style::default().fg(ui::LAVENDER)
                    };
                    spans.push(Span::styled(badge.clone(), badge_style));
                    spans.push(Span::raw(" "));
                }
                spans.push(Span::styled(
                    e.label.clone(),
                    Style::default().fg(ui::TEXT).add_modifier(Modifier::BOLD),
                ));
                spans.push(Span::raw("  "));
                spans.push(Span::styled(
                    e.subtitle.clone(),
                    Style::default().fg(ui::OVERLAY0),
                ));
                ListItem::new(Line::from(spans))
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
            KeyCode::Enter => {
                self.record_recent();
                self.selected_action()
            }
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
            badge: None,
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
            badge: None,
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
        use crate::app::{IdentityState, RelayState, Screen};
        use nmp_nip29::GroupId;
        let gid = GroupId::new("wss://h", "room");
        let base_snap = TuiSnapshot {
            channel_tree: vec![],
            selected_channel_id: Some(gid.clone()),
            selected_messages: vec![],
            selected_members: vec![],
            profiles: Default::default(),
            is_admin: false,
            my_pubkey: None,
            publish_outbox: vec![],
            identity_state: IdentityState::LoggedOut,
            relay_state: RelayState::Connected,
            errors: vec![],
            selected_index: 0,
            focus: Focus::RoomList,
            message_scroll: 0,
            palette_open: false,
            active_form: None,
            login_error: None,
            screen: Screen::App,
            help_open: false,
            status_message: None,
            last_read_message_id: None,
            spinner_tick: 0,
            connecting_since: None,
            connected_at: None,
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

    /// Contextual empty-state: channels are sorted by timestamp descending.
    #[test]
    fn test_empty_state_channels_sorted_by_recency() {
        let mut p = Palette::new();
        // Three channel entries with different timestamps.
        p.entries = vec![
            Entry {
                label: "alpha".into(),
                subtitle: "channel".into(),
                badge: None,
                action: Action::Noop,
            },
            Entry {
                label: "beta".into(),
                subtitle: "channel".into(),
                badge: None,
                action: Action::Noop,
            },
            Entry {
                label: "gamma".into(),
                subtitle: "channel".into(),
                badge: None,
                action: Action::Noop,
            },
        ];
        p.channel_count = 3;
        p.channel_timestamps = vec![Some(100), Some(300), Some(200)];

        let out = p.filter(); // empty query → contextual order
                              // beta (ts=300) > gamma (ts=200) > alpha (ts=100)
        assert_eq!(p.entries[out[0]].label, "beta");
        assert_eq!(p.entries[out[1]].label, "gamma");
        assert_eq!(p.entries[out[2]].label, "alpha");
    }

    /// Contextual empty-state: recently confirmed entries surface at the top.
    #[test]
    fn test_empty_state_recent_entries_first() {
        use crossterm::event::{KeyEvent, KeyModifiers};
        use nmp_nip29::GroupId;

        let mut p = Palette::new();
        let gid = GroupId::new("wss://h", "chan");
        // One channel, one action.
        p.entries = vec![
            Entry {
                label: "general".into(),
                subtitle: "channel".into(),
                badge: None,
                action: Action::SelectChannel(gid.clone()),
            },
            Entry {
                label: "Compose message".into(),
                subtitle: "action".into(),
                badge: Some("[n]".into()),
                action: Action::SetFocus(Focus::Composer),
            },
        ];
        p.channel_count = 1;
        p.channel_timestamps = vec![Some(1000)];
        p.recompute();

        // Confirm "Compose message" (index 1) via Enter.
        p.selected = 1;
        let ev = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let _ = p.handle_event(&ev);

        // Re-run filter with empty query — "Compose message" should now be first.
        let out = p.filter();
        assert_eq!(
            p.entries[out[0]].label, "Compose message",
            "recently confirmed entry must appear first in empty-state"
        );
    }

    /// Admin entries carry the [Admin] badge; hotkey entries carry their shortcut badge.
    #[test]
    fn test_inline_badges() {
        use crate::app::{IdentityState, RelayState, Screen};
        use nmp_nip29::GroupId;

        let gid = GroupId::new("wss://h", "room");
        let snap = TuiSnapshot {
            channel_tree: vec![],
            selected_channel_id: Some(gid.clone()),
            selected_messages: vec![],
            selected_members: vec![],
            profiles: Default::default(),
            is_admin: true,
            my_pubkey: None,
            publish_outbox: vec![],
            identity_state: IdentityState::LoggedOut,
            relay_state: RelayState::Connected,
            errors: vec![],
            selected_index: 0,
            focus: Focus::RoomList,
            message_scroll: 0,
            palette_open: false,
            active_form: None,
            login_error: None,
            screen: Screen::App,
            help_open: false,
            status_message: None,
            last_read_message_id: None,
            spinner_tick: 0,
            connecting_since: None,
            connected_at: None,
        };
        let mut p = Palette::new();
        p.update(&snap);

        // "Compose message" must have badge "[n]"
        let compose = p
            .entries
            .iter()
            .find(|e| e.label == "Compose message")
            .unwrap();
        assert_eq!(compose.badge.as_deref(), Some("[n]"));

        // Admin-only entries must carry the "[Admin]" badge.
        let admin_entries: Vec<_> = p.entries.iter().filter(|e| e.subtitle == "admin").collect();
        assert!(!admin_entries.is_empty(), "admin entries must exist");
        for e in &admin_entries {
            assert_eq!(
                e.badge.as_deref(),
                Some("[Admin]"),
                "admin entry '{}' must have [Admin] badge",
                e.label
            );
        }

        // Channel entries must have no badge.
        for e in p.entries.iter().take(p.channel_count) {
            assert!(e.badge.is_none(), "channel entry must not have a badge");
        }
    }
}
