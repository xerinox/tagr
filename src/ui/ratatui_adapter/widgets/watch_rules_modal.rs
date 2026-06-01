//! Watch rules modal widget for displaying active watch rules

use crate::ui::ratatui_adapter::theme::Theme;
use crate::watch::WatchRule;
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap},
};

/// State for the watch rules modal (tracks scroll position)
#[derive(Debug, Clone, Default)]
pub struct WatchRulesState {
    /// Loaded watch rules
    pub rules: Vec<WatchRule>,
    /// Current scroll offset
    pub scroll: usize,
}

impl WatchRulesState {
    /// Create state from loaded rules
    #[must_use]
    pub const fn new(rules: Vec<WatchRule>) -> Self {
        Self { rules, scroll: 0 }
    }

    /// Scroll up by one line
    pub const fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    /// Scroll down by one line (clamped to content)
    pub fn scroll_down(&mut self, visible_height: usize) {
        let content_lines = self.content_line_count();
        if content_lines > visible_height {
            self.scroll = self.scroll.min(content_lines - visible_height).saturating_add(1);
            self.scroll = self.scroll.min(content_lines.saturating_sub(visible_height));
        }
    }

    /// Estimate number of rendered content lines
    fn content_line_count(&self) -> usize {
        if self.rules.is_empty() {
            return 3;
        }
        // header(2) + per-rule(patterns + tags + optional fields + separator)
        let mut lines = 2;
        for rule in &self.rules {
            lines += 2; // "Rule #N" + separator
            lines += 1; // patterns line
            lines += 1; // tags line
            if rule.filter.is_some() {
                lines += 1;
            }
            if !rule.vtags.is_empty() {
                lines += 1;
            }
            if !rule.filter_by_tags.is_empty() {
                lines += 1;
            }
            lines += 1; // blank separator
        }
        lines + 2 // footer
    }
}

/// Watch rules modal widget
pub struct WatchRulesModal<'a> {
    /// State with rules and scroll position
    state: &'a WatchRulesState,
    /// Theme for styling
    theme: &'a Theme,
}

impl<'a> WatchRulesModal<'a> {
    /// Create a new watch rules modal
    #[must_use]
    pub const fn new(state: &'a WatchRulesState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }

    /// Calculate centered area for the modal
    fn centered_rect(width_percent: u16, height_percent: u16, area: Rect) -> Rect {
        let w = width_percent.min(90);
        let h = height_percent.min(90);
        let popup_layout = Layout::vertical([
            Constraint::Percentage((100 - h) / 2),
            Constraint::Percentage(h),
            Constraint::Percentage((100 - h) / 2),
        ])
        .split(area);

        Layout::horizontal([
            Constraint::Percentage((100 - w) / 2),
            Constraint::Percentage(w),
            Constraint::Percentage((100 - w) / 2),
        ])
        .split(popup_layout[1])[1]
    }

    /// Build content lines
    fn build_content(&self) -> Vec<Line<'static>> {
        let label_style = Style::default().fg(Color::DarkGray);
        let value_style = Style::default().fg(Color::White);
        let tag_style = Style::default().fg(Color::Cyan);
        let heading_style = Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD);
        let dim_style = Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC);

        if self.state.rules.is_empty() {
            return vec![
                Line::default(),
                Line::from(Span::styled("No watch rules configured.", dim_style)),
                Line::default(),
                Line::from(Span::styled(
                    "Add rules in ~/.config/tagr/watch.toml",
                    dim_style,
                )),
                Line::default(),
                Line::from("─".repeat(50)),
                Line::default(),
                Line::from(Span::styled(
                    "ESC / any key to close   ↑↓ scroll",
                    dim_style,
                )),
            ];
        }

        let mut lines = Vec::new();
        lines.push(Line::default());

        #[allow(clippy::cast_possible_truncation)]
        let rule_count = self.state.rules.len();
        lines.push(Line::from(vec![
            Span::styled(
                format!("{rule_count} rule{}", if rule_count == 1 { "" } else { "s" }),
                heading_style,
            ),
            Span::styled(" loaded from ", label_style),
            Span::styled("watch.toml", value_style),
        ]));
        lines.push(Line::from("─".repeat(50)));

        for (i, rule) in self.state.rules.iter().enumerate() {
            lines.push(Line::default());
            lines.push(Line::from(Span::styled(
                format!("Rule #{}", i + 1),
                heading_style,
            )));

            // Patterns
            lines.push(Line::from(vec![
                Span::styled("  Patterns:  ", label_style),
                Span::styled(rule.patterns.join(", "), value_style),
            ]));

            // Tags
            lines.push(Line::from(vec![
                Span::styled("  Tags:      ", label_style),
                Span::styled(rule.tags.join(", "), tag_style),
            ]));

            // Optional: filter
            if let Some(ref filter) = rule.filter {
                lines.push(Line::from(vec![
                    Span::styled("  Filter:    ", label_style),
                    Span::styled(filter.clone(), value_style),
                ]));
            }

            // Optional: vtags
            if !rule.vtags.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled("  VTags:     ", label_style),
                    Span::styled(rule.vtags.join(", "), value_style),
                ]));
            }

            // Optional: filter_by_tags
            if !rule.filter_by_tags.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled("  Gate tags: ", label_style),
                    Span::styled(rule.filter_by_tags.join(", "), tag_style),
                ]));
            }
        }

        lines.push(Line::default());
        lines.push(Line::from("─".repeat(50)));
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            "ESC / any key to close   ↑↓ scroll",
            dim_style,
        )));

        lines
    }
}

impl Widget for WatchRulesModal<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let popup_area = Self::centered_rect(70, 70, area);

        Clear.render(popup_area, buf);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.cursor_style())
            .title(" Watch Rules ")
            .title_alignment(Alignment::Center);

        let content = self.build_content();

        #[allow(clippy::cast_possible_truncation)]
        let scroll_offset = self.state.scroll as u16;

        Paragraph::new(content)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((scroll_offset, 0))
            .render(popup_area, buf);
    }
}
