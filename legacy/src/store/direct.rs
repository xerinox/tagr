//! `DirectStore` — sled-backed `TagStore` implementation.
//!
//! Thin wrapper around [`Database`](crate::db::Database) that implements
//! [`TagStore`](super::TagStore). Converts between the database's raw types
//! (`String`, `PathBuf`) and the newtype vocabulary (`TagName`, `TagrPath`)
//! at the boundary, and maps `DbError` to `StoreError`.
//!
//! This is the single-process backend: CLI and TUI use it when no daemon
//! is running. The daemon's central loop also uses it internally.

use crate::db::Database;
use crate::schema::types::TagSchema;
use crate::types::{NoteRecord, Pair, QueryCriteria, TagName, TagrPath};

use super::{Result, StoreError, TagStore};

/// Sled-backed storage — wraps [`Database`] behind the [`TagStore`] trait.
///
/// # Examples
///
/// ```no_run
/// use tagr::store::DirectStore;
/// use tagr::store::TagStore;
/// use tagr::types::TagName;
///
/// let store = DirectStore::open("my_db").unwrap();
/// let tags = store.list_all_tags().unwrap();
/// ```
pub struct DirectStore {
    db: Database,
}

impl DirectStore {
    /// Open or create a database at the given path.
    ///
    /// # Errors
    ///
    /// Returns `StoreError::IoFailed` if the database cannot be opened.
    /// Returns `StoreError::DatabaseLocked` if another process holds the lock.
    pub fn open<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
        let db = Database::open(&path).map_err(|e| map_db_open_error(e, &path))?;
        Ok(Self { db })
    }

    /// Wrap an existing `Database` handle.
    #[must_use]
    pub const fn new(db: Database) -> Self {
        Self { db }
    }
}

/// Map a `DbError` from `Database::open()` to `StoreError`.
fn map_db_open_error(err: crate::db::DbError, path: &impl AsRef<std::path::Path>) -> StoreError {
    let path_display = path.as_ref().display().to_string();
    match err {
        // sled returns a specific error when the DB is locked
        crate::db::DbError::SledError(ref e) if format!("{e}").contains("lock") => {
            StoreError::DatabaseLocked
        }
        crate::db::DbError::SledError(e) => StoreError::IoFailed {
            context: format!("opening database at {path_display}"),
            source: std::io::Error::other(e),
        },
        other => StoreError::IoFailed {
            context: format!("opening database at {path_display}: {other}"),
            source: std::io::Error::other(other.to_string()),
        },
    }
}

/// Map a `DbError` from a data operation to `StoreError`.
fn map_db_error(err: crate::db::DbError, context: &str) -> StoreError {
    match err {
        crate::db::DbError::PostcardError(e) => StoreError::StorageCorrupted {
            key: context.to_string(),
            reason: e.to_string(),
        },
        crate::db::DbError::SledError(e) => StoreError::IoFailed {
            context: context.to_string(),
            source: std::io::Error::other(e),
        },
        other => StoreError::IoFailed {
            context: format!("{context}: {other}"),
            source: std::io::Error::other(other.to_string()),
        },
    }
}

/// Convert a `PathBuf` to `TagrPath`, mapping invalid UTF-8 to `StorageCorrupted`.
fn path_to_tagrpath(path: &std::path::Path, context: &str) -> Result<TagrPath> {
    TagrPath::new(path.to_str().ok_or_else(|| StoreError::StorageCorrupted {
        key: context.to_string(),
        reason: format!("non-UTF-8 path in database: {}", path.display()),
    })?)
    .map_err(|e| StoreError::StorageCorrupted {
        key: context.to_string(),
        reason: e.to_string(),
    })
}

/// Convert a raw tag string to `TagName`, mapping invalid tags to `StorageCorrupted`.
fn string_to_tagname(s: &str, context: &str) -> Result<TagName> {
    TagName::new(s).map_err(|e| StoreError::StorageCorrupted {
        key: context.to_string(),
        reason: format!("invalid tag in database: '{s}': {e}"),
    })
}

impl TagStore for DirectStore {
    fn list_all_tags(&self) -> Result<Vec<TagName>> {
        self.db
            .list_all_tags()
            .map_err(|e| map_db_error(e, "listing all tags"))?
            .into_iter()
            .map(|s| string_to_tagname(&s, "list_all_tags"))
            .collect()
    }

    fn list_tags_with_counts(&self) -> Result<Vec<(TagName, usize)>> {
        self.db
            .list_tags_with_counts()
            .map_err(|e| map_db_error(e, "listing tags with counts"))?
            .into_iter()
            .map(|(s, count)| string_to_tagname(&s, "list_tags_with_counts").map(|t| (t, count)))
            .collect()
    }

    fn list_all_files(&self) -> Result<Vec<TagrPath>> {
        self.db
            .list_all_files()
            .map_err(|e| map_db_error(e, "listing all files"))?
            .into_iter()
            .map(|p| path_to_tagrpath(&p, "list_all_files"))
            .collect()
    }

    fn list_all(&self) -> Result<Vec<Pair>> {
        self.db
            .list_all()
            .map_err(|e| map_db_error(e, "listing all pairs"))
    }

    fn get_tags(&self, file: &TagrPath) -> Result<Option<Vec<TagName>>> {
        let raw = self
            .db
            .get_tags(file.as_str())
            .map_err(|e| map_db_error(e, &format!("getting tags for '{file}'")))?;

        match raw {
            None => Ok(None),
            Some(strings) => Ok(Some(
                strings
                    .iter()
                    .map(|s| string_to_tagname(s, &format!("get_tags('{file}')")))
                    .collect::<Result<Vec<_>>>()?,
            )),
        }
    }

    fn find_by_tag(&self, tag: &TagName) -> Result<Vec<TagrPath>> {
        self.db
            .find_by_tag(tag.as_str())
            .map_err(|e| map_db_error(e, &format!("finding files by tag '{tag}'")))?
            .into_iter()
            .map(|p| path_to_tagrpath(&p, &format!("find_by_tag('{tag}')")))
            .collect()
    }

    fn find_by_all_tags(&self, tags: &[TagName]) -> Result<Vec<TagrPath>> {
        let raw: Vec<String> = tags.iter().map(ToString::to_string).collect();
        self.db
            .find_by_all_tags(&raw)
            .map_err(|e| map_db_error(e, "finding files by all tags"))?
            .into_iter()
            .map(|p| path_to_tagrpath(&p, "find_by_all_tags"))
            .collect()
    }

    fn find_by_any_tag(&self, tags: &[TagName]) -> Result<Vec<TagrPath>> {
        let raw: Vec<String> = tags.iter().map(ToString::to_string).collect();
        self.db
            .find_by_any_tag(&raw)
            .map_err(|e| map_db_error(e, "finding files by any tag"))?
            .into_iter()
            .map(|p| path_to_tagrpath(&p, "find_by_any_tag"))
            .collect()
    }

    fn find_by_tag_regex(&self, pattern: &str) -> Result<Vec<TagrPath>> {
        self.db
            .find_by_tag_regex(pattern)
            .map_err(|e| map_db_error(e, &format!("finding files by tag regex '{pattern}'")))?
            .into_iter()
            .map(|p| path_to_tagrpath(&p, "find_by_tag_regex"))
            .collect()
    }

    fn tag_exists(&self, tag: &TagName) -> Result<bool> {
        self.db
            .tag_exists(tag.as_str())
            .map_err(|e| map_db_error(e, &format!("checking tag existence '{tag}'")))
    }

    fn find_tags_by_prefix(&self, prefix: &TagName) -> Result<Vec<TagName>> {
        self.db
            .find_tags_by_prefix(prefix.as_str())
            .map_err(|e| map_db_error(e, &format!("finding tags by prefix '{prefix}'")))?
            .into_iter()
            .map(|s| string_to_tagname(&s, &format!("find_tags_by_prefix('{prefix}')")))
            .collect()
    }

    fn insert(&self, file: &TagrPath, tags: Vec<TagName>) -> Result<()> {
        let raw_tags: Vec<String> = tags.into_iter().map(|t| t.to_string()).collect();
        self.db
            .insert(file.as_str(), raw_tags)
            .map_err(|e| map_db_error(e, &format!("inserting '{file}'")))
    }

    fn add_tags(&self, file: &TagrPath, tags: Vec<TagName>) -> Result<()> {
        let raw_tags: Vec<String> = tags.into_iter().map(|t| t.to_string()).collect();
        self.db
            .add_tags(file.as_str(), raw_tags)
            .map_err(|e| map_db_error(e, &format!("adding tags to '{file}'")))
    }

    fn remove_tags(&self, file: &TagrPath, tags: &[TagName]) -> Result<()> {
        let raw_tags: Vec<String> = tags.iter().map(ToString::to_string).collect();
        self.db
            .remove_tags(file.as_str(), &raw_tags)
            .map_err(|e| map_db_error(e, &format!("removing tags from '{file}'")))
    }

    fn remove_file(&self, file: &TagrPath) -> Result<bool> {
        self.db
            .remove(file.as_str())
            .map_err(|e| map_db_error(e, &format!("removing file '{file}'")))
    }

    fn remove_tag_globally(&self, tag: &TagName) -> Result<u64> {
        self.db
            .remove_tag_globally(tag.as_str())
            .map(|n| n as u64)
            .map_err(|e| map_db_error(e, &format!("removing tag '{tag}' globally")))
    }

    fn get_note(&self, file: &TagrPath) -> Result<Option<NoteRecord>> {
        self.db
            .get_note(file.as_str())
            .map_err(|e| map_db_error(e, &format!("getting note for '{file}'")))
    }

    fn set_note(&self, file: &TagrPath, note: &NoteRecord) -> Result<()> {
        self.db
            .set_note(file.as_str(), note)
            .map_err(|e| map_db_error(e, &format!("setting note for '{file}'")))
    }

    fn delete_note(&self, file: &TagrPath) -> Result<bool> {
        self.db
            .delete_note(file.as_str())
            .map_err(|e| map_db_error(e, &format!("deleting note for '{file}'")))
    }

    fn list_all_notes(&self) -> Result<Vec<(TagrPath, NoteRecord)>> {
        self.db
            .list_all_notes()
            .map_err(|e| map_db_error(e, "listing all notes"))?
            .into_iter()
            .map(|(p, n)| path_to_tagrpath(&p, "list_all_notes").map(|tp| (tp, n)))
            .collect()
    }

    fn search_notes(&self, query: &str) -> Result<Vec<(TagrPath, NoteRecord)>> {
        self.db
            .search_notes(query)
            .map_err(|e| map_db_error(e, "searching notes"))?
            .into_iter()
            .map(|(p, n)| path_to_tagrpath(&p, "search_notes").map(|tp| (tp, n)))
            .collect()
    }

    fn query(&self, criteria: &QueryCriteria, schema: &TagSchema) -> Result<Vec<TagrPath>> {
        crate::query::execute(self, criteria, schema)
    }
}
