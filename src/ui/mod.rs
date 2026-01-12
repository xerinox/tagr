//! UI abstraction layer
//!
//! This module provides a backend-agnostic interface for interactive
//! fuzzy finding, preview functionality, user input, and output.
//! The abstraction allows swapping out CLI tools for custom TUI
//! implementations without changing business logic.
//!
//! # Core Traits
//!
//! - **`FuzzyFinder`** - Interactive item selection with fuzzy matching
//! - **`UserInput`** - User prompts (text input, confirmation, selection)
//! - **`OutputWriter`** - Status messages with severity levels
//! - **`PreviewProvider`** - File preview generation
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │      Business Logic Layer               │
//! │   (browse, commands, keybinds)          │
//! └────────────────┬────────────────────────┘
//!                  │ Uses traits
//!                  ▼
//! ┌─────────────────────────────────────────┐
//! │      UI Trait Abstraction               │
//! │  (FuzzyFinder, UserInput, etc.)         │
//! └────────────────┬────────────────────────┘
//!                  │ Implemented by
//!                  ▼
//! ┌───────────────────────────────────────┐
//! │      TUI Adapters                     │
//! │  - RatatuiFinder (nucleo + ratatui)   │
//! │  - Dialoguer (CLI prompts)            │
//! │  - Stdout (output)                    │
//! └───────────────────────────────────────┘
//! ```
//!
//! # Usage
//!
//! ## Fuzzy Finding
//!
//! ```no_run
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use tagr::ui::{FuzzyFinder, FinderConfig, DisplayItem, BrowsePhase};
//! use tagr::ui::ratatui_adapter::RatatuiFinder;
//!
//! let items = vec![
//!     DisplayItem::new("file1.rs".into(), "file1.rs".into(), "file1.rs".into()),
//!     DisplayItem::new("file2.rs".into(), "file2.rs".into(), "file2.rs".into()),
//! ];
//!
//! let config = FinderConfig {
//!     items,
//!     multi_select: true,
//!     prompt: "Select files:".into(),
//!     ansi: true,
//!     preview_config: None,
//!     bind: vec![],
//!     phase: BrowsePhase::FileSelection,
//!     available_tags: vec![],
//!     search_criteria: None,
//!     tag_schema: None,
//!     database: None,
//! };
//!
//! let finder = RatatuiFinder::new();
//! let result = finder.run(config)?;
//!
//! if !result.aborted {
//!     println!("Selected: {:?}", result.selected);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Implementing a Custom Finder
//!
//! ```no_run
//! use tagr::ui::{FuzzyFinder, FinderConfig, FinderResult, Result};
//!
//! struct MyFinder;
//!
//! impl FuzzyFinder for MyFinder {
//!     fn run(&self, config: FinderConfig) -> Result<FinderResult> {
//!         // Your custom UI implementation
//!         Ok(FinderResult {
//!             selected: vec![],
//!             aborted: false,
//!             final_key: Some("enter".to_string()),
//!             refine_search: None,
//!             input_action: None,
//!             direct_file_selection: false,
//!             selected_tags: vec![],
//!         })
//!     }
//! }
//! ```
//!
//! ## User Input
//!
//! ```no_run
//! use tagr::ui::input::{UserInput, DialoguerInput};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let input = DialoguerInput::new();
//!
//! // Text input
//! if let Some(name) = input.prompt_text("Enter name:", None, false)? {
//!     println!("Hello, {}!", name);
//! }
//!
//! // Confirmation
//! if let Some(true) = input.prompt_confirm("Delete files?", false)? {
//!     println!("Deleting...");
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Output Messages
//!
//! ```
//! use tagr::ui::output::{OutputWriter, StdoutWriter};
//!
//! let output = StdoutWriter::new();
//! output.success("Operation completed!");
//! output.error("Something went wrong");
//! output.warning("Be careful");
//! output.info("Additional info");
//! ```
//!
//! ## Buffered Messages for TUI
//!
//! ```
//! use tagr::ui::output::{OutputWriter, StatusBarWriter};
//! use std::time::Duration;
//!
//! let writer = StatusBarWriter::with_ttl(Duration::from_secs(5));
//! writer.success("File saved");
//! writer.error("Connection failed");
//!
//! // Get messages for rendering in TUI
//! let messages = writer.recent_messages();
//! for (level, msg) in messages {
//!     println!("{:?}: {}", level, msg);
//! }
//! ```
//!
//! # See Also
//!
//! - `examples/custom_frontend.rs` - Complete custom finder implementation
//! - `docs/custom-frontend-guide.md` - Comprehensive guide for ratatui migration
//! - `src/ui/ratatui_adapter/` - Modern ratatui implementation

mod error;
mod traits;
mod types;

pub mod input;
pub mod output;
pub mod ratatui_adapter;

#[cfg(test)]
pub mod mock;

pub use error::{Result, UiError};
pub use input::{DialoguerInput, InputError, UserInput};
pub use output::{MessageLevel, OutputWriter, StatusBarWriter, StdoutWriter};
pub use ratatui_adapter::{RatatuiFinder, RatatuiPreviewProvider};
pub use traits::{
    FinderConfig, FuzzyFinder, PreviewConfig, PreviewProvider, PreviewText, RefineSearchCriteria,
};
pub use types::{
    BrowsePhase, DisplayItem, FinderResult, ItemMetadata, PreviewPosition, RefinedSearchCriteria,
};
