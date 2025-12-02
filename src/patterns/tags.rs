use regex::Regex;

use super::error::{PatternError, PatternKind};

/// Tag pattern representation (literal or regex)
#[derive(Debug, Clone)]
pub enum TagPattern {
    Literal(String),
    Regex { original: String, compiled: Regex },
}

impl TagPattern {
    /// Construct a literal tag pattern.
    ///
    /// # Errors
    /// Returns `PatternError::InvalidEmpty` if `s` is empty.
    pub fn literal(s: &str) -> Result<Self, PatternError> {
        if s.is_empty() {
            return Err(PatternError::InvalidEmpty { kind: PatternKind::Tag });
        }
        Ok(Self::Literal(s.to_string()))
    }

    /// Construct a regex tag pattern.
    ///
    /// # Errors
    /// * Returns `PatternError::InvalidEmpty` if `p` is empty.
    /// * Returns `PatternError::RegexCompile` if the pattern fails to compile.
    pub fn regex(p: &str) -> Result<Self, PatternError> {
        if p.is_empty() {
            return Err(PatternError::InvalidEmpty { kind: PatternKind::Tag });
        }
        Regex::new(p)
            .map(|r| Self::Regex { original: p.to_string(), compiled: r })
            .map_err(|e| PatternError::regex_compile(p, &e.to_string()))
    }

    #[must_use]
    pub const fn is_regex(&self) -> bool {
        matches!(self, Self::Regex { .. })
    }

    #[must_use]
    pub const fn original(&self) -> &str {
        match self {
            Self::Literal(s) => s.as_str(),
            Self::Regex { original, .. } => original.as_str(),
        }
    }
}

impl PartialEq for TagPattern {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Literal(a), Self::Literal(b))
            | (Self::Regex { original: a, .. }, Self::Regex { original: b, .. }) => a == b,
            _ => false,
        }
    }
}

impl Eq for TagPattern {}

/// Query over tag patterns with a search mode
#[derive(Debug, Clone)]
pub struct TagQuery {
    pub patterns: Vec<TagPattern>,
    pub mode: crate::cli::SearchMode,
}

impl TagQuery {
    /// Create a new `TagQuery` ensuring pattern count does not exceed `max`.
    ///
    /// # Errors
    /// Returns `PatternError::TooManyPatterns` when `patterns.len() > max`.
    pub fn new(patterns: Vec<TagPattern>, mode: crate::cli::SearchMode, max: usize) -> Result<Self, PatternError> {
        if patterns.len() > max {
            return Err(PatternError::TooManyPatterns { provided: patterns.len(), max });
        }
        Ok(Self { patterns, mode })
    }
}
