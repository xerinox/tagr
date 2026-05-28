//! Core types — Layer 0 of the tagr architecture.
//!
//! This module defines the shared vocabulary used across all layers:
//! newtypes (`TagName`, `TagrPath`, `FilterName`), the unified query type
//! (`QueryCriteria`), tag expressions (`TagExpr`), and validation errors.
//!
//! **Zero dependencies on other tagr modules.** Every other module may import
//! these types, but this module never imports from higher layers.
//!
//! # Newtypes
//!
//! - [`TagName`] — validated, non-empty tag identifier (e.g. `"rust"`, `"language:rust"`)
//! - [`TagrPath`] — UTF-8-validated file path (stored as-given, canonicalized at CLI boundary)
//! - [`FilterName`] — validated saved-filter identifier
//!
//! # Query Types
//!
//! - [`QueryCriteria`] — unified search parameters replacing `SearchParams`,
//!   `FilterCriteria`, `ActiveFilter`, and `WireSearchParams`
//! - [`TagExpr`] — boolean expression tree for tag matching
//! - [`MatchMode`] — AND/OR matching semantics
//!
//! # Data Types
//!
//! - [`Pair`] — file + tags association (DTO)
//! - [`NoteRecord`] — note content + metadata

mod tag_name;
mod tagr_path;
mod filter_name;
mod query;
mod pair;
mod note;
mod error;

pub use error::{NameKind, ValidationError};
pub use filter_name::FilterName;
pub use note::{NoteMeta, NoteRecord};
pub use pair::Pair;
pub use query::{MatchMode, QueryCriteria, TagExpr};
pub use tag_name::TagName;
pub use tagr_path::TagrPath;

/// Maximum length for a tag name in characters.
pub const MAX_TAG_NAME_LEN: usize = 128;

/// Maximum length for a filter name in characters.
pub const MAX_FILTER_NAME_LEN: usize = 64;

/// The hierarchy delimiter used in tag names (e.g. `"language:rust"`).
pub const HIERARCHY_DELIMITER: char = ':';

/// Reserved virtual tag prefixes that cannot be used as regular tag names.
/// These are parsed by the vtag system and would conflict if stored as tags.
pub const RESERVED_VTAG_PREFIXES: &[&str] = &[
    "modified", "created", "accessed", "size", "ext", "ext-type", "dir", "path", "depth", "perm",
    "lines", "git",
];

#[cfg(test)]
mod tests;
