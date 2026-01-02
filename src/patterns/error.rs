use thiserror::Error;

/// Kind of pattern for error context
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternKind {
    Tag,
    File,
}

/// Errors produced while constructing or using patterns
#[derive(Debug, Error)]
pub enum PatternError {
    /// Empty pattern is invalid
    #[error("Empty {kind:?} pattern provided")]
    InvalidEmpty { kind: PatternKind },
    /// Regex failed to compile
    #[error("Invalid regex pattern '{pattern}': {reason}")]
    InvalidRegex { pattern: String, reason: String },
    /// Glob failed to parse
    #[error("Invalid glob pattern '{pattern}': {reason}")]
    InvalidGlob { pattern: String, reason: String },
    /// Misuse of mixed pattern kinds in a restricted context
    #[error("Mixed pattern misuse: {detail}")]
    MixedPatternMisuse { detail: String },
    /// Too many patterns were provided
    #[error("Too many patterns provided: {provided} (max {max})")]
    TooManyPatterns { provided: usize, max: usize },
    /// Unsupported feature attempted
    #[error("Unsupported feature: {feature}")]
    UnsupportedFeature { feature: String },
    /// Attempted conversion that does not apply (e.g., Into<Regex> for non-regex variant)
    #[error("Incompatible conversion: {detail}")]
    IncompatibleConversion { detail: String },
}

impl PatternError {
    #[must_use]
    pub fn regex_compile(pattern: &str, reason: &str) -> Self {
        Self::InvalidRegex {
            pattern: pattern.to_string(),
            reason: reason.to_string(),
        }
    }

    #[must_use]
    pub fn glob_parse(pattern: &str, reason: &str) -> Self {
        Self::InvalidGlob {
            pattern: pattern.to_string(),
            reason: reason.to_string(),
        }
    }
}
