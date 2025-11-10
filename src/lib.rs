//! Tagr - A file tagging system with optimized reverse lookups
//! 
//! This library provides functionality for tagging files and performing
//! efficient searches using an embedded database with reverse indices.

use std::path::PathBuf;
use serde::{Serialize, Deserialize};
use bincode::{self, Decode, Encode};
use thiserror::Error;

pub mod db;
pub mod cli;
pub mod commands;
pub mod config;
pub mod output;
pub mod search;

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
    /// Represents a configuration error
    #[error("Configuration error: {0}")]
    ConfigError(#[from] ::config::ConfigError),
    /// Represents an I/O error
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
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


