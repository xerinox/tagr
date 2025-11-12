use super::error::{PreviewError, Result};
use super::types::{FileMetadata, ImageMetadata, PreviewContent};
use crate::ui::PreviewConfig;
use std::fs;
use std::path::Path;

pub struct PreviewGenerator {
    config: PreviewConfig,
}

impl PreviewGenerator {
    #[must_use]
    pub const fn new(config: PreviewConfig) -> Self {
        Self { config }
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
            Err(PreviewError::InvalidUtf8(_)) => {
                Ok(self.generate_binary_preview(path, &metadata))
            }
            Err(e) => Err(e),
        }
    }

    fn generate_text_preview(&self, path: &Path, _file_size: u64) -> Result<PreviewContent> {
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

        let (lines, truncated) = if total_lines > max_lines {
            (all_lines.into_iter().take(max_lines).collect(), true)
        } else {
            (all_lines, false)
        };

        Ok(PreviewContent::Text {
            lines,
            truncated,
            total_lines,
        })
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
            permissions: self.format_permissions(metadata),
            file_type: self.detect_file_type(path),
        };

        if self.is_image(path) {
            if let Some(image_meta) = self.extract_image_metadata(path, file_metadata.clone()) {
                return PreviewContent::Image {
                    metadata: image_meta,
                };
            }
        }

        PreviewContent::Binary {
            metadata: file_metadata,
        }
    }

    #[cfg(unix)]
    fn format_permissions(&self, metadata: &fs::Metadata) -> Option<String> {
        use std::os::unix::fs::PermissionsExt;
        let mode = metadata.permissions().mode();
        Some(format!(
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
        ))
    }

    #[cfg(not(unix))]
    fn format_permissions(&self, metadata: &fs::Metadata) -> Option<String> {
        if metadata.permissions().readonly() {
            Some("readonly".to_string())
        } else {
            Some("read-write".to_string())
        }
    }

    fn detect_file_type(&self, path: &Path) -> Option<String> {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_uppercase())
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

    fn extract_image_metadata(
        &self,
        _path: &Path,
        file_metadata: FileMetadata,
    ) -> Option<ImageMetadata> {
        // TODO: Use image crate to extract actual dimensions
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

        let config = PreviewConfig::default();
        let generator = PreviewGenerator::new(config);
        let preview = generator.generate(temp.path()).unwrap();

        match preview {
            PreviewContent::Text { lines, truncated, .. } => {
                assert_eq!(lines.len(), 3);
                assert_eq!(lines[0], "Line 1");
                assert!(!truncated);
            }
            _ => panic!("Expected Text preview"),
        }
    }

    #[test]
    fn test_generate_truncated_preview() {
        let temp = TempFile::create("test.txt").unwrap();
        let content = (0..100).map(|i| format!("Line {i}")).collect::<Vec<_>>().join("\n");
        fs::write(temp.path(), content).unwrap();

        let mut config = PreviewConfig::default();
        config.max_lines = 10;
        let generator = PreviewGenerator::new(config);
        let preview = generator.generate(temp.path()).unwrap();

        match preview {
            PreviewContent::Text {
                lines,
                truncated,
                total_lines,
            } => {
                assert_eq!(lines.len(), 10);
                assert!(truncated);
                assert_eq!(total_lines, 100);
            }
            _ => panic!("Expected Text preview"),
        }
    }

    #[test]
    fn test_generate_empty_file_preview() {
        let temp = TempFile::create("empty.txt").unwrap();

        let config = PreviewConfig::default();
        let generator = PreviewGenerator::new(config);
        let preview = generator.generate(temp.path()).unwrap();

        assert!(matches!(preview, PreviewContent::Empty));
    }

    #[test]
    fn test_generate_nonexistent_file() {
        let config = PreviewConfig::default();
        let generator = PreviewGenerator::new(config);
        let preview = generator.generate(Path::new("/nonexistent/file.txt")).unwrap();

        assert!(matches!(preview, PreviewContent::Error(_)));
    }
}
