//! Tests for core types — Tier 1 priority (newtype validation, `matches_pair`).

use super::error::NameKind;
use super::*;

// =============================================================================
// TagName validation
// =============================================================================

mod tag_name_validation {
    use super::*;

    #[test]
    fn valid_simple() {
        assert!(TagName::new("rust").is_ok());
        assert!(TagName::new("my-tag").is_ok());
        assert!(TagName::new("my_tag").is_ok());
        assert!(TagName::new("tag.v2").is_ok());
        assert!(TagName::new("Tag123").is_ok());
    }

    #[test]
    fn valid_hierarchical() {
        assert!(TagName::new("language:rust").is_ok());
        assert!(TagName::new("language:rust:async").is_ok());
        assert!(TagName::new("a:b:c:d:e").is_ok());
    }

    #[test]
    fn reject_empty() {
        assert_eq!(
            TagName::new("").unwrap_err(),
            ValidationError::Empty {
                kind: NameKind::Tag
            }
        );
    }

    #[test]
    fn reject_too_long() {
        let long = "a".repeat(MAX_TAG_NAME_LEN + 1);
        assert_eq!(
            TagName::new(long.clone()).unwrap_err(),
            ValidationError::TooLong {
                kind: NameKind::Tag,
                len: long.len(),
                max: MAX_TAG_NAME_LEN,
            }
        );
    }

    #[test]
    fn accept_max_length() {
        let max = "a".repeat(MAX_TAG_NAME_LEN);
        assert!(TagName::new(max).is_ok());
    }

    #[test]
    fn reject_leading_delimiter() {
        assert_eq!(
            TagName::new(":rust").unwrap_err(),
            ValidationError::LeadingOrTrailingDelimiter
        );
    }

    #[test]
    fn reject_trailing_delimiter() {
        assert_eq!(
            TagName::new("rust:").unwrap_err(),
            ValidationError::LeadingOrTrailingDelimiter
        );
    }

    #[test]
    fn reject_empty_segment() {
        assert_eq!(
            TagName::new("rust::cli").unwrap_err(),
            ValidationError::EmptySegment
        );
    }

    #[test]
    fn reject_whitespace() {
        let err = TagName::new("my tag").unwrap_err();
        assert!(matches!(err, ValidationError::InvalidChar { ch: ' ', .. }));
    }

    #[test]
    fn reject_special_chars() {
        for ch in [
            '/', '\\', '!', '@', '#', '$', '%', '^', '&', '*', '(', ')', '=', '+',
        ] {
            let name = format!("tag{ch}name");
            assert!(
                TagName::new(name.clone()).is_err(),
                "should reject '{ch}' in tag name"
            );
        }
    }

    #[test]
    fn reject_reserved_vtag_prefix() {
        assert_eq!(
            TagName::new("modified").unwrap_err(),
            ValidationError::ReservedVtagPrefix {
                prefix: "modified".to_string()
            }
        );
        assert_eq!(
            TagName::new("size").unwrap_err(),
            ValidationError::ReservedVtagPrefix {
                prefix: "size".to_string()
            }
        );
    }

    #[test]
    fn reject_reserved_prefix_hierarchical() {
        // "modified:today" should be rejected — "modified" is a vtag prefix
        assert_eq!(
            TagName::new("modified:today").unwrap_err(),
            ValidationError::ReservedVtagPrefix {
                prefix: "modified".to_string()
            }
        );
    }

    #[test]
    fn accept_reserved_word_not_at_root() {
        // "language:ext" is fine — "ext" is not the root segment
        assert!(TagName::new("language:ext").is_ok());
    }

    #[test]
    fn all_reserved_prefixes_rejected() {
        for prefix in RESERVED_VTAG_PREFIXES {
            assert!(
                TagName::new(*prefix).is_err(),
                "should reject reserved prefix '{prefix}'"
            );
        }
    }
}

// =============================================================================
// TagName hierarchy methods
// =============================================================================

mod tag_name_hierarchy {
    use super::*;

    #[test]
    fn depth() {
        assert_eq!(TagName::new("rust").unwrap().depth(), 1);
        assert_eq!(TagName::new("language:rust").unwrap().depth(), 2);
        assert_eq!(TagName::new("a:b:c:d").unwrap().depth(), 4);
    }

    #[test]
    fn root() {
        assert_eq!(TagName::new("rust").unwrap().root(), "rust");
        assert_eq!(TagName::new("language:rust").unwrap().root(), "language");
        assert_eq!(TagName::new("a:b:c").unwrap().root(), "a");
    }

    #[test]
    fn parent() {
        assert_eq!(TagName::new("rust").unwrap().parent(), None);
        assert_eq!(
            TagName::new("language:rust").unwrap().parent(),
            Some(TagName::new("language").unwrap())
        );
        assert_eq!(
            TagName::new("a:b:c").unwrap().parent(),
            Some(TagName::new("a:b").unwrap())
        );
    }

    #[test]
    fn segments() {
        let tag = TagName::new("a:b:c").unwrap();
        let segs: Vec<&str> = tag.segments().collect();
        assert_eq!(segs, vec!["a", "b", "c"]);
    }

    #[test]
    fn join() {
        let parent = TagName::new("language").unwrap();
        let child = parent.join("rust").unwrap();
        assert_eq!(child.as_str(), "language:rust");
    }

    #[test]
    fn join_validates_result() {
        let parent = TagName::new("language").unwrap();
        // Joining with an empty string creates "language:" — trailing delimiter
        assert!(parent.join("").is_err());
    }

    #[test]
    fn is_child_of() {
        let parent = TagName::new("language").unwrap();
        let child = TagName::new("language:rust").unwrap();
        let grandchild = TagName::new("language:rust:async").unwrap();
        let unrelated = TagName::new("tool:cargo").unwrap();

        assert!(child.is_child_of(&parent));
        assert!(grandchild.is_child_of(&parent));
        assert!(grandchild.is_child_of(&child));
        assert!(!parent.is_child_of(&parent)); // not child of itself
        assert!(!unrelated.is_child_of(&parent));
    }

    #[test]
    fn matches_pattern() {
        let tag = TagName::new("language:rust").unwrap();
        let pattern_exact = TagName::new("language:rust").unwrap();
        let pattern_parent = TagName::new("language").unwrap();
        let pattern_unrelated = TagName::new("tool").unwrap();

        assert!(tag.matches_pattern(&pattern_exact));
        assert!(tag.matches_pattern(&pattern_parent));
        assert!(!tag.matches_pattern(&pattern_unrelated));
    }

    #[test]
    fn matches_pattern_no_partial_segment() {
        // "lang" should NOT match "language:rust" — not a segment boundary
        let tag = TagName::new("language:rust").unwrap();
        let pattern = TagName::new("lang").unwrap();
        assert!(!tag.matches_pattern(&pattern));
    }

    #[test]
    fn ord_parents_before_children() {
        // ':' (0x3A) sorts before lowercase letters, so parents sort first
        let parent = TagName::new("language").unwrap();
        let child = TagName::new("language:rust").unwrap();
        assert!(parent < child);
    }
}

// =============================================================================
// TagName glob matching
// =============================================================================

mod tag_name_glob {
    use super::*;

    #[test]
    fn star_matches_one_segment() {
        let tag = TagName::new("language:rust:arrays").unwrap();
        assert!(tag.matches_glob("language:*:arrays"));
        assert!(!tag.matches_glob("*:arrays")); // * = one segment, but "language:rust" is two
    }

    #[test]
    fn double_star_matches_multiple() {
        let tag = TagName::new("language:rust").unwrap();
        assert!(tag.matches_glob("**:rust"));

        let deep = TagName::new("a:b:c:rust").unwrap();
        assert!(deep.matches_glob("**:rust"));
    }

    #[test]
    fn exact_match() {
        let tag = TagName::new("language:rust").unwrap();
        assert!(tag.matches_glob("language:rust"));
    }

    #[test]
    fn no_match() {
        let tag = TagName::new("language:rust").unwrap();
        assert!(!tag.matches_glob("tool:cargo"));
    }

    #[test]
    fn star_at_end() {
        let tag = TagName::new("language:rust").unwrap();
        assert!(tag.matches_glob("language:*"));
    }

    #[test]
    fn double_star_at_start() {
        let tag = TagName::new("a:b:c").unwrap();
        assert!(tag.matches_glob("**:c"));
        assert!(tag.matches_glob("**:b:c"));
    }
}

// =============================================================================
// TagrPath
// =============================================================================

mod tagr_path_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn valid_path() {
        let path = TagrPath::new("src/main.rs").unwrap();
        assert_eq!(path.as_str(), "src/main.rs");
        assert_eq!(path.as_path(), Path::new("src/main.rs"));
    }

    #[test]
    fn from_string() {
        let path = TagrPath::from_string("hello.txt".to_string());
        assert_eq!(path.as_str(), "hello.txt");
    }

    #[test]
    fn display() {
        let path = TagrPath::new("/home/user/file.txt").unwrap();
        assert_eq!(format!("{path}"), "/home/user/file.txt");
    }

    #[test]
    fn into_path_buf() {
        let path = TagrPath::new("src/main.rs").unwrap();
        let pb = path.into_path_buf();
        assert_eq!(pb, std::path::PathBuf::from("src/main.rs"));
    }
}

// =============================================================================
// FilterName
// =============================================================================

mod filter_name_tests {
    use super::*;

    #[test]
    fn valid() {
        assert!(FilterName::new("my-filter").is_ok());
        assert!(FilterName::new("rust_projects").is_ok());
        assert!(FilterName::new("filter123").is_ok());
    }

    #[test]
    fn reject_empty() {
        assert_eq!(
            FilterName::new("").unwrap_err(),
            ValidationError::Empty {
                kind: NameKind::Filter,
            }
        );
    }

    #[test]
    fn reject_too_long() {
        let long = "a".repeat(MAX_FILTER_NAME_LEN + 1);
        assert!(matches!(
            FilterName::new(long).unwrap_err(),
            ValidationError::TooLong { .. }
        ));
    }

    #[test]
    fn reject_special_chars() {
        assert!(FilterName::new("my filter").is_err()); // space
        assert!(FilterName::new("my:filter").is_err()); // colon
        assert!(FilterName::new("my.filter").is_err()); // dot
        assert!(FilterName::new("my/filter").is_err()); // slash
    }
}

// =============================================================================
// MatchMode
// =============================================================================

mod match_mode_tests {
    use super::*;

    #[test]
    fn default_is_all() {
        assert_eq!(MatchMode::default(), MatchMode::All);
    }

    #[test]
    fn display() {
        assert_eq!(format!("{}", MatchMode::All), "all");
        assert_eq!(format!("{}", MatchMode::Any), "any");
    }
}

// =============================================================================
// TagExpr
// =============================================================================

mod tag_expr_tests {
    use super::*;

    fn tag(name: &str) -> TagName {
        TagName::new(name).unwrap()
    }

    fn tags(names: &[&str]) -> Vec<TagName> {
        names.iter().map(|n| tag(n)).collect()
    }

    #[test]
    fn single_tag_matches() {
        let expr = TagExpr::Tag(tag("rust"));
        assert!(expr.matches(&tags(&["rust", "cli"])));
        assert!(!expr.matches(&tags(&["python"])));
    }

    #[test]
    fn single_tag_hierarchy_match() {
        // Tag("language") matches a file tagged "language:rust"
        let expr = TagExpr::Tag(tag("language"));
        assert!(expr.matches(&tags(&["language:rust"])));
    }

    #[test]
    fn not_expr() {
        let expr = TagExpr::Not(Box::new(TagExpr::Tag(tag("deprecated"))));
        assert!(expr.matches(&tags(&["rust"])));
        assert!(!expr.matches(&tags(&["rust", "deprecated"])));
    }

    #[test]
    fn and_expr() {
        let expr = TagExpr::And(vec![TagExpr::Tag(tag("rust")), TagExpr::Tag(tag("cli"))]);
        assert!(expr.matches(&tags(&["rust", "cli", "tool"])));
        assert!(!expr.matches(&tags(&["rust"])));
    }

    #[test]
    fn or_expr() {
        let expr = TagExpr::Or(vec![TagExpr::Tag(tag("rust")), TagExpr::Tag(tag("python"))]);
        assert!(expr.matches(&tags(&["rust"])));
        assert!(expr.matches(&tags(&["python"])));
        assert!(!expr.matches(&tags(&["go"])));
    }

    #[test]
    fn complex_nested() {
        // (rust & (cli | web) & !deprecated)
        let expr = TagExpr::And(vec![
            TagExpr::Tag(tag("rust")),
            TagExpr::Or(vec![TagExpr::Tag(tag("cli")), TagExpr::Tag(tag("web"))]),
            TagExpr::Not(Box::new(TagExpr::Tag(tag("deprecated")))),
        ]);

        assert!(expr.matches(&tags(&["rust", "cli"])));
        assert!(expr.matches(&tags(&["rust", "web"])));
        assert!(!expr.matches(&tags(&["rust"]))); // missing cli or web
        assert!(!expr.matches(&tags(&["rust", "cli", "deprecated"]))); // excluded
    }
}

// =============================================================================
// QueryCriteria
// =============================================================================

mod query_criteria_tests {
    use super::*;

    fn tag(name: &str) -> TagName {
        TagName::new(name).unwrap()
    }

    #[test]
    fn empty_criteria() {
        let c = QueryCriteria::default();
        assert!(c.is_empty());
    }

    #[test]
    fn toggle_include_from_empty() {
        let mut c = QueryCriteria::default();
        assert!(c.toggle_include_tag(tag("rust")));
        assert_eq!(c.tag_expr, Some(TagExpr::Tag(tag("rust"))));
    }

    #[test]
    fn toggle_include_adds_second_tag() {
        let mut c = QueryCriteria::default();
        c.toggle_include_tag(tag("rust"));
        c.toggle_include_tag(tag("cli"));

        assert!(matches!(c.tag_expr, Some(TagExpr::And(ref v)) if v.len() == 2));
    }

    #[test]
    fn toggle_include_removes_existing() {
        let mut c = QueryCriteria::default();
        c.toggle_include_tag(tag("rust"));
        let removed = !c.toggle_include_tag(tag("rust"));
        assert!(removed);
        assert!(c.tag_expr.is_none());
    }

    #[test]
    fn toggle_include_in_and_list() {
        let mut c = QueryCriteria::default();
        c.toggle_include_tag(tag("rust"));
        c.toggle_include_tag(tag("cli"));
        c.toggle_include_tag(tag("web"));

        // Remove middle tag
        assert!(!c.toggle_include_tag(tag("cli")));
        let includes = c.flat_include_tags().unwrap();
        assert!(includes.contains(&tag("rust")));
        assert!(includes.contains(&tag("web")));
        assert!(!includes.contains(&tag("cli")));
    }

    #[test]
    fn toggle_exclude_from_empty() {
        let mut c = QueryCriteria::default();
        assert!(c.toggle_exclude_tag(&tag("deprecated")));
        assert!(matches!(
            c.tag_expr,
            Some(TagExpr::Not(ref inner)) if matches!(inner.as_ref(), TagExpr::Tag(t) if *t == tag("deprecated"))
        ));
    }

    #[test]
    fn toggle_exclude_removes_existing() {
        let mut c = QueryCriteria::default();
        c.toggle_exclude_tag(&tag("deprecated"));
        assert!(!c.toggle_exclude_tag(&tag("deprecated")));
        assert!(c.tag_expr.is_none());
    }

    #[test]
    fn mixed_include_exclude() {
        let mut c = QueryCriteria::default();
        c.toggle_include_tag(tag("rust"));
        c.toggle_exclude_tag(&tag("deprecated"));

        let includes = c.flat_include_tags().unwrap();
        let excludes = c.flat_exclude_tags().unwrap();

        assert!(includes.contains(&tag("rust")));
        assert!(excludes.contains(&tag("deprecated")));
    }

    #[test]
    fn toggle_tag_mode() {
        let mut c = QueryCriteria::default();
        c.toggle_include_tag(tag("rust"));
        c.toggle_include_tag(tag("cli"));

        // Should be And by default
        assert!(matches!(c.tag_expr, Some(TagExpr::And(_))));

        c.toggle_tag_mode();
        assert!(matches!(c.tag_expr, Some(TagExpr::Or(_))));

        c.toggle_tag_mode();
        assert!(matches!(c.tag_expr, Some(TagExpr::And(_))));
    }

    #[test]
    fn flat_include_tags_complex_returns_none() {
        // Nested And/Or — too complex to flatten
        let c = QueryCriteria {
            tag_expr: Some(TagExpr::And(vec![
                TagExpr::Tag(tag("rust")),
                TagExpr::Or(vec![TagExpr::Tag(tag("cli")), TagExpr::Tag(tag("web"))]),
            ])),
            ..Default::default()
        };
        assert!(c.flat_include_tags().is_none());
    }
}

// =============================================================================
// QueryCriteria::matches_pair
// =============================================================================

mod matches_pair_tests {
    use super::*;

    fn tag(name: &str) -> TagName {
        TagName::new(name).unwrap()
    }

    fn make_pair(file: &str, tags: &[&str]) -> Pair {
        Pair::new(
            TagrPath::new(file).unwrap(),
            tags.iter().map(|t| tag(t)).collect(),
        )
    }

    #[test]
    fn empty_criteria_matches_all() {
        let c = QueryCriteria::default();
        assert!(c.matches_pair(&make_pair("file.rs", &["rust"])));
        assert!(c.matches_pair(&make_pair("file.py", &[])));
    }

    #[test]
    fn tag_inclusion() {
        let mut c = QueryCriteria::default();
        c.toggle_include_tag(tag("rust"));

        assert!(c.matches_pair(&make_pair("file.rs", &["rust", "cli"])));
        assert!(!c.matches_pair(&make_pair("file.py", &["python"])));
    }

    #[test]
    fn tag_exclusion() {
        let mut c = QueryCriteria::default();
        c.toggle_exclude_tag(&tag("deprecated"));

        assert!(c.matches_pair(&make_pair("file.rs", &["rust"])));
        assert!(!c.matches_pair(&make_pair("old.rs", &["rust", "deprecated"])));
    }

    #[test]
    fn hierarchy_match_in_pair() {
        // Tag("language") should match files tagged "language:rust"
        let mut c = QueryCriteria::default();
        c.toggle_include_tag(tag("language"));

        assert!(c.matches_pair(&make_pair("file.rs", &["language:rust"])));
        assert!(!c.matches_pair(&make_pair("file.rs", &["tool:cargo"])));
    }

    #[test]
    fn file_pattern_glob_all() {
        let c = QueryCriteria {
            file_patterns: vec!["*.rs".to_string()],
            file_mode: MatchMode::All,
            ..Default::default()
        };
        assert!(c.matches_pair(&make_pair("main.rs", &["rust"])));
        assert!(!c.matches_pair(&make_pair("main.py", &["python"])));
    }

    #[test]
    fn file_pattern_glob_any() {
        let c = QueryCriteria {
            file_patterns: vec!["*.rs".to_string(), "*.toml".to_string()],
            file_mode: MatchMode::Any,
            ..Default::default()
        };
        assert!(c.matches_pair(&make_pair("main.rs", &["rust"])));
        assert!(c.matches_pair(&make_pair("Cargo.toml", &["config"])));
        assert!(!c.matches_pair(&make_pair("main.py", &["python"])));
    }

    #[test]
    fn combined_tags_and_patterns() {
        let mut c = QueryCriteria::default();
        c.toggle_include_tag(tag("rust"));
        c.file_patterns = vec!["*.rs".to_string()];

        assert!(c.matches_pair(&make_pair("main.rs", &["rust"])));
        assert!(!c.matches_pair(&make_pair("main.rs", &["python"]))); // wrong tag
        assert!(!c.matches_pair(&make_pair("main.py", &["rust"]))); // wrong pattern
    }
}

// =============================================================================
// Pair
// =============================================================================

mod pair_tests {
    use super::*;

    #[test]
    fn construction() {
        let pair = Pair::new(
            TagrPath::new("file.rs").unwrap(),
            vec![TagName::new("rust").unwrap()],
        );
        assert_eq!(pair.file.as_str(), "file.rs");
        assert_eq!(pair.tags.len(), 1);
        assert_eq!(pair.tags[0].as_str(), "rust");
    }
}

// =============================================================================
// Serialization round-trips
// =============================================================================

mod serde_tests {
    use super::*;

    #[test]
    fn tag_name_json_roundtrip() {
        let original = TagName::new("language:rust").unwrap();
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: TagName = serde_json::from_str(&json).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn tagr_path_json_roundtrip() {
        let original = TagrPath::new("/home/user/file.rs").unwrap();
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: TagrPath = serde_json::from_str(&json).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn query_criteria_json_roundtrip() {
        let mut criteria = QueryCriteria::default();
        criteria.toggle_include_tag(TagName::new("rust").unwrap());
        criteria.toggle_exclude_tag(&TagName::new("deprecated").unwrap());
        criteria.file_patterns = vec!["*.rs".to_string()];
        criteria.virtual_tags = vec!["modified:today".to_string()];

        let json = serde_json::to_string(&criteria).unwrap();
        let deserialized: QueryCriteria = serde_json::from_str(&json).unwrap();
        assert_eq!(criteria, deserialized);
    }

    #[test]
    fn pair_json_roundtrip() {
        let pair = Pair::new(
            TagrPath::new("file.rs").unwrap(),
            vec![
                TagName::new("rust").unwrap(),
                TagName::new("language:rust").unwrap(),
            ],
        );
        let json = serde_json::to_string(&pair).unwrap();
        let deserialized: Pair = serde_json::from_str(&json).unwrap();
        assert_eq!(pair, deserialized);
    }
}

// =============================================================================
// is_narrower_than — widest-result cache subset detection
// =============================================================================

mod is_narrower_than {
    use super::*;

    fn tag(name: &str) -> TagName {
        TagName::new(name).unwrap()
    }

    fn criteria_with_tags(include: &[&str], exclude: &[&str]) -> QueryCriteria {
        let mut c = QueryCriteria::default();
        for t in include {
            c.toggle_include_tag(tag(t));
        }
        for t in exclude {
            c.toggle_exclude_tag(&tag(t));
        }
        c
    }

    #[test]
    fn empty_is_not_narrower_than_tags() {
        let empty = QueryCriteria::default();
        let with_tags = criteria_with_tags(&["rust"], &[]);
        assert!(!empty.is_narrower_than(&with_tags));
    }

    #[test]
    fn more_include_tags_is_narrower() {
        let narrow = criteria_with_tags(&["rust", "cli"], &[]);
        let wide = criteria_with_tags(&["rust"], &[]);
        assert!(narrow.is_narrower_than(&wide));
    }

    #[test]
    fn fewer_include_tags_is_not_narrower() {
        let wide = criteria_with_tags(&["rust"], &[]);
        let narrow = criteria_with_tags(&["rust", "cli"], &[]);
        assert!(!wide.is_narrower_than(&narrow));
    }

    #[test]
    fn same_tags_is_narrower() {
        let a = criteria_with_tags(&["rust"], &[]);
        let b = criteria_with_tags(&["rust"], &[]);
        assert!(a.is_narrower_than(&b));
    }

    #[test]
    fn more_exclude_tags_is_narrower() {
        let narrow = criteria_with_tags(&["rust"], &["deprecated", "old"]);
        let wide = criteria_with_tags(&["rust"], &["deprecated"]);
        assert!(narrow.is_narrower_than(&wide));
    }

    #[test]
    fn different_file_patterns_not_narrower() {
        let mut a = criteria_with_tags(&["rust"], &[]);
        a.file_patterns = vec!["*.rs".to_string()];
        let b = criteria_with_tags(&["rust"], &[]);
        assert!(!a.is_narrower_than(&b));
    }

    #[test]
    fn different_query_not_narrower() {
        let mut a = criteria_with_tags(&["rust"], &[]);
        a.query = Some("test".to_string());
        let b = criteria_with_tags(&["rust"], &[]);
        assert!(!a.is_narrower_than(&b));
    }

    #[test]
    fn complex_nested_expr_not_narrower() {
        let mut a = QueryCriteria::default();
        // Nested And inside And — not flat
        a.tag_expr = Some(TagExpr::And(vec![
            TagExpr::And(vec![TagExpr::Tag(tag("rust"))]),
            TagExpr::Tag(tag("cli")),
        ]));
        let b = criteria_with_tags(&["rust"], &[]);
        assert!(!a.is_narrower_than(&b));
    }

    #[test]
    fn empty_criteria_narrower_than_empty() {
        let a = QueryCriteria::default();
        let b = QueryCriteria::default();
        assert!(a.is_narrower_than(&b));
    }

    #[test]
    fn single_tag_narrower_than_empty() {
        let narrow = criteria_with_tags(&["rust"], &[]);
        let wide = QueryCriteria::default();
        assert!(narrow.is_narrower_than(&wide));
    }
}

// =============================================================================
// QueryCache::apply_event — unit tests for cache event handling
// =============================================================================

mod query_cache_apply_event {
    use crate::ipc::wire::ServerEvent;
    use crate::types::{NoteRecord, Pair, QueryCriteria, TagName, TagrPath};
    use std::collections::HashMap;

    // Mirror of the private QueryCache to test apply_event logic in isolation
    struct QueryCache {
        widest_criteria: QueryCriteria,
        widest_data: Vec<Pair>,
        notes: HashMap<TagrPath, NoteRecord>,
    }

    impl QueryCache {
        fn apply_event(&mut self, event: ServerEvent) {
            match event {
                ServerEvent::FileTagged { file, tags } => {
                    let Ok(path) = TagrPath::new(&file) else {
                        return;
                    };
                    let tag_names: Vec<TagName> =
                        tags.iter().filter_map(|t| TagName::new(t).ok()).collect();
                    if let Some(pair) = self.widest_data.iter_mut().find(|p| p.file == path) {
                        pair.tags = tag_names;
                    } else {
                        self.widest_data.push(Pair::new(path, tag_names));
                    }
                }
                ServerEvent::FileUntagged { file, tags } => {
                    let Ok(path) = TagrPath::new(&file) else {
                        return;
                    };
                    let removed: std::collections::HashSet<&str> =
                        tags.iter().map(String::as_str).collect();
                    if let Some(pair) = self.widest_data.iter_mut().find(|p| p.file == path) {
                        pair.tags.retain(|t| !removed.contains(t.as_str()));
                    }
                }
                ServerEvent::FileRemoved { file } => {
                    let Ok(path) = TagrPath::new(&file) else {
                        return;
                    };
                    self.widest_data.retain(|p| p.file != path);
                    self.notes.remove(&path);
                }
                ServerEvent::NoteChanged { file, content } => {
                    let Ok(path) = TagrPath::new(&file) else {
                        return;
                    };
                    match content {
                        Some(c) => {
                            self.notes.insert(path, NoteRecord::new(c));
                        }
                        None => {
                            self.notes.remove(&path);
                        }
                    }
                }
                ServerEvent::ConfigReloaded => {
                    self.widest_data.clear();
                    self.notes.clear();
                    self.widest_criteria = QueryCriteria::default();
                }
            }
        }
    }

    fn make_cache() -> QueryCache {
        let pair = Pair::new(
            TagrPath::new("src/main.rs").unwrap(),
            vec![TagName::new("rust").unwrap(), TagName::new("cli").unwrap()],
        );
        let mut notes = HashMap::new();
        notes.insert(
            TagrPath::new("src/main.rs").unwrap(),
            NoteRecord::new("entry point".to_string()),
        );
        QueryCache {
            widest_criteria: QueryCriteria::default(),
            widest_data: vec![pair],
            notes,
        }
    }

    #[test]
    fn file_tagged_updates_existing() {
        let mut cache = make_cache();
        cache.apply_event(ServerEvent::FileTagged {
            file: "src/main.rs".to_string(),
            tags: vec!["rust".to_string(), "cli".to_string(), "new-tag".to_string()],
        });
        let pair = cache
            .widest_data
            .iter()
            .find(|p| p.file.as_str() == "src/main.rs")
            .unwrap();
        assert_eq!(pair.tags.len(), 3);
    }

    #[test]
    fn file_tagged_adds_new() {
        let mut cache = make_cache();
        cache.apply_event(ServerEvent::FileTagged {
            file: "src/lib.rs".to_string(),
            tags: vec!["rust".to_string()],
        });
        assert_eq!(cache.widest_data.len(), 2);
    }

    #[test]
    fn file_untagged_removes_tags() {
        let mut cache = make_cache();
        cache.apply_event(ServerEvent::FileUntagged {
            file: "src/main.rs".to_string(),
            tags: vec!["cli".to_string()],
        });
        let pair = cache
            .widest_data
            .iter()
            .find(|p| p.file.as_str() == "src/main.rs")
            .unwrap();
        assert_eq!(pair.tags.len(), 1);
        assert_eq!(pair.tags[0].as_str(), "rust");
    }

    #[test]
    fn file_removed_removes_pair_and_note() {
        let mut cache = make_cache();
        cache.apply_event(ServerEvent::FileRemoved {
            file: "src/main.rs".to_string(),
        });
        assert!(cache.widest_data.is_empty());
        assert!(cache.notes.is_empty());
    }

    #[test]
    fn note_changed_upsert() {
        let mut cache = make_cache();
        cache.apply_event(ServerEvent::NoteChanged {
            file: "src/main.rs".to_string(),
            content: Some("updated note".to_string()),
        });
        let path = TagrPath::new("src/main.rs").unwrap();
        assert_eq!(cache.notes.get(&path).unwrap().content, "updated note");
    }

    #[test]
    fn note_changed_delete() {
        let mut cache = make_cache();
        cache.apply_event(ServerEvent::NoteChanged {
            file: "src/main.rs".to_string(),
            content: None,
        });
        let path = TagrPath::new("src/main.rs").unwrap();
        assert!(cache.notes.get(&path).is_none());
    }

    #[test]
    fn note_changed_insert_new() {
        let mut cache = make_cache();
        cache.apply_event(ServerEvent::NoteChanged {
            file: "src/lib.rs".to_string(),
            content: Some("new note".to_string()),
        });
        let path = TagrPath::new("src/lib.rs").unwrap();
        assert_eq!(cache.notes.get(&path).unwrap().content, "new note");
    }

    #[test]
    fn config_reloaded_clears_everything() {
        let mut cache = make_cache();
        cache.apply_event(ServerEvent::ConfigReloaded);
        assert!(cache.widest_data.is_empty());
        assert!(cache.notes.is_empty());
        assert!(cache.widest_criteria.is_empty());
    }
}
