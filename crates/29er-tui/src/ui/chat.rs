//! Right pane message history. The pane owns TUI-only scroll state, day
//! dividers, read-marker placement, and the new-message indicator; each
//! projected chat message is rendered through the NMP registry chat row.
use std::collections::HashMap;

use crate::actions::Action;
use crate::app::{Focus, RelayState, TuiProfile, TuiSnapshot};
use crate::components::nostr_chat::{
    nostr_group_chat_wire::{NostrGroupChatMessageWire, NostrGroupChatReactionWire},
    nostr_group_message_row::NostrGroupMessageRow,
};
use crate::components::nostr_user::profile_wire::ProfileWire;
use crate::ui;
use crate::Component;
use crossterm::event::{Event, KeyCode, KeyEventKind};
use nmp_app_29er::group_chat::GroupChatMessage;
use ratatui::layout::{Alignment, Rect, Size};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};
use ratatui::Frame;
use tui_scrollview::{ScrollView, ScrollViewState};

pub struct ChatComponent {
    messages: Vec<GroupChatMessage>,
    profiles: HashMap<String, TuiProfile>,
    my_pubkey: Option<String>,
    title: String,
    has_room: bool,
    focused: bool,
    connected: bool,
    tick: usize,
    // Ephemeral scroll state — not stored in TuiSnapshot.
    scroll_state: ScrollViewState,
    /// True when we should auto-follow new messages (user is at the bottom).
    at_bottom: bool,
    /// Channel id from the previous update; used to detect channel switches.
    prev_channel_id: Option<String>,
    /// Message count at the previous update; used to detect new arrivals.
    prev_msg_count: usize,
    /// Messages that arrived while the user was scrolled up.
    new_since_scroll: usize,
    /// Visible-area height from the last draw call; used for at-bottom detection.
    last_inner_height: u16,
    /// Total content height from the last draw call.
    last_total_height: u16,
    /// Id of the last message the user had read in this channel (the separator anchor).
    last_read_message_id: Option<String>,
}

impl Default for ChatComponent {
    fn default() -> Self {
        Self::new()
    }
}

/// One laid-out row in the scroll buffer. Built once per draw so the height
/// (measure) pass and the render pass stay in lock-step.
enum Row<'a> {
    /// Centered day divider (`── Today ──`).
    Day(String),
    /// A Rust-projected message rendered through the registry chat row.
    Message(&'a GroupChatMessage),
    /// The "you've read to here" separator.
    ReadMarker,
    /// A single blank spacer row between groups / dividers.
    Gap,
}

impl ChatComponent {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            profiles: HashMap::new(),
            my_pubkey: None,
            title: " chat ".to_string(),
            has_room: false,
            focused: false,
            connected: false,
            tick: 0,
            scroll_state: ScrollViewState::new(),
            at_bottom: true,
            prev_channel_id: None,
            prev_msg_count: 0,
            new_since_scroll: 0,
            last_inner_height: 0,
            last_total_height: 0,
            last_read_message_id: None,
        }
    }

    pub fn update(&mut self, s: &TuiSnapshot) {
        let channel_id = s.selected_channel_id.as_ref().map(|g| g.local_id.clone());

        // Reset scroll when switching channels.
        if channel_id != self.prev_channel_id {
            self.scroll_state = ScrollViewState::new();
            self.at_bottom = true;
            self.new_since_scroll = 0;
            self.prev_channel_id = channel_id;
        }

        // Track new messages; auto-scroll or accumulate indicator count.
        let new_count = s.selected_messages.len();
        if new_count > self.prev_msg_count {
            if self.at_bottom {
                self.scroll_state.scroll_to_bottom();
            } else {
                self.new_since_scroll += new_count - self.prev_msg_count;
            }
        }
        self.prev_msg_count = new_count;

        self.messages = s.selected_messages.clone();

        self.profiles = s.profiles.clone();
        self.my_pubkey = s.my_pubkey.clone();
        self.last_read_message_id = s.last_read_message_id.clone();
        self.focused = matches!(s.focus, Focus::Chat | Focus::Composer);
        self.has_room = s.selected_channel_id.is_some();
        self.connected = matches!(s.relay_state, RelayState::Connected);
        self.title = s
            .selected_channel_id
            .as_ref()
            .map(|g| format!(" {} ", g.local_id))
            .unwrap_or_else(|| " chat ".to_string());
    }

    fn is_own(&self, pubkey: &str) -> bool {
        self.my_pubkey
            .as_deref()
            .map(|me| me == pubkey)
            .unwrap_or(false)
    }

    fn message_wire(&self, message: &GroupChatMessage) -> NostrGroupChatMessageWire {
        NostrGroupChatMessageWire {
            id: message.id.clone(),
            author_pubkey: message.pubkey.clone(),
            content: if message.copy_text.is_empty() {
                message.raw_content.clone()
            } else {
                message.copy_text.clone()
            },
            created_at_label: ui::clock_time(message.created_at),
            reply_preview: None,
            reactions: message
                .reactions
                .iter()
                .map(|reaction| NostrGroupChatReactionWire {
                    emoji: reaction.emoji.clone(),
                    count: u32::try_from(reaction.count).unwrap_or(u32::MAX),
                })
                .collect(),
            is_outgoing: self.is_own(&message.pubkey),
        }
    }

    fn profile_wire(&self, pubkey: &str) -> ProfileWire {
        match self.profiles.get(pubkey) {
            Some(profile) => {
                let npub = profile
                    .npub
                    .clone()
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| pubkey.to_string());
                ProfileWire {
                    pubkey: pubkey.to_string(),
                    display_name: profile.display_name.clone(),
                    about: None,
                    picture_url: profile.picture_url.clone(),
                    nip05: None,
                    npub_short: ui::short_pubkey(&npub),
                    npub,
                }
            }
            None => ProfileWire {
                pubkey: pubkey.to_string(),
                display_name: None,
                about: None,
                picture_url: None,
                nip05: None,
                npub: pubkey.to_string(),
                npub_short: ui::short_pubkey(pubkey),
            },
        }
    }

    /// Build the ordered row layout (oldest → newest) with day dividers.
    fn build_rows<'a>(&self, msgs: &[&'a GroupChatMessage]) -> Vec<Row<'a>> {
        let mut rows: Vec<Row> = Vec::new();
        let mut prev_day: Option<i64> = None;

        for m in msgs {
            let day = ui::day_index(m.created_at);
            let new_day = prev_day != Some(day);
            if new_day {
                if !rows.is_empty() {
                    rows.push(Row::Gap);
                }
                rows.push(Row::Day(ui::day_label(m.created_at)));
            } else if !rows.is_empty() {
                rows.push(Row::Gap);
            }

            rows.push(Row::Message(m));

            if self.last_read_message_id.as_deref() == Some(m.id.as_str()) {
                rows.push(Row::ReadMarker);
            }

            prev_day = Some(day);
        }
        rows
    }

    fn row_height(&self, row: &Row, content_width: u16) -> u16 {
        match row {
            Row::Day(_) | Row::ReadMarker | Row::Gap => 1,
            Row::Message(message) => {
                let wire = self.message_wire(message);
                let profile = self.profile_wire(&message.pubkey);
                NostrGroupMessageRow::new(&wire)
                    .author_profile(Some(&profile))
                    .preferred_height(content_width)
            }
        }
    }

    fn render_row(&self, sv: &mut ScrollView, row: &Row, y: u16, content_width: u16) {
        match row {
            Row::Day(label) => {
                let line = Line::from(Span::styled(
                    format!("\u{2500}\u{2500} {label} \u{2500}\u{2500}"),
                    Style::default().fg(ui::OVERLAY0),
                ))
                .alignment(Alignment::Center);
                sv.render_widget(Paragraph::new(line), Rect::new(0, y, content_width, 1));
            }
            Row::ReadMarker => {
                let sep = Paragraph::new(Line::from(Span::styled(
                    "\u{2500}\u{2500} You've read to here \u{2500}\u{2500}",
                    Style::default().fg(ui::OVERLAY0),
                )))
                .alignment(Alignment::Center);
                sv.render_widget(sep, Rect::new(0, y, content_width, 1));
            }
            Row::Gap => {}
            Row::Message(message) => {
                let wire = self.message_wire(message);
                let profile = self.profile_wire(&message.pubkey);
                let h = NostrGroupMessageRow::new(&wire)
                    .author_profile(Some(&profile))
                    .preferred_height(content_width);
                sv.render_widget(
                    NostrGroupMessageRow::new(&wire)
                        .author_profile(Some(&profile))
                        .max_width(content_width),
                    Rect::new(0, y, content_width, h),
                );
            }
        }
    }
}

impl Component for ChatComponent {
    fn draw(&mut self, f: &mut Frame, area: Rect) {
        self.tick = self.tick.wrapping_add(1);

        let border_style = if self.focused {
            Style::default().fg(ui::MAUVE)
        } else {
            Style::default().fg(ui::OVERLAY0)
        };

        // Closure to build a fresh Block (avoids Clone).
        let make_block = || {
            Block::default()
                .title(self.title.clone())
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(border_style)
        };

        let inner = make_block().inner(area);

        if !self.has_room {
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "Select a channel to view its messages.",
                    Style::default().fg(ui::SUBTEXT0),
                )))
                .block(make_block())
                .wrap(Wrap { trim: false }),
                area,
            );
            return;
        }

        if self.messages.is_empty() {
            let line = if self.connected {
                Line::from(Span::styled(
                    "No messages yet \u{2014} be the first to say something",
                    Style::default().fg(ui::SUBTEXT0),
                ))
            } else {
                Line::from(vec![
                    Span::styled(
                        format!("{} ", ui::spinner_frame(self.tick)),
                        Style::default().fg(ui::MAUVE),
                    ),
                    Span::styled(
                        "Connecting to relay\u{2026}",
                        Style::default().fg(ui::SUBTEXT0),
                    ),
                ])
            };
            f.render_widget(
                Paragraph::new(line)
                    .block(make_block())
                    .alignment(Alignment::Center)
                    .wrap(Wrap { trim: false }),
                area,
            );
            return;
        }

        // Guard against degenerate areas that would cause scrollbar panics.
        if inner.width == 0 || inner.height == 0 {
            f.render_widget(make_block(), area);
            return;
        }

        // Reserve 1 column for the vertical scrollbar (auto-shown by ScrollView).
        let content_width = inner.width.saturating_sub(1).max(1);

        // Messages stored newest-first; lay out oldest→newest (top→bottom).
        let msgs_in_order: Vec<&GroupChatMessage> = self.messages.iter().rev().collect();
        let rows = self.build_rows(&msgs_in_order);

        let total_height: u16 = rows
            .iter()
            .map(|r| self.row_height(r, content_width))
            .sum::<u16>()
            .max(1);

        self.last_inner_height = inner.height;
        self.last_total_height = total_height;

        // Render the border block first.
        f.render_widget(make_block(), area);

        // Build the scroll-view and populate it row by row.
        let mut scroll_view = ScrollView::new(Size::new(content_width, total_height));
        let mut y: u16 = 0;
        for row in &rows {
            let h = self.row_height(row, content_width);
            self.render_row(&mut scroll_view, row, y, content_width);
            y += h;
        }

        // Render the scroll-view into the inner area.
        f.render_stateful_widget(scroll_view, inner, &mut self.scroll_state);

        // "↓ N new" overlay indicator when scrolled up and new messages arrived.
        if self.new_since_scroll > 0 {
            let text = format!("\u{2193} {} new", self.new_since_scroll);
            let ind = Paragraph::new(Line::from(Span::styled(
                text,
                Style::default().fg(ui::MAUVE).add_modifier(Modifier::BOLD),
            )))
            .alignment(Alignment::Right);
            // Place at the last row of inner area (above any horizontal scrollbar).
            let ind_y = inner.y + inner.height.saturating_sub(1);
            f.render_widget(
                ind,
                Rect::new(inner.x, ind_y, inner.width.saturating_sub(1), 1),
            );
        }
    }

    fn handle_event(&mut self, event: &Event) -> Option<Action> {
        let Event::Key(key) = event else {
            return None;
        };
        if key.kind != KeyEventKind::Press {
            return None;
        }
        match key.code {
            KeyCode::PageUp | KeyCode::Char('k') => {
                self.scroll_state.scroll_page_up();
                self.at_bottom = false;
                None
            }
            KeyCode::PageDown | KeyCode::Char('j') => {
                self.scroll_state.scroll_page_down();
                // Detect if we have reached the bottom of the content.
                // last_total_height and last_inner_height are populated by draw().
                let approx_max = self
                    .last_total_height
                    .saturating_sub(self.last_inner_height.saturating_sub(1));
                if self.scroll_state.offset().y >= approx_max {
                    self.at_bottom = true;
                    self.new_since_scroll = 0;
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
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn msg(pk: &str, ts: u64, c: &str) -> GroupChatMessage {
        let tree =
            nmp_content::tokenize_with_kind(c, &[], nmp_content::RenderMode::Auto, 9).to_wire();
        GroupChatMessage {
            id: format!("{pk}{ts}"),
            pubkey: pk.to_string(),
            raw_content: c.to_string(),
            copy_text: c.to_string(),
            created_at: ts,
            kind: 9,
            content_tree_bytes: nmp_content::wire::encode_content_tree(&tree),
            mention_pubkeys: Vec::new(),
            event_ref_uris: Vec::new(),
            event_ref_primary_ids: Vec::new(),
            reactions: Vec::new(),
            reaction_reactor_pubkeys: Vec::new(),
        }
    }

    fn render(c: &mut ChatComponent, w: u16, h: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| c.draw(f, f.area())).unwrap();
        let buf = t.backend().buffer().clone();
        (0..h)
            .map(|y| {
                (0..w)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
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
        c.update(&snap(
            vec![msg("abcd", 100, &long)],
            true,
            Some("wss://h".into()),
        ));
        let out = render(&mut c, 40, 12);
        assert!(out.matches('x').count() > 40);
    }

    #[test]
    fn consecutive_same_author_messages_render_registry_rows() {
        let mut c = ChatComponent::new();
        c.update(&snap(
            vec![
                msg("abcd", 100, "first"),
                msg("abcd", 110, "second"),
                msg("abcd", 120, "third"),
            ],
            true,
            Some("wss://h".into()),
        ));
        let out = render(&mut c, 50, 20);
        assert!(out.contains("first") && out.contains("second") && out.contains("third"));
    }

    #[test]
    fn author_change_renders_profile_fallbacks() {
        let mut c = ChatComponent::new();
        c.update(&snap(
            vec![msg("aaaa", 100, "hi"), msg("bbbb", 130, "yo")],
            true,
            Some("wss://h".into()),
        ));
        let out = render(&mut c, 50, 20);
        assert!(out.contains("aaaa"), "{out}");
        assert!(out.contains("bbbb"), "{out}");
        assert!(out.contains("hi"), "{out}");
        assert!(out.contains("yo"), "{out}");
    }

    #[test]
    fn new_message_indicator_shown_when_scrolled_up() {
        let mut c = ChatComponent::new();
        // Start with one message.
        c.update(&snap(
            vec![msg("pk1", 1, "first")],
            true,
            Some("wss://h".into()),
        ));
        // Simulate user scrolling up.
        c.at_bottom = false;
        // New message arrives.
        c.update(&snap(
            vec![msg("pk1", 1, "first"), msg("pk2", 2, "second")],
            true,
            Some("wss://h".into()),
        ));
        assert_eq!(c.new_since_scroll, 1);
    }

    #[test]
    fn auto_scroll_resets_on_channel_change() {
        let mut c = ChatComponent::new();
        c.update(&snap(
            vec![msg("pk1", 1, "hi")],
            true,
            Some("wss://h".into()),
        ));
        c.at_bottom = false;
        c.new_since_scroll = 5;
        // Switch channel.
        c.update(&snap(vec![], true, Some("wss://h2".into())));
        assert!(c.at_bottom);
        assert_eq!(c.new_since_scroll, 0);
    }

    #[test]
    fn registry_row_uses_projected_copy_text() {
        let mut message = msg("pk1", 1, "raw body");
        message.copy_text = "projected copy".to_string();
        let mut c = ChatComponent::new();
        c.update(&snap(vec![message], true, Some("wss://h".into())));
        let out = render(&mut c, 80, 16);
        assert!(out.contains("projected copy"), "{out}");
        assert!(!out.contains("raw body"), "{out}");
    }

    #[test]
    fn registry_row_renders_projected_reaction_chips() {
        let mut message = msg("pk1", 1, "reacted body");
        message.reactions = vec![nmp_app_29er::group_chat::GroupChatReaction {
            emoji: "+".to_string(),
            count: 2,
        }];
        let mut c = ChatComponent::new();
        c.update(&snap(vec![message], true, Some("wss://h".into())));
        let out = render(&mut c, 80, 16);
        assert!(out.contains("+ 2"), "{out}");
    }

    fn snap(
        messages: Vec<GroupChatMessage>,
        connected: bool,
        room: Option<String>,
    ) -> crate::app::TuiSnapshot {
        use crate::app::{IdentityState, RelayState, Screen};
        crate::app::TuiSnapshot {
            channel_tree: vec![],
            selected_channel_id: room.map(|r| nmp_nip29::GroupId::new("wss://h", r)),
            selected_messages: messages,
            selected_members: vec![],
            profiles: Default::default(),
            event_envelopes: Default::default(),
            is_admin: false,
            my_pubkey: None,
            publish_outbox: vec![],
            identity_state: IdentityState::LoggedOut,
            relay_state: if connected {
                RelayState::Connected
            } else {
                RelayState::Connecting
            },
            errors: vec![],
            selected_index: 0,
            focus: Focus::Chat,
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
        }
    }
}
