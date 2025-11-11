//! Filter data structures and types
//!
//! This module defines the core data structures for saved filters:
//! - `FilterCriteria`: The search criteria (tags, file patterns, exclusions, etc.)
//! - `FilterMetadata`: Metadata about filter usage and creation
//! - `Filter`: Complete filter with criteria and metadata
//! - `FilterStorage`: Container for all filters

use crate::cli::SearchMode;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Filter criteria representing search parameters
///
/// This matches the search/browse command parameters and can be serialized to TOML.
///
/// # Future Enhancement
///
/// TODO: Consider refactoring to structured criteria for composable queries:
/// - `TagCriterion { text: String, exclude: bool, regex: bool }`
/// - `FileCriterion { pattern: String, exclude: bool, regex: bool }`
/// - Support expression trees: `(tag1 AND tag2) OR (file1 AND NOT tag3)`
///
/// This would enable complex filter expressions like:
/// ```text
/// (tag matching regex ".*doc.*" excluding "doctor") AND
/// (tag matching either "text" OR "code") OR
/// (file name "*.md")
/// ```
///
/// The current flat structure handles most use cases well. Consider this evolution
/// when users request complex query expressions or when building a filter DSL.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FilterCriteria {
    /// Tags to search for
    #[serde(default)]
    pub tags: Vec<String>,

    /// How to combine multiple tags ("all" = AND, "any" = OR)
    #[serde(default)]
    pub tag_mode: TagMode,

    /// File patterns to filter by (glob or regex)
    #[serde(default)]
    pub file_patterns: Vec<String>,

    /// How to combine multiple file patterns ("all" = AND, "any" = OR)
    #[serde(default)]
    pub file_mode: FileMode,

    /// Tags to exclude
    #[serde(default)]
    pub excludes: Vec<String>,

    /// Use regex for tag matching
    #[serde(default)]
    pub regex_tag: bool,

    /// Use regex for file pattern matching
    #[serde(default)]
    pub regex_file: bool,

    /// Virtual tags to filter by (e.g., "size:>1MB", "modified:today")
    #[serde(default)]
    pub virtual_tags: Vec<String>,

    /// How to combine multiple virtual tags ("all" = AND, "any" = OR)
    #[serde(default)]
    pub virtual_mode: TagMode,
}

impl FilterCriteria {
    /// Create a new filter criteria builder
    #[must_use]
    pub fn builder() -> FilterCriteriaBuilder {
        FilterCriteriaBuilder::default()
    }

    /// Create a new filter criteria (same as `builder().build()`)
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Merge with additional criteria (for combining loaded filter with CLI args)
    ///
    /// CLI arguments extend or override the saved filter:
    /// - Tags and file patterns are added
    /// - Exclusions are merged
    /// - Regex flags are OR'd (if either is true, use regex)
    pub fn merge(&mut self, other: &Self) {
        for tag in &other.tags {
            if !self.tags.contains(tag) {
                self.tags.push(tag.clone());
            }
        }

        for pattern in &other.file_patterns {
            if !self.file_patterns.contains(pattern) {
                self.file_patterns.push(pattern.clone());
            }
        }

        for exclude in &other.excludes {
            if !self.excludes.contains(exclude) {
                self.excludes.push(exclude.clone());
            }
        }

        self.regex_tag = self.regex_tag || other.regex_tag;
        self.regex_file = self.regex_file || other.regex_file;

        // Note: tag_mode and file_mode are NOT merged - the loaded filter's modes are preserved
        // unless the user explicitly provides mode flags in the CLI
    }

    /// Validate the criteria
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - No tags or file patterns are specified
    /// - Regex patterns are invalid when regex mode is enabled
    pub fn validate(&self) -> Result<(), String> {
        if self.tags.is_empty() && self.file_patterns.is_empty() {
            return Err("Filter must specify at least one tag or file pattern".to_string());
        }

        if self.regex_tag {
            for tag in &self.tags {
                if regex::Regex::new(tag).is_err() {
                    return Err(format!("Invalid regex pattern for tag: {tag}"));
                }
            }
        }

        if self.regex_file {
            for pattern in &self.file_patterns {
                if regex::Regex::new(pattern).is_err() {
                    return Err(format!("Invalid regex pattern for file: {pattern}"));
                }
            }
        }

        Ok(())
    }
}

/// Builder for `FilterCriteria`
#[derive(Debug, Clone, Default)]
pub struct FilterCriteriaBuilder {
    tags: Vec<String>,
    tag_mode: Option<TagMode>,
    file_patterns: Vec<String>,
    file_mode: Option<FileMode>,
    excludes: Vec<String>,
    regex_tag: bool,
    regex_file: bool,
    virtual_tags: Vec<String>,
    virtual_mode: Option<TagMode>,
}

impl FilterCriteriaBuilder {
    /// Add tags to search for
    #[must_use]
    pub fn tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Add a single tag
    #[must_use]
    pub fn tag(mut self, tag: String) -> Self {
        self.tags.push(tag);
        self
    }

    /// Set how to combine multiple tags
    #[must_use]
    pub const fn tag_mode(mut self, mode: TagMode) -> Self {
        self.tag_mode = Some(mode);
        self
    }

    /// Add file patterns to filter by
    #[must_use]
    pub fn file_patterns(mut self, patterns: Vec<String>) -> Self {
        self.file_patterns = patterns;
        self
    }

    /// Add a single file pattern
    #[must_use]
    pub fn file_pattern(mut self, pattern: String) -> Self {
        self.file_patterns.push(pattern);
        self
    }

    /// Set how to combine multiple file patterns
    #[must_use]
    pub const fn file_mode(mut self, mode: FileMode) -> Self {
        self.file_mode = Some(mode);
        self
    }

    /// Add tags to exclude
    #[must_use]
    pub fn excludes(mut self, excludes: Vec<String>) -> Self {
        self.excludes = excludes;
        self
    }

    /// Add a single exclusion tag
    #[must_use]
    pub fn exclude(mut self, tag: String) -> Self {
        self.excludes.push(tag);
        self
    }

    /// Enable regex matching for tags
    #[must_use]
    pub const fn regex_tag(mut self, enabled: bool) -> Self {
        self.regex_tag = enabled;
        self
    }

    /// Enable regex matching for file patterns
    #[must_use]
    pub const fn regex_file(mut self, enabled: bool) -> Self {
        self.regex_file = enabled;
        self
    }

    /// Add virtual tags to filter by
    #[must_use]
    pub fn virtual_tags(mut self, tags: Vec<String>) -> Self {
        self.virtual_tags = tags;
        self
    }

    /// Add a single virtual tag
    #[must_use]
    pub fn virtual_tag(mut self, tag: String) -> Self {
        self.virtual_tags.push(tag);
        self
    }

    /// Set how to combine multiple virtual tags
    #[must_use]
    pub const fn virtual_mode(mut self, mode: TagMode) -> Self {
        self.virtual_mode = Some(mode);
        self
    }

    /// Build the `FilterCriteria`
    #[must_use]
    pub fn build(self) -> FilterCriteria {
        FilterCriteria {
            tags: self.tags,
            tag_mode: self.tag_mode.unwrap_or(TagMode::All),
            file_patterns: self.file_patterns,
            file_mode: self.file_mode.unwrap_or(FileMode::Any),
            excludes: self.excludes,
            regex_tag: self.regex_tag,
            regex_file: self.regex_file,
            virtual_tags: self.virtual_tags,
            virtual_mode: self.virtual_mode.unwrap_or(TagMode::All),
        }
    }
}

impl Default for FilterCriteria {
    fn default() -> Self {
        Self {
            tags: Vec::new(),
            tag_mode: TagMode::All,
            file_patterns: Vec::new(),
            file_mode: FileMode::Any,
            excludes: Vec::new(),
            regex_tag: false,
            regex_file: false,
            virtual_tags: Vec::new(),
            virtual_mode: TagMode::All,
        }
    }
}

/// Tag matching mode (ALL = AND, ANY = OR)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum TagMode {
    /// Match ALL tags (AND logic)
    #[default]
    All,
    /// Match ANY tag (OR logic)
    Any,
}

impl From<SearchMode> for TagMode {
    fn from(mode: SearchMode) -> Self {
        match mode {
            SearchMode::All => Self::All,
            SearchMode::Any => Self::Any,
        }
    }
}

impl From<TagMode> for SearchMode {
    fn from(mode: TagMode) -> Self {
        match mode {
            TagMode::All => Self::All,
            TagMode::Any => Self::Any,
        }
    }
}

/// File pattern matching mode (ALL = AND, ANY = OR)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum FileMode {
    /// Match ALL patterns (AND logic)
    All,
    /// Match ANY pattern (OR logic)
    #[default]
    Any,
}

impl From<SearchMode> for FileMode {
    fn from(mode: SearchMode) -> Self {
        match mode {
            SearchMode::All => Self::All,
            SearchMode::Any => Self::Any,
        }
    }
}

impl From<FileMode> for SearchMode {
    fn from(mode: FileMode) -> Self {
        match mode {
            FileMode::All => Self::All,
            FileMode::Any => Self::Any,
        }
    }
}

/// Filter metadata (usage statistics and timestamps)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FilterMetadata {
    /// Human-readable description of the filter
    #[serde(default)]
    pub description: String,

    /// When the filter was created
    pub created: DateTime<Utc>,

    /// When the filter was last used
    pub last_used: DateTime<Utc>,

    /// Number of times the filter has been used
    #[serde(default)]
    pub use_count: u32,
}

impl FilterMetadata {
    /// Create new metadata with current timestamp
    #[must_use]
    pub fn new(description: String) -> Self {
        let now = Utc::now();
        Self {
            description,
            created: now,
            last_used: now,
            use_count: 0,
        }
    }

    /// Update usage statistics (increment count, update `last_used`)
    pub fn record_use(&mut self) {
        self.use_count += 1;
        self.last_used = Utc::now();
    }
}

/// Complete filter with name, criteria, and metadata
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Filter {
    /// Unique filter name
    pub name: String,

    /// Filter description (optional)
    #[serde(default)]
    pub description: String,

    /// When the filter was created
    pub created: DateTime<Utc>,

    /// When the filter was last used
    pub last_used: DateTime<Utc>,

    /// Number of times the filter has been used
    #[serde(default)]
    pub use_count: u32,

    /// The search criteria
    #[serde(rename = "criteria")]
    pub criteria: FilterCriteria,
}

impl Filter {
    /// Create a new filter
    #[must_use]
    pub fn new(name: String, description: String, criteria: FilterCriteria) -> Self {
        let now = Utc::now();
        Self {
            name,
            description,
            created: now,
            last_used: now,
            use_count: 0,
            criteria,
        }
    }

    /// Record that this filter was used
    pub fn record_use(&mut self) {
        self.use_count += 1;
        self.last_used = Utc::now();
    }

    /// Validate the filter
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The filter name is invalid
    /// - The filter criteria is invalid
    pub fn validate(&self) -> Result<(), String> {
        validate_filter_name(&self.name)?;
        self.criteria.validate()?;
        Ok(())
    }
}

/// Storage container for all filters
///
/// This is the root structure that gets serialized to TOML.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FilterStorage {
    /// All saved filters
    #[serde(rename = "filter")]
    pub filters: Vec<Filter>,
}

impl FilterStorage {
    /// Create a new empty filter storage
    #[must_use]
    pub const fn new() -> Self {
        Self {
            filters: Vec::new(),
        }
    }

    /// Get a filter by name
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Filter> {
        self.filters.iter().find(|f| f.name == name)
    }

    /// Get a mutable filter by name
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Filter> {
        self.filters.iter_mut().find(|f| f.name == name)
    }

    /// Check if a filter exists
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.filters.iter().any(|f| f.name == name)
    }

    /// Add a filter (returns error if name already exists)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - A filter with the same name already exists
    /// - The filter validation fails
    pub fn add(&mut self, filter: Filter) -> Result<(), String> {
        if self.contains(&filter.name) {
            return Err(format!("Filter '{}' already exists", filter.name));
        }
        filter.validate()?;
        self.filters.push(filter);
        Ok(())
    }

    /// Update an existing filter
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The filter is not found
    /// - The filter validation fails
    pub fn update(&mut self, filter: Filter) -> Result<(), String> {
        filter.validate()?;
        if let Some(existing) = self.get_mut(&filter.name) {
            *existing = filter;
            Ok(())
        } else {
            Err(format!("Filter '{}' not found", filter.name))
        }
    }

    /// Remove a filter by name
    pub fn remove(&mut self, name: &str) -> Option<Filter> {
        if let Some(pos) = self.filters.iter().position(|f| f.name == name) {
            Some(self.filters.remove(pos))
        } else {
            None
        }
    }

    /// List all filter names
    #[must_use]
    pub fn list_names(&self) -> Vec<&str> {
        self.filters.iter().map(|f| f.name.as_str()).collect()
    }

    /// Get filters sorted by use count (most used first)
    #[must_use]
    pub fn most_used(&self) -> Vec<&Filter> {
        let mut sorted: Vec<&Filter> = self.filters.iter().collect();
        sorted.sort_by(|a, b| b.use_count.cmp(&a.use_count));
        sorted
    }

    /// Get filters sorted by last used (most recent first)
    #[must_use]
    pub fn recently_used(&self) -> Vec<&Filter> {
        let mut sorted: Vec<&Filter> = self.filters.iter().collect();
        sorted.sort_by(|a, b| b.last_used.cmp(&a.last_used));
        sorted
    }
}

/// Validate a filter name
///
/// Filter names must:
/// - Be 1-64 characters long
/// - Contain only alphanumeric characters, hyphens, and underscores
/// - Not be empty
///
/// # Errors
///
/// Returns an error if:
/// - The name is empty
/// - The name exceeds 64 characters
/// - The name contains invalid characters
pub fn validate_filter_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Filter name cannot be empty".to_string());
    }

    if name.len() > 64 {
        return Err(format!(
            "Filter name too long (max 64 chars): {}",
            name.len()
        ));
    }

    // Check for valid characters: alphanumeric, hyphen, underscore
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(format!(
            "Filter name '{name}' contains invalid characters (only alphanumeric, '-', and '_' allowed)"
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_filter_name() {
        assert!(validate_filter_name("valid-name").is_ok());
        assert!(validate_filter_name("valid_name_123").is_ok());
        assert!(validate_filter_name("ValidName").is_ok());

        assert!(validate_filter_name("").is_err());
        assert!(validate_filter_name("invalid name").is_err()); // space
        assert!(validate_filter_name("invalid.name").is_err()); // dot
        assert!(validate_filter_name(&"a".repeat(65)).is_err()); // too long
    }

    #[test]
    fn test_filter_criteria_default() {
        let criteria = FilterCriteria::default();
        assert!(criteria.tags.is_empty());
        assert!(criteria.file_patterns.is_empty());
        assert_eq!(criteria.tag_mode, TagMode::All);
        assert_eq!(criteria.file_mode, FileMode::Any);
    }

    #[test]
    fn test_filter_criteria_merge() {
        let mut base = FilterCriteria {
            tags: vec!["rust".to_string()],
            tag_mode: TagMode::All,
            file_patterns: vec!["*.rs".to_string()],
            file_mode: FileMode::Any,
            excludes: vec!["test".to_string()],
            regex_tag: false,
            regex_file: false,
            virtual_tags: Vec::new(),
            virtual_mode: TagMode::All,
        };

        let additional = FilterCriteria {
            tags: vec!["tutorial".to_string()],
            tag_mode: TagMode::Any, // This should be ignored
            file_patterns: vec!["*.md".to_string()],
            file_mode: FileMode::All, // This should be ignored
            excludes: vec!["deprecated".to_string()],
            regex_tag: true,
            regex_file: false,
            virtual_tags: vec!["size:>1MB".to_string()],
            virtual_mode: TagMode::All,
        };

        base.merge(&additional);

        assert_eq!(base.tags.len(), 2);
        assert!(base.tags.contains(&"rust".to_string()));
        assert!(base.tags.contains(&"tutorial".to_string()));
        assert_eq!(base.tag_mode, TagMode::All); // Original mode preserved
        assert_eq!(base.file_patterns.len(), 2);
        assert_eq!(base.excludes.len(), 2);
        assert!(base.regex_tag); // OR'd
    }

    #[test]
    fn test_filter_storage() {
        let mut storage = FilterStorage::new();

        let filter = Filter::new(
            "test-filter".to_string(),
            "Test filter".to_string(),
            FilterCriteria {
                tags: vec!["test".to_string()],
                ..Default::default()
            },
        );

        assert!(storage.add(filter.clone()).is_ok());
        assert!(storage.contains("test-filter"));
        assert_eq!(storage.filters.len(), 1);

        // Try to add duplicate
        assert!(storage.add(filter).is_err());

        // Remove filter
        let removed = storage.remove("test-filter");
        assert!(removed.is_some());
        assert!(!storage.contains("test-filter"));
    }

    #[test]
    fn test_filter_serialization() {
        let filter = Filter::new(
            "rust-tutorials".to_string(),
            "Find Rust tutorial files".to_string(),
            FilterCriteria {
                tags: vec!["rust".to_string(), "tutorial".to_string()],
                tag_mode: TagMode::All,
                file_patterns: vec!["*.rs".to_string()],
                file_mode: FileMode::Any,
                excludes: vec![],
                regex_tag: false,
                regex_file: false,
                virtual_tags: Vec::new(),
                virtual_mode: TagMode::All,
            },
        );

        let mut storage = FilterStorage::new();
        storage.filters.push(filter);

        // Serialize to TOML
        let toml = toml::to_string_pretty(&storage).unwrap();
        assert!(toml.contains("rust-tutorials"));
        assert!(toml.contains("rust"));
        assert!(toml.contains("tutorial"));

        // Deserialize back
        let deserialized: FilterStorage = toml::from_str(&toml).unwrap();
        assert_eq!(deserialized.filters.len(), 1);
        assert_eq!(deserialized.filters[0].name, "rust-tutorials");
    }
}

impl std::fmt::Display for Filter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Filter: {}", self.name)?;

        if !self.description.is_empty() {
            writeln!(f, "Description: {}", self.description)?;
        }

        writeln!(f)?;
        write!(f, "{}", self.criteria)?;

        writeln!(f)?;
        writeln!(f, "Created: {}", self.created.format("%Y-%m-%d %H:%M:%S"))?;
        writeln!(
            f,
            "Last Used: {}",
            self.last_used.format("%Y-%m-%d %H:%M:%S")
        )?;
        writeln!(f, "Use Count: {}", self.use_count)?;

        Ok(())
    }
}

impl std::fmt::Display for FilterCriteria {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Tags
        if self.tags.is_empty() {
            writeln!(f, "Tags: (none)")?;
        } else {
            writeln!(
                f,
                "Tags: {} ({})",
                self.tags.join(", "),
                match self.tag_mode {
                    TagMode::All => "ALL",
                    TagMode::Any => "ANY",
                }
            )?;
        }

        // File patterns
        if self.file_patterns.is_empty() {
            writeln!(f, "File Patterns: (none)")?;
        } else {
            writeln!(
                f,
                "File Patterns: {} ({})",
                self.file_patterns.join(", "),
                match self.file_mode {
                    FileMode::All => "ALL",
                    FileMode::Any => "ANY",
                }
            )?;
        }

        // Excludes
        if !self.excludes.is_empty() {
            writeln!(f, "Excludes: {}", self.excludes.join(", "))?;
        }

        // Virtual tags
        if !self.virtual_tags.is_empty() {
            writeln!(
                f,
                "Virtual Tags: {} ({})",
                self.virtual_tags.join(", "),
                match self.virtual_mode {
                    TagMode::All => "ALL",
                    TagMode::Any => "ANY",
                }
            )?;
        }

        // Regex modes
        if self.regex_tag || self.regex_file {
            let mut regex_modes = Vec::new();
            if self.regex_tag {
                regex_modes.push("tags");
            }
            if self.regex_file {
                regex_modes.push("files");
            }
            writeln!(f, "Regex Mode: {}", regex_modes.join(", "))?;
        }

        Ok(())
    }
}
