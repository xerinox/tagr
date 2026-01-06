//! Interactive search module using fuzzy finder
//!
//! Provides an interactive browse mode that allows users to:
//! 1. Select tags using fuzzy finder (multi-select supported)
//! 2. View and select files matching those tags (multi-select supported)
//!
//! ## Usage
//!
//! The recommended API uses the unified browser pattern:
//!
//! ```no_run
//! use tagr::browse::{BrowseSession, BrowseController, BrowseConfig};
//! use tagr::ui::ratatui_adapter::RatatuiFinder;
//! # use tagr::db::Database;
//!
//! # fn example(db: &Database) -> Result<(), Box<dyn std::error::Error>> {
//! let config = BrowseConfig::default();
//! let session = BrowseSession::new(db, config)?;
//!
//! let finder = RatatuiFinder::new();
//!
//! let controller = BrowseController::new(session, finder);
//! controller.run()?;
//! # Ok(())
//! # }
//! ```

pub mod error;
pub mod filter;

pub use error::SearchError;
