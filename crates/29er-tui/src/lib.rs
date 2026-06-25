//! 29er-tui — a native Rust Ratatui shell over `nmp-app-29er`.
//!
//! Architecture (NMP doctrine: kernel emits, per-app crate composes, shell
//! only renders): the runtime loop lives in `main.rs`, the NMP integration +
//! read-model snapshotting lives in [`app`], terminal lifecycle in
//! [`terminal`], and pure rendering in [`ui`]. Render code calls
//! `projection.snapshot()` each tick and never contains business logic.

pub mod actions;
pub mod app;
pub mod terminal;
pub mod ui;

use crossterm::event::Event;
use ratatui::layout::Rect;
use ratatui::Frame;

use crate::actions::Action;

/// A self-contained UI region. `draw` is pure rendering; `handle_event`
/// translates a terminal event into an optional [`Action`] for the runtime
/// loop to apply. Components hold a cloned view-model refreshed via their own
/// `update(&AppState)` method before each draw.
pub trait Component {
    fn draw(&mut self, f: &mut Frame, area: Rect);
    fn handle_event(&mut self, _event: &Event) -> Option<Action> {
        None
    }
}
