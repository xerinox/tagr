//! Preview error types

use thiserror::Error;

/// Errors that can occur during preview generation
#[derive(Debug, Error)]
pub enum PreviewError {
    /// IO error while reading file
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// File is not UTF-8 encoded
    #[error("File is not valid UTF-8: {0}")]
    InvalidUtf8(String),

    /// File is too large to preview
    #[error("File too large: {0} bytes (max: {1} bytes)")]
    FileTooLarge(u64, u64),

    /// File type not supported for preview
    #[error("Unsupported file type: {0}")]
    UnsupportedFileType(String),

    /// Preview generation timed out
    #[error("Preview generation timed out")]
    Timeout,

    /// Generic preview error
    #[error("Preview error: {0}")]
    Other(String),
}

/// Result type for preview operations
pub type Result<T> = std::result::Result<T, PreviewError>;
