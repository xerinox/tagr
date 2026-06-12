//! Hierarchical tag matching with specificity-based filtering.
//!
//! Implements prefix-based hierarchy matching where:
//! - `-t lang` matches any tag starting with `lang:` (e.g., `lang:rust`, `lang:python:asyncio`)
//! - Deeper tags override shallower ones within the same hierarchy (specificity rule)
//! - Excludes always win against includes from different hierarchies
//!
//! # Examples
//!
//! ```ignore
//! // File: example.rs [lang:rust, tests]
//! // Search: -t lang -x tests
//! // Result: EXCLUDED (different hierarchies, exclude wins)
//!
//! // File: example.rs [lang:rust, lang:javascript]
//! // Search: -t lang -x lang:rust
//! // Result: lang:javascript ONLY (specificity within hierarchy)
//! ```

use crate::schema::HIERARCHY_DELIMITER;
use std::collections::HashMap;

/// Compute the depth of a hierarchical tag.
///
/// Depth = number of delimiters + 1:
/// - `lang` → 1
/// - `lang:rust` → 2
/// - `lang:rust:async` → 3
#[must_use]
pub fn tag_depth(tag: &str) -> usize {
    tag.matches(HIERARCHY_DELIMITER).count() + 1
}

/// Extract the hierarchy root from a tag.
///
/// Returns everything before the first delimiter, or the full tag if none.
#[must_use]
pub fn hierarchy_root(tag: &str) -> &str {
    tag.split(HIERARCHY_DELIMITER).next().unwrap_or(tag)
}

/// Check if a tag matches a pattern (prefix match for hierarchies).
///
/// A pattern matches a tag if:
/// - Exact match: `lang` == `lang`
/// - Prefix match: `lang` matches `lang:rust`
#[must_use]
pub fn pattern_matches(pattern: &str, tag: &str) -> bool {
    if pattern == tag {
        return true;
    }
    let prefix = format!("{pattern}{HIERARCHY_DELIMITER}");
    tag.starts_with(&prefix)
}

/// Signal indicating whether a tag should be included or excluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Signal {
    Include,
    Exclude,
}

/// Find the most specific matching pattern for a given tag.
///
/// Returns the matching pattern with the highest depth (most specific).
/// If multiple patterns have the same depth, prefers exclude over include.
fn most_specific_match(
    tag: &str,
    include_patterns: &[impl AsRef<str>],
    exclude_patterns: &[impl AsRef<str>],
) -> Option<(Signal, usize)> {
    let mut best_match: Option<(Signal, usize)> = None;

    for pattern in include_patterns {
        if pattern_matches(pattern.as_ref(), tag) {
            let depth = tag_depth(pattern.as_ref());
            match best_match {
                None => best_match = Some((Signal::Include, depth)),
                Some((_, best_depth)) if depth > best_depth => {
                    best_match = Some((Signal::Include, depth));
                }
                _ => {}
            }
        }
    }

    for pattern in exclude_patterns {
        if pattern_matches(pattern.as_ref(), tag) {
            let depth = tag_depth(pattern.as_ref());
            match best_match {
                None => best_match = Some((Signal::Exclude, depth)),
                Some((_, best_depth)) if depth > best_depth => {
                    best_match = Some((Signal::Exclude, depth));
                }
                Some((Signal::Include, best_depth)) if depth == best_depth => {
                    best_match = Some((Signal::Exclude, depth));
                }
                _ => {}
            }
        }
    }

    best_match
}

/// Determine if a file should be included based on hierarchical tag filtering.
///
/// Algorithm:
/// 1. For each file tag, find the most specific matching pattern in its hierarchy
/// 2. If ANY file tag produces an exclude signal → exclude the file
/// 3. If ALL file tags produce include signals (or no match) → include the file
///
/// Cross-hierarchy rule: Excludes from any hierarchy override includes from other hierarchies.
#[must_use]
pub fn should_include_file(
    file_tags: &[impl AsRef<str>],
    include_patterns: &[impl AsRef<str>],
    exclude_patterns: &[impl AsRef<str>],
) -> bool {
    let mut hierarchy_signals: HashMap<String, Vec<(Signal, usize)>> = HashMap::new();

    for tag in file_tags {
        let root = hierarchy_root(tag.as_ref()).to_string();

        if let Some((signal, depth)) =
            most_specific_match(tag.as_ref(), include_patterns, exclude_patterns)
        {
            hierarchy_signals
                .entry(root)
                .or_default()
                .push((signal, depth));
        }
    }

    for signals in hierarchy_signals.values() {
        let mut most_specific: Option<(Signal, usize)> = None;

        for &(signal, depth) in signals {
            match most_specific {
                None => most_specific = Some((signal, depth)),
                Some((_, best_depth)) if depth > best_depth => {
                    most_specific = Some((signal, depth));
                }
                Some((Signal::Include, best_depth))
                    if depth == best_depth && signal == Signal::Exclude =>
                {
                    most_specific = Some((signal, depth));
                }
                _ => {}
            }
        }

        if let Some((Signal::Exclude, _)) = most_specific {
            return false;
        }
    }

    if include_patterns.is_empty() {
        return true;
    }

    // File must match at least one include pattern
    file_tags.iter().any(|tag| {
        include_patterns
            .iter()
            .any(|pattern| pattern_matches(pattern.as_ref(), tag.as_ref()))
    })
}

/// Filter files based on hierarchical tag patterns.
///
/// Applies specificity-based filtering to a set of files.
pub fn filter_by_hierarchy<'a>(
    files_with_tags: impl Iterator<Item = (&'a str, &'a [String])>,
    include_patterns: &[String],
    exclude_patterns: &[String],
) -> Vec<String> {
    files_with_tags
        .filter(|(_, tags)| should_include_file(tags, include_patterns, exclude_patterns))
        .map(|(file, _)| file.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tag_depth() {
        assert_eq!(tag_depth("lang"), 1);
        assert_eq!(tag_depth("lang:rust"), 2);
        assert_eq!(tag_depth("lang:rust:async"), 3);
        assert_eq!(tag_depth("project:name:with:many:parts"), 5);
    }

    #[test]
    fn test_hierarchy_root() {
        assert_eq!(hierarchy_root("lang"), "lang");
        assert_eq!(hierarchy_root("lang:rust"), "lang");
        assert_eq!(hierarchy_root("lang:rust:async"), "lang");
        assert_eq!(hierarchy_root("project"), "project");
    }

    #[test]
    fn test_pattern_matches() {
        assert!(pattern_matches("lang", "lang"));
        assert!(pattern_matches("lang:rust", "lang:rust"));
        assert!(pattern_matches("lang", "lang:rust"));
        assert!(pattern_matches("lang", "lang:rust:async"));
        assert!(pattern_matches("lang:rust", "lang:rust:async"));
        assert!(!pattern_matches("lang:python", "lang:rust"));
        assert!(!pattern_matches("lang:rust", "lang:python"));
        assert!(!pattern_matches("lang", "other"));
        assert!(!pattern_matches("lang:rust:async", "lang:rust"));
    }

    #[test]
    fn test_most_specific_match_include_only() {
        let includes = vec!["lang".to_string()];
        let excludes: Vec<String> = vec![];
        assert_eq!(
            most_specific_match("lang:rust", &includes, &excludes),
            Some((Signal::Include, 1))
        );
    }

    #[test]
    fn test_most_specific_match_exclude_only() {
        let includes: Vec<String> = vec![];
        let excludes = vec!["lang:rust".to_string()];
        assert_eq!(
            most_specific_match("lang:rust:async", &includes, &excludes),
            Some((Signal::Exclude, 2))
        );
    }

    #[test]
    fn test_most_specific_match_specificity() {
        let includes = vec!["lang".to_string()];
        let excludes = vec!["lang:rust".to_string()];
        assert_eq!(
            most_specific_match("lang:rust:async", &includes, &excludes),
            Some((Signal::Exclude, 2))
        );
        assert_eq!(
            most_specific_match("lang:python", &includes, &excludes),
            Some((Signal::Include, 1))
        );
    }

    #[test]
    fn test_most_specific_match_same_depth_prefers_exclude() {
        let includes = vec!["lang:rust".to_string()];
        let excludes = vec!["lang:rust".to_string()];
        assert_eq!(
            most_specific_match("lang:rust", &includes, &excludes),
            Some((Signal::Exclude, 2))
        );
    }

    #[test]
    fn test_should_include_file_simple_include() {
        let tags = vec!["lang:rust".to_string()];
        let includes = vec!["lang".to_string()];
        let excludes: Vec<String> = vec![];
        assert!(should_include_file(&tags, &includes, &excludes));
    }

    #[test]
    fn test_should_include_file_simple_exclude() {
        let tags = vec!["lang:rust".to_string()];
        let includes: Vec<String> = vec![];
        let excludes = vec!["lang:rust".to_string()];
        assert!(!should_include_file(&tags, &includes, &excludes));
    }

    #[test]
    fn test_should_include_file_specificity_override() {
        let tags = vec!["lang:rust".to_string()];
        let includes = vec!["lang".to_string()];
        let excludes = vec!["lang:rust".to_string()];
        assert!(!should_include_file(&tags, &includes, &excludes));
    }

    #[test]
    fn test_should_include_file_cross_hierarchy_exclude_wins() {
        let tags = vec!["lang:rust".to_string(), "tests".to_string()];
        let includes = vec!["lang".to_string()];
        let excludes = vec!["tests".to_string()];
        assert!(!should_include_file(&tags, &includes, &excludes));
    }

    #[test]
    fn test_should_include_file_deeper_include_overrides_exclude() {
        let tags = vec!["lang:rust:async".to_string()];
        let includes = vec!["lang".to_string(), "lang:rust:async".to_string()];
        let excludes = vec!["lang:rust".to_string()];
        assert!(should_include_file(&tags, &includes, &excludes));
    }

    #[test]
    fn test_should_include_file_multiple_hierarchies() {
        let tags = vec![
            "lang:javascript".to_string(),
            "lang:rust".to_string(),
            "tests".to_string(),
        ];
        let includes = vec!["lang".to_string()];
        let excludes = vec!["lang:rust".to_string()];
        assert!(!should_include_file(&tags, &includes, &excludes));
    }

    #[test]
    fn test_should_include_file_no_patterns() {
        let tags = vec!["lang:rust".to_string()];
        let includes: Vec<String> = vec![];
        let excludes: Vec<String> = vec![];
        assert!(should_include_file(&tags, &includes, &excludes));
    }

    #[test]
    fn test_should_include_file_no_match_with_include_patterns() {
        let tags = vec!["other:tag".to_string()];
        let includes = vec!["lang".to_string()];
        let excludes: Vec<String> = vec![];
        assert!(!should_include_file(&tags, &includes, &excludes));
    }

    #[test]
    fn test_filter_by_hierarchy() {
        let files_tags = [
            (
                "file1.js",
                vec!["lang:javascript".to_string(), "production".to_string()],
            ),
            (
                "file2.js",
                vec!["lang:javascript".to_string(), "tests".to_string()],
            ),
            (
                "file3.rs",
                vec!["lang:rust".to_string(), "tests".to_string()],
            ),
        ];

        let includes = vec!["lang".to_string()];
        let excludes = vec!["lang:rust".to_string()];

        let files_refs = files_tags.iter().map(|(f, tags)| (*f, tags.as_slice()));
        let result = filter_by_hierarchy(files_refs, &includes, &excludes);

        assert_eq!(result.len(), 2);
        assert!(result.contains(&"file1.js".to_string()));
        assert!(result.contains(&"file2.js".to_string()));
        assert!(!result.contains(&"file3.rs".to_string()));
    }
}
