//! Storage abstraction — Layer 1 of the tagr architecture.
//!
//! Defines [`TagStore`], the trait that all storage backends implement,
//! and [`StoreError`], the domain-level error type that hides backend details.
//!
//! # Implementations
//!
//! - [`DirectStore`](direct::DirectStore) — wraps sled `Database` for single-process access
//! - [`DaemonStore`](daemon::DaemonStore) — wraps IPC `PersistentClient` for daemon mode
//! - [`MockStore`](mock::MockStore) — in-memory `HashMap`-based impl for tests
//!
//! # Design
//!
//! All methods take `&self` — backends use interior mutability where needed
//! (sled is internally thread-safe, `DaemonStore` uses `RwLock<QueryCache>`).
//! The trait requires `Send + Sync` so stores can be shared via `Arc<dyn TagStore>`.
//!
//! `StoreError` describes problems in domain terms (`FileNotFound`, `TagNotFound`)
//! rather than leaking backend types (`sled::Error`). Only `DirectStore` knows about sled.

pub mod daemon;
pub mod direct;
pub mod mock;

pub use daemon::DaemonStore;
pub use direct::DirectStore;
pub use mock::MockStore;

#[cfg(test)]
mod tests;

use crate::schema::types::TagSchema;
use crate::types::{NoteRecord, Pair, QueryCriteria, TagName, TagrPath};
use thiserror::Error;

/// Domain-level storage errors.
///
/// Hides backend details (sled, IPC, postcard) behind what actually went wrong.
/// Only backend implementations map raw errors to these variants.
///
/// # Examples
///
/// ```
/// use tagr::store::StoreError;
/// use tagr::types::TagrPath;
///
/// let err = StoreError::FileNotFound(TagrPath::new("/missing.txt").unwrap());
/// assert!(matches!(err, StoreError::FileNotFound(_)));
/// ```
// StoreError is the domain boundary — callers match on these variants,
// not on sled::Error or io::Error directly.
#[derive(Debug, Error)]
#[allow(clippy::module_name_repetitions)]
pub enum StoreError {
    /// File path not found in the database.
    #[error("file not found in database: {0}")]
    FileNotFound(TagrPath),

    /// Tag does not exist in the reverse index.
    #[error("tag does not exist: {0}")]
    TagNotFound(TagName),

    /// Another process holds the database lock.
    #[error("database is locked by another process")]
    DatabaseLocked,

    /// Stored data failed to deserialize or is structurally invalid.
    #[error("storage corrupted at key '{key}': {reason}")]
    StorageCorrupted {
        /// The key that failed to decode.
        key: String,
        /// What went wrong during deserialization.
        reason: String,
    },

    /// Backend I/O failed (disk, filesystem).
    #[error("database I/O failed: {context}")]
    IoFailed {
        /// Human-readable description of what operation failed.
        context: String,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// Backend connection lost (used by `DaemonStore` when IPC fails
    /// after internal reconnection attempts are exhausted).
    #[error("storage backend unavailable: {context}")]
    ConnectionLost {
        /// What the client was trying to do when the connection dropped.
        context: String,
    },
}

/// Convenience alias for store operations.
pub type Result<T> = std::result::Result<T, StoreError>;

/// Unified storage interface for tag operations.
///
/// All tagr data access goes through this trait. Implementations:
/// - `DirectStore` — wraps sled for single-process CLI/TUI
/// - `DaemonStore` — wraps IPC with pass-through to the daemon
/// - `MockStore` — in-memory for tests
///
/// # Thread Safety
///
/// All methods take `&self`. Backends handle interior mutability internally:
/// sled is thread-safe, `DaemonStore` uses `RwLock<QueryCache>`.
///
/// # Errors
///
/// All methods return [`Result<T>`](type@Result) (`Result<T, StoreError>`).
/// Errors are in domain terms — callers never see backend-specific types.
pub trait TagStore: Send + Sync {
    // -- Queries --

    /// List all distinct tag names in the database.
    ///
    /// # Errors
    ///
    /// Returns `StoreError` on backend I/O failure.
    fn list_all_tags(&self) -> Result<Vec<TagName>>;

    /// List all tags with the number of files tagged with each.
    ///
    /// # Errors
    ///
    /// Returns `StoreError` on backend I/O failure.
    fn list_tags_with_counts(&self) -> Result<Vec<(TagName, usize)>>;

    /// List all file paths in the database.
    ///
    /// # Errors
    ///
    /// Returns `StoreError` on backend I/O failure.
    fn list_all_files(&self) -> Result<Vec<TagrPath>>;

    /// List all file-tag pairs.
    ///
    /// # Errors
    ///
    /// Returns `StoreError` on backend I/O failure.
    fn list_all(&self) -> Result<Vec<Pair>>;

    /// Get the tags for a specific file.
    ///
    /// Returns `None` if the file is not in the database (not an error).
    ///
    /// # Errors
    ///
    /// Returns `StoreError` on backend I/O failure.
    fn get_tags(&self, file: &TagrPath) -> Result<Option<Vec<TagName>>>;

    /// Find all files that have a specific tag.
    ///
    /// Uses the reverse index for O(1) lookup in `DirectStore`.
    ///
    /// # Errors
    ///
    /// Returns `StoreError` on backend I/O failure.
    fn find_by_tag(&self, tag: &TagName) -> Result<Vec<TagrPath>>;

    /// Find files that have ALL of the specified tags.
    ///
    /// # Errors
    ///
    /// Returns `StoreError` on backend I/O failure.
    fn find_by_all_tags(&self, tags: &[TagName]) -> Result<Vec<TagrPath>>;

    /// Find files that have ANY of the specified tags.
    ///
    /// # Errors
    ///
    /// Returns `StoreError` on backend I/O failure.
    fn find_by_any_tag(&self, tags: &[TagName]) -> Result<Vec<TagrPath>>;

    /// Find files whose tags match a regex pattern.
    ///
    /// # Errors
    ///
    /// Returns `StoreError` on backend I/O failure or invalid regex.
    fn find_by_tag_regex(&self, pattern: &str) -> Result<Vec<TagrPath>>;

    /// Check whether a tag exists in the database.
    ///
    /// # Errors
    ///
    /// Returns `StoreError` on backend I/O failure.
    fn tag_exists(&self, tag: &TagName) -> Result<bool>;

    /// Find tags that start with a given prefix.
    ///
    /// Leverages sled's `scan_prefix()` for O(log n + k) in `DirectStore`.
    /// Powers hierarchy expansion, autocomplete, and alias resolution.
    ///
    /// # Errors
    ///
    /// Returns `StoreError` on backend I/O failure.
    fn find_tags_by_prefix(&self, prefix: &TagName) -> Result<Vec<TagName>>;

    // -- Mutations --

    /// Insert or replace a file's tags (set semantics).
    ///
    /// If the file already exists, its tags are fully replaced.
    /// Both the files tree and the reverse tag index are updated atomically.
    ///
    /// # Errors
    ///
    /// Returns `StoreError` on backend I/O failure.
    fn insert(&self, file: &TagrPath, tags: Vec<TagName>) -> Result<()>;

    /// Add tags to a file, preserving existing tags.
    ///
    /// If the file does not exist, creates it with the given tags.
    ///
    /// # Errors
    ///
    /// Returns `StoreError` on backend I/O failure.
    fn add_tags(&self, file: &TagrPath, tags: Vec<TagName>) -> Result<()>;

    /// Remove specific tags from a file.
    ///
    /// Tags not present on the file are silently ignored.
    ///
    /// # Errors
    ///
    /// Returns `StoreError` on backend I/O failure.
    fn remove_tags(&self, file: &TagrPath, tags: &[TagName]) -> Result<()>;

    /// Remove a file and all its tag associations from the database.
    ///
    /// Returns `true` if the file existed and was removed, `false` if not found.
    ///
    /// # Errors
    ///
    /// Returns `StoreError` on backend I/O failure.
    fn remove_file(&self, file: &TagrPath) -> Result<bool>;

    /// Remove a tag from all files that have it.
    ///
    /// Returns the number of files that were affected.
    ///
    /// # Errors
    ///
    /// Returns `StoreError` on backend I/O failure.
    fn remove_tag_globally(&self, tag: &TagName) -> Result<u64>;

    // -- Notes --

    /// Get the note attached to a file.
    ///
    /// Returns `None` if no note exists (not an error).
    ///
    /// # Errors
    ///
    /// Returns `StoreError` on backend I/O failure.
    fn get_note(&self, file: &TagrPath) -> Result<Option<NoteRecord>>;

    /// Set or replace the note for a file.
    ///
    /// # Errors
    ///
    /// Returns `StoreError` on backend I/O failure.
    fn set_note(&self, file: &TagrPath, note: &NoteRecord) -> Result<()>;

    /// Delete the note for a file.
    ///
    /// Returns `true` if a note existed and was removed, `false` if not found.
    ///
    /// # Errors
    ///
    /// Returns `StoreError` on backend I/O failure.
    fn delete_note(&self, file: &TagrPath) -> Result<bool>;

    /// List all notes in the database.
    ///
    /// # Errors
    ///
    /// Returns `StoreError` on backend I/O failure.
    fn list_all_notes(&self) -> Result<Vec<(TagrPath, NoteRecord)>>;

    /// Search notes by content substring.
    ///
    /// Returns all file-note pairs where the note content contains the query.
    ///
    /// # Errors
    ///
    /// Returns `StoreError` on backend I/O failure.
    fn search_notes(&self, query: &str) -> Result<Vec<(TagrPath, NoteRecord)>>;

    // -- Composite query --

    /// Execute a query against the store.
    ///
    /// **Required method — no default impl.** Each backend provides its own body:
    /// - `DirectStore` evaluates the flat-tag subset locally
    ///   (full pipeline via `query::execute()` in Phase 3)
    /// - `DaemonStore` serializes criteria over IPC
    /// - `MockStore` uses `QueryCriteria::matches_pair()` for local filtering
    ///
    /// # Errors
    ///
    /// Returns `StoreError` on backend I/O or query evaluation failure.
    fn query(&self, criteria: &QueryCriteria, schema: &TagSchema) -> Result<Vec<TagrPath>>;
}
