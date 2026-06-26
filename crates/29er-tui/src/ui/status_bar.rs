//! Bottom status + context hint bar (issue #5).
//!
//! Left side:  identity  [key]verb hints (context-aware, LAVENDER keys)
//! Right side: relay state indicator  optional unread badge
use crossterm::event::Event;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use crate::actions::Action;
use crate::app::{Focus, IdentityState, RelayState, TuiSnapshot};
use crate::ui;
use crate::Component;

/// A hint pair: bracket+key label rendered in LAVENDER; action verb in TEXT.
type HintEntry = (&'static str, &'static str);

static HINTS_ROOMLIST: &[HintEntry] = &[
    ("[j/k]",    "navigate"),
    ("[Enter]",  "open"),
    ("[h/l]",    "expand"),
    ("[n]",      "compose"),
    ("[/]",      "search"),
    ("[?]",      "help"),
];
static HINTS_CHAT: &[HintEntry] = &[
    ("[j/k]",       "scroll"),
    ("[PgUp/Dn]",   "page"),
    ("[n]",         "compose"),
    ("[/]",         "search"),
    ("[?]",         "help"),
];
static HINTS_COMPOSER: &[HintEntry] = &[
    ("[Enter]",  "send"),
    ("[Esc]",    "cancel"),
    ("[@]",      "mention"),
    ("[?]",      "help"),
];
static HINTS_PALETTE: &[HintEntry] = &[
    ("[Enter]",  "select"),
    ("[Esc]",    "close"),
    ("[↑/↓]",   "navigate"),
];
static HINTS_FORM: &[HintEntry] = &[
    ("[Enter]",  "submit"),
    ("[Tab]",    "next field"),
    ("[Esc]",    "cancel"),
];
static HINTS_HELP: &[HintEntry] = &[
    ("[? / Esc]", "close help"),
];

pub struct StatusBar {
    relay_state: RelayState,
    identity: String,
    total_unread: u32,
    hints: &'static [HintEntry],
}

impl Default for StatusBar {
    fn default() -> Self {
        Self {
            relay_state: RelayState::Disconnected,
            identity: String::new(),
            total_unread: 0,
            hints: HINTS_ROOMLIST,
        }
    }
}

impl StatusBar {
    pub fn new() -> Self { Self::default() }

    pub fn update(&mut self, s: &TuiSnapshot) {
        self.relay_state = s.relay_state;
        self.identity = match &s.identity_state {
            IdentityState::LoggedIn { npub } => ui::short_pubkey(npub),
            IdentityState::LoggingIn         => "signing in\u{2026}".to_string(),
            IdentityState::LoggedOut         => "offline".to_string(),
        };
        self.total_unread = s.channel_tree.iter().map(|c| c.unread).sum();
        self.hints = if s.help_open {
            HINTS_HELP
        } else if s.active_form.is_some() || matches!(s.focus, Focus::Modal) {
            HINTS_FORM
        } else {
            match s.focus {
                Focus::RoomList  => HINTS_ROOMLIST,
                Focus::Chat      => HINTS_CHAT,
                Focus::Composer  => HINTS_COMPOSER,
                Focus::Palette   => HINTS_PALETTE,
                Focus::Modal     => HINTS_FORM,
            }
        };
    }

    /// Build the left-side line: identity then space-separated [key]verb hints.
    fn left_line(&self) -> Line<'static> {
        let mut spans: Vec<Span<'static>> = vec![
            Span::raw(" "),
            Span::styled(self.identity.clone(), Style::default().fg(ui::TEXT)),
            Span::raw("  "),
        ];
        for (i, (key, action)) in self.hints.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw("  "));
            }
            spans.push(Span::styled(*key,    Style::default().fg(ui::LAVENDER)));
            spans.push(Span::styled(*action, Style::default().fg(ui::TEXT)));
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

        // Relay state indicator: "relay: ● Connected" / "relay: ○ Connecting…" / "relay: ✗ Offline"
        let (dot, dot_fg, state_text, state_fg) = match self.relay_state {
            RelayState::Connected    => ("\u{25cf}", ui::GREEN,  " Connected",           ui::TEXT),
            RelayState::Connecting   => ("\u{25cb}", ui::YELLOW, " Connecting\u{2026}",  ui::SUBTEXT0),
            RelayState::Disconnected => ("\u{2717}", ui::RED,    " Offline",             ui::SUBTEXT0),
        };
        // "relay: " = 7 cols, dot = 1 col, state_text chars
        width += 7 + 1 + state_text.chars().count() as u16;
        spans.push(Span::raw("relay: "));
        spans.push(Span::styled(dot,        Style::default().fg(dot_fg)));
        spans.push(Span::styled(state_text, Style::default().fg(state_fg)));

        // 1-col right padding so text doesn't hug the terminal edge
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
        let left_w  = area.width.saturating_sub(right_w);

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

    fn handle_event(&mut self, _event: &Event) -> Option<Action> { None }
}
