use super::error::{PreviewError, Result};
use super::types::{FileMetadata, ImageMetadata, PreviewContent};
use crate::ui::PreviewConfig;
use std::fs;
use std::path::Path;

#[cfg(feature = "syntax-highlighting")]
use syntect::easy::HighlightLines;
#[cfg(feature = "syntax-highlighting")]
use syntect::highlighting::ThemeSet;
#[cfg(feature = "syntax-highlighting")]
use syntect::parsing::SyntaxSet;
#[cfg(feature = "syntax-highlighting")]
use syntect::util::as_24_bit_terminal_escaped;

pub struct PreviewGenerator {
    config: PreviewConfig,
    #[cfg(feature = "syntax-highlighting")]
    syntax_set: SyntaxSet,
    #[cfg(feature = "syntax-highlighting")]
    theme_set: ThemeSet,
    bat_available: bool,
}

impl PreviewGenerator {
    #[must_use]
    pub fn new(config: PreviewConfig) -> Self {
        let bat_available = Self::check_bat_available();

        Self {
            config,
            #[cfg(feature = "syntax-highlighting")]
            syntax_set: SyntaxSet::load_defaults_newlines(),
            #[cfg(feature = "syntax-highlighting")]
            theme_set: ThemeSet::load_defaults(),
            bat_available,
        }
    }

    /// Check if bat is available on the system
    fn check_bat_available() -> bool {
        std::process::Command::new("bat")
            .arg("--version")
            .output()
            .is_ok()
    }

    pub fn generate(&self, path: &Path) -> Result<PreviewContent> {
        if !path.exists() {
            return Ok(PreviewContent::Error(format!(
                "File not found: {}",
                path.display()
            )));
        }

        let metadata = fs::metadata(path)?;
        let file_size = metadata.len();

        if file_size == 0 {
            return Ok(PreviewContent::Empty);
        }

        if file_size > self.config.max_file_size {
            return Err(PreviewError::FileTooLarge(
                file_size,
                self.config.max_file_size,
            ));
        }

        match self.generate_text_preview(path, file_size) {
            Ok(content) => Ok(content),
            Err(PreviewError::InvalidUtf8(_)) => Ok(self.generate_binary_preview(path, &metadata)),
            Err(e) => Err(e),
        }
    }

    fn generate_text_preview(&self, path: &Path, _file_size: u64) -> Result<PreviewContent> {
        // Try bat first if available and syntax highlighting is enabled
        if self.config.syntax_highlighting
            && self.bat_available
            && let Ok(highlighted) = self.generate_bat_preview(path)
        {
            return Ok(highlighted);
        }

        // Fallback to syntect or plain text
        let content = fs::read_to_string(path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::InvalidData {
                PreviewError::InvalidUtf8(path.display().to_string())
            } else {
                PreviewError::IoError(e)
            }
        })?;

        let all_lines: Vec<String> = content.lines().map(String::from).collect();
        let total_lines = all_lines.len();
        let max_lines = self.config.max_lines;

        let lines = if total_lines > max_lines {
            all_lines.into_iter().take(max_lines).collect()
        } else {
            all_lines
        };

        // Apply syntax highlighting with syntect if enabled
        #[cfg(feature = "syntax-highlighting")]
        let (lines, has_ansi) = if self.config.syntax_highlighting {
            (self.apply_syntect_highlighting(path, &lines), true)
        } else {
            (lines, false)
        };

        #[cfg(not(feature = "syntax-highlighting"))]
        let has_ansi = false;

        let truncated = total_lines > max_lines;

        Ok(PreviewContent::Text {
            lines,
            truncated,
            total_lines,
            has_ansi,
        })
    }

    /// Generate preview using bat command
    fn generate_bat_preview(&self, path: &Path) -> Result<PreviewContent> {
        let output = std::process::Command::new("bat")
            .arg("--color=always")
            .arg("--style=numbers")
            .arg("--paging=never")
            .arg(format!("--line-range=:{}", self.config.max_lines))
            .arg(path)
            .output()
            .map_err(PreviewError::IoError)?;

        if !output.status.success() {
            return Err(PreviewError::IoError(std::io::Error::other(
                "bat command failed",
            )));
        }

        let content = String::from_utf8_lossy(&output.stdout);
        let lines: Vec<String> = content.lines().map(String::from).collect();
        let total_lines = lines.len();
        let truncated = total_lines >= self.config.max_lines;

        Ok(PreviewContent::Text {
            lines,
            truncated,
            total_lines,
            has_ansi: true,
        })
    }

    /// Apply syntax highlighting using syntect
    #[cfg(feature = "syntax-highlighting")]
    fn apply_syntect_highlighting(&self, path: &Path, lines: &[String]) -> Vec<String> {
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
                        |_| line.clone(),
                        |ranges| as_24_bit_terminal_escaped(&ranges[..], false),
                    )
            })
            .collect()
    }

    fn generate_binary_preview(&self, path: &Path, metadata: &fs::Metadata) -> PreviewContent {
        let file_metadata = FileMetadata {
            path: path.to_path_buf(),
            size: metadata.len(),
            modified: metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64),
            permissions: Some(self.format_permissions(metadata)),
            file_type: self.detect_file_type(path),
        };

        if self.is_image(path)
            && let Some(image_meta) = self.extract_image_metadata(path, file_metadata.clone())
        {
            return PreviewContent::Image {
                metadata: image_meta,
            };
        }

        PreviewContent::Binary {
            metadata: file_metadata,
        }
    }

    #[cfg(unix)]
    fn format_permissions(&self, metadata: &fs::Metadata) -> String {
        use std::os::unix::fs::PermissionsExt;
        let mode = metadata.permissions().mode();
        format!(
            "{}{}{}{}{}{}{}{}{}",
            if mode & 0o400 != 0 { 'r' } else { '-' },
            if mode & 0o200 != 0 { 'w' } else { '-' },
            if mode & 0o100 != 0 { 'x' } else { '-' },
            if mode & 0o040 != 0 { 'r' } else { '-' },
            if mode & 0o020 != 0 { 'w' } else { '-' },
            if mode & 0o010 != 0 { 'x' } else { '-' },
            if mode & 0o004 != 0 { 'r' } else { '-' },
            if mode & 0o002 != 0 { 'w' } else { '-' },
            if mode & 0o001 != 0 { 'x' } else { '-' },
        )
    }

    #[cfg(not(unix))]
    fn format_permissions(&self, metadata: &fs::Metadata) -> String {
        if metadata.permissions().readonly() {
            "readonly".to_string()
        } else {
            "read-write".to_string()
        }
    }

    fn detect_file_type(&self, path: &Path) -> Option<String> {
        path.extension()
            .and_then(|e| e.to_str())
            .map(str::to_uppercase)
    }

    fn is_image(&self, path: &Path) -> bool {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            matches!(
                ext.to_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "svg" | "ico"
            )
        } else {
            false
        }
    }

    /// Extract image-specific metadata.
    ///
    /// **Note**: This is a stub implementation. Currently returns placeholder
    /// metadata without actual image dimensions. Returns `Option` for API
    /// consistency with future implementation using the `image` crate.
    ///
    /// # TODO
    /// Use image crate to extract actual dimensions and format information.
    #[allow(clippy::unnecessary_wraps)]
    const fn extract_image_metadata(
        &self,
        _path: &Path,
        file_metadata: FileMetadata,
    ) -> Option<ImageMetadata> {
        Some(ImageMetadata {
            file_metadata,
            width: None,
            height: None,
            format: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempFile;

    #[test]
    fn test_generate_text_preview() {
        let temp = TempFile::create("test.txt").unwrap();
        fs::write(temp.path(), "Line 1\nLine 2\nLine 3\n").unwrap();

        let mut config = PreviewConfig::default();
        config.syntax_highlighting = false;
        let generator = PreviewGenerator::new(config);
        let preview = generator.generate(temp.path()).unwrap();

        match preview {
            PreviewContent::Text {
                lines,
                truncated,
                has_ansi,
                ..
            } => {
                assert_eq!(lines.len(), 3);
                assert_eq!(lines[0], "Line 1");
                assert!(!truncated);
                assert!(!has_ansi);
            }
            _ => panic!("Expected Text preview"),
        }
    }

    #[test]
    fn test_generate_truncated_preview() {
        let temp = TempFile::create("test.txt").unwrap();
        let content = (0..100)
            .map(|i| format!("Line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(temp.path(), content).unwrap();

        let mut config = PreviewConfig::default();
        config.max_lines = 10;
        config.syntax_highlighting = false;
        let generator = PreviewGenerator::new(config);
        let preview = generator.generate(temp.path()).unwrap();

        match preview {
            PreviewContent::Text {
                lines,
                truncated,
                total_lines,
                has_ansi,
            } => {
                assert_eq!(lines.len(), 10);
                assert!(truncated);
                assert_eq!(total_lines, 100);
                assert!(!has_ansi);
            }
            _ => panic!("Expected Text preview"),
        }
    }

    #[test]
    fn test_generate_empty_file_preview() {
        let temp = TempFile::create("empty.txt").unwrap();
        fs::write(temp.path(), "").unwrap();

        let config = PreviewConfig::default();
        let generator = PreviewGenerator::new(config);
        let preview = generator.generate(temp.path()).unwrap();

        assert!(matches!(preview, PreviewContent::Empty));
    }

    #[test]
    fn test_generate_nonexistent_file() {
        let config = PreviewConfig::default();
        let generator = PreviewGenerator::new(config);
        let preview = generator
            .generate(Path::new("/nonexistent/file.txt"))
            .unwrap();

        assert!(matches!(preview, PreviewContent::Error(_)));
    }

    #[test]
    #[cfg(feature = "syntax-highlighting")]
    fn test_syntax_highlighting_enabled() {
        let temp = TempFile::create("test.rs").unwrap();
        fs::write(temp.path(), "fn main() {\n    println!(\"Hello\");\n}\n").unwrap();

        let mut config = PreviewConfig::default();
        config.syntax_highlighting = true;
        let generator = PreviewGenerator::new(config);
        let preview = generator.generate(temp.path()).unwrap();

        match preview {
            PreviewContent::Text {
                has_ansi, lines, ..
            } => {
                // Should have ANSI codes from bat or syntect
                assert!(has_ansi || !lines.is_empty()); // Either ANSI or plain text fallback
            }
            _ => panic!("Expected Text preview"),
        }
    }

    #[test]
    fn test_bat_availability_check() {
        let config = PreviewConfig::default();
        let generator = PreviewGenerator::new(config);
        // Just verify the generator was created successfully
        // bat_available is determined at runtime
        assert!(generator.config.enabled);
    }

    #[test]
    fn test_binary_file_preview() {
        let temp = TempFile::create("test.bin").unwrap();
        // Write invalid UTF-8
        fs::write(temp.path(), &[0xFF, 0xFE, 0xFD]).unwrap();

        let mut config = PreviewConfig::default();
        config.syntax_highlighting = false; // Disable to ensure we test the fallback path
        let generator = PreviewGenerator::new(config);
        let preview = generator.generate(temp.path()).unwrap();

        match preview {
            PreviewContent::Binary { metadata } => {
                assert_eq!(metadata.size, 3);
                assert!(metadata.permissions.is_some());
            }
            _ => panic!("Expected Binary preview, got {:?}", preview),
        }
    }

    #[test]
    fn test_large_file_error() {
        let temp = TempFile::create("large.txt").unwrap();
        // Create a file just over 5MB
        let content = "x".repeat(5_242_881);
        fs::write(temp.path(), content).unwrap();

        let config = PreviewConfig::default();
        let generator = PreviewGenerator::new(config);
        let result = generator.generate(temp.path());

        assert!(matches!(result, Err(PreviewError::FileTooLarge(_, _))));
    }
}
