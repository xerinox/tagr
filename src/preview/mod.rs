//! File preview system

mod error;
mod generator;
mod provider;
mod types;

pub use error::{PreviewError, Result};
pub use generator::PreviewGenerator;
pub use provider::FilePreviewProvider;
pub use types::{FileMetadata, ImageMetadata, PreviewContent};
