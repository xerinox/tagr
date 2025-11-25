//! UI abstraction layer
//!
//! This module provides a backend-agnostic interface for interactive
//! fuzzy finding, preview functionality, user input, and output.
//! The abstraction allows swapping out CLI tools (skim, dialoguer, stdout)
//! for custom TUI implementations (ratatui) without changing business logic.
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
//!         ┌────────┴────────┐
//!         ▼                 ▼
//! ┌───────────────┐  ┌──────────────┐
//! │ CLI Adapters  │  │ TUI Adapters │
//! │ - SkimFinder  │  │ - Ratatui    │
//! │ - Dialoguer   │  │   (future)   │
//! │ - Stdout      │  │              │
//! └───────────────┘  └──────────────┘
//! ```
//!
//! # Examples
//!
//! ## Using the Skim Finder (Default)
//!
//! ```no_run
//! use tagr::ui::{FuzzyFinder, FinderConfig, DisplayItem};
//! use tagr::ui::skim_adapter::SkimFinder;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
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
//! };
//!
//! let finder = SkimFinder::new();
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
//! - `src/ui/skim_adapter.rs` - Reference implementation

mod error;
mod traits;
mod types;

pub mod input;
pub mod output;
pub mod skim_adapter;

#[cfg(test)]
pub mod mock;

pub use error::{Result, UiError};
pub use input::{DialoguerInput, InputError, UserInput};
pub use output::{MessageLevel, OutputWriter, StatusBarWriter, StdoutWriter};
pub use traits::{FinderConfig, FuzzyFinder, PreviewConfig, PreviewProvider, PreviewText};
pub use types::{DisplayItem, FinderResult, ItemMetadata, PreviewPosition};
