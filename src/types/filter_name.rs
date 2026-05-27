//! `FilterName` — validated saved-filter identifier.

use std::borrow::Borrow;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::error::{NameKind, ValidationError};
use super::MAX_FILTER_NAME_LEN;

/// A validated filter name.
///
/// Filter names are non-empty strings containing only alphanumeric characters,
/// hyphens (`-`), and underscores (`_`). Max 64 characters.
///
/// # Examples
///
/// ```
/// use tagr::types::FilterName;
///
/// let name = FilterName::new("my-rust-filter").unwrap();
/// assert_eq!(name.as_str(), "my-rust-filter");
/// ```
///
/// # Errors
///
/// [`FilterName::new`] returns [`ValidationError`] for invalid input.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub struct FilterName(String);

impl FilterName {
    /// Create a new `FilterName`, validating the input.
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError`] if the input is empty, too long, or contains
    /// characters other than alphanumeric, hyphens, or underscores.
    pub fn new(s: impl Into<String>) -> Result<Self, ValidationError> {
        let s = s.into();
        validate_filter_name(&s)?;
        Ok(Self(s))
    }

    /// Returns the inner string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes self and returns the inner `String`.
    #[must_use]
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl AsRef<str> for FilterName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for FilterName {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for FilterName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn validate_filter_name(s: &str) -> Result<(), ValidationError> {
    if s.is_empty() {
        return Err(ValidationError::Empty {
            kind: NameKind::Filter,
        });
    }

    if s.len() > MAX_FILTER_NAME_LEN {
        return Err(ValidationError::TooLong {
            kind: NameKind::Filter,
            len: s.len(),
            max: MAX_FILTER_NAME_LEN,
        });
    }

    for (pos, ch) in s.char_indices() {
        if !is_valid_filter_char(ch) {
            return Err(ValidationError::InvalidChar {
                kind: NameKind::Filter,
                ch,
                pos,
            });
        }
    }

    Ok(())
}

/// Allowed characters in filter names: alphanumeric, `-`, `_`.
fn is_valid_filter_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '-' || ch == '_'
}
