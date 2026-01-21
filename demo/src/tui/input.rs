//! Input handling for the TUI.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use super::app::App;

/// Handles an input event.
///
/// Returns `true` if the event was handled.
pub fn handle_event(app: &mut App, event: Event) -> bool {
    match event {
        Event::Key(key) => handle_key(app, key),
        _ => false,
    }
}

/// Handles a key event.
fn handle_key(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Char('q') => {
            app.quit();
            true
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.quit();
            true
        }
        KeyCode::Esc => {
            app.quit();
            true
        }
        _ => false,
    }
}
