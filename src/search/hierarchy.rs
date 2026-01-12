//! Hierarchical tag matching with specificity-based filtering
//!
//! This module implements prefix-based hierarchy matching where:
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

/// Compute the depth of a hierarchical tag
///
/// Depth is defined as the number of delimiters + 1.
/// Examples:
/// - `lang` → depth 1
/// - `lang:rust` → depth 2
/// - `lang:rust:async` → depth 3
///
/// # Examples
/// ```
/// # use tagr::search::hierarchy::tag_depth;
/// assert_eq!(tag_depth("lang"), 1);
/// assert_eq!(tag_depth("lang:rust"), 2);
/// assert_eq!(tag_depth("lang:rust:async"), 3);
/// ```
pub fn tag_depth(tag: &str) -> usize {
    tag.matches(HIERARCHY_DELIMITER).count() + 1
}

/// Extract the hierarchy root from a tag
///
/// Returns everything before the first delimiter, or the full tag if no delimiter.
///
/// # Examples
/// ```
/// # use tagr::search::hierarchy::hierarchy_root;
/// assert_eq!(hierarchy_root("lang"), "lang");
/// assert_eq!(hierarchy_root("lang:rust"), "lang");
/// assert_eq!(hierarchy_root("lang:rust:async"), "lang");
/// ```
pub fn hierarchy_root(tag: &str) -> &str {
    tag.split(HIERARCHY_DELIMITER).next().unwrap_or(tag)
}

/// Check if a tag matches a pattern (prefix match for hierarchies)
///
/// A pattern matches a tag if:
/// - Exact match: `lang` == `lang`
/// - Prefix match: `lang` matches `lang:rust`
///
/// # Examples
/// ```
/// # use tagr::search::hierarchy::pattern_matches;
/// assert!(pattern_matches("lang", "lang"));
/// assert!(pattern_matches("lang", "lang:rust"));
/// assert!(pattern_matches("lang:rust", "lang:rust:async"));
/// assert!(!pattern_matches("lang:python", "lang:rust"));
/// ```
pub fn pattern_matches(pattern: &str, tag: &str) -> bool {
    if pattern == tag {
        return true;
    }
    
    // Check if tag starts with "pattern:"
    let prefix = format!("{pattern}{HIERARCHY_DELIMITER}");
    tag.starts_with(&prefix)
}

/// Signal indicating whether a tag should be included or excluded
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Signal {
    Include,
    Exclude,
}

/// Find the most specific matching pattern for a given tag
///
/// Returns the matching pattern with the highest depth (most specific).
/// If multiple patterns have the same depth, prefers exclude over include.
///
/// # Arguments
/// * `tag` - The tag to match against
/// * `include_patterns` - Patterns that signal inclusion
/// * `exclude_patterns` - Patterns that signal exclusion
///
/// # Returns
/// `Some((Signal, depth))` if a match is found, `None` otherwise
fn most_specific_match(
    tag: &str,
    include_patterns: &[String],
    exclude_patterns: &[String],
) -> Option<(Signal, usize)> {
    let mut best_match: Option<(Signal, usize)> = None;

    // Check include patterns
    for pattern in include_patterns {
        if pattern_matches(pattern, tag) {
            let depth = tag_depth(pattern);
            match best_match {
                None => best_match = Some((Signal::Include, depth)),
                Some((_, best_depth)) if depth > best_depth => {
                    best_match = Some((Signal::Include, depth));
                }
                Some((Signal::Exclude, best_depth)) if depth == best_depth => {
                    // Keep exclude at same depth
                }
                _ => {}
            }
        }
    }

    // Check exclude patterns
    for pattern in exclude_patterns {
        if pattern_matches(pattern, tag) {
            let depth = tag_depth(pattern);
            match best_match {
                None => best_match = Some((Signal::Exclude, depth)),
                Some((_, best_depth)) if depth > best_depth => {
                    best_match = Some((Signal::Exclude, depth));
                }
                Some((Signal::Include, best_depth)) if depth == best_depth => {
                    // Prefer exclude at same depth
                    best_match = Some((Signal::Exclude, depth));
                }
                _ => {}
            }
        }
    }

    best_match
}

/// Determine if a file should be included based on hierarchical tag filtering
///
/// Algorithm:
/// 1. For each file tag, find the most specific matching pattern in its hierarchy
/// 2. If ANY file tag produces an exclude signal → exclude the file
/// 3. If ALL file tags produce include signals (or no match) → include the file
///
/// Cross-hierarchy rule: Excludes from any hierarchy override includes from other hierarchies.
///
/// # Arguments
/// * `file_tags` - Tags associated with the file
/// * `include_patterns` - Include patterns from CLI (e.g., `-t lang`)
/// * `exclude_patterns` - Exclude patterns from CLI (e.g., `-x tests`)
///
/// # Returns
/// `true` if the file should be included, `false` otherwise
///
/// # Examples
/// ```ignore
/// let file_tags = vec!["lang:rust".to_string(), "tests".to_string()];
/// let include = vec!["lang".to_string()];
/// let exclude = vec!["tests".to_string()];
/// 
/// // Result: false (tests is excluded, cross-hierarchy)
/// assert!(!should_include_file(&file_tags, &include, &exclude));
/// ```
pub fn should_include_file(
    file_tags: &[String],
    include_patterns: &[String],
    exclude_patterns: &[String],
) -> bool {
    // Group file tags by hierarchy
    let mut hierarchy_signals: HashMap<String, Vec<(Signal, usize)>> = HashMap::new();

    for tag in file_tags {
        let root = hierarchy_root(tag).to_string();
        
        if let Some((signal, depth)) = most_specific_match(tag, include_patterns, exclude_patterns) {
            hierarchy_signals
                .entry(root)
                .or_default()
                .push((signal, depth));
        }
    }

    // Check if any hierarchy has an exclude signal
    for signals in hierarchy_signals.values() {
        // Within a hierarchy, use the most specific signal
        let mut most_specific: Option<(Signal, usize)> = None;
        
        for &(signal, depth) in signals {
            match most_specific {
                None => most_specific = Some((signal, depth)),
                Some((_, best_depth)) if depth > best_depth => {
                    most_specific = Some((signal, depth));
                }
                Some((Signal::Include, best_depth)) if depth == best_depth && signal == Signal::Exclude => {
                    // Prefer exclude at same depth
                    most_specific = Some((signal, depth));
                }
                _ => {}
            }
        }

        // If the most specific signal in this hierarchy is exclude, exclude the file
        if let Some((Signal::Exclude, _)) = most_specific {
            return false;
        }
    }

    // If we have include patterns but no tags matched, exclude the file
    if !include_patterns.is_empty() {
        let has_match = file_tags.iter().any(|tag| {
            include_patterns.iter().any(|pattern| pattern_matches(pattern, tag))
        });
        
        if !has_match {
            return false;
        }
    }

    true
}

/// Filter files based on hierarchical tag patterns
///
/// This function applies specificity-based filtering to a set of files.
/// It's used internally by the search module to implement hierarchy-aware queries.
///
/// # Arguments
/// * `files_with_tags` - Iterator of (file, tags) pairs
/// * `include_patterns` - Patterns to include (e.g., `-t lang`)
/// * `exclude_patterns` - Patterns to exclude (e.g., `-x tests`)
///
/// # Returns
/// Vector of files that pass the hierarchical filter
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
        // Exact matches
        assert!(pattern_matches("lang", "lang"));
        assert!(pattern_matches("lang:rust", "lang:rust"));

        // Prefix matches
        assert!(pattern_matches("lang", "lang:rust"));
        assert!(pattern_matches("lang", "lang:rust:async"));
        assert!(pattern_matches("lang:rust", "lang:rust:async"));

        // Non-matches
        assert!(!pattern_matches("lang:python", "lang:rust"));
        assert!(!pattern_matches("lang:rust", "lang:python"));
        assert!(!pattern_matches("lang", "other"));
        assert!(!pattern_matches("lang:rust:async", "lang:rust"));
    }

    #[test]
    fn test_most_specific_match_include_only() {
        let includes = vec!["lang".to_string()];
        let excludes = vec![];

        assert_eq!(
            most_specific_match("lang:rust", &includes, &excludes),
            Some((Signal::Include, 1))
        );
    }

    #[test]
    fn test_most_specific_match_exclude_only() {
        let includes = vec![];
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

        // More specific exclude wins
        assert_eq!(
            most_specific_match("lang:rust:async", &includes, &excludes),
            Some((Signal::Exclude, 2))
        );

        // Only shallow include matches
        assert_eq!(
            most_specific_match("lang:python", &includes, &excludes),
            Some((Signal::Include, 1))
        );
    }

    #[test]
    fn test_most_specific_match_same_depth_prefers_exclude() {
        let includes = vec!["lang:rust".to_string()];
        let excludes = vec!["lang:rust".to_string()];

        // Same depth - prefer exclude
        assert_eq!(
            most_specific_match("lang:rust", &includes, &excludes),
            Some((Signal::Exclude, 2))
        );
    }

    #[test]
    fn test_should_include_file_simple_include() {
        let file_tags = vec!["lang:rust".to_string()];
        let includes = vec!["lang".to_string()];
        let excludes = vec![];

        assert!(should_include_file(&file_tags, &includes, &excludes));
    }

    #[test]
    fn test_should_include_file_simple_exclude() {
        let file_tags = vec!["lang:rust".to_string()];
        let includes = vec![];
        let excludes = vec!["lang:rust".to_string()];

        assert!(!should_include_file(&file_tags, &includes, &excludes));
    }

    #[test]
    fn test_should_include_file_specificity_override() {
        let file_tags = vec!["lang:rust".to_string()];
        let includes = vec!["lang".to_string()];
        let excludes = vec!["lang:rust".to_string()];

        // Exclude is more specific
        assert!(!should_include_file(&file_tags, &includes, &excludes));
    }

    #[test]
    fn test_should_include_file_cross_hierarchy_exclude_wins() {
        let file_tags = vec!["lang:rust".to_string(), "tests".to_string()];
        let includes = vec!["lang".to_string()];
        let excludes = vec!["tests".to_string()];

        // Different hierarchies - exclude wins
        assert!(!should_include_file(&file_tags, &includes, &excludes));
    }

    #[test]
    fn test_should_include_file_deeper_include_overrides_exclude() {
        let file_tags = vec!["lang:rust:async".to_string()];
        let includes = vec!["lang".to_string(), "lang:rust:async".to_string()];
        let excludes = vec!["lang:rust".to_string()];

        // Depth 3 include overrides depth 2 exclude
        assert!(should_include_file(&file_tags, &includes, &excludes));
    }

    #[test]
    fn test_should_include_file_multiple_hierarchies() {
        let file_tags = vec![
            "lang:javascript".to_string(),
            "lang:rust".to_string(),
            "tests".to_string(),
        ];
        let includes = vec!["lang".to_string()];
        let excludes = vec!["lang:rust".to_string()];

        // lang:rust is excluded, but lang:javascript is included
        // However, having any excluded tag in the file excludes it
        // Wait, this needs clarification...
        
        // Based on the algorithm: we check per-hierarchy
        // lang hierarchy: has both javascript (include) and rust (exclude at depth 2)
        // Most specific in lang hierarchy would be the exclude at depth 2
        // So this should be excluded
        
        // Actually, let me re-read the algorithm...
        // We need to check if ANY tag produces an exclude signal
        
        // For lang:javascript: matches lang (include, depth 1)
        // For lang:rust: matches lang (include, depth 1) and lang:rust (exclude, depth 2)
        //   → most specific is exclude
        // For tests: no match
        
        // Since lang:rust produces exclude signal, file is excluded
        assert!(!should_include_file(&file_tags, &includes, &excludes));
    }

    #[test]
    fn test_should_include_file_no_patterns() {
        let file_tags = vec!["lang:rust".to_string()];
        let includes = vec![];
        let excludes = vec![];

        // No patterns means include everything
        assert!(should_include_file(&file_tags, &includes, &excludes));
    }

    #[test]
    fn test_should_include_file_no_match_with_include_patterns() {
        let file_tags = vec!["other:tag".to_string()];
        let includes = vec!["lang".to_string()];
        let excludes = vec![];

        // Has include patterns but no tags match - exclude
        assert!(!should_include_file(&file_tags, &includes, &excludes));
    }

    #[test]
    fn test_filter_by_hierarchy() {
        let files_tags = vec![
            ("file1.js", vec!["lang:javascript".to_string(), "production".to_string()]),
            ("file2.js", vec!["lang:javascript".to_string(), "tests".to_string()]),
            ("file3.rs", vec!["lang:rust".to_string(), "tests".to_string()]),
        ];

        let includes = vec!["lang".to_string()];
        let excludes = vec!["lang:rust".to_string()];

        let files_tags_refs: Vec<(&str, &[String])> = files_tags
            .iter()
            .map(|(f, tags)| (*f, tags.as_slice()))
            .collect();

        let result = filter_by_hierarchy(files_tags_refs.into_iter(), &includes, &excludes);

        assert_eq!(result.len(), 2);
        assert!(result.contains(&"file1.js".to_string()));
        assert!(result.contains(&"file2.js".to_string()));
        assert!(!result.contains(&"file3.rs".to_string()));
    }
}
