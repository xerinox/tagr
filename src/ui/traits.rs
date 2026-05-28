//! Core traits for UI abstraction layer

use super::error::Result;
use super::types::{DisplayItem, FinderResult, RefinedSearchCriteria};
use crate::config::PreviewConfig;

/// Configuration for fuzzy finder
#[derive(Clone)]
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
    pub search_criteria: Option<RefinedSearchCriteria>,
    /// Tag schema for canonicalization (used for CLI preview)
    pub tag_schema: Option<std::sync::Arc<crate::schema::TagSchema>>,
    /// Database reference for live file count queries (used in tag selection phase)
    pub database: Option<std::sync::Arc<dyn crate::store::TagStore>>,
}

impl std::fmt::Debug for FinderConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FinderConfig")
            .field("items", &self.items)
            .field("multi_select", &self.multi_select)
            .field("prompt", &self.prompt)
            .field("ansi", &self.ansi)
            .field("preview_config", &self.preview_config)
            .field("bind", &self.bind)
            .field("available_tags", &self.available_tags)
            .field("search_criteria", &self.search_criteria)
            .field("tag_schema", &self.tag_schema)
            .field("database", &self.database.as_ref().map(|_| "..."))
            .finish()
    }
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
    pub fn with_search_criteria(mut self, criteria: RefinedSearchCriteria) -> Self {
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
    pub fn with_database(mut self, db: Option<std::sync::Arc<dyn crate::store::TagStore>>) -> Self {
        self.database = db;
        self
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
