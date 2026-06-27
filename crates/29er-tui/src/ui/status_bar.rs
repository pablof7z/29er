//! Bottom status + context hint bar (issue #5).
//!
//! Left side:  identity  [key]verb hints (context-aware, LAVENDER keys)
//! Right side: relay state indicator  optional unread badge
//!
//! Graduated async feedback:
//!   Connecting < 1s  → hollow dot + "Connecting…" (quiet)
//!   Connecting 1-10s → spinner + "Connecting…"
//!   Connecting > 10s → spinner + "Connecting… (Xs)" (yellow urgency)
//!   Connected flash  → solid green dot + "Connected" for 2 seconds
//!   Error            → red remedy hint "Relay disconnected. Press [R] to reconnect."

/// Braille spinner frames.  Use `(spinner_tick % SPINNER_FRAMES.len())` to pick one.
pub const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

use crate::actions::Action;
use crate::app::{Focus, IdentityState, RelayState, TuiSnapshot};
use crate::ui;
use crate::Component;
use crossterm::event::Event;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

/// A hint pair: bracket+key label rendered in LAVENDER; action verb in TEXT.
type HintEntry = (&'static str, &'static str);

static HINTS_ROOMLIST: &[HintEntry] = &[
    ("[j/k]", "navigate"),
    ("[Enter]", "open"),
    ("[h/l]", "expand"),
    ("[n]", "compose"),
    ("[/]", "search"),
    ("[?]", "help"),
];
static HINTS_CHAT: &[HintEntry] = &[
    ("[j/k]", "scroll"),
    ("[PgUp/Dn]", "page"),
    ("[n]", "compose"),
    ("[/]", "search"),
    ("[?]", "help"),
];
static HINTS_COMPOSER: &[HintEntry] = &[
    ("[Enter]", "send"),
    ("[Esc]", "cancel"),
    ("[@]", "mention"),
    ("[?]", "help"),
];
static HINTS_PALETTE: &[HintEntry] = &[
    ("[Enter]", "select"),
    ("[Esc]", "close"),
    ("[↑/↓]", "navigate"),
];
static HINTS_FORM: &[HintEntry] = &[
    ("[Enter]", "submit"),
    ("[Tab]", "next field"),
    ("[Esc]", "cancel"),
];
static HINTS_HELP: &[HintEntry] = &[("[? / Esc]", "close help")];

pub struct StatusBar {
    relay_state: RelayState,
    identity: String,
    total_unread: u32,
    hints: &'static [HintEntry],
    /// Transient acknowledgment message (e.g. "Joining room…"). Overrides hints
    /// on the left side while set; `None` when expired.
    status_message: Option<String>,
    spinner_tick: u64,
    connecting_since: Option<std::time::Instant>,
    connected_at: Option<std::time::Instant>,
}

impl Default for StatusBar {
    fn default() -> Self {
        Self {
            relay_state: RelayState::Disconnected,
            identity: String::new(),
            total_unread: 0,
            hints: HINTS_ROOMLIST,
            status_message: None,
            spinner_tick: 0,
            connecting_since: None,
            connected_at: None,
        }
    }
}

impl StatusBar {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self, s: &TuiSnapshot) {
        self.relay_state = s.relay_state.clone();
        self.identity = match &s.identity_state {
            IdentityState::LoggedIn { npub } => ui::short_pubkey(npub),
            IdentityState::LoggingIn => "signing in\u{2026}".to_string(),
            IdentityState::LoggedOut => "offline".to_string(),
        };
        self.total_unread = s.channel_tree.iter().map(|c| c.unread).sum();
        self.status_message = s.status_message.clone();
        self.spinner_tick = s.spinner_tick;
        self.connecting_since = s.connecting_since;
        self.connected_at = s.connected_at;
        self.hints = if s.help_open {
            HINTS_HELP
        } else if s.active_form.is_some() || matches!(s.focus, Focus::Modal) {
            HINTS_FORM
        } else {
            match s.focus {
                Focus::RoomList => HINTS_ROOMLIST,
                Focus::Chat => HINTS_CHAT,
                Focus::Composer => HINTS_COMPOSER,
                Focus::Palette => HINTS_PALETTE,
                Focus::Modal => HINTS_FORM,
            }
        };
    }

    /// Build the left-side line: identity, then either the transient status
    /// message (when set) or the context-aware [key]verb hints.
    fn left_line(&self) -> Line<'static> {
        let mut spans: Vec<Span<'static>> = vec![
            Span::raw(" "),
            Span::styled(self.identity.clone(), Style::default().fg(ui::TEXT)),
            Span::raw("  "),
        ];
        if let Some(msg) = &self.status_message {
            // Show the transient acknowledgment message instead of hints.
            spans.push(Span::styled(
                msg.clone(),
                Style::default().fg(ui::YELLOW).add_modifier(Modifier::BOLD),
            ));
        } else {
            for (i, (key, action)) in self.hints.iter().enumerate() {
                if i > 0 {
                    spans.push(Span::raw("  "));
                }
                spans.push(Span::styled(*key, Style::default().fg(ui::LAVENDER)));
                spans.push(Span::styled(*action, Style::default().fg(ui::TEXT)));
            }
        }
        Line::from(spans)
    }

    /// Build the right-side line (relay indicator + optional unread badge).
    /// Returns the line and its approximate display-column width.
    fn right_line(&self) -> (Line<'static>, u16) {
        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut width: u16 = 0;

        // Unread badge (shown when any channel has unread messages)
        if self.total_unread > 0 {
            let badge = format!("\u{26a1} {} unread", self.total_unread); // ⚡ N unread
            let badge_width = badge.chars().count() as u16;
            spans.push(Span::styled(
                badge,
                Style::default().fg(ui::MAUVE).add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::raw("  "));
            width += badge_width + 2;
        }

        // Spinner frame (advances with spinner_tick).
        let frame = SPINNER_FRAMES[(self.spinner_tick as usize) % SPINNER_FRAMES.len()];

        match &self.relay_state {
            RelayState::Connected => {
                // Flash "● Connected" in green for 2 s after first connection.
                let flash_active = self
                    .connected_at
                    .map(|t| t.elapsed().as_secs_f32() < 2.0)
                    .unwrap_or(false);
                let state_fg = if flash_active { ui::GREEN } else { ui::TEXT };
                // "relay: " = 7, "●" = 1, " Connected" = 10
                width += 7 + 1 + 10;
                spans.push(Span::raw("relay: "));
                spans.push(Span::styled("\u{25cf}", Style::default().fg(ui::GREEN)));
                spans.push(Span::styled(" Connected", Style::default().fg(state_fg)));
            }
            RelayState::Connecting => {
                let elapsed = self
                    .connecting_since
                    .map(|t| t.elapsed().as_secs())
                    .unwrap_or(0);
                spans.push(Span::raw("relay: "));
                if elapsed < 1 {
                    // < 1 s: quiet hollow-dot indicator, no spinner yet.
                    let text = " Connecting\u{2026}";
                    width += 7 + 1 + text.chars().count() as u16;
                    spans.push(Span::styled("\u{25cb}", Style::default().fg(ui::YELLOW)));
                    spans.push(Span::styled(text, Style::default().fg(ui::SUBTEXT0)));
                } else if elapsed <= 10 {
                    // 1-10 s: animated spinner + label.
                    let text = format!(" {frame} Connecting\u{2026}");
                    width += 7 + text.chars().count() as u16;
                    spans.push(Span::styled(text, Style::default().fg(ui::SUBTEXT0)));
                } else {
                    // > 10 s: spinner + elapsed seconds, colour bumped to yellow.
                    let text = format!(" {frame} Connecting\u{2026} ({elapsed}s)");
                    width += 7 + text.chars().count() as u16;
                    spans.push(Span::styled(text, Style::default().fg(ui::YELLOW)));
                }
            }
            RelayState::Disconnected => {
                // "relay: " = 7, "✗" = 1, " Offline" = 8
                width += 7 + 1 + 8;
                spans.push(Span::raw("relay: "));
                spans.push(Span::styled("\u{2717}", Style::default().fg(ui::RED)));
                spans.push(Span::styled(" Offline", Style::default().fg(ui::SUBTEXT0)));
            }
            RelayState::Error(_msg) => {
                // Remedy hint: tell the user exactly how to recover.
                let remedy = "Relay disconnected. Press [R] to reconnect.";
                width += remedy.chars().count() as u16;
                spans.push(Span::styled(remedy, Style::default().fg(ui::RED)));
            }
        }

        // 1-col right padding so text doesn't hug the terminal edge.
        spans.push(Span::raw(" "));
        width += 1;

        (Line::from(spans), width)
    }
}

impl Component for StatusBar {
    fn draw(&mut self, f: &mut Frame, area: Rect) {
        let bg = Style::default().bg(ui::SURFACE0);

        let (right, right_w) = self.right_line();
        // Guard: never let the right column consume more than half the bar
        let right_w = right_w.min(area.width / 2);
        let left_w = area.width.saturating_sub(right_w);

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(left_w), Constraint::Length(right_w)])
            .split(area);

        f.render_widget(Paragraph::new(self.left_line()).style(bg), chunks[0]);
        f.render_widget(
            Paragraph::new(right).style(bg).alignment(Alignment::Right),
            chunks[1],
        );
    }

    fn handle_event(&mut self, _event: &Event) -> Option<Action> {
        None
    }
}
