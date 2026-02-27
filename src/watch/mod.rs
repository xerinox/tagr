//! Watch mode configuration and rule definitions.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;
use crate::filters::FilterCriteria;

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

    /// Optional saved filter name to apply (e.g., "markdown-files").
    /// This is resolved to a FilterCriteria at daemon startup.
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
    pub filter_criteria: Option<FilterCriteria>,
}

/// Configuration structure stored in ~/.config/tagr/watch.toml
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WatchConfig {
    /// List of watch rules
    #[serde(default)]
    pub rules: Vec<WatchRule>,
}

impl WatchConfig {
    /// Load configuration from default location
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&path)?;
        let config: WatchConfig = toml::from_str(&content)?;
        Ok(config)
    }

    /// Save configuration to default location
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Append a new rule and save
    pub fn append_rule(&mut self, rule: WatchRule) -> Result<()> {
        self.rules.push(rule);
        self.save()
    }
    
    /// Get the path to watch.toml
    pub fn config_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| WatchError::LoadError("Could not determine config directory".to_string()))?;
        Ok(config_dir.join("tagr").join("watch.toml"))
    }
}
