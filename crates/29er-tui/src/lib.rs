//! 29er-tui — a native Rust Ratatui shell over `nmp-app-29er`.
pub mod actions;
pub mod app;
pub mod components;
pub mod terminal;
pub mod ui;

use crossterm::event::Event;
use ratatui::layout::Rect;
use ratatui::Frame;
use crate::actions::Action;

/// A self-contained UI region. `draw` is pure rendering; `handle_event`
/// translates a terminal event into an optional [`Action`].
pub trait Component {
    fn draw(&mut self, f: &mut Frame, area: Rect);
    fn handle_event(&mut self, _event: &Event) -> Option<Action> { None }
}

#[cfg(test)]
mod tests;
