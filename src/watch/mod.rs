//! Watch mode configuration and rule definitions.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;
use crate::types::QueryCriteria;

pub mod matcher;

#[derive(Debug, Error)]
pub enum WatchError {
    #[error("Failed to load watch config: {0}")]
    LoadError(String),

    #[error("Failed to save watch config: {0}")]
    SaveError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("TOML serialization error: {0}")]
    TomlError(#[from] toml::ser::Error),
    
    #[error("TOML deserialization error: {0}")]
    TomlDeError(#[from] toml::de::Error),
}

pub type Result<T> = std::result::Result<T, WatchError>;

/// A single watch rule defining patterns to match and tags to apply.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WatchRule {
    /// Glob patterns to match (e.g., "~/docs/*.md")
    pub patterns: Vec<String>,

    /// Tags to apply when files match
    pub tags: Vec<String>,

    /// Optional saved filter name to apply (e.g., `"markdown-files"`).
    /// This is resolved to a `QueryCriteria` at daemon startup.
    pub filter: Option<String>,

    /// Inline virtual-tag conditions (e.g., `["size:small", "modified:today"]`).
    /// Only files satisfying ALL conditions trigger the rule.
    #[serde(default)]
    pub vtags: Vec<String>,

    /// DB-tag gate: only trigger the rule when the file already carries ALL of
    /// these tags in the database (e.g., `["docs"]`).
    #[serde(default)]
    pub filter_by_tags: Vec<String>,

    /// Resolved filter criteria (internal use, not serialized).
    /// Populated from `filter`, `vtags`, and `filter_by_tags` at daemon startup.
    #[serde(skip)]
    pub filter_criteria: Option<QueryCriteria>,
}

/// Configuration structure stored in ~/.config/tagr/watch.toml
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WatchConfig {
    /// List of watch rules
    #[serde(default)]
    pub rules: Vec<WatchRule>,
}

impl WatchConfig {
    /// Load configuration from default location.
    ///
    /// # Errors
    /// Returns `WatchError` if the config directory cannot be determined,
    /// or if reading/parsing the file fails.
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&path)?;
        let config: Self = toml::from_str(&content)?;
        Ok(config)
    }

    /// Save configuration to default location.
    ///
    /// # Errors
    /// Returns `WatchError` if the config directory cannot be determined,
    /// or if creating directories or writing the file fails.
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Append a new rule and save.
    ///
    /// # Errors
    /// Returns `WatchError` if saving the configuration fails.
    pub fn append_rule(&mut self, rule: WatchRule) -> Result<()> {
        self.rules.push(rule);
        self.save()
    }
    
    /// Get the path to `watch.toml`.
    ///
    /// # Errors
    /// Returns `WatchError::LoadError` if the config directory cannot be determined.
    pub fn config_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| WatchError::LoadError("Could not determine config directory".to_string()))?;
        Ok(config_dir.join("tagr").join("watch.toml"))
    }

    /// Load configuration from a specific path (useful for testing).
    ///
    /// # Errors
    /// Returns `WatchError` if reading or parsing the file fails.
    pub fn load_from(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path)?;
        let config: Self = toml::from_str(&content)?;
        Ok(config)
    }

    /// Save configuration to a specific path (useful for testing).
    ///
    /// # Errors
    /// Returns `WatchError` if creating directories or writing the file fails.
    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_rule() -> WatchRule {
        WatchRule {
            patterns: vec!["~/docs/*.md".to_string()],
            tags: vec!["docs".to_string(), "markdown".to_string()],
            filter: None,
            vtags: vec![],
            filter_by_tags: vec![],
            filter_criteria: None,
        }
    }

    #[test]
    fn test_watch_config_default_is_empty() {
        let config = WatchConfig::default();
        assert!(config.rules.is_empty());
    }

    #[test]
    fn test_watch_rule_serde_round_trip() {
        let rule = sample_rule();
        let toml_str = toml::to_string_pretty(&rule).unwrap();
        let deserialized: WatchRule = toml::from_str(&toml_str).unwrap();

        assert_eq!(rule.patterns, deserialized.patterns);
        assert_eq!(rule.tags, deserialized.tags);
        assert_eq!(rule.filter, deserialized.filter);
        // filter_criteria is #[serde(skip)] so it should be None after deser
        assert!(deserialized.filter_criteria.is_none());
    }

    #[test]
    fn test_watch_rule_with_all_fields() {
        let rule = WatchRule {
            patterns: vec!["/tmp/*.rs".to_string()],
            tags: vec!["rust".to_string()],
            filter: Some("my-filter".to_string()),
            vtags: vec!["size:small".to_string()],
            filter_by_tags: vec!["code".to_string()],
            filter_criteria: None,
        };

        let toml_str = toml::to_string_pretty(&rule).unwrap();
        let deserialized: WatchRule = toml::from_str(&toml_str).unwrap();

        assert_eq!(deserialized.filter, Some("my-filter".to_string()));
        assert_eq!(deserialized.vtags, vec!["size:small".to_string()]);
        assert_eq!(deserialized.filter_by_tags, vec!["code".to_string()]);
    }

    #[test]
    fn test_watch_config_serde_round_trip() {
        let config = WatchConfig {
            rules: vec![sample_rule(), sample_rule()],
        };

        let toml_str = toml::to_string_pretty(&config).unwrap();
        let deserialized: WatchConfig = toml::from_str(&toml_str).unwrap();

        assert_eq!(deserialized.rules.len(), 2);
        assert_eq!(deserialized.rules[0].patterns, config.rules[0].patterns);
    }

    #[test]
    fn test_watch_config_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("watch.toml");

        let config = WatchConfig {
            rules: vec![sample_rule()],
        };
        config.save_to(&path).unwrap();

        let loaded = WatchConfig::load_from(&path).unwrap();
        assert_eq!(loaded.rules.len(), 1);
        assert_eq!(loaded.rules[0].tags, vec!["docs", "markdown"]);
    }

    #[test]
    fn test_watch_config_load_nonexistent_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.toml");

        let config = WatchConfig::load_from(&path).unwrap();
        assert!(config.rules.is_empty());
    }

    #[test]
    fn test_watch_config_save_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("dir").join("watch.toml");

        let config = WatchConfig {
            rules: vec![sample_rule()],
        };
        config.save_to(&path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn test_watch_rule_optional_fields_default() {
        let toml_str = r#"
            patterns = ["/tmp/*.txt"]
            tags = ["text"]
        "#;

        let rule: WatchRule = toml::from_str(toml_str).unwrap();
        assert!(rule.filter.is_none());
        assert!(rule.vtags.is_empty());
        assert!(rule.filter_by_tags.is_empty());
        assert!(rule.filter_criteria.is_none());
    }

    #[test]
    fn test_watch_config_path_ends_with_watch_toml() {
        // May fail in environments without a config dir, but should work in CI/dev
        if let Ok(path) = WatchConfig::config_path() {
            assert!(path.ends_with("tagr/watch.toml"));
        }
    }
}
