//! Search bar widget for query input

use crate::ui::ratatui_adapter::theme::Theme;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

/// Search bar widget that displays the query with cursor
pub struct SearchBar<'a> {
    /// Current query text
    query: &'a str,
    /// Cursor position in the query
    cursor: usize,
    /// Prompt text
    prompt: &'a str,
    /// Theme for styling
    theme: &'a Theme,
    /// Whether the widget has focus
    focused: bool,
}

impl<'a> SearchBar<'a> {
    /// Create a new search bar widget
    #[must_use]
    pub const fn new(query: &'a str, cursor: usize, prompt: &'a str, theme: &'a Theme) -> Self {
        Self {
            query,
            cursor,
            prompt,
            theme,
            focused: true,
        }
    }

    /// Set focus state
    #[must_use]
    pub const fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }
}

impl Widget for SearchBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let border_style = if self.focused {
            self.theme.cursor_style()
        } else {
            self.theme.border_style()
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(" Search ");

        let inner = block.inner(area);
        block.render(area, buf);

        // Build the line with prompt and query
        let mut spans = vec![
            Span::styled(self.prompt, self.theme.dimmed_style()),
            Span::raw(" "),
        ];

        if self.query.is_empty() {
            // Show cursor at start
            spans.push(Span::styled(
                "│",
                Style::default().add_modifier(Modifier::SLOW_BLINK),
            ));
        } else {
            // Split query at cursor position
            let (before, after) = self.query.split_at(self.cursor);
            spans.push(Span::raw(before));
            spans.push(Span::styled(
                "│",
                Style::default().add_modifier(Modifier::SLOW_BLINK),
            ));
            spans.push(Span::raw(after));
        }

        let line = Line::from(spans);
        let paragraph = Paragraph::new(line);
        paragraph.render(inner, buf);
    }
}
