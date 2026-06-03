//! File pattern filtering — glob and regex matching for file paths.
//!
//! Provides unified filtering for both glob patterns and regex matching,
//! with AND/OR logic for combining multiple patterns.

use crate::store::StoreError;
use glob::Pattern as GlobPattern;
use regex::Regex;

/// Filter file paths by glob or regex patterns with AND/OR logic.
///
/// # Arguments
/// * `paths` - File paths to filter (as `AsRef<str>`)
/// * `patterns` - Patterns to match against
/// * `use_regex` - If true, patterns are regex; otherwise glob
/// * `match_all` - If true, path must match ALL patterns (AND); otherwise ANY (OR)
///
/// # Errors
/// Returns `StoreError::IoFailed` if any pattern is invalid.
pub fn filter_by_patterns<S: AsRef<str>>(
    paths: &[S],
    patterns: &[String],
    use_regex: bool,
    match_all: bool,
) -> Result<Vec<String>, StoreError> {
    if patterns.is_empty() {
        return Ok(paths.iter().map(|s| s.as_ref().to_string()).collect());
    }

    if use_regex {
        let matchers: Vec<Regex> = patterns
            .iter()
            .map(|p| {
                Regex::new(p).map_err(|e| StoreError::IoFailed {
                    context: format!("invalid regex pattern '{p}': {e}"),
                    source: std::io::Error::new(std::io::ErrorKind::InvalidInput, e.to_string()),
                })
            })
            .collect::<Result<_, _>>()?;

        Ok(paths
            .iter()
            .map(AsRef::as_ref)
            .filter(|s| {
                if match_all {
                    matchers.iter().all(|m| m.is_match(s))
                } else {
                    matchers.iter().any(|m| m.is_match(s))
                }
            })
            .map(String::from)
            .collect())
    } else {
        let matchers: Vec<GlobPattern> = patterns
            .iter()
            .map(|p| {
                GlobPattern::new(p).map_err(|e| StoreError::IoFailed {
                    context: format!("invalid glob pattern '{p}': {e}"),
                    source: std::io::Error::new(std::io::ErrorKind::InvalidInput, e.to_string()),
                })
            })
            .collect::<Result<_, _>>()?;

        Ok(paths
            .iter()
            .map(AsRef::as_ref)
            .filter(|s| {
                if match_all {
                    matchers.iter().all(|m| m.matches(s))
                } else {
                    matchers.iter().any(|m| m.matches(s))
                }
            })
            .map(String::from)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_empty_patterns() {
        let files = vec!["test.rs", "main.rs"];
        let result = filter_by_patterns(&files, &[], false, false).unwrap();
        assert_eq!(result, vec!["test.rs", "main.rs"]);
    }

    #[test]
    fn test_filter_glob_any() {
        let files = vec!["test.rs", "main.rs", "test.txt"];
        let result = filter_by_patterns(&files, &["*.rs".to_string()], false, false).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.contains(&"test.rs".to_string()));
        assert!(result.contains(&"main.rs".to_string()));
    }

    #[test]
    fn test_filter_glob_all() {
        let files = vec!["src/test.rs", "src/main.rs", "test.txt"];
        let result = filter_by_patterns(
            &files,
            &["src/*".to_string(), "*.rs".to_string()],
            false,
            true,
        )
        .unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.contains(&"src/test.rs".to_string()));
    }

    #[test]
    fn test_filter_regex_any() {
        let files = vec!["test123.rs", "main.rs", "test.txt"];
        let result =
            filter_by_patterns(&files, &[r"test\d+\.rs".to_string()], true, false).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "test123.rs");
    }

    #[test]
    fn test_invalid_regex() {
        let files = vec!["test.rs"];
        let result = filter_by_patterns(&files, &["[invalid".to_string()], true, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_glob() {
        let files = vec!["test.rs"];
        let result = filter_by_patterns(&files, &["[".to_string()], false, false);
        assert!(result.is_err());
    }
}
