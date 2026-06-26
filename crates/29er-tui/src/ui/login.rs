//! Full-screen nsec login (issue #10). Secure masked paste; validates the
//! nsec1 prefix on submit and emits `Action::LoginSubmit`.
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;
use ratatui_textarea::{CursorMove, TextArea};
use crate::actions::Action;
use crate::app::TuiSnapshot;
use crate::ui;
use crate::Component;

pub struct LoginComponent { textarea: TextArea<'static>, error: Option<String> }
impl Default for LoginComponent { fn default() -> Self { Self::new() } }
impl LoginComponent {
    pub fn new() -> Self {
        let mut ta = TextArea::default();
        ta.set_mask_char('\u{2022}');
        ta.set_placeholder_text("nsec1\u{2026}");
        Self { textarea: ta, error: None }
    }
    pub fn update(&mut self, s: &TuiSnapshot) { self.error = s.login_error.clone(); }
}
impl Component for LoginComponent {
    fn draw(&mut self, f: &mut Frame, area: Rect) {
        f.render_widget(Clear, area);
        let v = Layout::default().direction(Direction::Vertical).constraints([
            Constraint::Percentage(35), Constraint::Length(3), Constraint::Length(2), Constraint::Min(0),
        ]).split(area);
        let h = Layout::default().direction(Direction::Horizontal).constraints([
            Constraint::Percentage(20), Constraint::Percentage(60), Constraint::Percentage(20),
        ]).split(v[1]);
        f.render_widget(Paragraph::new(Line::from(Span::styled("29er \u{2014} paste your nsec to sign in", Style::default().fg(ui::MAUVE).add_modifier(Modifier::BOLD)))).alignment(ratatui::layout::Alignment::Center), v[0]);
        let block = Block::default().title(" secret key ").borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(ui::MAUVE));
        self.textarea.set_block(block);
        f.render_widget(&self.textarea, h[1]);
        let hint = match &self.error {
            Some(e) => Span::styled(e.clone(), Style::default().fg(ui::RED)),
            None => Span::styled("Enter to sign in \u{2022} Ctrl-C to quit", Style::default().fg(ui::SUBTEXT)),
        };
        f.render_widget(Paragraph::new(Line::from(hint)).alignment(ratatui::layout::Alignment::Center), v[2]);
    }
    fn handle_event(&mut self, event: &Event) -> Option<Action> {
        let Event::Key(key) = event else { return None };
        if key.kind != KeyEventKind::Press { return None; }
        match key.code {
            KeyCode::Enter => Some(Action::LoginSubmit(self.textarea.lines().join(""))),
            KeyCode::Char(c) => { self.textarea.insert_char(c); None }
            KeyCode::Backspace => { self.textarea.delete_char(); None }
            KeyCode::Left => { self.textarea.move_cursor(CursorMove::Back); None }
            KeyCode::Right => { self.textarea.move_cursor(CursorMove::Forward); None }
            _ => None,
        }
    }
}
