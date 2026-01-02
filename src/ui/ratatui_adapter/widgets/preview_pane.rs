//! Preview pane widget for displaying file previews

use crate::ui::ratatui_adapter::theme::Theme;
use crate::ui::traits::PreviewText;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Text},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};

/// Preview pane widget that displays file content
pub struct PreviewPane<'a> {
    /// Preview content
    content: Option<&'a PreviewText>,
    /// Theme for styling
    theme: &'a Theme,
    /// Title for the preview block
    title: String,
    /// Scroll offset
    scroll: u16,
}

impl<'a> PreviewPane<'a> {
    /// Create a new preview pane widget
    #[must_use]
    pub fn new(content: Option<&'a PreviewText>, theme: &'a Theme) -> Self {
        Self {
            content,
            theme,
            title: " Preview ".to_string(),
            scroll: 0,
        }
    }

    /// Set custom title
    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Set scroll offset
    #[must_use]
    pub const fn scroll(mut self, scroll: usize) -> Self {
        self.scroll = scroll as u16;
        self
    }
}

impl Widget for PreviewPane<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.border_style())
            .title(self.title.as_str());

        let inner = block.inner(area);
        block.render(area, buf);

        let paragraph = match self.content {
            Some(preview) => {
                // TODO: Parse ANSI codes if preview.has_ansi is true
                // For now, render as plain text
                let text = Text::from(preview.content.as_str());
                Paragraph::new(text)
            }
            None => {
                let empty_text = Line::styled("No preview available", self.theme.dimmed_style());
                Paragraph::new(empty_text)
            }
        };

        paragraph
            .scroll((self.scroll, 0))
            .wrap(Wrap { trim: false })
            .render(inner, buf);
    }
}
