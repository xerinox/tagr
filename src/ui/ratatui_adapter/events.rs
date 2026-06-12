//! Event handling for the ratatui TUI
//!
//! Handles keyboard and mouse events, mapping them to application actions.

use super::state::{AppState, Mode};
use crate::keybinds::actions::BrowseAction;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use std::collections::HashMap;
use std::time::Duration;

/// Result of handling an event
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventResult {
    /// Continue running the event loop
    Continue,
    /// Exit with an action to execute (actions requiring special handling like `edit_note`)
    Action {
        action: BrowseAction,
        context: Vec<String>,
    },
    /// Exit with confirmation (enter key)
    Confirm,
    /// Exit the finder as aborted
    Abort,
    /// Query changed, needs re-matching
    QueryChanged,
    /// Preview mode changed, needs regeneration
    PreviewChanged,
    /// Text input submitted with action and values
    InputSubmitted {
        action: BrowseAction,
        values: Vec<String>,
        context: Vec<String>,
    },
    /// Text input cancelled
    InputCancelled,
    /// Confirmation dialog confirmed with action and context
    ConfirmSubmitted {
        action: BrowseAction,
        context: Vec<String>,
    },
    /// Confirmation dialog cancelled
    ConfirmCancelled,
    /// Refine search completed with updated criteria
    RefineSearchDone,
    /// No action taken
    Ignored,
}

/// Keybind mapping from key events to action strings
pub type KeybindMap = HashMap<KeyEvent, String>;

/// Convert a key event to a string representation (for `final_key` and help display)
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

/// Resolve a key event into a `BrowseAction` based on keybind configuration and app state.
///
/// Custom keybinds are checked first. Then default keybinds are resolved based on
/// the current application context (search active, tag selection phase, focused pane).
///
/// Returns `None` if the key has no associated action.
#[must_use]
fn resolve_action(
    key: KeyEvent,
    custom_binds: &KeybindMap,
    state: &AppState,
) -> Option<BrowseAction> {
    // Custom keybinds take priority
    if let Some(action_str) = custom_binds.get(&key)
        && let Ok(action) = action_str.parse::<BrowseAction>()
    {
        return Some(action);
    }

    resolve_default_keybind(key, state)
}

/// Map default (non-configurable) keybinds to actions based on app context.
const fn resolve_default_keybind(key: KeyEvent, state: &AppState) -> Option<BrowseAction> {
    match (key.code, key.modifiers) {
        // Exit / abort
        (KeyCode::Esc, _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
            if state.search_active {
                Some(BrowseAction::ExitSearch)
            } else {
                Some(BrowseAction::Abort)
            }
        }
        (KeyCode::Char('q'), KeyModifiers::NONE) if !state.search_active => {
            Some(BrowseAction::Abort)
        }

        // Confirm / exit search
        (KeyCode::Enter, _) => {
            if state.search_active {
                Some(BrowseAction::ExitSearch)
            } else {
                Some(BrowseAction::Confirm)
            }
        }

        // Preview scroll
        (KeyCode::Up, KeyModifiers::SHIFT) => Some(BrowseAction::ScrollPreviewUp),
        (KeyCode::Down, KeyModifiers::SHIFT) => Some(BrowseAction::ScrollPreviewDown),

        // Navigation (arrow keys + ctrl variants)
        (KeyCode::Up, KeyModifiers::NONE | KeyModifiers::CONTROL)
        | (KeyCode::Char('k'), KeyModifiers::CONTROL) => Some(BrowseAction::MoveUp),
        (KeyCode::Char('k'), KeyModifiers::NONE) if !state.search_active => {
            Some(BrowseAction::MoveUp)
        }
        (KeyCode::Down, KeyModifiers::NONE | KeyModifiers::CONTROL)
        | (KeyCode::Char('j'), KeyModifiers::CONTROL) => Some(BrowseAction::MoveDown),
        (KeyCode::Char('j'), KeyModifiers::NONE) if !state.search_active => {
            Some(BrowseAction::MoveDown)
        }
        (KeyCode::PageUp, _) => Some(BrowseAction::PageUp),
        (KeyCode::PageDown, _) => Some(BrowseAction::PageDown),
        (KeyCode::Home, _) => Some(BrowseAction::JumpStart),
        (KeyCode::End, _) => Some(BrowseAction::JumpEnd),

        // Selection toggle
        (KeyCode::Tab, _) => Some(BrowseAction::ToggleSelect),
        (KeyCode::BackTab, _) => Some(BrowseAction::ToggleExclude),

        // Tag tree expand/collapse
        (KeyCode::Char(' '), KeyModifiers::NONE) if state.is_tag_selection_phase() => {
            Some(BrowseAction::ExpandToggle)
        }

        // Pane focus (vim-style + arrow keys)
        (KeyCode::Char('h') | KeyCode::Left, KeyModifiers::NONE)
            if state.is_tag_selection_phase() && !state.search_active =>
        {
            Some(BrowseAction::FocusLeft)
        }
        (KeyCode::Char('l') | KeyCode::Right, KeyModifiers::NONE)
            if state.is_tag_selection_phase() && !state.search_active =>
        {
            Some(BrowseAction::FocusRight)
        }

        // Help
        (KeyCode::F(1) | KeyCode::Char('?'), _) => Some(BrowseAction::ShowHelp),

        // Watch rules
        (KeyCode::F(3), _) => Some(BrowseAction::ShowWatchRules),

        // Toggle preview mode
        (KeyCode::Char('n'), KeyModifiers::ALT) => Some(BrowseAction::ToggleNotePreview),

        // Search mode entry
        (KeyCode::Char('/'), KeyModifiers::NONE) => Some(BrowseAction::EnterSearch),

        // Search text input (only when search is active)
        (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) if state.search_active => {
            Some(BrowseAction::CharInput(c))
        }
        (KeyCode::Backspace, _) if state.search_active => Some(BrowseAction::Backspace),
        (KeyCode::Delete, _) if state.search_active => Some(BrowseAction::Delete),

        // Query cursor movement (always available — handles non-search Left/Right too)
        (KeyCode::Left, _) => Some(BrowseAction::QueryCursorLeft),
        (KeyCode::Right, _) => Some(BrowseAction::QueryCursorRight),

        // Query editing shortcuts
        (KeyCode::Char('u'), KeyModifiers::CONTROL) if state.search_active => {
            Some(BrowseAction::ClearQuery)
        }
        (KeyCode::Char('w'), KeyModifiers::CONTROL) if state.search_active => {
            Some(BrowseAction::DeleteWord)
        }

        _ => None,
    }
}

/// Handle events in normal mode
fn handle_normal_mode(
    state: &mut AppState,
    key: KeyEvent,
    custom_binds: &KeybindMap,
) -> EventResult {
    resolve_action(key, custom_binds, state)
        .map_or(EventResult::Ignored, |action| state.execute_action(action))
}

/// Handle events in help mode
const fn handle_help_mode(state: &mut AppState, _key: KeyEvent) -> EventResult {
    // Any key closes help
    state.mode = Mode::Normal;
    EventResult::Continue
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
            // Esc exits without adding
            (KeyCode::Esc, _) => {
                refine_state.exit_selection();
                EventResult::Continue
            }
            // Enter: if query text doesn't match a listed item, add it as custom entry
            // If it matches an item at cursor, toggle that item
            (KeyCode::Enter, _) => {
                if !refine_state.selection_query.is_empty() {
                    // Add the typed text as a custom entry
                    refine_state.add_custom_entry();
                } else if !refine_state.selection_items.is_empty() {
                    refine_state.toggle_current_selection();
                }
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
                EventResult::RefineSearchDone
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
const fn handle_mouse(state: &mut AppState, mouse: MouseEvent) -> EventResult {
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
            let action_str = input_state.action_id.clone();

            // Don't submit empty values
            if values.is_empty() {
                state.cancel_text_input();
                return EventResult::InputCancelled;
            }

            // Parse action string to enum
            let Ok(action) = action_str.parse::<BrowseAction>() else {
                state.cancel_text_input();
                return EventResult::Ignored; // Unknown action
            };

            // Get context (selected files) from input state
            let input_state_data = state.exit_text_input();
            let context = input_state_data.map_or_else(Vec::new, |s| s.context);

            EventResult::InputSubmitted {
                action,
                values,
                context,
            }
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
        (KeyCode::Esc | KeyCode::Char('n' | 'N'), _) => {
            state.cancel_confirm();
            EventResult::ConfirmCancelled
        }

        // Confirm action
        (KeyCode::Enter | KeyCode::Char('y' | 'Y'), _) => {
            if let Some(confirm_state) = state.exit_confirm() {
                // Parse action string to enum
                let Ok(action) = confirm_state.action_id.parse::<BrowseAction>() else {
                    state.cancel_confirm();
                    return EventResult::Ignored; // Unknown action
                };

                EventResult::ConfirmSubmitted {
                    action,
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

/// Handle events in details mode
fn handle_details_mode(state: &mut AppState, _key: KeyEvent) -> EventResult {
    // Any key closes details modal
    state.exit_details();
    EventResult::Continue
}

/// Handle events in watch rules modal mode
fn handle_watch_rules_mode(state: &mut AppState, key: KeyEvent) -> EventResult {
    match key.code {
        // Scroll support
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(ref mut wrs) = state.watch_rules_state {
                wrs.scroll_up();
            }
            EventResult::Continue
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if let Some(ref mut wrs) = state.watch_rules_state {
                // Estimate visible height from the last render pass
                let visible = state.visible_height;
                wrs.scroll_down(visible);
            }
            EventResult::Continue
        }
        // Any other key closes
        _ => {
            state.exit_watch_rules();
            EventResult::Continue
        }
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
            Mode::Details => handle_details_mode(state, key),
            Mode::WatchRules => handle_watch_rules_mode(state, key),
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
        AppState::new(items, true, None, None, "> ".to_string(), vec![], None)
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
        let result = handle_normal_mode(
            &mut state,
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            &binds,
        );
        assert_eq!(result, EventResult::Continue);
        assert_eq!(state.cursor, 1);

        // Up arrow
        let result = handle_normal_mode(
            &mut state,
            KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
            &binds,
        );
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
        // open_editor doesn't require input
        binds.insert(
            KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL),
            "open_editor".to_string(),
        );

        let result = handle_normal_mode(
            &mut state,
            KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL),
            &binds,
        );
        assert_eq!(
            result,
            EventResult::Action {
                action: BrowseAction::OpenInEditor,
                context: vec!["item0".to_string()]
            }
        );
    }

    #[test]
    fn test_query_input() {
        let mut state = make_state();
        let binds = KeybindMap::new();

        // Enter search mode first with /
        let result = handle_normal_mode(
            &mut state,
            KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE),
            &binds,
        );
        assert_eq!(result, EventResult::Continue);
        assert!(state.search_active);

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

        let result = handle_normal_mode(
            &mut state,
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            &binds,
        );
        assert_eq!(result, EventResult::Abort);
    }

    // === resolve_action / resolve_default_keybind tests ===

    #[test]
    fn test_resolve_default_navigation_keys() {
        let state = make_state();

        assert_eq!(
            resolve_default_keybind(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE), &state),
            Some(BrowseAction::MoveUp)
        );
        assert_eq!(
            resolve_default_keybind(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), &state),
            Some(BrowseAction::MoveDown)
        );
        assert_eq!(
            resolve_default_keybind(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE), &state),
            Some(BrowseAction::PageUp)
        );
        assert_eq!(
            resolve_default_keybind(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE), &state),
            Some(BrowseAction::PageDown)
        );
        assert_eq!(
            resolve_default_keybind(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE), &state),
            Some(BrowseAction::JumpStart)
        );
        assert_eq!(
            resolve_default_keybind(KeyEvent::new(KeyCode::End, KeyModifiers::NONE), &state),
            Some(BrowseAction::JumpEnd)
        );
    }

    #[test]
    fn test_resolve_vim_navigation_requires_no_search() {
        let mut state = make_state();

        // j/k work when search is inactive
        assert_eq!(
            resolve_default_keybind(
                KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
                &state
            ),
            Some(BrowseAction::MoveDown)
        );
        assert_eq!(
            resolve_default_keybind(
                KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE),
                &state
            ),
            Some(BrowseAction::MoveUp)
        );

        // j/k become char input when search is active
        state.search_active = true;
        assert_eq!(
            resolve_default_keybind(
                KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
                &state
            ),
            Some(BrowseAction::CharInput('j'))
        );
        assert_eq!(
            resolve_default_keybind(
                KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE),
                &state
            ),
            Some(BrowseAction::CharInput('k'))
        );
    }

    #[test]
    fn test_resolve_ctrl_jk_always_navigates() {
        let mut state = make_state();
        state.search_active = true;

        // Ctrl+j/k always navigate, even during search
        assert_eq!(
            resolve_default_keybind(
                KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL),
                &state
            ),
            Some(BrowseAction::MoveDown)
        );
        assert_eq!(
            resolve_default_keybind(
                KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
                &state
            ),
            Some(BrowseAction::MoveUp)
        );
    }

    #[test]
    fn test_resolve_esc_context_dependent() {
        let mut state = make_state();

        // Esc without search = abort
        assert_eq!(
            resolve_default_keybind(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), &state),
            Some(BrowseAction::Abort)
        );

        // Esc during search = exit search
        state.search_active = true;
        assert_eq!(
            resolve_default_keybind(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), &state),
            Some(BrowseAction::ExitSearch)
        );
    }

    #[test]
    fn test_resolve_enter_context_dependent() {
        let mut state = make_state();

        assert_eq!(
            resolve_default_keybind(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), &state),
            Some(BrowseAction::Confirm)
        );

        state.search_active = true;
        assert_eq!(
            resolve_default_keybind(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), &state),
            Some(BrowseAction::ExitSearch)
        );
    }

    #[test]
    fn test_resolve_search_text_input() {
        let mut state = make_state();
        state.search_active = true;

        assert_eq!(
            resolve_default_keybind(
                KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
                &state
            ),
            Some(BrowseAction::CharInput('a'))
        );
        assert_eq!(
            resolve_default_keybind(
                KeyEvent::new(KeyCode::Char('A'), KeyModifiers::SHIFT),
                &state
            ),
            Some(BrowseAction::CharInput('A'))
        );
        assert_eq!(
            resolve_default_keybind(
                KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
                &state
            ),
            Some(BrowseAction::Backspace)
        );
        assert_eq!(
            resolve_default_keybind(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE), &state),
            Some(BrowseAction::Delete)
        );
    }

    #[test]
    fn test_resolve_search_editing_shortcuts() {
        let mut state = make_state();
        state.search_active = true;

        assert_eq!(
            resolve_default_keybind(
                KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
                &state
            ),
            Some(BrowseAction::ClearQuery)
        );
        assert_eq!(
            resolve_default_keybind(
                KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL),
                &state
            ),
            Some(BrowseAction::DeleteWord)
        );
    }

    #[test]
    fn test_resolve_no_text_input_without_search() {
        let state = make_state();

        // Regular chars produce None when search is inactive (except mapped keys)
        assert_eq!(
            resolve_default_keybind(
                KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
                &state
            ),
            None
        );
        // Backspace also None without search
        assert_eq!(
            resolve_default_keybind(
                KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
                &state
            ),
            None
        );
    }

    #[test]
    fn test_resolve_selection_and_misc() {
        let state = make_state();

        assert_eq!(
            resolve_default_keybind(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE), &state),
            Some(BrowseAction::ToggleSelect)
        );
        assert_eq!(
            resolve_default_keybind(KeyEvent::new(KeyCode::BackTab, KeyModifiers::NONE), &state),
            Some(BrowseAction::ToggleExclude)
        );
        assert_eq!(
            resolve_default_keybind(
                KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE),
                &state
            ),
            Some(BrowseAction::EnterSearch)
        );
        assert_eq!(
            resolve_default_keybind(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE), &state),
            Some(BrowseAction::ShowHelp)
        );
        assert_eq!(
            resolve_default_keybind(
                KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE),
                &state
            ),
            Some(BrowseAction::ShowHelp)
        );
    }

    #[test]
    fn test_resolve_preview_scroll() {
        let state = make_state();

        assert_eq!(
            resolve_default_keybind(KeyEvent::new(KeyCode::Up, KeyModifiers::SHIFT), &state),
            Some(BrowseAction::ScrollPreviewUp)
        );
        assert_eq!(
            resolve_default_keybind(KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT), &state),
            Some(BrowseAction::ScrollPreviewDown)
        );
    }

    #[test]
    fn test_resolve_custom_bind_overrides_default() {
        let state = make_state();
        let mut binds = KeybindMap::new();
        // Override Tab (normally ToggleSelect) with add_tag
        binds.insert(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            "add_tag".to_string(),
        );

        let result = resolve_action(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            &binds,
            &state,
        );
        assert_eq!(result, Some(BrowseAction::AddTag));
    }

    #[test]
    fn test_resolve_invalid_custom_bind_falls_through() {
        let state = make_state();
        let mut binds = KeybindMap::new();
        binds.insert(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            "not_a_real_action".to_string(),
        );

        // Invalid action string falls through to default binding
        let result = resolve_action(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            &binds,
            &state,
        );
        assert_eq!(result, Some(BrowseAction::ToggleSelect));
    }

    #[test]
    fn test_resolve_unbound_key_returns_none() {
        let state = make_state();

        assert_eq!(
            resolve_default_keybind(KeyEvent::new(KeyCode::F(12), KeyModifiers::NONE), &state),
            None
        );
    }
}
