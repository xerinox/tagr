//! Type wrappers for database keys
//!
//! Provides `PathKey` — a wrapper around `PathBuf` for serializing file paths
//! as sled database keys via postcard.
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
use std::path::{Path, PathBuf};

/// Wrapper for `PathBuf` that can be converted to `Vec<u8>` for database keys
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathKey(pub PathBuf);

impl TryFrom<PathKey> for Vec<u8> {
    type Error = DbError;

    fn try_from(key: PathKey) -> Result<Self, Self::Error> {
        Ok(postcard::to_allocvec(&key.0)?)
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
        let path: PathBuf = postcard::from_bytes(bytes)?;
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

#[cfg(test)]
#[path = "types_tests.rs"]
mod types_tests;
