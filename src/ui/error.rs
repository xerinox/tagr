//! UI error types

use thiserror::Error;

/// Errors that can occur in UI operations
#[derive(Debug, Error)]
pub enum UiError {
    /// Error building UI configuration
    #[error("Failed to build UI configuration: {0}")]
    BuildError(String),

    /// UI operation was interrupted or cancelled
    #[error("UI operation was interrupted")]
    InterruptedError,

    /// Invalid configuration
    #[error("Invalid UI configuration: {0}")]
    InvalidConfig(String),

    /// IO error during UI operations
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// Preview generation failed
    #[error("Preview generation failed: {0}")]
    PreviewError(String),
}

/// Result type for UI operations
pub type Result<T> = std::result::Result<T, UiError>;
