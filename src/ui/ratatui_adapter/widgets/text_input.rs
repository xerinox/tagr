//! Modal text input widget with fuzzy autocomplete support
//!
//! Provides a text input overlay with:
//! - Single-line text editing with cursor
//! - Fuzzy autocomplete suggestions from a provided list
//! - TAB to accept autocomplete, Enter to submit, ESC to cancel

use crate::ui::ratatui_adapter::theme::Theme;
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Widget},
};

/// State for the text input modal
#[derive(Debug, Clone)]
pub struct TextInputState {
    /// The prompt/title to display
    pub prompt: String,
    /// Current input buffer
    pub buffer: String,
    /// Cursor position (character index, not byte)
    pub cursor: usize,
    /// Available items for autocomplete
    pub autocomplete_items: Vec<String>,
    /// Tags already on the file(s) - excluded from suggestions
    pub excluded_tags: Vec<String>,
    /// Filtered autocomplete suggestions
    pub suggestions: Vec<String>,
    /// Currently highlighted suggestion index
    pub suggestion_cursor: usize,
    /// Whether to show autocomplete suggestions
    pub show_suggestions: bool,
    /// Multi-value mode: space-separated values, each can autocomplete
    pub multi_value: bool,
    /// Already entered values (for multi-value mode)
    pub entered_values: Vec<String>,
    /// Callback identifier for completion
    pub action_id: String,
}

impl TextInputState {
    /// Create a new text input state
    #[must_use]
    pub fn new(prompt: impl Into<String>, action_id: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            buffer: String::new(),
            cursor: 0,
            autocomplete_items: Vec::new(),
            excluded_tags: Vec::new(),
            suggestions: Vec::new(),
            suggestion_cursor: 0,
            show_suggestions: false,
            multi_value: false,
            entered_values: Vec::new(),
            action_id: action_id.into(),
        }
    }

    /// Enable multi-value mode (space-separated values)
    #[must_use]
    pub fn with_multi_value(mut self, multi: bool) -> Self {
        self.multi_value = multi;
        self
    }

    /// Set autocomplete items
    #[must_use]
    pub fn with_autocomplete(mut self, items: Vec<String>) -> Self {
        self.autocomplete_items = items;
        self.update_suggestions();
        self
    }

    /// Set tags to exclude from suggestions (already on the file)
    #[must_use]
    pub fn with_excluded_tags(mut self, tags: Vec<String>) -> Self {
        self.excluded_tags = tags;
        self.update_suggestions();
        self
    }

    /// Get the current word being typed (for multi-value mode)
    fn current_word(&self) -> &str {
        if self.multi_value {
            // Find the start of the current word
            let before_cursor = &self.buffer[..self.byte_index()];
            before_cursor
                .rsplit_once(|c: char| c.is_whitespace())
                .map_or(before_cursor, |(_, word)| word)
        } else {
            &self.buffer
        }
    }

    /// Get byte index from cursor (character) position
    fn byte_index(&self) -> usize {
        self.buffer
            .char_indices()
            .nth(self.cursor)
            .map_or(self.buffer.len(), |(i, _)| i)
    }

    /// Get byte index of current word start
    fn current_word_byte_start(&self) -> usize {
        if self.multi_value {
            let before_cursor = &self.buffer[..self.byte_index()];
            before_cursor
                .rfind(|c: char| c.is_whitespace())
                .map_or(0, |i| i + 1)
        } else {
            0
        }
    }

    /// Update autocomplete suggestions based on current input
    pub fn update_suggestions(&mut self) {
        let query = self.current_word().to_lowercase();

        // Filter items that match the query (or show all if query is empty)
        self.suggestions = self
            .autocomplete_items
            .iter()
            .filter(|item| {
                // Skip tags already on the file(s)
                if self.excluded_tags.contains(item) {
                    return false;
                }
                // Skip already-entered values in multi-value mode
                if self.multi_value && self.entered_values.contains(item) {
                    return false;
                }
                // If query is empty, show all available items
                if query.is_empty() {
                    return true;
                }
                // Fuzzy match: contains query chars in order
                let item_lower = item.to_lowercase();
                fuzzy_match(&item_lower, &query)
            })
            .take(10) // Limit suggestions
            .cloned()
            .collect();

        // Reset cursor if out of bounds
        if self.suggestion_cursor >= self.suggestions.len() {
            self.suggestion_cursor = 0;
        }

        self.show_suggestions = !self.suggestions.is_empty();
    }

    /// Insert a character at cursor position
    pub fn insert_char(&mut self, c: char) {
        let byte_idx = self.byte_index();
        self.buffer.insert(byte_idx, c);
        self.cursor += 1;
        self.update_suggestions();
    }

    /// Delete character before cursor (backspace)
    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            let byte_idx = self.byte_index();
            let prev_byte_idx = self.buffer[..byte_idx]
                .char_indices()
                .next_back()
                .map_or(0, |(i, _)| i);
            self.buffer.remove(prev_byte_idx);
            self.cursor -= 1;
            self.update_suggestions();
        }
    }

    /// Delete character at cursor (delete key)
    pub fn delete(&mut self) {
        let byte_idx = self.byte_index();
        if byte_idx < self.buffer.len() {
            self.buffer.remove(byte_idx);
            self.update_suggestions();
        }
    }

    /// Move cursor left
    pub fn cursor_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    /// Move cursor right
    pub fn cursor_right(&mut self) {
        let char_count = self.buffer.chars().count();
        if self.cursor < char_count {
            self.cursor += 1;
        }
    }

    /// Move cursor to start
    pub fn cursor_home(&mut self) {
        self.cursor = 0;
    }

    /// Move cursor to end
    pub fn cursor_end(&mut self) {
        self.cursor = self.buffer.chars().count();
    }

    /// Move suggestion cursor up
    pub fn suggestion_up(&mut self) {
        if self.suggestion_cursor > 0 {
            self.suggestion_cursor -= 1;
        } else if !self.suggestions.is_empty() {
            self.suggestion_cursor = self.suggestions.len() - 1;
        }
    }

    /// Move suggestion cursor down
    pub fn suggestion_down(&mut self) {
        if self.suggestion_cursor + 1 < self.suggestions.len() {
            self.suggestion_cursor += 1;
        } else {
            self.suggestion_cursor = 0;
        }
    }

    /// Accept the current autocomplete suggestion (TAB)
    pub fn accept_suggestion(&mut self) {
        if let Some(suggestion) = self.suggestions.get(self.suggestion_cursor).cloned() {
            // Replace current word with suggestion
            let word_start = self.current_word_byte_start();
            let byte_idx = self.byte_index();

            // Remove current word
            self.buffer.drain(word_start..byte_idx);

            // Insert suggestion
            self.buffer.insert_str(word_start, &suggestion);

            // Update cursor
            self.cursor = self.buffer[..word_start].chars().count() + suggestion.chars().count();

            // In multi-value mode, add space after accepted suggestion
            if self.multi_value {
                self.buffer.insert(self.byte_index(), ' ');
                self.cursor += 1;
                self.entered_values.push(suggestion);
            }

            self.update_suggestions();
        }
    }

    /// Get the final values (splits by whitespace in multi-value mode)
    #[must_use]
    pub fn values(&self) -> Vec<String> {
        if self.multi_value {
            self.buffer
                .split_whitespace()
                .map(String::from)
                .collect()
        } else if self.buffer.trim().is_empty() {
            Vec::new()
        } else {
                vec![self.buffer.clone()]
            }
    }

    /// Clear the input
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.cursor = 0;
        self.entered_values.clear();
        self.update_suggestions();
    }

    /// Clear word backwards (Ctrl+W)
    pub fn delete_word_backwards(&mut self) {
        let byte_idx = self.byte_index();
        let before = &self.buffer[..byte_idx];

        // Find the start of the word to delete
        let trimmed = before.trim_end();
        let new_end = trimmed.rfind(|c: char| c.is_whitespace()).map_or(0, |last_space| last_space +1);

        self.buffer.drain(new_end..byte_idx);
        self.cursor = self.buffer[..new_end].chars().count();
        self.update_suggestions();
    }

    /// Clear the entire line (Ctrl+U)
    pub fn clear_line(&mut self) {
        self.clear();
    }
}

/// Simple fuzzy matching: checks if pattern chars appear in text in order
fn fuzzy_match(text: &str, pattern: &str) -> bool {
    if pattern.is_empty() {
        return true;
    }

    let mut pattern_chars = pattern.chars().peekable();
    for c in text.chars() {
        if pattern_chars.peek() == Some(&c) {
            pattern_chars.next();
            if pattern_chars.peek().is_none() {
                return true;
            }
        }
    }

    pattern_chars.peek().is_none()
}

/// Text input modal overlay widget
pub struct TextInputModal<'a> {
    state: &'a TextInputState,
    theme: &'a Theme,
}

impl<'a> TextInputModal<'a> {
    /// Create a new text input modal
    #[must_use]
    pub const fn new(state: &'a TextInputState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }

    /// Calculate centered area for the modal
    fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        Rect::new(x, y, width.min(area.width), height.min(area.height))
    }

    /// Build the suggestion list
    fn build_suggestions(&self) -> Vec<ListItem<'static>> {
        self.state
            .suggestions
            .iter()
            .enumerate()
            .map(|(idx, suggestion)| {
                let is_selected = idx == self.state.suggestion_cursor;
                let style = if is_selected {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                let prefix = if is_selected { "▶ " } else { "  " };
                ListItem::new(Line::from(vec![
                    Span::styled(prefix, style),
                    Span::styled(suggestion.clone(), style),
                ]))
            })
            .collect()
    }
}

impl Widget for TextInputModal<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Calculate modal size
        let width = 60.min(area.width.saturating_sub(4));

        // Calculate height for entered tags row (if any)
        let has_entered_tags = self.state.multi_value && !self.state.entered_values.is_empty();
        let entered_tags_height: u16 = if has_entered_tags { 2 } else { 0 };

        let suggestions_height = if self.state.show_suggestions {
            (self.state.suggestions.len() as u16).min(8) + 2 // +2 for borders
        } else {
            0
        };
        // Base height: 2 (modal borders) + 3 (input field) + 1 (help text) = 6
        let height = 6 + entered_tags_height + suggestions_height;

        let modal_area = Self::centered_rect(width, height, area);

        // Clear background
        Clear.render(modal_area, buf);

        // Main modal block
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.cursor_style())
            .title(format!(" {} ", self.state.prompt))
            .title_alignment(Alignment::Center);

        let inner = block.inner(modal_area);
        block.render(modal_area, buf);

        // Layout: entered tags (if any) + input field + suggestions (if any) + help
        let mut constraints = Vec::new();

        if has_entered_tags {
            constraints.push(Constraint::Length(entered_tags_height));
        }
        constraints.push(Constraint::Length(3)); // Input field

        if self.state.show_suggestions {
            constraints.push(Constraint::Length(suggestions_height));
        }
        constraints.push(Constraint::Length(1)); // Help text

        let chunks = Layout::vertical(constraints).split(inner);
        let mut chunk_idx = 0;

        // Render entered tags as pills (if any)
        if has_entered_tags {
            let tags_line = Line::from(
                self.state
                    .entered_values
                    .iter()
                    .flat_map(|tag| {
                        vec![
                            Span::styled(
                                format!(" {} ", tag),
                                Style::default()
                                    .bg(Color::Cyan)
                                    .fg(Color::Black)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::raw(" "),
                        ]
                    })
                    .collect::<Vec<_>>(),
            );
            let tags_para = Paragraph::new(tags_line);
            tags_para.render(chunks[chunk_idx], buf);
            chunk_idx += 1;
        }

        // Render input field
        let input_block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.border_style());

        let input_inner = input_block.inner(chunks[chunk_idx]);
        input_block.render(chunks[chunk_idx], buf);
        chunk_idx += 1;

        // Render the buffer with cursor
        let display_width = input_inner.width as usize;

        // Calculate visible window of text
        let (visible_text, cursor_offset) = {
            let buffer = &self.state.buffer;
            let cursor_char = self.state.cursor;
            let total_chars = buffer.chars().count();

            if total_chars <= display_width {
                // Everything fits
                (buffer.clone(), cursor_char)
            } else {
                // Need to scroll
                let half_width = display_width / 2;
                let start_char = if cursor_char <= half_width {
                    0
                } else if cursor_char + half_width >= total_chars {
                    total_chars.saturating_sub(display_width)
                } else {
                    cursor_char.saturating_sub(half_width)
                };

                let visible: String = buffer
                    .chars()
                    .skip(start_char)
                    .take(display_width)
                    .collect();
                (visible, cursor_char - start_char)
            }
        };

        // Render text with cursor highlight
        let before_cursor: String = visible_text.chars().take(cursor_offset).collect();
        let cursor_char: String = visible_text.chars().skip(cursor_offset).take(1).collect();
        let after_cursor: String = visible_text.chars().skip(cursor_offset + 1).collect();

        let cursor_display = if cursor_char.is_empty() { " " } else { &cursor_char };

        let line = Line::from(vec![
            Span::raw(before_cursor),
            Span::styled(
                cursor_display.to_string(),
                Style::default()
                    .bg(self.theme.cursor)
                    .fg(Color::Black)
                    .add_modifier(Modifier::SLOW_BLINK),
            ),
            Span::raw(after_cursor),
        ]);

        let input_paragraph = Paragraph::new(line);
        input_paragraph.render(input_inner, buf);

        // Render suggestions if visible (with border and title)
        if self.state.show_suggestions {
            let suggestions = self.build_suggestions();
            let suggestion_count = self.state.autocomplete_items.len();
            let shown_count = self.state.suggestions.len();
            let title = if shown_count < suggestion_count {
                format!(" Tags ({}/{}) ", shown_count, suggestion_count)
            } else {
                format!(" Tags ({}) ", shown_count)
            };

            let list = List::new(suggestions).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(self.theme.border_style())
                    .title(title)
                    .title_alignment(Alignment::Left),
            );
            list.render(chunks[chunk_idx], buf);
            chunk_idx += 1;
        }

        // Render help text
        let help_text = if self.state.show_suggestions {
            "TAB: accept | ↑↓: navigate | Enter: submit | ESC: cancel"
        } else {
            "Enter: submit | ESC: cancel"
        };

        let help = Paragraph::new(help_text)
            .style(self.theme.dimmed_style())
            .alignment(Alignment::Center);
        help.render(chunks[chunk_idx], buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_input() {
        let mut state = TextInputState::new("Test", "test_action");

        state.insert_char('h');
        state.insert_char('e');
        state.insert_char('l');
        state.insert_char('l');
        state.insert_char('o');

        assert_eq!(state.buffer, "hello");
        assert_eq!(state.cursor, 5);
    }

    #[test]
    fn test_cursor_movement() {
        let mut state = TextInputState::new("Test", "test_action");
        state.buffer = "hello".to_string();
        state.cursor = 5;

        state.cursor_left();
        assert_eq!(state.cursor, 4);

        state.cursor_home();
        assert_eq!(state.cursor, 0);

        state.cursor_end();
        assert_eq!(state.cursor, 5);

        state.cursor_right();
        assert_eq!(state.cursor, 5); // Should not go past end
    }

    #[test]
    fn test_backspace() {
        let mut state = TextInputState::new("Test", "test_action");
        state.buffer = "hello".to_string();
        state.cursor = 5;

        state.backspace();
        assert_eq!(state.buffer, "hell");
        assert_eq!(state.cursor, 4);

        state.cursor = 0;
        state.backspace();
        assert_eq!(state.buffer, "hell"); // Nothing happens at position 0
    }

    #[test]
    fn test_autocomplete() {
        let state = TextInputState::new("Test", "test_action")
            .with_autocomplete(vec![
                "rust".to_string(),
                "ruby".to_string(),
                "python".to_string(),
            ]);

        assert!(state.autocomplete_items.len() == 3);
    }

    #[test]
    fn test_suggestions_filter() {
        let mut state = TextInputState::new("Test", "test_action")
            .with_autocomplete(vec![
                "rust".to_string(),
                "ruby".to_string(),
                "python".to_string(),
            ]);

        state.insert_char('r');
        state.insert_char('u');

        assert!(state.suggestions.contains(&"rust".to_string()));
        assert!(state.suggestions.contains(&"ruby".to_string()));
        assert!(!state.suggestions.contains(&"python".to_string()));
    }

    #[test]
    fn test_accept_suggestion() {
        let mut state = TextInputState::new("Test", "test_action")
            .with_autocomplete(vec!["rust".to_string(), "ruby".to_string()]);

        state.insert_char('r');
        state.insert_char('u');

        // First suggestion should be "ruby" or "rust"
        assert!(!state.suggestions.is_empty());

        state.accept_suggestion();
        assert!(state.buffer == "rust" || state.buffer == "ruby");
    }

    #[test]
    fn test_multi_value_mode() {
        let mut state = TextInputState::new("Tags", "add_tag")
            .with_multi_value(true)
            .with_autocomplete(vec!["rust".to_string(), "python".to_string()]);

        state.buffer = "rust python".to_string();
        state.cursor = 11;

        let values = state.values();
        assert_eq!(values, vec!["rust", "python"]);
    }

    #[test]
    fn test_fuzzy_match() {
        assert!(fuzzy_match("rust", "rs"));
        assert!(fuzzy_match("rust", "rust"));
        assert!(fuzzy_match("rust", "rt"));
        assert!(!fuzzy_match("rust", "py"));
        assert!(fuzzy_match("hello", ""));
    }

    #[test]
    fn test_delete_word_backwards() {
        let mut state = TextInputState::new("Test", "test_action");
        state.buffer = "hello world".to_string();
        state.cursor = 11;

        state.delete_word_backwards();
        assert_eq!(state.buffer, "hello ");
        assert_eq!(state.cursor, 6);

        state.delete_word_backwards();
        assert_eq!(state.buffer, "");
        assert_eq!(state.cursor, 0);
    }
}
