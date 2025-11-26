//! Type wrappers for database keys and values
//!
//! This module provides type-safe wrappers around paths for use as database keys.
//! These wrappers handle serialization, UTF-8 validation, and provide ergonomic
//! conversions between different path representations.
//!
//! # Types
//!
//! - **`PathKey`**: Wrapper for `PathBuf` that can be serialized to `Vec<u8>` for database keys
//! - **`PathString`**: Wrapper that guarantees a path can be represented as valid UTF-8
//!
//! # Design Rationale
//!
//! These types ensure type safety and proper error handling when working with paths
//! in the database. Path serialization is handled consistently via bincode, and UTF-8
//! validation happens at the type level rather than at each use site.
//!
//! # Examples
//!
//! ```no_run
//! use tagr::db::types::{PathKey, PathString};
//! use std::path::PathBuf;
//!
//! // Create a database key from a path
//! let key = PathKey::new("file.txt");
//! let bytes: Vec<u8> = key.try_into().unwrap();
//!
//! // Ensure path is valid UTF-8
//! let path_str = PathString::new("file.txt").unwrap();
//! assert_eq!(path_str.as_str(), "file.txt");
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

/// Wrapper for a path that guarantees valid UTF-8 string representation
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathString(String);

impl TryFrom<PathBuf> for PathString {
    type Error = DbError;

    fn try_from(path: PathBuf) -> Result<Self, Self::Error> {
        path.to_str()
            .ok_or_else(|| DbError::SerializeError("Invalid UTF-8 in path".into()))
            .map(|s| Self(s.to_string()))
    }
}

impl TryFrom<&Path> for PathString {
    type Error = DbError;

    fn try_from(path: &Path) -> Result<Self, Self::Error> {
        path.to_str()
            .ok_or_else(|| DbError::SerializeError("Invalid UTF-8 in path".into()))
            .map(|s| Self(s.to_string()))
    }
}

impl PathString {
    /// # Errors
    ///
    /// Returns `DbError` if the path contains invalid UTF-8 characters.
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, DbError> {
        path.as_ref()
            .to_str()
            .ok_or_else(|| DbError::SerializeError("Invalid UTF-8 in path".into()))
            .map(|s| Self(s.to_string()))
    }

    /// Returns the string slice for this path.
    ///
    /// # Note
    ///
    /// This method is deprecated. Use `Deref` coercion instead:
    /// ```ignore
    /// let path_string = PathString::new("/path")?;
    /// // Instead of: path_string.as_str()
    /// // Use: &*path_string or just &path_string in most contexts
    /// ```
    #[must_use]
    #[deprecated(since = "0.5.0", note = "Use Deref coercion instead: &*path_string")]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

impl AsRef<str> for PathString {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::ops::Deref for PathString {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(test)]
#[path = "types_tests.rs"]
mod types_tests;
