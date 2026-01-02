//! Browse module - tag and file selection workflows
//!
//! This module provides data models and business logic for the interactive
//! browse functionality in Tagr. It is designed to be UI-agnostic, allowing
//! different frontends (skim, ratatui) to use the same underlying logic.
//!
//! # Architecture
//!
//! - `models`: Core data types (`TagrItem`, `SelectionState`, etc.)
//! - `query`: Business logic for data retrieval
//! - `actions`: Pure action business logic
//! - `session`: Unified browser session orchestration
//! - `ui`: UI controller (presentation bridge)
//! - Pure data structures with minimal business logic
//! - Conversions via From/TryFrom traits
//! - Idiomatic Rust patterns (direct field access for comparisons)
//!
//! # Public API Examples
//!
//! ## Basic Browse Session
//!
//! ```no_run
//! use tagr::browse::{BrowseSession, BrowseController, BrowseConfig};
//! use tagr::db::Database;
//!
//! // Use the appropriate finder based on feature flags
//! #[cfg(feature = "ratatui-tui")]
//! use tagr::ui::ratatui_adapter::RatatuiFinder;
//!
//! #[cfg(all(feature = "skim-tui", not(feature = "ratatui-tui")))]
//! use tagr::ui::skim_adapter::SkimFinder;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Open database
//! let db = Database::open("mydb")?;
//!
//! // Create browse session with default config
//! let config = BrowseConfig::default();
//! let session = BrowseSession::new(&db, config)?;
//!
//! // Create controller with finder
//! #[cfg(feature = "ratatui-tui")]
//! let finder = RatatuiFinder::new();
//!
//! #[cfg(all(feature = "skim-tui", not(feature = "ratatui-tui")))]
//! let finder = SkimFinder::new();
//!
//! let controller = BrowseController::new(session, finder);
//!
//! // Run interactive browse
//! match controller.run()? {
//!     Some(result) => {
//!         println!("Selected {} files", result.selected_files.len());
//!         for file in &result.selected_files {
//!             println!("  - {}", file.display());
//!         }
//!     }
//!     None => println!("Cancelled"),
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Custom Configuration
//!
//! ```no_run
//! use tagr::browse::{BrowseSession, BrowseConfig, PathFormat};
//! use tagr::db::Database;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let db = Database::open("mydb")?;
//!
//! // Configure browse behavior
//! let config = BrowseConfig {
//!     path_format: PathFormat::Relative,
//!     ..Default::default()
//! };
//!
//! let session = BrowseSession::new(&db, config)?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Using as a Library
//!
//! For external crates using tagr as a library:
//!
//! ```no_run
//! use tagr::browse::{BrowseSession, BrowseController};
//! use tagr::ui::FuzzyFinder;
//! use tagr::db::Database;
//!
//! // Your custom finder implementation
//! struct MyCustomFinder;
//! impl FuzzyFinder for MyCustomFinder {
//!     fn run(&self, config: tagr::ui::FinderConfig)
//!         -> tagr::ui::Result<tagr::ui::FinderResult>
//!     {
//!         // Your implementation here
//! #       unimplemented!()
//!     }
//! }
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let db = Database::open("mydb")?;
//! let session = BrowseSession::new(&db, Default::default())?;
//! let finder = MyCustomFinder;
//! let controller = BrowseController::new(session, finder);
//!
//! let result = controller.run()?;
//! # Ok(())
//! # }
//! ```
//!
//! See `examples/custom_frontend.rs` for a complete working example.

pub mod actions;
pub mod models;
pub mod query;
pub mod session;
pub mod ui;

pub use actions::{
    execute_add_tag, execute_copy_files, execute_copy_path, execute_delete_from_db,
    execute_open_in_default, execute_open_in_editor, execute_remove_tag,
};
pub use models::{
    ActionContext, ActionData, ActionOutcome, CachedMetadata, FileMetadata, ItemMetadata,
    MetadataCache, PairWithCache, PathWithDb, SearchMode, SelectionState, TagMetadata, TagWithDb,
    TagrItem,
};
pub use query::{get_available_tags, get_files_by_tags, get_matching_files};
pub use session::{
    AcceptResult, BrowseConfig, BrowseError, BrowseResult, BrowseSession, BrowserPhase, HelpText,
    PathFormat, PhaseSettings, PhaseType,
};
pub use ui::BrowseController;
