use std::path::{Path, PathBuf};

use glob::Pattern as GlobPattern;
use regex::Regex;

use super::error::{PatternError, PatternKind};

/// File pattern representation (literal path, regex, or glob)
#[derive(Debug, Clone)]
pub enum FilePattern {
    Literal(PathBuf),
    Regex { original: String, compiled: Regex },
    Glob { original: String, spec: GlobPattern },
}

impl FilePattern {
    /// Construct a literal file pattern.
    ///
    /// # Errors
    /// Returns `PatternError::InvalidEmpty` if the path renders as an empty string.
    pub fn literal(path: &Path) -> Result<Self, PatternError> {
        let s = path.to_string_lossy();
        if s.is_empty() {
            return Err(PatternError::InvalidEmpty {
                kind: PatternKind::File,
            });
        }
        Ok(Self::Literal(path.to_path_buf()))
    }

    /// Construct a regex file pattern.
    ///
    /// # Errors
    /// * Returns `PatternError::InvalidEmpty` if `p` is empty.
    /// * Returns `PatternError::RegexCompile` if the regex fails to compile.
    pub fn regex(p: &str) -> Result<Self, PatternError> {
        if p.is_empty() {
            return Err(PatternError::InvalidEmpty {
                kind: PatternKind::File,
            });
        }
        Regex::new(p)
            .map(|r| Self::Regex {
                original: p.to_string(),
                compiled: r,
            })
            .map_err(|e| PatternError::regex_compile(p, &e.to_string()))
    }

    /// Construct a glob file pattern.
    ///
    /// # Errors
    /// * Returns `PatternError::InvalidEmpty` if `p` is empty.
    /// * Returns `PatternError::GlobParse` if the glob specification is invalid.
    pub fn glob(p: &str) -> Result<Self, PatternError> {
        if p.is_empty() {
            return Err(PatternError::InvalidEmpty {
                kind: PatternKind::File,
            });
        }
        GlobPattern::new(p)
            .map(|g| Self::Glob {
                original: p.to_string(),
                spec: g,
            })
            .map_err(|e| PatternError::glob_parse(p, &e.to_string()))
    }

    #[must_use]
    pub const fn is_regex(&self) -> bool {
        matches!(self, Self::Regex { .. })
    }

    #[must_use]
    pub const fn is_glob(&self) -> bool {
        matches!(self, Self::Glob { .. })
    }

    #[must_use]
    pub fn original(&self) -> String {
        match self {
            Self::Literal(p) => p.to_string_lossy().into_owned(),
            Self::Regex { original, .. } | Self::Glob { original, .. } => original.clone(),
        }
    }
}

/// Query over file patterns with a search mode
#[derive(Debug, Clone)]
pub struct FileQuery {
    pub patterns: Vec<FilePattern>,
    pub mode: crate::cli::SearchMode,
}

impl FileQuery {
    /// Create a new `FileQuery` ensuring pattern count does not exceed `max`.
    ///
    /// # Errors
    /// Returns `PatternError::TooManyPatterns` when `patterns.len() > max`.
    pub fn new(
        patterns: Vec<FilePattern>,
        mode: crate::cli::SearchMode,
        max: usize,
    ) -> Result<Self, PatternError> {
        if patterns.len() > max {
            return Err(PatternError::TooManyPatterns {
                provided: patterns.len(),
                max,
            });
        }
        Ok(Self { patterns, mode })
    }
}

impl PartialEq for FilePattern {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Literal(a), Self::Literal(b)) => a == b,
            (Self::Regex { original: a, .. }, Self::Regex { original: b, .. })
            | (Self::Glob { original: a, .. }, Self::Glob { original: b, .. }) => a == b,
            _ => false,
        }
    }
}

impl Eq for FilePattern {}
