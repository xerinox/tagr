//! Interactive search module using fuzzy finder
//!
//! Provides an interactive browse mode that allows users to:
//! 1. Select tags using fuzzy finder (multi-select supported)
//! 2. View and select files matching those tags (multi-select supported)
//!
//! ## Usage
//!
//! The recommended API uses the unified browser pattern:
//!
//! ```no_run
//! use tagr::browse::{BrowseSession, BrowseController, BrowseConfig};
//! use tagr::ui::ratatui_adapter::RatatuiFinder;
//! # use tagr::db::Database;
//!
//! # fn example(db: &Database) -> Result<(), Box<dyn std::error::Error>> {
//! let config = BrowseConfig::default();
//! let session = BrowseSession::new(db, config)?;
//!
//! let finder = RatatuiFinder::new();
//!
//! let controller = BrowseController::new(session, finder);
//! controller.run()?;
//! # Ok(())
//! # }
//! ```

pub mod error;
pub mod filter;

pub use error::SearchError;

use crate::schema::TagSchema;
use std::collections::HashSet;

/// Expand a list of tags using the schema (aliases and hierarchies)
///
/// For each input tag:
/// - Expands to all synonyms (canonical + all aliases)
/// - If `include_hierarchy` is true, also expands hierarchical tags to parent levels
///
/// # Examples
/// ```ignore
/// let schema = TagSchema::new();
/// // Assuming js -> javascript alias
/// let expanded = expand_tags(&["js"], &schema, true);
/// // Returns: ["javascript", "js", "es"] (all synonyms)
/// ```
#[must_use]
pub fn expand_tags(tags: &[String], schema: &TagSchema, include_hierarchy: bool) -> Vec<String> {
    let mut expanded = HashSet::new();

    for tag in tags {
        if include_hierarchy {
            // Expand with hierarchy + synonyms
            for expanded_tag in schema.expand_with_hierarchy(tag) {
                expanded.insert(expanded_tag);
            }
        } else {
            // Just expand synonyms
            for synonym in schema.expand_synonyms(tag) {
                expanded.insert(synonym);
            }
        }
    }

    expanded.into_iter().collect()
}
