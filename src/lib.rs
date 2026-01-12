//! Tagr - A file tagging system with optimized reverse lookups
//!
//! This library provides functionality for tagging files and performing
//! efficient searches using an embedded database with reverse indices.

use bincode::{self, Decode, Encode};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

pub mod browse;
pub mod cli;
pub mod commands;
pub mod config;
pub mod db;
pub mod discovery;
pub mod filters;
pub mod keybinds;
pub mod output;
pub mod patterns;
pub mod preview;
pub mod schema;
pub mod search;
pub mod ui;
pub mod vtags;

#[cfg(test)]
pub mod testing;

/// Error enum, contains all failure states of the program
#[derive(Debug, Error)]
pub enum TagrError {
    /// Database error
    #[error("Database error: {0}")]
    DbError(#[from] db::DbError),
    /// Search error
    #[error("Search error: {0}")]
    SearchError(#[from] search::SearchError),
    /// Browse error
    #[error("Browse error: {0}")]
    BrowseError(String),
    /// Filter error
    #[error("Filter error: {0}")]
    FilterError(#[from] filters::FilterError),
    /// UI error
    #[error("UI error: {0}")]
    UiError(#[from] ui::UiError),
    /// Preview error
    #[error("Preview error: {0}")]
    PreviewError(#[from] preview::PreviewError),
    /// Represents a configuration error
    #[error("Configuration error: {0}")]
    ConfigError(#[from] ::config::ConfigError),
    /// Represents an I/O error
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
    /// Pattern system error
    #[error("Pattern error: {0}")]
    PatternError(#[from] patterns::PatternError),
    /// Schema error
    #[error("Schema error: {0}")]
    SchemaError(#[from] schema::SchemaError),
    /// Invalid input error
    #[error("Invalid input: {0}")]
    InvalidInput(String),
}

/// Data struct containing the pairings of file and tags
#[derive(Encode, Decode, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Pair {
    pub file: PathBuf,
    pub tags: Vec<String>,
}

impl Pair {
    /// Create a new Pair
    #[must_use]
    pub const fn new(file: PathBuf, tags: Vec<String>) -> Self {
        Self { file, tags }
    }
}

impl search::AsFileTagPair for Pair {
    fn as_pair(&self) -> search::FileTagPair<'_> {
        // Convert PathBuf to &str - if invalid UTF-8, use empty string
        let file_str = self.file.to_str().unwrap_or("");
        search::FileTagPair::new(file_str, &self.tags)
    }
}
