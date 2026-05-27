//! Tests for the `store` module — `MockStore` and `DirectStore`.

#[cfg(test)]
mod mock_tests {
    use crate::schema::types::TagSchema;
    use crate::store::mock::MockStore;
    use crate::store::TagStore;
    use crate::types::{Pair, QueryCriteria, TagExpr, TagName, TagrPath};

    fn tag(s: &str) -> TagName {
        TagName::new(s).unwrap()
    }

    fn path(s: &str) -> TagrPath {
        TagrPath::new(s).unwrap()
    }

    fn sample_store() -> MockStore {
        MockStore::with_tags(&[
            ("src/main.rs", &["rust", "cli"]),
            ("src/lib.rs", &["rust", "library"]),
            ("README.md", &["docs"]),
            ("tests/test.rs", &["rust", "test"]),
        ])
    }

    #[test]
    fn empty_store() {
        let store = MockStore::new();
        assert!(store.list_all_tags().unwrap().is_empty());
        assert!(store.list_all_files().unwrap().is_empty());
        assert!(store.list_all().unwrap().is_empty());
    }

    #[test]
    fn with_tags_constructor() {
        let store = sample_store();
        let tags = store.list_all_tags().unwrap();
        assert_eq!(tags.len(), 5); // cli, docs, library, rust, test
    }

    #[test]
    fn list_all_tags_sorted() {
        let store = sample_store();
        let tags = store.list_all_tags().unwrap();
        let names: Vec<&str> = tags.iter().map(TagName::as_str).collect();
        assert_eq!(names, vec!["cli", "docs", "library", "rust", "test"]);
    }

    #[test]
    fn list_tags_with_counts() {
        let store = sample_store();
        let counts = store.list_tags_with_counts().unwrap();
        let rust_count = counts.iter().find(|(t, _)| t.as_str() == "rust").unwrap();
        assert_eq!(rust_count.1, 3);
        let docs_count = counts.iter().find(|(t, _)| t.as_str() == "docs").unwrap();
        assert_eq!(docs_count.1, 1);
    }

    #[test]
    fn list_all_files_sorted() {
        let store = sample_store();
        let files = store.list_all_files().unwrap();
        assert_eq!(files.len(), 4);
        // Sorted
        let is_sorted = files.windows(2).all(|w| w[0] <= w[1]);
        assert!(is_sorted);
    }

    #[test]
    fn list_all_pairs() {
        let store = sample_store();
        let pairs = store.list_all().unwrap();
        assert_eq!(pairs.len(), 4);
    }

    #[test]
    fn get_tags_existing_file() {
        let store = sample_store();
        let tags = store.get_tags(&path("src/main.rs")).unwrap().unwrap();
        let names: Vec<&str> = tags.iter().map(TagName::as_str).collect();
        assert!(names.contains(&"rust"));
        assert!(names.contains(&"cli"));
    }

    #[test]
    fn get_tags_missing_file() {
        let store = sample_store();
        assert!(store.get_tags(&path("nonexistent.rs")).unwrap().is_none());
    }

    #[test]
    fn find_by_tag() {
        let store = sample_store();
        let files = store.find_by_tag(&tag("rust")).unwrap();
        assert_eq!(files.len(), 3);
    }

    #[test]
    fn find_by_all_tags() {
        let store = sample_store();
        let files = store
            .find_by_all_tags(&[tag("rust"), tag("cli")])
            .unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].as_str(), "src/main.rs");
    }

    #[test]
    fn find_by_any_tag() {
        let store = sample_store();
        let files = store
            .find_by_any_tag(&[tag("cli"), tag("docs")])
            .unwrap();
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn find_by_tag_regex() {
        let store = sample_store();
        let files = store.find_by_tag_regex("^(cli|docs)$").unwrap();
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn tag_exists() {
        let store = sample_store();
        assert!(store.tag_exists(&tag("rust")).unwrap());
        assert!(!store.tag_exists(&tag("python")).unwrap());
    }

    #[test]
    fn find_tags_by_prefix() {
        let store = MockStore::with_tags(&[
            ("a.rs", &["language-rust", "language-python"]),
            ("b.rs", &["library"]),
        ]);
        let tags = store.find_tags_by_prefix(&tag("language")).unwrap();
        assert_eq!(tags.len(), 2);
        assert!(tags.iter().all(|t| t.as_str().starts_with("language")));
    }

    #[test]
    fn with_pairs_constructor() {
        let pairs = vec![
            Pair::new(path("a.rs"), vec![tag("rust")]),
            Pair::new(path("b.py"), vec![tag("python")]),
        ];
        let store = MockStore::with_pairs(pairs);
        assert_eq!(store.list_all_files().unwrap().len(), 2);
    }

    #[test]
    fn query_empty_criteria_returns_all() {
        let store = sample_store();
        let schema = TagSchema::new();
        let criteria = QueryCriteria::default();
        let files = store.query(&criteria, &schema).unwrap();
        assert_eq!(files.len(), 4);
    }

    #[test]
    fn query_single_tag() {
        let store = sample_store();
        let schema = TagSchema::new();
        let criteria = QueryCriteria {
            tag_expr: Some(TagExpr::Tag(tag("rust"))),
            ..Default::default()
        };
        let files = store.query(&criteria, &schema).unwrap();
        assert_eq!(files.len(), 3);
    }

    #[test]
    fn query_and_tags() {
        let store = sample_store();
        let schema = TagSchema::new();
        let criteria = QueryCriteria {
            tag_expr: Some(TagExpr::And(vec![
                TagExpr::Tag(tag("rust")),
                TagExpr::Tag(tag("cli")),
            ])),
            ..Default::default()
        };
        let files = store.query(&criteria, &schema).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].as_str(), "src/main.rs");
    }

    #[test]
    fn query_or_tags() {
        let store = sample_store();
        let schema = TagSchema::new();
        let criteria = QueryCriteria {
            tag_expr: Some(TagExpr::Or(vec![
                TagExpr::Tag(tag("cli")),
                TagExpr::Tag(tag("docs")),
            ])),
            ..Default::default()
        };
        let files = store.query(&criteria, &schema).unwrap();
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn query_not_tag() {
        let store = sample_store();
        let schema = TagSchema::new();
        let criteria = QueryCriteria {
            tag_expr: Some(TagExpr::And(vec![
                TagExpr::Tag(tag("rust")),
                TagExpr::Not(Box::new(TagExpr::Tag(tag("cli")))),
            ])),
            ..Default::default()
        };
        let files = store.query(&criteria, &schema).unwrap();
        // rust files minus cli: lib.rs and test.rs
        assert_eq!(files.len(), 2);
        assert!(files.iter().all(|f| f.as_str() != "src/main.rs"));
    }

    #[test]
    fn query_file_pattern() {
        let store = sample_store();
        let schema = TagSchema::new();
        let criteria = QueryCriteria {
            file_patterns: vec!["src/*".to_string()],
            ..Default::default()
        };
        let files = store.query(&criteria, &schema).unwrap();
        assert_eq!(files.len(), 2);
        assert!(files.iter().all(|f| f.as_str().starts_with("src/")));
    }

    #[test]
    fn store_error_file_not_found() {
        use crate::store::StoreError;
        let err = StoreError::FileNotFound(path("missing.txt"));
        assert!(matches!(err, StoreError::FileNotFound(_)));
        assert!(err.to_string().contains("missing.txt"));
    }

    #[test]
    fn store_error_connection_lost() {
        use crate::store::StoreError;
        let err = StoreError::ConnectionLost {
            context: "IPC reconnect failed".to_string(),
        };
        assert!(err.to_string().contains("unavailable"));
    }
}

#[cfg(test)]
mod direct_store_tests {
    use crate::schema::types::TagSchema;
    use crate::store::direct::DirectStore;
    use crate::store::TagStore;
    use crate::testing::{TempFile, TestDb};
    use crate::types::{NoteRecord, QueryCriteria, TagExpr, TagName, TagrPath};

    fn tag(s: &str) -> TagName {
        TagName::new(s).unwrap()
    }

    fn path_of(tf: &TempFile) -> TagrPath {
        TagrPath::new(tf.path()).unwrap()
    }

    fn setup_store(name: &str) -> (TestDb, DirectStore) {
        let test_db = TestDb::new(name);
        let db = test_db.db();
        db.clear().unwrap();
        let store = DirectStore::new(db.clone());
        (test_db, store)
    }

    #[test]
    fn insert_and_get_tags() {
        let (_db, store) = setup_store("direct_insert_get");
        let f = TempFile::create("insert_get.rs").unwrap();
        let p = path_of(&f);

        store.insert(&p, vec![tag("rust"), tag("cli")]).unwrap();

        let tags = store.get_tags(&p).unwrap().unwrap();
        let names: Vec<&str> = tags.iter().map(TagName::as_str).collect();
        assert!(names.contains(&"rust"));
        assert!(names.contains(&"cli"));
    }

    #[test]
    fn get_tags_missing_file() {
        let (_db, store) = setup_store("direct_get_missing");
        assert!(store
            .get_tags(&TagrPath::new("nonexistent").unwrap())
            .unwrap()
            .is_none());
    }

    #[test]
    fn list_all_tags() {
        let (_db, store) = setup_store("direct_list_tags");
        let f1 = TempFile::create("list_tags_a.rs").unwrap();
        let f2 = TempFile::create("list_tags_b.py").unwrap();

        store
            .insert(&path_of(&f1), vec![tag("rust"), tag("cli")])
            .unwrap();
        store
            .insert(&path_of(&f2), vec![tag("python")])
            .unwrap();

        let tags = store.list_all_tags().unwrap();
        assert_eq!(tags.len(), 3);
    }

    #[test]
    fn list_tags_with_counts() {
        let (_db, store) = setup_store("direct_tags_counts");
        let f1 = TempFile::create("counts_a.rs").unwrap();
        let f2 = TempFile::create("counts_b.rs").unwrap();

        store
            .insert(&path_of(&f1), vec![tag("rust"), tag("cli")])
            .unwrap();
        store
            .insert(&path_of(&f2), vec![tag("rust")])
            .unwrap();

        let counts = store.list_tags_with_counts().unwrap();
        let rust_count = counts.iter().find(|(t, _)| t.as_str() == "rust").unwrap();
        assert_eq!(rust_count.1, 2);
    }

    #[test]
    fn find_by_tag() {
        let (_db, store) = setup_store("direct_find_tag");
        let f1 = TempFile::create("find_a.rs").unwrap();
        let f2 = TempFile::create("find_b.rs").unwrap();
        let f3 = TempFile::create("find_c.py").unwrap();

        store.insert(&path_of(&f1), vec![tag("rust")]).unwrap();
        store
            .insert(&path_of(&f2), vec![tag("rust"), tag("cli")])
            .unwrap();
        store.insert(&path_of(&f3), vec![tag("python")]).unwrap();

        let files = store.find_by_tag(&tag("rust")).unwrap();
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn find_by_all_tags() {
        let (_db, store) = setup_store("direct_find_all");
        let f1 = TempFile::create("all_a.rs").unwrap();
        let f2 = TempFile::create("all_b.rs").unwrap();

        store
            .insert(&path_of(&f1), vec![tag("rust"), tag("cli")])
            .unwrap();
        store.insert(&path_of(&f2), vec![tag("rust")]).unwrap();

        let files = store
            .find_by_all_tags(&[tag("rust"), tag("cli")])
            .unwrap();
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn add_and_remove_tags() {
        let (_db, store) = setup_store("direct_add_remove");
        let f = TempFile::create("addrem.rs").unwrap();
        let p = path_of(&f);

        store.insert(&p, vec![tag("rust")]).unwrap();
        store.add_tags(&p, vec![tag("cli")]).unwrap();

        let tags = store.get_tags(&p).unwrap().unwrap();
        assert_eq!(tags.len(), 2);

        store.remove_tags(&p, &[tag("cli")]).unwrap();
        let tags = store.get_tags(&p).unwrap().unwrap();
        assert_eq!(tags.len(), 1);
    }

    #[test]
    fn remove_file() {
        let (_db, store) = setup_store("direct_remove_file");
        let f = TempFile::create("remove.rs").unwrap();
        let p = path_of(&f);

        store.insert(&p, vec![tag("rust")]).unwrap();
        assert!(store.remove_file(&p).unwrap());
        assert!(!store.remove_file(&p).unwrap());
    }

    #[test]
    fn remove_tag_globally() {
        let (_db, store) = setup_store("direct_remove_global");
        let f1 = TempFile::create("global_a.rs").unwrap();
        let f2 = TempFile::create("global_b.rs").unwrap();

        store
            .insert(&path_of(&f1), vec![tag("rust"), tag("cli")])
            .unwrap();
        store.insert(&path_of(&f2), vec![tag("rust")]).unwrap();

        store.remove_tag_globally(&tag("rust")).unwrap();
        assert!(!store.tag_exists(&tag("rust")).unwrap());
    }

    #[test]
    fn notes_crud() {
        let (_db, store) = setup_store("direct_notes");
        let f = TempFile::create("notes.rs").unwrap();
        let p = path_of(&f);

        store.insert(&p, vec![tag("rust")]).unwrap();
        assert!(store.get_note(&p).unwrap().is_none());

        let note = NoteRecord::new("test note".to_string());
        store.set_note(&p, &note).unwrap();

        let retrieved = store.get_note(&p).unwrap().unwrap();
        assert_eq!(retrieved.content, "test note");

        assert!(store.delete_note(&p).unwrap());
        assert!(store.get_note(&p).unwrap().is_none());
    }

    #[test]
    fn list_all_notes() {
        let (_db, store) = setup_store("direct_list_notes");
        let f1 = TempFile::create("notes_a.rs").unwrap();
        let f2 = TempFile::create("notes_b.rs").unwrap();

        store.insert(&path_of(&f1), vec![tag("rust")]).unwrap();
        store.insert(&path_of(&f2), vec![tag("python")]).unwrap();

        store
            .set_note(&path_of(&f1), &NoteRecord::new("note a".to_string()))
            .unwrap();
        store
            .set_note(&path_of(&f2), &NoteRecord::new("note b".to_string()))
            .unwrap();

        let notes = store.list_all_notes().unwrap();
        assert_eq!(notes.len(), 2);
    }

    #[test]
    fn find_tags_by_prefix() {
        let (_db, store) = setup_store("direct_prefix");
        let f1 = TempFile::create("prefix_a.rs").unwrap();
        let f2 = TempFile::create("prefix_b.rs").unwrap();

        store
            .insert(
                &path_of(&f1),
                vec![tag("language-rust"), tag("language-python")],
            )
            .unwrap();
        store
            .insert(&path_of(&f2), vec![tag("library")])
            .unwrap();

        let tags = store.find_tags_by_prefix(&tag("language")).unwrap();
        assert_eq!(tags.len(), 2);
        assert!(tags.iter().all(|t| t.as_str().starts_with("language")));
    }

    #[test]
    fn query_empty_criteria() {
        let (_db, store) = setup_store("direct_query_empty");
        let f1 = TempFile::create("qempty_a.rs").unwrap();
        let f2 = TempFile::create("qempty_b.py").unwrap();

        store.insert(&path_of(&f1), vec![tag("rust")]).unwrap();
        store.insert(&path_of(&f2), vec![tag("python")]).unwrap();

        let schema = TagSchema::new();
        let files = store.query(&QueryCriteria::default(), &schema).unwrap();
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn query_single_tag() {
        let (_db, store) = setup_store("direct_query_single");
        let f1 = TempFile::create("qsingle_a.rs").unwrap();
        let f2 = TempFile::create("qsingle_b.py").unwrap();

        store
            .insert(&path_of(&f1), vec![tag("rust"), tag("cli")])
            .unwrap();
        store.insert(&path_of(&f2), vec![tag("python")]).unwrap();

        let schema = TagSchema::new();
        let criteria = QueryCriteria {
            tag_expr: Some(TagExpr::Tag(tag("rust"))),
            ..Default::default()
        };
        let files = store.query(&criteria, &schema).unwrap();
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn query_with_not() {
        let (_db, store) = setup_store("direct_query_not");
        let f1 = TempFile::create("qnot_a.rs").unwrap();
        let f2 = TempFile::create("qnot_b.rs").unwrap();
        let p1 = path_of(&f1);
        let p2 = path_of(&f2);

        store
            .insert(&p1, vec![tag("rust"), tag("cli")])
            .unwrap();
        store
            .insert(&p2, vec![tag("rust"), tag("library")])
            .unwrap();

        let schema = TagSchema::new();
        let criteria = QueryCriteria {
            tag_expr: Some(TagExpr::And(vec![
                TagExpr::Tag(tag("rust")),
                TagExpr::Not(Box::new(TagExpr::Tag(tag("cli")))),
            ])),
            ..Default::default()
        };
        let files = store.query(&criteria, &schema).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0], p2);
    }
}
