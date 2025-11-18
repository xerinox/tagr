//! Common types for UI abstraction layer

use serde::{Deserialize, Serialize};
use std::fmt;

/// Position of preview pane
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PreviewPosition {
    /// Preview on the right side
    Right,
    /// Preview at the bottom
    Bottom,
    /// Preview at the top
    Top,
}

impl PreviewPosition {
    /// Convert to string representation for UI backends
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Right => "right",
            Self::Bottom => "down",
            Self::Top => "up",
        }
    }
}

impl Default for PreviewPosition {
    fn default() -> Self {
        Self::Right
    }
}

impl fmt::Display for PreviewPosition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Item to display in the fuzzy finder
#[derive(Debug, Clone)]
pub struct DisplayItem {
    /// Unique identifier (e.g., file path)
    pub key: String,
    /// What the user sees (formatted, may include ANSI colors)
    pub display: String,
    /// Text to search against (should not include formatting codes)
    pub searchable: String,
    /// Additional metadata
    pub metadata: ItemMetadata,
}

impl DisplayItem {
    /// Create a new display item
    #[must_use]
    pub fn new(key: String, display: String, searchable: String) -> Self {
        Self {
            key,
            display,
            searchable,
            metadata: ItemMetadata::default(),
        }
    }

    /// Create a display item with metadata
    #[must_use]
    pub fn with_metadata(
        key: String,
        display: String,
        searchable: String,
        metadata: ItemMetadata,
    ) -> Self {
        Self {
            key,
            display,
            searchable,
            metadata,
        }
    }
}

/// Metadata for display items
#[derive(Debug, Clone, Default)]
pub struct ItemMetadata {
    /// Tags associated with this item
    pub tags: Vec<String>,
    /// Whether the item exists (e.g., file exists on disk)
    pub exists: bool,
    /// Optional index for ordering
    pub index: Option<usize>,
}

/// Result from fuzzy finder
#[derive(Debug)]
pub struct FinderResult {
    /// Selected items (keys from DisplayItem)
    pub selected: Vec<String>,
    /// Whether the operation was aborted by user
    pub aborted: bool,
    /// The final key pressed (for keybind detection)
    pub final_key: Option<String>,
}

impl FinderResult {
    /// Create result with selections
    #[must_use]
    pub fn selected(items: Vec<String>) -> Self {
        Self {
            selected: items,
            aborted: false,
            final_key: None,
        }
    }

    /// Create result for aborted operation
    #[must_use]
    pub fn aborted() -> Self {
        Self {
            selected: Vec::new(),
            aborted: true,
            final_key: None,
        }
    }

    /// Create result with final key information
    #[must_use]
    pub fn with_key(items: Vec<String>, key: Option<String>) -> Self {
        Self {
            selected: items,
            aborted: false,
            final_key: key,
        }
    }
}
