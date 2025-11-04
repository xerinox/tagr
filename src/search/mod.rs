//! Interactive search module using skim fuzzy finder
//! 
//! Provides an interactive browse mode that allows users to:
//! 1. Select tags using fuzzy finder (multi-select supported)
//! 2. View and select files matching those tags (multi-select supported)

pub mod error;
pub mod browse;

pub use error::SearchError;
pub use browse::{browse, browse_advanced, BrowseResult};
