//! Core traits for UI abstraction layer

use super::error::Result;
use super::types::{DisplayItem, FinderResult, PreviewPosition};

/// Search criteria for refine search feature
#[derive(Debug, Clone, Default)]
pub struct RefineSearchCriteria {
    /// Tags to include in search
    pub include_tags: Vec<String>,
    /// Tags to exclude from search
    pub exclude_tags: Vec<String>,
    /// File patterns to match
    pub file_patterns: Vec<String>,
    /// Virtual tag patterns
    pub virtual_tags: Vec<String>,
}

impl RefineSearchCriteria {
    /// Create new search criteria
    #[must_use]
    pub const fn new(
        include_tags: Vec<String>,
        exclude_tags: Vec<String>,
        file_patterns: Vec<String>,
        virtual_tags: Vec<String>,
    ) -> Self {
        Self {
            include_tags,
            exclude_tags,
            file_patterns,
            virtual_tags,
        }
    }
}

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
    /// Custom keybinds ("key:action" format)
    pub bind: Vec<String>,
    /// Available tags from database (for refine search)
    pub available_tags: Vec<String>,
    /// Current search criteria for refine search
    pub search_criteria: Option<RefineSearchCriteria>,
    /// Tag schema for canonicalization (used for CLI preview)
    pub tag_schema: Option<std::sync::Arc<crate::schema::TagSchema>>,
    /// Database reference for live file count queries (used in tag selection phase)
    pub database: Option<std::sync::Arc<crate::db::Database>>,
}

impl FinderConfig {
    /// Create a basic finder configuration
    #[must_use]
    pub const fn new(items: Vec<DisplayItem>, prompt: String) -> Self {
        Self {
            items,
            multi_select: false,
            prompt,
            ansi: false,
            preview_config: None,
            bind: Vec::new(),
            available_tags: Vec::new(),
            search_criteria: None,
            tag_schema: None,
            database: None,
        }
    }

    /// Set available tags for refine search
    #[must_use]
    pub fn with_available_tags(mut self, tags: Vec<String>) -> Self {
        self.available_tags = tags;
        self
    }

    /// Set current search criteria for refine search
    #[must_use]
    pub fn with_search_criteria(mut self, criteria: RefineSearchCriteria) -> Self {
        self.search_criteria = Some(criteria);
        self
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
    pub const fn with_preview(mut self, config: PreviewConfig) -> Self {
        self.preview_config = Some(config);
        self
    }

    /// Set custom keybinds
    #[must_use]
    pub fn with_binds(mut self, bind: Vec<String>) -> Self {
        self.bind = bind;
        self
    }

    /// Set tag schema for canonicalization
    #[must_use]
    pub fn with_schema(mut self, schema: Option<std::sync::Arc<crate::schema::TagSchema>>) -> Self {
        self.tag_schema = schema;
        self
    }

    /// Set database for live file count queries
    #[must_use]
    pub fn with_database(mut self, db: Option<std::sync::Arc<crate::db::Database>>) -> Self {
        self.database = db;
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

impl From<crate::config::PreviewConfig> for PreviewConfig {
    fn from(cfg: crate::config::PreviewConfig) -> Self {
        Self {
            enabled: cfg.enabled,
            max_file_size: cfg.max_file_size,
            max_lines: cfg.max_lines,
            syntax_highlighting: cfg.syntax_highlighting,
            show_line_numbers: cfg.show_line_numbers,
            position: cfg.position,
            width_percent: cfg.width_percent,
        }
    }
}

/// Trait for fuzzy finder implementations
///
/// This trait abstracts away the specific fuzzy finder backend,
/// allowing different TUI implementations to be used without
/// changing business logic.
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
    pub const fn plain(content: String) -> Self {
        Self {
            content,
            has_ansi: false,
        }
    }

    /// Create preview text with ANSI codes
    #[must_use]
    pub const fn ansi(content: String) -> Self {
        Self {
            content,
            has_ansi: true,
        }
    }
}
