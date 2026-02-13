use anyhow::Result;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
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

use event::run_app;
use state::{App, SyncStatus};

pub fn run_tui(db: &Database) -> Result<()> {
    enable_raw_mode()?;
    let mut stderr = io::stderr();
    execute!(stderr, EnterAlternateScreen, EnableMouseCapture)?;

    let backend = CrosstermBackend::new(stderr);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(db);
    // Initial sync on launch
    if app.sync_enabled() {
        app.trigger_sync();
    }
    app.refresh_all()?;

    let res = run_app(&mut terminal, &mut app);

    // Final sync before quit
    if app.sync_enabled() {
        app.trigger_sync();
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    // Print sync result after terminal teardown
    if app.sync_enabled() {
        match &app.sync_status {
            SyncStatus::Synced(_) => eprintln!("Synced with Turso."),
            SyncStatus::Error(e) => eprintln!("Warning: final sync failed: {}", e),
            _ => {}
        }
    }

    res
}
