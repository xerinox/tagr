use std::path::PathBuf;

use crate::cli::ConditionalArgs;
use crate::testing::{TempFile, TestDb};
use crate::types::{QueryCriteria, TagExpr, TagName};

use super::batch::{batch_from_file, parse_csv, parse_json, parse_plaintext};
use super::mapping::{parse_mapping_csv, parse_mapping_json, parse_mapping_text};
use super::propagate::{parse_dir_mapping, parse_ext_mapping};
use super::transform::TagTransformation;
use super::{
    BatchFormat, CopyTagsConfig, bulk_delete_files, bulk_map_tags, bulk_tag, bulk_untag, copy_tags,
    merge_tags, propagate_by_directory, propagate_by_extension, rename_tag, transform_tags,
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
    assert!(format!("{err}").contains("Invalid format"));
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
    assert!(format!("{err}").contains("Invalid CSV"));
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
    let msg = format!("{err}");
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
    let criteria = QueryCriteria {
        tag_expr: Some(TagExpr::Tag(TagName::new("initial").unwrap())),
        ..QueryCriteria::default()
    };
    bulk_tag(
        test_db.store(),
        &criteria,
        &["bulk".into(), "added".into()],
        &ConditionalArgs::default(),
        false,
        true,
        true,
        &mut std::io::sink(),
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
    let criteria = QueryCriteria {
        tag_expr: Some(TagExpr::Tag(TagName::new("tag1").unwrap())),
        ..QueryCriteria::default()
    };
    bulk_untag(
        test_db.store(),
        &criteria,
        &["tag1".into(), "tag2".into()],
        false,
        &ConditionalArgs::default(),
        false,
        true,
        true,
        &mut std::io::sink(),
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
    rename_tag(
        test_db.store(),
        "oldname",
        "newname",
        false,
        true,
        true,
        &mut std::io::sink(),
    )
    .unwrap();
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
        test_db.store(),
        &["javascript".into(), "JS".into()],
        "js",
        false,
        true,
        true,
        &mut std::io::sink(),
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
    let criteria = QueryCriteria {
        tag_expr: Some(TagExpr::Tag(TagName::new("initial").unwrap())),
        ..QueryCriteria::default()
    };
    copy_tags(
        test_db.store(),
        source.path(),
        &criteria,
        CopyTagsConfig {
            specific_tags: None,
            exclude_tags: &[],
            dry_run: false,
            yes: true,
            quiet: true,
        },
        &mut std::io::sink(),
    )
    .unwrap();
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
        test_db.store(),
        mapping_file.path(),
        BatchFormat::PlainText,
        false,
        true,
        true,
        &mut std::io::sink(),
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
        test_db.store(),
        file_list.path(),
        BatchFormat::PlainText,
        false,
        true,
        true,
        &mut std::io::sink(),
    )
    .unwrap();
    assert_eq!(db.count(), 0);
}

#[test]
fn test_bulk_tag_if_not_exists() {
    let test_db = TestDb::new("test_bulk_tag_if_not_exists");
    let db = test_db.db();
    db.clear().unwrap();
    let f1 = TempFile::create("file1.txt").unwrap();
    let f2 = TempFile::create("file2.txt").unwrap();
    db.add_tags(f1.path(), vec!["existing".into(), "old".into()])
        .unwrap();
    db.add_tags(f2.path(), vec!["old".into()]).unwrap();
    let criteria = QueryCriteria {
        tag_expr: Some(TagExpr::Tag(TagName::new("old").unwrap())),
        ..QueryCriteria::default()
    };
    let conditions = ConditionalArgs {
        if_not_exists: true,
        if_has_tag: vec![],
        if_missing_tag: vec![],
    };
    bulk_tag(
        test_db.store(),
        &criteria,
        &["existing".into(), "new".into()],
        &conditions,
        false,
        true,
        true,
        &mut std::io::sink(),
    )
    .unwrap();
    let tags1 = db.get_tags(f1.path()).unwrap().unwrap();
    assert!(
        !tags1.contains(&"new".into()),
        "f1 should not get 'new' since it has 'existing'"
    );
    assert!(tags1.contains(&"existing".into()));
    let tags2 = db.get_tags(f2.path()).unwrap().unwrap();
    assert!(tags2.contains(&"new".into()), "f2 should get 'new'");
    assert!(
        tags2.contains(&"existing".into()),
        "f2 should get 'existing'"
    );
}

#[test]
fn test_bulk_tag_if_has_tag() {
    let test_db = TestDb::new("test_bulk_tag_if_has_tag");
    let db = test_db.db();
    db.clear().unwrap();
    let f1 = TempFile::create("file1.txt").unwrap();
    let f2 = TempFile::create("file2.txt").unwrap();
    let f3 = TempFile::create("file3.txt").unwrap();
    db.add_tags(
        f1.path(),
        vec!["search".into(), "required1".into(), "required2".into()],
    )
    .unwrap();
    db.add_tags(f2.path(), vec!["search".into(), "required1".into()])
        .unwrap();
    db.add_tags(f3.path(), vec!["search".into()]).unwrap();
    let criteria = QueryCriteria {
        tag_expr: Some(TagExpr::Tag(TagName::new("search").unwrap())),
        ..QueryCriteria::default()
    };
    let conditions = ConditionalArgs {
        if_not_exists: false,
        if_has_tag: vec!["required1".into(), "required2".into()],
        if_missing_tag: vec![],
    };
    bulk_tag(
        test_db.store(),
        &criteria,
        &["conditional".into()],
        &conditions,
        false,
        true,
        true,
        &mut std::io::sink(),
    )
    .unwrap();
    let tags1 = db.get_tags(f1.path()).unwrap().unwrap();
    assert!(
        tags1.contains(&"conditional".into()),
        "f1 has all required tags"
    );
    let tags2 = db.get_tags(f2.path()).unwrap().unwrap();
    assert!(
        !tags2.contains(&"conditional".into()),
        "f2 missing required2"
    );
    let tags3 = db.get_tags(f3.path()).unwrap().unwrap();
    assert!(
        !tags3.contains(&"conditional".into()),
        "f3 missing both required tags"
    );
}

#[test]
fn test_bulk_tag_if_missing_tag() {
    let test_db = TestDb::new("test_bulk_tag_if_missing_tag");
    let db = test_db.db();
    db.clear().unwrap();
    let f1 = TempFile::create("file1.txt").unwrap();
    let f2 = TempFile::create("file2.txt").unwrap();
    let f3 = TempFile::create("file3.txt").unwrap();
    // f1 has both complete and wip (shouldn't get tagged - no tags missing)
    db.add_tags(
        f1.path(),
        vec!["search".into(), "complete".into(), "wip".into()],
    )
    .unwrap();
    // f2 has complete but missing wip (should get tagged - missing ANY)
    db.add_tags(f2.path(), vec!["search".into(), "complete".into()])
        .unwrap();
    // f3 has neither (should get tagged - missing ANY)
    db.add_tags(f3.path(), vec!["search".into()]).unwrap();
    let criteria = QueryCriteria {
        tag_expr: Some(TagExpr::Tag(TagName::new("search").unwrap())),
        ..QueryCriteria::default()
    };
    let conditions = ConditionalArgs {
        if_not_exists: false,
        if_has_tag: vec![],
        if_missing_tag: vec!["complete".into(), "wip".into()],
    };
    bulk_tag(
        test_db.store(),
        &criteria,
        &["needs-review".into()],
        &conditions,
        false,
        true,
        true,
        &mut std::io::sink(),
    )
    .unwrap();
    let tags1 = db.get_tags(f1.path()).unwrap().unwrap();
    assert!(
        !tags1.contains(&"needs-review".into()),
        "f1 has all tags (none missing)"
    );
    let tags2 = db.get_tags(f2.path()).unwrap().unwrap();
    assert!(tags2.contains(&"needs-review".into()), "f2 missing 'wip'");
    let tags3 = db.get_tags(f3.path()).unwrap().unwrap();
    assert!(
        tags3.contains(&"needs-review".into()),
        "f3 missing both tags"
    );
}

// ========== TagTransformation::apply tests ==========

#[test]
fn test_transform_apply_lowercase() {
    let t = TagTransformation::Lowercase;
    assert_eq!(t.apply("FooBar").unwrap(), "foobar");
}

#[test]
fn test_transform_apply_uppercase() {
    let t = TagTransformation::Uppercase;
    assert_eq!(t.apply("foobar").unwrap(), "FOOBAR");
}

#[test]
fn test_transform_apply_kebab_case() {
    let t = TagTransformation::KebabCase;
    assert_eq!(t.apply("FooBar").unwrap(), "foo-bar");
}

#[test]
fn test_transform_apply_snake_case() {
    let t = TagTransformation::SnakeCase;
    assert_eq!(t.apply("FooBar").unwrap(), "foo_bar");
}

#[test]
fn test_transform_apply_camel_case() {
    let t = TagTransformation::CamelCase;
    assert_eq!(t.apply("foo-bar").unwrap(), "fooBar");
}

#[test]
fn test_transform_apply_pascal_case() {
    let t = TagTransformation::PascalCase;
    assert_eq!(t.apply("foo-bar").unwrap(), "FooBar");
}

#[test]
fn test_transform_apply_add_prefix() {
    let t = TagTransformation::AddPrefix("pre-".into());
    assert_eq!(t.apply("tag").unwrap(), "pre-tag");
}

#[test]
fn test_transform_apply_add_suffix() {
    let t = TagTransformation::AddSuffix("-v2".into());
    assert_eq!(t.apply("tag").unwrap(), "tag-v2");
}

#[test]
fn test_transform_apply_remove_prefix_present() {
    let t = TagTransformation::RemovePrefix("old-".into());
    assert_eq!(t.apply("old-tag").unwrap(), "tag");
}

#[test]
fn test_transform_apply_remove_prefix_absent() {
    let t = TagTransformation::RemovePrefix("missing-".into());
    assert_eq!(t.apply("tag").unwrap(), "tag");
}

#[test]
fn test_transform_apply_remove_suffix_present() {
    let t = TagTransformation::RemoveSuffix("-old".into());
    assert_eq!(t.apply("tag-old").unwrap(), "tag");
}

#[test]
fn test_transform_apply_remove_suffix_absent() {
    let t = TagTransformation::RemoveSuffix("-missing".into());
    assert_eq!(t.apply("tag").unwrap(), "tag");
}

#[test]
fn test_transform_apply_regex_replace() {
    let t = TagTransformation::RegexReplace {
        pattern: r"^v(\d+)".into(),
        replacement: "version-$1".into(),
    };
    assert_eq!(t.apply("v42-beta").unwrap(), "version-42-beta");
}

#[test]
fn test_transform_apply_regex_invalid() {
    let t = TagTransformation::RegexReplace {
        pattern: r"[invalid".into(),
        replacement: "x".into(),
    };
    assert!(t.apply("tag").is_err());
}

// ========== transform_tags integration tests ==========

#[test]
fn test_transform_tags_empty_db() {
    let test_db = TestDb::new("test_transform_empty");
    let db = test_db.db();
    db.clear().unwrap();
    let mut out = Vec::new();
    transform_tags(
        test_db.store(),
        &TagTransformation::Lowercase,
        None,
        false,
        true,
        false,
        &mut out,
    )
    .unwrap();
    let output = String::from_utf8(out).unwrap();
    assert!(output.contains("No tags found"));
}

#[test]
fn test_transform_tags_identity_noop() {
    let test_db = TestDb::new("test_transform_identity");
    let db = test_db.db();
    db.clear().unwrap();
    let f = TempFile::create("f.txt").unwrap();
    db.add_tags(f.path(), vec!["lowercase".into()]).unwrap();
    let mut out = Vec::new();
    transform_tags(
        test_db.store(),
        &TagTransformation::Lowercase,
        None,
        false,
        true,
        false,
        &mut out,
    )
    .unwrap();
    let output = String::from_utf8(out).unwrap();
    assert!(output.contains("No transformations to apply"));
}

#[test]
fn test_transform_tags_lowercase_applied() {
    let test_db = TestDb::new("test_transform_lower");
    let db = test_db.db();
    db.clear().unwrap();
    let f = TempFile::create("f.txt").unwrap();
    db.add_tags(f.path(), vec!["MyTag".into(), "keep".into()])
        .unwrap();
    transform_tags(
        test_db.store(),
        &TagTransformation::Lowercase,
        None,
        false,
        true,
        true,
        &mut std::io::sink(),
    )
    .unwrap();
    let tags = db.get_tags(f.path()).unwrap().unwrap();
    assert!(tags.contains(&"mytag".into()));
    assert!(tags.contains(&"keep".into()));
    assert!(!tags.contains(&"MyTag".into()));
}

#[test]
fn test_transform_tags_filter_specific() {
    let test_db = TestDb::new("test_transform_filter");
    let db = test_db.db();
    db.clear().unwrap();
    let f = TempFile::create("f.txt").unwrap();
    db.add_tags(f.path(), vec!["UPPER".into(), "KEEP".into()])
        .unwrap();
    let filter = vec!["UPPER".to_string()];
    transform_tags(
        test_db.store(),
        &TagTransformation::Lowercase,
        Some(&filter),
        false,
        true,
        true,
        &mut std::io::sink(),
    )
    .unwrap();
    let tags = db.get_tags(f.path()).unwrap().unwrap();
    assert!(tags.contains(&"upper".into()), "UPPER should be lowercased");
    assert!(tags.contains(&"KEEP".into()), "KEEP should be untouched");
}

#[test]
fn test_transform_tags_dry_run() {
    let test_db = TestDb::new("test_transform_dry");
    let db = test_db.db();
    db.clear().unwrap();
    let f = TempFile::create("f.txt").unwrap();
    db.add_tags(f.path(), vec!["UPPER".into()]).unwrap();
    let mut out = Vec::new();
    transform_tags(
        test_db.store(),
        &TagTransformation::Lowercase,
        None,
        true,
        true,
        false,
        &mut out,
    )
    .unwrap();
    let output = String::from_utf8(out).unwrap();
    assert!(output.contains("Dry Run"));
    // Tag should NOT have changed
    let tags = db.get_tags(f.path()).unwrap().unwrap();
    assert!(tags.contains(&"UPPER".into()));
}

#[test]
fn test_transform_tags_collision_dedup() {
    let test_db = TestDb::new("test_transform_collision");
    let db = test_db.db();
    db.clear().unwrap();
    let f = TempFile::create("f.txt").unwrap();
    // Both FOO and Foo will lowercase to "foo"
    db.add_tags(f.path(), vec!["FOO".into(), "Foo".into()])
        .unwrap();
    let mut out = Vec::new();
    transform_tags(
        test_db.store(),
        &TagTransformation::Lowercase,
        None,
        false,
        true,
        false,
        &mut out,
    )
    .unwrap();
    let tags = db.get_tags(f.path()).unwrap().unwrap();
    assert!(tags.contains(&"foo".into()));
    let foo_count = tags.iter().filter(|t| *t == "foo").count();
    assert_eq!(foo_count, 1, "should be deduplicated");
}

// ========== parse_dir_mapping / parse_ext_mapping tests ==========

#[test]
fn test_parse_dir_mapping_valid() {
    let (dir, tag) = parse_dir_mapping("src:source-code").unwrap();
    assert_eq!(dir, "src");
    assert_eq!(tag, "source-code");
}

#[test]
fn test_parse_dir_mapping_invalid() {
    assert!(parse_dir_mapping("no-colon").is_err());
}

#[test]
fn test_parse_ext_mapping_valid() {
    let (ext, tags) = parse_ext_mapping("rs:rust,systems").unwrap();
    assert_eq!(ext, "rs");
    assert_eq!(tags, vec!["rust", "systems"]);
}

#[test]
fn test_parse_ext_mapping_single_tag() {
    let (ext, tags) = parse_ext_mapping("py:python").unwrap();
    assert_eq!(ext, "py");
    assert_eq!(tags, vec!["python"]);
}

#[test]
fn test_parse_ext_mapping_invalid() {
    assert!(parse_ext_mapping("no-colon").is_err());
}

// ========== propagate_by_directory tests ==========

#[test]
fn test_propagate_dir_empty_db() {
    let test_db = TestDb::new("test_prop_dir_empty");
    let db = test_db.db();
    db.clear().unwrap();
    let mut out = Vec::new();
    propagate_by_directory(
        test_db.store(),
        None,
        &[],
        false,
        false,
        true,
        false,
        &mut out,
    )
    .unwrap();
    let output = String::from_utf8(out).unwrap();
    assert!(output.contains("No files found"));
}

#[test]
fn test_propagate_dir_immediate_parent() {
    let test_db = TestDb::new("test_prop_dir_parent");
    let db = test_db.db();
    db.clear().unwrap();
    let f = TempFile::create("f.txt").unwrap();
    db.add_tags(f.path(), vec!["initial".into()]).unwrap();
    propagate_by_directory(
        test_db.store(),
        None,
        &[],
        false,
        false,
        true,
        true,
        &mut std::io::sink(),
    )
    .unwrap();
    let tags = db.get_tags(f.path()).unwrap().unwrap();
    // Should have a tag from the parent directory name
    assert!(tags.len() >= 2, "should have at least initial + dir tag");
}

#[test]
fn test_propagate_dir_custom_mapping() {
    let test_db = TestDb::new("test_prop_dir_custom");
    let db = test_db.db();
    db.clear().unwrap();
    let f = TempFile::create("f.txt").unwrap();
    // Get the parent dir name so we can create a mapping for it
    let parent_name = f
        .path()
        .parent()
        .unwrap()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let mapping = format!("{parent_name}:custom-tag");
    db.add_tags(f.path(), vec!["initial".into()]).unwrap();
    propagate_by_directory(
        test_db.store(),
        None,
        &[mapping],
        false,
        false,
        true,
        true,
        &mut std::io::sink(),
    )
    .unwrap();
    let tags = db.get_tags(f.path()).unwrap().unwrap();
    assert!(
        tags.contains(&"custom-tag".into()),
        "custom mapping should override dir name"
    );
}

#[test]
fn test_propagate_dir_dry_run() {
    let test_db = TestDb::new("test_prop_dir_dry");
    let db = test_db.db();
    db.clear().unwrap();
    let f = TempFile::create("f.txt").unwrap();
    db.add_tags(f.path(), vec!["initial".into()]).unwrap();
    let mut out = Vec::new();
    propagate_by_directory(
        test_db.store(),
        None,
        &[],
        false,
        true,
        true,
        false,
        &mut out,
    )
    .unwrap();
    let output = String::from_utf8(out).unwrap();
    assert!(output.contains("Dry Run"));
    // Tags should NOT have changed
    let tags = db.get_tags(f.path()).unwrap().unwrap();
    assert_eq!(tags, vec!["initial".to_string()]);
}

#[test]
fn test_propagate_dir_invalid_mapping() {
    let test_db = TestDb::new("test_prop_dir_badmap");
    let db = test_db.db();
    db.clear().unwrap();
    let err = propagate_by_directory(
        test_db.store(),
        None,
        &["no-colon".into()],
        false,
        false,
        true,
        true,
        &mut std::io::sink(),
    );
    assert!(err.is_err());
}

// ========== propagate_by_extension tests ==========

#[test]
fn test_propagate_ext_defaults() {
    let test_db = TestDb::new("test_prop_ext_defaults");
    let db = test_db.db();
    db.clear().unwrap();
    let f = TempFile::create("code.rs").unwrap();
    db.add_tags(f.path(), vec!["initial".into()]).unwrap();
    propagate_by_extension(
        test_db.store(),
        &[],
        false,
        false,
        true,
        true,
        &mut std::io::sink(),
    )
    .unwrap();
    let tags = db.get_tags(f.path()).unwrap().unwrap();
    assert!(
        tags.contains(&"rust".into()),
        "default .rs mapping should add 'rust'"
    );
}

#[test]
fn test_propagate_ext_custom_override() {
    let test_db = TestDb::new("test_prop_ext_custom");
    let db = test_db.db();
    db.clear().unwrap();
    let f = TempFile::create("code.rs").unwrap();
    db.add_tags(f.path(), vec!["initial".into()]).unwrap();
    propagate_by_extension(
        test_db.store(),
        &["rs:my-rust".into()],
        false,
        false,
        true,
        true,
        &mut std::io::sink(),
    )
    .unwrap();
    let tags = db.get_tags(f.path()).unwrap().unwrap();
    assert!(
        tags.contains(&"my-rust".into()),
        "custom mapping should override default"
    );
    assert!(!tags.contains(&"rust".into()), "default should be replaced");
}

#[test]
fn test_propagate_ext_no_defaults_no_custom_error() {
    let test_db = TestDb::new("test_prop_ext_nodef");
    let db = test_db.db();
    db.clear().unwrap();
    let err = propagate_by_extension(
        test_db.store(),
        &[],
        true,
        false,
        true,
        true,
        &mut std::io::sink(),
    );
    assert!(err.is_err(), "no_defaults with no custom should error");
}

#[test]
fn test_propagate_ext_unknown_extension() {
    let test_db = TestDb::new("test_prop_ext_unknown");
    let db = test_db.db();
    db.clear().unwrap();
    let f = TempFile::create("file.xyz123").unwrap();
    db.add_tags(f.path(), vec!["initial".into()]).unwrap();
    let mut out = Vec::new();
    propagate_by_extension(test_db.store(), &[], false, false, true, false, &mut out).unwrap();
    let output = String::from_utf8(out).unwrap();
    assert!(output.contains("No files match"));
}

#[test]
fn test_propagate_ext_dry_run() {
    let test_db = TestDb::new("test_prop_ext_dry");
    let db = test_db.db();
    db.clear().unwrap();
    let f = TempFile::create("script.py").unwrap();
    db.add_tags(f.path(), vec!["initial".into()]).unwrap();
    let mut out = Vec::new();
    propagate_by_extension(test_db.store(), &[], false, true, true, false, &mut out).unwrap();
    let output = String::from_utf8(out).unwrap();
    assert!(output.contains("Dry Run"));
    let tags = db.get_tags(f.path()).unwrap().unwrap();
    assert!(!tags.contains(&"python".into()), "dry run should not apply");
}

// ========== batch_from_file orchestration tests ==========

#[test]
fn test_batch_from_file_plaintext() {
    let test_db = TestDb::new("test_batch_orch_plain");
    let db = test_db.db();
    db.clear().unwrap();
    let f = TempFile::create("target.txt").unwrap();
    let content = format!("{} tag1 tag2", f.path().display());
    let input = TempFile::create_with_content("batch.txt", content.as_bytes()).unwrap();
    batch_from_file(
        test_db.store(),
        input.path(),
        BatchFormat::PlainText,
        false,
        true,
        true,
        &mut std::io::sink(),
    )
    .unwrap();
    let tags = db.get_tags(f.path()).unwrap().unwrap();
    assert!(tags.contains(&"tag1".into()));
    assert!(tags.contains(&"tag2".into()));
}

#[test]
fn test_batch_from_file_json() {
    let test_db = TestDb::new("test_batch_orch_json");
    let db = test_db.db();
    db.clear().unwrap();
    let f = TempFile::create("target.txt").unwrap();
    let content = format!(
        r#"[{{"file":"{}","tags":["j1","j2"]}}]"#,
        f.path().display()
    );
    let input = TempFile::create_with_content("batch.json", content.as_bytes()).unwrap();
    batch_from_file(
        test_db.store(),
        input.path(),
        BatchFormat::Json,
        false,
        true,
        true,
        &mut std::io::sink(),
    )
    .unwrap();
    let tags = db.get_tags(f.path()).unwrap().unwrap();
    assert!(tags.contains(&"j1".into()));
}

#[test]
fn test_batch_from_file_missing_input() {
    let test_db = TestDb::new("test_batch_orch_missing");
    let db = test_db.db();
    db.clear().unwrap();
    let err = batch_from_file(
        test_db.store(),
        std::path::Path::new("/nonexistent/file.txt"),
        BatchFormat::PlainText,
        false,
        true,
        true,
        &mut std::io::sink(),
    );
    assert!(err.is_err());
}

#[test]
fn test_batch_from_file_dry_run() {
    let test_db = TestDb::new("test_batch_orch_dry");
    let db = test_db.db();
    db.clear().unwrap();
    let f = TempFile::create("target.txt").unwrap();
    let content = format!("{} newtag", f.path().display());
    let input = TempFile::create_with_content("batch.txt", content.as_bytes()).unwrap();
    let mut out = Vec::new();
    batch_from_file(
        test_db.store(),
        input.path(),
        BatchFormat::PlainText,
        true,
        true,
        false,
        &mut out,
    )
    .unwrap();
    let output = String::from_utf8(out).unwrap();
    assert!(output.contains("Dry Run"));
    assert!(
        db.get_tags(f.path()).unwrap().is_none(),
        "dry run should not apply"
    );
}

#[test]
fn test_batch_from_file_empty_tags_skipped() {
    let test_db = TestDb::new("test_batch_orch_emptytags");
    let db = test_db.db();
    db.clear().unwrap();
    // JSON with empty tags array
    let content = r#"[{"file":"/some/file.txt","tags":[]}]"#;
    let input = TempFile::create_with_content("batch.json", content.as_bytes()).unwrap();
    let mut out = Vec::new();
    batch_from_file(
        test_db.store(),
        input.path(),
        BatchFormat::Json,
        false,
        true,
        false,
        &mut out,
    )
    .unwrap();
    let output = String::from_utf8(out).unwrap();
    assert!(output.contains("Skipped"));
}

// ========== mapping parser tests ==========

#[test]
fn test_parse_mapping_text_valid() {
    let content = "old new\n# comment\nfoo bar";
    let mappings = parse_mapping_text(content).unwrap();
    assert_eq!(mappings.len(), 2);
    assert_eq!(mappings[0].from, "old");
    assert_eq!(mappings[0].to, "new");
}

#[test]
fn test_parse_mapping_text_wrong_columns() {
    let content = "only-one-token";
    assert!(parse_mapping_text(content).is_err());
}

#[test]
fn test_parse_mapping_text_three_columns() {
    let content = "too many tokens";
    assert!(parse_mapping_text(content).is_err());
}

#[test]
fn test_parse_mapping_csv_valid() {
    let content = "old,new\nfoo,bar";
    let mappings = parse_mapping_csv(content, ',').unwrap();
    assert_eq!(mappings.len(), 2);
    assert_eq!(mappings[1].from, "foo");
    assert_eq!(mappings[1].to, "bar");
}

#[test]
fn test_parse_mapping_csv_empty_field() {
    let content = ",new";
    assert!(parse_mapping_csv(content, ',').is_err());
}

#[test]
fn test_parse_mapping_csv_json_content_error() {
    let content = r#"[{"from":"a","to":"b"}]"#;
    let err = parse_mapping_csv(content, ',').unwrap_err();
    assert!(format!("{err}").contains("JSON"));
}

#[test]
fn test_parse_mapping_json_valid() {
    let content = r#"[{"from":"old","to":"new"}]"#;
    let mappings = parse_mapping_json(content).unwrap();
    assert_eq!(mappings.len(), 1);
    assert_eq!(mappings[0].from, "old");
}

#[test]
fn test_parse_mapping_json_invalid() {
    let content = "not json at all";
    assert!(parse_mapping_json(content).is_err());
}

// ========== bulk_map_tags integration tests ==========

#[test]
fn test_bulk_map_tags_identical_skipped() {
    let test_db = TestDb::new("test_map_identical");
    let db = test_db.db();
    db.clear().unwrap();
    let f = TempFile::create("f.txt").unwrap();
    db.add_tags(f.path(), vec!["same".into()]).unwrap();
    let mapping = TempFile::create_with_content("map.txt", b"same same").unwrap();
    let mut out = Vec::new();
    bulk_map_tags(
        test_db.store(),
        mapping.path(),
        BatchFormat::PlainText,
        false,
        true,
        false,
        &mut out,
    )
    .unwrap();
    let output = String::from_utf8(out).unwrap();
    assert!(output.contains("Skipped"));
}

#[test]
fn test_bulk_map_tags_not_found_skipped() {
    let test_db = TestDb::new("test_map_notfound");
    let db = test_db.db();
    db.clear().unwrap();
    let f = TempFile::create("f.txt").unwrap();
    db.add_tags(f.path(), vec!["existing".into()]).unwrap();
    let mapping = TempFile::create_with_content("map.txt", b"nonexistent replacement").unwrap();
    let mut out = Vec::new();
    bulk_map_tags(
        test_db.store(),
        mapping.path(),
        BatchFormat::PlainText,
        false,
        true,
        false,
        &mut out,
    )
    .unwrap();
    let output = String::from_utf8(out).unwrap();
    assert!(output.contains("Skipped"));
}

#[test]
fn test_bulk_map_tags_target_exists_dedup() {
    let test_db = TestDb::new("test_map_dedup");
    let db = test_db.db();
    db.clear().unwrap();
    let f = TempFile::create("f.txt").unwrap();
    // File has both "old" and "new" — mapping old→new should just remove "old"
    db.add_tags(f.path(), vec!["old".into(), "new".into()])
        .unwrap();
    let mapping = TempFile::create_with_content("map.txt", b"old new").unwrap();
    bulk_map_tags(
        test_db.store(),
        mapping.path(),
        BatchFormat::PlainText,
        false,
        true,
        true,
        &mut std::io::sink(),
    )
    .unwrap();
    let tags = db.get_tags(f.path()).unwrap().unwrap();
    assert!(tags.contains(&"new".into()));
    assert!(!tags.contains(&"old".into()));
    let new_count = tags.iter().filter(|t| *t == "new").count();
    assert_eq!(new_count, 1, "no duplicates");
}

#[test]
fn test_bulk_map_tags_dry_run() {
    let test_db = TestDb::new("test_map_dry");
    let db = test_db.db();
    db.clear().unwrap();
    let f = TempFile::create("f.txt").unwrap();
    db.add_tags(f.path(), vec!["old".into()]).unwrap();
    let mapping = TempFile::create_with_content("map.txt", b"old new").unwrap();
    let mut out = Vec::new();
    bulk_map_tags(
        test_db.store(),
        mapping.path(),
        BatchFormat::PlainText,
        true,
        true,
        false,
        &mut out,
    )
    .unwrap();
    let output = String::from_utf8(out).unwrap();
    assert!(output.contains("Dry Run"));
    let tags = db.get_tags(f.path()).unwrap().unwrap();
    assert!(
        tags.contains(&"old".into()),
        "dry run should not change tags"
    );
}

// ========== rename_tag edge cases ==========

#[test]
fn test_rename_tag_same_name() {
    let test_db = TestDb::new("test_rename_same");
    let db = test_db.db();
    db.clear().unwrap();
    let f = TempFile::create("f.txt").unwrap();
    db.add_tags(f.path(), vec!["tag".into()]).unwrap();
    let err = rename_tag(
        test_db.store(),
        "tag",
        "tag",
        false,
        true,
        true,
        &mut std::io::sink(),
    );
    assert!(err.is_err(), "renaming to same name should error");
}

#[test]
fn test_rename_tag_not_found() {
    let test_db = TestDb::new("test_rename_notfound");
    let db = test_db.db();
    db.clear().unwrap();
    let mut out = Vec::new();
    rename_tag(
        test_db.store(),
        "nonexistent",
        "new",
        false,
        true,
        false,
        &mut out,
    )
    .unwrap();
    let output = String::from_utf8(out).unwrap();
    assert!(output.contains("not found") || output.contains("No files"));
}

// ========== merge_tags edge cases ==========

#[test]
fn test_merge_tags_target_in_sources() {
    let test_db = TestDb::new("test_merge_target_in_src");
    let db = test_db.db();
    db.clear().unwrap();
    let f = TempFile::create("f.txt").unwrap();
    db.add_tags(f.path(), vec!["a".into()]).unwrap();
    let err = merge_tags(
        test_db.store(),
        &["a".into(), "b".into()],
        "a",
        false,
        true,
        true,
        &mut std::io::sink(),
    );
    assert!(err.is_err(), "target in sources should error");
}

// ========== bulk_delete edge cases ==========

#[test]
fn test_bulk_delete_not_in_db() {
    let test_db = TestDb::new("test_delete_notindb");
    let db = test_db.db();
    db.clear().unwrap();
    let content = "/nonexistent/path.txt";
    let input = TempFile::create_with_content("del.txt", content.as_bytes()).unwrap();
    let mut out = Vec::new();
    bulk_delete_files(
        test_db.store(),
        input.path(),
        BatchFormat::PlainText,
        false,
        true,
        false,
        &mut out,
    )
    .unwrap();
    let output = String::from_utf8(out).unwrap();
    assert!(output.contains("Skipped"));
}

#[test]
fn test_bulk_delete_dry_run() {
    let test_db = TestDb::new("test_delete_dry");
    let db = test_db.db();
    db.clear().unwrap();
    let f = TempFile::create("f.txt").unwrap();
    db.add_tags(f.path(), vec!["tag".into()]).unwrap();
    let content = format!("{}", f.path().display());
    let input = TempFile::create_with_content("del.txt", content.as_bytes()).unwrap();
    let mut out = Vec::new();
    bulk_delete_files(
        test_db.store(),
        input.path(),
        BatchFormat::PlainText,
        true,
        true,
        false,
        &mut out,
    )
    .unwrap();
    let output = String::from_utf8(out).unwrap();
    assert!(output.contains("Dry Run"));
    assert_eq!(db.count(), 1, "dry run should not delete");
}
