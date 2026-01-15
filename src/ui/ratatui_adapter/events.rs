//! Event handling for the ratatui TUI
//!
//! Handles keyboard and mouse events, mapping them to application actions.

use super::state::{AppState, Mode};
use crate::filters::TagMode;
use crate::keybinds::actions::BrowseAction;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use std::collections::HashMap;
use std::time::Duration;

/// Result of handling an event
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventResult {
    /// Continue running the event loop
    Continue,
    /// Exit with an action to execute (actions requiring special handling like edit_note)
    Action(BrowseAction),
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
#[allow(clippy::too_many_lines)]
fn handle_normal_mode(
    state: &mut AppState,
    key: KeyEvent,
    custom_binds: &KeybindMap,
) -> EventResult {
    // Check custom keybinds first
    if let Some(action_str) = custom_binds.get(&key) {
        // Parse action string to enum
        let action = match action_str.parse::<BrowseAction>() {
            Ok(a) => a,
            Err(_) => return EventResult::Ignored, // Unknown action
        };

        // Check phase availability
        if state.is_tag_selection_phase() && !action.available_in_tag_phase() {
            // Action not available in tag phase, ignore
            return EventResult::Ignored;
        }

        // Special case: actions that should be handled inline without exiting
        if action == BrowseAction::ToggleNotePreview {
            state.toggle_preview_mode();
            return EventResult::PreviewChanged;
        }

        // Special case: ShowDetails - display modal inline
        if action == BrowseAction::ShowDetails {
            // Get current file based on phase and focus
            let file_path = if state.is_tag_selection_phase() {
                // In 3-pane view, only show details if file preview pane has focus
                if state.focused_pane == crate::ui::ratatui_adapter::state::FocusPane::FilePreview {
                    state
                        .file_preview_items
                        .get(state.file_preview_cursor)
                        .map(|item| std::path::PathBuf::from(&item.key))
                } else {
                    None // Tag tree has focus, no file to show
                }
            } else {
                // In 2-pane view, get the current selected item
                state.current_key().map(std::path::PathBuf::from)
            };

            if let Some(path) = file_path {
                // Get tags and note from database
                let tags = state
                    .database
                    .as_ref()
                    .and_then(|db| db.get_tags(&path).ok())
                    .flatten()
                    .unwrap_or_default();

                let note = state
                    .database
                    .as_ref()
                    .and_then(|db| db.get_note(&path).ok())
                    .flatten();

                // Create FileDetails and enter details mode
                use crate::ui::ratatui_adapter::widgets::FileDetails;
                if let Ok(details) = FileDetails::from_path(&path, tags, note) {
                    state.enter_details(details);
                }
            }
            return EventResult::Continue;
        }

        // Special case: actions requiring special handling (terminal suspend, etc.)
        if action.requires_special_handling() {
            // Signal to caller to handle (e.g., suspend TUI for edit_note)
            return EventResult::Action(action);
        }

        // Actions that require text input open the modal
        if action.requires_input() {
            let (title, _placeholder) = action.input_prompt();

            // Get tags on selected file(s)
            let file_tags = state.get_selected_items_tags();

            // For remove_tag: show only tags on the file(s), no exclusions
            // For add_tag: show all available tags, exclude those already on file(s)
            let (autocomplete_items, excluded_tags) = match action {
                BrowseAction::RemoveTag => (file_tags, Vec::new()),
                BrowseAction::AddTag => (state.available_tags.clone(), file_tags),
                _ => (Vec::new(), Vec::new()),
            };

            // Enter text input modal (still uses string action_id for state management)
            state.enter_text_input(
                title,
                action.as_str().to_string(),
                autocomplete_items,
                excluded_tags,
                true,
            );
            return EventResult::Continue;
        }

        // Actions that require confirmation open the confirm dialog
        if action.requires_confirmation() {
            let selected_keys = state.selected_keys();
            let selected_count = selected_keys.len();
            if selected_count > 0 {
                let (title, message) = action.confirmation_prompt();
                state.enter_confirm(title, message, action.as_str().to_string(), selected_keys);
                return EventResult::Continue;
            }
        }

        return EventResult::Action(action);
    }

    // Handle standard keybinds
    match (key.code, key.modifiers) {
        // Exit (or exit search mode)
        (KeyCode::Esc, _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
            // If actively typing in search, exit search mode but keep filter
            if state.search_active {
                state.search_active = false;
                state.search_initiated_from = None;
                return EventResult::Continue;
            }
            EventResult::Abort
        }
        (KeyCode::Enter, _) => {
            // If actively typing in search, exit search mode but keep filter
            if state.search_active {
                state.search_active = false;
                state.search_initiated_from = None;
                return EventResult::Continue;
            }
            // In TagSelection phase, Enter behavior depends on focused pane
            if state.is_tag_selection_phase() {
                use crate::ui::ratatui_adapter::state::FocusPane;
                match state.focused_pane {
                    FocusPane::TagTree => {
                        // Move focus to file list
                        state.focused_pane = FocusPane::FilePreview;
                        return EventResult::Continue;
                    }
                    FocusPane::FilePreview => {
                        // Confirm selection - use multi-select if any, otherwise current file
                        return EventResult::Confirm;
                    }
                }
            }
            EventResult::Confirm
        }

        // Preview scroll (Shift+Up/Down) - must be before general navigation
        (KeyCode::Up, KeyModifiers::SHIFT) => {
            state.preview_scroll = state.preview_scroll.saturating_sub(1);
            EventResult::Continue
        }
        (KeyCode::Down, KeyModifiers::SHIFT) => {
            state.preview_scroll += 1;
            EventResult::Continue
        }

        // Navigation - route based on focused pane in TagSelection phase
        (KeyCode::Up, KeyModifiers::NONE | KeyModifiers::CONTROL)
        | (KeyCode::Char('k'), KeyModifiers::CONTROL) => {
            if state.is_tag_selection_phase() {
                use crate::ui::ratatui_adapter::state::FocusPane;
                match state.focused_pane {
                    FocusPane::TagTree => state.tag_tree_move_up(),
                    FocusPane::FilePreview => state.file_preview_cursor_up(),
                }
            } else {
                state.cursor_up();
            }
            EventResult::Continue
        }
        (KeyCode::Char('k'), KeyModifiers::NONE) if !state.search_active => {
            if state.is_tag_selection_phase() {
                use crate::ui::ratatui_adapter::state::FocusPane;
                match state.focused_pane {
                    FocusPane::TagTree => state.tag_tree_move_up(),
                    FocusPane::FilePreview => state.file_preview_cursor_up(),
                }
            } else {
                state.cursor_up();
            }
            EventResult::Continue
        }
        (KeyCode::Down, KeyModifiers::NONE | KeyModifiers::CONTROL)
        | (KeyCode::Char('j'), KeyModifiers::CONTROL) => {
            if state.is_tag_selection_phase() {
                use crate::ui::ratatui_adapter::state::FocusPane;
                match state.focused_pane {
                    FocusPane::TagTree => state.tag_tree_move_down(),
                    FocusPane::FilePreview => state.file_preview_cursor_down(),
                }
            } else {
                state.cursor_down();
            }
            EventResult::Continue
        }
        (KeyCode::Char('j'), KeyModifiers::NONE) if !state.search_active => {
            if state.is_tag_selection_phase() {
                use crate::ui::ratatui_adapter::state::FocusPane;
                match state.focused_pane {
                    FocusPane::TagTree => state.tag_tree_move_down(),
                    FocusPane::FilePreview => state.file_preview_cursor_down(),
                }
            } else {
                state.cursor_down();
            }
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

        // Multi-select / Tag tree toggle - route based on focused pane
        (KeyCode::Tab, _) => {
            if state.is_tag_selection_phase() {
                use crate::ui::ratatui_adapter::state::FocusPane;
                match state.focused_pane {
                    FocusPane::TagTree => {
                        // Toggle tag inclusion: parent nodes affect all children (Option A)
                        if let Some(tree) = state.tag_tree_state.as_ref()
                            && let Some(current_tag) = tree.current_tag()
                        {
                            let children = tree.get_all_descendant_tags(&current_tag);

                            if children.is_empty() {
                                // Leaf node - toggle just this tag
                                state.active_filter.toggle_include_tag(current_tag);
                            } else {
                                // Parent node - toggle all children + parent if it's actual tag
                                if tree.current_is_actual_tag() {
                                    state.active_filter.toggle_include_tag(current_tag);
                                }
                                for child in children {
                                    state.active_filter.toggle_include_tag(child);
                                }
                            }

                            // Update tag mode based on number of selected tags
                            // Multiple tags -> Any (OR), single tag -> All (AND)
                            state.active_filter.criteria.tag_mode =
                                if state.active_filter.criteria.tags.len() > 1 {
                                    TagMode::Any
                                } else {
                                    TagMode::All
                                };

                            // Sync tag tree visual state from active_filter
                            state.sync_tag_tree_from_filter();
                            // Update file preview with new filter
                            state.update_file_preview();
                        }
                        state.tag_tree_move_down();
                    }
                    FocusPane::FilePreview => {
                        state.file_preview_toggle_selection();
                        state.file_preview_cursor_down();
                    }
                }
            } else {
                state.toggle_selection();
                state.cursor_down();
            }
            EventResult::Continue
        }
        (KeyCode::BackTab, _) => {
            if state.is_tag_selection_phase() {
                use crate::ui::ratatui_adapter::state::FocusPane;
                match state.focused_pane {
                    FocusPane::TagTree => {
                        // Toggle tag exclusion: parent nodes affect all children (Option A)
                        if let Some(tree) = state.tag_tree_state.as_ref()
                            && let Some(current_tag) = tree.current_tag()
                        {
                            let children = tree.get_all_descendant_tags(&current_tag);

                            if children.is_empty() {
                                // Leaf node - toggle just this tag
                                state.active_filter.toggle_exclude_tag(current_tag);
                            } else {
                                // Parent node - toggle all children + parent if it's actual tag
                                if tree.current_is_actual_tag() {
                                    state.active_filter.toggle_exclude_tag(current_tag);
                                }
                                for child in children {
                                    state.active_filter.toggle_exclude_tag(child);
                                }
                            }

                            // Update tag mode based on number of selected tags
                            // Multiple tags -> Any (OR), single tag -> All (AND)
                            state.active_filter.criteria.tag_mode =
                                if state.active_filter.criteria.tags.len() > 1 {
                                    TagMode::Any
                                } else {
                                    TagMode::All
                                };

                            // Sync exclusion state
                            state.sync_tag_tree_exclusions();
                            // Update file preview with new filter
                            state.update_file_preview();
                        }
                        state.tag_tree_move_down();
                    }
                    FocusPane::FilePreview => {
                        state.file_preview_toggle_selection();
                        state.file_preview_cursor_down();
                    }
                }
            } else {
                state.toggle_selection();
                state.cursor_down();
            }
            EventResult::Continue
        }

        // Tag tree expansion toggle (Space key in TagSelection phase)
        (KeyCode::Char(' '), KeyModifiers::NONE) if state.is_tag_selection_phase() => {
            state.tag_tree_toggle_expand();
            EventResult::Continue
        }

        // Pane navigation: h/Left moves to previous pane, l/Right moves to next pane
        (KeyCode::Char('h'), KeyModifiers::NONE)
            if state.is_tag_selection_phase() && !state.search_active =>
        {
            use crate::ui::ratatui_adapter::state::FocusPane;
            if state.focused_pane == FocusPane::FilePreview {
                state.focused_pane = FocusPane::TagTree;
            }
            EventResult::Continue
        }
        (KeyCode::Left, KeyModifiers::NONE)
            if state.is_tag_selection_phase() && !state.search_active =>
        {
            use crate::ui::ratatui_adapter::state::FocusPane;
            if state.focused_pane == FocusPane::FilePreview {
                state.focused_pane = FocusPane::TagTree;
            }
            EventResult::Continue
        }
        (KeyCode::Char('l'), KeyModifiers::NONE)
            if state.is_tag_selection_phase() && !state.search_active =>
        {
            use crate::ui::ratatui_adapter::state::FocusPane;
            if state.focused_pane == FocusPane::TagTree {
                state.focused_pane = FocusPane::FilePreview;
            }
            EventResult::Continue
        }
        (KeyCode::Right, KeyModifiers::NONE)
            if state.is_tag_selection_phase() && !state.search_active =>
        {
            use crate::ui::ratatui_adapter::state::FocusPane;
            if state.focused_pane == FocusPane::TagTree {
                state.focused_pane = FocusPane::FilePreview;
            }
            EventResult::Continue
        }

        // Help overlay
        (KeyCode::F(1) | KeyCode::Char('?'), _) => {
            state.mode = Mode::Help;
            EventResult::Continue
        }

        // Toggle preview mode (Alt+N) - switch between file content and note
        (KeyCode::Char('n'), KeyModifiers::ALT) => {
            state.toggle_preview_mode();
            EventResult::PreviewChanged
        }

        // Query editing - / activates search mode
        (KeyCode::Char('/'), KeyModifiers::NONE) => {
            state.search_active = true;
            EventResult::Continue
        }
        // Regular character input only when search is active
        (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) if state.search_active => {
            state.query_push(c);
            EventResult::QueryChanged
        }
        (KeyCode::Backspace, _) if state.search_active => {
            if state.query.is_empty() {
                EventResult::Ignored
            } else {
                state.query_backspace();
                EventResult::QueryChanged
            }
        }
        (KeyCode::Delete, _) if state.search_active => {
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
        (KeyCode::Char('u'), KeyModifiers::CONTROL) if state.search_active => {
            state.query_clear();
            EventResult::QueryChanged
        }
        (KeyCode::Char('w'), KeyModifiers::CONTROL) if state.search_active => {
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
            let action = match action_str.parse::<BrowseAction>() {
                Ok(a) => a,
                Err(_) => {
                    state.cancel_text_input();
                    return EventResult::Ignored; // Unknown action
                }
            };

            let _ = state.exit_text_input();
            EventResult::InputSubmitted { action, values }
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
                let action = match confirm_state.action_id.parse::<BrowseAction>() {
                    Ok(a) => a,
                    Err(_) => {
                        state.cancel_confirm();
                        return EventResult::Ignored; // Unknown action
                    }
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
        assert_eq!(result, EventResult::Action(BrowseAction::OpenInEditor));
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
}
