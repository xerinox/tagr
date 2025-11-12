//! File preview functionality
//!
//! This module provides preview generation for various file types:
//! - Text files with line truncation
//! - Binary files with metadata
//! - Images with dimensions and format info
//! - Archives with content listing

mod error;
mod generator;
mod types;

pub use error::{PreviewError, Result};
pub use generator::PreviewGenerator;
pub use types::{FileMetadata, ImageMetadata, PreviewContent};
