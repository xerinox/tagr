//! Output formatting for CLI display
//!
//! This module provides utilities for formatting output in the CLI,
//! including path display formatting and file/tag formatting.

use colored::Colorize;
use std::path::Path;
use crate::config::PathFormat;

/// Format a path according to the display mode
pub fn format_path(path: &Path, format: PathFormat) -> String {
    match format {
        PathFormat::Absolute => path.display().to_string(),
        PathFormat::Relative => {
            if let Ok(cwd) = std::env::current_dir()
                && let Ok(rel_path) = path.strip_prefix(&cwd) {
                    return rel_path.display().to_string();
                }
            // Fallback to absolute if relative path cannot be computed
            path.display().to_string()
        }
    }
}

/// Format a file with its tags for display
pub fn file_with_tags(path: &Path, tags: &[String], format: PathFormat, quiet: bool) -> String {
    let path_str = format_path(path, format);
    
    if quiet {
        path_str
    } else if tags.is_empty() {
        format!("  {} (no tags)", path_str)
    } else {
        format!("  {} [{}]", path_str, tags.join(", "))
    }
}

/// Format a tag with usage count
pub fn tag_with_count(tag: &str, count: usize, quiet: bool) -> String {
    if quiet {
        tag.to_string()
    } else {
        format!("  {} (used by {} file(s))", tag, count)
    }
}

/// Color a path based on file existence (green if exists, red if missing)
pub fn colorize_path(path: &Path, format: PathFormat) -> String {
    let formatted = format_path(path, format);
    if path.exists() {
        formatted.green().to_string()
    } else {
        formatted.red().to_string()
    }
}
