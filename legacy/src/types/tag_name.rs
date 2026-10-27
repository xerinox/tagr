//! `TagName` — validated, hierarchical tag identifier.

use std::borrow::Borrow;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::error::{NameKind, ValidationError};
use super::{HIERARCHY_DELIMITER, MAX_TAG_NAME_LEN, RESERVED_VTAG_PREFIXES};

/// A validated tag name.
///
/// Tag names are non-empty strings containing only alphanumeric characters,
/// hyphens (`-`), underscores (`_`), dots (`.`), and the hierarchy delimiter
/// (`:`). They are case-sensitive, max 128 characters, and must not start or
/// end with `:` or contain consecutive `::`.
///
/// The first segment (before the first `:`) must not be a reserved virtual tag
/// prefix (e.g. `"modified"`, `"size"`, `"ext"`).
///
/// # Hierarchy
///
/// Tags support a colon-delimited hierarchy: `"language:rust:async"` has depth 3,
/// root `"language"`, and parent `"language:rust"`. The `':'` character (0x3A)
/// sorts before all alphanumeric characters, so lexicographic `Ord` on the inner
/// `String` naturally places parent tags before their children.
///
/// # Examples
///
/// ```
/// use tagr::types::TagName;
///
/// let tag = TagName::new("language:rust").unwrap();
/// assert_eq!(tag.depth(), 2);
/// assert_eq!(tag.root(), "language");
/// assert_eq!(tag.parent().unwrap().as_str(), "language");
/// ```
///
/// # Errors
///
/// [`TagName::new`] returns [`ValidationError`] for invalid input.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub struct TagName(String);

impl TagName {
    /// Create a new `TagName`, validating the input.
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError`] if the input is empty, too long, contains
    /// invalid characters, has leading/trailing delimiters, empty segments,
    /// or uses a reserved vtag prefix.
    pub fn new(s: impl Into<String>) -> Result<Self, ValidationError> {
        let s = s.into();
        validate_tag_name(&s)?;
        Ok(Self(s))
    }

    /// Create a `TagName` without validation.
    ///
    /// Used for regex patterns that need to pass through as raw strings
    /// without tag name validation (e.g., `"lang:.*"` contains invalid chars).
    #[must_use]
    pub const fn from_raw(s: String) -> Self {
        Self(s)
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

    /// Returns the hierarchy depth (number of segments).
    ///
    /// `"a"` → 1, `"a:b"` → 2, `"a:b:c"` → 3.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.0.matches(HIERARCHY_DELIMITER).count() + 1
    }

    /// Returns the root segment (everything before the first `:`).
    ///
    /// `"a:b:c"` → `"a"`, `"a"` → `"a"`.
    #[must_use]
    pub fn root(&self) -> &str {
        self.0.split(HIERARCHY_DELIMITER).next().unwrap_or(&self.0)
    }

    /// Returns the parent tag, or `None` if this is a root tag (depth 1).
    ///
    /// `"a:b:c"` → `Some("a:b")`, `"a"` → `None`.
    #[must_use]
    pub fn parent(&self) -> Option<Self> {
        self.0.rfind(HIERARCHY_DELIMITER).map(|pos| {
            // Parent is always valid if self is valid — no re-validation needed.
            Self(self.0[..pos].to_string())
        })
    }

    /// Returns an iterator over the colon-separated segments.
    ///
    /// `"a:b:c"` → `["a", "b", "c"]`.
    pub fn segments(&self) -> impl Iterator<Item = &str> {
        self.0.split(HIERARCHY_DELIMITER)
    }

    /// Creates a child tag by appending a segment.
    ///
    /// `"a:b".join("c")` → `Ok("a:b:c")`.
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError`] if the resulting tag name is invalid
    /// (e.g. exceeds max length or `child` contains invalid characters).
    pub fn join(&self, child: &str) -> Result<Self, ValidationError> {
        let joined = format!("{}{HIERARCHY_DELIMITER}{child}", self.0);
        Self::new(joined)
    }

    /// Returns `true` if this tag is a direct or indirect child of `parent`.
    ///
    /// `"a:b:c".is_child_of("a")` → `true`
    /// `"a:b:c".is_child_of("a:b")` → `true`
    /// `"a:b".is_child_of("a:b")` → `false` (not a child of itself)
    #[must_use]
    pub fn is_child_of(&self, parent: &Self) -> bool {
        let prefix = format!("{}{HIERARCHY_DELIMITER}", parent.0);
        self.0.starts_with(&prefix)
    }

    /// Returns `true` if this tag matches a pattern via prefix matching.
    ///
    /// A pattern matches if it equals the tag or is a prefix at a segment boundary.
    /// `"language"` matches `"language"` and `"language:rust"` but not `"languages"`.
    #[must_use]
    pub fn matches_pattern(&self, pattern: &Self) -> bool {
        if self.0 == pattern.0 {
            return true;
        }
        let prefix = format!("{}{HIERARCHY_DELIMITER}", pattern.0);
        self.0.starts_with(&prefix)
    }

    /// Glob matching on colon-separated segments.
    ///
    /// - `*` matches exactly one segment
    /// - `**` matches one or more segments
    ///
    /// # Examples
    ///
    /// - `"language:*:arrays"` matches `"language:rust:arrays"` but not `"math:arrays"`
    /// - `"*:rust"` matches `"language:rust"` but not `"metal:iron:rust"`
    /// - `"**:rust"` matches `"language:rust"` AND `"metal:iron:rust"`
    #[must_use]
    pub fn matches_glob(&self, pattern: &str) -> bool {
        let tag_segments: Vec<&str> = self.0.split(HIERARCHY_DELIMITER).collect();
        let pat_segments: Vec<&str> = pattern.split(HIERARCHY_DELIMITER).collect();
        glob_match(&tag_segments, &pat_segments)
    }
}

/// Recursive glob matcher on segment arrays.
fn glob_match(tag: &[&str], pat: &[&str]) -> bool {
    match (tag, pat) {
        // Both exhausted — match
        ([], []) => true,
        // Pattern exhausted but tag remains — no match
        (_, []) => false,
        // Tag exhausted but pattern remains — only match if remaining pattern is all `**`
        ([], [p, rest @ ..]) => *p == "**" && glob_match(&[], rest),
        // `**` matches one or more segments
        ([_, tag_rest @ ..], [p, pat_rest @ ..]) if *p == "**" => {
            // Try consuming current segment with `**`, or move past `**`
            glob_match(tag_rest, pat) || glob_match(tag, pat_rest)
        }
        // `*` matches exactly one segment
        ([_, tag_rest @ ..], [p, pat_rest @ ..]) if *p == "*" => glob_match(tag_rest, pat_rest),
        // Literal match
        ([t, tag_rest @ ..], [p, pat_rest @ ..]) => *t == *p && glob_match(tag_rest, pat_rest),
    }
}

impl AsRef<str> for TagName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for TagName {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TagName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<&str> for TagName {
    type Error = ValidationError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::new(s)
    }
}

impl TryFrom<String> for TagName {
    type Error = ValidationError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::new(s)
    }
}

/// Validates a tag name string, returning the first error found.
fn validate_tag_name(s: &str) -> Result<(), ValidationError> {
    if s.is_empty() {
        return Err(ValidationError::Empty {
            kind: NameKind::Tag,
        });
    }

    if s.len() > MAX_TAG_NAME_LEN {
        return Err(ValidationError::TooLong {
            kind: NameKind::Tag,
            len: s.len(),
            max: MAX_TAG_NAME_LEN,
        });
    }

    if s.starts_with(HIERARCHY_DELIMITER) || s.ends_with(HIERARCHY_DELIMITER) {
        return Err(ValidationError::LeadingOrTrailingDelimiter);
    }

    if s.contains("::") {
        return Err(ValidationError::EmptySegment);
    }

    for (pos, ch) in s.char_indices() {
        if !is_valid_tag_char(ch) {
            return Err(ValidationError::InvalidChar {
                kind: NameKind::Tag,
                ch,
                pos,
            });
        }
    }

    // Check reserved vtag prefixes — first segment only
    let root = s.split(HIERARCHY_DELIMITER).next().unwrap_or(s);
    if RESERVED_VTAG_PREFIXES.contains(&root) {
        return Err(ValidationError::ReservedVtagPrefix {
            prefix: root.to_string(),
        });
    }

    Ok(())
}

/// Returns `true` if the character is allowed in tag names.
///
/// Allowed: alphanumeric, `-`, `_`, `.`, `:` (hierarchy delimiter).
fn is_valid_tag_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '-' || ch == '_' || ch == '.' || ch == HIERARCHY_DELIMITER
}
