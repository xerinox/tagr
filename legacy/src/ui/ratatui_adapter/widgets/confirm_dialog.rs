//! Confirmation dialog widget for destructive actions
//!
//! Provides a modal dialog overlay that asks the user to confirm
//! before executing potentially destructive operations like deleting
//! files from the database.

use crate::ui::ratatui_adapter::theme::Theme;
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

/// State for the confirmation dialog
#[derive(Debug, Clone)]
pub struct ConfirmDialogState {
    /// The title/prompt for the dialog
    pub title: String,
    /// Detailed message explaining what will happen
    pub message: String,
    /// Action identifier to execute on confirmation
    pub action_id: String,
    /// Additional context data (e.g., file paths, tag names)
    pub context: Vec<String>,
}

impl ConfirmDialogState {
    /// Create a new confirmation dialog state
    #[must_use]
    pub fn new(
        title: impl Into<String>,
        message: impl Into<String>,
        action_id: impl Into<String>,
    ) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            action_id: action_id.into(),
            context: Vec::new(),
        }
    }

    /// Add context data (will be passed back on confirmation)
    #[must_use]
    pub fn with_context(mut self, context: Vec<String>) -> Self {
        self.context = context;
        self
    }
}

/// Confirmation dialog overlay widget
pub struct ConfirmDialog<'a> {
    state: &'a ConfirmDialogState,
    theme: &'a Theme,
}

impl<'a> ConfirmDialog<'a> {
    /// Create a new confirmation dialog widget
    #[must_use]
    pub const fn new(state: &'a ConfirmDialogState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }

    /// Calculate centered area for the modal
    fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        Rect::new(x, y, width.min(area.width), height.min(area.height))
    }
}

impl Widget for ConfirmDialog<'_> {
    #[allow(clippy::cast_possible_truncation)]
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Calculate modal size - wider for longer messages
        let message_width = self.state.message.len() as u16 + 4;
        let width = message_width
            .clamp(40, 70)
            .min(area.width.saturating_sub(4));

        // Height: title border + message + context preview + buttons + help
        let context_lines = if self.state.context.is_empty() {
            0
        } else {
            self.state.context.len().min(3) as u16 + 1 // +1 for separator
        };
        let height = 7 + context_lines; // border + message + spacing + buttons + help + border

        let modal_area = Self::centered_rect(width, height, area);

        // Clear background
        Clear.render(modal_area, buf);

        // Main modal block with warning-colored border
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .title(format!(" {} ", self.state.title))
            .title_alignment(Alignment::Center);

        let inner = block.inner(modal_area);
        block.render(modal_area, buf);

        // Layout
        let mut constraints = vec![
            Constraint::Length(1), // Spacing
            Constraint::Length(1), // Message
        ];

        if context_lines > 0 {
            constraints.push(Constraint::Length(1)); // Separator
            constraints.push(Constraint::Length(context_lines - 1)); // Context items
        }

        constraints.push(Constraint::Length(1)); // Spacing
        constraints.push(Constraint::Length(1)); // Buttons
        constraints.push(Constraint::Length(1)); // Help

        let chunks = Layout::vertical(constraints).split(inner);

        // Message
        let message = Paragraph::new(self.state.message.as_str())
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::White));
        message.render(chunks[1], buf);

        // Context preview (if any)
        let mut next_chunk = 2;
        if !self.state.context.is_empty() {
            // Separator
            let separator = Paragraph::new("─".repeat(inner.width as usize - 2))
                .alignment(Alignment::Center)
                .style(self.theme.dimmed_style());
            separator.render(chunks[next_chunk], buf);
            next_chunk += 1;

            // Show context items (truncated if many)
            let context_text: Vec<Line> = self
                .state
                .context
                .iter()
                .take(3)
                .map(|s| {
                    // Truncate long paths
                    let display = if s.len() > width as usize - 6 {
                        format!("...{}", &s[s.len() - (width as usize - 9)..])
                    } else {
                        s.clone()
                    };
                    Line::from(format!("  • {display}"))
                })
                .collect();

            let context_para = Paragraph::new(context_text).style(self.theme.dimmed_style());
            context_para.render(chunks[next_chunk], buf);
            next_chunk += 1;
        }

        // Spacing
        next_chunk += 1;

        // Button hints
        let buttons = Line::from(vec![
            Span::styled(
                " [Y] Yes ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Red)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("    "),
            Span::styled(
                " [N] No ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);

        let buttons_para = Paragraph::new(buttons).alignment(Alignment::Center);
        buttons_para.render(chunks[next_chunk], buf);
        next_chunk += 1;

        // Help text
        let help = Paragraph::new("Y/Enter: confirm | N/ESC: cancel")
            .style(self.theme.dimmed_style())
            .alignment(Alignment::Center);
        help.render(chunks[next_chunk], buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_confirm_dialog_state_creation() {
        let state = ConfirmDialogState::new(
            "Delete Files",
            "Are you sure you want to delete these files?",
            "delete_from_db",
        );

        assert_eq!(state.title, "Delete Files");
        assert_eq!(state.action_id, "delete_from_db");
        assert!(state.context.is_empty());
    }

    #[test]
    fn test_confirm_dialog_with_context() {
        let state =
            ConfirmDialogState::new("Delete Files", "Delete from database?", "delete_from_db")
                .with_context(vec![
                    "/path/to/file1.txt".to_string(),
                    "/path/to/file2.txt".to_string(),
                ]);

        assert_eq!(state.context.len(), 2);
    }
}
