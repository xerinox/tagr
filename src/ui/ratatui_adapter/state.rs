//! Application state for the ratatui TUI
//!
//! Manages all mutable state for the fuzzy finder interface,
//! including items, selection, query, and UI mode.

use crate::ui::output::MessageLevel;
use crate::ui::ratatui_adapter::widgets::{ConfirmDialogState, RefineSearchState, TextInputState};
use crate::ui::types::DisplayItem;
use std::collections::HashSet;
use std::time::{Duration, Instant};

/// Current mode of the TUI application
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Normal browsing mode
    #[default]
    Normal,
    /// Help overlay is visible
    Help,
    /// Text input modal is active
    Input,
    /// Confirmation dialog is active
    Confirm,
    /// Refine search criteria overlay is visible
    RefineSearch,
}

/// A status message with timestamp for TTL-based expiry
#[derive(Debug, Clone)]
pub struct StatusMessage {
    /// Message level (success, error, warning, info)
    pub level: MessageLevel,
    /// Message text
    pub text: String,
    /// When the message was created
    pub created_at: Instant,
}

impl StatusMessage {
    /// Create a new status message
    #[must_use]
    pub fn new(level: MessageLevel, text: String) -> Self {
        Self {
            level,
            text,
            created_at: Instant::now(),
        }
    }

    /// Check if the message has expired based on TTL
    #[must_use]
    pub fn is_expired(&self, ttl: Duration) -> bool {
        self.created_at.elapsed() > ttl
    }
}

/// Application state for the fuzzy finder
#[derive(Debug)]
pub struct AppState {
    /// All items available for selection
    pub items: Vec<DisplayItem>,
    /// Indices of items matching current query (from nucleo)
    pub filtered_indices: Vec<u32>,
    /// Current cursor position in filtered list
    pub cursor: usize,
    /// Set of selected item indices (for multi-select)
    pub selected: HashSet<usize>,
    /// Current search query
    pub query: String,
    /// Cursor position within the query string
    pub query_cursor: usize,
    /// Current UI mode
    pub mode: Mode,
    /// Whether multi-select is enabled
    pub multi_select: bool,
    /// Status messages
    pub messages: Vec<StatusMessage>,
    /// Message TTL for auto-expiry
    pub message_ttl: Duration,
    /// Whether the finder should exit
    pub should_exit: bool,
    /// Whether the operation was aborted
    pub aborted: bool,
    /// The final key that caused exit (for action dispatch)
    pub final_key: Option<String>,
    /// Scroll offset for the item list
    pub scroll_offset: usize,
    /// Scroll offset for the preview pane
    pub preview_scroll: usize,
    /// Height of the visible item list area (set during render)
    pub visible_height: usize,
    /// State for refine search overlay
    pub refine_search_state: Option<RefineSearchState>,
    /// State for text input modal
    pub text_input_state: Option<TextInputState>,
    /// State for confirmation dialog
    pub confirm_state: Option<ConfirmDialogState>,
    /// Available tags for autocomplete (set by finder from config)
    pub available_tags: Vec<String>,
}

impl AppState {
    /// Create new application state with given items
    #[must_use]
    pub fn new(items: Vec<DisplayItem>, multi_select: bool) -> Self {
        let item_count = items.len();
        // Initially all items are visible (no filter applied)
        let filtered_indices: Vec<u32> = (0..item_count as u32).collect();

        Self {
            items,
            filtered_indices,
            cursor: 0,
            selected: HashSet::new(),
            query: String::new(),
            query_cursor: 0,
            mode: Mode::Normal,
            multi_select,
            messages: Vec::new(),
            message_ttl: Duration::from_secs(5),
            should_exit: false,
            aborted: false,
            final_key: None,
            scroll_offset: 0,
            preview_scroll: 0,
            visible_height: 20, // Default, updated during render
            refine_search_state: None,
            text_input_state: None,
            confirm_state: None,
            available_tags: Vec::new(),
        }
    }

    /// Move cursor up
    pub fn cursor_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.adjust_scroll();
        }
    }

    /// Move cursor down
    pub fn cursor_down(&mut self) {
        if self.cursor + 1 < self.filtered_indices.len() {
            self.cursor += 1;
            self.adjust_scroll();
        }
    }

    /// Move cursor up by one page
    pub fn page_up(&mut self) {
        self.cursor = self.cursor.saturating_sub(self.visible_height);
        self.adjust_scroll();
    }

    /// Move cursor down by one page
    pub fn page_down(&mut self) {
        let max_cursor = self.filtered_indices.len().saturating_sub(1);
        self.cursor = (self.cursor + self.visible_height).min(max_cursor);
        self.adjust_scroll();
    }

    /// Jump to first item
    pub fn jump_to_start(&mut self) {
        self.cursor = 0;
        self.adjust_scroll();
    }

    /// Jump to last item
    pub fn jump_to_end(&mut self) {
        self.cursor = self.filtered_indices.len().saturating_sub(1);
        self.adjust_scroll();
    }

    /// Adjust scroll offset to keep cursor visible
    fn adjust_scroll(&mut self) {
        // Ensure cursor is visible in the viewport
        if self.cursor < self.scroll_offset {
            self.scroll_offset = self.cursor;
        } else if self.cursor >= self.scroll_offset + self.visible_height {
            self.scroll_offset = self.cursor.saturating_sub(self.visible_height - 1);
        }
    }

    /// Toggle selection of current item (for multi-select)
    pub fn toggle_selection(&mut self) {
        if !self.multi_select || self.filtered_indices.is_empty() {
            return;
        }

        let item_idx = self.filtered_indices[self.cursor] as usize;
        if self.selected.contains(&item_idx) {
            self.selected.remove(&item_idx);
        } else {
            self.selected.insert(item_idx);
        }
    }

    /// Get the currently highlighted item
    #[must_use]
    pub fn current_item(&self) -> Option<&DisplayItem> {
        self.filtered_indices
            .get(self.cursor)
            .and_then(|&idx| self.items.get(idx as usize))
    }

    /// Get the key of the currently highlighted item
    #[must_use]
    pub fn current_key(&self) -> Option<&str> {
        self.current_item().map(|item| item.key.as_str())
    }

    /// Get all selected items' keys
    ///
    /// If multi-select is enabled, returns selected items.
    /// Otherwise, returns the current item.
    #[must_use]
    pub fn selected_keys(&self) -> Vec<String> {
        if self.multi_select && !self.selected.is_empty() {
            self.selected
                .iter()
                .filter_map(|&idx| self.items.get(idx).map(|item| item.key.clone()))
                .collect()
        } else {
            self.current_item()
                .map(|item| vec![item.key.clone()])
                .unwrap_or_default()
        }
    }

    /// Get all tags from selected items (or current item if no selection)
    ///
    /// Returns the union of all tags across selected items.
    #[must_use]
    pub fn get_selected_items_tags(&self) -> Vec<String> {
        let items: Vec<&DisplayItem> = if self.multi_select && !self.selected.is_empty() {
            self.selected
                .iter()
                .filter_map(|&idx| self.items.get(idx))
                .collect()
        } else {
            self.current_item().into_iter().collect()
        };

        // Collect unique tags from all selected items
        let mut tags: Vec<String> = items
            .iter()
            .flat_map(|item| item.metadata.tags.iter().cloned())
            .collect();
        tags.sort();
        tags.dedup();
        tags
    }

    /// Update the filtered indices (called after nucleo matching)
    pub fn update_filtered(&mut self, indices: Vec<u32>) {
        self.filtered_indices = indices;
        // Reset cursor if it's out of bounds
        if self.cursor >= self.filtered_indices.len() {
            self.cursor = self.filtered_indices.len().saturating_sub(1);
        }
        self.scroll_offset = 0;
        self.adjust_scroll();
    }

    /// Add a character to the query
    pub fn query_push(&mut self, c: char) {
        self.query.insert(self.query_cursor, c);
        self.query_cursor += c.len_utf8();
    }

    /// Remove a character from the query (backspace)
    pub fn query_backspace(&mut self) {
        if self.query_cursor > 0 {
            let prev_char_boundary = self.query[..self.query_cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.query.remove(prev_char_boundary);
            self.query_cursor = prev_char_boundary;
        }
    }

    /// Delete character under cursor
    pub fn query_delete(&mut self) {
        if self.query_cursor < self.query.len() {
            self.query.remove(self.query_cursor);
        }
    }

    /// Move query cursor left
    pub fn query_cursor_left(&mut self) {
        if self.query_cursor > 0 {
            self.query_cursor = self.query[..self.query_cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    /// Move query cursor right
    pub fn query_cursor_right(&mut self) {
        if self.query_cursor < self.query.len() {
            self.query_cursor = self.query[self.query_cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.query_cursor + i)
                .unwrap_or(self.query.len());
        }
    }

    /// Clear the query
    pub fn query_clear(&mut self) {
        self.query.clear();
        self.query_cursor = 0;
    }

    /// Add a status message
    pub fn add_message(&mut self, level: MessageLevel, text: String) {
        self.messages.push(StatusMessage::new(level, text));
    }

    /// Get non-expired messages
    #[must_use]
    pub fn active_messages(&self) -> Vec<&StatusMessage> {
        self.messages
            .iter()
            .filter(|m| !m.is_expired(self.message_ttl))
            .collect()
    }

    /// Clean up expired messages
    pub fn cleanup_messages(&mut self) {
        self.messages.retain(|m| !m.is_expired(self.message_ttl));
    }

    /// Mark the finder to exit with confirmation
    pub fn confirm(&mut self, final_key: Option<String>) {
        self.should_exit = true;
        self.aborted = false;
        self.final_key = final_key;
    }

    /// Mark the finder to exit as aborted
    pub fn abort(&mut self) {
        self.should_exit = true;
        self.aborted = true;
        self.final_key = Some("esc".to_string());
    }

    /// Check if an item is selected (for multi-select indicator)
    #[must_use]
    pub fn is_selected(&self, item_idx: usize) -> bool {
        self.selected.contains(&item_idx)
    }

    /// Enter refine search mode with initial state
    pub fn enter_refine_search(
        &mut self,
        include_tags: Vec<String>,
        exclude_tags: Vec<String>,
        file_patterns: Vec<String>,
        virtual_tags: Vec<String>,
        available_tags: Vec<String>,
    ) {
        self.refine_search_state = Some(RefineSearchState::new(
            include_tags,
            exclude_tags,
            file_patterns,
            virtual_tags,
            available_tags,
        ));
        self.mode = Mode::RefineSearch;
    }

    /// Exit refine search mode and return collected criteria
    #[must_use]
    pub fn exit_refine_search(&mut self) -> Option<RefineSearchState> {
        self.mode = Mode::Normal;
        self.refine_search_state.take()
    }

    /// Get mutable reference to refine search state
    #[must_use]
    pub fn refine_search_state_mut(&mut self) -> Option<&mut RefineSearchState> {
        self.refine_search_state.as_mut()
    }

    /// Get immutable reference to refine search state
    #[must_use]
    pub fn refine_search_state(&self) -> Option<&RefineSearchState> {
        self.refine_search_state.as_ref()
    }

    /// Enter text input mode
    ///
    /// # Arguments
    /// * `prompt` - The prompt/title to display
    /// * `action_id` - Identifier for the action (e.g., "add_tag", "remove_tag")
    /// * `autocomplete_items` - Items to use for fuzzy autocomplete
    /// * `excluded_tags` - Tags already on the file(s), excluded from suggestions
    /// * `multi_value` - Whether to accept multiple space-separated values
    pub fn enter_text_input(
        &mut self,
        prompt: impl Into<String>,
        action_id: impl Into<String>,
        autocomplete_items: Vec<String>,
        excluded_tags: Vec<String>,
        multi_value: bool,
    ) {
        self.text_input_state = Some(
            TextInputState::new(prompt, action_id)
                .with_autocomplete(autocomplete_items)
                .with_excluded_tags(excluded_tags)
                .with_multi_value(multi_value),
        );
        self.mode = Mode::Input;
    }

    /// Exit text input mode and return the collected values
    ///
    /// Returns `None` if not in input mode, otherwise returns the input state
    /// with all entered values.
    #[must_use]
    pub fn exit_text_input(&mut self) -> Option<TextInputState> {
        self.mode = Mode::Normal;
        self.text_input_state.take()
    }

    /// Cancel text input mode without returning values
    pub fn cancel_text_input(&mut self) {
        self.mode = Mode::Normal;
        self.text_input_state = None;
    }

    /// Get mutable reference to text input state
    #[must_use]
    pub fn text_input_state_mut(&mut self) -> Option<&mut TextInputState> {
        self.text_input_state.as_mut()
    }

    /// Get immutable reference to text input state
    #[must_use]
    pub fn text_input_state(&self) -> Option<&TextInputState> {
        self.text_input_state.as_ref()
    }

    /// Enter confirmation dialog mode
    ///
    /// # Arguments
    /// * `title` - Dialog title
    /// * `message` - Message explaining what will happen
    /// * `action_id` - Action identifier to execute on confirmation
    /// * `context` - Additional context data (e.g., affected file paths)
    pub fn enter_confirm(
        &mut self,
        title: impl Into<String>,
        message: impl Into<String>,
        action_id: impl Into<String>,
        context: Vec<String>,
    ) {
        self.confirm_state =
            Some(ConfirmDialogState::new(title, message, action_id).with_context(context));
        self.mode = Mode::Confirm;
    }

    /// Exit confirmation mode with confirmed state
    ///
    /// Returns the confirmation state if confirmed, None if cancelled.
    #[must_use]
    pub fn exit_confirm(&mut self) -> Option<ConfirmDialogState> {
        self.mode = Mode::Normal;
        self.confirm_state.take()
    }

    /// Cancel confirmation mode without executing the action
    pub fn cancel_confirm(&mut self) {
        self.mode = Mode::Normal;
        self.confirm_state = None;
    }

    /// Get immutable reference to confirm state
    #[must_use]
    pub fn confirm_state(&self) -> Option<&ConfirmDialogState> {
        self.confirm_state.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_items(count: usize) -> Vec<DisplayItem> {
        (0..count)
            .map(|i| DisplayItem::new(format!("item{i}"), format!("Item {i}"), format!("item{i}")))
            .collect()
    }

    #[test]
    fn test_cursor_navigation() {
        let mut state = AppState::new(make_items(5), false);

        assert_eq!(state.cursor, 0);

        state.cursor_down();
        assert_eq!(state.cursor, 1);

        state.cursor_down();
        state.cursor_down();
        state.cursor_down();
        assert_eq!(state.cursor, 4);

        // Should not go past end
        state.cursor_down();
        assert_eq!(state.cursor, 4);

        state.cursor_up();
        assert_eq!(state.cursor, 3);

        state.jump_to_start();
        assert_eq!(state.cursor, 0);

        state.jump_to_end();
        assert_eq!(state.cursor, 4);
    }

    #[test]
    fn test_multi_select() {
        let mut state = AppState::new(make_items(5), true);

        assert!(state.selected.is_empty());

        state.toggle_selection();
        assert!(state.is_selected(0));

        state.cursor_down();
        state.toggle_selection();
        assert!(state.is_selected(0));
        assert!(state.is_selected(1));

        // Toggle off
        state.cursor_up();
        state.toggle_selection();
        assert!(!state.is_selected(0));
        assert!(state.is_selected(1));
    }

    #[test]
    fn test_query_editing() {
        let mut state = AppState::new(vec![], false);

        state.query_push('h');
        state.query_push('e');
        state.query_push('l');
        state.query_push('l');
        state.query_push('o');
        assert_eq!(state.query, "hello");
        assert_eq!(state.query_cursor, 5);

        state.query_backspace();
        assert_eq!(state.query, "hell");
        assert_eq!(state.query_cursor, 4);

        state.query_cursor_left();
        state.query_cursor_left();
        assert_eq!(state.query_cursor, 2);

        state.query_push('y');
        assert_eq!(state.query, "heyll");

        state.query_clear();
        assert!(state.query.is_empty());
        assert_eq!(state.query_cursor, 0);
    }

    #[test]
    fn test_selected_keys() {
        let mut state = AppState::new(make_items(5), true);

        // No selection, returns current item
        let keys = state.selected_keys();
        assert_eq!(keys, vec!["item0"]);

        // With selections
        state.toggle_selection();
        state.cursor_down();
        state.cursor_down();
        state.toggle_selection();

        let mut keys = state.selected_keys();
        keys.sort();
        assert_eq!(keys, vec!["item0", "item2"]);
    }
}
