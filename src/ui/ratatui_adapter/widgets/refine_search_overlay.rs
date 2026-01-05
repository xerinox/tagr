//! Refine search overlay widget for modifying search criteria
//!
//! Provides an interactive UI for editing search parameters including:
//! - Include tags (with fuzzy selection from available tags)
//! - Exclude tags (with fuzzy selection)
//! - File patterns
//! - Virtual tags (with selection from defined vtag patterns)

use crate::ui::ratatui_adapter::theme::Theme;
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Widget},
};

/// Fields that can be edited in the refine search overlay
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RefineField {
    #[default]
    IncludeTags,
    ExcludeTags,
    FilePatterns,
    VirtualTags,
}

impl RefineField {
    /// Get the label for this field
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::IncludeTags => "Include Tags",
            Self::ExcludeTags => "Exclude Tags",
            Self::FilePatterns => "File Patterns",
            Self::VirtualTags => "Virtual Tags",
        }
    }

    /// Get all fields in order
    #[must_use]
    pub const fn all() -> [Self; 4] {
        [
            Self::IncludeTags,
            Self::ExcludeTags,
            Self::FilePatterns,
            Self::VirtualTags,
        ]
    }

    /// Get next field (wrapping)
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::IncludeTags => Self::ExcludeTags,
            Self::ExcludeTags => Self::FilePatterns,
            Self::FilePatterns => Self::VirtualTags,
            Self::VirtualTags => Self::IncludeTags,
        }
    }

    /// Get previous field (wrapping)
    #[must_use]
    pub const fn prev(self) -> Self {
        match self {
            Self::IncludeTags => Self::VirtualTags,
            Self::ExcludeTags => Self::IncludeTags,
            Self::FilePatterns => Self::ExcludeTags,
            Self::VirtualTags => Self::FilePatterns,
        }
    }
}

/// State for the refine search overlay
#[derive(Debug, Clone, Default)]
pub struct RefineSearchState {
    /// Currently selected field
    pub selected_field: RefineField,
    /// Whether we're in sub-selection mode (picking from list)
    pub in_selection: bool,
    /// Current include tags
    pub include_tags: Vec<String>,
    /// Current exclude tags
    pub exclude_tags: Vec<String>,
    /// Current file patterns
    pub file_patterns: Vec<String>,
    /// Current virtual tags
    pub virtual_tags: Vec<String>,
    /// Available tags from database (for selection)
    pub available_tags: Vec<String>,
    /// Available virtual tag patterns
    pub available_vtags: Vec<String>,
    /// Cursor position in sub-selection list
    pub selection_cursor: usize,
    /// Items currently shown in selection (filtered subset)
    pub selection_items: Vec<String>,
    /// Search query for filtering selection items
    pub selection_query: String,
}

impl RefineSearchState {
    /// Create new state with initial values
    #[must_use]
    pub fn new(
        include_tags: Vec<String>,
        exclude_tags: Vec<String>,
        file_patterns: Vec<String>,
        virtual_tags: Vec<String>,
        available_tags: Vec<String>,
    ) -> Self {
        Self {
            selected_field: RefineField::IncludeTags,
            in_selection: false,
            include_tags,
            exclude_tags,
            file_patterns,
            virtual_tags,
            available_tags,
            available_vtags: Self::default_vtag_patterns(),
            selection_cursor: 0,
            selection_items: Vec::new(),
            selection_query: String::new(),
        }
    }

    /// Get default virtual tag patterns for selection
    fn default_vtag_patterns() -> Vec<String> {
        vec![
            // Time-based
            "modified:today".to_string(),
            "modified:yesterday".to_string(),
            "modified:this-week".to_string(),
            "modified:this-month".to_string(),
            "created:today".to_string(),
            "created:this-week".to_string(),
            // Size-based
            "size:empty".to_string(),
            "size:tiny".to_string(),
            "size:small".to_string(),
            "size:medium".to_string(),
            "size:large".to_string(),
            "size:huge".to_string(),
            "size:>1MB".to_string(),
            "size:<100KB".to_string(),
            // Extension types
            "ext-type:source".to_string(),
            "ext-type:document".to_string(),
            "ext-type:image".to_string(),
            "ext-type:archive".to_string(),
            "ext-type:config".to_string(),
            // Git status
            "git:tracked".to_string(),
            "git:untracked".to_string(),
            "git:modified".to_string(),
            "git:staged".to_string(),
            // Permissions
            "perm:executable".to_string(),
            "perm:readonly".to_string(),
        ]
    }

    /// Move to next field
    pub fn next_field(&mut self) {
        self.selected_field = self.selected_field.next();
    }

    /// Move to previous field
    pub fn prev_field(&mut self) {
        self.selected_field = self.selected_field.prev();
    }

    /// Enter selection mode for current field
    pub fn enter_selection(&mut self) {
        self.in_selection = true;
        self.selection_cursor = 0;
        self.selection_query.clear();
        self.update_selection_items();
    }

    /// Exit selection mode
    pub fn exit_selection(&mut self) {
        self.in_selection = false;
        self.selection_query.clear();
    }

    /// Update the filtered selection items based on query
    pub fn update_selection_items(&mut self) {
        let source = match self.selected_field {
            RefineField::IncludeTags | RefineField::ExcludeTags => &self.available_tags,
            RefineField::VirtualTags => &self.available_vtags,
            RefineField::FilePatterns => {
                // For file patterns, show common patterns
                self.selection_items = vec![
                    "*.rs".to_string(),
                    "*.txt".to_string(),
                    "*.md".to_string(),
                    "*.json".to_string(),
                    "*.toml".to_string(),
                    "*.yaml".to_string(),
                    "src/**/*".to_string(),
                    "tests/**/*".to_string(),
                ];
                return;
            }
        };

        if self.selection_query.is_empty() {
            self.selection_items = source.clone();
        } else {
            let query_lower = self.selection_query.to_lowercase();
            self.selection_items = source
                .iter()
                .filter(|item| item.to_lowercase().contains(&query_lower))
                .cloned()
                .collect();
        }

        // Clamp cursor
        if self.selection_cursor >= self.selection_items.len() {
            self.selection_cursor = self.selection_items.len().saturating_sub(1);
        }
    }

    /// Toggle the currently highlighted item in selection
    pub fn toggle_current_selection(&mut self) {
        if let Some(item) = self.selection_items.get(self.selection_cursor).cloned() {
            let target = match self.selected_field {
                RefineField::IncludeTags => &mut self.include_tags,
                RefineField::ExcludeTags => &mut self.exclude_tags,
                RefineField::FilePatterns => &mut self.file_patterns,
                RefineField::VirtualTags => &mut self.virtual_tags,
            };

            if let Some(pos) = target.iter().position(|x| x == &item) {
                target.remove(pos);
            } else {
                target.push(item);
            }
        }
    }

    /// Check if an item is currently selected
    #[must_use]
    pub fn is_item_selected(&self, item: &str) -> bool {
        let target = match self.selected_field {
            RefineField::IncludeTags => &self.include_tags,
            RefineField::ExcludeTags => &self.exclude_tags,
            RefineField::FilePatterns => &self.file_patterns,
            RefineField::VirtualTags => &self.virtual_tags,
        };
        target.contains(&item.to_string())
    }

    /// Move cursor up in selection
    pub fn selection_up(&mut self) {
        if self.selection_cursor > 0 {
            self.selection_cursor -= 1;
        }
    }

    /// Move cursor down in selection
    pub fn selection_down(&mut self) {
        if self.selection_cursor + 1 < self.selection_items.len() {
            self.selection_cursor += 1;
        }
    }

    /// Add character to selection query
    pub fn query_push(&mut self, c: char) {
        self.selection_query.push(c);
        self.update_selection_items();
    }

    /// Remove last character from selection query
    pub fn query_backspace(&mut self) {
        self.selection_query.pop();
        self.update_selection_items();
    }

    /// Get values for current field
    #[must_use]
    pub fn current_field_values(&self) -> &[String] {
        match self.selected_field {
            RefineField::IncludeTags => &self.include_tags,
            RefineField::ExcludeTags => &self.exclude_tags,
            RefineField::FilePatterns => &self.file_patterns,
            RefineField::VirtualTags => &self.virtual_tags,
        }
    }
}

/// Refine search overlay widget
pub struct RefineSearchOverlay<'a> {
    theme: &'a Theme,
    state: &'a RefineSearchState,
}

impl<'a> RefineSearchOverlay<'a> {
    /// Create a new refine search overlay
    #[must_use]
    pub const fn new(theme: &'a Theme, state: &'a RefineSearchState) -> Self {
        Self { theme, state }
    }

    /// Calculate centered area for the overlay
    fn centered_rect(width_percent: u16, height_percent: u16, area: Rect) -> Rect {
        let popup_layout = Layout::vertical([
            Constraint::Percentage((100 - height_percent) / 2),
            Constraint::Percentage(height_percent),
            Constraint::Percentage((100 - height_percent) / 2),
        ])
        .split(area);

        Layout::horizontal([
            Constraint::Percentage((100 - width_percent) / 2),
            Constraint::Percentage(width_percent),
            Constraint::Percentage((100 - width_percent) / 2),
        ])
        .split(popup_layout[1])[1]
    }

    /// Format values as a comma-separated string
    fn format_values(values: &[String]) -> String {
        if values.is_empty() {
            "(none)".to_string()
        } else {
            values.join(", ")
        }
    }

    /// Build the field list content
    fn build_field_list(&self) -> Vec<ListItem<'static>> {
        RefineField::all()
            .iter()
            .map(|field| {
                let is_selected = *field == self.state.selected_field;
                let values = match field {
                    RefineField::IncludeTags => &self.state.include_tags,
                    RefineField::ExcludeTags => &self.state.exclude_tags,
                    RefineField::FilePatterns => &self.state.file_patterns,
                    RefineField::VirtualTags => &self.state.virtual_tags,
                };

                let label = field.label();
                let value_str = Self::format_values(values);

                let style = if is_selected {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                let prefix = if is_selected { "▶ " } else { "  " };
                let line = Line::from(vec![
                    Span::styled(prefix, style),
                    Span::styled(format!("{label}: "), style),
                    Span::styled(
                        value_str,
                        if values.is_empty() {
                            Style::default().fg(Color::DarkGray)
                        } else {
                            Style::default().fg(Color::Green)
                        },
                    ),
                ]);

                ListItem::new(line)
            })
            .collect()
    }

    /// Build the selection list when in sub-selection mode
    fn build_selection_list(&self) -> Vec<ListItem<'static>> {
        self.state
            .selection_items
            .iter()
            .enumerate()
            .map(|(idx, item)| {
                let is_cursor = idx == self.state.selection_cursor;
                let is_selected = self.state.is_item_selected(item);

                let checkbox = if is_selected { "[✓] " } else { "[ ] " };
                let prefix = if is_cursor { "▶ " } else { "  " };

                let style = if is_cursor {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else if is_selected {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default()
                };

                let line = Line::from(vec![
                    Span::styled(prefix, style),
                    Span::styled(checkbox, style),
                    Span::styled(item.clone(), style),
                ]);

                ListItem::new(line)
            })
            .collect()
    }
}

impl Widget for RefineSearchOverlay<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let popup_area = Self::centered_rect(70, 80, area);

        // Clear the background
        Clear.render(popup_area, buf);

        let title = if self.state.in_selection {
            format!(
                " Select {} (TAB to toggle, Enter to confirm) ",
                self.state.selected_field.label()
            )
        } else {
            " Refine Search (↑↓/jk to navigate, Enter to edit, Esc to apply) ".to_string()
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.cursor_style())
            .title(title)
            .title_alignment(Alignment::Center);

        let inner = block.inner(popup_area);
        block.render(popup_area, buf);

        if self.state.in_selection {
            // Show selection list with search
            let chunks = Layout::vertical([
                Constraint::Length(3), // Search bar
                Constraint::Min(5),    // List
                Constraint::Length(1), // Help
            ])
            .split(inner);

            // Search bar
            let search_block = Block::default()
                .borders(Borders::ALL)
                .title(" Filter ");
            let search_text = Paragraph::new(self.state.selection_query.clone()).block(search_block);
            search_text.render(chunks[0], buf);

            // Selection list
            let items = self.build_selection_list();
            let list = List::new(items);
            list.render(chunks[1], buf);

            // Help text
            let help = Paragraph::new("TAB: toggle | Enter: done | Esc: cancel | Type to filter")
                .style(Style::default().fg(Color::DarkGray))
                .alignment(Alignment::Center);
            help.render(chunks[2], buf);
        } else {
            // Show field list
            let chunks = Layout::vertical([
                Constraint::Length(2), // Header
                Constraint::Min(5),    // Fields
                Constraint::Length(1), // Help
            ])
            .split(inner);

            // Header
            let header = Paragraph::new("Current search criteria:")
                .style(Style::default().add_modifier(Modifier::BOLD))
                .alignment(Alignment::Center);
            header.render(chunks[0], buf);

            // Field list
            let items = self.build_field_list();
            let list = List::new(items);
            list.render(chunks[1], buf);

            // Help text
            let help = Paragraph::new("↑↓/jk: navigate | Enter: edit field | Esc: apply & close")
                .style(Style::default().fg(Color::DarkGray))
                .alignment(Alignment::Center);
            help.render(chunks[2], buf);
        }
    }
}
