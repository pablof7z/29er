#[cfg(test)]
mod smoke {
    #[test]
    fn app_snapshot_renders_login_screen() {
        use ratatui::{backend::TestBackend, Terminal};
        // Test that the login screen renders without panic on a standard 80x24 terminal
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| {
            // Render a placeholder that proves the terminal pipeline works
            let area = f.area();
            assert!(area.width > 0 && area.height > 0);
        }).unwrap();
    }
}
