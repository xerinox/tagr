//! Filter data structures and types
//!
//! This module defines the core data structures for saved filters:
//! - [`SavedFilter`]: Public filter type with [`QueryCriteria`] field
//! - [`FilterStorage`]: Container for all filters (TOML-serializable)
//!
//! Internally, `FilterCriteria` / `TagMode` / `FileMode` are kept as
//! private serde helpers to maintain backward-compatible TOML format.

use crate::types::{MatchMode, QueryCriteria};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// TOML-level filter criteria — private serde helper for backward compat.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct FilterCriteria {
    #[serde(default)]
    pub(crate) tags: Vec<String>,
    #[serde(default)]
    pub(crate) tag_mode: TagMode,
    #[serde(default)]
    pub(crate) file_patterns: Vec<String>,
    #[serde(default)]
    pub(crate) file_mode: FileMode,
    #[serde(default)]
    pub(crate) excludes: Vec<String>,
    #[serde(default)]
    pub(crate) regex_tag: bool,
    #[serde(default)]
    pub(crate) regex_file: bool,
    #[serde(default)]
    pub(crate) glob_files: bool,
    #[serde(default)]
    pub(crate) virtual_tags: Vec<String>,
    #[serde(default)]
    pub(crate) virtual_mode: TagMode,
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
            glob_files: false,
            virtual_tags: Vec::new(),
            virtual_mode: TagMode::All,
        }
    }
}

/// Tag matching mode — private serde helper.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub(crate) enum TagMode {
    /// Match ALL tags (AND logic)
    #[default]
    All,
    /// Match ANY tag (OR logic)
    Any,
}

impl From<MatchMode> for TagMode {
    fn from(mode: MatchMode) -> Self {
        match mode {
            MatchMode::All => Self::All,
            MatchMode::Any => Self::Any,
        }
    }
}

impl From<TagMode> for MatchMode {
    fn from(mode: TagMode) -> Self {
        match mode {
            TagMode::All => Self::All,
            TagMode::Any => Self::Any,
        }
    }
}

/// File pattern matching mode — private serde helper.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub(crate) enum FileMode {
    /// Match ALL patterns (AND logic)
    All,
    /// Match ANY pattern (OR logic)
    #[default]
    Any,
}

impl From<MatchMode> for FileMode {
    fn from(mode: MatchMode) -> Self {
        match mode {
            MatchMode::All => Self::All,
            MatchMode::Any => Self::Any,
        }
    }
}

impl From<FileMode> for MatchMode {
    fn from(mode: FileMode) -> Self {
        match mode {
            FileMode::All => Self::All,
            FileMode::Any => Self::Any,
        }
    }
}

impl From<&FilterCriteria> for crate::types::QueryCriteria {
    fn from(fc: &FilterCriteria) -> Self {
        use crate::types::{MatchMode, TagExpr, TagName};

        let mode = match fc.tag_mode {
            TagMode::All => MatchMode::All,
            TagMode::Any => MatchMode::Any,
        };

        let include_exprs: Vec<TagExpr> = fc
            .tags
            .iter()
            .filter_map(|t| TagName::new(t).ok().map(TagExpr::Tag))
            .collect();

        let exclude_exprs: Vec<TagExpr> = fc
            .excludes
            .iter()
            .filter_map(|t| {
                TagName::new(t)
                    .ok()
                    .map(|tn| TagExpr::Not(Box::new(TagExpr::Tag(tn))))
            })
            .collect();

        let mut all_exprs = include_exprs;
        all_exprs.extend(exclude_exprs);

        let tag_expr = match all_exprs.len() {
            0 => None,
            1 => all_exprs.into_iter().next(),
            _ => match mode {
                MatchMode::All => Some(TagExpr::And(all_exprs)),
                MatchMode::Any => {
                    let (includes, excludes): (Vec<_>, Vec<_>) = all_exprs
                        .into_iter()
                        .partition(|e| !matches!(e, TagExpr::Not(_)));
                    if excludes.is_empty() {
                        Some(TagExpr::Or(includes))
                    } else if includes.is_empty() {
                        Some(TagExpr::And(excludes))
                    } else {
                        let include_expr = if includes.len() == 1 {
                            includes.into_iter().next().unwrap_or_else(|| unreachable!())
                        } else {
                            TagExpr::Or(includes)
                        };
                        let mut combined = vec![include_expr];
                        combined.extend(excludes);
                        Some(TagExpr::And(combined))
                    }
                }
            },
        };

        Self {
            tag_expr,
            regex_tags: fc.regex_tag,
            expand_hierarchy: true,
            file_patterns: fc.file_patterns.clone(),
            file_mode: match fc.file_mode {
                FileMode::All => MatchMode::All,
                FileMode::Any => MatchMode::Any,
            },
            regex_files: fc.regex_file,
            virtual_tags: fc.virtual_tags.clone(),
            virtual_mode: match fc.virtual_mode {
                TagMode::All => MatchMode::All,
                TagMode::Any => MatchMode::Any,
            },
            query: None,
        }
    }
}

impl From<&crate::types::QueryCriteria> for FilterCriteria {
    fn from(qc: &crate::types::QueryCriteria) -> Self {
        let (tags, excludes) = extract_flat_tags(qc);

        let tag_mode = match &qc.tag_expr {
            Some(crate::types::TagExpr::Or(_)) => TagMode::Any,
            _ => TagMode::All,
        };

        Self {
            tags,
            tag_mode,
            file_patterns: qc.file_patterns.clone(),
            file_mode: match qc.file_mode {
                crate::types::MatchMode::All => FileMode::All,
                crate::types::MatchMode::Any => FileMode::Any,
            },
            excludes,
            regex_tag: qc.regex_tags,
            regex_file: qc.regex_files,
            glob_files: !qc.regex_files,
            virtual_tags: qc.virtual_tags.clone(),
            virtual_mode: match qc.virtual_mode {
                crate::types::MatchMode::All => TagMode::All,
                crate::types::MatchMode::Any => TagMode::Any,
            },
        }
    }
}

/// Extract flat include/exclude tag lists from a `QueryCriteria`.
fn extract_flat_tags(qc: &crate::types::QueryCriteria) -> (Vec<String>, Vec<String>) {
    let mut includes = Vec::new();
    let mut excludes = Vec::new();

    if let Some(ref expr) = qc.tag_expr {
        collect_flat_tags(expr, &mut includes, &mut excludes);
    }

    (includes, excludes)
}

fn collect_flat_tags(
    expr: &crate::types::TagExpr,
    includes: &mut Vec<String>,
    excludes: &mut Vec<String>,
) {
    match expr {
        crate::types::TagExpr::Tag(t) => includes.push(t.to_string()),
        crate::types::TagExpr::Not(inner) => {
            if let crate::types::TagExpr::Tag(t) = inner.as_ref() {
                excludes.push(t.to_string());
            }
        }
        crate::types::TagExpr::And(exprs) | crate::types::TagExpr::Or(exprs) => {
            for e in exprs {
                collect_flat_tags(e, includes, excludes);
            }
        }
    }
}

/// Raw filter — private TOML serde type. Public API uses [`SavedFilter`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct RawSavedFilter {
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) description: String,
    pub(crate) created: DateTime<Utc>,
    pub(crate) last_used: DateTime<Utc>,
    #[serde(default)]
    pub(crate) use_count: u32,
    #[serde(rename = "criteria")]
    pub(crate) criteria: FilterCriteria,
}

/// TOML root container — private serde type.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct RawFilterStorage {
    #[serde(rename = "filter")]
    pub(crate) filters: Vec<RawSavedFilter>,
}

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A saved filter with its search criteria and metadata.
///
/// The public API exposes [`QueryCriteria`] directly. The TOML
/// persistence format is handled internally via `FilterCriteria`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedFilter {
    /// Unique filter name
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// When the filter was created
    pub created: DateTime<Utc>,
    /// When the filter was last used
    pub last_used: DateTime<Utc>,
    /// Number of times the filter has been used
    pub use_count: u32,
    /// The search criteria
    pub criteria: QueryCriteria,
}

impl SavedFilter {
    /// Create a new saved filter from a name, description, and query criteria.
    #[must_use]
    pub fn new(name: String, description: String, criteria: QueryCriteria) -> Self {
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
    /// Returns an error string if the name or criteria are invalid.
    pub fn validate(&self) -> Result<(), String> {
        validate_filter_name(&self.name)?;
        // Convert to raw to validate criteria constraints
        let raw_criteria = FilterCriteria::from(&self.criteria);
        if raw_criteria.tags.is_empty() && raw_criteria.file_patterns.is_empty() {
            return Err("Filter must specify at least one tag or file pattern".to_string());
        }
        Ok(())
    }
}

impl std::fmt::Display for SavedFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Filter: {}", self.name)?;

        if !self.description.is_empty() {
            writeln!(f, "Description: {}", self.description)?;
        }

        writeln!(f)?;
        writeln!(f, "CLI: {}", self.criteria.to_cli_string())?;

        if !self.criteria.file_patterns.is_empty() {
            writeln!(
                f,
                "File Patterns: {} ({})",
                self.criteria.file_patterns.join(", "),
                self.criteria.file_mode,
            )?;
        }

        if !self.criteria.virtual_tags.is_empty() {
            writeln!(
                f,
                "Virtual Tags: {} ({})",
                self.criteria.virtual_tags.join(", "),
                self.criteria.virtual_mode,
            )?;
        }

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

/// Public storage container for filters (used by export to stdout).
///
/// For TOML serialization this delegates to [`RawFilterStorage`] internally.
#[derive(Debug, Clone, Default)]
pub struct FilterStorage {
    /// All saved filters
    pub filters: Vec<SavedFilter>,
}

impl FilterStorage {
    /// Create a new empty filter storage
    #[must_use]
    pub const fn new() -> Self {
        Self {
            filters: Vec::new(),
        }
    }
}

impl Serialize for FilterStorage {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let raw = RawFilterStorage {
            filters: self.filters.iter().map(raw_from_saved).collect(),
        };
        raw.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for FilterStorage {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = RawFilterStorage::deserialize(deserializer)?;
        Ok(Self {
            filters: raw.filters.into_iter().map(saved_from_raw).collect(),
        })
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

    // Validate against shell-unsafe characters; filenames should be shell-friendly
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

// ---------------------------------------------------------------------------
// Raw ↔ Saved conversion helpers
// ---------------------------------------------------------------------------

pub(crate) fn saved_from_raw(raw: RawSavedFilter) -> SavedFilter {
    let criteria = QueryCriteria::from(&raw.criteria);
    SavedFilter {
        name: raw.name,
        description: raw.description,
        created: raw.created,
        last_used: raw.last_used,
        use_count: raw.use_count,
        criteria,
    }
}

pub(crate) fn raw_from_saved(saved: &SavedFilter) -> RawSavedFilter {
    RawSavedFilter {
        name: saved.name.clone(),
        description: saved.description.clone(),
        created: saved.created,
        last_used: saved.last_used,
        use_count: saved.use_count,
        criteria: FilterCriteria::from(&saved.criteria),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{TagExpr, TagName};

    #[test]
    fn test_validate_filter_name() {
        assert!(validate_filter_name("valid-name").is_ok());
        assert!(validate_filter_name("valid_name_123").is_ok());
        assert!(validate_filter_name("ValidName").is_ok());

        assert!(validate_filter_name("").is_err());
        assert!(validate_filter_name("invalid name").is_err());
        assert!(validate_filter_name("invalid.name").is_err());
        assert!(validate_filter_name(&"a".repeat(65)).is_err());
    }

    #[test]
    fn test_filter_criteria_serde_defaults() {
        let criteria = FilterCriteria::default();
        assert!(criteria.tags.is_empty());
        assert!(criteria.file_patterns.is_empty());
        assert_eq!(criteria.tag_mode, TagMode::All);
        assert_eq!(criteria.file_mode, FileMode::Any);
    }

    #[test]
    fn test_saved_filter_roundtrip_via_toml() {
        let qc = QueryCriteria {
            tag_expr: Some(TagExpr::And(vec![
                TagExpr::Tag(TagName::new("rust").unwrap()),
                TagExpr::Tag(TagName::new("tutorial").unwrap()),
            ])),
            file_patterns: vec!["*.rs".to_string()],
            ..Default::default()
        };

        let saved = SavedFilter::new(
            "rust-tutorials".to_string(),
            "Find Rust tutorial files".to_string(),
            qc,
        );

        let mut storage = FilterStorage::new();
        storage.filters.push(saved);

        let toml = toml::to_string_pretty(&storage).unwrap();
        assert!(toml.contains("rust-tutorials"));
        assert!(toml.contains("rust"));
        assert!(toml.contains("tutorial"));

        let deserialized: FilterStorage = toml::from_str(&toml).unwrap();
        assert_eq!(deserialized.filters.len(), 1);
        assert_eq!(deserialized.filters[0].name, "rust-tutorials");
        // Tags survive the roundtrip
        let rt_tags = deserialized.filters[0]
            .criteria
            .flat_include_tags()
            .unwrap();
        assert_eq!(rt_tags.len(), 2);
    }

    #[test]
    fn test_saved_filter_validate() {
        let empty = SavedFilter::new(
            "empty".to_string(),
            String::new(),
            QueryCriteria::default(),
        );
        assert!(empty.validate().is_err());

        let valid = SavedFilter::new(
            "ok".to_string(),
            String::new(),
            QueryCriteria {
                tag_expr: Some(TagExpr::Tag(TagName::new("rust").unwrap())),
                ..Default::default()
            },
        );
        assert!(valid.validate().is_ok());
    }

    #[test]
    fn test_query_criteria_to_filter_criteria_roundtrip() {
        let qc = QueryCriteria {
            tag_expr: Some(TagExpr::And(vec![
                TagExpr::Tag(TagName::new("rust").unwrap()),
                TagExpr::Not(Box::new(TagExpr::Tag(TagName::new("deprecated").unwrap()))),
            ])),
            file_patterns: vec!["*.rs".to_string()],
            file_mode: MatchMode::Any,
            regex_tags: false,
            regex_files: false,
            virtual_tags: vec!["size:>1MB".to_string()],
            virtual_mode: MatchMode::All,
            ..Default::default()
        };

        let fc = FilterCriteria::from(&qc);
        assert!(fc.tags.contains(&"rust".to_string()));
        assert!(fc.excludes.contains(&"deprecated".to_string()));
        assert_eq!(fc.tag_mode, TagMode::All);

        let qc2 = QueryCriteria::from(&fc);
        // Tags survive
        let includes = qc2.flat_include_tags().unwrap();
        assert!(includes.contains(&TagName::new("rust").unwrap()));
        let excludes = qc2.flat_exclude_tags().unwrap();
        assert!(excludes.contains(&TagName::new("deprecated").unwrap()));
    }
}
