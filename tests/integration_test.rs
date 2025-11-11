//! Integration tests for tagr CLI
//!
//! These tests verify end-to-end functionality by creating temporary databases
//! and testing the complete command workflows.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tagr::{db::Database, cli::execute_command_on_files, Pair};

/// Helper function to create a temporary test database
fn setup_test_db(name: &str) -> (Database, PathBuf) {
    let db_path = PathBuf::from(format!("test_integration_{}", name));
    let db = Database::open(&db_path).unwrap();
    db.clear().unwrap();
    (db, db_path)
}

/// Helper function to clean up test database
fn cleanup_test_db(db: Database, db_path: PathBuf) {
    drop(db);
    let _ = fs::remove_dir_all(db_path);
}

/// Helper function to create a test file
fn create_test_file(path: &str, content: &str) -> std::io::Result<()> {
    let mut file = fs::File::create(path)?;
    file.write_all(content.as_bytes())?;
    Ok(())
}

#[test]
fn test_tag_command_with_new_file() {
    let (db, db_path) = setup_test_db("tag_new");
    
    create_test_file("test_tag_file.txt", "test content").unwrap();
    
    let result = db.insert("test_tag_file.txt", vec!["rust".into(), "test".into()]);
    assert!(result.is_ok());
    
    let tags = db.get_tags("test_tag_file.txt").unwrap();
    assert_eq!(tags, Some(vec!["rust".into(), "test".into()]));
    
    let _ = fs::remove_file("test_tag_file.txt");
    cleanup_test_db(db, db_path);
}

#[test]
fn test_tag_command_add_tags() {
    let (db, db_path) = setup_test_db("tag_add");
    
    create_test_file("test_add_tags.txt", "content").unwrap();
    
    db.insert("test_add_tags.txt", vec!["tag1".into()]).unwrap();
    
    db.add_tags("test_add_tags.txt", vec!["tag2".into(), "tag3".into()]).unwrap();
    
    let tags = db.get_tags("test_add_tags.txt").unwrap().unwrap();
    assert!(tags.contains(&"tag1".into()));
    assert!(tags.contains(&"tag2".into()));
    assert!(tags.contains(&"tag3".into()));
    
    let _ = fs::remove_file("test_add_tags.txt");
    cleanup_test_db(db, db_path);
}

#[test]
fn test_search_command_single_tag() {
    let (db, db_path) = setup_test_db("search_single");
    
    create_test_file("file1.txt", "content1").unwrap();
    create_test_file("file2.txt", "content2").unwrap();
    create_test_file("file3.txt", "content3").unwrap();
    
    db.insert("file1.txt", vec!["rust".into()]).unwrap();
    db.insert("file2.txt", vec!["rust".into(), "programming".into()]).unwrap();
    db.insert("file3.txt", vec!["python".into()]).unwrap();
    
    let files = db.find_by_tag("rust").unwrap();
    assert_eq!(files.len(), 2);
    assert!(files.contains(&PathBuf::from("file1.txt")));
    assert!(files.contains(&PathBuf::from("file2.txt")));
    
    let _ = fs::remove_file("file1.txt");
    let _ = fs::remove_file("file2.txt");
    let _ = fs::remove_file("file3.txt");
    cleanup_test_db(db, db_path);
}

#[test]
fn test_search_command_nonexistent_tag() {
    let (db, db_path) = setup_test_db("search_nonexistent");
    
    create_test_file("file1.txt", "content").unwrap();
    db.insert("file1.txt", vec!["exists".into()]).unwrap();
    
    let files = db.find_by_tag("nonexistent").unwrap();
    assert!(files.is_empty());
    
    let _ = fs::remove_file("file1.txt");
    cleanup_test_db(db, db_path);
}

#[test]
fn test_untag_command_specific_tags() {
    let (db, db_path) = setup_test_db("untag_specific");
    
    create_test_file("file_untag.txt", "content").unwrap();
    let canonical_path = std::fs::canonicalize("file_untag.txt").unwrap();
    
    db.insert(&canonical_path, vec!["tag1".into(), "tag2".into(), "tag3".into()]).unwrap();
    
    // Verify file is in database with 3 tags
    assert!(db.contains(&canonical_path).unwrap());
    let initial_tags = db.get_tags(&canonical_path).unwrap().unwrap();
    assert_eq!(initial_tags.len(), 3);
    
    // Remove two tags, leaving one
    db.remove_tags(&canonical_path, &["tag1".into(), "tag3".into()]).unwrap();
    
    // File should still be in database with remaining tag
    assert!(db.contains(&canonical_path).unwrap());
    let remaining_tags = db.get_tags(&canonical_path).unwrap().unwrap();
    assert_eq!(remaining_tags.len(), 1);
    assert_eq!(remaining_tags[0], "tag2");
    
    // Now remove the last tag - file should be removed from database
    db.remove_tags(&canonical_path, &["tag2".into()]).unwrap();
    
    // Verify file is completely gone from database
    assert!(!db.contains(&canonical_path).unwrap());
    assert!(db.get_tags(&canonical_path).unwrap().is_none());
    
    let _ = fs::remove_file("file_untag.txt");
    cleanup_test_db(db, db_path);
}

#[test]
fn test_untag_command_all_tags() {
    let (db, db_path) = setup_test_db("untag_all");
    
    create_test_file("file_untag_all.txt", "content").unwrap();
    let canonical_path = std::fs::canonicalize("file_untag_all.txt").unwrap();
    
    db.insert(&canonical_path, vec!["tag1".into(), "tag2".into()]).unwrap();
    
    // Verify file is in database before removal
    assert!(db.contains(&canonical_path).unwrap());
    let tags_before = db.get_tags(&canonical_path).unwrap();
    assert!(tags_before.is_some());
    assert_eq!(tags_before.unwrap().len(), 2);
    
    // Remove all tags (should remove file from database)
    db.remove(&canonical_path).unwrap();
    
    // Verify file is completely gone from database
    assert!(!db.contains(&canonical_path).unwrap());
    let tags_after = db.get_tags(&canonical_path).unwrap();
    assert!(tags_after.is_none());
    
    let _ = fs::remove_file("file_untag_all.txt");
    cleanup_test_db(db, db_path);
}

#[test]
fn test_tags_list_command() {
    let (db, db_path) = setup_test_db("tags_list");
    
    create_test_file("f1.txt", "c1").unwrap();
    create_test_file("f2.txt", "c2").unwrap();
    create_test_file("f3.txt", "c3").unwrap();
    
    db.insert("f1.txt", vec!["rust".into(), "programming".into()]).unwrap();
    db.insert("f2.txt", vec!["rust".into()]).unwrap();
    db.insert("f3.txt", vec!["python".into()]).unwrap();
    
    let tags = db.list_all_tags().unwrap();
    assert_eq!(tags.len(), 3);
    assert!(tags.contains(&"rust".into()));
    assert!(tags.contains(&"programming".into()));
    assert!(tags.contains(&"python".into()));
    
    let _ = fs::remove_file("f1.txt");
    let _ = fs::remove_file("f2.txt");
    let _ = fs::remove_file("f3.txt");
    cleanup_test_db(db, db_path);
}

#[test]
fn test_tags_remove_command() {
    let (db, db_path) = setup_test_db("tags_remove");
    
    create_test_file("r1.txt", "c1").unwrap();
    create_test_file("r2.txt", "c2").unwrap();
    
    db.insert("r1.txt", vec!["removeme".into(), "keep".into()]).unwrap();
    db.insert("r2.txt", vec!["removeme".into()]).unwrap();
    
    db.remove_tag_globally("removeme").unwrap();
    
    assert!(!db.list_all_tags().unwrap().contains(&"removeme".into()));
    
    let r1_tags = db.get_tags("r1.txt").unwrap();
    assert_eq!(r1_tags, Some(vec!["keep".into()]));
    
    assert!(!db.contains("r2.txt").unwrap());
    
    let _ = fs::remove_file("r1.txt");
    let _ = fs::remove_file("r2.txt");
    cleanup_test_db(db, db_path);
}

#[test]
fn test_cleanup_command_missing_files() {
    let (db, db_path) = setup_test_db("cleanup_missing");
    
    create_test_file("temp_file.txt", "temp").unwrap();
    
    db.insert("temp_file.txt", vec!["tag".into()]).unwrap();
    
    fs::remove_file("temp_file.txt").unwrap();
    
    assert!(!Path::new("temp_file.txt").exists());
    
    assert!(db.contains("temp_file.txt").unwrap());
    
    db.remove("temp_file.txt").unwrap();
    
    assert!(!db.contains("temp_file.txt").unwrap());
    
    cleanup_test_db(db, db_path);
}

#[test]
fn test_cleanup_command_untagged_files() {
    let (db, db_path) = setup_test_db("cleanup_untagged");
    
    create_test_file("untagged.txt", "content").unwrap();
    
    db.insert("untagged.txt", vec!["temp".into()]).unwrap();
    db.remove_tags("untagged.txt", &["temp".into()]).unwrap();
    
    assert!(!db.contains("untagged.txt").unwrap());
    
    let _ = fs::remove_file("untagged.txt");
    cleanup_test_db(db, db_path);
}

#[test]
fn test_execute_command_on_files() {
    create_test_file("exec_test1.txt", "hello").unwrap();
    create_test_file("exec_test2.txt", "world").unwrap();
    
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
    create_test_file("exec_fail_test.txt", "content").unwrap();
    
    let files = vec![PathBuf::from("exec_fail_test.txt")];
    
    let success_count = execute_command_on_files(&files, "false", true);
    
    assert_eq!(success_count, 0);
    
    let _ = fs::remove_file("exec_fail_test.txt");
}

#[test]
fn test_find_by_all_tags() {
    let (db, db_path) = setup_test_db("find_all_tags");
    
    create_test_file("multi1.txt", "c1").unwrap();
    create_test_file("multi2.txt", "c2").unwrap();
    create_test_file("multi3.txt", "c3").unwrap();
    
    db.insert("multi1.txt", vec!["rust".into(), "programming".into()]).unwrap();
    db.insert("multi2.txt", vec!["rust".into()]).unwrap();
    db.insert("multi3.txt", vec!["rust".into(), "programming".into(), "web".into()]).unwrap();
    
    let files = db.find_by_all_tags(&["rust".into(), "programming".into()]).unwrap();
    assert_eq!(files.len(), 2);
    assert!(files.contains(&PathBuf::from("multi1.txt")));
    assert!(files.contains(&PathBuf::from("multi3.txt")));
    
    let _ = fs::remove_file("multi1.txt");
    let _ = fs::remove_file("multi2.txt");
    let _ = fs::remove_file("multi3.txt");
    cleanup_test_db(db, db_path);
}

#[test]
fn test_find_by_any_tag() {
    let (db, db_path) = setup_test_db("find_any_tag");
    
    create_test_file("any1.txt", "c1").unwrap();
    create_test_file("any2.txt", "c2").unwrap();
    create_test_file("any3.txt", "c3").unwrap();
    
    db.insert("any1.txt", vec!["rust".into()]).unwrap();
    db.insert("any2.txt", vec!["python".into()]).unwrap();
    db.insert("any3.txt", vec!["javascript".into()]).unwrap();
    
    let files = db.find_by_any_tag(&["rust".into(), "python".into()]).unwrap();
    assert_eq!(files.len(), 2);
    assert!(files.contains(&PathBuf::from("any1.txt")));
    assert!(files.contains(&PathBuf::from("any2.txt")));
    
    let _ = fs::remove_file("any1.txt");
    let _ = fs::remove_file("any2.txt");
    let _ = fs::remove_file("any3.txt");
    cleanup_test_db(db, db_path);
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
    
    create_test_file("persist.txt", "data").unwrap();
    
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
    
    let _ = fs::remove_file("persist.txt");
    let _ = fs::remove_dir_all(db_path);
}

#[test]
fn test_list_all_files() {
    let (db, db_path) = setup_test_db("list_files");
    
    create_test_file("list1.txt", "c1").unwrap();
    create_test_file("list2.txt", "c2").unwrap();
    
    db.insert("list1.txt", vec!["a".into()]).unwrap();
    db.insert("list2.txt", vec!["b".into()]).unwrap();
    
    let files = db.list_all_files().unwrap();
    assert_eq!(files.len(), 2);
    assert!(files.contains(&PathBuf::from("list1.txt")));
    assert!(files.contains(&PathBuf::from("list2.txt")));
    
    let _ = fs::remove_file("list1.txt");
    let _ = fs::remove_file("list2.txt");
    cleanup_test_db(db, db_path);
}

#[test]
fn test_database_count() {
    let (db, db_path) = setup_test_db("count");
    
    assert_eq!(db.count(), 0);
    
    create_test_file("count1.txt", "c1").unwrap();
    create_test_file("count2.txt", "c2").unwrap();
    
    db.insert("count1.txt", vec!["tag".into()]).unwrap();
    assert_eq!(db.count(), 1);
    
    db.insert("count2.txt", vec!["tag".into()]).unwrap();
    assert_eq!(db.count(), 2);
    
    db.remove("count1.txt").unwrap();
    assert_eq!(db.count(), 1);
    
    let _ = fs::remove_file("count1.txt");
    let _ = fs::remove_file("count2.txt");
    cleanup_test_db(db, db_path);
}

#[test]
fn test_insert_nonexistent_file() {
    let (db, db_path) = setup_test_db("nonexistent");
    
    let result = db.insert("this_file_does_not_exist.txt", vec!["tag".into()]);
    
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(e.to_string().contains("File not found"));
    }
    
    cleanup_test_db(db, db_path);
}

#[test]
fn test_get_pair() {
    let (db, db_path) = setup_test_db("get_pair");
    
    create_test_file("pair.txt", "content").unwrap();
    
    db.insert("pair.txt", vec!["tag1".into(), "tag2".into()]).unwrap();
    
    let pair = db.get_pair("pair.txt").unwrap();
    assert!(pair.is_some());
    
    let pair = pair.unwrap();
    assert_eq!(pair.file, PathBuf::from("pair.txt"));
    assert_eq!(pair.tags, vec!["tag1".to_string(), "tag2".to_string()]);
    
    let _ = fs::remove_file("pair.txt");
    cleanup_test_db(db, db_path);
}

// ============================================================================
// Filter Integration Tests
// ============================================================================

use tagr::filters::{FilterManager, FilterCriteria, TagMode, FileMode};

/// RAII wrapper for FilterManager with automatic cleanup
struct TestFilterManager {
    manager: FilterManager,
    path: PathBuf,
}

impl TestFilterManager {
    fn new(test_name: &str) -> Self {
        let path = PathBuf::from(format!("test_filters_{}.toml", test_name));
        // Clean up any leftover files from previous test runs
        let _ = fs::remove_file(&path);
        let backup_path = path.with_extension("toml.backup");
        let _ = fs::remove_file(&backup_path);
        
        let manager = FilterManager::new(path.clone());
        Self { manager, path }
    }
    
    fn manager(&self) -> &FilterManager {
        &self.manager
    }
    
    fn path(&self) -> &PathBuf {
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
    
    fn path(&self) -> &PathBuf {
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
    
    let criteria = FilterCriteria::from_search_params(
        vec!["rust".into(), "tutorial".into()],
        tagr::cli::SearchMode::All,
        vec!["*.rs".into()],
        tagr::cli::SearchMode::Any,
        vec![],
        false,
        false,
    );
    
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
    
    let criteria = FilterCriteria::from_search_params(
        vec!["rust".into(), "tutorial".into(), "documentation".into()],
        tagr::cli::SearchMode::All,
        vec!["*.rs".into(), "*.toml".into()],
        tagr::cli::SearchMode::Any,
        vec!["deprecated".into(), "old".into()],
        true,  // regex_tag
        false, // regex_file
    );
    
    let filter = manager.create(
        "complex-filter",
        "Complex filter with all options".into(),
        criteria
    ).unwrap();
    
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
    
    let criteria = FilterCriteria::from_search_params(
        vec!["rust".into()],
        tagr::cli::SearchMode::All,
        vec!["src/*.rs".into()],
        tagr::cli::SearchMode::All,
        vec![],
        false,
        false,
    );
    
    manager.create("get-test", "Get test filter".into(), criteria).unwrap();
    
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
    
    let criteria = FilterCriteria::from_search_params(
        vec!["test".into()],
        tagr::cli::SearchMode::All,
        vec![],
        tagr::cli::SearchMode::All,
        vec![],
        false,
        false,
    );
    
    manager.create("old-name", "Description".into(), criteria).unwrap();
    
    let result = manager.rename("old-name", "new-name".to_string());
    assert!(result.is_ok());
    
    assert!(manager.get("old-name").is_err());
    assert!(manager.get("new-name").is_ok());
}

#[test]
fn test_filter_delete() {
    let test_mgr = TestFilterManager::new("delete");
    let manager = test_mgr.manager();
    
    let criteria = FilterCriteria::from_search_params(
        vec!["test".into()],
        tagr::cli::SearchMode::All,
        vec![],
        tagr::cli::SearchMode::All,
        vec![],
        false,
        false,
    );
    
    manager.create("to-delete", "Will be deleted".into(), criteria).unwrap();
    assert!(manager.get("to-delete").is_ok());
    
    let result = manager.delete("to-delete");
    assert!(result.is_ok());
    
    assert!(manager.get("to-delete").is_err());
}

#[test]
fn test_filter_duplicate_name() {
    let test_mgr = TestFilterManager::new("duplicate");
    let manager = test_mgr.manager();
    
    let criteria = FilterCriteria::from_search_params(
        vec!["test".into()],
        tagr::cli::SearchMode::All,
        vec![],
        tagr::cli::SearchMode::All,
        vec![],
        false,
        false,
    );
    
    manager.create("duplicate", "First".into(), criteria.clone()).unwrap();
    
    let result = manager.create("duplicate", "Second".into(), criteria);
    assert!(result.is_err());
}

#[test]
fn test_filter_export_and_import() {
    let test_mgr = TestFilterManager::new("export_import");
    let manager = test_mgr.manager();
    let temp_file = TempFilterFile::new("test_export.toml");
    let export_path = temp_file.path();
    
    let criteria1 = FilterCriteria::from_search_params(
        vec!["rust".into()],
        tagr::cli::SearchMode::All,
        vec!["*.rs".into()],
        tagr::cli::SearchMode::All,
        vec![],
        false,
        false,
    );
    
    let criteria2 = FilterCriteria::from_search_params(
        vec!["python".into()],
        tagr::cli::SearchMode::All,
        vec!["*.py".into()],
        tagr::cli::SearchMode::All,
        vec![],
        false,
        false,
    );
    
    manager.create("filter1", "First filter".into(), criteria1).unwrap();
    manager.create("filter2", "Second filter".into(), criteria2).unwrap();
    
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
    
    let criteria = FilterCriteria::from_search_params(
        vec!["test".into()],
        tagr::cli::SearchMode::All,
        vec![],
        tagr::cli::SearchMode::All,
        vec![],
        false,
        false,
    );
    
    manager.create("filter-a", "A".into(), criteria.clone()).unwrap();
    manager.create("filter-b", "B".into(), criteria.clone()).unwrap();
    manager.create("filter-c", "C".into(), criteria).unwrap();
    
    // Export only filter-a and filter-c
    let result = manager.export(
        export_path,
        &["filter-a".to_string(), "filter-c".to_string()]
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
    
    let criteria1 = FilterCriteria::from_search_params(
        vec!["existing".into()],
        tagr::cli::SearchMode::All,
        vec![],
        tagr::cli::SearchMode::All,
        vec![],
        false,
        false,
    );
    
    let criteria2 = FilterCriteria::from_search_params(
        vec!["new".into()],
        tagr::cli::SearchMode::All,
        vec![],
        tagr::cli::SearchMode::All,
        vec![],
        false,
        false,
    );
    
    // Create existing filter
    manager.create("conflict", "Original".into(), criteria1.clone()).unwrap();
    
    // Export a filter with same name but different description
    let test_mgr_temp = TestFilterManager::new("temp_export_skip");
    let manager2 = test_mgr_temp.manager();
    manager2.create("conflict", "Imported".into(), criteria1).unwrap();
    manager2.create("new-filter", "New".into(), criteria2).unwrap();
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
    
    let criteria1 = FilterCriteria::from_search_params(
        vec!["original".into()],
        tagr::cli::SearchMode::All,
        vec![],
        tagr::cli::SearchMode::All,
        vec![],
        false,
        false,
    );
    
    let criteria2 = FilterCriteria::from_search_params(
        vec!["updated".into()],
        tagr::cli::SearchMode::All,
        vec![],
        tagr::cli::SearchMode::All,
        vec![],
        false,
        false,
    );
    
    // Create existing filter
    manager.create("overwrite-me", "Original".into(), criteria1).unwrap();
    
    // Export updated version
    let test_mgr_temp = TestFilterManager::new("temp_export_overwrite");
    let manager2 = test_mgr_temp.manager();
    manager2.create("overwrite-me", "Updated".into(), criteria2).unwrap();
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
    
    let criteria = FilterCriteria::from_search_params(
        vec!["test".into()],
        tagr::cli::SearchMode::All,
        vec![],
        tagr::cli::SearchMode::All,
        vec![],
        false,
        false,
    );
    
    let filter = manager.create("track-usage", "Test".into(), criteria).unwrap();
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
    let empty_criteria = FilterCriteria::from_search_params(
        vec![],
        tagr::cli::SearchMode::All,
        vec![],
        tagr::cli::SearchMode::All,
        vec![],
        false,
        false,
    );
    
    let result = manager.create("invalid", "Invalid".into(), empty_criteria);
    assert!(result.is_err());
}

#[test]
fn test_filter_name_validation() {
    let test_mgr = TestFilterManager::new("name_validation");
    let manager = test_mgr.manager();
    
    let criteria = FilterCriteria::from_search_params(
        vec!["test".into()],
        tagr::cli::SearchMode::All,
        vec![],
        tagr::cli::SearchMode::All,
        vec![],
        false,
        false,
    );
    
    // Invalid characters
    let result = manager.create("invalid name!", "Invalid".into(), criteria.clone());
    assert!(result.is_err());
    
    // Too long
    let long_name = "a".repeat(100);
    let result = manager.create(&long_name, "Too long".into(), criteria);
    assert!(result.is_err());
}

