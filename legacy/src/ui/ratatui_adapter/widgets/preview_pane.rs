//! Preview pane widget for displaying file previews

use crate::ui::ratatui_adapter::styled_preview::StyledPreview;
use crate::ui::ratatui_adapter::theme::Theme;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::Line,
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};

/// Preview pane widget that displays file content with syntax highlighting
pub struct PreviewPane<'a> {
    /// Styled preview content (native ratatui lines)
    styled_content: Option<&'a StyledPreview>,
    /// Theme for styling
    theme: &'a Theme,
    /// Scroll offset
    scroll: u16,
}

impl<'a> PreviewPane<'a> {
    /// Create a new preview pane widget with styled content
    #[must_use]
    pub const fn new(content: Option<&'a StyledPreview>, theme: &'a Theme) -> Self {
        Self {
            styled_content: content,
            theme,
            scroll: 0,
        }
    }

    /// Set scroll offset
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub const fn scroll(mut self, scroll: usize) -> Self {
        self.scroll = scroll as u16;
        self
    }
}

impl Widget for PreviewPane<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (title, lines) = self.styled_content.map_or_else(
            || {
                let empty_line = Line::styled("No preview available", self.theme.dimmed_style());
                (String::from(" Preview "), vec![empty_line])
            },
            |preview| {
                let mut lines = preview.lines.clone();

                // Add truncation message if needed
                if preview.truncated {
                    lines.push(Line::raw(""));
                    lines.push(Line::styled(
                        format!(
                            "... truncated ({} of {} lines) ...",
                            lines.len().saturating_sub(2),
                            preview.total_lines
                        ),
                        self.theme.dimmed_style(),
                    ));
                }

                (preview.title.clone(), lines)
            },
        );

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.border_style())
            .title(title);

        let paragraph = Paragraph::new(lines)
            .block(block)
            .scroll((self.scroll, 0))
            .wrap(Wrap { trim: false });

        paragraph.render(area, buf);
    }
}
