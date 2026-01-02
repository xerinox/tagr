//! Help bar widget for displaying keybind hints

use crate::ui::ratatui_adapter::theme::Theme;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

/// A keybind hint to display in the help bar
#[derive(Debug, Clone)]
pub struct KeyHint {
    /// Key combination (e.g., "TAB", "ctrl+t")
    pub key: String,
    /// Action description (e.g., "select", "add tag")
    pub action: String,
}

impl KeyHint {
    /// Create a new key hint
    #[must_use]
    pub fn new(key: impl Into<String>, action: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            action: action.into(),
        }
    }
}

/// Help bar widget that displays keybind hints at the bottom
pub struct HelpBar<'a> {
    /// Hints to display
    hints: &'a [KeyHint],
    /// Theme for styling
    theme: &'a Theme,
}

impl<'a> HelpBar<'a> {
    /// Create a new help bar widget
    #[must_use]
    pub const fn new(hints: &'a [KeyHint], theme: &'a Theme) -> Self {
        Self { hints, theme }
    }

    /// Get default hints for the finder
    #[must_use]
    pub fn default_hints() -> Vec<KeyHint> {
        vec![
            KeyHint::new("TAB", "select"),
            KeyHint::new("Enter", "confirm"),
            KeyHint::new("ESC", "cancel"),
            KeyHint::new("?", "help"),
        ]
    }
}

impl Widget for HelpBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut spans = Vec::new();

        for (i, hint) in self.hints.iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled("  ", self.theme.dimmed_style()));
            }
            spans.push(Span::styled(
                hint.key.as_str(),
                self.theme.cursor_style(),
            ));
            spans.push(Span::styled(":", self.theme.dimmed_style()));
            spans.push(Span::raw(hint.action.as_str()));
        }

        let line = Line::from(spans);
        Paragraph::new(line).render(area, buf);
    }
}
