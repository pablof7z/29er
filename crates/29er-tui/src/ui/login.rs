//! 3-step progressive onboarding (issue #10).
//!
//! Step 1 – Identity:  nsec paste (masked), validates `nsec1` prefix.
//! Step 2 – Relay:     relay URL, pre-filled from Rust-owned app config.
//! Step 3 – Discover:  optional quick-join prompt (shown when no rooms exist).
use crate::actions::Action;
use crate::app::TuiSnapshot;
use crate::ui;
use crate::Component;
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;
use ratatui_textarea::{CursorMove, TextArea};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Identity,
    Relay,
    Discover,
}

pub struct LoginComponent {
    step: Step,
    /// Masked nsec textarea (Step 1).
    nsec_ta: TextArea<'static>,
    /// Plain relay URL textarea (Step 2).
    relay_ta: TextArea<'static>,
    /// Validated nsec carried forward from Step 1.
    nsec: Option<String>,
    /// Error propagated from App (NMP init failure).
    error: Option<String>,
    /// Inline validation error shown before the action is submitted.
    inline_error: Option<String>,
    /// Whether the app already has rooms (drives Step 3 skip).
    has_rooms: bool,
}

impl Default for LoginComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl LoginComponent {
    pub fn new() -> Self {
        let mut nsec_ta = TextArea::default();
        nsec_ta.set_mask_char('\u{2022}');
        nsec_ta.set_placeholder_text("nsec1\u{2026}");

        let mut relay_ta = TextArea::default();
        relay_ta.insert_str(nmp_app_29er::config::public_group_relay_url());

        Self {
            step: Step::Identity,
            nsec_ta,
            relay_ta,
            nsec: None,
            error: None,
            inline_error: None,
            has_rooms: false,
        }
    }

    pub fn update(&mut self, s: &TuiSnapshot) {
        self.error = s.login_error.clone();
        self.has_rooms = !s.channel_tree.is_empty();
    }

    // ── private layout helpers ──────────────────────────────────────────────

    fn center_form(area: Rect) -> (Rect, Rect, Rect) {
        let v = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(28),
                Constraint::Length(1), // header
                Constraint::Length(1), // spacer
                Constraint::Length(3), // input field
                Constraint::Length(1), // spacer
                Constraint::Length(1), // hint / error
                Constraint::Length(1), // subtext
                Constraint::Min(0),
            ])
            .split(area);
        let h = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(15),
                Constraint::Percentage(70),
                Constraint::Percentage(15),
            ])
            .split(v[3]);
        (v[1], h[1], v[5])
    }

    fn render_step_badge(f: &mut Frame, area: Rect, num: u8) {
        let label = format!("Step {num}/3  ");
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                label,
                Style::default().fg(ui::OVERLAY),
            )))
            .alignment(Alignment::Right),
            area,
        );
    }

    fn current_relay(&self) -> String {
        self.relay_ta.lines().join("").trim().to_string()
    }

    // ── step draw functions ─────────────────────────────────────────────────

    fn draw_identity(&mut self, f: &mut Frame, area: Rect) {
        let (header_area, field_area, hint_area) = Self::center_form(area);

        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "Welcome to 29er \u{2014} Nostr Group Chat",
                Style::default()
                    .fg(ui::LAVENDER)
                    .add_modifier(Modifier::BOLD),
            )))
            .alignment(Alignment::Center),
            header_area,
        );

        let border_color = if self.inline_error.is_some() || self.error.is_some() {
            ui::RED
        } else {
            ui::LAVENDER
        };
        let block = Block::default()
            .title(" secret key ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border_color));
        self.nsec_ta.set_block(block);
        f.render_widget(&self.nsec_ta, field_area);

        let hint_line = if let Some(e) = self.inline_error.as_deref().or(self.error.as_deref()) {
            Line::from(vec![
                Span::styled("\u{2717} ", Style::default().fg(ui::RED)),
                Span::styled(e.to_owned(), Style::default().fg(ui::RED)),
            ])
        } else {
            Line::from(Span::styled(
                "Paste your nsec1\u{2026} key to sign in  \u{2022}  Esc to quit",
                Style::default().fg(ui::SUBTEXT),
            ))
        };
        f.render_widget(
            Paragraph::new(hint_line).alignment(Alignment::Center),
            hint_area,
        );

        // Subtext row (one below hint_area)
        let subtext_area = Rect {
            y: hint_area.y + 1,
            ..hint_area
        };
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "Your key never leaves this device",
                Style::default().fg(ui::OVERLAY),
            )))
            .alignment(Alignment::Center),
            subtext_area,
        );
    }

    fn draw_relay(&mut self, f: &mut Frame, area: Rect) {
        let (header_area, field_area, hint_area) = Self::center_form(area);

        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "Connect to a relay",
                Style::default()
                    .fg(ui::LAVENDER)
                    .add_modifier(Modifier::BOLD),
            )))
            .alignment(Alignment::Center),
            header_area,
        );

        let border_color = if self.inline_error.is_some() || self.error.is_some() {
            ui::RED
        } else {
            ui::LAVENDER
        };
        let block = Block::default()
            .title(" relay url ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border_color));
        self.relay_ta.set_block(block);
        f.render_widget(&self.relay_ta, field_area);

        let hint_line = if let Some(e) = self.inline_error.as_deref().or(self.error.as_deref()) {
            Line::from(vec![
                Span::styled("\u{2717} ", Style::default().fg(ui::RED)),
                Span::styled(e.to_owned(), Style::default().fg(ui::RED)),
            ])
        } else {
            Line::from(Span::styled(
                "Relay comes from 29er config; edit to use another relay  \u{2022}  Esc to go back",
                Style::default().fg(ui::SUBTEXT),
            ))
        };
        f.render_widget(
            Paragraph::new(hint_line).alignment(Alignment::Center),
            hint_area,
        );
    }

    fn draw_discover(&mut self, f: &mut Frame, area: Rect) {
        let v = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(35),
                Constraint::Length(1), // header
                Constraint::Length(1), // spacer
                Constraint::Length(1), // body
                Constraint::Length(1), // spacer
                Constraint::Length(1), // hint
                Constraint::Min(0),
            ])
            .split(area);

        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "Connect and discover rooms?",
                Style::default()
                    .fg(ui::LAVENDER)
                    .add_modifier(Modifier::BOLD),
            )))
            .alignment(Alignment::Center),
            v[1],
        );

        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "29er will scan the relay and list available rooms.",
                Style::default().fg(ui::TEXT),
            )))
            .alignment(Alignment::Center),
            v[3],
        );

        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "Enter to connect  \u{2022}  Esc to go back",
                Style::default().fg(ui::SUBTEXT),
            )))
            .alignment(Alignment::Center),
            v[5],
        );
    }
}

impl Component for LoginComponent {
    fn draw(&mut self, f: &mut Frame, area: Rect) {
        f.render_widget(Clear, area);

        let step_num: u8 = match self.step {
            Step::Identity => 1,
            Step::Relay => 2,
            Step::Discover => 3,
        };
        Self::render_step_badge(f, area, step_num);

        match self.step {
            Step::Identity => self.draw_identity(f, area),
            Step::Relay => self.draw_relay(f, area),
            Step::Discover => self.draw_discover(f, area),
        }
    }

    fn handle_event(&mut self, event: &Event) -> Option<Action> {
        let Event::Key(key) = event else { return None };
        if key.kind != KeyEventKind::Press {
            return None;
        }

        match self.step {
            Step::Identity => match key.code {
                KeyCode::Esc => Some(Action::Quit),
                KeyCode::Enter => {
                    let nsec = self.nsec_ta.lines().join("").trim().to_string();
                    if !nsec.starts_with("nsec1") || nsec.len() < 6 {
                        self.inline_error =
                            Some("Secret key must start with nsec1\u{2026}".to_string());
                        return None;
                    }
                    self.nsec = Some(nsec);
                    self.inline_error = None;
                    self.error = None;
                    self.step = Step::Relay;
                    None
                }
                KeyCode::Char(c) => {
                    self.nsec_ta.insert_char(c);
                    self.inline_error = None;
                    None
                }
                KeyCode::Backspace => {
                    self.nsec_ta.delete_char();
                    None
                }
                KeyCode::Left => {
                    self.nsec_ta.move_cursor(CursorMove::Back);
                    None
                }
                KeyCode::Right => {
                    self.nsec_ta.move_cursor(CursorMove::Forward);
                    None
                }
                _ => None,
            },

            Step::Relay => match key.code {
                KeyCode::Esc => {
                    self.step = Step::Identity;
                    None
                }
                KeyCode::Enter => {
                    let relay = self.current_relay();
                    if relay.is_empty() {
                        self.inline_error = Some("Relay URL is required".to_string());
                        return None;
                    }
                    self.inline_error = None;
                    self.error = None;
                    if self.has_rooms {
                        // Existing session — skip Step 3 and go straight to app.
                        let nsec = self.nsec.take().unwrap_or_default();
                        Some(Action::LoginSubmit { nsec, relay })
                    } else {
                        // First run — show the Discover prompt.
                        self.step = Step::Discover;
                        None
                    }
                }
                KeyCode::Char(c) => {
                    self.relay_ta.insert_char(c);
                    self.inline_error = None;
                    None
                }
                KeyCode::Backspace => {
                    self.relay_ta.delete_char();
                    None
                }
                KeyCode::Left => {
                    self.relay_ta.move_cursor(CursorMove::Back);
                    None
                }
                KeyCode::Right => {
                    self.relay_ta.move_cursor(CursorMove::Forward);
                    None
                }
                _ => None,
            },

            Step::Discover => match key.code {
                KeyCode::Esc => {
                    self.step = Step::Relay;
                    None
                }
                KeyCode::Enter => {
                    let relay = self.current_relay();
                    if relay.is_empty() {
                        self.inline_error = Some("Relay URL is required".to_string());
                        self.step = Step::Relay;
                        return None;
                    }
                    let nsec = self.nsec.take().unwrap_or_default();
                    Some(Action::LoginSubmit { nsec, relay })
                }
                _ => None,
            },
        }
    }
}
