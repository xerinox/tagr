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
    /// Optional CLI preview command (educational feature)
    cli_preview: Option<&'a str>,
}

impl<'a> StatusBar<'a> {
    /// Create a new status bar widget
    #[must_use]
    pub const fn new(messages: &'a [&'a StatusMessage], theme: &'a Theme) -> Self {
        Self {
            messages,
            theme,
            cli_preview: None,
        }
    }

    /// Set CLI preview command
    #[must_use]
    pub const fn with_cli_preview(mut self, preview: Option<&'a str>) -> Self {
        self.cli_preview = preview;
        self
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

    /// Build a syntax-highlighted line for CLI preview
    fn build_cli_preview_line(&self, cmd: &str) -> Line<'static> {
        use ratatui::style::{Color, Modifier, Style};

        let mut spans = Vec::new();

        // Prefix
        spans.push(Span::styled(
            "CLI: ".to_string(),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::DIM),
        ));

        // Parse and color-code the command
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        for (i, part) in parts.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw(" ".to_string()));
            }

            let style = if i == 0 {
                // Command name (tagr)
                Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD)
            } else if i == 1 {
                // Subcommand (search/browse)
                Style::default().fg(Color::Magenta)
            } else if part.starts_with('-') {
                // Flags (-t, --any-tag, etc.)
                Style::default().fg(Color::Yellow)
            } else {
                // Tag values
                Style::default().fg(Color::Green)
            };

            spans.push(Span::styled(part.to_string(), style));
        }

        Line::from(spans)
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

        // Priority 1: Show CLI preview if available (educational feature)
        if let Some(cmd) = self.cli_preview {
            let line = self.build_cli_preview_line(cmd);
            Paragraph::new(line).render(inner, buf);
            return;
        }

        // Priority 2: Show messages if any
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
