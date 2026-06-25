//! RAII terminal lifecycle: enters raw mode + the alternate screen on
//! construction and restores the terminal on `Drop` AND on panic (via a
//! chained panic hook). This is the only place that talks to crossterm's
//! global terminal state, keeping the runtime loop free of teardown concerns.

use std::io::{self, Stdout};
use std::panic::{set_hook, take_hook};

use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::{Frame, Terminal};

pub type Backend = CrosstermBackend<Stdout>;

/// Owns the ratatui [`Terminal`] and guarantees terminal restoration.
pub struct TerminalHandle {
    terminal: Terminal<Backend>,
}

impl TerminalHandle {
    /// Install the panic hook, enter raw mode + the alternate screen, and
    /// build the ratatui terminal. Resize is handled implicitly by ratatui:
    /// each `draw` re-queries the backend size and recomputes the layout.
    pub fn new() -> io::Result<Self> {
        Self::install_panic_hook();
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let terminal = Terminal::new(CrosstermBackend::new(stdout))?;
        Ok(Self { terminal })
    }

    /// Render one frame.
    pub fn draw<F>(&mut self, render: F) -> io::Result<()>
    where
        F: FnOnce(&mut Frame),
    {
        self.terminal.draw(render)?;
        Ok(())
    }

    /// Clear the screen (used right before exit so the shell prompt is clean).
    pub fn clear(&mut self) -> io::Result<()> {
        self.terminal.clear()
    }

    /// Best-effort restoration of the user's terminal. Safe to call multiple
    /// times; both the panic hook and `Drop` route through here.
    fn restore() -> io::Result<()> {
        let mut stdout = io::stdout();
        execute!(stdout, LeaveAlternateScreen, DisableMouseCapture)?;
        disable_raw_mode()?;
        Ok(())
    }

    /// Chain a panic hook that restores the terminal before the default hook
    /// prints the panic message — otherwise a panic would leave the user in
    /// raw mode on the alternate screen.
    fn install_panic_hook() {
        let original = take_hook();
        set_hook(Box::new(move |info| {
            let _ = Self::restore();
            original(info);
        }));
    }
}

impl Drop for TerminalHandle {
    fn drop(&mut self) {
        let _ = Self::restore();
    }
}
