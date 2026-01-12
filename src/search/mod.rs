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
pub mod hierarchy;
pub mod traits;

pub use error::SearchError;
pub use traits::{AsFileTagPair, FileTagPair, FilterExt};

use crate::db::Database;
use crate::schema::{HIERARCHY_DELIMITER, TagSchema};
use std::collections::HashSet;

/// Expand a list of tags using the schema (aliases and hierarchies) and database (prefix matching)
///
/// For each input tag:
/// - Expands to all synonyms (canonical + all aliases)
/// - If `include_hierarchy` is true, also expands hierarchical tags to parent levels
/// - If tag doesn't exist but has children (e.g., "lang" → "lang:rust", "lang:python"), expands to all children
///
/// # Examples
/// ```ignore
/// let schema = TagSchema::new();
/// // Assuming js -> javascript alias
/// let expanded = expand_tags(&["js"], &schema, &db, true);
/// // Returns: ["javascript", "js", "es"] (all synonyms)
///
/// // Prefix matching for hierarchical tags
/// let expanded = expand_tags(&["lang"], &schema, &db, true);
/// // Returns: ["lang:rust", "lang:python", "lang:typescript", ...] (all lang:* children)
/// ```
///
/// # Errors
/// Returns error if database operations fail
pub fn expand_tags(
    tags: &[String],
    schema: &TagSchema,
    db: &Database,
    include_hierarchy: bool,
) -> Result<Vec<String>, crate::db::DbError> {
    let mut expanded = HashSet::new();

    // Get all available tags from database for prefix matching
    let all_tags = db.list_all_tags()?;
    let all_tags_set: HashSet<_> = all_tags.iter().map(String::as_str).collect();

    for tag in tags {
        if include_hierarchy {
            // Expand with hierarchy + synonyms
            let hierarchy_expanded = schema.expand_with_hierarchy(tag);

            // Check if any of the expanded tags actually exist in the database
            let has_real_tag = hierarchy_expanded
                .iter()
                .any(|t| all_tags_set.contains(t.as_str()));

            if has_real_tag {
                // Use the hierarchy expansion
                expanded.extend(hierarchy_expanded);
            } else {
                // Tag doesn't exist - check for hierarchical children (prefix match)
                let canonical = schema.canonicalize(tag);
                let prefix = format!("{canonical}{HIERARCHY_DELIMITER}");

                // Find all tags starting with "tag:"
                let children: Vec<_> = all_tags
                    .iter()
                    .filter(|t| t.starts_with(&prefix))
                    .cloned()
                    .collect();

                if children.is_empty() {
                    // No children found - keep original tag (will result in no matches)
                    expanded.insert(tag.clone());
                } else {
                    expanded.extend(children);
                }
            }
        } else {
            // Just expand synonyms
            for synonym in schema.expand_synonyms(tag) {
                expanded.insert(synonym);
            }
        }
    }

    Ok(expanded.into_iter().collect())
}
