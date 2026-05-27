//! Tests for the query engine using `MockStore`.

use crate::query;
use crate::schema::types::TagSchema;
use crate::store::MockStore;
use crate::types::{MatchMode, QueryCriteria, TagExpr, TagName, TagrPath};

fn schema() -> TagSchema {
    TagSchema::default()
}

fn tag(s: &str) -> TagName {
    TagName::new(s).unwrap()
}

fn path(s: &str) -> TagrPath {
    TagrPath::from_string(s.to_string())
}

#[test]
fn empty_criteria_returns_all_files() {
    let store = MockStore::with_tags(&[
        ("file1.rs", &["rust"]),
        ("file2.py", &["python"]),
    ]);
    let criteria = QueryCriteria::default();
    let result = query::execute(&store, &criteria, &schema()).unwrap();
    assert_eq!(result.len(), 2);
}

#[test]
fn single_tag_finds_files() {
    let store = MockStore::with_tags(&[
        ("file1.rs", &["rust", "code"]),
        ("file2.py", &["python"]),
        ("file3.rs", &["rust"]),
    ]);
    let mut criteria = QueryCriteria::default();
    criteria.tag_expr = Some(TagExpr::Tag(tag("rust")));
    let result = query::execute(&store, &criteria, &schema()).unwrap();
    assert_eq!(result.len(), 2);
    assert!(result.contains(&path("file1.rs")));
    assert!(result.contains(&path("file3.rs")));
}

#[test]
fn and_tags_intersect() {
    let store = MockStore::with_tags(&[
        ("file1.rs", &["rust", "web"]),
        ("file2.rs", &["rust"]),
        ("file3.rs", &["web"]),
    ]);
    let mut criteria = QueryCriteria::default();
    criteria.tag_expr = Some(TagExpr::And(vec![
        TagExpr::Tag(tag("rust")),
        TagExpr::Tag(tag("web")),
    ]));
    let result = query::execute(&store, &criteria, &schema()).unwrap();
    assert_eq!(result.len(), 1);
    assert!(result.contains(&path("file1.rs")));
}

#[test]
fn or_tags_union() {
    let store = MockStore::with_tags(&[
        ("file1.rs", &["rust"]),
        ("file2.py", &["python"]),
        ("file3.go", &["go"]),
    ]);
    let mut criteria = QueryCriteria::default();
    criteria.tag_expr = Some(TagExpr::Or(vec![
        TagExpr::Tag(tag("rust")),
        TagExpr::Tag(tag("python")),
    ]));
    let result = query::execute(&store, &criteria, &schema()).unwrap();
    assert_eq!(result.len(), 2);
    assert!(result.contains(&path("file1.rs")));
    assert!(result.contains(&path("file2.py")));
}

#[test]
fn not_tag_excludes() {
    let store = MockStore::with_tags(&[
        ("file1.rs", &["rust", "tests"]),
        ("file2.rs", &["rust"]),
    ]);
    let mut criteria = QueryCriteria::default();
    criteria.tag_expr = Some(TagExpr::And(vec![
        TagExpr::Tag(tag("rust")),
        TagExpr::Not(Box::new(TagExpr::Tag(tag("tests")))),
    ]));
    let result = query::execute(&store, &criteria, &schema()).unwrap();
    assert_eq!(result.len(), 1);
    assert!(result.contains(&path("file2.rs")));
}

#[test]
fn file_pattern_glob_filters() {
    let store = MockStore::with_tags(&[
        ("src/main.rs", &["rust"]),
        ("src/lib.rs", &["rust"]),
        ("tests/test.rs", &["rust"]),
    ]);
    let mut criteria = QueryCriteria::default();
    criteria.tag_expr = Some(TagExpr::Tag(tag("rust")));
    criteria.file_patterns = vec!["src/*".to_string()];
    criteria.file_mode = MatchMode::Any;
    let result = query::execute(&store, &criteria, &schema()).unwrap();
    assert_eq!(result.len(), 2);
    assert!(result.contains(&path("src/main.rs")));
    assert!(result.contains(&path("src/lib.rs")));
}

#[test]
fn file_pattern_regex_filters() {
    let store = MockStore::with_tags(&[
        ("test123.rs", &["rust"]),
        ("main.rs", &["rust"]),
        ("test.txt", &["docs"]),
    ]);
    let mut criteria = QueryCriteria::default();
    criteria.tag_expr = Some(TagExpr::Tag(tag("rust")));
    criteria.file_patterns = vec![r"test\d+\.rs".to_string()];
    criteria.regex_files = true;
    let result = query::execute(&store, &criteria, &schema()).unwrap();
    assert_eq!(result.len(), 1);
    assert!(result.contains(&path("test123.rs")));
}

#[test]
fn hierarchy_prefix_matching() {
    let store = MockStore::with_tags(&[
        ("file1.rs", &["lang:rust"]),
        ("file2.py", &["lang:python"]),
        ("file3.md", &["docs"]),
    ]);
    let mut criteria = QueryCriteria::default();
    criteria.tag_expr = Some(TagExpr::Tag(tag("lang")));
    criteria.expand_hierarchy = true;
    let result = query::execute(&store, &criteria, &schema()).unwrap();
    assert_eq!(result.len(), 2);
    assert!(result.contains(&path("file1.rs")));
    assert!(result.contains(&path("file2.py")));
}

#[test]
fn hierarchy_exclude_specificity() {
    let store = MockStore::with_tags(&[
        ("file1.rs", &["lang:rust"]),
        ("file2.py", &["lang:python"]),
    ]);
    let mut criteria = QueryCriteria::default();
    criteria.tag_expr = Some(TagExpr::And(vec![
        TagExpr::Tag(tag("lang")),
        TagExpr::Not(Box::new(TagExpr::Tag(tag("lang:rust")))),
    ]));
    criteria.expand_hierarchy = true;
    let result = query::execute(&store, &criteria, &schema()).unwrap();
    assert_eq!(result.len(), 1);
    assert!(result.contains(&path("file2.py")));
}

#[test]
fn regex_tag_any_mode() {
    let store = MockStore::with_tags(&[
        ("file1.rs", &["markdown"]),
        ("file2.rs", &["rust"]),
        ("file3.rs", &["markdown", "docs"]),
    ]);
    // Use free-text query for regex matching (regex patterns can't go through TagName validation)
    let mut criteria = QueryCriteria::default();
    criteria.query = Some("mark".to_string());
    let result = query::execute(&store, &criteria, &schema()).unwrap();
    assert_eq!(result.len(), 2);
    assert!(result.contains(&path("file1.rs")));
    assert!(result.contains(&path("file3.rs")));
}

#[test]
fn combined_tags_and_file_patterns() {
    let store = MockStore::with_tags(&[
        ("src/main.rs", &["rust", "code"]),
        ("src/test.rs", &["rust", "tests"]),
        ("docs/readme.md", &["docs"]),
    ]);
    let mut criteria = QueryCriteria::default();
    criteria.tag_expr = Some(TagExpr::Tag(tag("rust")));
    criteria.file_patterns = vec!["src/*".to_string()];
    let result = query::execute(&store, &criteria, &schema()).unwrap();
    assert_eq!(result.len(), 2);
}

#[test]
fn no_matching_tag_returns_empty() {
    let store = MockStore::with_tags(&[
        ("file1.rs", &["python"]),
    ]);
    let mut criteria = QueryCriteria::default();
    criteria.tag_expr = Some(TagExpr::Tag(tag("rust")));
    let result = query::execute(&store, &criteria, &schema()).unwrap();
    assert!(result.is_empty());
}
