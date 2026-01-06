//! Help overlay widget for displaying full keybind reference

use crate::ui::ratatui_adapter::theme::Theme;
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

/// Help overlay widget that displays a centered help screen
pub struct HelpOverlay<'a> {
    /// Theme for styling
    theme: &'a Theme,
    /// Custom keybind descriptions
    custom_binds: Vec<(String, String)>,
}

impl<'a> HelpOverlay<'a> {
    /// Create a new help overlay
    #[must_use]
    pub const fn new(theme: &'a Theme) -> Self {
        Self {
            theme,
            custom_binds: Vec::new(),
        }
    }

    /// Add custom keybinds to display
    #[must_use]
    pub fn with_custom_binds(mut self, binds: Vec<(String, String)>) -> Self {
        self.custom_binds = binds;
        self
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

    /// Build help content lines
    fn build_content(&self) -> Vec<Line<'static>> {
        let mut lines = vec![
            Line::default(),
            Line::styled(
                "  Navigation",
                self.theme.cursor_style().add_modifier(Modifier::UNDERLINED),
            ),
            Line::default(),
            Self::help_line("  ↑/↓", "Move cursor"),
            Self::help_line("  PgUp/PgDn", "Page up/down"),
            Self::help_line("  Home/End", "Jump to start/end"),
            Self::help_line("  TAB", "Toggle selection"),
            Self::help_line("  Enter", "Confirm selection"),
            Self::help_line("  ESC", "Cancel / Go back"),
            Line::default(),
            Line::styled(
                "  Search",
                self.theme.cursor_style().add_modifier(Modifier::UNDERLINED),
            ),
            Line::default(),
            Self::help_line("  Type", "Filter items"),
            Self::help_line("  Ctrl+U", "Clear query"),
            Self::help_line("  Ctrl+W", "Delete word"),
            Self::help_line("  ←/→", "Move cursor in query"),
        ];

        if !self.custom_binds.is_empty() {
            lines.push(Line::default());
            lines.push(Line::styled(
                "  Actions",
                self.theme.cursor_style().add_modifier(Modifier::UNDERLINED),
            ));
            lines.push(Line::default());

            for (key, action) in &self.custom_binds {
                lines.push(Self::help_line_owned(&format!("  {key}"), action.clone()));
            }
        }

        lines.push(Line::default());
        lines.push(Line::styled(
            "  Press any key to close",
            self.theme.dimmed_style(),
        ));
        lines.push(Line::default());

        lines
    }

    /// Create a help line with key and description
    fn help_line(key: &'static str, desc: &'static str) -> Line<'static> {
        Line::from(vec![
            Span::styled(
                format!("{key:<14}"),
                ratatui::style::Style::default().fg(ratatui::style::Color::Cyan),
            ),
            Span::raw(desc),
        ])
    }

    /// Create a help line with owned strings
    fn help_line_owned(key: &str, desc: String) -> Line<'static> {
        Line::from(vec![
            Span::styled(
                format!("{key:<14}"),
                ratatui::style::Style::default().fg(ratatui::style::Color::Cyan),
            ),
            Span::raw(desc),
        ])
    }
}

impl Widget for HelpOverlay<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let popup_area = Self::centered_rect(60, 70, area);

        // Clear the background
        Clear.render(popup_area, buf);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.cursor_style())
            .title(" Help ")
            .title_alignment(Alignment::Center);

        let content = self.build_content();
        let paragraph = Paragraph::new(content).block(block);
        paragraph.render(popup_area, buf);
    }
}
