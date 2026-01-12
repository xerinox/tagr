//! Query composition logic for building file searches
//!
//! This module provides shared query building functionality used by both
//! search and browse commands to construct file lists based on search parameters.

use crate::cli::{SearchMode, SearchParams};
use crate::db::{Database, DbError};
use crate::search::filter::{PathFilterExt, PathTagFilterExt};
use crate::search::hierarchy;
use crate::vtags::{VirtualTag, VirtualTagConfig, VirtualTagEvaluator};
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

/// Apply search parameters to build a filtered file list
///
/// Constructs a list of files based on the search criteria in params:
/// - General query mode: searches both tags (regex) and filenames
/// - Tag mode: finds files by tags (all/any) with hierarchical filtering
/// - No criteria: returns all files
///
/// Tag matching uses hierarchical prefix matching and specificity rules:
/// - `-t lang` matches any tag starting with `lang:` (e.g., `lang:rust`)
/// - Deeper tags override shallower ones within the same hierarchy
/// - Excludes always win against includes from different hierarchies
///
/// Then applies file pattern filters and tag exclusions.
///
/// If params contains tags and regex mode is disabled, tags will be expanded
/// using the schema to include synonyms and (if `no_hierarchy` is false) parent levels.
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
pub fn apply_search_params(db: &Database, params: &SearchParams) -> Result<Vec<PathBuf>, DbError> {
    // Expand tags via schema if not in regex mode
    let mut expanded_params = params.clone();
    let original_tag_count = params.tags.len();

    if !params.tags.is_empty() && !params.regex_tag {
        // Load schema (gracefully handle missing schema)
        if let Ok(schema) = crate::schema::load_default_schema() {
            let include_hierarchy = !params.no_hierarchy;
            let expanded =
                crate::search::expand_tags(&params.tags, &schema, db, include_hierarchy)?;
            expanded_params.tags = expanded;

            // If tags were expanded from synonyms/hierarchy and user specified only 1 tag originally,
            // switch to ANY mode (OR logic) instead of ALL (AND logic) for intuitive behavior
            if original_tag_count == 1 && expanded_params.tags.len() > 1 {
                expanded_params.tag_mode = SearchMode::Any;
            }
        }
    }

    let mut files = if let Some(query) = &expanded_params.query {
        let files_by_tag = db.find_by_tag_regex(query)?;

        let all_files = db.list_all_files()?;
        let filename_pattern = format!("*{query}*");
        let files_by_name = all_files.into_iter().filter_glob_any(&[filename_pattern])?;

        let mut file_set: HashSet<_> = files_by_tag.into_iter().collect();
        file_set.extend(files_by_name);
        let mut files: Vec<_> = file_set.into_iter().collect();
        files.sort();
        files
    } else if !expanded_params.tags.is_empty() {
        if expanded_params.regex_tag {
            // Handle regex tag matching
            match expanded_params.tag_mode {
                SearchMode::All => {
                    // For ALL mode with regex, we need to find files that match all patterns
                    if expanded_params.tags.is_empty() {
                        Vec::new()
                    } else {
                        // Get files matching each regex pattern
                        let mut file_sets: Vec<HashSet<PathBuf>> = Vec::new();
                        for tag_pattern in &expanded_params.tags {
                            let matching_files = db.find_by_tag_regex(tag_pattern)?;
                            file_sets.push(matching_files.into_iter().collect());
                        }

                        // Find intersection of all sets
                        let first_set = file_sets.remove(0);
                        let result: Vec<PathBuf> = first_set
                            .into_iter()
                            .filter(|file| file_sets.iter().all(|set| set.contains(file)))
                            .collect();
                        result
                    }
                }
                SearchMode::Any => {
                    // For ANY mode with regex, collect all files matching any pattern
                    let mut file_set = HashSet::new();
                    for tag_pattern in &expanded_params.tags {
                        let matching_files = db.find_by_tag_regex(tag_pattern)?;
                        file_set.extend(matching_files);
                    }
                    let mut files: Vec<_> = file_set.into_iter().collect();
                    files.sort();
                    files
                }
            }
        } else {
            // Handle exact tag matching with hierarchical filtering
            // When no_hierarchy is false, use hierarchical prefix matching
            if params.no_hierarchy {
                // Traditional exact matching
                match expanded_params.tag_mode {
                    SearchMode::All => db.find_by_all_tags(&expanded_params.tags)?,
                    SearchMode::Any => db.find_by_any_tag(&expanded_params.tags)?,
                }
            } else {
                // Hierarchical matching with specificity rules
                // Note: tag_mode (Any/All) is handled by the hierarchical filter
                // For now, we treat it as ANY since hierarchical matching is more nuanced
                // Get all files from database and filter using hierarchical logic
                let all_files = db.list_all()?;

                let files_with_tags: Vec<(String, Vec<String>)> = all_files
                    .into_iter()
                    .filter_map(|pair| pair.file.to_str().map(|s| (s.to_string(), pair.tags)))
                    .collect();

                let files_refs: Vec<(&str, &[String])> = files_with_tags
                    .iter()
                    .map(|(f, tags)| (f.as_str(), tags.as_slice()))
                    .collect();

                // For ALL mode with hierarchical matching, we need files that match ALL patterns
                // For ANY mode, files that match ANY pattern
                let filtered_paths: Vec<PathBuf> = match expanded_params.tag_mode {
                    SearchMode::All => {
                        // File must have tags matching ALL include patterns
                        files_refs
                            .into_iter()
                            .filter(|(_, tags)| {
                                // Check if file has tags matching all patterns
                                expanded_params.tags.iter().all(|pattern| {
                                    tags.iter()
                                        .any(|tag| hierarchy::pattern_matches(pattern, tag))
                                })
                            })
                            .map(|(file, _)| PathBuf::from(file))
                            .collect()
                    }
                    SearchMode::Any => {
                        // File must have tags matching ANY include pattern
                        hierarchy::filter_by_hierarchy(
                            files_refs.into_iter(),
                            &expanded_params.tags,
                            &[], // Excludes handled separately
                        )
                        .into_iter()
                        .map(PathBuf::from)
                        .collect()
                    }
                };

                filtered_paths
            }
        }
    } else {
        db.list_all_files()?
    };

    if !expanded_params.file_patterns.is_empty() {
        let match_all = expanded_params.file_mode == SearchMode::All;
        files = files.into_iter().filter_patterns(
            &expanded_params.file_patterns,
            expanded_params.regex_file,
            match_all,
        )?;
    }

    if !expanded_params.exclude_tags.is_empty() {
        if params.no_hierarchy {
            // Traditional exclude logic (simple contains check)
            files = files.exclude_tags(db, &expanded_params.exclude_tags)?;
        } else {
            // Hierarchical exclude logic with specificity rules
            let mut filtered_files = Vec::new();
            for file in files {
                if let Some(file_tags) = db.get_tags(&file)? {
                    // Use hierarchical filtering - pass include patterns from original search
                    let should_include = hierarchy::should_include_file(
                        &file_tags,
                        &expanded_params.tags,
                        &expanded_params.exclude_tags,
                    );

                    if should_include {
                        filtered_files.push(file);
                    }
                } else {
                    // Files without tags pass through
                    filtered_files.push(file);
                }
            }
            files = filtered_files;
        }
    }

    if !expanded_params.virtual_tags.is_empty() {
        files = apply_virtual_tags(
            files,
            &expanded_params.virtual_tags,
            expanded_params.virtual_mode,
        )?;
    }

    Ok(files)
}

fn apply_virtual_tags(
    files: Vec<PathBuf>,
    virtual_tags: &[String],
    mode: SearchMode,
) -> Result<Vec<PathBuf>, DbError> {
    use rayon::prelude::*;

    let config = VirtualTagConfig::default();

    let parsed_tags: Vec<VirtualTag> = virtual_tags
        .iter()
        .map(|s| VirtualTag::parse_with_config(s, &config))
        .collect::<Result<_, _>>()
        .map_err(|e| DbError::InvalidInput(format!("Invalid virtual tag: {e}")))?;

    let cache_ttl = Duration::from_secs(config.cache_ttl_seconds);

    let filtered: Vec<PathBuf> = files
        .into_par_iter()
        .filter(|path| {
            let mut evaluator = VirtualTagEvaluator::new(cache_ttl, config.clone());
            match mode {
                SearchMode::All => parsed_tags
                    .iter()
                    .all(|vtag| evaluator.matches(path, vtag).unwrap_or(false)),
                SearchMode::Any => parsed_tags
                    .iter()
                    .any(|vtag| evaluator.matches(path, vtag).unwrap_or(false)),
            }
        })
        .collect();

    Ok(filtered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{TempFile, TestDb};
    use std::path::PathBuf;

    #[test]
    fn test_apply_search_params_compiles() {
        // This test just ensures the module compiles and the function signature is correct
        // Real testing happens in integration tests with actual database
        let _: fn(&Database, &SearchParams) -> Result<Vec<PathBuf>, DbError> = apply_search_params;
    }

    #[test]
    fn test_regex_tag_search_any_mode() {
        let test_db = TestDb::new("test_regex_tag_any");
        let db = test_db.db();

        // Create test files with tags
        let file1 = TempFile::create("file1.txt").unwrap();
        let file2 = TempFile::create("file2.txt").unwrap();
        let file3 = TempFile::create("file3.txt").unwrap();

        db.add_tags(file1.path(), vec!["markdown".into(), "note".into()])
            .unwrap();
        db.add_tags(file2.path(), vec!["rust".into(), "code".into()])
            .unwrap();
        db.add_tags(file3.path(), vec!["markdown".into(), "document".into()])
            .unwrap();

        // Test regex search for tags matching "mark.*" (should match "markdown")
        let params = SearchParams {
            query: None,
            tags: vec!["mark.*".to_string()],
            tag_mode: SearchMode::Any,
            file_patterns: vec![],
            file_mode: SearchMode::All,
            exclude_tags: vec![],
            regex_tag: true,
            regex_file: false,
            glob_files: false,
            virtual_tags: vec![],
            virtual_mode: SearchMode::All,
            no_hierarchy: false,
        };

        let results = apply_search_params(db, &params).unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.contains(&file1.path().to_path_buf()));
        assert!(results.contains(&file3.path().to_path_buf()));
        assert!(!results.contains(&file2.path().to_path_buf()));
    }

    #[test]
    fn test_regex_tag_search_all_mode() {
        let test_db = TestDb::new("test_regex_tag_all");
        let db = test_db.db();

        let file1 = TempFile::create("file1.txt").unwrap();
        let file2 = TempFile::create("file2.txt").unwrap();

        db.add_tags(file1.path(), vec!["markdown".into(), "note".into()])
            .unwrap();
        db.add_tags(file2.path(), vec!["markdown".into(), "document".into()])
            .unwrap();

        // Test regex search requiring both "mark.*" and ".*note" (should match only file1)
        let params = SearchParams {
            query: None,
            tags: vec!["mark.*".to_string(), ".*note".to_string()],
            tag_mode: SearchMode::All,
            file_patterns: vec![],
            file_mode: SearchMode::All,
            exclude_tags: vec![],
            regex_tag: true,
            regex_file: false,
            glob_files: false,
            virtual_tags: vec![],
            virtual_mode: SearchMode::All,
            no_hierarchy: false,
        };

        let results = apply_search_params(db, &params).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results.contains(&file1.path().to_path_buf()));
    }

    #[test]
    fn test_regex_tag_match_all() {
        let test_db = TestDb::new("test_regex_tag_match_all");
        let db = test_db.db();

        let file1 = TempFile::create("file1.txt").unwrap();
        let file2 = TempFile::create("file2.txt").unwrap();

        db.add_tags(file1.path(), vec!["tag1".into()]).unwrap();
        db.add_tags(file2.path(), vec!["tag2".into()]).unwrap();

        // Test ".*" pattern which should match all tags
        let params = SearchParams {
            query: None,
            tags: vec![".*".to_string()],
            tag_mode: SearchMode::Any,
            file_patterns: vec![],
            file_mode: SearchMode::All,
            exclude_tags: vec![],
            regex_tag: true,
            regex_file: false,
            glob_files: false,
            virtual_tags: vec![],
            virtual_mode: SearchMode::All,
            no_hierarchy: false,
        };

        let results = apply_search_params(db, &params).unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.contains(&file1.path().to_path_buf()));
        assert!(results.contains(&file2.path().to_path_buf()));
    }

    #[test]
    fn test_regex_file_pattern() {
        let test_db = TestDb::new("test_regex_file");
        let db = test_db.db();

        let file1 = TempFile::create("test.rs").unwrap();
        let file2 = TempFile::create("test.txt").unwrap();
        let file3 = TempFile::create("main.rs").unwrap();

        db.add_tags(file1.path(), vec!["rust".into()]).unwrap();
        db.add_tags(file2.path(), vec!["text".into()]).unwrap();
        db.add_tags(file3.path(), vec!["rust".into()]).unwrap();

        // Search for files matching ".*\.rs" pattern
        let params = SearchParams {
            query: None,
            tags: vec![],
            tag_mode: SearchMode::All,
            file_patterns: vec![".*\\.rs".to_string()],
            file_mode: SearchMode::All,
            exclude_tags: vec![],
            regex_tag: false,
            regex_file: true,
            glob_files: false,
            virtual_tags: vec![],
            virtual_mode: SearchMode::All,
            no_hierarchy: false,
        };

        let results = apply_search_params(db, &params).unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.contains(&file1.path().to_path_buf()));
        assert!(results.contains(&file3.path().to_path_buf()));
        assert!(!results.contains(&file2.path().to_path_buf()));
    }

    #[test]
    fn test_regex_tag_and_file_combined() {
        let test_db = TestDb::new("test_regex_combined");
        let db = test_db.db();

        let file1 = TempFile::create("test.rs").unwrap();
        let file2 = TempFile::create("test.txt").unwrap();

        db.add_tags(file1.path(), vec!["rust".into(), "code".into()])
            .unwrap();
        db.add_tags(file2.path(), vec!["rust".into(), "note".into()])
            .unwrap();

        // Search for rust files with regex patterns
        let params = SearchParams {
            query: None,
            tags: vec!["rust.*".to_string()],
            tag_mode: SearchMode::Any,
            file_patterns: vec![".*\\.rs".to_string()],
            file_mode: SearchMode::All,
            exclude_tags: vec![],
            regex_tag: true,
            regex_file: true,
            glob_files: false,
            virtual_tags: vec![],
            virtual_mode: SearchMode::All,
            no_hierarchy: false,
        };

        let results = apply_search_params(db, &params).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results.contains(&file1.path().to_path_buf()));
    }

    #[test]
    fn test_regex_tag_no_matches() {
        let test_db = TestDb::new("test_regex_no_match");
        let db = test_db.db();

        let file1 = TempFile::create("file1.txt").unwrap();
        db.add_tags(file1.path(), vec!["python".into(), "script".into()])
            .unwrap();

        // Search for regex that doesn't match any tags
        let params = SearchParams {
            query: None,
            tags: vec!["rust.*".to_string()],
            tag_mode: SearchMode::Any,
            file_patterns: vec![],
            file_mode: SearchMode::All,
            exclude_tags: vec![],
            regex_tag: true,
            regex_file: false,
            glob_files: false,
            virtual_tags: vec![],
            virtual_mode: SearchMode::All,
            no_hierarchy: false,
        };

        let results = apply_search_params(db, &params).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_regex_tag_multiple_patterns_any() {
        let test_db = TestDb::new("test_regex_multi_any");
        let db = test_db.db();

        let file1 = TempFile::create("file1.txt").unwrap();
        let file2 = TempFile::create("file2.txt").unwrap();
        let file3 = TempFile::create("file3.txt").unwrap();

        db.add_tags(file1.path(), vec!["python".into()]).unwrap();
        db.add_tags(file2.path(), vec!["rust".into()]).unwrap();
        db.add_tags(file3.path(), vec!["javascript".into()])
            .unwrap();

        // Match any file with tags starting with "py" or "ru"
        let params = SearchParams {
            query: None,
            tags: vec!["py.*".to_string(), "ru.*".to_string()],
            tag_mode: SearchMode::Any,
            file_patterns: vec![],
            file_mode: SearchMode::All,
            exclude_tags: vec![],
            regex_tag: true,
            regex_file: false,
            glob_files: false,
            virtual_tags: vec![],
            virtual_mode: SearchMode::All,
            no_hierarchy: false,
        };

        let results = apply_search_params(db, &params).unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.contains(&file1.path().to_path_buf()));
        assert!(results.contains(&file2.path().to_path_buf()));
        assert!(!results.contains(&file3.path().to_path_buf()));
    }
}
