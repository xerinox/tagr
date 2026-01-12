//! Query logic for browse workflows
//!
//! This module contains business logic for retrieving and preparing data
//! for the browse interface. It bridges the data layer (Database) and the
//! domain layer (`TagrItem` models).
//!
//! Functions here return domain models (`TagrItem`) rather than raw database
//! types, making them suitable for direct use in browse workflows.

use crate::browse::models::{PairWithCache, TagWithDb, TagrItem};
use crate::cli::SearchParams;
use crate::db::{Database, DbError};
use crate::search::FilterExt; // Import trait for in-memory filtering
use std::collections::{HashMap, HashSet};

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
pub fn get_available_tags(db: &Database) -> Result<Vec<TagrItem>, DbError> {
    let tag_names = db.list_all_tags()?;

    // Load schema to consolidate aliases
    let schema = crate::schema::load_default_schema().ok();

    if let Some(schema) = schema {
        // Group tags by canonical form and count UNIQUE files
        let mut canonical_map: HashMap<String, HashSet<String>> = HashMap::new();

        for tag_name in tag_names {
            let canonical = schema.canonicalize(&tag_name);
            let files = db.find_by_tag(&tag_name)?;

            // Add unique file paths to the canonical tag's set
            let file_set = canonical_map.entry(canonical).or_default();
            for file_path in files {
                if let Some(path_str) = file_path.to_str() {
                    file_set.insert(path_str.to_string());
                }
            }
        }

        // Convert to TagrItem instances with unique file counts
        let mut tags: Vec<TagrItem> = canonical_map
            .into_iter()
            .map(|(canonical, file_set)| TagrItem::tag(canonical, file_set.len()))
            .collect();

        tags.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(tags)
    } else {
        // No schema - use original behavior
        let tags: Result<Vec<TagrItem>, DbError> = tag_names
            .into_iter()
            .map(|tag_name| TagrItem::try_from(TagWithDb { tag: tag_name, db }))
            .collect();

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
pub fn get_matching_files(db: &Database, params: &SearchParams) -> Result<Vec<TagrItem>, DbError> {
    let file_paths = crate::db::query::apply_search_params(db, params)?;

    let items: Result<Vec<TagrItem>, DbError> = file_paths
        .into_iter()
        .map(|path| {
            let tags = db.get_tags(&path)?.unwrap_or_default();
            let pair = crate::Pair { file: path, tags };

            let mut cache = crate::browse::models::MetadataCache::new();
            Ok(TagrItem::from(PairWithCache {
                pair,
                cache: &mut cache,
            }))
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
    db: &Database,
    tags: &[String],
    mode: crate::browse::models::SearchMode,
) -> Result<Vec<TagrItem>, DbError> {
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

    get_matching_files(db, &params)
}

/// Filter an existing collection of items in-memory using search parameters
///
/// This function provides fast in-memory filtering without requiring database queries.
/// Useful for live filtering in the TUI as users type or adjust search criteria.
///
/// # Arguments
/// * `items` - Collection of `TagrItem` to filter
/// * `params` - Search parameters containing tag filters
///
/// # Returns
/// Vector of references to items that match the search criteria
///
/// # Examples
/// ```ignore
/// let filtered: Vec<_> = filter_items_in_memory(&all_items, &params);
/// ```
pub fn filter_items_in_memory<'a>(
    items: &'a [TagrItem],
    params: &'a SearchParams,
) -> Vec<&'a TagrItem> {
    items.apply_filter(params).collect()
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
    use crate::testing::{TempFile, TestDb};

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

        let tags = get_available_tags(db).unwrap();

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

        let tags = get_available_tags(db).unwrap();
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

        let files = get_matching_files(db, &params).unwrap();
        assert_eq!(files.len(), 2);

        for item in &files {
            if let crate::browse::models::ItemMetadata::File(ref file_meta) = item.metadata {
                assert!(file_meta.tags.contains(&"rust".to_string()));
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

        let files =
            get_files_by_tags(db, &["rust".into(), "python".into()], SearchMode::Any).unwrap();
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

        let files = get_files_by_tags(db, &["rust".into(), "web".into()], SearchMode::All).unwrap();
        assert_eq!(files.len(), 1);

        let item = &files[0];
        if let crate::browse::models::ItemMetadata::File(ref file_meta) = item.metadata {
            assert!(file_meta.tags.contains(&"rust".to_string()));
            assert!(file_meta.tags.contains(&"web".to_string()));
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

        let files = get_matching_files(db, &params).unwrap();
        assert_eq!(files.len(), 0);
    }
}
