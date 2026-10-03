//! `Pair` — file + tags association (DTO).

use serde::{Deserialize, Serialize};

use super::tag_name::TagName;
use super::tagr_path::TagrPath;

/// A file-tags association.
///
/// This is the core data transfer object: a file path paired with its tags.
/// Used in query results, wire protocol messages, and store operations.
///
/// # Examples
///
/// ```
/// use tagr::types::{Pair, TagName, TagrPath};
///
/// let pair = Pair::new(
///     TagrPath::new("src/main.rs").unwrap(),
///     vec![TagName::new("rust").unwrap(), TagName::new("cli").unwrap()],
/// );
/// assert_eq!(pair.file.as_str(), "src/main.rs");
/// assert_eq!(pair.tags.len(), 2);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pair {
    /// The file path.
    pub file: TagrPath,
    /// Tags associated with this file.
    pub tags: Vec<TagName>,
}

impl Pair {
    /// Create a new `Pair`.
    #[must_use]
    pub const fn new(file: TagrPath, tags: Vec<TagName>) -> Self {
        Self { file, tags }
    }

    /// Create a `Pair` from a path and raw tag strings (test convenience).
    ///
    /// # Panics
    ///
    /// Panics if any tag string is invalid.
    #[cfg(test)]
    pub(crate) fn from_raw(file: &std::path::Path, tags: Vec<&str>) -> Self {
        Self {
            file: TagrPath::new(file).expect("valid UTF-8 path"),
            tags: tags
                .into_iter()
                .map(|s| TagName::new(s).expect("valid tag name"))
                .collect(),
        }
    }
}
