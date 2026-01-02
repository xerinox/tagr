//! Bulk command module (split into smaller units)
//!
//! This directory breaks the previous monolithic `bulk.rs` into focused modules:
//! - `core`: shared types, summary, confirmation & preview helpers
//! - `tag_ops`: add/remove/rename/merge/copy tag operations
//! - `batch`: apply tags from batch specification files
//! - `mapping`: rename many tags via mapping files
//! - `delete`: bulk delete files from the database
//! - `propagate`: auto-tag files by directory or extension
//! - `transform`: apply transformations to existing tags
//!
//! Public functions and enums are re-exported to preserve the original API
//! surface (`tagr::commands::bulk::*`).

mod batch;
mod core;
mod delete;
mod mapping;
mod propagate;
mod tag_ops;
mod transform;

pub use batch::{BatchFormat, batch_from_file};
pub use core::{BulkAction, BulkOpSummary};
pub use delete::bulk_delete_files;
pub use mapping::bulk_map_tags;
pub use propagate::{propagate_by_directory, propagate_by_extension};
pub use tag_ops::{bulk_tag, bulk_untag, copy_tags, merge_tags, rename_tag};
pub use transform::{TagTransformation, transform_tags};

// Re-export used parsing types for external callers that may switch on format.
pub use batch::BatchFormat as _BatchFormatForExternal; // compatibility alias (if needed)

#[cfg(test)]
mod tests;
