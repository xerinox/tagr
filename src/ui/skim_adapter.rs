//! Skim fuzzy finder adapter
//!
//! This module provides an implementation of the `FuzzyFinder` trait
//! using skim as the backend. It handles converting between our
//! abstract types and skim-specific types.

use super::error::{Result, UiError};
use super::traits::{FinderConfig, FuzzyFinder, PreviewProvider, PreviewText};
use super::types::{DisplayItem, FinderResult};
use crate::preview::PreviewGenerator;
use skim::prelude::*;
use std::borrow::Cow;
use std::io::Cursor;
use std::sync::Arc;

/// Skim-based fuzzy finder implementation
pub struct SkimFinder {
    preview_provider: Option<Arc<dyn PreviewProvider>>,
}

impl SkimFinder {
    /// Create a new skim finder
    #[must_use]
    pub fn new() -> Self {
        Self {
            preview_provider: None,
        }
    }

    /// Create a skim finder with preview provider
    #[must_use]
    pub fn with_preview_provider(preview_provider: impl PreviewProvider + 'static) -> Self {
        Self {
            preview_provider: Some(Arc::new(preview_provider)),
        }
    }

    /// Build skim options from finder configuration
    fn build_skim_options(&self, config: &FinderConfig) -> Result<SkimOptions> {
        let mut builder = SkimOptionsBuilder::default();

        builder
            .multi(config.multi_select)
            .prompt(config.prompt.clone())
            .reverse(true);

        if config.ansi {
            builder.ansi(true).color(Some("dark".to_string()));
        }

        // Preview configuration
        if let Some(preview_config) = &config.preview_config
            && preview_config.enabled
            && self.preview_provider.is_some()
        {
            // Skim requires a preview command to enable preview window
            // Use empty string to signal we're using ItemPreview trait
            builder.preview(Some(String::new()));

            let preview_window = format!(
                "{}:{}%",
                preview_config.position.as_str(),
                preview_config.width_percent
            );
            builder.preview_window(preview_window);
        }

        builder
            .build()
            .map_err(|e| UiError::BuildError(format!("Failed to build skim options: {e}")))
    }

    /// Convert display items to skim items
    fn convert_to_skim_items(
        items: Vec<DisplayItem>,
        preview_provider: Option<&Arc<dyn PreviewProvider>>,
    ) -> SkimItemReceiver {
        let (tx, rx): (SkimItemSender, SkimItemReceiver) = unbounded();

        for item in items {
            let skim_item = Arc::new(SkimDisplayItem::new(item, preview_provider.cloned()));
            let _ = tx.send(skim_item);
        }
        drop(tx);

        rx
    }
}

impl Default for SkimFinder {
    fn default() -> Self {
        Self::new()
    }
}

impl FuzzyFinder for SkimFinder {
    fn run(&self, config: FinderConfig) -> Result<FinderResult> {
        let options = self.build_skim_options(&config)?;
        let preview_provider = self.preview_provider.clone();
        let rx = Self::convert_to_skim_items(config.items, preview_provider.as_ref());

        let output = Skim::run_with(&options, Some(rx)).ok_or(UiError::InterruptedError)?;

        if output.is_abort {
            return Ok(FinderResult::aborted());
        }

        let selected: Vec<String> = output
            .selected_items
            .iter()
            .map(|item| item.output().to_string())
            .collect();

        Ok(FinderResult::selected(selected))
    }
}

/// Wrapper around `DisplayItem` that implements `SkimItem`
struct SkimDisplayItem {
    item: DisplayItem,
    preview_provider: Option<Arc<dyn PreviewProvider>>,
}

impl SkimDisplayItem {
    fn new(item: DisplayItem, preview_provider: Option<Arc<dyn PreviewProvider>>) -> Self {
        Self {
            item,
            preview_provider,
        }
    }
}

impl SkimItem for SkimDisplayItem {
    fn text(&self) -> Cow<'_, str> {
        // Use searchable text for fuzzy matching
        Cow::Borrowed(&self.item.searchable)
    }

    fn display<'a>(&'a self, _context: DisplayContext<'a>) -> AnsiString<'a> {
        // Use display string (may contain ANSI codes)
        AnsiString::parse(&self.item.display)
    }

    fn output(&self) -> Cow<'_, str> {
        // Return the key for selection
        Cow::Borrowed(&self.item.key)
    }

    fn get_index(&self) -> usize {
        // Use metadata index if available
        self.item.metadata.index.unwrap_or(0)
    }

    fn preview(&self, _context: PreviewContext) -> ItemPreview {
        // If we have a preview provider, use it to generate preview
        self.preview_provider.as_ref().map_or_else(
            || ItemPreview::Text(String::new()),
            |provider| match provider.preview(&self.item.key) {
                Ok(preview_text) => {
                    if preview_text.has_ansi {
                        ItemPreview::AnsiText(preview_text.content)
                    } else {
                        ItemPreview::Text(preview_text.content)
                    }
                }
                Err(_) => {
                    ItemPreview::Text(format!("Error generating preview for {}", self.item.key))
                }
            },
        )
    }
}

/// Simple preview provider for skim
pub struct SkimPreviewProvider {
    generator: Arc<PreviewGenerator>,
}

impl SkimPreviewProvider {
    /// Create a new skim preview provider
    #[must_use]
    pub const fn new(generator: Arc<PreviewGenerator>) -> Self {
        Self { generator }
    }
}

impl PreviewProvider for SkimPreviewProvider {
    fn preview(&self, item: &str) -> Result<PreviewText> {
        use crate::preview::PreviewContent;
        use std::path::PathBuf;

        let path = PathBuf::from(item);
        match self.generator.generate(&path) {
            Ok(content) => {
                let display = content.to_display_string();
                let has_ansi = matches!(content, PreviewContent::Text { has_ansi: true, .. });
                Ok(if has_ansi {
                    PreviewText::ansi(display)
                } else {
                    PreviewText::plain(display)
                })
            }
            Err(e) => Ok(PreviewText::plain(format!("Preview error: {e}"))),
        }
    }
}

/// Alternative: Run skim with simple string items (for backwards compatibility)
/// Simple wrapper around skim for backwards compatibility
/// Returns the selected items or empty vec if aborted
///
/// # Errors
///
/// Returns `UiError::BuildError` if skim options cannot be built.
/// Returns `UiError::ExecutionError` if skim fails to run.
pub fn run_skim_simple(items: &[String], multi: bool, prompt: &str) -> Result<FinderResult> {
    let items_text = items.join("\n");
    let item_reader = SkimItemReader::default();
    let items = item_reader.of_bufread(Cursor::new(items_text));

    let options = SkimOptionsBuilder::default()
        .multi(multi)
        .prompt(prompt.to_string())
        .reverse(true)
        .build()
        .map_err(|e| UiError::BuildError(format!("Failed to build skim options: {e}")))?;

    let output = Skim::run_with(&options, Some(items)).ok_or(UiError::InterruptedError)?;

    if output.is_abort {
        return Ok(FinderResult::aborted());
    }

    let selected: Vec<String> = output
        .selected_items
        .iter()
        .map(|item| item.output().to_string())
        .collect();

    Ok(FinderResult::selected(selected))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{ItemMetadata, PreviewConfig};

    #[test]
    fn test_skim_finder_creation() {
        let finder = SkimFinder::new();
        assert!(finder.preview_provider.is_none());
    }

    #[test]
    fn test_skim_finder_with_preview() {
        let config = PreviewConfig::default();
        let generator = PreviewGenerator::new(config);
        let provider = SkimPreviewProvider::new(Arc::new(generator));
        let finder = SkimFinder::with_preview_provider(provider);
        assert!(finder.preview_provider.is_some());
    }

    #[test]
    fn test_display_item_conversion() {
        let item = DisplayItem::with_metadata(
            "/path/to/file.txt".to_string(),
            "file.txt [tag1, tag2]".to_string(),
            "file.txt".to_string(),
            ItemMetadata {
                tags: vec!["tag1".to_string(), "tag2".to_string()],
                exists: true,
                index: Some(42),
            },
        );

        let skim_item = SkimDisplayItem::new(item, None);
        assert_eq!(skim_item.text(), "file.txt");
        assert_eq!(skim_item.output(), "/path/to/file.txt");
        assert_eq!(skim_item.get_index(), 42);
    }
}
