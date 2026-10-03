//! File preview system

mod error;
mod generator;
mod provider;
mod types;

pub use error::PreviewError;
pub use generator::PreviewGenerator;
pub use provider::FilePreviewProvider;
pub use types::PreviewContent;
