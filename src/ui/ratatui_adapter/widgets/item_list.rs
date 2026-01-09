//! Item list widget for displaying filtered items

use crate::ui::ratatui_adapter::state::AppState;
use crate::ui::ratatui_adapter::theme::Theme;
use crate::ui::types::DisplayItem;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Color,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Widget},
};

/// Item list widget that displays filtered items with selection indicators
pub struct ItemList<'a> {
    /// Application state
    state: &'a AppState,
    /// Theme for styling
    theme: &'a Theme,
    /// Title for the list block
    title: String,
    /// Optional match positions for highlighting (`item_idx` -> vec of char positions)
    match_positions: Option<&'a [(u32, Vec<u32>)]>,
}

impl<'a> ItemList<'a> {
    /// Create a new item list widget
    #[must_use]
    pub fn new(state: &'a AppState, theme: &'a Theme) -> Self {
        let filtered = state.filtered_indices.len();
        let total = state.items.len();
        let title = format!(" Items ({filtered}/{total}) ");

        Self {
            state,
            theme,
            title,
            match_positions: None,
        }
    }

    /// Set custom title
    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Set match positions for highlighting
    #[must_use]
    pub const fn with_matches(mut self, positions: &'a [(u32, Vec<u32>)]) -> Self {
        self.match_positions = Some(positions);
        self
    }

    /// Render a single item
    fn render_item(&self, item: &DisplayItem, item_idx: usize, is_cursor: bool) -> ListItem<'a> {
        let is_selected = if self.state.is_tag_selection_phase() {
            // In TagSelection phase, check tag tree selection OR file preview selection
            // (depending on which items we're rendering)
            if self.state.tag_tree_state.is_some() {
                // Check if this is a tag (in tag tree) or a file (in file preview)
                // Tags are in items list, files are in file_preview_items
                let is_tag = self.state.items.iter().any(|i| i.key == item.key);
                if is_tag {
                    self.state
                        .tag_tree_state
                        .as_ref()
                        .map_or(false, |tree| tree.selected_tags.contains(&item.key))
                } else {
                    // This is a file - check file preview selection by key
                    self.state.is_file_preview_selected_key(&item.key)
                }
            } else {
                self.state.is_selected(item_idx)
            }
        } else {
            // In FileSelection phase, use regular multi-select
            self.state.is_selected(item_idx)
        };
        let exists = item.metadata.exists;

        // Build prefix: cursor indicator + selection indicator
        let cursor_char = if is_cursor { ">" } else { " " };

        let mut spans = vec![
            Span::styled(cursor_char, self.theme.cursor_style()),
            Span::raw(" "),
        ];

        // Green checkmark for selected items
        if is_selected {
            spans.push(Span::styled(
                "✓",
                ratatui::style::Style::default().fg(Color::Green),
            ));
            spans.push(Span::raw(" "));
        } else {
            spans.push(Span::raw("  "));
        }

        spans.push(Span::raw(" "));

        // Add the display text with appropriate styling
        // Use `searchable` (plain text) instead of `display` (ANSI-formatted)
        // since ratatui uses native styles, not ANSI escape codes
        let text_style = if !exists {
            self.theme.missing_file_style()
        } else if is_cursor {
            self.theme.selected_style()
        } else {
            self.theme.normal_style()
        };

        // Use searchable (plain text) and apply ratatui styling
        spans.push(Span::styled(item.searchable.clone(), text_style));

        let line = Line::from(spans);

        if is_cursor {
            ListItem::new(line).style(self.theme.selected_style())
        } else {
            ListItem::new(line)
        }
    }
}

impl Widget for ItemList<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.border_style())
            .title(self.title.as_str());

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height == 0 {
            return;
        }

        // Calculate visible range
        let visible_height = inner.height as usize;
        let start = self.state.scroll_offset;
        let end = (start + visible_height).min(self.state.filtered_indices.len());

        // Build list items for visible range
        let items: Vec<ListItem> = (start..end)
            .filter_map(|visible_idx| {
                let item_idx = *self.state.filtered_indices.get(visible_idx)? as usize;
                let item = self.state.items.get(item_idx)?;
                let is_cursor = visible_idx == self.state.cursor;
                Some(self.render_item(item, item_idx, is_cursor))
            })
            .collect();

        let list = List::new(items);
        list.render(inner, buf);
    }
}
