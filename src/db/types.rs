//! Type wrappers for database keys
//!
//! Provides `PathKey` — a wrapper around `PathBuf` for serializing file paths
//! as sled database keys via bincode.
//!
//! # Examples
//!
//! ```no_run
//! use tagr::db::types::PathKey;
//! use std::path::PathBuf;
//!
//! let key = PathKey::new("file.txt");
//! let bytes: Vec<u8> = key.try_into().unwrap();
//! ```

use super::error::DbError;
use bincode;
use std::path::{Path, PathBuf};

/// Wrapper for `PathBuf` that can be converted to `Vec<u8>` for database keys
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathKey(pub PathBuf);

impl TryFrom<PathKey> for Vec<u8> {
    type Error = DbError;

    fn try_from(key: PathKey) -> Result<Self, Self::Error> {
        Ok(bincode::encode_to_vec(&key.0, bincode::config::standard())?)
    }
}

impl PathKey {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self(path.as_ref().to_path_buf())
    }

    /// # Errors
    ///
    /// Returns `DbError` if the bytes cannot be deserialized into a `PathBuf`.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DbError> {
        let (path, _): (PathBuf, usize) =
            bincode::decode_from_slice(bytes, bincode::config::standard())?;
        Ok(Self(path))
    }

    #[must_use]
    pub fn into_inner(self) -> PathBuf {
        self.0
    }

    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for PathKey {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

/// Metadata for a note
///
/// Canonical type lives in [`crate::types::NoteMeta`]. Re-exported here
/// for backward compatibility with existing `use crate::db::NoteMeta`.
pub use crate::types::NoteMeta;

/// A note attached to a file
///
/// Canonical type lives in [`crate::types::NoteRecord`]. Re-exported here
/// for backward compatibility with existing `use crate::db::NoteRecord`.
pub use crate::types::NoteRecord;

#[cfg(test)]
#[path = "types_tests.rs"]
mod types_tests;
