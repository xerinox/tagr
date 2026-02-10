//! Abstraction layer for completion APIs
//!
//! Wraps clap_complete types to isolate upstream API changes.
//! If clap_complete changes its API, only this module needs updating.

use std::ffi::OsStr;

/// A completion suggestion returned to the shell
#[derive(Debug, Clone)]
pub struct Candidate {
    /// The value to insert
    pub value: String,
    /// Optional help text shown alongside
    pub help: Option<String>,
}

impl Candidate {
    /// Create a new candidate with just a value
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            help: None,
        }
    }

    /// Add help text to the candidate
    #[must_use]
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
}

/// Trait for dynamic completers (database lookups, etc.)
///
/// Implementations should be fast and never panic - return empty vec on errors.
pub trait DynamicCompleter: Send + Sync {
    /// Generate completion candidates based on current input
    fn complete(&self, current: &OsStr) -> Vec<Candidate>;
}

/// Trait for static candidate sources
pub trait StaticCandidates {
    /// Return all possible candidates
    fn candidates(&self) -> Vec<Candidate>;
}

// Adapter implementations that convert to/from clap_complete types
#[cfg(feature = "dynamic-completions")]
mod adapters {
    use super::*;
    use clap_complete::engine::{ArgValueCompleter, CompletionCandidate};

    impl From<Candidate> for CompletionCandidate {
        fn from(c: Candidate) -> Self {
            let mut candidate = CompletionCandidate::new(c.value);
            if let Some(help) = c.help {
                candidate = candidate.help(Some(help.into()));
            }
            candidate
        }
    }

    impl From<CompletionCandidate> for Candidate {
        fn from(c: CompletionCandidate) -> Self {
            Self {
                value: c.get_value().to_string_lossy().to_string(),
                help: c.get_help().map(|h| h.to_string()),
            }
        }
    }

    /// Convert a DynamicCompleter into clap's ArgValueCompleter
    pub fn to_arg_completer<C: DynamicCompleter + 'static>(completer: C) -> ArgValueCompleter {
        ArgValueCompleter::new(move |current: &OsStr| {
            completer
                .complete(current)
                .into_iter()
                .map(CompletionCandidate::from)
                .collect()
        })
    }
}

#[cfg(feature = "dynamic-completions")]
pub use adapters::to_arg_completer;
