//! Query logic for browse workflows
//!
//! This module contains business logic for retrieving and preparing data
//! for the browse interface. It bridges the data layer (Database) and the
//! domain layer (`TagrItem` models).
//!
//! Functions here return domain models (`TagrItem`) rather than raw database
//! types, making them suitable for direct use in browse workflows.

use crate::browse::models::{MetadataCache, TagWithDb, TagrItem};
use crate::cli::SearchParams;
use crate::db::query::search_params_to_criteria;
use crate::store::{StoreError, TagStore};
use crate::types::TagName;
use crate::query::hierarchy;
use std::collections::{HashMap, HashSet};

/// Query files that have notes but no tags (notes-only files)
///
/// Returns files as `TagrItem` instances. These are files tracked in the database
/// because they have notes, but have an empty tags list.
///
/// # Arguments
/// * `db` - Database to query
///
/// # Returns
/// Vector of `TagrItem` instances for files with notes but no tags
///
/// # Errors
/// Returns `DbError` if database operations fail
pub fn get_notes_only_files(ds: &dyn TagStore) -> Result<Vec<TagrItem>, StoreError> {
    let all_notes = ds.list_all_notes()?;

    #[allow(clippy::match_same_arms)]
    let items: Result<Vec<TagrItem>, StoreError> = all_notes
        .into_iter()
        .filter_map(|(path, _note)| {
            match ds.get_tags(&path) {
                Ok(Some(tags)) if tags.is_empty() => {
                    let mut cache = MetadataCache::new();
                    let cached = cache.get_or_insert(path.as_path());
                    Some(Ok(TagrItem::file(path, vec![], cached)))
                }
                Ok(Some(_)) => None,    // Has tags - exclude
                Ok(None) => None,       // Not in files tree - exclude
                Err(e) => Some(Err(e)), // Propagate error
            }
        })
        .collect();

    items
}

/// Query all available tags from the database with file counts
///
/// Returns tags as `TagrItem` instances with metadata including the number
/// of files associated with each tag. When a schema is available, this
/// function consolidates aliases into their canonical forms (e.g., `js` and
/// `javascript` are merged into a single `javascript` tag with combined file count).
///
/// # Arguments
/// * `db` - Database to query
///
/// # Returns
/// Vector of `TagrItem` instances representing tags, sorted alphabetically
///
/// # Errors
/// Returns `DbError` if database operations fail
///
/// # Examples
/// ```ignore
/// let tags = get_available_tags(&db)?;
/// for tag in tags {
///     println!("{} ({} files)", tag.name, tag.metadata.file_count());
/// }
/// ```
pub fn get_available_tags(ds: &dyn TagStore) -> Result<Vec<TagrItem>, StoreError> {
    let tag_names: Vec<String> = ds.list_all_tags()?.into_iter().map(|t| t.to_string()).collect();

    // Load schema to consolidate aliases
    let schema = crate::schema::load_default_schema().ok();

    if let Some(schema) = schema {
        // Group tags by canonical form and count UNIQUE files
        let mut canonical_map: HashMap<String, HashSet<String>> = HashMap::new();

        for tag_name in tag_names {
            let canonical = schema.canonicalize(&tag_name);
            let files = TagName::new(&tag_name)
                .ok()
                .map(|tn| ds.find_by_tag(&tn))
                .transpose()?
                .unwrap_or_default();

            let file_set = canonical_map.entry(canonical).or_default();
            for file_path in files {
                file_set.insert(file_path.as_str().to_string());
            }
        }

        let mut tags: Vec<TagrItem> = canonical_map
            .into_iter()
            .map(|(canonical, file_set)| TagrItem::tag(canonical, file_set.len()))
            .collect();

        tags.sort_by(|a, b| a.name.cmp(&b.name));

        if let Ok(notes_only_files) = get_notes_only_files(ds)
            && !notes_only_files.is_empty()
        {
            tags.push(TagrItem::tag(
                crate::browse::models::NOTES_ONLY_TAG.to_string(),
                notes_only_files.len(),
            ));
        }

        Ok(tags)
    } else {
        let mut tags: Result<Vec<TagrItem>, StoreError> = tag_names
            .into_iter()
            .map(|tag_name| TagrItem::try_from(TagWithDb { tag: tag_name, ds }))
            .collect();

        if let Ok(mut tag_vec) = tags {
            if let Ok(notes_only_files) = get_notes_only_files(ds)
                && !notes_only_files.is_empty()
            {
                tag_vec.push(TagrItem::tag(
                    crate::browse::models::NOTES_ONLY_TAG.to_string(),
                    notes_only_files.len(),
                ));
            }
            tags = Ok(tag_vec);
        }

        tags
    }
}

/// Query files matching the given search parameters
///
/// Applies search criteria including tag matching (any/all), file patterns,
/// exclusions, and virtual tags. Returns files as `TagrItem` instances with
/// full metadata.
///
/// # Arguments
/// * `db` - Database to query
/// * `params` - Search parameters specifying filters
///
/// # Returns
/// Vector of `TagrItem` instances representing files, with tags and metadata
///
/// # Errors
/// Returns `DbError` if database operations or pattern matching fails
///
/// # Examples
/// ```ignore
/// let params = SearchParams {
///     tags: vec!["rust".to_string()],
///     tag_mode: SearchMode::Any,
///     ..Default::default()
/// };
/// let files = get_matching_files(&db, &params)?;
/// ```
pub fn get_matching_files(ds: &dyn TagStore, params: &SearchParams) -> Result<Vec<TagrItem>, StoreError> {
    let criteria = search_params_to_criteria(params);
    let schema = crate::schema::load_default_schema().unwrap_or_default();
    let result_paths = ds.query(&criteria, &schema)?;

    let items: Result<Vec<TagrItem>, StoreError> = result_paths
        .into_iter()
        .map(|tagrpath| {
            let tags = ds.get_tags(&tagrpath)?.unwrap_or_default();
            let mut cache = MetadataCache::new();
            let cached = cache.get_or_insert(tagrpath.as_path());
            Ok(TagrItem::file(tagrpath, tags, cached))
        })
        .collect();

    items
}

/// Query files for specific tags with a given search mode
///
/// Convenience function that builds `SearchParams` from tags and mode,
/// then queries matching files.
///
/// # Arguments
/// * `db` - Database to query
/// * `tags` - Tags to search for
/// * `mode` - Search mode (Any = OR, All = AND)
///
/// # Returns
/// Vector of `TagrItem` instances for matching files
///
/// # Errors
/// Returns `DbError` if database operations fail
pub fn get_files_by_tags(
    ds: &dyn TagStore,
    tags: &[String],
    mode: crate::browse::models::SearchMode,
) -> Result<Vec<TagrItem>, StoreError> {
    let params = SearchParams {
        query: None,
        tags: tags.to_vec(),
        tag_mode: mode.into(),
        file_patterns: vec![],
        file_mode: crate::cli::SearchMode::All,
        exclude_tags: vec![],
        regex_tag: false,
        regex_file: false,
        glob_files: false,
        virtual_tags: vec![],
        virtual_mode: crate::cli::SearchMode::All,
        no_hierarchy: false,
    };

    get_matching_files(ds, &params)
}

/// Filter an existing collection of items in-memory using search parameters
///
/// Uses hierarchy-aware tag matching for include/exclude criteria.
/// Useful for live filtering in the TUI as users type or adjust search criteria.
#[must_use]
pub fn filter_items_in_memory<'a>(
    items: &'a [TagrItem],
    params: &'a SearchParams,
) -> Vec<&'a TagrItem> {
    items
        .iter()
        .filter(|item| {
            let tags: &[TagName] = match &item.metadata {
                crate::browse::models::ItemMetadata::File(fm) => &fm.tags,
                crate::browse::models::ItemMetadata::Tag(_) => return true,
            };

            if params.tags.is_empty() && params.exclude_tags.is_empty() {
                return true;
            }

            if params.no_hierarchy {
                // Exact matching
                if !params.tags.is_empty() {
                    let has_match = match params.tag_mode {
                        crate::cli::SearchMode::All => {
                            params.tags.iter().all(|t| tags.iter().any(|tag| tag.as_str() == t))
                        }
                        crate::cli::SearchMode::Any => {
                            params.tags.iter().any(|t| tags.iter().any(|tag| tag.as_str() == t))
                        }
                    };
                    if !has_match {
                        return false;
                    }
                }
                if params.exclude_tags.iter().any(|t| tags.iter().any(|tag| tag.as_str() == t)) {
                    return false;
                }
            } else {
                // Hierarchy-aware matching
                if !params.tags.is_empty() {
                    let matches = match params.tag_mode {
                        crate::cli::SearchMode::All => params.tags.iter().all(|pattern| {
                            tags.iter()
                                .any(|tag| hierarchy::pattern_matches(pattern, tag.as_str()))
                        }),
                        crate::cli::SearchMode::Any => params.tags.iter().any(|pattern| {
                            tags.iter()
                                .any(|tag| hierarchy::pattern_matches(pattern, tag.as_str()))
                        }),
                    };
                    if !matches {
                        return false;
                    }
                }

                if !hierarchy::should_include_file(tags, &params.tags, &params.exclude_tags) {
                    return false;
                }
            }

            true
        })
        .collect()
}

impl From<crate::browse::models::SearchMode> for crate::cli::SearchMode {
    fn from(mode: crate::browse::models::SearchMode) -> Self {
        match mode {
            crate::browse::models::SearchMode::Any => Self::Any,
            crate::browse::models::SearchMode::All => Self::All,
        }
    }
}

impl From<crate::cli::SearchMode> for crate::browse::models::SearchMode {
    fn from(mode: crate::cli::SearchMode) -> Self {
        match mode {
            crate::cli::SearchMode::Any => Self::Any,
            crate::cli::SearchMode::All => Self::All,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Pair;
    use crate::browse::models::SearchMode;
    use crate::cli::SearchParams;
    use crate::store::DirectStore;
    use crate::testing::{TempFile, TestDb};

    fn ds(db: &TestDb) -> DirectStore {
        DirectStore::new(db.db().clone())
    }

    #[test]
    fn test_get_available_tags() {
        let test_db = TestDb::new("test_get_available_tags");
        let db = test_db.db();
        db.clear().unwrap();

        let file1 = TempFile::create("file1.txt").unwrap();
        let file2 = TempFile::create("file2.txt").unwrap();
        let file3 = TempFile::create("file3.txt").unwrap();

        let pair1 = Pair::new(
            file1.path().to_path_buf(),
            vec!["rust".into(), "code".into()],
        );
        let pair2 = Pair::new(
            file2.path().to_path_buf(),
            vec!["rust".into(), "docs".into()],
        );
        let pair3 = Pair::new(
            file3.path().to_path_buf(),
            vec!["python".into(), "script".into()],
        );

        db.insert_pair(&pair1).unwrap();
        db.insert_pair(&pair2).unwrap();
        db.insert_pair(&pair3).unwrap();

        let source = ds(&test_db);
        let tags = get_available_tags(&source).unwrap();

        assert_eq!(tags.len(), 5);

        let rust_tag = tags.iter().find(|t| t.name == "rust").unwrap();
        if let crate::browse::models::ItemMetadata::Tag(crate::browse::models::TagMetadata {
            file_count,
        }) = rust_tag.metadata
        {
            assert_eq!(file_count, 2);
        } else {
            panic!("Expected Tag metadata");
        }

        let python_tag = tags.iter().find(|t| t.name == "python").unwrap();
        if let crate::browse::models::ItemMetadata::Tag(crate::browse::models::TagMetadata {
            file_count,
        }) = python_tag.metadata
        {
            assert_eq!(file_count, 1);
        } else {
            panic!("Expected Tag metadata");
        }
    }

    #[test]
    fn test_get_available_tags_empty_db() {
        let test_db = TestDb::new("test_get_tags_empty");
        let db = test_db.db();
        db.clear().unwrap();

        let source = ds(&test_db);
        let tags = get_available_tags(&source).unwrap();
        assert_eq!(tags.len(), 0);
    }

    #[test]
    fn test_get_matching_files_by_tag() {
        let test_db = TestDb::new("test_get_matching_files");
        let db = test_db.db();
        db.clear().unwrap();

        let file1 = TempFile::create("file1.txt").unwrap();
        let file2 = TempFile::create("file2.txt").unwrap();
        let file3 = TempFile::create("file3.txt").unwrap();

        let pair1 = Pair::new(file1.path().to_path_buf(), vec!["rust".into()]);
        let pair2 = Pair::new(
            file2.path().to_path_buf(),
            vec!["rust".into(), "docs".into()],
        );
        let pair3 = Pair::new(file3.path().to_path_buf(), vec!["python".into()]);

        db.insert_pair(&pair1).unwrap();
        db.insert_pair(&pair2).unwrap();
        db.insert_pair(&pair3).unwrap();

        let params = SearchParams {
            query: None,
            tags: vec!["rust".to_string()],
            tag_mode: crate::cli::SearchMode::Any,
            file_patterns: vec![],
            file_mode: crate::cli::SearchMode::All,
            exclude_tags: vec![],
            regex_tag: false,
            regex_file: false,
            glob_files: false,
            virtual_tags: vec![],
            virtual_mode: crate::cli::SearchMode::All,
            no_hierarchy: false,
        };

        let source = ds(&test_db);
        let files = get_matching_files(&source, &params).unwrap();
        assert_eq!(files.len(), 2);

        for item in &files {
            if let crate::browse::models::ItemMetadata::File(ref file_meta) = item.metadata {
                assert!(file_meta.tags.iter().any(|t| t.as_str() == "rust"));
                assert!(file_meta.cached.exists);
            } else {
                panic!("Expected File metadata");
            }
        }
    }

    #[test]
    fn test_get_files_by_tags_any_mode() {
        let test_db = TestDb::new("test_files_by_tags_any");
        let db = test_db.db();
        db.clear().unwrap();

        let file1 = TempFile::create("file1.txt").unwrap();
        let file2 = TempFile::create("file2.txt").unwrap();
        let file3 = TempFile::create("file3.txt").unwrap();

        db.insert_pair(&Pair::new(file1.path().to_path_buf(), vec!["rust".into()]))
            .unwrap();
        db.insert_pair(&Pair::new(
            file2.path().to_path_buf(),
            vec!["python".into()],
        ))
        .unwrap();
        db.insert_pair(&Pair::new(file3.path().to_path_buf(), vec!["go".into()]))
            .unwrap();

        let source = ds(&test_db);
        let files =
            get_files_by_tags(&source, &["rust".into(), "python".into()], SearchMode::Any).unwrap();
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_get_files_by_tags_all_mode() {
        let test_db = TestDb::new("test_files_by_tags_all");
        let db = test_db.db();
        db.clear().unwrap();

        let file1 = TempFile::create("file1.txt").unwrap();
        let file2 = TempFile::create("file2.txt").unwrap();
        let file3 = TempFile::create("file3.txt").unwrap();

        db.insert_pair(&Pair::new(
            file1.path().to_path_buf(),
            vec!["rust".into(), "web".into()],
        ))
        .unwrap();
        db.insert_pair(&Pair::new(file2.path().to_path_buf(), vec!["rust".into()]))
            .unwrap();
        db.insert_pair(&Pair::new(file3.path().to_path_buf(), vec!["web".into()]))
            .unwrap();

        let source = ds(&test_db);
        let files = get_files_by_tags(&source, &["rust".into(), "web".into()], SearchMode::All).unwrap();
        assert_eq!(files.len(), 1);

        let item = &files[0];
        if let crate::browse::models::ItemMetadata::File(ref file_meta) = item.metadata {
            assert!(file_meta.tags.iter().any(|t| t.as_str() == "rust"));
            assert!(file_meta.tags.iter().any(|t| t.as_str() == "web"));
        } else {
            panic!("Expected File metadata");
        }
    }

    #[test]
    fn test_search_mode_conversion() {
        let cli_any: crate::cli::SearchMode = SearchMode::Any.into();
        assert!(matches!(cli_any, crate::cli::SearchMode::Any));

        let cli_all: crate::cli::SearchMode = SearchMode::All.into();
        assert!(matches!(cli_all, crate::cli::SearchMode::All));

        let browse_any: SearchMode = crate::cli::SearchMode::Any.into();
        assert!(matches!(browse_any, SearchMode::Any));

        let browse_all: SearchMode = crate::cli::SearchMode::All.into();
        assert!(matches!(browse_all, SearchMode::All));
    }

    #[test]
    fn test_get_matching_files_no_results() {
        let test_db = TestDb::new("test_matching_no_results");
        let db = test_db.db();
        db.clear().unwrap();

        let file1 = TempFile::create("file1.txt").unwrap();
        db.insert_pair(&Pair::new(
            file1.path().to_path_buf(),
            vec!["python".into()],
        ))
        .unwrap();

        let params = SearchParams {
            query: None,
            tags: vec!["rust".to_string()],
            tag_mode: crate::cli::SearchMode::Any,
            file_patterns: vec![],
            file_mode: crate::cli::SearchMode::All,
            exclude_tags: vec![],
            regex_tag: false,
            regex_file: false,
            glob_files: false,
            virtual_tags: vec![],
            virtual_mode: crate::cli::SearchMode::All,
            no_hierarchy: false,
        };

        let source = ds(&test_db);
        let files = get_matching_files(&source, &params).unwrap();
        assert_eq!(files.len(), 0);
    }

    #[test]
    fn test_get_notes_only_files() {
        use crate::db::NoteMeta;
        use crate::db::NoteRecord;

        let test_db = TestDb::new("test_notes_only");
        let db = test_db.db();
        db.clear().unwrap();

        let file1 = TempFile::create("file1.txt").unwrap();
        let file2 = TempFile::create("file2.txt").unwrap();
        let file3 = TempFile::create("file3.txt").unwrap();

        // File with tags and note - should be excluded
        db.insert_pair(&Pair::new(file1.path().to_path_buf(), vec!["rust".into()]))
            .unwrap();
        db.set_note(
            file1.path(),
            &NoteRecord {
                content: "Note 1".into(),
                metadata: NoteMeta {
                    created_at: 0,
                    updated_at: 0,
                },
            },
        )
        .unwrap();

        // File with note but no tags - should be included
        db.set_note(
            file2.path(),
            &NoteRecord {
                content: "Note 2".into(),
                metadata: NoteMeta {
                    created_at: 0,
                    updated_at: 0,
                },
            },
        )
        .unwrap();

        // File with tags but no note - should be excluded
        db.insert_pair(&Pair::new(
            file3.path().to_path_buf(),
            vec!["python".into()],
        ))
        .unwrap();

        let source = ds(&test_db);
        let notes_only = get_notes_only_files(&source).unwrap();

        // Only file2 should be included
        assert_eq!(notes_only.len(), 1);
        let file = &notes_only[0];
        assert_eq!(file.name, "file2.txt");
        assert_eq!(file.file_tags(), Some(&[][..]));
    }
}
