//! Error types for filter operations
//!
//! This module defines all possible errors that can occur during filter operations,
//! including validation errors, I/O errors, and serialization errors.

use std::io;
use thiserror::Error;

/// Errors that can occur during filter operations
#[derive(Debug, Error)]
pub enum FilterError {
    /// Filter not found
    #[error("Filter '{0}' not found")]
    NotFound(String),

    /// Filter already exists
    #[error("Filter '{0}' already exists")]
    AlreadyExists(String),

    /// Invalid filter name
    #[error("Invalid filter name '{0}': {1}")]
    InvalidName(String, String),

    /// Invalid filter criteria
    #[error("Invalid filter criteria: {0}")]
    InvalidCriteria(String),

    /// I/O error
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    /// Serialization/deserialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(#[from] config::ConfigError),

    /// Database error
    #[error("Database error: {0}")]
    Database(String),
}

impl From<toml::de::Error> for FilterError {
    fn from(err: toml::de::Error) -> Self {
        Self::Serialization(err.to_string())
    }
}

impl From<toml::ser::Error> for FilterError {
    fn from(err: toml::ser::Error) -> Self {
        Self::Serialization(err.to_string())
    }
}
