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
    /// Text input submitted with action ID and values
    InputSubmitted { action_id: String, values: Vec<String> },
    /// Text input cancelled
    InputCancelled,
    /// Confirmation dialog confirmed with action ID and context
    ConfirmSubmitted { action_id: String, context: Vec<String> },
    /// Confirmation dialog cancelled
    ConfirmCancelled,
    /// No action taken
    Ignored,
}

/// Keybind mapping from key events to action strings
pub type KeybindMap = HashMap<KeyEvent, String>;

/// Check if an action requires text input before executing
#[must_use]
fn action_requires_input(action: &str) -> bool {
    matches!(
        action,
        "add_tag" | "remove_tag" | "rename_tag" | "copy_tags" | "set_tags"
    )
}

/// Get the prompt title and placeholder text for an input-requiring action
#[must_use]
fn get_input_prompt_for_action(action: &str) -> (String, String) {
    match action {
        "add_tag" => ("Add Tags".to_string(), "Enter tags (space-separated)".to_string()),
        "remove_tag" => ("Remove Tags".to_string(), "Enter tags to remove".to_string()),
        "rename_tag" => ("Rename Tag".to_string(), "old_name new_name".to_string()),
        "copy_tags" => ("Copy Tags From".to_string(), "Enter source file path".to_string()),
        "set_tags" => ("Set Tags".to_string(), "Enter tags (replaces existing)".to_string()),
        _ => ("Input".to_string(), "Enter value".to_string()),
    }
}

/// Check if an action requires user confirmation before executing
#[must_use]
fn action_requires_confirmation(action: &str) -> bool {
    matches!(action, "delete_from_db")
}

/// Get the confirmation dialog title and message for an action
#[must_use]
fn get_confirm_prompt_for_action(action: &str, selected_count: usize) -> (String, String) {
    match action {
        "delete_from_db" => {
            let title = "Delete from Database".to_string();
            let message = if selected_count == 1 {
                "Remove this file from the tagr database?".to_string()
            } else {
                format!("Remove {} files from the tagr database?", selected_count)
            };
            (title, message)
        }
        _ => ("Confirm Action".to_string(), "Are you sure?".to_string()),
    }
}

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
        // Actions that require text input open the modal
        if action_requires_input(action) {
            let (title, _placeholder) = get_input_prompt_for_action(action);

            // Get tags on selected file(s)
            let file_tags = state.get_selected_items_tags();

            // For remove_tag: show only tags on the file(s), no exclusions
            // For add_tag: show all available tags, exclude those already on file(s)
            let (autocomplete_items, excluded_tags) = if action == "remove_tag" {
                (file_tags, Vec::new())
            } else if action.contains("tag") {
                (state.available_tags.clone(), file_tags)
            } else {
                (Vec::new(), Vec::new())
            };

            // enter_text_input(prompt, action_id, autocomplete_items, excluded_tags, multi_value)
            state.enter_text_input(title, action.clone(), autocomplete_items, excluded_tags, true);
            return EventResult::Continue;
        }

        // Actions that require confirmation open the confirm dialog
        if action_requires_confirmation(action) {
            let selected_keys = state.selected_keys();
            let selected_count = selected_keys.len();
            if selected_count > 0 {
                let (title, message) = get_confirm_prompt_for_action(action, selected_count);
                state.enter_confirm(title, message, action.clone(), selected_keys);
                return EventResult::Continue;
            }
        }

        return EventResult::Confirm(Some(action.clone()));
    }

    // Handle standard keybinds
    match (key.code, key.modifiers) {
        // Exit
        (KeyCode::Esc, _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => EventResult::Abort,
        (KeyCode::Enter, _) => EventResult::Confirm(Some("enter".to_string())),

        // Preview scroll (Shift+Up/Down) - must be before general navigation
        (KeyCode::Up, KeyModifiers::SHIFT) => {
            state.preview_scroll = state.preview_scroll.saturating_sub(1);
            EventResult::Continue
        }
        (KeyCode::Down, KeyModifiers::SHIFT) => {
            state.preview_scroll += 1;
            EventResult::Continue
        }

        // Navigation
        (KeyCode::Up, KeyModifiers::NONE | KeyModifiers::CONTROL)
        | (KeyCode::Char('k'), KeyModifiers::CONTROL) => {
            state.cursor_up();
            EventResult::Continue
        }
        (KeyCode::Down, KeyModifiers::NONE | KeyModifiers::CONTROL)
        | (KeyCode::Char('j'), KeyModifiers::CONTROL) => {
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
        (KeyCode::F(1) | KeyCode::Char('?'), _) => {
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

/// Handle events in refine search mode
fn handle_refine_search_mode(state: &mut AppState, key: KeyEvent) -> EventResult {
    let Some(refine_state) = state.refine_search_state_mut() else {
        state.mode = Mode::Normal;
        return EventResult::Continue;
    };

    if refine_state.in_selection {
        // In sub-selection mode (selecting items from list)
        match (key.code, key.modifiers) {
            // Exit sub-selection and apply changes
            (KeyCode::Enter | KeyCode::Esc, _) => {
                refine_state.exit_selection();
                EventResult::Continue
            }
            // Toggle current item
            (KeyCode::Tab, _) => {
                refine_state.toggle_current_selection();
                refine_state.selection_down();
                EventResult::Continue
            }
            // Navigate up
            (KeyCode::Up | KeyCode::Char('k'), _) => {
                refine_state.selection_up();
                EventResult::Continue
            }
            // Navigate down
            (KeyCode::Down | KeyCode::Char('j'), _) => {
                refine_state.selection_down();
                EventResult::Continue
            }
            // Filter query
            (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                refine_state.query_push(c);
                EventResult::Continue
            }
            (KeyCode::Backspace, _) => {
                refine_state.query_backspace();
                EventResult::Continue
            }
            _ => EventResult::Continue,
        }
    } else {
        // In field selection mode
        match (key.code, key.modifiers) {
            // Exit refine search and apply changes
            (KeyCode::Esc, _) => {
                // Apply changes - this will be handled by the finder
                // We signal a special action
                state.mode = Mode::Normal;
                EventResult::Confirm(Some("refine_search_done".to_string()))
            }
            // Navigate fields
            (KeyCode::Up | KeyCode::Char('k'), _) => {
                refine_state.prev_field();
                EventResult::Continue
            }
            (KeyCode::Down | KeyCode::Char('j'), _) => {
                refine_state.next_field();
                EventResult::Continue
            }
            // Enter edit/selection mode for current field
            (KeyCode::Enter, _) => {
                refine_state.enter_selection();
                EventResult::Continue
            }
            _ => EventResult::Continue,
        }
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

/// Handle events in text input mode
fn handle_input_mode(state: &mut AppState, key: KeyEvent) -> EventResult {
    let Some(input_state) = state.text_input_state_mut() else {
        state.mode = Mode::Normal;
        return EventResult::Continue;
    };

    match (key.code, key.modifiers) {
        // Cancel input
        (KeyCode::Esc, _) => {
            state.cancel_text_input();
            EventResult::InputCancelled
        }

        // Submit input
        (KeyCode::Enter, _) => {
            let values = input_state.values();
            let action_id = input_state.action_id.clone();

            // Don't submit empty values
            if values.is_empty() {
                state.cancel_text_input();
                return EventResult::InputCancelled;
            }

            let _ = state.exit_text_input();
            EventResult::InputSubmitted { action_id, values }
        }

        // Accept autocomplete suggestion
        (KeyCode::Tab, _) => {
            if input_state.show_suggestions {
                input_state.accept_suggestion();
            }
            EventResult::Continue
        }

        // Navigate suggestions (when visible)
        (KeyCode::Up, _) if input_state.show_suggestions => {
            input_state.suggestion_up();
            EventResult::Continue
        }
        (KeyCode::Down, _) if input_state.show_suggestions => {
            input_state.suggestion_down();
            EventResult::Continue
        }

        // Cursor movement
        (KeyCode::Left, _) => {
            input_state.cursor_left();
            EventResult::Continue
        }
        (KeyCode::Right, _) => {
            input_state.cursor_right();
            EventResult::Continue
        }
        (KeyCode::Home, _) => {
            input_state.cursor_home();
            EventResult::Continue
        }
        (KeyCode::End, _) => {
            input_state.cursor_end();
            EventResult::Continue
        }

        // Text editing
        (KeyCode::Backspace, _) => {
            input_state.backspace();
            EventResult::Continue
        }
        (KeyCode::Delete, _) => {
            input_state.delete();
            EventResult::Continue
        }
        (KeyCode::Char('w'), KeyModifiers::CONTROL) => {
            input_state.delete_word_backwards();
            EventResult::Continue
        }
        (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
            input_state.clear_line();
            EventResult::Continue
        }

        // Character input
        (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
            input_state.insert_char(c);
            EventResult::Continue
        }

        _ => EventResult::Continue,
    }
}

/// Handle events in confirm mode
fn handle_confirm_mode(state: &mut AppState, key: KeyEvent) -> EventResult {
    match (key.code, key.modifiers) {
        // Cancel confirmation
        (KeyCode::Esc, _) | (KeyCode::Char('n'), _) | (KeyCode::Char('N'), _) => {
            state.cancel_confirm();
            EventResult::ConfirmCancelled
        }

        // Confirm action
        (KeyCode::Enter, _) | (KeyCode::Char('y'), _) | (KeyCode::Char('Y'), _) => {
            if let Some(confirm_state) = state.exit_confirm() {
                EventResult::ConfirmSubmitted {
                    action_id: confirm_state.action_id,
                    context: confirm_state.context,
                }
            } else {
                state.cancel_confirm();
                EventResult::ConfirmCancelled
            }
        }

        _ => EventResult::Continue,
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
            Mode::RefineSearch => handle_refine_search_mode(state, key),
            Mode::Input => handle_input_mode(state, key),
            Mode::Confirm => handle_confirm_mode(state, key),
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
    fn test_custom_keybind_opens_input_modal() {
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
        // add_tag requires input, so it opens the input modal instead of confirming
        assert_eq!(result, EventResult::Continue);
        assert_eq!(state.mode, Mode::Input);
        assert!(state.text_input_state().is_some());
        assert_eq!(state.text_input_state().unwrap().action_id, "add_tag");
    }

    #[test]
    fn test_custom_keybind_direct_action() {
        let mut state = make_state();
        let mut binds = KeybindMap::new();
        // open_file doesn't require input
        binds.insert(
            KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL),
            "open_file".to_string(),
        );

        let result = handle_normal_mode(
            &mut state,
            KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL),
            &binds,
        );
        assert_eq!(result, EventResult::Confirm(Some("open_file".to_string())));
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
