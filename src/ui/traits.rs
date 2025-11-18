//! Core traits for UI abstraction layer

use super::error::Result;
use super::types::{DisplayItem, FinderResult, PreviewPosition};

/// Configuration for fuzzy finder
#[derive(Debug, Clone)]
pub struct FinderConfig {
    /// Items to display in the finder
    pub items: Vec<DisplayItem>,
    /// Enable multi-select mode
    pub multi_select: bool,
    /// Prompt text
    pub prompt: String,
    /// Enable ANSI color support
    pub ansi: bool,
    /// Preview configuration (None = no preview)
    pub preview_config: Option<PreviewConfig>,
    /// Custom keybinds (skim --bind format: "key:action")
    pub bind: Vec<String>,
}

impl FinderConfig {
    /// Create a basic finder configuration
    #[must_use]
    pub fn new(items: Vec<DisplayItem>, prompt: String) -> Self {
        Self {
            items,
            multi_select: false,
            prompt,
            ansi: false,
            preview_config: None,
            bind: Vec::new(),
        }
    }

    /// Enable multi-select
    #[must_use]
    pub const fn with_multi_select(mut self, multi: bool) -> Self {
        self.multi_select = multi;
        self
    }

    /// Enable ANSI colors
    #[must_use]
    pub const fn with_ansi(mut self, ansi: bool) -> Self {
        self.ansi = ansi;
        self
    }

    /// Set preview configuration
    #[must_use]
    pub fn with_preview(mut self, config: PreviewConfig) -> Self {
        self.preview_config = Some(config);
        self
    }

    /// Set custom keybinds
    #[must_use]
    pub fn with_binds(mut self, bind: Vec<String>) -> Self {
        self.bind = bind;
        self
    }
}

/// Configuration for preview pane
#[derive(Debug, Clone)]
pub struct PreviewConfig {
    /// Enable preview
    pub enabled: bool,
    /// Maximum file size to preview (bytes)
    pub max_file_size: u64,
    /// Maximum lines to display
    pub max_lines: usize,
    /// Enable syntax highlighting
    pub syntax_highlighting: bool,
    /// Show line numbers
    pub show_line_numbers: bool,
    /// Position of preview pane
    pub position: PreviewPosition,
    /// Width percentage (0-100)
    pub width_percent: u8,
}

impl Default for PreviewConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_file_size: 5_242_880, // 5MB
            max_lines: 50,
            syntax_highlighting: true,
            show_line_numbers: true,
            position: PreviewPosition::Right,
            width_percent: 50,
        }
    }
}

/// Trait for fuzzy finder implementations
///
/// This trait abstracts away the specific fuzzy finder backend,
/// allowing skim to be swapped out for a custom TUI implementation
/// or other backends in the future.
pub trait FuzzyFinder {
    /// Run the fuzzy finder with given configuration
    ///
    /// # Errors
    ///
    /// Returns an error if the finder cannot be initialized or
    /// if the operation fails.
    fn run(&self, config: FinderConfig) -> Result<FinderResult>;
}

/// Trait for preview providers
///
/// Implementations generate preview content for items.
/// This is backend-agnostic and can be reused across different
/// fuzzy finder implementations.
pub trait PreviewProvider: Send + Sync {
    /// Generate preview content for the given item
    ///
    /// # Arguments
    ///
    /// * `item` - The item key (e.g., file path)
    ///
    /// # Errors
    ///
    /// Returns an error if preview generation fails.
    fn preview(&self, item: &str) -> Result<PreviewText>;
}

/// Preview text with metadata about formatting
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewText {
    /// The preview content
    pub content: String,
    /// Whether the content contains ANSI escape codes
    pub has_ansi: bool,
}

impl PreviewText {
    /// Create preview text without ANSI codes
    #[must_use]
    pub fn plain(content: String) -> Self {
        Self {
            content,
            has_ansi: false,
        }
    }

    /// Create preview text with ANSI codes
    #[must_use]
    pub fn ansi(content: String) -> Self {
        Self {
            content,
            has_ansi: true,
        }
    }
}
