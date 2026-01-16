//! Preview content types

use std::path::PathBuf;

/// Type of content being previewed
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewContent {
    /// Text file content
    Text {
        /// Lines of text
        lines: Vec<String>,
        /// Whether the content was truncated
        truncated: bool,
        /// Total number of lines in file
        total_lines: usize,
        /// Whether the content contains ANSI escape codes
        has_ansi: bool,
    },

    /// Binary file with metadata
    Binary {
        /// File metadata
        metadata: FileMetadata,
    },

    /// Image file with metadata
    Image {
        /// Image-specific metadata
        metadata: ImageMetadata,
    },

    /// Archive file with contents listing
    Archive {
        /// List of files in archive
        contents: Vec<String>,
        /// Whether the listing was truncated
        truncated: bool,
    },

    /// Note content attached to a file
    Note {
        /// Note content (markdown)
        content: String,
        /// Creation timestamp
        created_at: i64,
        /// Last update timestamp
        updated_at: i64,
        /// Optional author
        author: Option<String>,
    },

    /// Empty file
    Empty,

    /// Error occurred during preview generation
    Error(String),
}

/// File metadata for non-text files
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMetadata {
    /// File path
    pub path: PathBuf,
    /// File size in bytes
    pub size: u64,
    /// File modified time (Unix timestamp)
    pub modified: Option<i64>,
    /// File permissions (Unix-style string like "rwxr-xr-x")
    pub permissions: Option<String>,
    /// Detected file type
    pub file_type: Option<String>,
}

/// Image metadata
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageMetadata {
    /// Base file metadata
    pub file_metadata: FileMetadata,
    /// Image width in pixels
    pub width: Option<u32>,
    /// Image height in pixels
    pub height: Option<u32>,
    /// Image format (PNG, JPEG, etc.)
    pub format: Option<String>,
}

impl PreviewContent {
    /// Check if content was truncated
    #[must_use]
    pub const fn is_truncated(&self) -> bool {
        match self {
            Self::Text { truncated, .. } | Self::Archive { truncated, .. } => *truncated,
            _ => false,
        }
    }

    /// Get a display string for the content
    ///
    /// # Note
    ///
    /// This method is deprecated. Use the `Display` trait instead:
    /// ```ignore
    /// let content = PreviewContent::Empty;
    /// // Instead of: content.to_display_string()
    /// // Use: content.to_string() or format!("{}", content)
    /// ```
    #[must_use]
    #[deprecated(
        since = "0.5.0",
        note = "Use Display trait instead: .to_string() or format!()"
    )]
    pub fn to_display_string(&self) -> String {
        format!("{self}")
    }
}

impl std::fmt::Display for PreviewContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text {
                lines,
                truncated,
                total_lines,
                has_ansi: _,
            } => {
                write!(f, "{}", lines.join("\n"))?;
                if *truncated {
                    write!(
                        f,
                        "\n\n[... truncated, showing {} of {} lines ...]",
                        lines.len(),
                        total_lines
                    )?;
                }
                Ok(())
            }
            Self::Binary { metadata } => write!(f, "{}", format_file_metadata(metadata)),
            Self::Image { metadata } => write!(f, "{}", format_image_metadata(metadata)),
            Self::Archive {
                contents,
                truncated,
            } => {
                write!(f, "Archive contents:\n\n{}", contents.join("\n"))?;
                if *truncated {
                    write!(f, "\n\n[... more files ...]")?;
                }
                Ok(())
            }
            Self::Note {
                content,
                created_at,
                updated_at,
                author,
            } => {
                writeln!(f, "📝 Note")?;
                writeln!(f, "Created: {}", format_timestamp(*created_at))?;
                writeln!(f, "Updated: {}", format_timestamp(*updated_at))?;
                if let Some(author_name) = author {
                    writeln!(f, "Author: {author_name}")?;
                }
                write!(f, "\n{}", content.trim())
            }
            Self::Empty => write!(f, "Empty file (0 bytes)"),
            Self::Error(msg) => write!(f, "Error: {msg}"),
        }
    }
}

/// Format file metadata for display
fn format_file_metadata(metadata: &FileMetadata) -> String {
    use byte_unit::{Byte, UnitType};
    use std::fmt::Write;

    let mut output = String::from("Binary file - cannot preview\n\n");
    let _ = writeln!(output, "Path: {}", metadata.path.display());

    let size = Byte::from_u64(metadata.size)
        .get_appropriate_unit(UnitType::Binary)
        .to_string();
    let _ = writeln!(output, "Size: {size}");

    if let Some(modified) = metadata.modified {
        use chrono::{Local, TimeZone};
        if let Some(dt) = Local.timestamp_opt(modified, 0).single() {
            let _ = writeln!(output, "Modified: {}", dt.format("%Y-%m-%d %H:%M:%S"));
        }
    }

    if let Some(perms) = &metadata.permissions {
        let _ = writeln!(output, "Permissions: {perms}");
    }

    if let Some(file_type) = &metadata.file_type {
        let _ = writeln!(output, "Type: {file_type}");
    }

    output
}

/// Format image metadata for display
fn format_image_metadata(metadata: &ImageMetadata) -> String {
    use std::fmt::Write;

    let mut output = format_file_metadata(&metadata.file_metadata);
    output.push('\n');

    if let Some(format) = &metadata.format {
        let _ = writeln!(output, "Format: {format}");
    }

    if let (Some(width), Some(height)) = (metadata.width, metadata.height) {
        let _ = writeln!(output, "Dimensions: {width} x {height} pixels");
    }

    output
}

/// Format Unix timestamp as human-readable string
fn format_timestamp(timestamp: i64) -> String {
    use chrono::{DateTime, Local, TimeZone};

    Local.timestamp_opt(timestamp, 0).single().map_or_else(
        || "unknown".to_string(),
        |dt: DateTime<Local>| dt.format("%Y-%m-%d %H:%M:%S").to_string(),
    )
}
