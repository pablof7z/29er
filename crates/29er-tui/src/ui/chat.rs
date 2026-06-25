//! Right pane: group-chat message history (top) and the message composer
//! (bottom). Message bodies are tokenized with `nmp_content` so URLs,
//! hashtags, mentions, and media render distinctly; author names get a stable
//! per-pubkey color.

use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};
use ratatui::Frame;
use tui_textarea::{CursorMove, TextArea};

use nmp_content::{tokenize, RenderMode, Segment};
use nmp_nip29::projection::GroupChatMessage;

use crate::actions::Action;
use crate::app::{AppState, Focus};
use crate::ui;
use crate::Component;

pub struct ChatComponent {
    messages: Vec<GroupChatMessage>,
    room_title: String,
    has_room: bool,
    focused: bool,
    textarea: TextArea<'static>,
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
            room_title: " chat ".to_string(),
            has_room: false,
            focused: false,
            textarea: Self::fresh_textarea(),
        }
    }

    fn fresh_textarea() -> TextArea<'static> {
        let mut textarea = TextArea::default();
        textarea.set_placeholder_text("Type a message, Enter to send\u{2026}");
        textarea
    }

    fn reset_input(&mut self) {
        self.textarea = Self::fresh_textarea();
    }

    pub fn update(&mut self, state: &AppState) {
        self.messages = state.messages.clone();
        self.focused = state.focus == Focus::Input;
        self.has_room = state.selected_room.is_some();
        self.room_title = match &state.selected_room {
            Some(g) => format!(" {} ", g.local_id),
            None => " chat ".to_string(),
        };
    }

    fn render_message(msg: &GroupChatMessage) -> Vec<Line<'static>> {
        let header = Line::from(vec![
            Span::styled(
                short_pubkey(&msg.pubkey),
                Style::default()
                    .fg(ui::author_color(&msg.pubkey))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                msg.created_at.to_string(),
                Style::default().fg(ui::OVERLAY),
            ),
        ]);
        let tree = tokenize(&msg.content, &[], RenderMode::Plain);
        vec![header, Line::from(render_segments(&tree.segments))]
    }

    fn draw_history(&self, f: &mut Frame, area: Rect) {
        let border_style = if self.focused {
            Style::default().fg(ui::MAUVE)
        } else {
            Style::default().fg(ui::OVERLAY)
        };
        let block = Block::default()
            .title(self.room_title.clone())
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(border_style);
        let inner = block.inner(area);

        let mut lines: Vec<Line> = Vec::new();
        if !self.has_room {
            lines.push(Line::from(Span::styled(
                "Select a room (focus the sidebar, press Enter) to view its chat.",
                Style::default().fg(ui::SUBTEXT),
            )));
        } else if self.messages.is_empty() {
            lines.push(Line::from(Span::styled(
                "No messages yet.",
                Style::default().fg(ui::SUBTEXT),
            )));
        } else {
            for msg in self.messages.iter().rev() {
                lines.extend(Self::render_message(msg));
                lines.push(Line::from(""));
            }
        }

        let total = lines.len() as u16;
        let scroll = total.saturating_sub(inner.height);
        let paragraph = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0));
        f.render_widget(paragraph, area);
    }

    fn draw_input(&mut self, f: &mut Frame, area: Rect) {
        let border_style = if self.focused {
            Style::default().fg(ui::MAUVE)
        } else {
            Style::default().fg(ui::OVERLAY)
        };
        let block = Block::default()
            .title(" message ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(border_style);
        self.textarea.set_block(block);
        f.render_widget(&self.textarea, area);
    }
}

impl Component for ChatComponent {
    fn draw(&mut self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(5)])
            .split(area);
        self.draw_history(f, chunks[0]);
        self.draw_input(f, chunks[1]);
    }

    fn handle_event(&mut self, event: &Event) -> Option<Action> {
        let Event::Key(key) = event else {
            return None;
        };
        if key.kind != KeyEventKind::Press {
            return None;
        }
        match key.code {
            KeyCode::Enter => {
                let text = self.textarea.lines().join("\n").trim().to_string();
                self.reset_input();
                if text.is_empty() {
                    None
                } else {
                    Some(Action::SendMessage(text))
                }
            }
            KeyCode::Char(c) => {
                self.textarea.insert_char(c);
                None
            }
            KeyCode::Backspace => {
                self.textarea.delete_char();
                None
            }
            KeyCode::Left => {
                self.textarea.move_cursor(CursorMove::Back);
                None
            }
            KeyCode::Right => {
                self.textarea.move_cursor(CursorMove::Forward);
                None
            }
            _ => None,
        }
    }
}

fn short_pubkey(pubkey: &str) -> String {
    if pubkey.len() <= 12 {
        pubkey.to_string()
    } else {
        format!("{}\u{2026}{}", &pubkey[..8], &pubkey[pubkey.len() - 4..])
    }
}

fn render_segments(segments: &[Segment]) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    for segment in segments {
        match segment {
            Segment::Text(t) => {
                spans.push(Span::styled(t.clone(), Style::default().fg(ui::TEXT)));
            }
            Segment::Hashtag(h) => {
                spans.push(Span::styled(
                    format!("#{h}"),
                    Style::default().fg(ui::YELLOW),
                ));
            }
            Segment::Url(url) => {
                spans.push(Span::styled(
                    url.to_string(),
                    Style::default()
                        .fg(ui::MAUVE)
                        .add_modifier(Modifier::UNDERLINED),
                ));
            }
            Segment::Mention(_) => {
                spans.push(Span::styled("@mention", Style::default().fg(ui::GREEN)));
            }
            Segment::EventRef(_) => {
                spans.push(Span::styled(
                    "\u{2197}event",
                    Style::default().fg(ui::GREEN),
                ));
            }
            Segment::Emoji { shortcode, .. } => {
                spans.push(Span::styled(
                    format!(":{shortcode}:"),
                    Style::default().fg(ui::YELLOW),
                ));
            }
            Segment::Media { urls, .. } => {
                for url in urls {
                    spans.push(Span::styled(
                        format!("[media] {url} "),
                        Style::default().fg(ui::MAUVE),
                    ));
                }
            }
            Segment::Invoice(_) => {
                spans.push(Span::styled("[invoice]", Style::default().fg(ui::YELLOW)));
            }
            Segment::MarkdownBlock(_) => {}
        }
    }
    if spans.is_empty() {
        spans.push(Span::raw(String::new()));
    }
    spans
}
