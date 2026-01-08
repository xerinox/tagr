//! Application state for the ratatui TUI
//!
//! Manages all mutable state for the fuzzy finder interface,
//! including items, selection, query, and UI mode.

use crate::ui::output::MessageLevel;
use crate::ui::ratatui_adapter::widgets::{
    ConfirmDialogState, RefineSearchState, TagTreeState, TextInputState,
};
use crate::ui::types::{BrowsePhase, DisplayItem};
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

/// Which pane has focus during TagSelection phase
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FocusPane {
    /// Tag tree pane (left)
    #[default]
    TagTree,
    /// File preview pane (right)
    FilePreview,
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
    /// Current browse phase (TagSelection or FileSelection)
    pub phase: BrowsePhase,
    /// Tag tree state (for TagSelection phase)
    pub tag_tree_state: Option<TagTreeState>,
    /// Tag schema for canonicalization (used in CLI preview)
    pub tag_schema: Option<std::sync::Arc<crate::schema::TagSchema>>,
    /// Database reference for live file count queries
    pub database: Option<std::sync::Arc<crate::db::Database>>,
    /// Which pane has focus (during TagSelection phase)
    pub focused_pane: FocusPane,
    /// File preview items (live query results)
    pub file_preview_items: Vec<DisplayItem>,
    /// Original unfiltered file preview items (before search filtering)
    pub file_preview_items_unfiltered: Vec<DisplayItem>,
    /// Cursor position in file preview pane
    pub file_preview_cursor: usize,
    /// Scroll offset for file preview pane
    pub file_preview_scroll: usize,
    /// Selected file indices in preview pane (for multi-select)
    pub file_preview_selected: HashSet<usize>,
    /// Which pane initiated the search (for context-aware filtering)
    pub search_initiated_from: Option<FocusPane>,
    /// Whether user is actively typing in search field (vs browsing filtered results)
    pub search_active: bool,
}

impl AppState {
    /// Create new application state with given items
    #[must_use]
    pub fn new(
        items: Vec<DisplayItem>,
        multi_select: bool,
        tag_schema: Option<std::sync::Arc<crate::schema::TagSchema>>,
        database: Option<std::sync::Arc<crate::db::Database>>,
    ) -> Self {
        let item_count = items.len();
        // Initially all items are visible (no filter applied)
        #[allow(clippy::cast_possible_truncation)]
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
            phase: BrowsePhase::FileSelection, // Default to file selection
            tag_tree_state: None,
            tag_schema,
            database,
            focused_pane: FocusPane::TagTree,
            file_preview_items: Vec::new(),
            file_preview_items_unfiltered: Vec::new(),
            file_preview_cursor: 0,
            file_preview_scroll: 0,
            file_preview_selected: HashSet::new(),
            search_initiated_from: None,
            search_active: false,
        }
    }

    /// Move cursor up
    pub const fn cursor_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.adjust_scroll();
        }
    }

    /// Move cursor down
    pub const fn cursor_down(&mut self) {
        if self.cursor + 1 < self.filtered_indices.len() {
            self.cursor += 1;
            self.adjust_scroll();
        }
    }

    /// Move cursor up by one page
    pub const fn page_up(&mut self) {
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
    pub const fn jump_to_start(&mut self) {
        self.cursor = 0;
        self.adjust_scroll();
    }

    /// Jump to last item
    pub const fn jump_to_end(&mut self) {
        self.cursor = self.filtered_indices.len().saturating_sub(1);
        self.adjust_scroll();
    }

    /// Adjust scroll offset to keep cursor visible
    const fn adjust_scroll(&mut self) {
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
    ///
    /// In TagSelection phase with FilePreview focus, returns selected files from right pane.
    #[must_use]
    pub fn selected_keys(&self) -> Vec<String> {
        // In tag selection phase, behavior depends on focused pane
        if self.is_tag_selection_phase() {
            use crate::ui::ratatui_adapter::state::FocusPane;
            match self.focused_pane {
                FocusPane::FilePreview => {
                    // Return selected files from preview pane
                    return self.get_selected_files_from_preview();
                }
                FocusPane::TagTree => {
                    // Return selected tags (for other operations)
                    let tree_selections = self.tag_tree_selected_tags();
                    if !tree_selections.is_empty() {
                        return tree_selections;
                    }
                    // Fall back to current item
                    return self
                        .current_item()
                        .map(|item| vec![item.key.clone()])
                        .unwrap_or_default();
                }
            }
        }

        // In file selection phase, use standard multi-select logic
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

        // Filter tag tree to match query
        if self.is_tag_selection_phase() {
            self.filter_tag_tree();
            self.sync_tag_tree_with_cursor();
        }
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
                .map_or(0, |(i, _)| i);
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
                .map_or(0, |(i, _)| i);
        }
    }

    /// Move query cursor right
    pub fn query_cursor_right(&mut self) {
        if self.query_cursor < self.query.len() {
            self.query_cursor = self.query[self.query_cursor..]
                .char_indices()
                .nth(1)
                .map_or(self.query.len(), |(i, _)| self.query_cursor + i);
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
    pub const fn exit_refine_search(&mut self) -> Option<RefineSearchState> {
        self.mode = Mode::Normal;
        self.refine_search_state.take()
    }

    /// Get mutable reference to refine search state
    #[must_use]
    pub const fn refine_search_state_mut(&mut self) -> Option<&mut RefineSearchState> {
        self.refine_search_state.as_mut()
    }

    /// Get immutable reference to refine search state
    #[must_use]
    pub const fn refine_search_state(&self) -> Option<&RefineSearchState> {
        self.refine_search_state.as_ref()
    }

    /// Enter text input mode
    ///
    /// # Arguments
    /// * `prompt` - The prompt/title to display
    /// * `action_id` - Identifier for the action (e.g., "`add_tag`", "`remove_tag`")
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
    pub const fn exit_text_input(&mut self) -> Option<TextInputState> {
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
    pub const fn text_input_state_mut(&mut self) -> Option<&mut TextInputState> {
        self.text_input_state.as_mut()
    }

    /// Get immutable reference to text input state
    #[must_use]
    pub const fn text_input_state(&self) -> Option<&TextInputState> {
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
    pub const fn exit_confirm(&mut self) -> Option<ConfirmDialogState> {
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
    pub const fn confirm_state(&self) -> Option<&ConfirmDialogState> {
        self.confirm_state.as_ref()
    }

    // ============================================================================
    // Tag Tree Navigation Methods (TagSelection phase)
    // ============================================================================

    /// Check if we're in TagSelection phase
    #[must_use]
    pub const fn is_tag_selection_phase(&self) -> bool {
        matches!(self.phase, BrowsePhase::TagSelection)
    }

    /// Move up in tag tree
    pub fn tag_tree_move_up(&mut self) {
        if let Some(ref mut tree) = self.tag_tree_state {
            tree.move_up();
            self.sync_cursor_with_tag_tree();
        }
    }

    /// Move down in tag tree
    pub fn tag_tree_move_down(&mut self) {
        if let Some(ref mut tree) = self.tag_tree_state {
            tree.move_down();
            self.sync_cursor_with_tag_tree();
        }
    }

    /// Toggle selection of current tag in tree
    ///
    /// Note: CLI preview (with file count) will be rebuilt on next render
    pub fn tag_tree_toggle_selection(&mut self) {
        if let Some(ref mut tree) = self.tag_tree_state {
            tree.toggle_tag_selection();
        }
        // Update file preview after selection changes
        self.update_file_preview();
        // CLI preview will be rebuilt automatically on next render via build_cli_preview()
    }

    /// Update file preview based on currently selected tags
    ///
    /// Queries database for files matching selected tags (with alias expansion)
    /// and updates the file_preview_items list.
    pub fn update_file_preview(&mut self) {
        let selected_tags = self.tag_tree_selected_tags();

        if selected_tags.is_empty() {
            self.file_preview_items.clear();
            self.file_preview_cursor = 0;
            self.file_preview_scroll = 0;
            self.file_preview_selected.clear();
            return;
        }

        // Get database and schema
        let db = match &self.database {
            Some(db) => db,
            None => {
                self.file_preview_items.clear();
                self.file_preview_selected.clear();
                return;
            }
        };

        // Canonicalize and expand tags (same as calculate_matching_files)
        let canonical_tags: Vec<String> = selected_tags
            .iter()
            .map(|tag| {
                if let Some(ref schema) = self.tag_schema {
                    schema.canonicalize(tag)
                } else {
                    tag.clone()
                }
            })
            .collect();

        let expanded_tags: Vec<String> = if let Some(ref schema) = self.tag_schema {
            canonical_tags
                .iter()
                .flat_map(|tag| schema.expand_synonyms(tag))
                .collect()
        } else {
            canonical_tags
        };

        // Query files (ANY mode - union)
        let mut file_set = std::collections::HashSet::new();
        for tag in &expanded_tags {
            if let Ok(files) = db.find_by_tag(tag) {
                for file in files {
                    if let Some(file_str) = file.to_str() {
                        file_set.insert(file_str.to_string());
                    }
                }
            }
        }

        // Convert to DisplayItems
        let mut files: Vec<String> = file_set.into_iter().collect();
        files.sort();

        self.file_preview_items = files
            .iter()
            .map(|path| DisplayItem::new(path.clone(), path.clone(), path.clone()))
            .collect();

        // Save unfiltered list for search filtering
        self.file_preview_items_unfiltered = self.file_preview_items.clone();

        // Clear selections when file list changes
        self.file_preview_selected.clear();

        // Reset cursor if out of bounds
        if self.file_preview_cursor >= self.file_preview_items.len() {
            self.file_preview_cursor = self.file_preview_items.len().saturating_sub(1);
        }
        self.file_preview_scroll = 0;
    }

    /// Switch focus between tag tree and file preview panes
    pub fn toggle_focus_pane(&mut self) {
        self.focused_pane = match self.focused_pane {
            FocusPane::TagTree => FocusPane::FilePreview,
            FocusPane::FilePreview => FocusPane::TagTree,
        };
    }

    /// Move cursor up in file preview pane
    pub fn file_preview_cursor_up(&mut self) {
        if self.file_preview_cursor > 0 {
            self.file_preview_cursor -= 1;
            self.adjust_file_preview_scroll();
        }
    }

    /// Move cursor down in file preview pane
    pub fn file_preview_cursor_down(&mut self) {
        if self.file_preview_cursor + 1 < self.file_preview_items.len() {
            self.file_preview_cursor += 1;
            self.adjust_file_preview_scroll();
        }
    }

    /// Adjust file preview scroll to keep cursor visible
    fn adjust_file_preview_scroll(&mut self) {
        if self.file_preview_cursor < self.file_preview_scroll {
            self.file_preview_scroll = self.file_preview_cursor;
        } else if self.file_preview_cursor >= self.file_preview_scroll + self.visible_height {
            self.file_preview_scroll = self
                .file_preview_cursor
                .saturating_sub(self.visible_height - 1);
        }
    }

    /// Toggle selection of current file in preview pane
    pub fn file_preview_toggle_selection(&mut self) {
        if self.file_preview_items.is_empty() {
            return;
        }

        let idx = self.file_preview_cursor;
        if self.file_preview_selected.contains(&idx) {
            self.file_preview_selected.remove(&idx);
        } else {
            self.file_preview_selected.insert(idx);
        }
    }

    /// Get selected files from preview pane, or current file if none selected
    #[must_use]
    pub fn get_selected_files_from_preview(&self) -> Vec<String> {
        if self.file_preview_selected.is_empty() {
            // No multi-select, return current item
            self.file_preview_items
                .get(self.file_preview_cursor)
                .map(|item| vec![item.key.clone()])
                .unwrap_or_default()
        } else {
            // Return all selected items
            self.file_preview_selected
                .iter()
                .filter_map(|&idx| {
                    self.file_preview_items
                        .get(idx)
                        .map(|item| item.key.clone())
                })
                .collect()
        }
    }

    /// Toggle expansion of current node in tree
    pub fn tag_tree_toggle_expand(&mut self) {
        if let Some(ref mut tree) = self.tag_tree_state {
            tree.toggle_selected();
        }
    }

    /// Get selected tags from tag tree
    #[must_use]
    pub fn tag_tree_selected_tags(&self) -> Vec<String> {
        self.tag_tree_state
            .as_ref()
            .map_or_else(Vec::new, |tree| tree.selected_tag_paths())
    }

    /// Build CLI preview command from current tag selection (for educational display)
    ///
    /// Shows canonical tag names to educate users on what actually gets stored.
    /// Also includes live file count based on current selection.
    #[must_use]
    pub fn build_cli_preview(&self) -> Option<String> {
        // Only show CLI preview during TagSelection phase
        if self.phase != BrowsePhase::TagSelection {
            return None;
        }

        let selected_tags = self.tag_tree_selected_tags();
        if selected_tags.is_empty() {
            return Some("tagr browse".to_string());
        }

        let mut cmd = String::from("tagr search");
        let mut canonical_tags = Vec::new();

        for tag in &selected_tags {
            // Canonicalize tag if schema is available
            let canonical = if let Some(ref schema) = self.tag_schema {
                schema.canonicalize(&tag)
            } else {
                tag.clone()
            };
            canonical_tags.push(canonical.clone());

            cmd.push_str(" -t ");
            // Quote tags with spaces or special chars
            if canonical.contains(' ') || canonical.contains('$') || canonical.contains('"') {
                cmd.push('"');
                cmd.push_str(&canonical);
                cmd.push('"');
            } else {
                cmd.push_str(&canonical);
            }
        }

        // Add mode flag if multiple tags selected (defaults to ALL mode)
        if selected_tags.len() > 1 {
            cmd.push_str(" --any-tag");
        }

        // Add live file count if database is available
        if let Some(file_count) = self.calculate_matching_files(&canonical_tags) {
            let plural = if file_count == 1 { "file" } else { "files" };
            cmd.push_str(&format!(" → {} {}", file_count, plural));
        }

        Some(cmd)
    }

    /// Calculate number of files matching the given tags
    ///
    /// Uses ANY mode (union) when multiple tags are selected.
    /// Expands tags to include all aliases (same as actual search).
    fn calculate_matching_files(&self, tags: &[String]) -> Option<usize> {
        let db = self.database.as_ref()?;

        if tags.is_empty() {
            return Some(0);
        }

        // Expand tags to include all aliases (same as actual search does)
        let expanded_tags: Vec<String> = if let Some(ref schema) = self.tag_schema {
            tags.iter()
                .flat_map(|tag| schema.expand_synonyms(tag))
                .collect()
        } else {
            tags.to_vec()
        };

        // Use ANY mode (union) - count unique files across all expanded tags
        let mut file_set = std::collections::HashSet::new();

        for tag in &expanded_tags {
            if let Ok(files) = db.find_by_tag(tag) {
                for file in files {
                    if let Some(file_str) = file.to_str() {
                        file_set.insert(file_str.to_string());
                    }
                }
            }
        }

        Some(file_set.len())
    }

    /// Synchronize items list cursor with tag tree cursor
    ///
    /// Finds the item in the items list that matches the currently selected
    /// tag in the tag tree and updates the cursor to highlight it.
    pub fn sync_cursor_with_tag_tree(&mut self) {
        if let Some(current_tag) = self
            .tag_tree_state
            .as_ref()
            .and_then(|tree| tree.current_tag())
        {
            // Find the index of this tag in the items list
            if let Some(item_idx) = self.items.iter().position(|item| item.key == current_tag) {
                // Find the position in filtered_indices
                #[allow(clippy::cast_possible_truncation)]
                let item_idx_u32 = item_idx as u32;
                if let Some(filtered_pos) = self
                    .filtered_indices
                    .iter()
                    .position(|&idx| idx == item_idx_u32)
                {
                    self.cursor = filtered_pos;
                    self.adjust_scroll();
                }
            }
        }
    }

    /// Synchronize tag tree cursor with items list cursor (reverse sync)
    ///
    /// When the items list is filtered/navigated, update the tag tree cursor
    /// to match the currently selected item in the filtered list.
    pub fn sync_tag_tree_with_cursor(&mut self) {
        if !self.filtered_indices.is_empty() && self.cursor < self.filtered_indices.len() {
            // Get the actual item at the current cursor position
            #[allow(clippy::cast_possible_truncation)]
            let item_idx = self.filtered_indices[self.cursor] as usize;
            if let Some(item) = self.items.get(item_idx) {
                // Update tag tree to select this tag
                if let Some(ref mut tree) = self.tag_tree_state {
                    tree.select_tag(&item.key);
                }
            }
        }
    }

    /// Filter tag tree based on current search query
    fn filter_tag_tree(&mut self) {
        if let Some(ref mut tree) = self.tag_tree_state {
            // Get the keys of visible/filtered items
            let visible_tags: Vec<String> = self
                .filtered_indices
                .iter()
                .filter_map(|&idx| self.items.get(idx as usize).map(|item| item.key.clone()))
                .collect();

            tree.filter_visible_tags(&visible_tags);
        }
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
        let mut state = AppState::new(make_items(5), false, None, None);

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
        let mut state = AppState::new(make_items(5), true, None, None);

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
        let mut state = AppState::new(vec![], false, None, None);

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
        let mut state = AppState::new(make_items(5), true, None, None);

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
