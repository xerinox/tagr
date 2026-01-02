//! Event handling for the ratatui TUI
//!
//! Handles keyboard and mouse events, mapping them to application actions.

use super::state::{AppState, Mode};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use std::collections::HashMap;
use std::time::Duration;

/// Result of handling an event
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventResult {
    /// Continue running the event loop
    Continue,
    /// Exit the finder with confirmation
    Confirm(Option<String>),
    /// Exit the finder as aborted
    Abort,
    /// Query changed, needs re-matching
    QueryChanged,
    /// No action taken
    Ignored,
}

/// Keybind mapping from key events to action strings
pub type KeybindMap = HashMap<KeyEvent, String>;

/// Convert a key event to a string representation (for `final_key`)
#[must_use]
pub fn key_to_string(key: &KeyEvent) -> Option<String> {
    let base = match key.code {
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Enter => "enter".to_string(),
        KeyCode::Esc => "esc".to_string(),
        KeyCode::Backspace => "bspace".to_string(),
        KeyCode::Delete => "del".to_string(),
        KeyCode::Up => "up".to_string(),
        KeyCode::Down => "down".to_string(),
        KeyCode::Left => "left".to_string(),
        KeyCode::Right => "right".to_string(),
        KeyCode::Home => "home".to_string(),
        KeyCode::End => "end".to_string(),
        KeyCode::PageUp => "pgup".to_string(),
        KeyCode::PageDown => "pgdn".to_string(),
        KeyCode::Tab => "tab".to_string(),
        KeyCode::BackTab => "btab".to_string(),
        KeyCode::F(n) => format!("f{n}"),
        _ => return None,
    };

    // Add modifier prefixes
    let mut result = String::new();
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        result.push_str("ctrl-");
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        result.push_str("alt-");
    }
    if key.modifiers.contains(KeyModifiers::SHIFT)
        && !matches!(key.code, KeyCode::Char(_) | KeyCode::BackTab)
    {
        result.push_str("shift-");
    }
    result.push_str(&base);

    Some(result)
}

/// Handle events in normal mode
fn handle_normal_mode(
    state: &mut AppState,
    key: KeyEvent,
    custom_binds: &KeybindMap,
) -> EventResult {
    // Check custom keybinds first
    if let Some(action) = custom_binds.get(&key) {
        return EventResult::Confirm(Some(action.clone()));
    }

    // Handle standard keybinds
    match (key.code, key.modifiers) {
        // Exit
        (KeyCode::Esc, _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => EventResult::Abort,
        (KeyCode::Enter, _) => EventResult::Confirm(Some("enter".to_string())),

        // Navigation
        (KeyCode::Up, _) | (KeyCode::Char('k'), KeyModifiers::CONTROL) => {
            state.cursor_up();
            EventResult::Continue
        }
        (KeyCode::Down, _) | (KeyCode::Char('j'), KeyModifiers::CONTROL) => {
            state.cursor_down();
            EventResult::Continue
        }
        (KeyCode::PageUp, _) => {
            state.page_up();
            EventResult::Continue
        }
        (KeyCode::PageDown, _) => {
            state.page_down();
            EventResult::Continue
        }
        (KeyCode::Home, _) => {
            state.jump_to_start();
            EventResult::Continue
        }
        (KeyCode::End, _) => {
            state.jump_to_end();
            EventResult::Continue
        }

        // Multi-select
        (KeyCode::Tab, _) => {
            state.toggle_selection();
            state.cursor_down();
            EventResult::Continue
        }
        (KeyCode::BackTab, _) => {
            state.toggle_selection();
            state.cursor_up();
            EventResult::Continue
        }

        // Help overlay
        (KeyCode::F(1), _) | (KeyCode::Char('?'), _) => {
            state.mode = Mode::Help;
            EventResult::Continue
        }

        // Query editing
        (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
            state.query_push(c);
            EventResult::QueryChanged
        }
        (KeyCode::Backspace, _) => {
            if state.query.is_empty() {
                EventResult::Ignored
            } else {
                state.query_backspace();
                EventResult::QueryChanged
            }
        }
        (KeyCode::Delete, _) => {
            if state.query_cursor >= state.query.len() {
                EventResult::Ignored
            } else {
                state.query_delete();
                EventResult::QueryChanged
            }
        }
        (KeyCode::Left, _) => {
            state.query_cursor_left();
            EventResult::Continue
        }
        (KeyCode::Right, _) => {
            state.query_cursor_right();
            EventResult::Continue
        }
        (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
            state.query_clear();
            EventResult::QueryChanged
        }
        (KeyCode::Char('w'), KeyModifiers::CONTROL) => {
            // Delete word backwards
            let trimmed = state.query[..state.query_cursor].trim_end();
            if let Some(last_space) = trimmed.rfind(' ') {
                state.query.drain(last_space + 1..state.query_cursor);
                state.query_cursor = last_space + 1;
            } else {
                state.query.drain(..state.query_cursor);
                state.query_cursor = 0;
            }
            EventResult::QueryChanged
        }

        // Preview scroll
        (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
            state.preview_scroll = state.preview_scroll.saturating_sub(1);
            EventResult::Continue
        }
        (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
            state.preview_scroll += 1;
            EventResult::Continue
        }

        _ => EventResult::Ignored,
    }
}

/// Handle events in help mode
fn handle_help_mode(state: &mut AppState, key: KeyEvent) -> EventResult {
    // Any key closes help
    state.mode = Mode::Normal;
    if key.code == KeyCode::Esc {
        EventResult::Continue
    } else {
        EventResult::Continue
    }
}

/// Handle mouse events
fn handle_mouse(state: &mut AppState, mouse: MouseEvent) -> EventResult {
    match mouse.kind {
        MouseEventKind::ScrollUp => {
            state.cursor_up();
            EventResult::Continue
        }
        MouseEventKind::ScrollDown => {
            state.cursor_down();
            EventResult::Continue
        }
        // Click to select could be added here
        _ => EventResult::Ignored,
    }
}

/// Poll for events and handle them
///
/// # Errors
///
/// Returns an error if event polling fails.
pub fn poll_and_handle(
    state: &mut AppState,
    custom_binds: &KeybindMap,
    timeout: Duration,
) -> std::io::Result<EventResult> {
    if !event::poll(timeout)? {
        return Ok(EventResult::Continue);
    }

    let result = match event::read()? {
        Event::Key(key) => match state.mode {
            Mode::Normal => handle_normal_mode(state, key, custom_binds),
            Mode::Help => handle_help_mode(state, key),
            Mode::Input | Mode::Confirm => {
                // These modes would be handled by modal widgets
                EventResult::Ignored
            }
        },
        Event::Mouse(mouse) => handle_mouse(state, mouse),
        Event::Resize(_, _) => EventResult::Continue,
        _ => EventResult::Ignored,
    };

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::DisplayItem;

    fn make_state() -> AppState {
        let items: Vec<DisplayItem> = (0..10)
            .map(|i| DisplayItem::new(format!("item{i}"), format!("Item {i}"), format!("item{i}")))
            .collect();
        AppState::new(items, true)
    }

    #[test]
    fn test_key_to_string() {
        assert_eq!(
            key_to_string(&KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Some("enter".to_string())
        );
        assert_eq!(
            key_to_string(&KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL)),
            Some("ctrl-t".to_string())
        );
        assert_eq!(
            key_to_string(&KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE)),
            Some("f1".to_string())
        );
        assert_eq!(
            key_to_string(&KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
            Some("tab".to_string())
        );
    }

    #[test]
    fn test_navigation_handling() {
        let mut state = make_state();
        let binds = KeybindMap::new();

        // Down arrow
        let result =
            handle_normal_mode(&mut state, KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), &binds);
        assert_eq!(result, EventResult::Continue);
        assert_eq!(state.cursor, 1);

        // Up arrow
        let result =
            handle_normal_mode(&mut state, KeyEvent::new(KeyCode::Up, KeyModifiers::NONE), &binds);
        assert_eq!(result, EventResult::Continue);
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn test_custom_keybind() {
        let mut state = make_state();
        let mut binds = KeybindMap::new();
        binds.insert(
            KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL),
            "add_tag".to_string(),
        );

        let result = handle_normal_mode(
            &mut state,
            KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL),
            &binds,
        );
        assert_eq!(result, EventResult::Confirm(Some("add_tag".to_string())));
    }

    #[test]
    fn test_query_input() {
        let mut state = make_state();
        let binds = KeybindMap::new();

        let result = handle_normal_mode(
            &mut state,
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
            &binds,
        );
        assert_eq!(result, EventResult::QueryChanged);
        assert_eq!(state.query, "r");

        let result = handle_normal_mode(
            &mut state,
            KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE),
            &binds,
        );
        assert_eq!(result, EventResult::QueryChanged);
        assert_eq!(state.query, "ru");
    }

    #[test]
    fn test_abort() {
        let mut state = make_state();
        let binds = KeybindMap::new();

        let result =
            handle_normal_mode(&mut state, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), &binds);
        assert_eq!(result, EventResult::Abort);
    }
}
