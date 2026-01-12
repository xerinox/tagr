//! Integration tests for tagr CLI
//!
//! These tests verify end-to-end functionality by creating temporary databases
//! and testing the complete command workflows.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tagr::cli::{SearchMode, SearchParams};
use tagr::commands::bulk::{bulk_tag, bulk_untag};
use tagr::commands::search as search_cmd;
use tagr::config;
use tagr::{Pair, cli::execute_command_on_files, db::Database};

/// Test database wrapper that cleans up on drop
struct TestDb {
    db: Database,
    path: PathBuf,
}

impl TestDb {
    fn new(name: &str) -> Self {
        let path = PathBuf::from(format!("test_integration_{name}"));
        let db = Database::open(&path).unwrap();
        db.clear().unwrap();
        Self { db, path }
    }

    const fn db(&self) -> &Database {
        &self.db
    }
}

impl Drop for TestDb {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Test file wrapper that cleans up on drop
struct TestFile {
    path: PathBuf,
}

impl TestFile {
    fn create(path: &str, content: &str) -> std::io::Result<Self> {
        let mut file = fs::File::create(path)?;
        file.write_all(content.as_bytes())?;
        Ok(Self {
            path: PathBuf::from(path),
        })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

// ============================================================================
// E2E CLI-like Invocation Tests using handlers
// ============================================================================

#[test]
fn test_e2e_bulk_tag_with_glob_file_patterns() {
    let test_db = TestDb::new("e2e_bulk_tag_glob");

    let f_rs1 = TestFile::create("e2e1.rs", "content").unwrap();
    let f_rs2 = TestFile::create("e2e2.rs", "content").unwrap();

    let f_txt = TestFile::create("e2e3.txt", "content").unwrap();

    // Insert files with initial tags
    test_db
        .db()
        .insert(f_rs1.path(), vec!["init".into()])
        .unwrap();
    test_db
        .db()
        .insert(f_rs2.path(), vec!["init".into()])
        .unwrap();
    test_db
        .db()
        .insert(f_txt.path(), vec!["init".into()])
        .unwrap();

    // Build SearchParams like CLI: file_patterns only, no explicit glob flag
    let params = SearchParams {
        query: None,
        tags: vec![],
        tag_mode: SearchMode::All,
        file_patterns: vec!["*.rs".to_string()],
        file_mode: SearchMode::All,
        exclude_tags: vec![],
        regex_tag: false,
        regex_file: false,
        glob_files: false,
        virtual_tags: vec![],
        virtual_mode: SearchMode::All,
        no_hierarchy: false,
    };

    // Execute bulk tag (normalize should enable glob and match only .rs files)
    bulk_tag(
        test_db.db(),
        &params,
        &["added".into()],
        &tagr::cli::ConditionalArgs::default(),
        /*dry_run*/ false,
        /*yes*/ true,
        /*quiet*/ true,
    )
    .unwrap();

    let tags_rs1 = test_db.db().get_tags(f_rs1.path()).unwrap().unwrap();
    let tags_rs2 = test_db.db().get_tags(f_rs2.path()).unwrap().unwrap();
    let tags_txt = test_db.db().get_tags(f_txt.path()).unwrap().unwrap();

    assert!(tags_rs1.contains(&"added".into()));
    assert!(tags_rs2.contains(&"added".into()));
    assert!(!tags_txt.contains(&"added".into()));
}

#[test]
fn test_e2e_bulk_untag_with_regex_file_patterns() {
    let test_db = TestDb::new("e2e_bulk_untag_regex");

    let f_txt1 = TestFile::create("e2e_r1.txt", "content").unwrap();
    let f_txt2 = TestFile::create("e2e_r2.txt", "content").unwrap();
    let f_rs = TestFile::create("e2e_r3.rs", "content").unwrap();

    test_db
        .db()
        .insert(f_txt1.path(), vec!["remove".into(), "keep".into()])
        .unwrap();
    test_db
        .db()
        .insert(f_txt2.path(), vec!["remove".into()])
        .unwrap();
    test_db
        .db()
        .insert(f_rs.path(), vec!["remove".into()])
        .unwrap();

    let params = SearchParams {
        query: None,
        tags: vec![],
        tag_mode: SearchMode::All,
        file_patterns: vec![".*\\.txt".to_string()],
        file_mode: SearchMode::All,
        exclude_tags: vec![],
        regex_tag: false,
        regex_file: true,
        glob_files: false,
        virtual_tags: vec![],
        virtual_mode: SearchMode::All,
        no_hierarchy: false,
    };

    bulk_untag(
        test_db.db(),
        &params,
        &["remove".into()],
        /*remove_all*/ false,
        &tagr::cli::ConditionalArgs::default(),
        /*dry_run*/ false,
        /*yes*/ true,
        /*quiet*/ true,
    )
    .unwrap();

    // .txt files should have 'remove' tag removed, .rs should still have it
    let tags_txt1 = test_db.db().get_tags(f_txt1.path()).unwrap().unwrap();
    assert!(!tags_txt1.contains(&"remove".into()));
    assert!(tags_txt1.contains(&"keep".into()));

    let tags_txt2 = test_db.db().get_tags(f_txt2.path()).unwrap();
    assert!(tags_txt2.is_none(), "file removed when no tags remain");

    let tags_rs = test_db.db().get_tags(f_rs.path()).unwrap().unwrap();
    assert!(tags_rs.contains(&"remove".into()));
}

#[test]
fn test_e2e_search_execute_with_glob_flag() {
    let test_db = TestDb::new("e2e_search_glob");
    let db = test_db.db();
    let f_rs = TestFile::create("e2e_s1.rs", "content").unwrap();
    db.insert(f_rs.path(), vec!["t1".into()]).unwrap();

    let params = SearchParams {
        query: None,
        tags: vec![],
        tag_mode: SearchMode::All,
        file_patterns: vec!["*.rs".to_string()],
        file_mode: SearchMode::All,
        exclude_tags: vec![],
        regex_tag: false,
        regex_file: false,
        glob_files: true,
        virtual_tags: vec![],
        virtual_mode: SearchMode::All,
        no_hierarchy: false,
    };

    let res = search_cmd::execute(
        db,
        params,
        None,
        None,
        false, // has_explicit_tag_mode
        false, // has_explicit_file_mode
        false, // has_explicit_virtual_mode
        config::PathFormat::Absolute,
        /*quiet*/ true,
    );
    assert!(res.is_ok());
}

#[test]
fn test_tag_command_with_new_file() {
    let test_db = TestDb::new("tag_new");
    let _test_file = TestFile::create("test_tag_file.txt", "test content").unwrap();

    let result = test_db
        .db()
        .insert("test_tag_file.txt", vec!["rust".into(), "test".into()]);
    assert!(result.is_ok());

    let tags = test_db.db().get_tags("test_tag_file.txt").unwrap();
    assert_eq!(tags, Some(vec!["rust".into(), "test".into()]));
    // Cleanup happens automatically via Drop
}

#[test]
fn test_tag_command_add_tags() {
    let test_db = TestDb::new("tag_add");
    let _test_file = TestFile::create("test_add_tags.txt", "content").unwrap();

    test_db
        .db()
        .insert("test_add_tags.txt", vec!["tag1".into()])
        .unwrap();

    test_db
        .db()
        .add_tags("test_add_tags.txt", vec!["tag2".into(), "tag3".into()])
        .unwrap();

    let tags = test_db.db().get_tags("test_add_tags.txt").unwrap().unwrap();
    assert!(tags.contains(&"tag1".into()));
    assert!(tags.contains(&"tag2".into()));
    assert!(tags.contains(&"tag3".into()));
    // Cleanup happens automatically via Drop
}

#[test]
fn test_search_command_single_tag() {
    let test_db = TestDb::new("search_single");

    let file1 = TestFile::create("file1.txt", "content1").unwrap();
    let file2 = TestFile::create("file2.txt", "content2").unwrap();
    let file3 = TestFile::create("file3.txt", "content3").unwrap();

    let file1_path = fs::canonicalize(file1.path()).unwrap();
    let file2_path = fs::canonicalize(file2.path()).unwrap();
    let file3_path = fs::canonicalize(file3.path()).unwrap();

    test_db
        .db()
        .insert(&file1_path, vec!["rust".into()])
        .unwrap();
    test_db
        .db()
        .insert(&file2_path, vec!["rust".into(), "programming".into()])
        .unwrap();
    test_db
        .db()
        .insert(&file3_path, vec!["python".into()])
        .unwrap();

    let files = test_db.db().find_by_tag("rust").unwrap();
    assert_eq!(files.len(), 2);
    assert!(files.contains(&file1_path));
    assert!(files.contains(&file2_path));
    // Cleanup happens automatically via Drop
}

#[test]
fn test_search_command_nonexistent_tag() {
    let test_db = TestDb::new("search_nonexistent");

    let _test_file = TestFile::create("file1.txt", "content").unwrap();
    test_db
        .db()
        .insert("file1.txt", vec!["exists".into()])
        .unwrap();

    let files = test_db.db().find_by_tag("nonexistent").unwrap();
    assert!(files.is_empty());

    let _ = fs::remove_file("file1.txt");
    // Cleanup happens automatically via Drop
}

#[test]
fn test_untag_command_specific_tags() {
    let test_db = TestDb::new("untag_specific");

    let _test_file = TestFile::create("file_untag.txt", "content").unwrap();
    let canonical_path = std::fs::canonicalize("file_untag.txt").unwrap();

    test_db
        .db()
        .insert(
            &canonical_path,
            vec!["tag1".into(), "tag2".into(), "tag3".into()],
        )
        .unwrap();

    // Verify file is in database with 3 tags
    assert!(test_db.db().contains(&canonical_path).unwrap());
    let initial_tags = test_db.db().get_tags(&canonical_path).unwrap().unwrap();
    assert_eq!(initial_tags.len(), 3);

    // Remove two tags, leaving one
    test_db
        .db()
        .remove_tags(&canonical_path, &["tag1".into(), "tag3".into()])
        .unwrap();

    // File should still be in database with remaining tag
    assert!(test_db.db().contains(&canonical_path).unwrap());
    let remaining_tags = test_db.db().get_tags(&canonical_path).unwrap().unwrap();
    assert_eq!(remaining_tags.len(), 1);
    assert_eq!(remaining_tags[0], "tag2");

    // Now remove the last tag - file should be removed from database
    test_db
        .db()
        .remove_tags(&canonical_path, &["tag2".into()])
        .unwrap();

    // Verify file is completely gone from database
    assert!(!test_db.db().contains(&canonical_path).unwrap());
    assert!(test_db.db().get_tags(&canonical_path).unwrap().is_none());

    let _ = fs::remove_file("file_untag.txt");
    // Cleanup happens automatically via Drop
}

#[test]
fn test_untag_command_all_tags() {
    let test_db = TestDb::new("untag_all");

    let _test_file = TestFile::create("file_untag_all.txt", "content").unwrap();
    let canonical_path = std::fs::canonicalize("file_untag_all.txt").unwrap();

    test_db
        .db()
        .insert(&canonical_path, vec!["tag1".into(), "tag2".into()])
        .unwrap();

    // Verify file is in database before removal
    assert!(test_db.db().contains(&canonical_path).unwrap());
    let tags_before = test_db.db().get_tags(&canonical_path).unwrap();
    assert!(tags_before.is_some());
    assert_eq!(tags_before.unwrap().len(), 2);

    // Remove all tags (should remove file from database)
    test_db.db().remove(&canonical_path).unwrap();

    // Verify file is completely gone from database
    assert!(!test_db.db().contains(&canonical_path).unwrap());
    let tags_after = test_db.db().get_tags(&canonical_path).unwrap();
    assert!(tags_after.is_none());

    let _ = fs::remove_file("file_untag_all.txt");
    // Cleanup happens automatically via Drop
}

#[test]
fn test_tags_list_command() {
    let test_db = TestDb::new("tags_list");

    let _test_file = TestFile::create("f1.txt", "c1").unwrap();
    let _test_file = TestFile::create("f2.txt", "c2").unwrap();
    let _test_file = TestFile::create("f3.txt", "c3").unwrap();

    test_db
        .db()
        .insert("f1.txt", vec!["rust".into(), "programming".into()])
        .unwrap();
    test_db.db().insert("f2.txt", vec!["rust".into()]).unwrap();
    test_db
        .db()
        .insert("f3.txt", vec!["python".into()])
        .unwrap();

    let tags = test_db.db().list_all_tags().unwrap();
    assert_eq!(tags.len(), 3);
    assert!(tags.contains(&"rust".into()));
    assert!(tags.contains(&"programming".into()));
    assert!(tags.contains(&"python".into()));

    let _ = fs::remove_file("f1.txt");
    let _ = fs::remove_file("f2.txt");
    let _ = fs::remove_file("f3.txt");
    // Cleanup happens automatically via Drop
}

#[test]
fn test_tags_remove_command() {
    let test_db = TestDb::new("tags_remove");

    let _test_file = TestFile::create("r1.txt", "c1").unwrap();
    let _test_file = TestFile::create("r2.txt", "c2").unwrap();

    test_db
        .db()
        .insert("r1.txt", vec!["removeme".into(), "keep".into()])
        .unwrap();
    test_db
        .db()
        .insert("r2.txt", vec!["removeme".into()])
        .unwrap();

    test_db.db().remove_tag_globally("removeme").unwrap();

    assert!(
        !test_db
            .db()
            .list_all_tags()
            .unwrap()
            .contains(&"removeme".into())
    );

    let r1_tags = test_db.db().get_tags("r1.txt").unwrap();
    assert_eq!(r1_tags, Some(vec!["keep".into()]));

    assert!(!test_db.db().contains("r2.txt").unwrap());

    let _ = fs::remove_file("r1.txt");
    let _ = fs::remove_file("r2.txt");
    // Cleanup happens automatically via Drop
}

#[test]
fn test_cleanup_command_missing_files() {
    let test_db = TestDb::new("cleanup_missing");

    let _test_file = TestFile::create("temp_file.txt", "temp").unwrap();

    test_db
        .db()
        .insert("temp_file.txt", vec!["tag".into()])
        .unwrap();

    fs::remove_file("temp_file.txt").unwrap();

    assert!(!Path::new("temp_file.txt").exists());

    assert!(test_db.db().contains("temp_file.txt").unwrap());

    test_db.db().remove("temp_file.txt").unwrap();

    assert!(!test_db.db().contains("temp_file.txt").unwrap());

    // Cleanup happens automatically via Drop
}

#[test]
fn test_cleanup_command_untagged_files() {
    let test_db = TestDb::new("cleanup_untagged");

    let _test_file = TestFile::create("untagged.txt", "content").unwrap();

    test_db
        .db()
        .insert("untagged.txt", vec!["temp".into()])
        .unwrap();
    test_db
        .db()
        .remove_tags("untagged.txt", &["temp".into()])
        .unwrap();

    assert!(!test_db.db().contains("untagged.txt").unwrap());

    let _ = fs::remove_file("untagged.txt");
    // Cleanup happens automatically via Drop
}

#[test]
fn test_execute_command_on_files() {
    let _test_file = TestFile::create("exec_test1.txt", "hello").unwrap();
    let _test_file = TestFile::create("exec_test2.txt", "world").unwrap();

    let files = vec![
        PathBuf::from("exec_test1.txt"),
        PathBuf::from("exec_test2.txt"),
    ];

    let success_count = execute_command_on_files(&files, "test -f {}", true);

    assert_eq!(success_count, 2);

    let _ = fs::remove_file("exec_test1.txt");
    let _ = fs::remove_file("exec_test2.txt");
}

#[test]
fn test_execute_command_on_files_with_failure() {
    let _test_file = TestFile::create("exec_fail_test.txt", "content").unwrap();

    let files = vec![PathBuf::from("exec_fail_test.txt")];

    let success_count = execute_command_on_files(&files, "false", true);

    assert_eq!(success_count, 0);

    let _ = fs::remove_file("exec_fail_test.txt");
}

#[test]
fn test_find_by_all_tags() {
    let test_db = TestDb::new("find_all_tags");

    let _test_file = TestFile::create("multi1.txt", "c1").unwrap();
    let _test_file = TestFile::create("multi2.txt", "c2").unwrap();
    let _test_file = TestFile::create("multi3.txt", "c3").unwrap();

    test_db
        .db()
        .insert("multi1.txt", vec!["rust".into(), "programming".into()])
        .unwrap();
    test_db
        .db()
        .insert("multi2.txt", vec!["rust".into()])
        .unwrap();
    test_db
        .db()
        .insert(
            "multi3.txt",
            vec!["rust".into(), "programming".into(), "web".into()],
        )
        .unwrap();

    let files = test_db
        .db()
        .find_by_all_tags(&["rust".into(), "programming".into()])
        .unwrap();
    assert_eq!(files.len(), 2);
    assert!(files.contains(&PathBuf::from("multi1.txt")));
    assert!(files.contains(&PathBuf::from("multi3.txt")));

    let _ = fs::remove_file("multi1.txt");
    let _ = fs::remove_file("multi2.txt");
    let _ = fs::remove_file("multi3.txt");
    // Cleanup happens automatically via Drop
}

#[test]
fn test_find_by_any_tag() {
    let test_db = TestDb::new("find_any_tag");

    let _test_file = TestFile::create("any1.txt", "c1").unwrap();
    let _test_file = TestFile::create("any2.txt", "c2").unwrap();
    let _test_file = TestFile::create("any3.txt", "c3").unwrap();

    test_db
        .db()
        .insert("any1.txt", vec!["rust".into()])
        .unwrap();
    test_db
        .db()
        .insert("any2.txt", vec!["python".into()])
        .unwrap();
    test_db
        .db()
        .insert("any3.txt", vec!["javascript".into()])
        .unwrap();

    let files = test_db
        .db()
        .find_by_any_tag(&["rust".into(), "python".into()])
        .unwrap();
    assert_eq!(files.len(), 2);
    assert!(files.contains(&PathBuf::from("any1.txt")));
    assert!(files.contains(&PathBuf::from("any2.txt")));

    let _ = fs::remove_file("any1.txt");
    let _ = fs::remove_file("any2.txt");
    let _ = fs::remove_file("any3.txt");
    // Cleanup happens automatically via Drop
}

#[test]
fn test_pair_struct_operations() {
    let file_path = PathBuf::from("test_pair.txt");
    let tags = vec!["tag1".into(), "tag2".into()];

    let pair = Pair::new(file_path.clone(), tags.clone());

    assert_eq!(pair.file, file_path);
    assert_eq!(pair.tags, tags);
}

#[test]
fn test_database_persistence() {
    let db_path = PathBuf::from("test_persistence");

    let _test_file = TestFile::create("persist.txt", "data").unwrap();

    {
        let db = Database::open(&db_path).unwrap();
        db.clear().unwrap();
        db.insert("persist.txt", vec!["persistent".into()]).unwrap();
        db.flush().unwrap();
    }

    {
        let db = Database::open(&db_path).unwrap();
        let tags = db.get_tags("persist.txt").unwrap();
        assert_eq!(tags, Some(vec!["persistent".into()]));
        db.clear().unwrap();
    }

    let _ = fs::remove_dir_all(db_path);
}

#[test]
fn test_list_all_files() {
    let test_db = TestDb::new("list_files");

    let _test_file = TestFile::create("list1.txt", "c1").unwrap();
    let _test_file = TestFile::create("list2.txt", "c2").unwrap();

    test_db.db().insert("list1.txt", vec!["a".into()]).unwrap();
    test_db.db().insert("list2.txt", vec!["b".into()]).unwrap();

    let files = test_db.db().list_all_files().unwrap();
    assert_eq!(files.len(), 2);
    assert!(files.contains(&PathBuf::from("list1.txt")));
    assert!(files.contains(&PathBuf::from("list2.txt")));

    let _ = fs::remove_file("list1.txt");
    let _ = fs::remove_file("list2.txt");
    // Cleanup happens automatically via Drop
}

#[test]
fn test_database_count() {
    let test_db = TestDb::new("count");

    assert_eq!(test_db.db().count(), 0);

    let _test_file = TestFile::create("count1.txt", "c1").unwrap();
    let _test_file = TestFile::create("count2.txt", "c2").unwrap();

    test_db
        .db()
        .insert("count1.txt", vec!["tag".into()])
        .unwrap();
    assert_eq!(test_db.db().count(), 1);

    test_db
        .db()
        .insert("count2.txt", vec!["tag".into()])
        .unwrap();
    assert_eq!(test_db.db().count(), 2);

    test_db.db().remove("count1.txt").unwrap();
    assert_eq!(test_db.db().count(), 1);

    let _ = fs::remove_file("count1.txt");
    let _ = fs::remove_file("count2.txt");
    // Cleanup happens automatically via Drop
}

#[test]
fn test_insert_nonexistent_file() {
    let test_db = TestDb::new("nonexistent");

    let result = test_db
        .db()
        .insert("this_file_does_not_exist.txt", vec!["tag".into()]);

    assert!(result.is_err());
    if let Err(e) = result {
        assert!(e.to_string().contains("File not found"));
    }

    // Cleanup happens automatically via Drop
}

#[test]
fn test_get_pair() {
    let test_db = TestDb::new("get_pair");

    let _test_file = TestFile::create("pair.txt", "content").unwrap();

    test_db
        .db()
        .insert("pair.txt", vec!["tag1".into(), "tag2".into()])
        .unwrap();

    let pair = test_db.db().get_pair("pair.txt").unwrap();
    assert!(pair.is_some());

    let pair = pair.unwrap();
    assert_eq!(pair.file, PathBuf::from("pair.txt"));
    assert_eq!(pair.tags, vec!["tag1".to_string(), "tag2".to_string()]);

    let _ = fs::remove_file("pair.txt");
    // Cleanup happens automatically via Drop
}

// ============================================================================
// Filter Integration Tests
// ============================================================================

use tagr::filters::{FileMode, FilterCriteria, FilterManager, TagMode};

/// RAII wrapper for `FilterManager` with automatic cleanup
struct TestFilterManager {
    manager: FilterManager,
    path: PathBuf,
}

impl TestFilterManager {
    fn new(test_name: &str) -> Self {
        let path = PathBuf::from(format!("test_filters_{test_name}.toml"));
        // Clean up any leftover files from previous test runs
        let _ = fs::remove_file(&path);
        let backup_path = path.with_extension("toml.backup");
        let _ = fs::remove_file(&backup_path);

        let manager = FilterManager::new(path.clone());
        Self { manager, path }
    }

    const fn manager(&self) -> &FilterManager {
        &self.manager
    }

    #[allow(dead_code)]
    const fn path(&self) -> &PathBuf {
        &self.path
    }
}

impl Drop for TestFilterManager {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
        let backup_path = self.path.with_extension("toml.backup");
        let _ = fs::remove_file(&backup_path);
    }
}

/// Helper to clean up exported filter files
struct TempFilterFile(PathBuf);

impl TempFilterFile {
    fn new(name: &str) -> Self {
        let path = PathBuf::from(name);
        // Clean up any leftover from previous runs
        let _ = fs::remove_file(&path);
        Self(path)
    }

    const fn path(&self) -> &PathBuf {
        &self.0
    }
}

impl Drop for TempFilterFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[test]
fn test_filter_create_and_list() {
    let test_mgr = TestFilterManager::new("create_list");
    let manager = test_mgr.manager();

    let criteria = FilterCriteria::builder()
        .tags(vec!["rust".into(), "tutorial".into()])
        .tag_mode(TagMode::All)
        .file_patterns(vec!["*.rs".into()])
        .file_mode(FileMode::Any)
        .build();

    let result = manager.create("test-filter", "Test filter".into(), criteria);
    assert!(result.is_ok());

    let filters = manager.list().unwrap();
    assert_eq!(filters.len(), 1);
    assert_eq!(filters[0].name, "test-filter");
    assert_eq!(filters[0].description, "Test filter");
}

#[test]
fn test_filter_create_with_all_options() {
    let test_mgr = TestFilterManager::new("create_full");
    let manager = test_mgr.manager();

    let criteria = FilterCriteria::builder()
        .tags(vec![
            "rust".into(),
            "tutorial".into(),
            "documentation".into(),
        ])
        .tag_mode(TagMode::All)
        .file_patterns(vec!["*.rs".into(), "*.toml".into()])
        .file_mode(FileMode::Any)
        .excludes(vec!["deprecated".into(), "old".into()])
        .regex_tag(true)
        .regex_file(false)
        .build();

    let filter = manager
        .create(
            "complex-filter",
            "Complex filter with all options".into(),
            criteria,
        )
        .unwrap();

    assert_eq!(filter.name, "complex-filter");
    assert_eq!(filter.criteria.tags.len(), 3);
    assert_eq!(filter.criteria.file_patterns.len(), 2);
    assert_eq!(filter.criteria.excludes.len(), 2);
    assert_eq!(filter.criteria.tag_mode, TagMode::All);
    assert_eq!(filter.criteria.file_mode, FileMode::Any);
    assert!(filter.criteria.regex_tag);
    assert!(!filter.criteria.regex_file);
}

#[test]
fn test_filter_get_and_show() {
    let test_mgr = TestFilterManager::new("get_show");
    let manager = test_mgr.manager();

    let criteria = FilterCriteria::builder()
        .tags(vec!["rust".into()])
        .tag_mode(TagMode::All)
        .file_patterns(vec!["src/*.rs".into()])
        .file_mode(FileMode::All)
        .build();

    manager
        .create("get-test", "Get test filter".into(), criteria)
        .unwrap();

    let filter = manager.get("get-test").unwrap();
    assert_eq!(filter.name, "get-test");
    assert_eq!(filter.description, "Get test filter");
    assert_eq!(filter.criteria.tags, vec!["rust"]);
    assert_eq!(filter.criteria.file_patterns, vec!["src/*.rs"]);
}

#[test]
fn test_filter_get_nonexistent() {
    let test_mgr = TestFilterManager::new("get_nonexistent");
    let manager = test_mgr.manager();

    let result = manager.get("does-not-exist");
    assert!(result.is_err());
}

#[test]
fn test_filter_rename() {
    let test_mgr = TestFilterManager::new("rename");
    let manager = test_mgr.manager();

    let criteria = FilterCriteria::builder().tag("test".into()).build();

    manager
        .create("old-name", "Description".into(), criteria)
        .unwrap();

    let result = manager.rename("old-name", "new-name".to_string());
    assert!(result.is_ok());

    assert!(manager.get("old-name").is_err());
    assert!(manager.get("new-name").is_ok());
}

#[test]
fn test_filter_delete() {
    let test_mgr = TestFilterManager::new("delete");
    let manager = test_mgr.manager();

    let criteria = FilterCriteria::builder().tag("test".into()).build();

    manager
        .create("to-delete", "Will be deleted".into(), criteria)
        .unwrap();
    assert!(manager.get("to-delete").is_ok());

    let result = manager.delete("to-delete");
    assert!(result.is_ok());

    assert!(manager.get("to-delete").is_err());
}

#[test]
fn test_filter_duplicate_name() {
    let test_mgr = TestFilterManager::new("duplicate");
    let manager = test_mgr.manager();

    let criteria = FilterCriteria::builder().tag("test".into()).build();

    manager
        .create("duplicate", "First".into(), criteria.clone())
        .unwrap();

    let result = manager.create("duplicate", "Second".into(), criteria);
    assert!(result.is_err());
}

#[test]
fn test_filter_export_and_import() {
    let test_mgr = TestFilterManager::new("export_import");
    let manager = test_mgr.manager();
    let temp_file = TempFilterFile::new("test_export.toml");
    let export_path = temp_file.path();

    let criteria1 = FilterCriteria::builder()
        .tag("rust".into())
        .file_pattern("*.rs".into())
        .build();

    let criteria2 = FilterCriteria::builder()
        .tag("python".into())
        .file_pattern("*.py".into())
        .build();

    manager
        .create("filter1", "First filter".into(), criteria1)
        .unwrap();
    manager
        .create("filter2", "Second filter".into(), criteria2)
        .unwrap();

    // Export
    let result = manager.export(export_path, &[]);
    assert!(result.is_ok());
    assert!(export_path.exists());

    // Create new manager and import
    let test_mgr2 = TestFilterManager::new("import_dest");
    let manager2 = test_mgr2.manager();

    let result = manager2.import(export_path, false, false);
    assert!(result.is_ok());

    let filters = manager2.list().unwrap();
    assert_eq!(filters.len(), 2);
}

#[test]
fn test_filter_export_selective() {
    let test_mgr = TestFilterManager::new("export_selective");
    let manager = test_mgr.manager();
    let temp_file = TempFilterFile::new("test_export_selective.toml");
    let export_path = temp_file.path();

    let criteria = FilterCriteria::builder().tag("test".into()).build();

    manager
        .create("filter-a", "A".into(), criteria.clone())
        .unwrap();
    manager
        .create("filter-b", "B".into(), criteria.clone())
        .unwrap();
    manager.create("filter-c", "C".into(), criteria).unwrap();

    // Export only filter-a and filter-c
    let result = manager.export(
        export_path,
        &["filter-a".to_string(), "filter-c".to_string()],
    );
    assert!(result.is_ok());

    // Import to new manager
    let test_mgr2 = TestFilterManager::new("import_selective");
    let manager2 = test_mgr2.manager();
    manager2.import(export_path, false, false).unwrap();

    let filters = manager2.list().unwrap();
    assert_eq!(filters.len(), 2);
    assert!(manager2.get("filter-a").is_ok());
    assert!(manager2.get("filter-c").is_ok());
    assert!(manager2.get("filter-b").is_err());
}

#[test]
fn test_filter_import_conflict_skip() {
    let test_mgr = TestFilterManager::new("import_skip");
    let manager = test_mgr.manager();
    let temp_file = TempFilterFile::new("test_import_skip.toml");
    let export_path = temp_file.path();

    let criteria1 = FilterCriteria::builder().tag("existing".into()).build();

    let criteria2 = FilterCriteria::builder().tag("new".into()).build();

    // Create existing filter
    manager
        .create("conflict", "Original".into(), criteria1.clone())
        .unwrap();

    // Export a filter with same name but different description
    let test_mgr_temp = TestFilterManager::new("temp_export_skip");
    let manager2 = test_mgr_temp.manager();
    manager2
        .create("conflict", "Imported".into(), criteria1)
        .unwrap();
    manager2
        .create("new-filter", "New".into(), criteria2)
        .unwrap();
    manager2.export(export_path, &[]).unwrap();

    // Import with skip-existing
    let result = manager.import(export_path, false, true);
    assert!(result.is_ok());

    // Original should remain unchanged
    let filter = manager.get("conflict").unwrap();
    assert_eq!(filter.description, "Original");

    // New filter should be imported
    assert!(manager.get("new-filter").is_ok());
}

#[test]
fn test_filter_import_conflict_overwrite() {
    let test_mgr = TestFilterManager::new("import_overwrite");
    let manager = test_mgr.manager();
    let temp_file = TempFilterFile::new("test_import_overwrite.toml");
    let export_path = temp_file.path();

    let criteria1 = FilterCriteria::builder().tag("original".into()).build();

    let criteria2 = FilterCriteria::builder().tag("updated".into()).build();

    // Create existing filter
    manager
        .create("overwrite-me", "Original".into(), criteria1)
        .unwrap();

    // Export updated version
    let test_mgr_temp = TestFilterManager::new("temp_export_overwrite");
    let manager2 = test_mgr_temp.manager();
    manager2
        .create("overwrite-me", "Updated".into(), criteria2)
        .unwrap();
    manager2.export(export_path, &[]).unwrap();

    // Import with overwrite
    let result = manager.import(export_path, true, false);
    assert!(result.is_ok());

    // Should be updated
    let filter = manager.get("overwrite-me").unwrap();
    assert_eq!(filter.description, "Updated");
    assert_eq!(filter.criteria.tags, vec!["updated"]);
}

#[test]
fn test_filter_usage_tracking() {
    let test_mgr = TestFilterManager::new("usage_tracking");
    let manager = test_mgr.manager();

    let criteria = FilterCriteria::builder().tag("test".into()).build();

    let filter = manager
        .create("track-usage", "Test".into(), criteria)
        .unwrap();
    assert_eq!(filter.use_count, 0);

    // Record usage
    manager.record_use("track-usage").unwrap();

    let updated = manager.get("track-usage").unwrap();
    assert_eq!(updated.use_count, 1);

    // Record again
    manager.record_use("track-usage").unwrap();
    let updated2 = manager.get("track-usage").unwrap();
    assert_eq!(updated2.use_count, 2);
}

#[test]
fn test_filter_criteria_validation() {
    let test_mgr = TestFilterManager::new("validation");
    let manager = test_mgr.manager();

    // Empty criteria should fail
    let empty_criteria = FilterCriteria::builder().build();

    let result = manager.create("invalid", "Invalid".into(), empty_criteria);
    assert!(result.is_err());
}

#[test]
fn test_filter_name_validation() {
    let test_mgr = TestFilterManager::new("name_validation");
    let manager = test_mgr.manager();

    let criteria = FilterCriteria::builder().tag("test".into()).build();

    // Invalid characters
    let result = manager.create("invalid name!", "Invalid".into(), criteria.clone());
    assert!(result.is_err());

    // Too long
    let long_name = "a".repeat(100);
    let result = manager.create(&long_name, "Too long".into(), criteria);
    assert!(result.is_err());
}

// ============================================================================
// Hierarchical Tag Filtering Tests
// ============================================================================

#[test]
fn test_hierarchy_prefix_matching() {
    let test_db = TestDb::new("hierarchy_prefix");
    let db = test_db.db();

    let file1 = TestFile::create("file1.js", "").unwrap();
    let file2 = TestFile::create("file2.rs", "").unwrap();
    let file3 = TestFile::create("file3.py", "").unwrap();

    db.insert_pair(&Pair::new(
        file1.path().to_path_buf(),
        vec!["lang:javascript".into(), "production".into()],
    ))
    .unwrap();
    db.insert_pair(&Pair::new(
        file2.path().to_path_buf(),
        vec!["lang:rust".into(), "tests".into()],
    ))
    .unwrap();
    db.insert_pair(&Pair::new(
        file3.path().to_path_buf(),
        vec!["lang:python".into(), "tests".into()],
    ))
    .unwrap();

    // Search for "-t lang" should match all files with lang:* tags
    let params = SearchParams {
        query: None,
        tags: vec!["lang".to_string()],
        tag_mode: SearchMode::Any,
        file_patterns: vec![],
        file_mode: SearchMode::All,
        exclude_tags: vec![],
        regex_tag: false,
        regex_file: false,
        glob_files: false,
        virtual_tags: vec![],
        virtual_mode: SearchMode::All,
        no_hierarchy: false,
    };

    let results = tagr::db::query::apply_search_params(db, &params).unwrap();
    assert_eq!(results.len(), 3);
}

#[test]
fn test_hierarchy_specificity_exclude_wins() {
    let test_db = TestDb::new("hierarchy_specificity");
    let db = test_db.db();

    let file1 = TestFile::create("spec1.js", "").unwrap();
    let file2 = TestFile::create("spec2.rs", "").unwrap();

    db.insert_pair(&Pair::new(
        file1.path().to_path_buf(),
        vec!["lang:javascript".into()],
    ))
    .unwrap();
    db.insert_pair(&Pair::new(
        file2.path().to_path_buf(),
        vec!["lang:rust".into()],
    ))
    .unwrap();

    // Search: -t lang -x lang:rust
    // Should include lang:javascript but exclude lang:rust
    let params = SearchParams {
        query: None,
        tags: vec!["lang".to_string()],
        tag_mode: SearchMode::Any,
        file_patterns: vec![],
        file_mode: SearchMode::All,
        exclude_tags: vec!["lang:rust".to_string()],
        regex_tag: false,
        regex_file: false,
        glob_files: false,
        virtual_tags: vec![],
        virtual_mode: SearchMode::All,
        no_hierarchy: false,
    };

    let results = tagr::db::query::apply_search_params(db, &params).unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].to_str().unwrap().contains("spec1.js"));
}

#[test]
fn test_hierarchy_cross_hierarchy_exclude() {
    let test_db = TestDb::new("cross_hierarchy");
    let db = test_db.db();

    let file1 = TestFile::create("cross1.js", "").unwrap();
    let file2 = TestFile::create("cross2.js", "").unwrap();

    db.insert_pair(&Pair::new(
        file1.path().to_path_buf(),
        vec!["lang:javascript".into(), "production".into()],
    ))
    .unwrap();
    db.insert_pair(&Pair::new(
        file2.path().to_path_buf(),
        vec!["lang:javascript".into(), "tests".into()],
    ))
    .unwrap();

    // Search: -t lang -x tests
    // Different hierarchies - exclude wins
    let params = SearchParams {
        query: None,
        tags: vec!["lang".to_string()],
        tag_mode: SearchMode::Any,
        file_patterns: vec![],
        file_mode: SearchMode::All,
        exclude_tags: vec!["tests".to_string()],
        regex_tag: false,
        regex_file: false,
        glob_files: false,
        virtual_tags: vec![],
        virtual_mode: SearchMode::All,
        no_hierarchy: false,
    };

    let results = tagr::db::query::apply_search_params(db, &params).unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].to_str().unwrap().contains("cross1.js"));
}

#[test]
fn test_hierarchy_deeper_include_overrides_exclude() {
    let test_db = TestDb::new("deeper_override");
    let db = test_db.db();

    let file = TestFile::create("deep.rs", "").unwrap();

    db.insert_pair(&Pair::new(
        file.path().to_path_buf(),
        vec!["lang:rust:async".into()],
    ))
    .unwrap();

    // Search: -t lang -t lang:rust:async -x lang:rust
    // Depth 3 include should override depth 2 exclude
    let params = SearchParams {
        query: None,
        tags: vec!["lang".to_string(), "lang:rust:async".to_string()],
        tag_mode: SearchMode::Any,
        file_patterns: vec![],
        file_mode: SearchMode::All,
        exclude_tags: vec!["lang:rust".to_string()],
        regex_tag: false,
        regex_file: false,
        glob_files: false,
        virtual_tags: vec![],
        virtual_mode: SearchMode::All,
        no_hierarchy: false,
    };

    let results = tagr::db::query::apply_search_params(db, &params).unwrap();
    assert_eq!(results.len(), 1);
}

#[test]
fn test_hierarchy_all_mode_requires_all_patterns() {
    let test_db = TestDb::new("all_mode_hierarchy");
    let db = test_db.db();

    let file1 = TestFile::create("all1.rs", "").unwrap();
    let file2 = TestFile::create("all2.rs", "").unwrap();
    let file3 = TestFile::create("all3.rs", "").unwrap();

    db.insert_pair(&Pair::new(
        file1.path().to_path_buf(),
        vec!["lang:rust".into(), "project:backend".into()],
    ))
    .unwrap();
    db.insert_pair(&Pair::new(
        file2.path().to_path_buf(),
        vec!["lang:rust".into()],
    ))
    .unwrap();
    db.insert_pair(&Pair::new(
        file3.path().to_path_buf(),
        vec!["project:backend".into()],
    ))
    .unwrap();

    // Search: -t lang -t project --all-tags
    // Only file1 has tags matching both patterns
    let params = SearchParams {
        query: None,
        tags: vec!["lang".to_string(), "project".to_string()],
        tag_mode: SearchMode::All,
        file_patterns: vec![],
        file_mode: SearchMode::All,
        exclude_tags: vec![],
        regex_tag: false,
        regex_file: false,
        glob_files: false,
        virtual_tags: vec![],
        virtual_mode: SearchMode::All,
        no_hierarchy: false,
    };

    let results = tagr::db::query::apply_search_params(db, &params).unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].to_str().unwrap().contains("all1.rs"));
}

#[test]
fn test_hierarchy_no_hierarchy_flag_disables_prefix_matching() {
    let test_db = TestDb::new("no_hierarchy_flag");
    let db = test_db.db();

    let file1 = TestFile::create("nohier1.rs", "").unwrap();
    let file2 = TestFile::create("nohier2.rs", "").unwrap();

    db.insert_pair(&Pair::new(
        file1.path().to_path_buf(),
        vec!["lang:rust".into()],
    ))
    .unwrap();
    db.insert_pair(&Pair::new(
        file2.path().to_path_buf(),
        vec!["lang".into()],
    ))
    .unwrap();

    // Search: -t lang --no-hierarchy
    // Should only match file2 (exact match only)
    let params = SearchParams {
        query: None,
        tags: vec!["lang".to_string()],
        tag_mode: SearchMode::Any,
        file_patterns: vec![],
        file_mode: SearchMode::All,
        exclude_tags: vec![],
        regex_tag: false,
        regex_file: false,
        glob_files: false,
        virtual_tags: vec![],
        virtual_mode: SearchMode::All,
        no_hierarchy: true,
    };

    let results = tagr::db::query::apply_search_params(db, &params).unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].to_str().unwrap().contains("nohier2.rs"));
}
