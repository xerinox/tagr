//! Interactive search module using skim fuzzy finder
//!
//! Provides an interactive browse mode that allows users to:
//! 1. Select tags using fuzzy finder (multi-select supported)
//! 2. View and select files matching those tags (multi-select supported)

pub mod browse;
pub mod error;
pub mod filter;

pub use browse::{BrowseResult, browse, browse_advanced, browse_with_params, browse_with_actions, show_actions_for_files};
pub use error::SearchError;
