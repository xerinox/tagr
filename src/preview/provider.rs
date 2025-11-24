//! File preview provider implementation

use super::generator::PreviewGenerator;
use super::types::PreviewContent;
use crate::ui::{PreviewConfig, PreviewProvider, PreviewText, Result};
use moka::sync::Cache;
use std::path::PathBuf;
use std::time::Duration;

/// File preview provider with caching
///
/// This provider generates file previews and caches them to improve
/// performance during fuzzy finding sessions.
pub struct FilePreviewProvider {
    generator: PreviewGenerator,
    cache: Cache<PathBuf, PreviewContent>,
}

impl FilePreviewProvider {
    /// Create a new file preview provider
    ///
    /// # Arguments
    ///
    /// * `config` - Preview configuration
    #[must_use]
    pub fn new(config: PreviewConfig) -> Self {
        Self::with_cache_config(config, Duration::from_secs(300), 1000)
    }

    /// Create a new file preview provider with custom cache configuration
    ///
    /// # Arguments
    ///
    /// * `config` - Preview configuration
    /// * `ttl` - Time-to-live for cache entries
    /// * `max_capacity` - Maximum number of cached previews
    #[must_use]
    pub fn with_cache_config(config: PreviewConfig, ttl: Duration, max_capacity: usize) -> Self {
        let cache = Cache::builder()
            .time_to_live(ttl)
            .max_capacity(max_capacity as u64)
            .build();

        Self {
            generator: PreviewGenerator::new(config),
            cache,
        }
    }

    /// Clear the preview cache
    pub fn clear_cache(&self) {
        self.cache.invalidate_all();
    }

    /// Get cache statistics
    #[must_use]
    pub fn cache_stats(&self) -> (u64, u64) {
        (self.cache.entry_count(), self.cache.weighted_size())
    }
}

impl PreviewProvider for FilePreviewProvider {
    fn preview(&self, item: &str) -> Result<PreviewText> {
        let path = PathBuf::from(item);

        if let Some(content) = self.cache.get(&path) {
            let display = content.to_display_string();
            let has_ansi = matches!(content, PreviewContent::Text { has_ansi: true, .. });
            return Ok(if has_ansi {
                PreviewText::ansi(display)
            } else {
                PreviewText::plain(display)
            });
        }

        // Generate preview, converting PreviewError to string
        let content = match self.generator.generate(&path) {
            Ok(c) => c,
            Err(e) => {
                // Return error as displayable content instead of propagating
                return Ok(PreviewText::plain(format!("Preview error: {e}")));
            }
        };
        
        // Cache the result
        self.cache.insert(path, content.clone());

        let display = content.to_display_string();
        let has_ansi = matches!(content, PreviewContent::Text { has_ansi: true, .. });
        Ok(if has_ansi {
            PreviewText::ansi(display)
        } else {
            PreviewText::plain(display)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempFile;
    use std::fs;

    #[test]
    fn test_file_preview_provider_text() {
        let temp = TempFile::create("test.txt").unwrap();
        fs::write(temp.path(), "Line 1\nLine 2\nLine 3\n").unwrap();

        let config = PreviewConfig::default();
        let provider = FilePreviewProvider::new(config);

        let preview = provider.preview(temp.path().to_str().unwrap()).unwrap();
        assert!(preview.content.contains("Line 1"));
        assert!(preview.content.contains("Line 2"));
        assert!(preview.content.contains("Line 3"));
    }

    #[test]
    fn test_file_preview_provider_caching() {
        let temp = TempFile::create("test.txt").unwrap();
        fs::write(temp.path(), "Test content\n").unwrap();

        let config = PreviewConfig::default();
        let provider = FilePreviewProvider::new(config);

        let preview1 = provider.preview(temp.path().to_str().unwrap()).unwrap();
        
        fs::write(temp.path(), "Modified content\n").unwrap();
        
        let preview2 = provider.preview(temp.path().to_str().unwrap()).unwrap();
        
        assert_eq!(preview1, preview2);
        assert!(preview1.content.contains("Test content"));
        assert!(!preview2.content.contains("Modified content"));
    }

    #[test]
    fn test_file_preview_provider_clear_cache() {
        let temp = TempFile::create("test.txt").unwrap();
        fs::write(temp.path(), "Test content\n").unwrap();

        let config = PreviewConfig::default();
        let provider = FilePreviewProvider::new(config);

        // Cache a preview
        let preview1 = provider.preview(temp.path().to_str().unwrap()).unwrap();
        assert!(preview1.content.contains("Test content"));

        // Modify the file
        fs::write(temp.path(), "Modified content\n").unwrap();
        
        // Clear cache
        provider.clear_cache();
        
        // Should now see the modified content
        let preview2 = provider.preview(temp.path().to_str().unwrap()).unwrap();
        assert!(preview2.content.contains("Modified content"));
        assert!(!preview2.content.contains("Test content"));
    }

    #[test]
    fn test_file_preview_provider_nonexistent() {
        let config = PreviewConfig::default();
        let provider = FilePreviewProvider::new(config);

        let preview = provider.preview("/nonexistent/file.txt").unwrap();
        assert!(preview.content.contains("File not found"));
    }

    #[test]
    fn test_file_preview_provider_empty_file() {
        let temp = TempFile::create("empty.txt").unwrap();
        fs::write(temp.path(), "").unwrap();

        let config = PreviewConfig::default();
        let provider = FilePreviewProvider::new(config);

        let preview = provider.preview(temp.path().to_str().unwrap()).unwrap();
        assert!(preview.content.contains("Empty file"));
    }
}
