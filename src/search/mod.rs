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
//! # use tagr::db::Database;
//!
//! #[cfg(feature = "ratatui-tui")]
//! use tagr::ui::ratatui_adapter::RatatuiFinder;
//!
//! #[cfg(all(feature = "skim-tui", not(feature = "ratatui-tui")))]
//! use tagr::ui::skim_adapter::SkimFinder;
//!
//! # fn example(db: &Database) -> Result<(), Box<dyn std::error::Error>> {
//! let config = BrowseConfig::default();
//! let session = BrowseSession::new(db, config)?;
//!
//! #[cfg(feature = "ratatui-tui")]
//! let finder = RatatuiFinder::new();
//!
//! #[cfg(all(feature = "skim-tui", not(feature = "ratatui-tui")))]
//! let finder = SkimFinder::new();
//!
//! let controller = BrowseController::new(session, finder);
//! controller.run()?;
//! # Ok(())
//! # }
//! ```

pub mod error;
pub mod filter;

pub use error::SearchError;
