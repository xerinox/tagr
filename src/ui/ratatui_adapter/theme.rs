//! Color theme definitions for the ratatui TUI
//!
//! Defines colors and styles used throughout the application.

use ratatui::style::{Color, Modifier, Style};

/// Theme configuration for the TUI
#[derive(Debug, Clone)]
pub struct Theme {
    /// Background color for selected/highlighted items
    pub selection_bg: Color,
    /// Foreground color for selected items
    pub selection_fg: Color,
    /// Color for matched characters in fuzzy search
    pub match_highlight: Color,
    /// Color for the cursor indicator
    pub cursor: Color,
    /// Color for success messages
    pub success: Color,
    /// Color for error messages
    pub error: Color,
    /// Color for warning messages
    pub warning: Color,
    /// Color for info messages
    pub info: Color,
    /// Color for borders
    pub border: Color,
    /// Color for dimmed/inactive text
    pub dimmed: Color,
    /// Color for tags
    pub tag: Color,
    /// Color for file paths
    pub path: Color,
    /// Color for missing files
    pub missing_file: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl Theme {
    /// Create a dark theme (default)
    #[must_use]
    pub const fn dark() -> Self {
        Self {
            selection_bg: Color::Blue,
            selection_fg: Color::White,
            match_highlight: Color::Yellow,
            cursor: Color::Cyan,
            success: Color::Green,
            error: Color::Red,
            warning: Color::Yellow,
            info: Color::Cyan,
            border: Color::DarkGray,
            dimmed: Color::DarkGray,
            tag: Color::Magenta,
            path: Color::White,
            missing_file: Color::Red,
        }
    }

    /// Style for the currently selected item
    #[must_use]
    pub fn selected_style(&self) -> Style {
        Style::default()
            .bg(self.selection_bg)
            .fg(self.selection_fg)
            .add_modifier(Modifier::BOLD)
    }

    /// Style for unselected items
    #[must_use]
    pub fn normal_style(&self) -> Style {
        Style::default()
    }

    /// Style for the cursor indicator (>)
    #[must_use]
    pub fn cursor_style(&self) -> Style {
        Style::default()
            .fg(self.cursor)
            .add_modifier(Modifier::BOLD)
    }

    /// Style for matched characters in fuzzy search
    #[must_use]
    pub fn match_style(&self) -> Style {
        Style::default()
            .fg(self.match_highlight)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    }

    /// Style for multi-select indicator (✓)
    #[must_use]
    pub fn multi_select_style(&self) -> Style {
        Style::default()
            .fg(self.success)
            .add_modifier(Modifier::BOLD)
    }

    /// Style for success messages
    #[must_use]
    pub fn success_style(&self) -> Style {
        Style::default().fg(self.success)
    }

    /// Style for error messages
    #[must_use]
    pub fn error_style(&self) -> Style {
        Style::default().fg(self.error)
    }

    /// Style for warning messages
    #[must_use]
    pub fn warning_style(&self) -> Style {
        Style::default().fg(self.warning)
    }

    /// Style for info messages
    #[must_use]
    pub fn info_style(&self) -> Style {
        Style::default().fg(self.info)
    }

    /// Style for borders
    #[must_use]
    pub fn border_style(&self) -> Style {
        Style::default().fg(self.border)
    }

    /// Style for dimmed text
    #[must_use]
    pub fn dimmed_style(&self) -> Style {
        Style::default().fg(self.dimmed)
    }

    /// Style for tags
    #[must_use]
    pub fn tag_style(&self) -> Style {
        Style::default().fg(self.tag)
    }

    /// Style for file paths
    #[must_use]
    pub fn path_style(&self) -> Style {
        Style::default().fg(self.path)
    }

    /// Style for missing files
    #[must_use]
    pub fn missing_file_style(&self) -> Style {
        Style::default()
            .fg(self.missing_file)
            .add_modifier(Modifier::CROSSED_OUT)
    }
}
