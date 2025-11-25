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
    /// Note: Also available via the `Display` trait
    #[must_use]
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
            Self::Empty => write!(f, "Empty file (0 bytes)"),
            Self::Error(msg) => write!(f, "Error: {msg}"),
        }
    }
}

/// Format file metadata for display
fn format_file_metadata(metadata: &FileMetadata) -> String {
    use byte_unit::{Byte, UnitType};

    let mut output = String::from("Binary file - cannot preview\n\n");
    output.push_str(&format!("Path: {}\n", metadata.path.display()));

    let size = Byte::from_u64(metadata.size)
        .get_appropriate_unit(UnitType::Binary)
        .to_string();
    output.push_str(&format!("Size: {size}\n"));

    if let Some(modified) = metadata.modified {
        use chrono::{Local, TimeZone};
        if let Some(dt) = Local.timestamp_opt(modified, 0).single() {
            output.push_str(&format!("Modified: {}\n", dt.format("%Y-%m-%d %H:%M:%S")));
        }
    }

    if let Some(perms) = &metadata.permissions {
        output.push_str(&format!("Permissions: {perms}\n"));
    }

    if let Some(file_type) = &metadata.file_type {
        output.push_str(&format!("Type: {file_type}\n"));
    }

    output
}

/// Format image metadata for display
fn format_image_metadata(metadata: &ImageMetadata) -> String {
    let mut output = format_file_metadata(&metadata.file_metadata);
    output.push('\n');

    if let Some(format) = &metadata.format {
        output.push_str(&format!("Format: {format}\n"));
    }

    if let (Some(width), Some(height)) = (metadata.width, metadata.height) {
        output.push_str(&format!("Dimensions: {width} x {height} pixels\n"));
    }

    output
}
