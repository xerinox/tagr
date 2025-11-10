//! Query composition logic for building file searches
//!
//! This module provides shared query building functionality used by both
//! search and browse commands to construct file lists based on search parameters.

use std::path::PathBuf;
use std::collections::HashSet;
use crate::db::{Database, DbError};
use crate::cli::{SearchParams, SearchMode};
use crate::search::filter::{PathFilterExt, PathTagFilterExt};

/// Apply search parameters to build a filtered file list
///
/// Constructs a list of files based on the search criteria in params:
/// - General query mode: searches both tags (regex) and filenames
/// - Tag mode: finds files by tags (all/any)
/// - No criteria: returns all files
///
/// Then applies file pattern filters and tag exclusions.
///
/// # Arguments
/// * `db` - Database to query
/// * `params` - Search parameters containing query, tags, patterns, and exclusions
///
/// # Returns
/// Vector of file paths matching the search criteria
///
/// # Errors
/// Returns `DbError` if database operations fail or pattern validation fails
///
/// # Examples
/// ```ignore
/// let params = SearchParams {
///     query: None,
///     tags: vec!["rust".to_string()],
///     tag_mode: SearchMode::Any,
///     ..Default::default()
/// };
/// let files = apply_search_params(&db, &params)?;
/// ```
pub fn apply_search_params(
    db: &Database,
    params: &SearchParams,
) -> Result<Vec<PathBuf>, DbError> {
    // Build initial file list based on query/tags
    let mut files = if let Some(query) = &params.query {
        // General query: search both tags (regex) and filenames (glob)
        let files_by_tag = db.find_by_tag_regex(query)?;
        
        let all_files = db.list_all_files()?;
        let filename_pattern = format!("*{query}*");
        let files_by_name = all_files.into_iter()
            .filter_glob_any(&[filename_pattern])?;
        
        // Union and deduplicate
        let mut file_set: HashSet<_> = files_by_tag.into_iter().collect();
        file_set.extend(files_by_name);
        let mut files: Vec<_> = file_set.into_iter().collect();
        files.sort();
        files
    } else if !params.tags.is_empty() {
        // Tag-based search with all/any logic
        match params.tag_mode {
            SearchMode::All => db.find_by_all_tags(&params.tags)?,
            SearchMode::Any => db.find_by_any_tag(&params.tags)?,
        }
    } else {
        // No criteria: return all files
        db.list_all_files()?
    };

    // Apply file pattern filters if specified
    if !params.file_patterns.is_empty() {
        let match_all = params.file_mode == SearchMode::All;
        files = files.into_iter()
            .filter_patterns(&params.file_patterns, params.regex_file, match_all)?;
    }

    // Apply tag exclusions if specified
    if !params.exclude_tags.is_empty() {
        files = files.exclude_tags(db, &params.exclude_tags)?;
    }

    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_apply_search_params_compiles() {
        // This test just ensures the module compiles and the function signature is correct
        // Real testing happens in integration tests with actual database
        let _: fn(&Database, &SearchParams) -> Result<Vec<PathBuf>, DbError> = apply_search_params;
    }
}
