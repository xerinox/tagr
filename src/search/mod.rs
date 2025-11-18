//! Interactive search module using skim fuzzy finder
//!
//! Provides an interactive browse mode that allows users to:
//! 1. Select tags using fuzzy finder (multi-select supported)
//! 2. View and select files matching those tags (multi-select supported)
//!
//! ## Usage
//!
//! The recommended API uses the stateful builder pattern:
//!
//! ```no_run
//! use tagr::search::{BrowseState, BrowseVariant};
//! # use tagr::db::Database;
//! # use tagr::config::PathFormat;
//! # fn example(db: &Database) -> Result<(), Box<dyn std::error::Error>> {
//! let mut browse = BrowseState::builder()
//!     .db(db)
//!     .path_format(PathFormat::Relative)
//!     .build()?;
//!
//! if let Some(result) = browse.run(BrowseVariant::Standard)? {
//!     println!("Selected {} files", result.selected_files.len());
//! }
//! # Ok(())
//! # }
//! ```

pub mod browse;
pub mod error;
pub mod filter;
pub mod state;

// Re-export the new API
pub use state::{BrowseResult, BrowseState, BrowseVariant};

// Legacy function exports for backward compatibility
pub use browse::{
    browse, browse_advanced, browse_with_actions, browse_with_params,
    browse_with_realtime_keybinds, show_actions_for_files,
};

pub use error::SearchError;
