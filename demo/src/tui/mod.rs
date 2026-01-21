//! TUI module for the demo.

mod app;
mod input;
mod view;

use std::{io, time::Duration};

pub use app::App;
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

/// Runs the TUI main loop.
pub async fn run(mut app: App) -> io::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Main loop
    let tick_rate = Duration::from_millis(100);

    loop {
        // Update views
        app.update_views().await;

        // Draw
        terminal.draw(|frame| {
            view::render(frame, app.views(), app.status());
        })?;

        // Poll for events
        if event::poll(tick_rate)? {
            if let Ok(evt) = event::read() {
                if let Event::Key(_) = evt {
                    input::handle_event(&mut app, evt);
                }
            }
        }

        if app.should_quit() {
            break;
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}
