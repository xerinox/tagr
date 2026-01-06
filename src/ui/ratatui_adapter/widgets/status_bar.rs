//! Status bar widget for displaying messages

use crate::ui::output::MessageLevel;
use crate::ui::ratatui_adapter::state::StatusMessage;
use crate::ui::ratatui_adapter::theme::Theme;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

/// Status bar widget that displays recent messages
pub struct StatusBar<'a> {
    /// Messages to display
    messages: &'a [&'a StatusMessage],
    /// Theme for styling
    theme: &'a Theme,
}

impl<'a> StatusBar<'a> {
    /// Create a new status bar widget
    #[must_use]
    pub const fn new(messages: &'a [&'a StatusMessage], theme: &'a Theme) -> Self {
        Self { messages, theme }
    }

    /// Get style for a message level
    fn style_for_level(&self, level: MessageLevel) -> ratatui::style::Style {
        match level {
            MessageLevel::Success => self.theme.success_style(),
            MessageLevel::Error => self.theme.error_style(),
            MessageLevel::Warning => self.theme.warning_style(),
            MessageLevel::Info => self.theme.info_style(),
            MessageLevel::Normal => self.theme.normal_style(),
        }
    }

    /// Get prefix for a message level
    const fn prefix_for_level(level: MessageLevel) -> &'static str {
        match level {
            MessageLevel::Success => "✓ ",
            MessageLevel::Error => "✗ ",
            MessageLevel::Warning => "⚠ ",
            MessageLevel::Info => "ℹ ",
            MessageLevel::Normal => "",
        }
    }
}

impl Widget for StatusBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.border_style())
            .title(" Status ");

        let inner = block.inner(area);
        block.render(area, buf);

        if self.messages.is_empty() {
            return;
        }

        // Show the most recent message
        if let Some(msg) = self.messages.last() {
            let style = self.style_for_level(msg.level);
            let prefix = Self::prefix_for_level(msg.level);
            let line = Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(msg.text.as_str(), style),
            ]);
            Paragraph::new(line).render(inner, buf);
        }
    }
}
