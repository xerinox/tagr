//! Active filter state for TUI sessions
//!
//! This module provides the `ActiveFilter` struct which wraps `FilterCriteria`
//! with additional runtime state and Display implementation for CLI preview generation.

use crate::cli::SearchParams;
use crate::filters::{FileMode, FilterCriteria, TagMode};
use std::fmt;

/// Live filter state for the TUI session
///
/// Wraps `FilterCriteria` with additional runtime state and Display impl
/// for CLI preview generation. This provides a single source of truth for
/// all filter state in the TUI, avoiding scattered fields across AppState.
#[derive(Debug, Clone, Default)]
pub struct ActiveFilter {
    /// Core filter criteria (reuses existing FilterCriteria)
    pub criteria: FilterCriteria,
}

impl ActiveFilter {
    /// Create a new empty active filter
    #[must_use]
    pub const fn new() -> Self {
        Self {
            criteria: FilterCriteria {
                tags: Vec::new(),
                tag_mode: TagMode::All,
                file_patterns: Vec::new(),
                file_mode: FileMode::Any,
                excludes: Vec::new(),
                regex_tag: false,
                regex_file: false,
                glob_files: false,
                virtual_tags: Vec::new(),
                virtual_mode: TagMode::All,
            },
        }
    }

    /// Create from existing filter criteria
    #[must_use]
    pub const fn from_criteria(criteria: FilterCriteria) -> Self {
        Self { criteria }
    }

    /// Load from a saved filter
    #[must_use]
    pub fn from_saved(filter: &crate::filters::Filter) -> Self {
        Self {
            criteria: filter.criteria.clone(),
        }
    }

    /// Add a tag to include list
    ///
    /// If the tag was in the exclude list, it's removed from there.
    /// Prevents duplicates in the include list.
    pub fn include_tag(&mut self, tag: String) {
        self.criteria.excludes.retain(|t| t != &tag);
        if !self.criteria.tags.contains(&tag) {
            self.criteria.tags.push(tag);
        }
    }

    /// Add a tag to exclude list
    ///
    /// If the tag was in the include list, it's removed from there.
    /// Prevents duplicates in the exclude list.
    pub fn exclude_tag(&mut self, tag: String) {
        self.criteria.tags.retain(|t| t != &tag);
        if !self.criteria.excludes.contains(&tag) {
            self.criteria.excludes.push(tag);
        }
    }

    /// Remove tag from both include and exclude lists
    pub fn remove_tag(&mut self, tag: &str) {
        self.criteria.tags.retain(|t| t != tag);
        self.criteria.excludes.retain(|t| t != tag);
    }

    /// Check if tag is included
    #[must_use]
    pub fn is_included(&self, tag: &str) -> bool {
        self.criteria.tags.iter().any(|t| t == tag)
    }

    /// Check if tag is excluded
    #[must_use]
    pub fn is_excluded(&self, tag: &str) -> bool {
        self.criteria.excludes.iter().any(|t| t == tag)
    }

    /// Toggle tag inclusion (add if not included, remove if already included)
    ///
    /// Returns true if tag was added, false if removed.
    pub fn toggle_include_tag(&mut self, tag: String) -> bool {
        // Remove from excludes regardless
        self.criteria.excludes.retain(|t| t != &tag);

        // Toggle inclusion
        if let Some(pos) = self.criteria.tags.iter().position(|t| t == &tag) {
            self.criteria.tags.remove(pos);
            false
        } else {
            self.criteria.tags.push(tag);
            true
        }
    }

    /// Toggle tag exclusion (add if not excluded, remove if already excluded)
    ///
    /// Returns true if tag was added to exclusions, false if removed.
    pub fn toggle_exclude_tag(&mut self, tag: String) -> bool {
        // Remove from includes regardless
        self.criteria.tags.retain(|t| t != &tag);

        // Toggle exclusion
        if let Some(pos) = self.criteria.excludes.iter().position(|t| t == &tag) {
            self.criteria.excludes.remove(pos);
            false
        } else {
            self.criteria.excludes.push(tag);
            true
        }
    }

    /// Toggle search mode (ANY ↔ ALL)
    pub fn toggle_mode(&mut self) {
        self.criteria.tag_mode = match self.criteria.tag_mode {
            TagMode::Any => TagMode::All,
            TagMode::All => TagMode::Any,
        };
    }

    /// Check if filter is empty (nothing to save)
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.criteria.tags.is_empty()
            && self.criteria.excludes.is_empty()
            && self.criteria.file_patterns.is_empty()
            && self.criteria.virtual_tags.is_empty()
    }

    /// Add a file pattern
    pub fn add_file_pattern(&mut self, pattern: String) {
        if !self.criteria.file_patterns.contains(&pattern) {
            self.criteria.file_patterns.push(pattern);
        }
    }

    /// Remove a file pattern
    pub fn remove_file_pattern(&mut self, pattern: &str) {
        self.criteria.file_patterns.retain(|p| p != pattern);
    }

    /// Add a virtual tag
    pub fn add_virtual_tag(&mut self, vtag: String) {
        if !self.criteria.virtual_tags.contains(&vtag) {
            self.criteria.virtual_tags.push(vtag);
        }
    }

    /// Remove a virtual tag
    pub fn remove_virtual_tag(&mut self, vtag: &str) {
        self.criteria.virtual_tags.retain(|v| v != vtag);
    }

    /// Toggle file mode (ANY ↔ ALL)
    pub fn toggle_file_mode(&mut self) {
        self.criteria.file_mode = match self.criteria.file_mode {
            FileMode::Any => FileMode::All,
            FileMode::All => FileMode::Any,
        };
    }

    /// Toggle virtual tag mode (ANY ↔ ALL)
    pub fn toggle_virtual_mode(&mut self) {
        self.criteria.virtual_mode = match self.criteria.virtual_mode {
            TagMode::Any => TagMode::All,
            TagMode::All => TagMode::Any,
        };
    }

    /// Merge with additional criteria
    ///
    /// Delegates to `FilterCriteria::merge` which adds/extends lists
    /// and OR's boolean flags.
    pub fn merge(&mut self, other: &FilterCriteria) {
        self.criteria.merge(other);
    }
}

impl fmt::Display for ActiveFilter {
    /// Generate CLI-style preview
    ///
    /// Example: `tagr search -t rust -t python -x javascript --any-tag`
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "tagr search")?;

        // Include tags
        for tag in &self.criteria.tags {
            write!(f, " -t ")?;
            // Quote tags with spaces or special chars
            if needs_quoting(tag) {
                write!(f, "\"{tag}\"")?;
            } else {
                write!(f, "{tag}")?;
            }
        }

        // Exclude tags
        for tag in &self.criteria.excludes {
            write!(f, " -x ")?;
            if needs_quoting(tag) {
                write!(f, "\"{tag}\"")?;
            } else {
                write!(f, "{tag}")?;
            }
        }

        // File patterns
        for pattern in &self.criteria.file_patterns {
            write!(f, " -p ")?;
            if needs_quoting(pattern) {
                write!(f, "\"{pattern}\"")?;
            } else {
                write!(f, "{pattern}")?;
            }
        }

        // Virtual tags
        for vtag in &self.criteria.virtual_tags {
            write!(f, " -v ")?;
            if needs_quoting(vtag) {
                write!(f, "\"{vtag}\"")?;
            } else {
                write!(f, "{vtag}")?;
            }
        }

        // Tag mode (only show if non-default or multiple tags)
        if self.criteria.tags.len() > 1 || !self.criteria.excludes.is_empty() {
            match self.criteria.tag_mode {
                TagMode::Any => write!(f, " --any-tag")?,
                TagMode::All => write!(f, " --all-tags")?,
            }
        }

        // File mode (only show if non-default and patterns exist)
        if !self.criteria.file_patterns.is_empty()
            && self.criteria.file_patterns.len() > 1
            && self.criteria.file_mode == FileMode::All
        {
            write!(f, " --all-files")?;
        }

        // Virtual mode (only show if non-default and vtags exist)
        if !self.criteria.virtual_tags.is_empty()
            && self.criteria.virtual_tags.len() > 1
            && self.criteria.virtual_mode == TagMode::All
        {
            write!(f, " --all-virtual")?;
        }

        // Regex flags
        if self.criteria.regex_tag {
            write!(f, " --regex-tag")?;
        }
        if self.criteria.regex_file {
            write!(f, " --regex-file")?;
        }

        Ok(())
    }
}

/// Check if a string needs quoting in shell context
fn needs_quoting(s: &str) -> bool {
    s.chars().any(|c| {
        c.is_whitespace()
            || matches!(
                c,
                '$' | '"' | '\'' | '\\' | '&' | '|' | ';' | '(' | ')' | '<' | '>'
            )
    })
}

// Easy conversion from CLI args
impl From<SearchParams> for ActiveFilter {
    fn from(params: SearchParams) -> Self {
        Self {
            criteria: params.into(),
        }
    }
}

impl From<&SearchParams> for ActiveFilter {
    fn from(params: &SearchParams) -> Self {
        Self {
            criteria: params.into(),
        }
    }
}

// Easy conversion to SearchParams for database queries
impl From<&ActiveFilter> for SearchParams {
    fn from(filter: &ActiveFilter) -> Self {
        (&filter.criteria).into()
    }
}

// Allow direct access to criteria
impl AsRef<FilterCriteria> for ActiveFilter {
    fn as_ref(&self) -> &FilterCriteria {
        &self.criteria
    }
}

impl AsMut<FilterCriteria> for ActiveFilter {
    fn as_mut(&mut self) -> &mut FilterCriteria {
        &mut self.criteria
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_include_tag() {
        let mut filter = ActiveFilter::new();

        filter.include_tag("rust".to_string());
        assert!(filter.is_included("rust"));
        assert!(!filter.is_excluded("rust"));
        assert_eq!(filter.criteria.tags.len(), 1);

        // Adding same tag again should not duplicate
        filter.include_tag("rust".to_string());
        assert_eq!(filter.criteria.tags.len(), 1);
    }

    #[test]
    fn test_exclude_tag() {
        let mut filter = ActiveFilter::new();

        filter.exclude_tag("python".to_string());
        assert!(filter.is_excluded("python"));
        assert!(!filter.is_included("python"));
        assert_eq!(filter.criteria.excludes.len(), 1);
    }

    #[test]
    fn test_toggle_between_include_exclude() {
        let mut filter = ActiveFilter::new();

        // Include tag
        filter.include_tag("rust".to_string());
        assert!(filter.is_included("rust"));
        assert_eq!(filter.criteria.tags.len(), 1);
        assert_eq!(filter.criteria.excludes.len(), 0);

        // Exclude same tag - should move from include to exclude
        filter.exclude_tag("rust".to_string());
        assert!(!filter.is_included("rust"));
        assert!(filter.is_excluded("rust"));
        assert_eq!(filter.criteria.tags.len(), 0);
        assert_eq!(filter.criteria.excludes.len(), 1);

        // Include again - should move back
        filter.include_tag("rust".to_string());
        assert!(filter.is_included("rust"));
        assert!(!filter.is_excluded("rust"));
        assert_eq!(filter.criteria.tags.len(), 1);
        assert_eq!(filter.criteria.excludes.len(), 0);
    }

    #[test]
    fn test_remove_tag() {
        let mut filter = ActiveFilter::new();

        filter.include_tag("rust".to_string());
        filter.exclude_tag("python".to_string());

        assert!(filter.is_included("rust"));
        assert!(filter.is_excluded("python"));

        filter.remove_tag("rust");
        assert!(!filter.is_included("rust"));
        assert!(!filter.is_excluded("rust"));

        filter.remove_tag("python");
        assert!(!filter.is_included("python"));
        assert!(!filter.is_excluded("python"));
    }

    #[test]
    fn test_toggle_mode() {
        let mut filter = ActiveFilter::new();

        assert_eq!(filter.criteria.tag_mode, TagMode::All);
        filter.toggle_mode();
        assert_eq!(filter.criteria.tag_mode, TagMode::Any);
        filter.toggle_mode();
        assert_eq!(filter.criteria.tag_mode, TagMode::All);
    }

    #[test]
    fn test_is_empty() {
        let mut filter = ActiveFilter::new();
        assert!(filter.is_empty());

        filter.include_tag("rust".to_string());
        assert!(!filter.is_empty());

        filter.remove_tag("rust");
        assert!(filter.is_empty());

        filter.add_file_pattern("*.rs".to_string());
        assert!(!filter.is_empty());
    }

    #[test]
    fn test_display_simple() {
        let mut filter = ActiveFilter::new();
        filter.include_tag("rust".to_string());

        let display = format!("{filter}");
        assert_eq!(display, "tagr search -t rust");
    }

    #[test]
    fn test_display_with_excludes() {
        let mut filter = ActiveFilter::new();
        filter.include_tag("rust".to_string());
        filter.exclude_tag("python".to_string());
        filter.criteria.tag_mode = TagMode::Any;

        let display = format!("{filter}");
        assert!(display.contains("-t rust"));
        assert!(display.contains("-x python"));
        assert!(display.contains("--any-tag"));
    }

    #[test]
    fn test_display_with_patterns() {
        let mut filter = ActiveFilter::new();
        filter.include_tag("rust".to_string());
        filter.add_file_pattern("*.rs".to_string());
        filter.add_virtual_tag("size:>1MB".to_string());

        let display = format!("{filter}");
        eprintln!("Display output: {}", display);
        assert!(display.contains("-t rust"));
        assert!(display.contains("-p *.rs"));
        assert!(display.contains("-v \"size:>1MB\"") || display.contains("-v size:>1MB"));
    }

    #[test]
    fn test_display_with_special_chars() {
        let mut filter = ActiveFilter::new();
        filter.include_tag("my tag with spaces".to_string());
        filter.add_file_pattern("src/**/*.rs".to_string());

        let display = format!("{filter}");
        assert!(display.contains("\"my tag with spaces\""));
    }

    #[test]
    fn test_needs_quoting() {
        assert!(needs_quoting("hello world"));
        assert!(needs_quoting("$var"));
        assert!(needs_quoting("a & b"));
        assert!(!needs_quoting("simple"));
        assert!(!needs_quoting("hyphen-underscore_123"));
    }
}
