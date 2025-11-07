//! Database-specific error types
//!
//! This module defines all error types that can occur during database operations.
//! Errors are properly categorized and include context for debugging.
//!
//! # Error Types
//!
//! - **`SledError`**: Errors from the underlying sled embedded database
//! - **`DecodeError`**: Failures when deserializing data from the database
//! - **`EncodeError`**: Failures when serializing data to the database
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
    
    /// Represents a bincode decoding error
    #[error("Error while decoding data: {0}")]
    DecodeError(#[from] bincode::error::DecodeError),
    
    /// Represents a bincode encoding error
    #[error("Error while encoding data: {0}")]
    EncodeError(#[from] bincode::error::EncodeError),
    
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
