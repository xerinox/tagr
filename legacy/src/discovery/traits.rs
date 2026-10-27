use std::path::PathBuf;

use crate::patterns::PatternError;

/// Kind of discovery implementation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoveryKind {
    Regex,
    Glob,
}

/// Trait for file discovery based on pre-validated patterns
pub trait FileDiscovery {
    /// Discover matching file paths from provided root directories.
    ///
    /// # Errors
    /// Returns `PatternError` if discovery fails due to pattern misuse or underlying I/O issues.
    fn discover(&self, roots: &[PathBuf]) -> Result<Vec<PathBuf>, PatternError>;

    /// Get the kind of discovery implementation
    #[must_use]
    fn kind(&self) -> DiscoveryKind;
}
