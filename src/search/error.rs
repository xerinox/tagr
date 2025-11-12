//! Search-specific error types
//!
//! This module defines error types specific to search and interactive browse operations.
//! These errors can occur during fuzzy finder interactions, UI rendering, or when
//! querying the database during search operations.
//!
//! # Error Types
//!
//! - **`DatabaseError`**: Errors from database queries during search (wraps `DbError`)
//! - **`InterruptedError`**: User cancelled the interactive fuzzy finder (Ctrl+C or ESC)
//! - **`BuildError`**: Failed to construct skim fuzzy finder options
//!
//! All errors implement proper error chaining and provide context for debugging.

use thiserror::Error;

/// Search-specific errors
#[derive(Debug, Error)]
pub enum SearchError {
    /// Database error occurred during search
    #[error("Database error: {0}")]
    DatabaseError(#[from] crate::db::DbError),

    /// UI error occurred during interactive selection
    #[error("UI error: {0}")]
    UiError(#[from] crate::ui::UiError),

    /// Skim fuzzy finder was interrupted
    #[error("Interactive selection was interrupted")]
    InterruptedError,

    /// Failed to build UI options
    #[error("Failed to build UI options: {0}")]
    BuildError(String),
}

#[cfg(test)]
#[path = "error_tests.rs"]
mod error_tests;
