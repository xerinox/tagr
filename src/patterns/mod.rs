//! Pattern system module - typed representations for tag and file patterns.

pub mod error;
pub mod tags;
pub mod files;

pub use error::{PatternError, PatternKind};
pub use tags::{TagPattern, TagQuery};
pub use files::{FilePattern, FileQuery};

/// Maximum number of patterns allowed in a single query (subject to tuning)
const MAX_PATTERNS: usize = 1000;

/// Context in which file patterns are interpreted
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternContext {
    /// Bulk file operations (tag/untag etc.) allow implicit glob detection
    BulkFiles,
    /// Non-bulk search/browse requires explicit flags for glob/regex
    SearchFiles,
}

/// Builder collecting raw CLI tokens and producing typed queries
#[derive(Debug)]
pub struct PatternBuilder {
    tag_tokens: Vec<String>,
    file_tokens: Vec<String>,
    regex_tags: bool,
    regex_files: bool,
    glob_files_flag: bool,
    context: PatternContext,
}

impl PatternBuilder {
    #[must_use]
    pub const fn new(context: PatternContext) -> Self { Self { tag_tokens: Vec::new(), file_tokens: Vec::new(), regex_tags: false, regex_files: false, glob_files_flag: false, context } }

    /// Set whether tag tokens should be interpreted as regex patterns.
    #[must_use]
    pub const fn regex_tags(mut self, v: bool) -> Self { self.regex_tags = v; self }
    /// Set whether file tokens should be interpreted as regex patterns.
    #[must_use]
    pub const fn regex_files(mut self, v: bool) -> Self { self.regex_files = v; self }
    /// Set whether file tokens should be treated as globs (explicit flag or bulk implicit logic).
    #[must_use]
    pub const fn glob_files_flag(mut self, v: bool) -> Self { self.glob_files_flag = v; self }

    pub fn add_tag_token<S: Into<String>>(&mut self, token: S) { self.tag_tokens.push(token.into()); }
    pub fn add_file_token<S: Into<String>>(&mut self, token: S) { self.file_tokens.push(token.into()); }

    fn is_glob_token(token: &str) -> bool {
        token.contains('*') || token.contains('?') || token.contains('[')
    }

    /// Build typed queries from collected tokens.
    ///
    /// # Errors
    /// * Returns `PatternError::MixedPatternMisuse` if a glob-like token is supplied as a tag without regex flag.
    /// * Returns pattern compilation / parse errors from regex or glob construction.
    /// * Returns `PatternError::InvalidEmpty` for empty tokens in literal/regex/glob constructors.
    pub fn build(self, tag_mode: crate::cli::SearchMode, file_mode: crate::cli::SearchMode) -> Result<(TagQuery, FileQuery), PatternError> {
        let mut tag_patterns = Vec::with_capacity(self.tag_tokens.len());
        for t in &self.tag_tokens {
            if self.regex_tags {
                tag_patterns.push(TagPattern::regex(t)?);
            } else if Self::is_glob_token(t) {
                // Prevent accidental glob usage in tag context
                return Err(PatternError::MixedPatternMisuse { detail: format!("Glob-like token '{t}' supplied as tag. Use --glob-files for file patterns or remove wildcards.") });
            } else {
                tag_patterns.push(TagPattern::literal(t)?);
            }
        }
        let mut file_patterns = Vec::with_capacity(self.file_tokens.len());
        for f in &self.file_tokens {
            if self.regex_files {
                file_patterns.push(FilePattern::regex(f)?);
                continue;
            }
            if self.glob_files_flag || (self.context == PatternContext::BulkFiles && Self::is_glob_token(f)) {
                file_patterns.push(FilePattern::glob(f)?);
            } else {
                file_patterns.push(FilePattern::literal(std::path::Path::new(f))?);
            }
        }
        let tag_query = build_tag_query(tag_patterns, tag_mode)?;
        let file_query = build_file_query(file_patterns, file_mode)?;
        Ok((tag_query, file_query))
    }
}

/// Helper to build a `TagQuery`
///
/// # Errors
/// Returns `PatternError::TooManyPatterns` if pattern count exceeds the configured maximum.
pub fn build_tag_query(patterns: Vec<TagPattern>, mode: crate::cli::SearchMode) -> Result<TagQuery, PatternError> {
    TagQuery::new(patterns, mode, MAX_PATTERNS)
}

/// Helper to build a `FileQuery`
///
/// # Errors
/// Returns `PatternError::TooManyPatterns` if pattern count exceeds the configured maximum.
pub fn build_file_query(patterns: Vec<FilePattern>, mode: crate::cli::SearchMode) -> Result<FileQuery, PatternError> {
    FileQuery::new(patterns, mode, MAX_PATTERNS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bulk_implicit_glob_detection() {
        let mut builder = PatternBuilder::new(PatternContext::BulkFiles)
            .regex_files(false)
            .glob_files_flag(false);
        builder.add_file_token("src/**/*.rs");
        let (_tq, fq) = builder.build(crate::cli::SearchMode::All, crate::cli::SearchMode::All).expect("builder should succeed");
        // Expect one glob pattern in file query
        assert_eq!(fq.patterns.len(), 1);
        match &fq.patterns[0] {
            FilePattern::Glob { .. } => {}
            _ => panic!("Expected glob classification in bulk context"),
        }
    }

    #[test]
    fn test_mixed_glob_like_tag_is_error() {
        let mut builder = PatternBuilder::new(PatternContext::BulkFiles)
            .regex_tags(false);
        builder.add_tag_token("feature/*");
        let err = builder.build(crate::cli::SearchMode::All, crate::cli::SearchMode::All).err().expect("should error");
        match err {
            PatternError::MixedPatternMisuse { .. } => {}
            _ => panic!("Expected MixedPatternMisuse error for glob-like tag"),
        }
    }

    #[test]
    fn test_search_requires_flag_for_glob() {
        // In search context without glob flag, glob-like file token becomes literal
        let mut builder = PatternBuilder::new(PatternContext::SearchFiles)
            .regex_files(false)
            .glob_files_flag(false);
        builder.add_file_token("*.md");
        let (_tq, fq) = builder.build(crate::cli::SearchMode::All, crate::cli::SearchMode::All).expect("builder should succeed");
        match &fq.patterns[0] {
            FilePattern::Literal(_) => {}
            _ => panic!("Expected literal classification without --glob-files in search context"),
        }

        // With explicit glob flag, it should classify as glob
        let mut builder = PatternBuilder::new(PatternContext::SearchFiles)
            .regex_files(false)
            .glob_files_flag(true);
        builder.add_file_token("*.md");
        let (_tq, fq) = builder.build(crate::cli::SearchMode::All, crate::cli::SearchMode::All).expect("builder should succeed");
        match &fq.patterns[0] {
            FilePattern::Glob { .. } => {}
            _ => panic!("Expected glob classification with --glob-files in search context"),
        }
    }
}
