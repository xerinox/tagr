//! Trait-based filtering abstraction for unified tag filtering
//!
//! This module provides a trait-based abstraction layer that allows filtering
//! any collection of file-tag pairs using the same logic. This enables:
//!
//! - Database types (`Pair`) and UI types (`TagrItem`) to use identical filtering
//! - In-memory filtering without database round-trips
//! - Third-party types to leverage tagr's filtering logic
//! - Zero-cost abstraction via borrows
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────┐
//! │  AsFileTagPair Trait                │  ← Adaptation layer
//! │  - as_pair() -> (&str, &[String])   │
//! └─────────────────────────────────────┘
//!            ▲
//!            │ implements
//!            │
//!    ┌───────┴────────┬─────────────┐
//!    │                │             │
//!  Pair          TagrItem      CustomType
//!
//! ┌─────────────────────────────────────┐
//! │  FilterExt<T: AsFileTagPair>        │  ← Filtering logic
//! │  - apply_filter(&SearchParams)      │
//! └─────────────────────────────────────┘
//! ```
//!
//! # Examples
//!
//! ```ignore
//! use tagr::search::traits::{AsFileTagPair, FilterExt};
//! use tagr::cli::SearchParams;
//!
//! // Works with any type implementing AsFileTagPair
//! let filtered: Vec<_> = items
//!     .iter()
//!     .apply_filter(&params)
//!     .collect();
//! ```

use crate::cli::SearchParams;
use crate::search::hierarchy;

/// Represents a file-tag pair as borrowed data
///
/// This is the core DTO (Data Transfer Object) for filtering operations.
/// It provides a zero-cost view of file path and tags without ownership.
#[derive(Debug, Clone, Copy)]
pub struct FileTagPair<'a> {
    /// File path as string slice
    pub file: &'a str,
    /// Tags associated with the file
    pub tags: &'a [String],
}

impl<'a> FileTagPair<'a> {
    /// Create a new file-tag pair
    #[must_use]
    pub const fn new(file: &'a str, tags: &'a [String]) -> Self {
        Self { file, tags }
    }
}

/// Trait for types that can be viewed as file-tag pairs
///
/// Types implement this trait to provide a borrowed view of their
/// file path and tags without allocating or copying data.
///
/// # Examples
///
/// ```ignore
/// impl AsFileTagPair for MyType {
///     fn as_pair(&self) -> FileTagPair<'_> {
///         FileTagPair::new(&self.path, &self.tags)
///     }
/// }
/// ```
pub trait AsFileTagPair {
    /// Return a borrowed view of this item as a file-tag pair
    fn as_pair(&self) -> FileTagPair<'_>;
}

/// Extension trait for filtering collections of file-tag pairs
///
/// This trait provides unified filtering logic that works with any type
/// implementing `AsFileTagPair`. It applies hierarchical tag matching,
/// specificity rules, and all other search criteria.
///
/// # Examples
///
/// ```ignore
/// use tagr::search::traits::FilterExt;
///
/// let params = SearchParams {
///     tags: vec!["lang:rust".to_string()],
///     exclude_tags: vec!["tests".to_string()],
///     ..Default::default()
/// };
///
/// let results: Vec<_> = items
///     .iter()
///     .apply_filter(&params)
///     .collect();
/// ```
pub trait FilterExt<T: AsFileTagPair> {
    /// Filter items based on search parameters
    ///
    /// Applies hierarchical tag matching with specificity rules:
    /// - Prefix matching: `lang` matches `lang:rust`, `lang:python`
    /// - Depth-based specificity within same hierarchy
    /// - Cross-hierarchy excludes always win
    ///
    /// # Arguments
    /// * `params` - Search parameters containing tag filters
    ///
    /// # Returns
    /// Iterator over items that match the search criteria
    fn apply_filter<'a>(&'a self, params: &'a SearchParams) -> impl Iterator<Item = &'a T> + 'a
    where
        T: 'a;
}

impl<T: AsFileTagPair> FilterExt<T> for [T] {
    fn apply_filter<'a>(&'a self, params: &'a SearchParams) -> impl Iterator<Item = &'a T> + 'a
    where
        T: 'a,
    {
        self.iter().filter(move |item| {
            let pair = item.as_pair();

            // Apply hierarchical tag filtering if we have tag criteria
            if !params.tags.is_empty() || !params.exclude_tags.is_empty() {
                if params.no_hierarchy {
                    // Traditional exact matching
                    if !params.tags.is_empty() {
                        let has_match = match params.tag_mode {
                            crate::cli::SearchMode::All => {
                                params.tags.iter().all(|t| pair.tags.contains(t))
                            }
                            crate::cli::SearchMode::Any => {
                                params.tags.iter().any(|t| pair.tags.contains(t))
                            }
                        };
                        if !has_match {
                            return false;
                        }
                    }

                    // Exact exclude matching
                    if !params.exclude_tags.is_empty() {
                        let has_excluded =
                            params.exclude_tags.iter().any(|t| pair.tags.contains(t));
                        if has_excluded {
                            return false;
                        }
                    }
                } else {
                    // Hierarchical matching with specificity
                    if !params.tags.is_empty() {
                        // Check if file has tags matching the patterns (ALL or ANY mode)
                        let matches = match params.tag_mode {
                            crate::cli::SearchMode::All => {
                                // File must have tags matching ALL patterns
                                params.tags.iter().all(|pattern| {
                                    pair.tags
                                        .iter()
                                        .any(|tag| hierarchy::pattern_matches(pattern, tag))
                                })
                            }
                            crate::cli::SearchMode::Any => {
                                // File must have tags matching ANY pattern
                                params.tags.iter().any(|pattern| {
                                    pair.tags
                                        .iter()
                                        .any(|tag| hierarchy::pattern_matches(pattern, tag))
                                })
                            }
                        };

                        if !matches {
                            return false;
                        }
                    }

                    // Apply hierarchical exclusion rules
                    let should_include = hierarchy::should_include_file(
                        pair.tags,
                        &params.tags,
                        &params.exclude_tags,
                    );

                    if !should_include {
                        return false;
                    }
                }
            }

            true
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{SearchMode, SearchParams};

    // Mock type for testing
    #[derive(Debug)]
    struct MockFile {
        path: String,
        tags: Vec<String>,
    }

    impl MockFile {
        fn new(path: &str, tags: Vec<&str>) -> Self {
            Self {
                path: path.to_string(),
                tags: tags.into_iter().map(String::from).collect(),
            }
        }
    }

    impl AsFileTagPair for MockFile {
        fn as_pair(&self) -> FileTagPair<'_> {
            FileTagPair::new(&self.path, &self.tags)
        }
    }

    #[test]
    fn test_file_tag_pair_creation() {
        let tags = vec!["rust".to_string(), "web".to_string()];
        let pair = FileTagPair::new("test.rs", &tags);
        assert_eq!(pair.file, "test.rs");
        assert_eq!(pair.tags.len(), 2);
    }

    #[test]
    fn test_as_pair_trait() {
        let mock = MockFile::new("test.rs", vec!["rust", "web"]);
        let pair = mock.as_pair();
        assert_eq!(pair.file, "test.rs");
        assert_eq!(pair.tags, &["rust", "web"]);
    }

    #[test]
    fn test_filter_ext_simple_match() {
        let files = vec![
            MockFile::new("file1.rs", vec!["rust", "backend"]),
            MockFile::new("file2.js", vec!["javascript", "frontend"]),
            MockFile::new("file3.rs", vec!["rust", "tests"]),
        ];

        let params = SearchParams {
            query: None,
            tags: vec!["rust".to_string()],
            tag_mode: SearchMode::Any,
            file_patterns: vec![],
            file_mode: SearchMode::All,
            exclude_tags: vec![],
            regex_tag: false,
            regex_file: false,
            glob_files: false,
            virtual_tags: vec![],
            virtual_mode: SearchMode::All,
            no_hierarchy: true, // Exact matching
        };

        let results: Vec<_> = files.apply_filter(&params).collect();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].path, "file1.rs");
        assert_eq!(results[1].path, "file3.rs");
    }

    #[test]
    fn test_filter_ext_exclude() {
        let files = vec![
            MockFile::new("file1.rs", vec!["rust", "backend"]),
            MockFile::new("file2.rs", vec!["rust", "tests"]),
        ];

        let params = SearchParams {
            query: None,
            tags: vec!["rust".to_string()],
            tag_mode: SearchMode::Any,
            file_patterns: vec![],
            file_mode: SearchMode::All,
            exclude_tags: vec!["tests".to_string()],
            regex_tag: false,
            regex_file: false,
            glob_files: false,
            virtual_tags: vec![],
            virtual_mode: SearchMode::All,
            no_hierarchy: true,
        };

        let results: Vec<_> = files.apply_filter(&params).collect();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, "file1.rs");
    }

    #[test]
    fn test_filter_ext_hierarchical_prefix() {
        let files = vec![
            MockFile::new("file1.rs", vec!["lang:rust"]),
            MockFile::new("file2.py", vec!["lang:python"]),
            MockFile::new("file3.js", vec!["javascript"]),
        ];

        let params = SearchParams {
            query: None,
            tags: vec!["lang".to_string()],
            tag_mode: SearchMode::Any,
            file_patterns: vec![],
            file_mode: SearchMode::All,
            exclude_tags: vec![],
            regex_tag: false,
            regex_file: false,
            glob_files: false,
            virtual_tags: vec![],
            virtual_mode: SearchMode::All,
            no_hierarchy: false, // Hierarchical matching
        };

        let results: Vec<_> = files.apply_filter(&params).collect();
        assert_eq!(results.len(), 2);
        assert!(results.iter().any(|f| f.path == "file1.rs"));
        assert!(results.iter().any(|f| f.path == "file2.py"));
    }

    #[test]
    fn test_filter_ext_hierarchical_specificity() {
        let files = vec![
            MockFile::new("file1.rs", vec!["lang:rust"]),
            MockFile::new("file2.py", vec!["lang:python"]),
        ];

        let params = SearchParams {
            query: None,
            tags: vec!["lang".to_string()],
            tag_mode: SearchMode::Any,
            file_patterns: vec![],
            file_mode: SearchMode::All,
            exclude_tags: vec!["lang:rust".to_string()],
            regex_tag: false,
            regex_file: false,
            glob_files: false,
            virtual_tags: vec![],
            virtual_mode: SearchMode::All,
            no_hierarchy: false,
        };

        let results: Vec<_> = files.apply_filter(&params).collect();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, "file2.py");
    }

    #[test]
    fn test_filter_ext_all_mode() {
        let files = vec![
            MockFile::new("file1.rs", vec!["lang:rust", "project:backend"]),
            MockFile::new("file2.rs", vec!["lang:rust"]),
            MockFile::new("file3.rs", vec!["project:backend"]),
        ];

        let params = SearchParams {
            query: None,
            tags: vec!["lang".to_string(), "project".to_string()],
            tag_mode: SearchMode::All,
            file_patterns: vec![],
            file_mode: SearchMode::All,
            exclude_tags: vec![],
            regex_tag: false,
            regex_file: false,
            glob_files: false,
            virtual_tags: vec![],
            virtual_mode: SearchMode::All,
            no_hierarchy: false,
        };

        let results: Vec<_> = files.apply_filter(&params).collect();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, "file1.rs");
    }

    #[test]
    fn test_filter_ext_no_criteria() {
        let files = vec![
            MockFile::new("file1.rs", vec!["rust"]),
            MockFile::new("file2.js", vec!["javascript"]),
        ];

        let params = SearchParams::default();

        let results: Vec<_> = files.apply_filter(&params).collect();
        assert_eq!(results.len(), 2); // All files pass with no criteria
    }
}
