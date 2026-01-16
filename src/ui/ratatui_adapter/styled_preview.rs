//! Native ratatui styled preview generation
//!
//! Converts syntect highlighting directly to ratatui styles without
//! intermediate ANSI escape codes.

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use std::path::Path;

#[cfg(feature = "syntax-highlighting")]
use syntect::easy::HighlightLines;
#[cfg(feature = "syntax-highlighting")]
use syntect::highlighting::{FontStyle, ThemeSet};
#[cfg(feature = "syntax-highlighting")]
use syntect::parsing::SyntaxSet;

/// Styled preview content ready for ratatui rendering
#[derive(Debug, Clone)]
pub struct StyledPreview {
    /// Lines of styled text
    pub lines: Vec<Line<'static>>,
    /// Whether the content was truncated
    pub truncated: bool,
    /// Total number of lines in original file
    pub total_lines: usize,
    /// Title for the preview (filename, etc.)
    pub title: String,
}

impl StyledPreview {
    /// Create a preview with an error message
    #[must_use]
    pub fn error(message: impl Into<String>) -> Self {
        let error_style = Style::default().fg(Color::Red);
        Self {
            lines: vec![Line::styled(message.into(), error_style)],
            truncated: false,
            total_lines: 1,
            title: String::from(" Error "),
        }
    }

    /// Create an empty preview
    #[must_use]
    pub fn empty() -> Self {
        let dim_style = Style::default().fg(Color::DarkGray);
        Self {
            lines: vec![Line::styled("Empty file (0 bytes)", dim_style)],
            truncated: false,
            total_lines: 0,
            title: String::from(" Preview "),
        }
    }

    /// Create a preview for binary files
    #[must_use]
    pub fn binary(metadata: &str) -> Self {
        let dim_style = Style::default().fg(Color::DarkGray);
        let lines: Vec<Line<'static>> = metadata
            .lines()
            .map(|line| Line::styled(line.to_string(), dim_style))
            .collect();
        Self {
            lines,
            truncated: false,
            total_lines: 0,
            title: String::from(" Binary File "),
        }
    }

    /// Create a preview for a note with syntax highlighting
    #[must_use]
    pub fn note(note_record: &crate::db::NoteRecord) -> Self {
        use chrono::{Local, TimeZone};

        let dim_style = Style::default().fg(Color::DarkGray);

        let mut lines = Vec::new();

        // Metadata section
        let created = Local
            .timestamp_opt(note_record.metadata.created_at, 0)
            .single()
            .map_or_else(
                || "unknown".to_string(),
                |dt| dt.format("%Y-%m-%d %H:%M:%S").to_string(),
            );
        lines.push(Line::from(vec![
            Span::styled("Created: ", dim_style),
            Span::raw(created),
        ]));

        let updated = Local
            .timestamp_opt(note_record.metadata.updated_at, 0)
            .single()
            .map_or_else(
                || "unknown".to_string(),
                |dt| dt.format("%Y-%m-%d %H:%M:%S").to_string(),
            );
        lines.push(Line::from(vec![
            Span::styled("Updated: ", dim_style),
            Span::raw(updated),
        ]));

        lines.push(Line::raw(""));
        lines.push(Line::styled("─".repeat(60), dim_style));
        lines.push(Line::raw(""));

        // Note content with markdown syntax highlighting
        #[cfg(feature = "syntax-highlighting")]
        let content_lines = Self::highlight_markdown(&note_record.content);

        #[cfg(not(feature = "syntax-highlighting"))]
        let content_lines: Vec<Line<'static>> = note_record
            .content
            .lines()
            .map(|line| Self::style_note_line(line))
            .collect();

        let total_lines = content_lines.len();
        lines.extend(content_lines);

        Self {
            lines,
            truncated: false,
            total_lines: total_lines + 8, // +8 for header lines
            title: String::from(" Note "),
        }
    }

    /// Highlight note content as markdown using syntect
    #[cfg(feature = "syntax-highlighting")]
    fn highlight_markdown(content: &str) -> Vec<Line<'static>> {
        use syntect::easy::HighlightLines;
        use syntect::highlighting::ThemeSet;
        use syntect::parsing::SyntaxSet;

        let syntax_set = SyntaxSet::load_defaults_newlines();
        let theme_set = ThemeSet::load_defaults();

        let syntax = syntax_set
            .find_syntax_by_extension("md")
            .unwrap_or_else(|| syntax_set.find_syntax_plain_text());

        let theme = &theme_set.themes["base16-ocean.dark"];
        let mut highlighter = HighlightLines::new(syntax, theme);

        content
            .lines()
            .map(|line| {
                highlighter.highlight_line(line, &syntax_set).map_or_else(
                    |_| Line::raw(line.to_string()),
                    |ranges| {
                        let spans: Vec<Span<'static>> = ranges
                            .iter()
                            .map(|(style, text)| {
                                Span::styled(text.to_string(), syntect_to_ratatui(style))
                            })
                            .collect();
                        Line::from(spans)
                    },
                )
            })
            .collect()
    }

    /// Style a note content line (fallback when syntax highlighting disabled)
    #[cfg(not(feature = "syntax-highlighting"))]
    fn style_note_line(line: &str) -> Line<'static> {
        // Match pattern: [Note added YYYY-MM-DD HH:MM]
        if line.starts_with("[Note added ") && line.ends_with(']') {
            let header_style = Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD);
            Line::styled(line.to_string(), header_style)
        } else {
            let content_style = Style::default().fg(Color::White);
            Line::styled(line.to_string(), content_style)
        }
    }

    /// Create a preview indicating no note exists
    #[must_use]
    pub fn no_note() -> Self {
        let dim_style = Style::default().fg(Color::DarkGray);
        let hint_style = Style::default().fg(Color::Yellow);

        let lines = vec![
            Line::styled("No note attached to this file", dim_style),
            Line::raw(""),
            Line::from(vec![
                Span::styled("Press ", dim_style),
                Span::styled("Ctrl+N", hint_style.add_modifier(Modifier::BOLD)),
                Span::styled(" to create a note", dim_style),
            ]),
        ];

        Self {
            lines,
            truncated: false,
            total_lines: 3,
            title: String::from(" No Note "),
        }
    }
}

/// Generator for styled previews using native ratatui styles
#[cfg(feature = "syntax-highlighting")]
pub struct StyledPreviewGenerator {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
    max_lines: usize,
}

#[cfg(feature = "syntax-highlighting")]
impl StyledPreviewGenerator {
    /// Create a new styled preview generator
    #[must_use]
    pub fn new(max_lines: usize) -> Self {
        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
            max_lines,
        }
    }

    /// Generate a styled preview for a file
    ///
    /// # Errors
    ///
    /// Returns error if the file cannot be read
    pub fn generate(&self, path: &Path) -> Result<StyledPreview, std::io::Error> {
        if !path.exists() {
            return Ok(StyledPreview::error(format!(
                "File not found: {}",
                path.display()
            )));
        }

        let metadata = std::fs::metadata(path)?;
        if metadata.len() == 0 {
            return Ok(StyledPreview::empty());
        }

        // Try to read as text
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
                // Binary file
                return Ok(StyledPreview::binary(&format!(
                    "Binary file - cannot preview\n\nSize: {} bytes",
                    metadata.len()
                )));
            }
            Err(e) => return Err(e),
        };

        let all_lines: Vec<&str> = content.lines().collect();
        let total_lines = all_lines.len();
        let truncated = total_lines > self.max_lines;
        let lines_to_render: Vec<&str> = all_lines.into_iter().take(self.max_lines).collect();

        // Apply syntax highlighting
        let styled_lines = self.highlight_lines(path, &lines_to_render);

        let title = path
            .file_name()
            .and_then(|n| n.to_str())
            .map_or_else(|| String::from(" Preview "), |n| format!(" {n} "));

        Ok(StyledPreview {
            lines: styled_lines,
            truncated,
            total_lines,
            title,
        })
    }

    /// Apply syntax highlighting to lines
    fn highlight_lines(&self, path: &Path, lines: &[&str]) -> Vec<Line<'static>> {
        let syntax = self
            .syntax_set
            .find_syntax_for_file(path)
            .ok()
            .flatten()
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let theme = &self.theme_set.themes["base16-ocean.dark"];
        let mut highlighter = HighlightLines::new(syntax, theme);

        lines
            .iter()
            .map(|line| {
                highlighter
                    .highlight_line(line, &self.syntax_set)
                    .map_or_else(
                        |_| Line::raw(line.to_string()),
                        |ranges| {
                            let spans: Vec<Span<'static>> = ranges
                                .iter()
                                .map(|(style, text)| {
                                    Span::styled(text.to_string(), syntect_to_ratatui(style))
                                })
                                .collect();
                            Line::from(spans)
                        },
                    )
            })
            .collect()
    }
}

/// Convert syntect style to ratatui style
#[cfg(feature = "syntax-highlighting")]
fn syntect_to_ratatui(style: &syntect::highlighting::Style) -> Style {
    let fg = Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);

    let mut ratatui_style = Style::default().fg(fg);

    // Only set background if it's not the default theme background
    // (to avoid overriding terminal transparency)
    if style.background.a > 0 && style.background != syntect::highlighting::Color::WHITE {
        let bg = Color::Rgb(style.background.r, style.background.g, style.background.b);
        ratatui_style = ratatui_style.bg(bg);
    }

    if style.font_style.contains(FontStyle::BOLD) {
        ratatui_style = ratatui_style.add_modifier(Modifier::BOLD);
    }
    if style.font_style.contains(FontStyle::ITALIC) {
        ratatui_style = ratatui_style.add_modifier(Modifier::ITALIC);
    }
    if style.font_style.contains(FontStyle::UNDERLINE) {
        ratatui_style = ratatui_style.add_modifier(Modifier::UNDERLINED);
    }

    ratatui_style
}

/// Fallback generator when syntax-highlighting feature is disabled
#[cfg(not(feature = "syntax-highlighting"))]
pub struct StyledPreviewGenerator {
    max_lines: usize,
}

#[cfg(not(feature = "syntax-highlighting"))]
impl StyledPreviewGenerator {
    #[must_use]
    pub fn new(max_lines: usize) -> Self {
        Self { max_lines }
    }

    pub fn generate(&self, path: &Path) -> Result<StyledPreview, std::io::Error> {
        if !path.exists() {
            return Ok(StyledPreview::error(format!(
                "File not found: {}",
                path.display()
            )));
        }

        let metadata = std::fs::metadata(path)?;
        if metadata.len() == 0 {
            return Ok(StyledPreview::empty());
        }

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
                return Ok(StyledPreview::binary(&format!(
                    "Binary file - cannot preview\n\nSize: {} bytes",
                    metadata.len()
                )));
            }
            Err(e) => return Err(e),
        };

        let all_lines: Vec<&str> = content.lines().collect();
        let total_lines = all_lines.len();
        let truncated = total_lines > self.max_lines;

        let lines: Vec<Line<'static>> = all_lines
            .into_iter()
            .take(self.max_lines)
            .map(|line| Line::raw(line.to_string()))
            .collect();

        let title = path
            .file_name()
            .and_then(|n| n.to_str())
            .map_or_else(|| String::from(" Preview "), |n| format!(" {} ", n));

        Ok(StyledPreview {
            lines,
            truncated,
            total_lines,
            title,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::NamedTempFile;

    #[test]
    fn test_styled_preview_error() {
        let preview = StyledPreview::error("Test error");
        assert_eq!(preview.lines.len(), 1);
        assert!(!preview.truncated);
    }

    #[test]
    fn test_styled_preview_empty() {
        let preview = StyledPreview::empty();
        assert_eq!(preview.lines.len(), 1);
        assert!(!preview.truncated);
    }

    #[test]
    fn test_generator_nonexistent_file() {
        let generator = StyledPreviewGenerator::new(100);
        let result = generator.generate(Path::new("/nonexistent/file.txt"));
        assert!(result.is_ok());
        let preview = result.unwrap();
        assert!(preview.title.contains("Error"));
    }

    #[test]
    fn test_generator_text_file() {
        let temp = NamedTempFile::new().unwrap();
        fs::write(temp.path(), "Line 1\nLine 2\nLine 3").unwrap();

        let generator = StyledPreviewGenerator::new(100);
        let result = generator.generate(temp.path());
        assert!(result.is_ok());

        let preview = result.unwrap();
        assert_eq!(preview.lines.len(), 3);
        assert!(!preview.truncated);
        assert_eq!(preview.total_lines, 3);
    }

    #[test]
    fn test_generator_truncation() {
        let temp = NamedTempFile::new().unwrap();
        let content: String = (0..100).map(|i| format!("Line {i}\n")).collect();
        fs::write(temp.path(), content).unwrap();

        let generator = StyledPreviewGenerator::new(10);
        let result = generator.generate(temp.path());
        assert!(result.is_ok());

        let preview = result.unwrap();
        assert_eq!(preview.lines.len(), 10);
        assert!(preview.truncated);
        assert_eq!(preview.total_lines, 100);
    }
}
