//! Filter management module
//!
//! This module provides functionality for saving, loading, and managing search filters.
//! Filters allow users to save complex search criteria with memorable names and quickly
//! recall them later.
//!
//! # Features
//!
//! - **Save filters**: Save search criteria (tags, file patterns, exclusions) as named filters
//! - **Load filters**: Quickly load and execute saved filters
//! - **Manage filters**: List, show, edit, delete, and rename filters
//! - **Export/Import**: Share filters with teams or backup filter collections
//! - **Statistics**: Track filter usage and identify frequently used filters
//!
//! # Storage
//!
//! Filters are stored in TOML format at `~/.config/tagr/filters.toml` by default.
//! The storage location can be customized in the tagr configuration.
//!
//! # Examples
//!
//! ```no_run
//! use tagr::filters::{FilterCriteria, FilterManager};
//! use std::path::PathBuf;
//!
//! let manager = FilterManager::new(PathBuf::from("~/.config/tagr/filters.toml"));
//!
//! // Create a new filter
//! let criteria = FilterCriteria {
//!     tags: vec!["rust".to_string(), "tutorial".to_string()],
//!     ..Default::default()
//! };
//!
//! manager.create(
//!     "rust-tutorials",
//!     "Find Rust tutorial files".to_string(),
//!     criteria,
//! ).unwrap();
//!
//! // Load and use a filter
//! let filter = manager.get("rust-tutorials").unwrap();
//! println!("Filter: {} - {}", filter.name, filter.description);
//! ```

pub mod error;
pub mod operations;
pub mod types;

pub use error::FilterError;
pub use operations::FilterManager;
pub use types::{FileMode, Filter, FilterCriteria, FilterStorage, TagMode, validate_filter_name};

use std::path::PathBuf;

/// Get the default filter storage path
///
/// Returns `~/.config/tagr/filters.toml` (platform-specific)
///
/// # Errors
///
/// Returns `FilterError` if the config directory cannot be determined
pub fn default_filter_path() -> Result<PathBuf, FilterError> {
    let config_dir = dirs::config_dir().ok_or_else(|| {
        FilterError::Config(config::ConfigError::Message(
            "Could not determine config directory".to_string(),
        ))
    })?;

    Ok(config_dir.join("tagr").join("filters.toml"))
}

/// Get the filter storage path from configuration or use default
///
/// # Errors
///
/// Returns `FilterError` if the path cannot be determined
pub fn get_filter_path() -> Result<PathBuf, FilterError> {
    // TODO: Read from config once filter config options are added
    default_filter_path()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_filter_path() {
        let path = default_filter_path().unwrap();
        assert!(path.to_string_lossy().contains("tagr"));
        assert!(path.to_string_lossy().ends_with("filters.toml"));
    }
}
