//! Details modal widget for displaying file information

use crate::{db::NoteRecord, ui::ratatui_adapter::theme::Theme};
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap},
};
use std::path::{Path, PathBuf};

/// File details to display in the modal
#[derive(Debug, Clone, PartialEq)]
pub struct FileDetails {
    /// File path
    pub path: PathBuf,
    /// File size in bytes
    pub size: u64,
    /// Last modified timestamp (formatted)
    pub modified: String,
    /// Tags associated with the file
    pub tags: Vec<String>,
    /// Unix file permissions (if available)
    #[cfg(unix)]
    pub permissions: Option<u32>,
    /// Note content (if file has a note)
    pub note: Option<NoteRecord>,
}

impl FileDetails {
    /// Create file details from metadata
    ///
    /// # Errors
    /// Returns error if file metadata cannot be read
    pub fn from_path(
        path: &Path,
        tags: Vec<String>,
        note: Option<NoteRecord>,
    ) -> std::io::Result<Self> {
        let metadata = std::fs::metadata(path)?;

        #[cfg(unix)]
        let permissions = {
            use std::os::unix::fs::PermissionsExt;
            Some(metadata.permissions().mode() & 0o777)
        };

        let modified = if let Ok(time) = metadata.modified() {
            if let Ok(duration) = time.duration_since(std::time::UNIX_EPOCH) {
                let timestamp = duration.as_secs() as i64;
                let dt =
                    chrono::DateTime::from_timestamp(timestamp, 0).unwrap_or_else(chrono::Utc::now);
                dt.format("%Y-%m-%d %H:%M:%S").to_string()
            } else {
                "Unknown".to_string()
            }
        } else {
            "Unknown".to_string()
        };

        Ok(Self {
            path: path.to_path_buf(),
            size: metadata.len(),
            modified,
            tags,
            #[cfg(unix)]
            permissions,
            note,
        })
    }
}

/// Details modal widget that displays file information
pub struct DetailsModal<'a> {
    /// File details to display
    details: &'a FileDetails,
    /// Theme for styling
    theme: &'a Theme,
}

impl<'a> DetailsModal<'a> {
    /// Create a new details modal
    #[must_use]
    pub const fn new(details: &'a FileDetails, theme: &'a Theme) -> Self {
        Self { details, theme }
    }

    /// Calculate centered area for the modal
    fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
        let popup_layout = Layout::vertical([
            Constraint::Percentage((100 - height.min(90)) / 2),
            Constraint::Percentage(height.min(90)),
            Constraint::Percentage((100 - height.min(90)) / 2),
        ])
        .split(area);

        Layout::horizontal([
            Constraint::Percentage((100 - width.min(90)) / 2),
            Constraint::Percentage(width.min(90)),
            Constraint::Percentage((100 - width.min(90)) / 2),
        ])
        .split(popup_layout[1])[1]
    }

    /// Format file size with units
    fn format_size(bytes: u64) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
        let mut size = bytes as f64;
        let mut unit_idx = 0;

        while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
            size /= 1024.0;
            unit_idx += 1;
        }

        if unit_idx == 0 {
            format!("{size:.0} {}", UNITS[unit_idx])
        } else {
            format!("{size:.2} {}", UNITS[unit_idx])
        }
    }

    /// Build content lines for the modal
    fn build_content(&self) -> Vec<Line<'static>> {
        let mut lines = vec![Line::from(vec![Span::styled(
            self.details.path.display().to_string(),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )])];
        lines.push(Line::from("─".repeat(70)));
        lines.push(Line::default());

        // File metadata
        lines.push(Line::from(vec![
            Span::styled("Size:     ", Style::default().fg(Color::DarkGray)),
            Span::raw(Self::format_size(self.details.size)),
        ]));

        lines.push(Line::from(vec![
            Span::styled("Modified: ", Style::default().fg(Color::DarkGray)),
            Span::raw(self.details.modified.clone()),
        ]));

        #[cfg(unix)]
        if let Some(perms) = self.details.permissions {
            lines.push(Line::from(vec![
                Span::styled("Permissions: ", Style::default().fg(Color::DarkGray)),
                Span::raw(format!("{perms:o}")),
            ]));
        }

        lines.push(Line::default());

        // Tags
        lines.push(Line::from(vec![
            Span::styled("Tags:     ", Style::default().fg(Color::DarkGray)),
            if self.details.tags.is_empty() {
                Span::styled(
                    "(none)",
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC),
                )
            } else {
                Span::styled(
                    self.details.tags.join(", "),
                    Style::default().fg(Color::Cyan),
                )
            },
        ]));

        // Note preview (if exists)
        if let Some(note) = &self.details.note {
            lines.push(Line::default());
            lines.push(Line::from("─".repeat(70)));
            lines.push(Line::default());

            lines.push(Line::from(vec![Span::styled(
                "Note",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )]));
            lines.push(Line::default());

            // Note metadata
            let created_dt = chrono::DateTime::from_timestamp(note.metadata.created_at, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_else(|| "Unknown".to_string());
            let updated_dt = chrono::DateTime::from_timestamp(note.metadata.updated_at, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_else(|| "Unknown".to_string());

            lines.push(Line::from(vec![
                Span::styled("Created: ", Style::default().fg(Color::DarkGray)),
                Span::raw(created_dt),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Updated: ", Style::default().fg(Color::DarkGray)),
                Span::raw(updated_dt),
            ]));
            lines.push(Line::default());

            // Note content preview (first 10 lines)
            let content_lines: Vec<&str> = note.content.lines().take(10).collect();
            for line in content_lines {
                lines.push(Line::from(Span::raw(line.to_string())));
            }

            if note.content.lines().count() > 10 {
                lines.push(Line::from(Span::styled(
                    "... (truncated)",
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC),
                )));
            }
        }

        lines.push(Line::default());
        lines.push(Line::from("─".repeat(70)));
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            "Press any key to close",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        )));

        lines
    }
}

impl Widget for DetailsModal<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Calculate modal size
        let width = 80;
        let height = 70;
        let popup_area = Self::centered_rect(width, height, area);

        // Clear the background
        Clear.render(popup_area, buf);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.cursor_style())
            .title(" File Details ")
            .title_alignment(Alignment::Center);

        let content = self.build_content();
        let paragraph = Paragraph::new(content)
            .block(block)
            .wrap(Wrap { trim: false });
        paragraph.render(popup_area, buf);
    }
}
