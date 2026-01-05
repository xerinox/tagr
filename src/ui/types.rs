//! Common types for UI abstraction layer

use serde::{Deserialize, Serialize};
use std::fmt;

/// Current phase of the browse workflow
///
/// Used to filter which keybinds are shown in help overlays
/// and which actions are available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BrowsePhase {
    /// Tag selection phase - limited actions (navigation only)
    TagSelection,
    /// File selection phase - full actions available
    #[default]
    FileSelection,
}

/// Position of preview pane
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum PreviewPosition {
    /// Preview on the right side
    #[default]
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
    pub const fn with_metadata(
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
    /// Selected items (keys from `DisplayItem`)
    pub selected: Vec<String>,
    /// Whether the operation was aborted by user
    pub aborted: bool,
    /// The final key pressed (for keybind detection)
    pub final_key: Option<String>,
    /// Refined search criteria (if `refine_search` was triggered)
    pub refine_search: Option<RefinedSearchCriteria>,
    /// Input action that was submitted (action_id, values)
    pub input_action: Option<InputAction>,
}

/// Input action submitted from modal text input
#[derive(Debug, Clone)]
pub struct InputAction {
    /// The action identifier (e.g., "add_tag", "remove_tag")
    pub action_id: String,
    /// The values entered by the user
    pub values: Vec<String>,
}

/// Refined search criteria from refine search overlay
#[derive(Debug, Clone, Default)]
pub struct RefinedSearchCriteria {
    /// Tags to include in search
    pub include_tags: Vec<String>,
    /// Tags to exclude from search
    pub exclude_tags: Vec<String>,
    /// File patterns to match
    pub file_patterns: Vec<String>,
    /// Virtual tag patterns
    pub virtual_tags: Vec<String>,
}

impl FinderResult {
    /// Create result with selections
    #[must_use]
    pub const fn selected(items: Vec<String>) -> Self {
        Self {
            selected: items,
            aborted: false,
            final_key: None,
            refine_search: None,
            input_action: None,
        }
    }

    /// Create result for aborted operation
    #[must_use]
    pub const fn aborted() -> Self {
        Self {
            selected: Vec::new(),
            aborted: true,
            final_key: None,
            refine_search: None,
            input_action: None,
        }
    }

    /// Create result with final key information
    #[must_use]
    pub const fn with_key(items: Vec<String>, key: Option<String>) -> Self {
        Self {
            selected: items,
            aborted: false,
            final_key: key,
            refine_search: None,
            input_action: None,
        }
    }

    /// Create result with refined search criteria
    #[must_use]
    pub fn with_refine_search(
        include_tags: Vec<String>,
        exclude_tags: Vec<String>,
        file_patterns: Vec<String>,
        virtual_tags: Vec<String>,
    ) -> Self {
        Self {
            selected: Vec::new(),
            aborted: false,
            final_key: Some("refine_search_done".to_string()),
            refine_search: Some(RefinedSearchCriteria {
                include_tags,
                exclude_tags,
                file_patterns,
                virtual_tags,
            }),
            input_action: None,
        }
    }

    /// Create result with input action from modal text input
    #[must_use]
    pub fn with_action(items: Vec<String>, action_id: String, values: Vec<String>) -> Self {
        Self {
            selected: items,
            aborted: false,
            final_key: Some(action_id.clone()),
            refine_search: None,
            input_action: Some(InputAction { action_id, values }),
        }
    }

    /// Check if this result contains refined search criteria
    #[must_use]
    pub const fn has_refine_search(&self) -> bool {
        self.refine_search.is_some()
    }

    /// Check if this result contains an input action
    #[must_use]
    pub const fn has_input_action(&self) -> bool {
        self.input_action.is_some()
    }
}
