//! Logic for matching files against watch rules and filters.

use crate::db::Database;
use crate::filters::{FilterCriteria, TagMode};
use crate::vtags::config::VirtualTagConfig;
use crate::vtags::evaluator::VirtualTagEvaluator;
use crate::vtags::types::VirtualTag;
use std::path::Path;
use std::time::Duration;

/// Check if a file matches a list of glob patterns.
pub fn matches_patterns<P: AsRef<Path>>(path: P, patterns: &[String]) -> bool {
    let path_str = match path.as_ref().to_str() {
        Some(s) => s,
        None => return false,
    };

    // Use globwalk or similar for robust matching, but here we can use simple glob matching
    // since we receive full paths from notify
    for pattern in patterns {
        // Expand tilde if present
        let pattern_expanded = if pattern.starts_with("~/") {
             if let Some(home) = dirs::home_dir() {
                 pattern.replacen("~", home.to_string_lossy().as_ref(), 1)
             } else {
                 pattern.clone()
             }
        } else {
            pattern.clone()
        };

        if let Ok(glob) = glob::Pattern::new(&pattern_expanded) {
            if glob.matches(path_str) {
                return true;
            }
        }
    }
    false
}

/// Evaluator for checking if a file matches a FilterCriteria.
pub struct FilterEvaluator {
    vtag_evaluator: VirtualTagEvaluator,
}

impl FilterEvaluator {
    /// Create a new evaluator
    pub fn new() -> Self {
        // TODO: Load config from actual settings if available
        let config = VirtualTagConfig::default();
        // Use a short cache TTL for watch mode as we want fresh metadata
        let vtag_evaluator = VirtualTagEvaluator::new(Duration::from_secs(1), config);
        
        Self { vtag_evaluator }
    }

    /// Check if a file matches the filter criteria.
    /// 
    /// # Arguments
    /// * `path` - Path to the file
    /// * `criteria` - The filter criteria to match against
    /// * `db` - Database access for checking existing tags
    pub fn matches(&mut self, path: &Path, criteria: &FilterCriteria, db: &Database) -> bool {
        // 1. Check existing tags if specified in filter
        if !criteria.tags.is_empty() {
            // We need to look up tags for this file from DB
            // This assumes the file is already in the DB if we are checking for tags.
            // If the file is new, it won't have tags, so this check will fail (correctly).
            
            // Note: DB lookup might be expensive if done frequently. 
            // In watch mode, we only do this when file system events occur for matched patterns.
            
            // Convert path to key format used in DB
            // We use a simplified check here. Real implementation needs robust error handling.
            if let Ok(Some(pair)) = db.get_pair(path) {
                let matches_tags = match criteria.tag_mode {
                    TagMode::All => criteria.tags.iter().all(|t| pair.tags.contains(t)),
                    TagMode::Any => criteria.tags.iter().any(|t| pair.tags.contains(t)),
                };
                
                if !matches_tags {
                    return false;
                }
            } else {
                // File not in DB or error -> treat as having no tags
                // If filter requires tags, then it fails.
                return false;
            }
        }

        // 2. Check excluded tags
        if !criteria.excludes.is_empty() {
            if let Ok(Some(pair)) = db.get_pair(path) {
                if criteria.excludes.iter().any(|t| pair.tags.contains(t)) {
                    return false;
                }
            }
        }

        // 3. Check virtual tags (metadata)
        if !criteria.virtual_tags.is_empty() {
            let mut matched_count = 0;
            
            for vtag_str in &criteria.virtual_tags {
                if let Ok(vtag) = VirtualTag::try_from(vtag_str.as_str()) {
                    if let Ok(matches) = self.vtag_evaluator.matches(path, &vtag) {
                        if matches {
                            matched_count += 1;
                        }
                    }
                }
            }

            let matches_vtags = match criteria.virtual_mode {
                TagMode::All => matched_count == criteria.virtual_tags.len(),
                TagMode::Any => matched_count > 0,
            };

            if !matches_vtags {
                return false;
            }
        }
        
        // 4. File patterns are checked by the glob watcher itself usually, 
        // but if the filter has ADDITIONAL file patterns (e.g. extension), check them here.
        if !criteria.file_patterns.is_empty() {
             let matches_files = match criteria.file_mode {
                 crate::filters::FileMode::All => matches_patterns(path, &criteria.file_patterns), // This is simplified, usually All means ALL patterns match (which is rare for globs)
                 crate::filters::FileMode::Any => matches_patterns(path, &criteria.file_patterns),
             };
             
             if !matches_files {
                 return false;
             }
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{TempFile, TestDb};

    #[test]
    fn test_matches_patterns_basic_glob() {
        let patterns = vec!["/tmp/*.txt".to_string()];
        assert!(matches_patterns("/tmp/file.txt", &patterns));
        assert!(!matches_patterns("/tmp/file.rs", &patterns));
    }

    #[test]
    fn test_matches_patterns_no_match() {
        let patterns = vec!["/home/user/*.md".to_string()];
        assert!(!matches_patterns("/tmp/file.md", &patterns));
    }

    #[test]
    fn test_matches_patterns_multiple_patterns() {
        let patterns = vec!["/tmp/*.txt".to_string(), "/tmp/*.md".to_string()];
        assert!(matches_patterns("/tmp/notes.txt", &patterns));
        assert!(matches_patterns("/tmp/readme.md", &patterns));
        assert!(!matches_patterns("/tmp/code.rs", &patterns));
    }

    #[test]
    fn test_matches_patterns_empty_patterns() {
        let patterns: Vec<String> = vec![];
        assert!(!matches_patterns("/tmp/file.txt", &patterns));
    }

    #[test]
    fn test_matches_patterns_recursive_glob() {
        let patterns = vec!["/tmp/**/*.rs".to_string()];
        assert!(matches_patterns("/tmp/src/main.rs", &patterns));
        assert!(matches_patterns("/tmp/a/b/c/lib.rs", &patterns));
        assert!(!matches_patterns("/tmp/file.txt", &patterns));
    }

    #[test]
    fn test_matches_patterns_invalid_glob_skipped() {
        // An unclosed bracket is an invalid glob — should not panic, just skip
        let patterns = vec!["[invalid".to_string()];
        assert!(!matches_patterns("/tmp/file.txt", &patterns));
    }

    #[test]
    fn test_matches_patterns_exact_path() {
        let patterns = vec!["/tmp/specific_file.txt".to_string()];
        assert!(matches_patterns("/tmp/specific_file.txt", &patterns));
        assert!(!matches_patterns("/tmp/other_file.txt", &patterns));
    }

    #[test]
    fn test_filter_evaluator_empty_criteria_matches_everything() {
        let test_db = TestDb::new("filter_eval_empty");
        let db = test_db.db();
        let temp = TempFile::create("test.txt").unwrap();

        let criteria = FilterCriteria::default();
        let mut evaluator = FilterEvaluator::new();

        assert!(evaluator.matches(temp.path(), &criteria, db));
    }

    #[test]
    fn test_filter_evaluator_tag_match_all() {
        let test_db = TestDb::new("filter_eval_tag_all");
        let db = test_db.db();
        let temp = TempFile::create("tagged.txt").unwrap();

        db.insert(temp.path(), vec!["rust".into(), "code".into()]).unwrap();

        let criteria = FilterCriteria {
            tags: vec!["rust".into(), "code".into()],
            tag_mode: TagMode::All,
            ..Default::default()
        };
        let mut evaluator = FilterEvaluator::new();

        assert!(evaluator.matches(temp.path(), &criteria, db));
    }

    #[test]
    fn test_filter_evaluator_tag_match_all_missing_one() {
        let test_db = TestDb::new("filter_eval_tag_missing");
        let db = test_db.db();
        let temp = TempFile::create("partial.txt").unwrap();

        db.insert(temp.path(), vec!["rust".into()]).unwrap();

        let criteria = FilterCriteria {
            tags: vec!["rust".into(), "code".into()],
            tag_mode: TagMode::All,
            ..Default::default()
        };
        let mut evaluator = FilterEvaluator::new();

        assert!(!evaluator.matches(temp.path(), &criteria, db));
    }

    #[test]
    fn test_filter_evaluator_tag_match_any() {
        let test_db = TestDb::new("filter_eval_tag_any");
        let db = test_db.db();
        let temp = TempFile::create("any.txt").unwrap();

        db.insert(temp.path(), vec!["rust".into()]).unwrap();

        let criteria = FilterCriteria {
            tags: vec!["rust".into(), "python".into()],
            tag_mode: TagMode::Any,
            ..Default::default()
        };
        let mut evaluator = FilterEvaluator::new();

        assert!(evaluator.matches(temp.path(), &criteria, db));
    }

    #[test]
    fn test_filter_evaluator_tag_required_but_file_not_in_db() {
        let test_db = TestDb::new("filter_eval_tag_no_file");
        let db = test_db.db();
        let temp = TempFile::create("unknown.txt").unwrap();

        let criteria = FilterCriteria {
            tags: vec!["rust".into()],
            ..Default::default()
        };
        let mut evaluator = FilterEvaluator::new();

        assert!(!evaluator.matches(temp.path(), &criteria, db));
    }

    #[test]
    fn test_filter_evaluator_excludes_tag() {
        let test_db = TestDb::new("filter_eval_excludes");
        let db = test_db.db();
        let temp = TempFile::create("excluded.txt").unwrap();

        db.insert(temp.path(), vec!["draft".into(), "docs".into()]).unwrap();

        let criteria = FilterCriteria {
            excludes: vec!["draft".into()],
            ..Default::default()
        };
        let mut evaluator = FilterEvaluator::new();

        assert!(!evaluator.matches(temp.path(), &criteria, db));
    }

    #[test]
    fn test_filter_evaluator_excludes_not_present() {
        let test_db = TestDb::new("filter_eval_excludes_ok");
        let db = test_db.db();
        let temp = TempFile::create("clean.txt").unwrap();

        db.insert(temp.path(), vec!["docs".into()]).unwrap();

        let criteria = FilterCriteria {
            excludes: vec!["draft".into()],
            ..Default::default()
        };
        let mut evaluator = FilterEvaluator::new();

        assert!(evaluator.matches(temp.path(), &criteria, db));
    }

    #[test]
    fn test_filter_evaluator_file_pattern() {
        let test_db = TestDb::new("filter_eval_file_pattern");
        let db = test_db.db();
        let temp = TempFile::create("readme.md").unwrap();

        let path_str = temp.path().parent().unwrap().to_string_lossy().to_string();
        let criteria = FilterCriteria {
            file_patterns: vec![format!("{path_str}/*.md")],
            ..Default::default()
        };
        let mut evaluator = FilterEvaluator::new();

        assert!(evaluator.matches(temp.path(), &criteria, db));
    }

    #[test]
    fn test_filter_evaluator_file_pattern_no_match() {
        let test_db = TestDb::new("filter_eval_file_no_match");
        let db = test_db.db();
        let temp = TempFile::create("code.rs").unwrap();

        let criteria = FilterCriteria {
            file_patterns: vec!["/nonexistent/path/*.md".to_string()],
            ..Default::default()
        };
        let mut evaluator = FilterEvaluator::new();

        assert!(!evaluator.matches(temp.path(), &criteria, db));
    }
}
