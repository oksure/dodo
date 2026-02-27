use anyhow::Result;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, EnableBracketedPaste, DisableBracketedPaste},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;

use dodo::db::Database;

mod constants;
mod draw;
mod event;
mod format;
mod state;
#[cfg(test)]
mod tests;

use event::run_app;
use state::App;

pub fn run_tui(db: &Database) -> Result<()> {
    enable_raw_mode()?;
    let mut stderr = io::stderr();
    execute!(stderr, EnterAlternateScreen, EnableMouseCapture, EnableBracketedPaste)?;

    let backend = CrosstermBackend::new(stderr);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(db);
    // Initial sync on launch (non-blocking — fires background thread)
    if app.sync_enabled() {
        app.trigger_sync();
    }
    app.refresh_all()?;

    let res = run_app(&mut terminal, &mut app);

    // No final sync — data is safe in local DB, unpushed changes sync on next startup

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        DisableBracketedPaste,
    )?;
    terminal.show_cursor()?;

    res
}
