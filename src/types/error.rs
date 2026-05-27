//! Validation errors for newtype construction.

use std::fmt;
use thiserror::Error;

/// Errors returned when constructing newtypes (`TagName`, `TagrPath`, `FilterName`).
///
/// These are `Clone + PartialEq + Eq` so tests can assert exact error variants
/// with `assert_eq!`.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// The input string is empty.
    #[error("{kind} is empty")]
    Empty {
        /// Which newtype was being constructed.
        kind: NameKind,
    },

    /// The input exceeds the maximum allowed length.
    #[error("{kind} exceeds {max} characters: {len}")]
    TooLong {
        /// Which newtype was being constructed.
        kind: NameKind,
        /// Actual length of the input.
        len: usize,
        /// Maximum allowed length.
        max: usize,
    },

    /// The input contains a character not allowed in this context.
    #[error("invalid character '{ch}' in {kind} at position {pos}")]
    InvalidChar {
        /// Which newtype was being constructed.
        kind: NameKind,
        /// The offending character.
        ch: char,
        /// Byte position of the character in the input.
        pos: usize,
    },

    /// Tag name starts or ends with the hierarchy delimiter `:`.
    #[error("tag name cannot start or end with ':'")]
    LeadingOrTrailingDelimiter,

    /// Tag name contains an empty segment (consecutive `::` delimiters).
    #[error("tag name contains empty segment (::)")]
    EmptySegment,

    /// The first segment of the tag name is a reserved virtual tag prefix.
    #[error("'{prefix}' is a reserved virtual tag prefix")]
    ReservedVtagPrefix {
        /// The prefix that conflicts with a vtag.
        prefix: String,
    },

    /// A file path is not valid UTF-8.
    #[error("path is not valid UTF-8: {lossy_path}")]
    InvalidUtf8 {
        /// Lossy representation of the path for display.
        lossy_path: String,
    },
}

/// Identifies which newtype triggered a validation error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameKind {
    /// A tag name (`TagName`).
    Tag,
    /// A filter name (`FilterName`).
    Filter,
}

impl fmt::Display for NameKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tag => write!(f, "tag name"),
            Self::Filter => write!(f, "filter name"),
        }
    }
}
