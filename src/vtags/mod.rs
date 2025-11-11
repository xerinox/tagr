//! Virtual tags module for dynamic file metadata queries.
//!
//! This module provides virtual tags - dynamically computed filters based on file system
//! metadata that require zero database storage. Virtual tags enable powerful queries on
//! file properties like modification time, size, extension, location, and permissions.
//!
//! # Features
//!
//! - **Time-based queries**: Filter by modified, created, or accessed timestamps
//! - **Size-based queries**: Filter by file size with categories or specific ranges
//! - **Extension queries**: Filter by file extension or extension type categories
//! - **Location queries**: Filter by directory, path pattern, or depth
//! - **Permission queries**: Filter by file permissions (executable, readable, etc.)
//! - **Content queries**: Filter by line count or other content properties
//! - **Git queries**: Filter by Git status (tracked, modified, staged, etc.)
//! - **Metadata caching**: TTL-based caching for performance
//! - **Parallel evaluation**: Uses rayon for efficient multi-threaded processing
//!
//! # Examples
//!
//! ```no_run
//! use tagr::vtags::{VirtualTagParser, VirtualTagEvaluator, VirtualTagConfig};
//! use std::path::Path;
//! use std::time::Duration;
//!
//! // Parse a virtual tag
//! let config = VirtualTagConfig::default();
//! let parser = VirtualTagParser::new(config.clone());
//! let vtag = parser.parse("modified:today").unwrap();
//!
//! // Evaluate against a file
//! let cache_ttl = Duration::from_secs(300);
//! let mut evaluator = VirtualTagEvaluator::new(cache_ttl, config);
//! let matches = evaluator.matches(Path::new("file.txt"), &vtag).unwrap();
//! ```
//!
//! # Modules
//!
//! - [`types`]: Virtual tag type definitions and enums
//! - [`parser`]: Parse virtual tag strings into VirtualTag enums
//! - [`evaluator`]: Evaluate virtual tags against file metadata
//! - [`cache`]: Metadata caching layer for performance
//! - [`config`]: Virtual tag configuration and defaults

pub mod cache;
pub mod config;
pub mod evaluator;
pub mod parser;
pub mod types;

pub use cache::{FileMetadata, MetadataCache};
pub use config::VirtualTagConfig;
pub use evaluator::VirtualTagEvaluator;
pub use parser::{ParseError, VirtualTagParser};
pub use types::{
    ExtTypeCategory, GitCondition, PermissionCondition, RangeCondition, SizeCategory,
    SizeCondition, TimeCondition, VirtualTag,
};
