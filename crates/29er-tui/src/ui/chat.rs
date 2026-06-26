//! Right pane message history (issue #7): newest-at-bottom, per-author color,
//! own-message alignment, wrapping, auto-scroll, new-message indicator.
//! Uses tui-scrollview for scroll state management.
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::layout::{Alignment, Rect, Size};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};
use ratatui::Frame;
use tui_scrollview::{ScrollView, ScrollViewState};
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
}

impl Default for ChatComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl ChatComponent {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
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
        self.my_pubkey = s.my_pubkey.clone();
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

    /// Estimate the number of visual rows a body text takes at a given column width.
    /// Uses character count as a proxy for display width (good for ASCII/Latin text).
    fn body_line_count(content: &str, width: u16) -> u16 {
        if width == 0 {
            return 1;
        }
        let chars = content.chars().count() as u16;
        ((chars + width - 1) / width).max(1)
    }

    /// Total height of a single rendered message block (header + body + blank).
    fn message_height(m: &GroupChatMessage, width: u16) -> u16 {
        1 /* header */ + Self::body_line_count(&m.content, width) + 1 /* blank separator */
    }

    /// Render one message into the scroll-view buffer at row `y`.
    fn render_message_into(
        &self,
        sv: &mut ScrollView,
        m: &GroupChatMessage,
        y: u16,
        width: u16,
    ) {
        let own = self.is_own(&m.pubkey);

        // Header line.
        let header: Line<'static> = if own {
            Line::from(vec![
                Span::styled(
                    ui::clock_time(m.created_at),
                    Style::default().fg(ui::OVERLAY0),
                ),
                Span::raw("  "),
                Span::styled(
                    "you",
                    Style::default().fg(ui::GREEN).add_modifier(Modifier::BOLD),
                ),
            ])
            .alignment(Alignment::Right)
        } else {
            Line::from(vec![
                Span::styled(
                    ui::short_pubkey(&m.pubkey),
                    Style::default()
                        .fg(ui::author_color(&m.pubkey))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(
                    ui::clock_time(m.created_at),
                    Style::default().fg(ui::OVERLAY0),
                ),
            ])
        };
        sv.render_widget(Paragraph::new(header), Rect::new(0, y, width, 1));

        // Body (wrapped).
        let body_h = Self::body_line_count(&m.content, width);
        let body_color = if own { ui::SUBTEXT0 } else { ui::TEXT };
        let body_span = Span::styled(m.content.clone(), Style::default().fg(body_color));
        let mut body_para = Paragraph::new(Line::from(body_span)).wrap(Wrap { trim: false });
        if own {
            body_para = body_para.alignment(Alignment::Right);
        }
        sv.render_widget(body_para, Rect::new(0, y + 1, width, body_h));
        // blank separator row is left empty (just advance y in the caller)
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

        // Calculate total content height (oldest message at top, newest at bottom).
        let total_height: u16 = self
            .messages
            .iter()
            .map(|m| Self::message_height(m, content_width))
            .sum::<u16>()
            .max(1);

        self.last_inner_height = inner.height;
        self.last_total_height = total_height;

        // Render the border block first.
        f.render_widget(make_block(), area);

        // Build the scroll-view and populate it.
        let mut scroll_view = ScrollView::new(Size::new(content_width, total_height));
        let mut y: u16 = 0;
        // messages are stored newest-first; iterate rev for oldest→newest (top→bottom).
        for m in self.messages.iter().rev() {
            let h = Self::message_height(m, content_width);
            self.render_message_into(&mut scroll_view, m, y, content_width);
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
            f.render_widget(ind, Rect::new(inner.x, ind_y, inner.width.saturating_sub(1), 1));
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
        GroupChatMessage {
            id: format!("{pk}{ts}"),
            pubkey: pk.to_string(),
            content: c.to_string(),
            created_at: ts,
            kind: 9,
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
        c.update(&snap(vec![msg("abcd", 100, &long)], true, Some("wss://h".into())));
        let out = render(&mut c, 40, 12);
        assert!(out.matches('x').count() > 40);
    }

    #[test]
    fn new_message_indicator_shown_when_scrolled_up() {
        let mut c = ChatComponent::new();
        // Start with one message.
        c.update(&snap(vec![msg("pk1", 1, "first")], true, Some("wss://h".into())));
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
        c.update(&snap(vec![msg("pk1", 1, "hi")], true, Some("wss://h".into())));
        c.at_bottom = false;
        c.new_since_scroll = 5;
        // Switch channel.
        c.update(&snap(vec![], true, Some("wss://h2".into())));
        assert!(c.at_bottom);
        assert_eq!(c.new_since_scroll, 0);
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
        }
    }
}
