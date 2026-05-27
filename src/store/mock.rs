//! `MockStore` — in-memory `TagStore` for tests.
//!
//! Uses `HashMap` internally — no sled, no filesystem, no I/O.
//! Provides convenience constructors for quickly setting up test fixtures.
//!
//! `query()` uses [`QueryCriteria::matches_pair()`] for local filtering,
//! the same function `DaemonStore` will use for its narrow-path cache filter.

use std::collections::HashMap;

use crate::schema::types::TagSchema;
use crate::types::{NoteRecord, Pair, QueryCriteria, TagName, TagrPath};

use super::{Result, StoreError, TagStore};

/// In-memory tag store for testing.
///
/// # Examples
///
/// ```
/// use tagr::store::mock::MockStore;
/// use tagr::store::TagStore;
///
/// let store = MockStore::with_tags(&[
///     ("src/main.rs", &["rust", "cli"]),
///     ("README.md", &["docs"]),
/// ]);
/// let tags = store.list_all_tags().unwrap();
/// assert_eq!(tags.len(), 3);
/// ```
pub struct MockStore {
    files: HashMap<TagrPath, Vec<TagName>>,
    notes: HashMap<TagrPath, NoteRecord>,
}

impl MockStore {
    /// Create an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
            notes: HashMap::new(),
        }
    }

    /// Create a store pre-populated from `Pair` values.
    #[must_use]
    pub fn with_pairs(pairs: Vec<Pair>) -> Self {
        let files = pairs
            .into_iter()
            .map(|p| (p.file, p.tags))
            .collect();
        Self {
            files,
            notes: HashMap::new(),
        }
    }

    /// Create a store from `(&str, &[&str])` tuples for quick test setup.
    ///
    /// # Panics
    ///
    /// Panics if any file path or tag name is invalid — intended for test code only.
    #[must_use]
    pub fn with_tags(entries: &[(&str, &[&str])]) -> Self {
        let files = entries
            .iter()
            .map(|(file, tags)| {
                let path = TagrPath::new(file)
                    .unwrap_or_else(|e| panic!("MockStore::with_tags: invalid path '{file}': {e}"));
                let tag_names: Vec<TagName> = tags
                    .iter()
                    .map(|t| {
                        TagName::new(*t).unwrap_or_else(|e| {
                            panic!("MockStore::with_tags: invalid tag '{t}': {e}")
                        })
                    })
                    .collect();
                (path, tag_names)
            })
            .collect();
        Self {
            files,
            notes: HashMap::new(),
        }
    }
}

impl Default for MockStore {
    fn default() -> Self {
        Self::new()
    }
}

impl TagStore for MockStore {
    fn list_all_tags(&self) -> Result<Vec<TagName>> {
        let mut tags: Vec<TagName> = self
            .files
            .values()
            .flat_map(|v| v.iter().cloned())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        tags.sort();
        Ok(tags)
    }

    fn list_tags_with_counts(&self) -> Result<Vec<(TagName, usize)>> {
        let mut counts: HashMap<&TagName, usize> = HashMap::new();
        for tags in self.files.values() {
            for tag in tags {
                *counts.entry(tag).or_insert(0) += 1;
            }
        }
        let mut result: Vec<(TagName, usize)> = counts
            .into_iter()
            .map(|(t, c)| (t.clone(), c))
            .collect();
        result.sort_by(|(a, _), (b, _)| a.cmp(b));
        Ok(result)
    }

    fn list_all_files(&self) -> Result<Vec<TagrPath>> {
        let mut files: Vec<TagrPath> = self.files.keys().cloned().collect();
        files.sort();
        Ok(files)
    }

    fn list_all(&self) -> Result<Vec<Pair>> {
        let mut pairs: Vec<Pair> = self
            .files
            .iter()
            .map(|(f, t)| Pair::new(f.clone(), t.clone()))
            .collect();
        pairs.sort_by(|a, b| a.file.cmp(&b.file));
        Ok(pairs)
    }

    fn get_tags(&self, file: &TagrPath) -> Result<Option<Vec<TagName>>> {
        Ok(self.files.get(file).cloned())
    }

    fn find_by_tag(&self, tag: &TagName) -> Result<Vec<TagrPath>> {
        let mut files: Vec<TagrPath> = self
            .files
            .iter()
            .filter(|(_, tags)| tags.contains(tag))
            .map(|(f, _)| f.clone())
            .collect();
        files.sort();
        Ok(files)
    }

    fn find_by_all_tags(&self, tags: &[TagName]) -> Result<Vec<TagrPath>> {
        let mut files: Vec<TagrPath> = self
            .files
            .iter()
            .filter(|(_, file_tags)| tags.iter().all(|t| file_tags.contains(t)))
            .map(|(f, _)| f.clone())
            .collect();
        files.sort();
        Ok(files)
    }

    fn find_by_any_tag(&self, tags: &[TagName]) -> Result<Vec<TagrPath>> {
        let mut files: Vec<TagrPath> = self
            .files
            .iter()
            .filter(|(_, file_tags)| tags.iter().any(|t| file_tags.contains(t)))
            .map(|(f, _)| f.clone())
            .collect();
        files.sort();
        Ok(files)
    }

    fn find_by_tag_regex(&self, pattern: &str) -> Result<Vec<TagrPath>> {
        let regex = regex::Regex::new(pattern).map_err(|e| StoreError::IoFailed {
            context: format!("invalid regex '{pattern}': {e}"),
            source: std::io::Error::new(std::io::ErrorKind::InvalidInput, e.to_string()),
        })?;

        let matching_tags: Vec<TagName> = self
            .list_all_tags()?
            .into_iter()
            .filter(|t| regex.is_match(t.as_str()))
            .collect();

        if matching_tags.is_empty() {
            return Ok(Vec::new());
        }

        self.find_by_any_tag(&matching_tags)
    }

    fn tag_exists(&self, tag: &TagName) -> Result<bool> {
        Ok(self.files.values().any(|tags| tags.contains(tag)))
    }

    fn find_tags_by_prefix(&self, prefix: &TagName) -> Result<Vec<TagName>> {
        let prefix_str = prefix.as_str();
        let mut tags: Vec<TagName> = self
            .list_all_tags()?
            .into_iter()
            .filter(|t| t.as_str().starts_with(prefix_str))
            .collect();
        tags.sort();
        Ok(tags)
    }

    fn insert(&self, _file: &TagrPath, _tags: Vec<TagName>) -> Result<()> {
        // MockStore is not mutable through &self — this is a design limitation
        // that mirrors the real trait. For test mutations, build state upfront
        // via constructors.
        Err(StoreError::IoFailed {
            context: "MockStore does not support mutations through &self".to_string(),
            source: std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "use constructors to set up test state",
            ),
        })
    }

    fn add_tags(&self, _file: &TagrPath, _tags: Vec<TagName>) -> Result<()> {
        Err(StoreError::IoFailed {
            context: "MockStore does not support mutations through &self".to_string(),
            source: std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "use constructors to set up test state",
            ),
        })
    }

    fn remove_tags(&self, _file: &TagrPath, _tags: &[TagName]) -> Result<()> {
        Err(StoreError::IoFailed {
            context: "MockStore does not support mutations through &self".to_string(),
            source: std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "use constructors to set up test state",
            ),
        })
    }

    fn remove_file(&self, _file: &TagrPath) -> Result<bool> {
        Err(StoreError::IoFailed {
            context: "MockStore does not support mutations through &self".to_string(),
            source: std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "use constructors to set up test state",
            ),
        })
    }

    fn remove_tag_globally(&self, _tag: &TagName) -> Result<u64> {
        Err(StoreError::IoFailed {
            context: "MockStore does not support mutations through &self".to_string(),
            source: std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "use constructors to set up test state",
            ),
        })
    }

    fn get_note(&self, file: &TagrPath) -> Result<Option<NoteRecord>> {
        Ok(self.notes.get(file).cloned())
    }

    fn set_note(&self, _file: &TagrPath, _note: &NoteRecord) -> Result<()> {
        Err(StoreError::IoFailed {
            context: "MockStore does not support mutations through &self".to_string(),
            source: std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "use constructors to set up test state",
            ),
        })
    }

    fn delete_note(&self, _file: &TagrPath) -> Result<bool> {
        Err(StoreError::IoFailed {
            context: "MockStore does not support mutations through &self".to_string(),
            source: std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "use constructors to set up test state",
            ),
        })
    }

    fn list_all_notes(&self) -> Result<Vec<(TagrPath, NoteRecord)>> {
        let mut notes: Vec<(TagrPath, NoteRecord)> = self
            .notes
            .iter()
            .map(|(f, n)| (f.clone(), n.clone()))
            .collect();
        notes.sort_by(|(a, _), (b, _)| a.cmp(b));
        Ok(notes)
    }

    fn query(&self, criteria: &QueryCriteria, _schema: &TagSchema) -> Result<Vec<TagrPath>> {
        if criteria.is_empty() {
            return self.list_all_files();
        }

        let pairs = self.list_all()?;
        let mut files: Vec<TagrPath> = pairs
            .into_iter()
            .filter(|p| criteria.matches_pair(p))
            .map(|p| p.file)
            .collect();
        files.sort();
        Ok(files)
    }
}
