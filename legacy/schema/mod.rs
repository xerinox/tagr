//! Tag schema management for aliases and hierarchies
//!
//! This module provides the `TagSchema` type which manages:
//! - **Aliases**: Synonym mappings (e.g., "js" → "javascript")
//! - **Hierarchies**: Parent-child relationships using `:` delimiter (e.g., "lang:rust")
//!
//! The schema is persisted to `tag_schema.toml` in the config directory and can
//! be overridden by project-local `.tagr-config` files.
//!
//! # Examples
//!
//! ```no_run
//! use tagr::schema::TagSchema;
//! use std::path::Path;
//!
//! let mut schema = TagSchema::load(Path::new("tag_schema.toml"))?;
//!
//! // Add alias
//! schema.add_alias("js", "javascript")?;
//!
//! // Canonicalize tags
//! assert_eq!(schema.canonicalize("js"), "javascript");
//!
//! // Expand with synonyms
//! let synonyms = schema.expand_synonyms("javascript");
//! assert!(synonyms.contains(&"js".to_string()));
//!
//! // Save changes
//! schema.save()?;
//! # Ok::<(), tagr::schema::error::SchemaError>(())
//! ```

pub mod error;
pub mod types;

pub use error::{Result, SchemaError};
pub use types::{HIERARCHY_DELIMITER, TagSchema};

use std::path::PathBuf;

/// Get the tagr config directory
///
/// Returns `~/.config/tagr/` on Linux
///
/// # Panics
/// Panics if the system config directory cannot be determined
#[must_use]
pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .expect("Could not determine config directory")
        .join("tagr")
}

/// Get the default global schema file path
///
/// Returns `~/.config/tagr/tag_schema.toml` on Linux
#[must_use]
pub fn default_schema_path() -> PathBuf {
    config_dir().join("tag_schema.toml")
}

/// Load schema from default global location, or create new if doesn't exist
///
/// # Errors
/// Returns error if file exists but cannot be read or parsed
pub fn load_default_schema() -> Result<TagSchema> {
    TagSchema::load(&default_schema_path())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let schema_path = temp_dir.path().join("schema.toml");

        // Create and save schema
        let mut schema = TagSchema::load(&schema_path).unwrap();
        schema.add_alias("js", "javascript").unwrap();
        schema.add_alias("py", "python").unwrap();
        schema.save().unwrap();

        // Load and verify
        let loaded = TagSchema::load(&schema_path).unwrap();
        assert_eq!(loaded.canonicalize("js"), "javascript");
        assert_eq!(loaded.canonicalize("py"), "python");

        let mut aliases = loaded.list_aliases();
        aliases.sort();
        assert_eq!(
            aliases,
            vec![
                ("js".to_string(), "javascript".to_string()),
                ("py".to_string(), "python".to_string())
            ]
        );
    }

    #[test]
    fn test_load_nonexistent_creates_empty() {
        let temp_dir = TempDir::new().unwrap();
        let schema_path = temp_dir.path().join("nonexistent.toml");

        let schema = TagSchema::load(&schema_path).unwrap();
        assert!(schema.list_aliases().is_empty());
    }

    #[test]
    fn test_persistence_format() {
        let temp_dir = TempDir::new().unwrap();
        let schema_path = temp_dir.path().join("schema.toml");

        let mut schema = TagSchema::load(&schema_path).unwrap();
        schema.add_alias("js", "javascript").unwrap();
        schema.save().unwrap();

        let content = fs::read_to_string(&schema_path).unwrap();
        assert!(content.contains("[aliases]"));
        assert!(content.contains("js = \"javascript\""));
    }
}
