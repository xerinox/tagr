//! `TagrPath` — UTF-8-validated file path.

use std::borrow::Borrow;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::error::ValidationError;

/// A file path guaranteed to be valid UTF-8.
///
/// Stored as-given (no canonicalization). CLI commands should call
/// `std::fs::canonicalize()` before constructing a `TagrPath` when
/// storing paths to the database. `TagrPath::new()` only validates UTF-8.
///
/// # Examples
///
/// ```
/// use tagr::types::TagrPath;
///
/// let path = TagrPath::new("src/main.rs").unwrap();
/// assert_eq!(path.as_str(), "src/main.rs");
/// assert_eq!(path.as_path(), std::path::Path::new("src/main.rs"));
/// ```
///
/// # Errors
///
/// [`TagrPath::new`] returns [`ValidationError::InvalidUtf8`] if the path
/// cannot be represented as UTF-8.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub struct TagrPath(String);

impl TagrPath {
    /// Create a new `TagrPath`, validating that the path is valid UTF-8.
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError::InvalidUtf8`] if the path contains
    /// non-UTF-8 bytes.
    pub fn new(path: impl AsRef<Path>) -> Result<Self, ValidationError> {
        let path_ref = path.as_ref();
        path_ref
            .to_str()
            .ok_or_else(|| ValidationError::InvalidUtf8 {
                lossy_path: path_ref.to_string_lossy().into_owned(),
            })
            .map(|s| Self(s.to_string()))
    }

    /// Create a `TagrPath` from a string that is already known to be valid UTF-8.
    ///
    /// This avoids the path-to-string conversion when the input is already a `String`.
    #[must_use]
    pub const fn from_string(s: String) -> Self {
        Self(s)
    }

    /// Returns the path as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the path as a `&Path`.
    #[must_use]
    pub fn as_path(&self) -> &Path {
        Path::new(&self.0)
    }

    /// Consumes self and returns the inner `String`.
    #[must_use]
    pub fn into_inner(self) -> String {
        self.0
    }

    /// Converts into a `PathBuf`.
    #[must_use]
    pub fn into_path_buf(self) -> PathBuf {
        PathBuf::from(self.0)
    }
}

impl AsRef<str> for TagrPath {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl AsRef<Path> for TagrPath {
    fn as_ref(&self) -> &Path {
        Path::new(&self.0)
    }
}

impl Borrow<str> for TagrPath {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TagrPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
