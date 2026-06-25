use crossterm::event::Event;
use ratatui::layout::Rect;
use ratatui::Frame;
use crate::actions::Action;
use crate::app::TuiSnapshot;
use crate::Component;
#[derive(Default)]
pub struct Membership;
impl Membership { pub fn new() -> Self { Self } pub fn update(&mut self, _s: &TuiSnapshot) {} }
impl Component for Membership { fn draw(&mut self, _f: &mut Frame, _area: Rect) {} fn handle_event(&mut self, _e: &Event) -> Option<Action> { None } }
