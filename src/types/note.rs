//! `NoteRecord` and `NoteMeta` — note content and metadata.
//!
//! These types represent user-attached notes on tagged files.
//! Used by the `TagStore` trait, wire protocol, and CLI commands.

use serde::{Deserialize, Serialize};

/// Metadata for a note (timestamps only).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteMeta {
    /// Unix timestamp when note was created.
    pub created_at: i64,
    /// Unix timestamp when note was last updated.
    pub updated_at: i64,
}

impl Default for NoteMeta {
    fn default() -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            created_at: now,
            updated_at: now,
        }
    }
}

/// A note attached to a file.
///
/// Contains free-form markdown content and creation/update timestamps.
///
/// # Examples
///
/// ```
/// use tagr::types::NoteRecord;
///
/// let note = NoteRecord::new("TODO: refactor this module".to_string());
/// assert_eq!(note.content, "TODO: refactor this module");
/// assert!(note.metadata.created_at > 0);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteRecord {
    /// Markdown content of the note (free-form markdown with optional timestamped entries).
    pub content: String,
    /// Note metadata (timestamps only).
    pub metadata: NoteMeta,
}

impl NoteRecord {
    /// Create a new note with the given content.
    ///
    /// Timestamps are set to the current UTC time.
    #[must_use]
    pub fn new(content: String) -> Self {
        Self {
            content,
            metadata: NoteMeta::default(),
        }
    }

    /// Update the content and bump the `updated_at` timestamp.
    pub fn update_content(&mut self, content: String) {
        self.content = content;
        self.metadata.updated_at = chrono::Utc::now().timestamp();
    }
}
