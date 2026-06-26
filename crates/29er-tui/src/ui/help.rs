//! Help overlay: centered bordered block listing all keybindings in two columns.
//! Opens on `?`, dismissed with `Esc` or `?`.
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;
use crate::actions::Action;
use crate::app::TuiSnapshot;
use crate::ui;
use crate::Component;
use crossterm::event::{Event, KeyCode, KeyEventKind};

/// (key, description) pairs for the help table.
static KEYBINDS: &[(&str, &str)] = &[
    // Navigation
    ("j / ↓",        "Move down in list"),
    ("k / ↑",        "Move up in list"),
    ("g",            "Go to top of list"),
    ("G",            "Go to bottom of list"),
    ("Enter",        "Open selected channel"),
    // Focus
    ("Tab",          "Next panel"),
    ("Shift+Tab",    "Previous panel"),
    ("n",            "Focus composer"),
    ("Esc",          "Back / pop focus"),
    // Chat
    ("j / PgDn",     "Scroll chat down"),
    ("k / PgUp",     "Scroll chat up"),
    // Palette / commands
    ("/ or Ctrl+K",  "Open command palette"),
    // Misc
    ("?",            "Toggle this help"),
    ("Ctrl+C/Q",     "Quit"),
];

#[derive(Default)]
pub struct HelpOverlay;

impl HelpOverlay {
    pub fn new() -> Self { Self }
    pub fn update(&mut self, _s: &TuiSnapshot) {}
}

impl Component for HelpOverlay {
    fn draw(&mut self, f: &mut Frame, area: Rect) {
        // Center a 60×20 block.
        let modal_w = 62u16.min(area.width.saturating_sub(4));
        let modal_h = 22u16.min(area.height.saturating_sub(4));
        let v = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length((area.height.saturating_sub(modal_h)) / 2),
                Constraint::Length(modal_h),
                Constraint::Min(0),
            ])
            .split(area);
        let h = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length((area.width.saturating_sub(modal_w)) / 2),
                Constraint::Length(modal_w),
                Constraint::Min(0),
            ])
            .split(v[1]);
        let modal = h[1];

        f.render_widget(Clear, modal);

        let block = Block::default()
            .title(" Keybindings  (Esc or ? to close) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(ui::MAUVE));

        let inner = block.inner(modal);
        f.render_widget(block, modal);

        // Split into two equal columns.
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(inner);

        let half = KEYBINDS.len() / 2 + KEYBINDS.len() % 2;

        for (col_idx, (col_rect, slice_start)) in
            [(cols[0], 0), (cols[1], half)].iter().enumerate()
        {
            let end = if col_idx == 0 { half } else { KEYBINDS.len() };
            let lines: Vec<Line> = KEYBINDS[*slice_start..end]
                .iter()
                .map(|(key, desc)| {
                    Line::from(vec![
                        Span::styled(
                            format!("{:<16}", key),
                            Style::default().fg(ui::LAVENDER).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(*desc, Style::default().fg(ui::TEXT)),
                    ])
                })
                .collect();
            f.render_widget(Paragraph::new(lines), *col_rect);
        }
    }

    fn handle_event(&mut self, event: &Event) -> Option<Action> {
        let Event::Key(key) = event else { return None };
        if key.kind != KeyEventKind::Press { return None; }
        match key.code {
            KeyCode::Esc | KeyCode::Char('?') => Some(Action::CloseHelp),
            _ => None,
        }
    }
}
