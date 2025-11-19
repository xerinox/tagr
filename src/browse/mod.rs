//! Browse module - tag and file selection workflows
//!
//! This module provides data models and business logic for the interactive
//! browse functionality in Tagr. It is designed to be UI-agnostic, allowing
//! different frontends (skim, ratatui) to use the same underlying logic.
//!
//! # Architecture
//!
//! - `models`: Core data types (TagrItem, SelectionState, etc.)
//! - Pure data structures with minimal business logic
//! - Conversions via From/TryFrom traits
//! - Idiomatic Rust patterns (direct field access for comparisons)

pub mod models;

pub use models::{
    ActionContext, ActionData, ActionOutcome, CachedMetadata, FileMetadata, ItemMetadata,
    MetadataCache, SearchMode, SelectionState, TagMetadata, TagrItem,
};
