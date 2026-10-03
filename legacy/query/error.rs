//! Query and search error types.

use thiserror::Error;

/// Errors that can occur during search and query operations.
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
