//! File filtering operations used by search and browse
//!
//! This module provides unified filtering functionality for both glob patterns
//! and regex matching, used by both database queries and interactive search.
//!
//! # Iterator Adapters
//!
//! This module exports two extension traits that add fluent filtering to iterators:
//!
//! - [`PathFilterExt`]: Adds pattern filtering (glob/regex) to `PathBuf` iterators
//! - [`PathTagFilterExt`]: Adds tag-based exclusion filtering to `Vec<PathBuf>`
//!
//! These traits enable method chaining for complex filtering operations:
//!
//! ```ignore
//! use tagr::search::filter::{PathFilterExt, PathTagFilterExt};
//!
//! let result = all_files
//!     .into_iter()
//!     .filter_glob_any(&["*.rs".to_string(), "*.toml".to_string()])?
//!     .exclude_tags(db, &["deprecated".to_string()])?;
//! ```

use crate::db::{Database, DbError};
use glob::Pattern as GlobPattern;
use regex::Regex;
use std::path::PathBuf;

/// Filter files by patterns (glob or regex) with AND/OR logic
///
/// # Arguments
/// * `files` - Iterator of file paths to filter
/// * `patterns` - Patterns to match against file paths
/// * `use_regex` - If true, treat patterns as regex; otherwise as globs
/// * `match_all` - If true, file must match ALL patterns (AND); otherwise ANY pattern (OR)
///
/// # Returns
/// Vector of file paths matching the criteria
///
/// # Errors
/// Returns `DbError::InvalidInput` if any pattern is invalid
pub fn by_patterns(
    files: impl IntoIterator<Item = PathBuf>,
    patterns: &[String],
    use_regex: bool,
    match_all: bool,
) -> Result<Vec<PathBuf>, DbError> {
    if patterns.is_empty() {
        return Ok(files.into_iter().collect());
    }

    if use_regex {
        let matchers: Result<Vec<Regex>, _> = patterns
            .iter()
            .map(|p| {
                Regex::new(p)
                    .map_err(|e| DbError::InvalidInput(format!("Invalid regex pattern '{p}': {e}")))
            })
            .collect();
        let matchers = matchers?;

        Ok(files
            .into_iter()
            .filter(|f| {
                f.to_str().is_some_and(|s| {
                    if match_all {
                        matchers.iter().all(|m| m.is_match(s))
                    } else {
                        matchers.iter().any(|m| m.is_match(s))
                    }
                })
            })
            .collect())
    } else {
        let matchers: Result<Vec<GlobPattern>, _> = patterns
            .iter()
            .map(|p| {
                GlobPattern::new(p)
                    .map_err(|e| DbError::InvalidInput(format!("Invalid glob pattern '{p}': {e}")))
            })
            .collect();
        let matchers = matchers?;

        Ok(files
            .into_iter()
            .filter(|f| {
                f.to_str().is_some_and(|s| {
                    if match_all {
                        matchers.iter().all(|m| m.matches(s))
                    } else {
                        matchers.iter().any(|m| m.matches(s))
                    }
                })
            })
            .collect())
    }
}

/// Extension trait for filtering iterators of `PathBuf` by patterns
///
/// This trait adds pattern filtering capabilities directly to iterators,
/// enabling lazy evaluation and method chaining. Only available for
/// `PathBuf` iterators since patterns specifically match file paths.
pub trait PathFilterExt: IntoIterator<Item = PathBuf> + Sized {
    /// Filter paths by glob or regex patterns with AND/OR logic
    ///
    /// # Arguments
    /// * `patterns` - Patterns to match against file paths
    /// * `use_regex` - If true, treat patterns as regex; otherwise as globs
    /// * `match_all` - If true, path must match ALL patterns (AND); otherwise ANY pattern (OR)
    ///
    /// # Returns
    /// Vector of file paths matching the criteria
    ///
    /// # Errors
    /// Returns `DbError::InvalidInput` if any pattern is invalid
    ///
    /// # Examples
    /// ```ignore
    /// use tagr::search::filter::PathFilterExt;
    ///
    /// let rust_files = all_files
    ///     .into_iter()
    ///     .filter_patterns(&["*.rs".to_string()], false, false)?;
    /// ```
    fn filter_patterns(
        self,
        patterns: &[String],
        use_regex: bool,
        match_all: bool,
    ) -> Result<Vec<PathBuf>, DbError> {
        by_patterns(self, patterns, use_regex, match_all)
    }

    /// Filter paths by glob patterns with ANY logic (match at least one)
    ///
    /// Convenience method for the common case of OR-matching glob patterns.
    ///
    /// # Errors
    /// Returns an error if any glob pattern is invalid
    fn filter_glob_any(self, patterns: &[String]) -> Result<Vec<PathBuf>, DbError> {
        by_patterns(self, patterns, false, false)
    }

    /// Filter paths by glob patterns with ALL logic (match every pattern)
    ///
    /// Convenience method for AND-matching glob patterns.
    ///
    /// # Errors
    /// Returns an error if any glob pattern is invalid
    fn filter_glob_all(self, patterns: &[String]) -> Result<Vec<PathBuf>, DbError> {
        by_patterns(self, patterns, false, true)
    }

    /// Filter paths by regex patterns with ANY logic (match at least one)
    ///
    /// Convenience method for OR-matching regex patterns.
    ///
    /// # Errors
    /// Returns an error if any regex pattern is invalid
    fn filter_regex_any(self, patterns: &[String]) -> Result<Vec<PathBuf>, DbError> {
        by_patterns(self, patterns, true, false)
    }

    /// Filter paths by regex patterns with ALL logic (match every pattern)
    ///
    /// Convenience method for AND-matching regex patterns.
    ///
    /// # Errors
    /// Returns an error if any regex pattern is invalid
    fn filter_regex_all(self, patterns: &[String]) -> Result<Vec<PathBuf>, DbError> {
        by_patterns(self, patterns, true, true)
    }
}

// Implement for any iterator that yields PathBuf
impl<I> PathFilterExt for I where I: IntoIterator<Item = PathBuf> {}

/// Extension trait for filtering `PathBuf` collections with database tag access
///
/// This trait adds tag-based filtering that requires database access.
/// Scoped to Vec<PathBuf> since tag exclusion needs efficient random access.
pub trait PathTagFilterExt {
    /// Exclude files that have any of the specified tags
    ///
    /// Filters out files that contain at least one of the excluded tags.
    /// Files without tags in the database are kept.
    ///
    /// # Arguments
    /// * `db` - Database to query for file tags
    /// * `exclude_tags` - Tags that should cause files to be filtered out
    ///
    /// # Returns
    /// Vector of file paths without any of the excluded tags
    ///
    /// # Errors
    /// Returns `DbError` if database operations fail
    ///
    /// # Examples
    /// ```ignore
    /// use tagr::search::filter::PathTagFilterExt;
    ///
    /// let filtered = files
    ///     .exclude_tags(db, &["temp".to_string(), "deprecated".to_string()])?;
    /// ```
    fn exclude_tags(self, db: &Database, exclude_tags: &[String]) -> Result<Vec<PathBuf>, DbError>;
}

impl PathTagFilterExt for Vec<PathBuf> {
    fn exclude_tags(self, db: &Database, exclude_tags: &[String]) -> Result<Vec<PathBuf>, DbError> {
        if exclude_tags.is_empty() {
            return Ok(self);
        }

        let mut result = Self::new();
        for file in self {
            if let Some(file_tags) = db.get_tags(&file)? {
                let has_excluded = file_tags.iter().any(|tag| exclude_tags.contains(tag));
                if !has_excluded {
                    result.push(file);
                }
            } else {
                // Files without tags pass through
                result.push(file);
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_filter_empty_patterns() {
        let files = vec![PathBuf::from("test.rs"), PathBuf::from("main.rs")];

        let result = by_patterns(files.clone(), &[], false, false).unwrap();
        assert_eq!(result, files);
    }

    #[test]
    fn test_filter_glob_any() {
        let files = vec![
            PathBuf::from("test.rs"),
            PathBuf::from("main.rs"),
            PathBuf::from("test.txt"),
        ];

        let result = by_patterns(files, &["*.rs".to_string()], false, false).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.contains(&PathBuf::from("test.rs")));
        assert!(result.contains(&PathBuf::from("main.rs")));
    }

    #[test]
    fn test_filter_glob_all() {
        let files = vec![
            PathBuf::from("src/test.rs"),
            PathBuf::from("src/main.rs"),
            PathBuf::from("test.txt"),
        ];

        let result = by_patterns(
            files,
            &["src/*".to_string(), "*.rs".to_string()],
            false,
            true,
        )
        .unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.contains(&PathBuf::from("src/test.rs")));
    }

    #[test]
    fn test_filter_regex_any() {
        let files = vec![
            PathBuf::from("test123.rs"),
            PathBuf::from("main.rs"),
            PathBuf::from("test.txt"),
        ];

        let result = by_patterns(files, &[r"test\d+\.rs".to_string()], true, false).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], PathBuf::from("test123.rs"));
    }

    #[test]
    fn test_invalid_regex() {
        let files = vec![PathBuf::from("test.rs")];
        let result = by_patterns(files, &["[invalid".to_string()], true, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_glob() {
        let files = vec![PathBuf::from("test.rs")];
        let result = by_patterns(files, &["[".to_string()], false, false);
        assert!(result.is_err());
    }

    // Extension trait tests
    #[test]
    fn test_path_filter_ext_glob_any() {
        let files = vec![
            PathBuf::from("test.rs"),
            PathBuf::from("main.rs"),
            PathBuf::from("test.txt"),
        ];

        let result = files
            .into_iter()
            .filter_glob_any(&["*.rs".to_string()])
            .unwrap();

        assert_eq!(result.len(), 2);
        assert!(result.contains(&PathBuf::from("test.rs")));
        assert!(result.contains(&PathBuf::from("main.rs")));
    }

    #[test]
    fn test_path_filter_ext_chaining() {
        let files = vec![
            PathBuf::from("src/test.rs"),
            PathBuf::from("src/main.rs"),
            PathBuf::from("tests/integration.rs"),
            PathBuf::from("README.md"),
        ];

        // First filter: only .rs files
        let result = files
            .into_iter()
            .filter_glob_any(&["*.rs".to_string()])
            .unwrap();

        assert_eq!(result.len(), 3);
    }
}
