//! UI abstraction layer
//!
//! This module provides a backend-agnostic interface for interactive
//! fuzzy finding and preview functionality. The abstraction allows
//! swapping out skim for custom TUI implementations without changing
//! the browse logic.

mod error;
mod traits;
mod types;

pub mod skim_adapter;

#[cfg(test)]
pub mod mock;

pub use error::{Result, UiError};
pub use traits::{FinderConfig, FuzzyFinder, PreviewConfig, PreviewProvider};
pub use types::{DisplayItem, FinderResult, ItemMetadata, PreviewPosition};
