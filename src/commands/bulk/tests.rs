use std::path::PathBuf;

use crate::cli::{ConditionalArgs, SearchMode, SearchParams};
use crate::testing::{TempFile, TestDb};

use super::batch::{parse_csv, parse_json, parse_plaintext};
use super::{
    BatchFormat, bulk_delete_files, bulk_map_tags, bulk_tag, bulk_untag, copy_tags, merge_tags,
    rename_tag,
};

#[test]
fn test_parse_plaintext_ok() {
    let input = "/a/b.txt tag1 tag2\n# comment\n/c/d.md tag3";
    let entries = parse_plaintext(input).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].file, PathBuf::from("/a/b.txt"));
    assert_eq!(entries[0].tags, vec!["tag1", "tag2"]);
    assert_eq!(entries[1].file, PathBuf::from("/c/d.md"));
    assert_eq!(entries[1].tags, vec!["tag3"]);
}

#[test]
fn test_parse_plaintext_bad_line() {
    let input = "onlyfile\n"; // missing tags
    let err = parse_plaintext(input).unwrap_err();
    assert!(format!("{}", err).contains("Invalid format"));
}

#[test]
fn test_parse_csv_ok_basic_and_quoted() {
    let input = "/a/b.txt,tag1,tag2\n/c/d.md,\"tag3,tag4\"";
    let entries = parse_csv(input, ',').unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].file, PathBuf::from("/a/b.txt"));
    assert_eq!(entries[0].tags, vec!["tag1", "tag2"]);
    assert_eq!(entries[1].file, PathBuf::from("/c/d.md"));
    assert_eq!(entries[1].tags, vec!["tag3", "tag4"]);
}

#[test]
fn test_parse_csv_custom_delimiter() {
    let input = "/a/b.txt;tag1;tag2";
    let entries = parse_csv(input, ';').unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].tags, vec!["tag1", "tag2"]);
}

#[test]
fn test_parse_csv_bad_missing_file() {
    let input = ",tag1,tag2";
    let err = parse_csv(input, ',').unwrap_err();
    assert!(format!("{}", err).contains("Invalid CSV"));
}

#[test]
fn test_parse_json_ok() {
    let input = r#"[{"file":"/a/b.txt","tags":["t1","t2"]}]"#;
    let entries = parse_json(input).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].file, PathBuf::from("/a/b.txt"));
    assert_eq!(entries[0].tags, vec!["t1", "t2"]);
}

#[test]
fn test_parse_json_bad_with_csv_hint() {
    let input = "/a/b.txt,tag1,tag2\n"; // CSV-looking content
    let err = parse_json(input).unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("Invalid JSON"));
    assert!(msg.contains("Hint: The file appears to be CSV"));
}

#[test]
fn test_bulk_tag_basic() {
    let test_db = TestDb::new("test_bulk_tag");
    let db = test_db.db();
    db.clear().unwrap();
    let file1 = TempFile::create("file1.txt").unwrap();
    let file2 = TempFile::create("file2.txt").unwrap();
    db.add_tags(file1.path(), vec!["initial".into()]).unwrap();
    db.add_tags(file2.path(), vec!["initial".into()]).unwrap();
    let params = SearchParams {
        query: None,
        tags: vec!["initial".into()],
        tag_mode: SearchMode::Any,
        file_patterns: vec![],
        file_mode: SearchMode::All,
        exclude_tags: vec![],
        regex_tag: false,
        regex_file: false,
        glob_files: false,
        virtual_tags: vec![],
        virtual_mode: SearchMode::All,
    };
    bulk_tag(
        db,
        &params,
        &["bulk".into(), "added".into()],
        &ConditionalArgs::default(),
        false,
        true,
        true,
    )
    .unwrap();
    let tags1 = db.get_tags(file1.path()).unwrap().unwrap();
    assert!(tags1.contains(&"bulk".into()));
    let tags2 = db.get_tags(file2.path()).unwrap().unwrap();
    assert!(tags2.contains(&"bulk".into()));
}

#[test]
fn test_bulk_untag_specific_tags() {
    let test_db = TestDb::new("test_bulk_untag");
    let db = test_db.db();
    db.clear().unwrap();
    let f1 = TempFile::create("file1.txt").unwrap();
    let f2 = TempFile::create("file2.txt").unwrap();
    db.add_tags(f1.path(), vec!["tag1".into(), "tag2".into(), "keep".into()])
        .unwrap();
    db.add_tags(f2.path(), vec!["tag1".into(), "tag2".into(), "keep".into()])
        .unwrap();
    let params = SearchParams {
        query: None,
        tags: vec!["tag1".into()],
        tag_mode: SearchMode::Any,
        file_patterns: vec![],
        file_mode: SearchMode::All,
        exclude_tags: vec![],
        regex_tag: false,
        regex_file: false,
        glob_files: false,
        virtual_tags: vec![],
        virtual_mode: SearchMode::All,
    };
    bulk_untag(
        db,
        &params,
        &["tag1".into(), "tag2".into()],
        false,
        &ConditionalArgs::default(),
        false,
        true,
        true,
    )
    .unwrap();
    let tags1 = db.get_tags(f1.path()).unwrap().unwrap();
    assert!(!tags1.contains(&"tag1".into()));
    assert!(tags1.contains(&"keep".into()));
}

#[test]
fn test_rename_tag_basic() {
    let test_db = TestDb::new("test_rename_tag");
    let db = test_db.db();
    db.clear().unwrap();
    let f1 = TempFile::create("file1.txt").unwrap();
    let f2 = TempFile::create("file2.txt").unwrap();
    db.add_tags(f1.path(), vec!["oldname".into(), "other".into()])
        .unwrap();
    db.add_tags(f2.path(), vec!["oldname".into()]).unwrap();
    rename_tag(db, "oldname", "newname", false, true, true).unwrap();
    let tags1 = db.get_tags(f1.path()).unwrap().unwrap();
    assert!(tags1.contains(&"newname".into()));
}

#[test]
fn test_merge_tags_basic() {
    let test_db = TestDb::new("test_merge_tags");
    let db = test_db.db();
    db.clear().unwrap();
    let f1 = TempFile::create("file1.txt").unwrap();
    let f2 = TempFile::create("file2.txt").unwrap();
    let f3 = TempFile::create("file3.txt").unwrap();
    db.add_tags(f1.path(), vec!["javascript".into(), "frontend".into()])
        .unwrap();
    db.add_tags(f2.path(), vec!["js".into(), "frontend".into()])
        .unwrap();
    db.add_tags(f3.path(), vec!["JS".into(), "backend".into()])
        .unwrap();
    merge_tags(
        db,
        &["javascript".into(), "JS".into()],
        "js",
        false,
        true,
        true,
    )
    .unwrap();
    let tags1 = db.get_tags(f1.path()).unwrap().unwrap();
    assert!(tags1.contains(&"js".into()));
}

#[test]
fn test_copy_tags_all() {
    let test_db = TestDb::new("test_copy_tags_all");
    let db = test_db.db();
    db.clear().unwrap();
    let source = TempFile::create("source.txt").unwrap();
    db.add_tags(
        source.path(),
        vec!["tag1".into(), "tag2".into(), "tag3".into()],
    )
    .unwrap();
    let t1 = TempFile::create("target1.txt").unwrap();
    let t2 = TempFile::create("target2.txt").unwrap();
    db.add_tags(t1.path(), vec!["initial".into()]).unwrap();
    db.add_tags(t2.path(), vec!["initial".into()]).unwrap();
    let params = SearchParams {
        query: None,
        tags: vec!["initial".into()],
        tag_mode: SearchMode::Any,
        file_patterns: vec![],
        file_mode: SearchMode::All,
        exclude_tags: vec![],
        regex_tag: false,
        regex_file: false,
        glob_files: false,
        virtual_tags: vec![],
        virtual_mode: SearchMode::All,
    };
    copy_tags(db, source.path(), &params, None, &[], false, true, true).unwrap();
    let tags1 = db.get_tags(t1.path()).unwrap().unwrap();
    assert!(tags1.contains(&"tag1".into()));
}

#[test]
fn test_bulk_map_tags_basic() {
    let test_db = TestDb::new("test_bulk_map_tags_basic");
    let db = test_db.db();
    db.clear().unwrap();
    let f = TempFile::create("file.txt").unwrap();
    db.add_tags(f.path(), vec!["old".into(), "keep".into()])
        .unwrap();
    let mapping_file = TempFile::create_with_content("map.txt", b"old new").unwrap();
    bulk_map_tags(
        db,
        mapping_file.path(),
        BatchFormat::PlainText,
        false,
        true,
        true,
    )
    .unwrap();
    let tags = db.get_tags(f.path()).unwrap().unwrap();
    assert!(tags.contains(&"new".into()));
    assert!(!tags.contains(&"old".into()));
}

#[test]
fn test_bulk_delete_files_basic() {
    let test_db = TestDb::new("test_bulk_delete_files_basic");
    let db = test_db.db();
    db.clear().unwrap();
    let f1 = TempFile::create("f1.txt").unwrap();
    let f2 = TempFile::create("f2.txt").unwrap();
    db.add_tags(f1.path(), vec!["t".into()]).unwrap();
    db.add_tags(f2.path(), vec!["t".into()]).unwrap();
    assert_eq!(db.count(), 2);
    let list = format!("{}\n{}", f1.path().display(), f2.path().display());
    let file_list = TempFile::create_with_content("delete.txt", list.as_bytes()).unwrap();
    bulk_delete_files(
        db,
        file_list.path(),
        BatchFormat::PlainText,
        false,
        true,
        true,
    )
    .unwrap();
    assert_eq!(db.count(), 0);
}
