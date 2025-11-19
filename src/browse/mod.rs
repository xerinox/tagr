//! Browse module - tag and file selection workflows
//!
//! This module provides data models and business logic for the interactive
//! browse functionality in Tagr. It is designed to be UI-agnostic, allowing
//! different frontends (skim, ratatui) to use the same underlying logic.
//!
//! # Architecture
//!
//! - `models`: Core data types (TagrItem, SelectionState, etc.)
//! - `query`: Business logic for data retrieval
//! - `actions`: Pure action business logic
//! - Pure data structures with minimal business logic
//! - Conversions via From/TryFrom traits
//! - Idiomatic Rust patterns (direct field access for comparisons)

pub mod actions;
pub mod models;
pub mod query;

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

