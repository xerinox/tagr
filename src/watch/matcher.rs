//! Logic for matching files against watch rules and filters.

use crate::store::TagStore;
use crate::types::{MatchMode, Pair, QueryCriteria, TagrPath};
use crate::vtags::config::VirtualTagConfig;
use crate::vtags::evaluator::VirtualTagEvaluator;
use crate::vtags::types::VirtualTag;
use std::path::Path;
use std::time::Duration;

/// Check if a file matches a list of glob patterns.
#[must_use]
pub fn matches_patterns<P: AsRef<Path>>(path: P, patterns: &[String]) -> bool {
    let Some(path_str) = path.as_ref().to_str() else {
        return false;
    };

    for pattern in patterns {
        let pattern_expanded = if pattern.starts_with("~/") {
            dirs::home_dir().map_or_else(
                || pattern.clone(),
                |home| pattern.replacen('~', home.to_string_lossy().as_ref(), 1),
            )
        } else {
            pattern.clone()
        };

        if let Ok(glob) = glob::Pattern::new(&pattern_expanded)
            && glob.matches(path_str)
        {
            return true;
        }
    }
    false
}

/// Evaluator for checking if a file matches a `QueryCriteria`.
pub struct FilterEvaluator {
    vtag_evaluator: VirtualTagEvaluator,
}

impl Default for FilterEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

impl FilterEvaluator {
    /// Create a new evaluator.
    #[must_use]
    pub fn new() -> Self {
        let config = VirtualTagConfig::default();
        // Short cache TTL for watch mode — we want fresh metadata
        let vtag_evaluator = VirtualTagEvaluator::new(Duration::from_secs(1), config);

        Self { vtag_evaluator }
    }

    /// Check if a file matches the query criteria.
    ///
    /// # Arguments
    /// * `path` - Path to the file
    /// * `criteria` - The query criteria to match against
    /// * `store` - Tag store for checking existing tags
    pub fn matches(&mut self, path: &Path, criteria: &QueryCriteria, store: &dyn TagStore) -> bool {
        // 1. Tag + file pattern matching via QueryCriteria::matches_pair()
        let has_tag_or_file_criteria =
            criteria.tag_expr.is_some() || !criteria.file_patterns.is_empty();
        if has_tag_or_file_criteria {
            let Ok(tagr_path) = TagrPath::new(path) else {
                return false;
            };

            let tags = match store.get_tags(&tagr_path) {
                Ok(Some(t)) => t,
                Ok(None) => {
                    // File not in DB — if tags are required, fail
                    if criteria.tag_expr.is_some() {
                        return false;
                    }
                    vec![]
                }
                Err(_) => return false,
            };

            let pair = Pair::new(tagr_path, tags);
            if !criteria.matches_pair(&pair) {
                return false;
            }
        }

        // 2. Virtual tags (metadata)
        if !criteria.virtual_tags.is_empty() {
            let mut matched_count = 0;

            for vtag_str in &criteria.virtual_tags {
                if let Ok(vtag) = VirtualTag::try_from(vtag_str.as_str())
                    && matches!(self.vtag_evaluator.matches(path, &vtag), Ok(true))
                {
                    matched_count += 1;
                }
            }

            let matches_vtags = match criteria.virtual_mode {
                MatchMode::All => matched_count == criteria.virtual_tags.len(),
                MatchMode::Any => matched_count > 0,
            };

            if !matches_vtags {
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
    use crate::types::{TagExpr, TagName};

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
        let store = test_db.store();
        let temp = TempFile::create("test.txt").unwrap();

        let criteria = QueryCriteria::default();
        let mut evaluator = FilterEvaluator::new();

        assert!(evaluator.matches(temp.path(), &criteria, store));
    }

    #[test]
    fn test_filter_evaluator_tag_match_all() {
        let test_db = TestDb::new("filter_eval_tag_all");
        let db = test_db.db();
        let store = test_db.store();
        let temp = TempFile::create("tagged.txt").unwrap();

        db.insert(temp.path(), vec!["rust".into(), "code".into()])
            .unwrap();

        let criteria = QueryCriteria {
            tag_expr: Some(TagExpr::And(vec![
                TagExpr::Tag(TagName::new("rust").unwrap()),
                TagExpr::Tag(TagName::new("code").unwrap()),
            ])),
            ..Default::default()
        };
        let mut evaluator = FilterEvaluator::new();

        assert!(evaluator.matches(temp.path(), &criteria, store));
    }

    #[test]
    fn test_filter_evaluator_tag_match_all_missing_one() {
        let test_db = TestDb::new("filter_eval_tag_missing");
        let db = test_db.db();
        let store = test_db.store();
        let temp = TempFile::create("partial.txt").unwrap();

        db.insert(temp.path(), vec!["rust".into()]).unwrap();

        let criteria = QueryCriteria {
            tag_expr: Some(TagExpr::And(vec![
                TagExpr::Tag(TagName::new("rust").unwrap()),
                TagExpr::Tag(TagName::new("code").unwrap()),
            ])),
            ..Default::default()
        };
        let mut evaluator = FilterEvaluator::new();

        assert!(!evaluator.matches(temp.path(), &criteria, store));
    }

    #[test]
    fn test_filter_evaluator_tag_match_any() {
        let test_db = TestDb::new("filter_eval_tag_any");
        let db = test_db.db();
        let store = test_db.store();
        let temp = TempFile::create("any.txt").unwrap();

        db.insert(temp.path(), vec!["rust".into()]).unwrap();

        let criteria = QueryCriteria {
            tag_expr: Some(TagExpr::Or(vec![
                TagExpr::Tag(TagName::new("rust").unwrap()),
                TagExpr::Tag(TagName::new("python").unwrap()),
            ])),
            ..Default::default()
        };
        let mut evaluator = FilterEvaluator::new();

        assert!(evaluator.matches(temp.path(), &criteria, store));
    }

    #[test]
    fn test_filter_evaluator_tag_required_but_file_not_in_db() {
        let test_db = TestDb::new("filter_eval_tag_no_file");
        let store = test_db.store();
        let temp = TempFile::create("unknown.txt").unwrap();

        let criteria = QueryCriteria {
            tag_expr: Some(TagExpr::Tag(TagName::new("rust").unwrap())),
            ..Default::default()
        };
        let mut evaluator = FilterEvaluator::new();

        assert!(!evaluator.matches(temp.path(), &criteria, store));
    }

    #[test]
    fn test_filter_evaluator_excludes_tag() {
        let test_db = TestDb::new("filter_eval_excludes");
        let db = test_db.db();
        let store = test_db.store();
        let temp = TempFile::create("excluded.txt").unwrap();

        db.insert(temp.path(), vec!["draft".into(), "docs".into()])
            .unwrap();

        let criteria = QueryCriteria {
            tag_expr: Some(TagExpr::Not(Box::new(TagExpr::Tag(
                TagName::new("draft").unwrap(),
            )))),
            ..Default::default()
        };
        let mut evaluator = FilterEvaluator::new();

        assert!(!evaluator.matches(temp.path(), &criteria, store));
    }

    #[test]
    fn test_filter_evaluator_excludes_not_present() {
        let test_db = TestDb::new("filter_eval_excludes_ok");
        let db = test_db.db();
        let store = test_db.store();
        let temp = TempFile::create("clean.txt").unwrap();

        db.insert(temp.path(), vec!["docs".into()]).unwrap();

        let criteria = QueryCriteria {
            tag_expr: Some(TagExpr::Not(Box::new(TagExpr::Tag(
                TagName::new("draft").unwrap(),
            )))),
            ..Default::default()
        };
        let mut evaluator = FilterEvaluator::new();

        assert!(evaluator.matches(temp.path(), &criteria, store));
    }

    #[test]
    fn test_filter_evaluator_file_pattern() {
        let test_db = TestDb::new("filter_eval_file_pattern");
        let store = test_db.store();
        let temp = TempFile::create("readme.md").unwrap();

        let path_str = temp.path().parent().unwrap().to_string_lossy().to_string();
        let criteria = QueryCriteria {
            file_patterns: vec![format!("{path_str}/*.md")],
            ..Default::default()
        };
        let mut evaluator = FilterEvaluator::new();

        assert!(evaluator.matches(temp.path(), &criteria, store));
    }

    #[test]
    fn test_filter_evaluator_file_pattern_no_match() {
        let test_db = TestDb::new("filter_eval_file_no_match");
        let store = test_db.store();
        let temp = TempFile::create("code.rs").unwrap();

        let criteria = QueryCriteria {
            file_patterns: vec!["/nonexistent/path/*.md".to_string()],
            ..Default::default()
        };
        let mut evaluator = FilterEvaluator::new();

        assert!(!evaluator.matches(temp.path(), &criteria, store));
    }
}
