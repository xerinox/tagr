//! Filter data structures and types
//!
//! This module defines the core data structures for saved filters:
//! - `FilterCriteria`: The search criteria (tags, file patterns, exclusions, etc.)
//! - `FilterMetadata`: Metadata about filter usage and creation
//! - `Filter`: Complete filter with criteria and metadata
//! - `FilterStorage`: Container for all filters

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::cli::SearchMode;

/// Filter criteria representing search parameters
///
/// This matches the search/browse command parameters and can be serialized to TOML.
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
}

impl FilterCriteria {
    /// Create a new filter criteria
    pub fn new() -> Self {
        Self::default()
    }

    /// Create from search mode parameters
    pub fn from_search_params(
        tags: Vec<String>,
        tag_mode: SearchMode,
        file_patterns: Vec<String>,
        file_mode: SearchMode,
        excludes: Vec<String>,
        regex_tag: bool,
        regex_file: bool,
    ) -> Self {
        Self {
            tags,
            tag_mode: tag_mode.into(),
            file_patterns,
            file_mode: file_mode.into(),
            excludes,
            regex_tag,
            regex_file,
        }
    }

    /// Merge with additional criteria (for combining loaded filter with CLI args)
    ///
    /// CLI arguments extend or override the saved filter:
    /// - Tags and file patterns are added
    /// - Exclusions are merged
    /// - Regex flags are OR'd (if either is true, use regex)
    pub fn merge(&mut self, other: &FilterCriteria) {
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
    pub fn validate(&self) -> Result<(), String> {
        if self.tags.is_empty() && self.file_patterns.is_empty() {
            return Err("Filter must specify at least one tag or file pattern".to_string());
        }

        if self.regex_tag {
            for tag in &self.tags {
                if regex::Regex::new(tag).is_err() {
                    return Err(format!("Invalid regex pattern for tag: {}", tag));
                }
            }
        }

        if self.regex_file {
            for pattern in &self.file_patterns {
                if regex::Regex::new(pattern).is_err() {
                    return Err(format!("Invalid regex pattern for file: {}", pattern));
                }
            }
        }

        Ok(())
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
        }
    }
}

/// Tag matching mode (ALL = AND, ANY = OR)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TagMode {
    /// Match ALL tags (AND logic)
    All,
    /// Match ANY tag (OR logic)
    Any,
}

impl Default for TagMode {
    fn default() -> Self {
        Self::All
    }
}

impl From<SearchMode> for TagMode {
    fn from(mode: SearchMode) -> Self {
        match mode {
            SearchMode::All => TagMode::All,
            SearchMode::Any => TagMode::Any,
        }
    }
}

impl From<TagMode> for SearchMode {
    fn from(mode: TagMode) -> Self {
        match mode {
            TagMode::All => SearchMode::All,
            TagMode::Any => SearchMode::Any,
        }
    }
}

/// File pattern matching mode (ALL = AND, ANY = OR)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FileMode {
    /// Match ALL patterns (AND logic)
    All,
    /// Match ANY pattern (OR logic)
    Any,
}

impl Default for FileMode {
    fn default() -> Self {
        Self::Any
    }
}

impl From<SearchMode> for FileMode {
    fn from(mode: SearchMode) -> Self {
        match mode {
            SearchMode::All => FileMode::All,
            SearchMode::Any => FileMode::Any,
        }
    }
}

impl From<FileMode> for SearchMode {
    fn from(mode: FileMode) -> Self {
        match mode {
            FileMode::All => SearchMode::All,
            FileMode::Any => SearchMode::Any,
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
    pub fn new(description: String) -> Self {
        let now = Utc::now();
        Self {
            description,
            created: now,
            last_used: now,
            use_count: 0,
        }
    }

    /// Update usage statistics (increment count, update last_used)
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
    pub fn new() -> Self {
        Self {
            filters: Vec::new(),
        }
    }

    /// Get a filter by name
    pub fn get(&self, name: &str) -> Option<&Filter> {
        self.filters.iter().find(|f| f.name == name)
    }

    /// Get a mutable filter by name
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Filter> {
        self.filters.iter_mut().find(|f| f.name == name)
    }

    /// Check if a filter exists
    pub fn contains(&self, name: &str) -> bool {
        self.filters.iter().any(|f| f.name == name)
    }

    /// Add a filter (returns error if name already exists)
    pub fn add(&mut self, filter: Filter) -> Result<(), String> {
        if self.contains(&filter.name) {
            return Err(format!("Filter '{}' already exists", filter.name));
        }
        filter.validate()?;
        self.filters.push(filter);
        Ok(())
    }

    /// Update an existing filter
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
    pub fn list_names(&self) -> Vec<&str> {
        self.filters.iter().map(|f| f.name.as_str()).collect()
    }

    /// Get filters sorted by use count (most used first)
    pub fn most_used(&self) -> Vec<&Filter> {
        let mut sorted: Vec<&Filter> = self.filters.iter().collect();
        sorted.sort_by(|a, b| b.use_count.cmp(&a.use_count));
        sorted
    }

    /// Get filters sorted by last used (most recent first)
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
pub fn validate_filter_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Filter name cannot be empty".to_string());
    }

    if name.len() > 64 {
        return Err(format!("Filter name too long (max 64 chars): {}", name.len()));
    }

    // Check for valid characters: alphanumeric, hyphen, underscore
    if !name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return Err(format!(
            "Filter name '{}' contains invalid characters (only alphanumeric, '-', and '_' allowed)",
            name
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
        };

        let additional = FilterCriteria {
            tags: vec!["tutorial".to_string()],
            tag_mode: TagMode::Any, // This should be ignored
            file_patterns: vec!["*.md".to_string()],
            file_mode: FileMode::All, // This should be ignored
            excludes: vec!["deprecated".to_string()],
            regex_tag: true,
            regex_file: false,
        };

        base.merge(&additional);

        assert_eq!(base.tags.len(), 2);
        assert!(base.tags.contains(&"rust".to_string()));
        assert!(base.tags.contains(&"tutorial".to_string()));
        assert_eq!(base.tag_mode, TagMode::All); // Original mode preserved
        assert_eq!(base.file_patterns.len(), 2);
        assert_eq!(base.excludes.len(), 2);
        assert_eq!(base.regex_tag, true); // OR'd
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
