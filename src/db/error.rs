//! Database-specific error types
//!
//! This module defines all error types that can occur during database operations.
//! Errors are properly categorized and include context for debugging.
//!
//! # Error Types
//!
//! - **`SledError`**: Errors from the underlying sled embedded database
//! - **`PostcardError`**: Failures when serializing/deserializing data
//! - **`SerializeError`**: Generic serialization errors (e.g., invalid UTF-8 in paths)
//!
//! All errors implement `std::error::Error` via the `thiserror` crate and provide
//! helpful error messages for debugging.

use thiserror::Error;

/// Database-specific errors
#[derive(Debug, Error)]
pub enum DbError {
    /// Represents a sled database error
    #[error("Database error: {0}")]
    SledError(#[from] sled::Error),

    /// Represents a postcard serialization/deserialization error
    #[error("Error while serializing/deserializing data: {0}")]
    PostcardError(#[from] postcard::Error),

    /// Generic serialization/deserialization error
    #[error("Error during serialization: {0}")]
    SerializeError(String),

    /// File does not exist on the filesystem
    #[error("File not found: {0}")]
    FileNotFound(String),

    /// File does not exist on the filesystem
    #[error("Error while reading path{0}")]
    PathError(String),

    /// Invalid input provided (e.g., invalid regex or glob pattern)
    #[error("Invalid input: {0}")]
    InvalidInput(String),
}

#[cfg(test)]
#[path = "error_tests.rs"]
mod error_tests;
