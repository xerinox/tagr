//! Database wrapper module for tagr
//!
//! Provides a clean API for storing and retrieving file-tag pairings
//! using sled as the embedded database backend.
//!
//! Uses multiple sled trees for efficient indexing:
//! - `files`: Main tree mapping file paths to tags
//! - `tags`: Reverse index mapping tags to file paths

use crate::Pair;
use bincode;
use regex::Regex;
use sled::{Db, Tree};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub mod error;
pub mod query;
pub mod types;

pub use error::DbError;
pub use types::{NoteMeta, NoteRecord, PathKey, PathString};

/// Database wrapper that encapsulates all database operations
///
/// Uses multiple trees for efficient operations:
/// - `files` tree: `file_path` -> `Vec<tag>`
/// - `tags` tree: tag -> `Vec<file_path>` (reverse index)
/// - `notes` tree: `file_path` -> `NoteRecord`
///
/// Clone is cheap - both `Db` and `Tree` are reference-counted internally.
#[derive(Debug, Clone)]
pub struct Database {
    db: Db,
    files: Tree,
    tags: Tree,
    notes: Tree,
}

impl Database {
    /// Opens or creates a database at the specified path
    ///
    /// # Arguments
    /// * `path` - Path to the database directory
    ///
    /// # Examples
    /// ```no_run
    /// use tagr::db::Database;
    /// let db = Database::open("my_db").unwrap();
    /// ```
    ///
    /// # Errors
    ///
    /// Returns `DbError` if the database cannot be opened or if the internal trees cannot be created.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, DbError> {
        let db = sled::open(path)?;
        let files = db.open_tree("files")?;
        let tags = db.open_tree("tags")?;
        let notes = db.open_tree("notes")?;
        Ok(Self {
            db,
            files,
            tags,
            notes,
        })
    }

    /// Insert or update a file-tags pairing
    ///
    /// # Arguments
    /// * `pair` - The Pair struct containing file path and tags
    ///
    /// # Examples
    /// ```no_run
    /// use tagr::{db::Database, Pair};
    /// use std::path::PathBuf;
    ///
    /// let db = Database::open("my_db").unwrap();
    /// let pair = Pair::new(PathBuf::from("file.txt"), vec!["tag1".into()]);
    /// db.insert_pair(&pair).unwrap();
    /// ```
    ///
    /// # Errors
    ///
    /// Returns `DbError` if the file does not exist, the path contains invalid UTF-8,
    /// database operations fail, or serialization errors occur.
    pub fn insert_pair(&self, pair: &Pair) -> Result<(), DbError> {
        if !pair.file.exists() {
            return Err(DbError::FileNotFound(pair.file.display().to_string()));
        }

        let file_path = PathString::new(&pair.file)?;

        if let Some(old_tags) = self.get_tags(&pair.file)? {
            self.remove_from_tag_index(&file_path, &old_tags)?;
        }

        let key = bincode::encode_to_vec(&pair.file, bincode::config::standard())?;
        let value = bincode::encode_to_vec(&pair.tags, bincode::config::standard())?;
        self.files.insert(key, value)?;

        self.add_to_tag_index(&file_path, &pair.tags)?;

        Ok(())
    }

    /// Insert or update tags for a specific file
    ///
    /// # Arguments
    /// * `file` - Path to the file
    /// * `tags` - Vector of tag strings
    ///
    /// # Errors
    ///
    /// Returns `DbError` if the file does not exist, the path contains invalid UTF-8,
    /// database operations fail, or serialization errors occur.
    pub fn insert<P: AsRef<Path>>(&self, file: P, tags: Vec<String>) -> Result<(), DbError> {
        if !file.as_ref().exists() {
            return Err(DbError::FileNotFound(file.as_ref().display().to_string()));
        }

        let pair = Pair::new(file.as_ref().to_path_buf(), tags);
        self.insert_pair(&pair)
    }

    /// Get tags for a specific file
    ///
    /// # Arguments
    /// * `file` - Path to the file
    ///
    /// # Returns
    /// * `Some(Vec<String>)` if the file exists in the database
    /// * `None` if the file is not found
    ///
    /// # Errors
    ///
    /// Returns `DbError` if database operations fail or deserialization errors occur.
    pub fn get_tags<P: AsRef<Path>>(&self, file: P) -> Result<Option<Vec<String>>, DbError> {
        let key: Vec<u8> = PathKey::new(file).try_into()?;

        match self.files.get(key.as_slice())? {
            Some(value) => {
                let (tags, _): (Vec<String>, usize) =
                    bincode::decode_from_slice(&value, bincode::config::standard())?;
                Ok(Some(tags))
            }
            None => Ok(None),
        }
    }

    /// Get the complete Pair (file and tags) for a specific file
    ///
    /// # Arguments
    /// * `file` - Path to the file
    ///
    /// # Errors
    ///
    /// Returns `DbError` if database operations fail or deserialization errors occur.
    pub fn get_pair<P: AsRef<Path>>(&self, file: P) -> Result<Option<Pair>, DbError> {
        let key: Vec<u8> = PathKey::new(&file).try_into()?;

        match self.files.get(key.as_slice())? {
            Some(value) => {
                let (file_path, _): (PathBuf, usize) =
                    bincode::decode_from_slice(&key, bincode::config::standard())?;
                let (tags, _): (Vec<String>, usize) =
                    bincode::decode_from_slice(&value, bincode::config::standard())?;
                Ok(Some(Pair::new(file_path, tags)))
            }
            None => Ok(None),
        }
    }

    /// Remove a file and its tags from the database
    ///
    /// # Arguments
    /// * `file` - Path to the file to remove
    ///
    /// # Errors
    ///
    /// Returns `DbError` if the path contains invalid UTF-8, database operations fail,
    /// or tag index cleanup fails.
    pub fn remove<P: AsRef<Path>>(&self, file: P) -> Result<bool, DbError> {
        let file_path = PathString::new(file.as_ref())?;

        let key: Vec<u8> = PathKey::new(file.as_ref()).try_into()?;

        if let Some(tags) = self.get_tags(file.as_ref())? {
            self.remove_from_tag_index(&file_path, &tags)?;
        }

        // Also remove associated note if it exists
        self.delete_note(file.as_ref())?;

        Ok(self.files.remove(key.as_slice())?.is_some())
    }

    /// Add tags to an existing file (merges with existing tags)
    ///
    /// # Arguments
    /// * `file` - Path to the file
    /// * `new_tags` - Tags to add
    ///
    /// # Errors
    ///
    /// Returns `DbError` if database operations fail or if insertion fails.
    pub fn add_tags<P: AsRef<Path>>(&self, file: P, new_tags: Vec<String>) -> Result<(), DbError> {
        let path = file.as_ref();
        let existing = self.get_tags(path)?.unwrap_or_default();

        let mut tag_set: HashSet<String> = existing.into_iter().collect();
        tag_set.extend(new_tags);

        self.insert(path, tag_set.into_iter().collect())
    }

    /// Remove specific tags from a file
    ///
    /// If all tags are removed but the file has a note, the file entry will be preserved
    /// with an empty tags list (maintaining the equality model: files with notes are tracked).
    ///
    /// # Arguments
    /// * `file` - Path to the file
    /// * `tags_to_remove` - Tags to remove
    ///
    /// # Errors
    ///
    /// Returns `DbError` if database operations fail or if updating the file entry fails.
    pub fn remove_tags<P: AsRef<Path>>(
        &self,
        file: P,
        tags_to_remove: &[String],
    ) -> Result<(), DbError> {
        let path = file.as_ref();
        if let Some(mut tags) = self.get_tags(path)? {
            tags.retain(|tag| !tags_to_remove.contains(tag));

            if tags.is_empty() {
                // Check if file has a note before removing from database
                let has_note = self.get_note(path)?.is_some();
                if has_note {
                    // Keep file in database with empty tags (equality model)
                    self.insert(path, tags)?;
                } else {
                    // No note - safe to remove completely
                    self.remove(path)?;
                }
            } else {
                self.insert(path, tags)?;
            }
        }
        Ok(())
    }

    /// List all file-tag pairings in the database
    ///
    /// # Returns
    /// Vector of all Pair structs in the database
    ///
    /// # Errors
    ///
    /// Returns `DbError` if database iteration fails or deserialization errors occur.
    pub fn list_all(&self) -> Result<Vec<Pair>, DbError> {
        let mut pairs = Vec::new();
        for result in &self.files {
            let (key, value) = result?;
            let (file, _): (PathBuf, usize) =
                bincode::decode_from_slice(&key, bincode::config::standard())?;
            let (tags, _): (Vec<String>, usize) =
                bincode::decode_from_slice(&value, bincode::config::standard())?;
            pairs.push(Pair::new(file, tags));
        }
        Ok(pairs)
    }

    /// Find all files that have a specific tag (optimized with reverse index)
    ///
    /// # Arguments
    /// * `tag` - The tag to search for
    ///
    /// # Returns
    /// Vector of file paths that contain the specified tag
    ///
    /// # Performance
    /// O(1) lookup using the reverse tag index instead of O(n) full scan
    ///
    /// # Errors
    ///
    /// Returns `DbError` if database operations fail or deserialization errors occur.
    pub fn find_by_tag(&self, tag: &str) -> Result<Vec<PathBuf>, DbError> {
        let key = tag.as_bytes();

        match self.tags.get(key)? {
            Some(value) => {
                let (files, _): (Vec<String>, usize) =
                    bincode::decode_from_slice(&value, bincode::config::standard())?;
                Ok(files.into_iter().map(PathBuf::from).collect())
            }
            None => Ok(Vec::new()),
        }
    }

    /// Find all files that have all of the specified tags (optimized)
    ///
    /// # Arguments
    /// * `tags` - The tags to search for (AND operation)
    ///
    /// # Returns
    /// Vector of file paths that contain all specified tags
    ///
    /// # Performance
    /// Uses reverse index to find intersection of file sets
    ///
    /// # Errors
    ///
    /// Returns `DbError` if any tag lookup fails or database operations fail.
    pub fn find_by_all_tags(&self, tags: &[String]) -> Result<Vec<PathBuf>, DbError> {
        if tags.is_empty() {
            return Ok(Vec::new());
        }

        let mut file_sets: Vec<HashSet<String>> = tags
            .iter()
            .map(|tag| {
                self.find_by_tag(tag).map(|files| {
                    files
                        .into_iter()
                        .filter_map(|p| p.to_str().map(String::from))
                        .collect()
                })
            })
            .collect::<Result<_, _>>()?;

        let first_set = file_sets.remove(0);
        let result: HashSet<_> = first_set
            .into_iter()
            .filter(|file| file_sets.iter().all(|set| set.contains(file)))
            .collect();

        Ok(result.into_iter().map(PathBuf::from).collect())
    }

    /// Find all files that have any of the specified tags (optimized)
    ///
    /// # Arguments
    /// * `tags` - The tags to search for (OR operation)
    ///
    /// # Returns
    /// Vector of file paths that contain at least one of the specified tags
    ///
    /// # Performance
    /// Uses reverse index to find union of file sets
    ///
    /// # Errors
    ///
    /// Returns `DbError` if any tag lookup fails or database operations fail.
    pub fn find_by_any_tag(&self, tags: &[String]) -> Result<Vec<PathBuf>, DbError> {
        let mut file_set = HashSet::new();

        for tag in tags {
            let files = self.find_by_tag(tag)?;
            for file in files {
                if let Some(file_str) = file.to_str() {
                    file_set.insert(file_str.to_string());
                }
            }
        }

        Ok(file_set.into_iter().map(PathBuf::from).collect())
    }

    /// Get all unique tags in the database (optimized)
    ///
    /// # Returns
    /// Vector of all unique tags across all files
    ///
    /// # Performance
    /// O(k) where k is number of unique tags, using the tags tree
    ///
    /// # Errors
    ///
    /// Returns `DbError` if database iteration fails or if tag keys contain invalid UTF-8.
    pub fn list_all_tags(&self) -> Result<Vec<String>, DbError> {
        let mut tag_vec: Vec<String> = self
            .tags
            .iter()
            .filter_map(|result| {
                result
                    .ok()
                    .and_then(|(key, _)| String::from_utf8(key.to_vec()).ok())
            })
            .collect();
        tag_vec.sort();
        Ok(tag_vec)
    }

    /// Get the number of entries in the database
    #[must_use]
    pub fn count(&self) -> usize {
        self.files.len()
    }

    /// Check if a file exists in the database
    ///
    /// # Errors
    ///
    /// Returns `DbError` if database operations fail or serialization errors occur.
    pub fn contains<P: AsRef<Path>>(&self, file: P) -> Result<bool, DbError> {
        let key: Vec<u8> = PathKey::new(file).try_into()?;

        Ok(self.files.contains_key(key.as_slice())?)
    }

    /// Flush all pending writes to disk
    ///
    /// This ensures data durability by forcing a write to disk
    ///
    /// # Errors
    ///
    /// Returns `DbError` if the flush operation fails.
    pub fn flush(&self) -> Result<(), DbError> {
        self.db.flush()?;
        Ok(())
    }

    /// Remove a specific tag from all files in the database
    ///
    /// This method removes the tag from all files and then cleans up
    /// any files that have no remaining tags.
    ///
    /// # Arguments
    /// * `tag` - The tag to remove from all files
    ///
    /// # Returns
    /// Number of files that were removed from the database (files with no remaining tags)
    ///
    /// # Errors
    ///
    /// Returns `DbError` if database operations fail.
    pub fn remove_tag_globally(&self, tag: &str) -> Result<usize, DbError> {
        let files_with_tag = self.find_by_tag(tag)?;
        let mut files_removed = 0;

        for file in files_with_tag {
            self.remove_tags(&file, &[tag.to_string()])?;

            if let Some(remaining_tags) = self.get_tags(&file)?
                && remaining_tags.is_empty()
            {
                files_removed += 1;
            }
        }

        Ok(files_removed)
    }

    /// Clear all entries from the database
    ///
    /// # Warning
    /// This operation is irreversible!
    ///
    /// # Errors
    ///
    /// Returns `DbError` if clearing either the files or tags tree fails.
    pub fn clear(&self) -> Result<(), DbError> {
        self.files.clear()?;
        self.tags.clear()?;
        Ok(())
    }

    /// Get all file paths stored in the database
    ///
    /// # Returns
    /// Vector of all file paths in the database
    ///
    /// # Errors
    ///
    /// Returns `DbError` if database iteration fails or deserialization errors occur.
    pub fn list_all_files(&self) -> Result<Vec<PathBuf>, DbError> {
        let mut files = Vec::new();
        for result in &self.files {
            let (key, _) = result?;
            let (file, _): (PathBuf, usize) =
                bincode::decode_from_slice(&key, bincode::config::standard())?;
            files.push(file);
        }
        Ok(files)
    }

    /// Find files matching a regex pattern for tags
    ///
    /// Searches for tags that match the regex pattern, then returns all files
    /// that have ANY of the matching tags.
    ///
    /// # Arguments
    /// * `pattern` - Regex pattern to match against tag names
    ///
    /// # Returns
    /// Vector of file paths that contain tags matching the pattern
    ///
    /// # Errors
    ///
    /// Returns `DbError` if the regex pattern is invalid or database operations fail.
    pub fn find_by_tag_regex(&self, pattern: &str) -> Result<Vec<PathBuf>, DbError> {
        let regex = Regex::new(pattern)
            .map_err(|e| DbError::InvalidInput(format!("Invalid regex pattern: {e}")))?;

        let matching_tags: Vec<String> = self
            .list_all_tags()?
            .into_iter()
            .filter(|tag| regex.is_match(tag))
            .collect();

        if matching_tags.is_empty() {
            return Ok(Vec::new());
        }

        self.find_by_any_tag(&matching_tags)
    }

    /// Find files excluding certain tags
    ///
    /// Returns files that match the include criteria but don't have any of the excluded tags.
    ///
    /// # Arguments
    /// * `include_tags` - Tags that files must have (uses AND logic if multiple)
    /// * `exclude_tags` - Tags that files must NOT have (uses OR logic)
    ///
    /// # Returns
    /// Vector of file paths matching the criteria
    ///
    /// # Errors
    ///
    /// Returns `DbError` if database operations fail.
    pub fn find_excluding_tags(
        &self,
        include_tags: &[String],
        exclude_tags: &[String],
    ) -> Result<Vec<PathBuf>, DbError> {
        let included = if include_tags.is_empty() {
            self.list_all_files()?
        } else {
            self.find_by_all_tags(include_tags)?
        };

        if exclude_tags.is_empty() {
            return Ok(included);
        }

        let excluded: HashSet<_> = self.find_by_any_tag(exclude_tags)?.into_iter().collect();

        Ok(included
            .into_iter()
            .filter(|f| !excluded.contains(f))
            .collect())
    }

    // Private helper methods for managing the tag index

    /// Add file to tag index for all specified tags
    ///
    /// Updates the reverse index to include the file path under each tag.
    /// If a tag doesn't exist in the index, it is created.
    ///
    /// # Arguments
    /// * `file_path` - String representation of the file path
    /// * `tags` - List of tags to add the file under
    ///
    /// # Errors
    ///
    /// Returns `DbError` if database operations fail or serialization errors occur.
    fn add_to_tag_index(&self, file_path: &str, tags: &[String]) -> Result<(), DbError> {
        for tag in tags {
            let tag_key = tag.as_bytes();

            let mut files: Vec<String> = match self.tags.get(tag_key)? {
                Some(value) => {
                    let (files, _): (Vec<String>, usize) =
                        bincode::decode_from_slice(&value, bincode::config::standard())?;
                    files
                }
                None => Vec::new(),
            };

            if !files.contains(&file_path.to_string()) {
                files.push(file_path.to_string());
            }

            let encoded = bincode::encode_to_vec(&files, bincode::config::standard())?;
            self.tags.insert(tag_key, encoded)?;
        }
        Ok(())
    }

    /// Remove file from tag index for all specified tags
    ///
    /// Updates the reverse index to remove the file path from each tag's file list.
    /// If a tag has no remaining files after removal, the tag is deleted from the index.
    ///
    /// # Arguments
    /// * `file_path` - String representation of the file path
    /// * `tags` - List of tags to remove the file from
    ///
    /// # Errors
    ///
    /// Returns `DbError` if database operations fail or deserialization errors occur.
    fn remove_from_tag_index(&self, file_path: &str, tags: &[String]) -> Result<(), DbError> {
        for tag in tags {
            let tag_key = tag.as_bytes();

            if let Some(value) = self.tags.get(tag_key)? {
                let (mut files, _): (Vec<String>, usize) =
                    bincode::decode_from_slice(&value, bincode::config::standard())?;

                files.retain(|f| f != file_path);

                if files.is_empty() {
                    self.tags.remove(tag_key)?;
                } else {
                    let encoded = bincode::encode_to_vec(&files, bincode::config::standard())?;
                    self.tags.insert(tag_key, encoded)?;
                }
            }
        }
        Ok(())
    }

    // ==================== Note Operations ====================

    /// Set or update a note for a file
    ///
    /// # Arguments
    /// * `file` - Path to the file
    /// * `note` - Note content and metadata
    ///
    /// # Examples
    /// ```no_run
    /// use tagr::db::{Database, NoteRecord};
    ///
    /// let db = Database::open("my_db").unwrap();
    /// let note = NoteRecord::new("My note content".to_string());
    /// db.set_note("file.txt", note).unwrap();
    /// ```
    ///
    /// # Errors
    ///
    /// Returns `DbError` if path contains invalid UTF-8 or serialization fails.
    pub fn set_note<P: AsRef<Path>>(&self, file: P, note: NoteRecord) -> Result<(), DbError> {
        let file_path = file.as_ref();
        let key = bincode::encode_to_vec(file_path, bincode::config::standard())?;
        let value = bincode::encode_to_vec(&note, bincode::config::standard())?;
        self.notes.insert(key, value)?;

        // Ensure file exists in files tree (with empty tags if not already present)
        // This maintains the equality model: files with notes are "tracked" even without tags
        if self.get_tags(file_path)?.is_none() {
            // File not in database - add it with empty tags
            self.insert(file_path, vec![])?;
        }

        Ok(())
    }

    /// Get the note for a file
    ///
    /// # Arguments
    /// * `file` - Path to the file
    ///
    /// # Returns
    /// * `Some(NoteRecord)` if note exists
    /// * `None` if no note exists for this file
    ///
    /// # Errors
    ///
    /// Returns `DbError` if deserialization fails.
    pub fn get_note<P: AsRef<Path>>(&self, file: P) -> Result<Option<NoteRecord>, DbError> {
        let key = bincode::encode_to_vec(file.as_ref(), bincode::config::standard())?;

        if let Some(value) = self.notes.get(key)? {
            let (note, _): (NoteRecord, usize) =
                bincode::decode_from_slice(&value, bincode::config::standard())?;
            Ok(Some(note))
        } else {
            Ok(None)
        }
    }

    /// Delete a note for a file
    ///
    /// If the file has no tags after note deletion, it will be removed from the files tree
    /// (maintaining the equality model: files with neither tags nor notes are not tracked).
    ///
    /// # Arguments
    /// * `file` - Path to the file
    ///
    /// # Returns
    /// * `true` if note was deleted
    /// * `false` if no note existed
    ///
    /// # Errors
    ///
    /// Returns `DbError` if database operation fails.
    pub fn delete_note<P: AsRef<Path>>(&self, file: P) -> Result<bool, DbError> {
        let file_path = file.as_ref();
        let key = bincode::encode_to_vec(file_path, bincode::config::standard())?;
        let was_deleted = self.notes.remove(key.clone())?.is_some();

        if was_deleted {
            // Check if file has any tags - if not, remove from files tree
            if let Some(tags_value) = self.files.get(key.clone())? {
                let (tags, _): (Vec<String>, usize) =
                    bincode::decode_from_slice(&tags_value, bincode::config::standard())?;

                if tags.is_empty() {
                    // No tags and no note - remove from files tree
                    self.files.remove(key)?;
                }
            }
        }

        Ok(was_deleted)
    }

    /// List all files that have notes
    ///
    /// # Returns
    /// Vector of (`file_path`, note) tuples
    ///
    /// # Errors
    ///
    /// Returns `DbError` if deserialization fails.
    pub fn list_all_notes(&self) -> Result<Vec<(PathBuf, NoteRecord)>, DbError> {
        let mut results = Vec::new();

        for item in &self.notes {
            let (key, value) = item?;
            let (path, _): (PathBuf, usize) =
                bincode::decode_from_slice(&key, bincode::config::standard())?;
            let (note, _): (NoteRecord, usize) =
                bincode::decode_from_slice(&value, bincode::config::standard())?;
            results.push((path, note));
        }

        Ok(results)
    }

    /// Search for files whose notes contain the query string
    ///
    /// Uses case-insensitive substring matching. For large databases (>100 notes),
    /// consider using a token index for better performance (future enhancement).
    ///
    /// # Arguments
    /// * `query` - Search query (searches in note content)
    ///
    /// # Returns
    /// Vector of (`file_path`, note) tuples matching the query
    ///
    /// # Errors
    ///
    /// Returns `DbError` if deserialization fails.
    pub fn search_notes(&self, query: &str) -> Result<Vec<(PathBuf, NoteRecord)>, DbError> {
        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        for item in &self.notes {
            let (key, value) = item?;
            let (path, _): (PathBuf, usize) =
                bincode::decode_from_slice(&key, bincode::config::standard())?;
            let (note, _): (NoteRecord, usize) =
                bincode::decode_from_slice(&value, bincode::config::standard())?;

            // Case-insensitive search in content
            if note.content.to_lowercase().contains(&query_lower) {
                results.push((path, note));
            }
        }

        Ok(results)
    }
}

impl Drop for Database {
    fn drop(&mut self) {
        // Best-effort flush on drop. Errors are ignored since we can't
        // propagate them from Drop. Callers should explicitly flush()
        // if they need guaranteed durability.
        let _ = self.db.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{TempFile, TestDb};
    use std::fs;

    #[test]
    fn test_create_database() {
        let test_db = TestDb::new("test_db_create");
        let db = test_db.db();

        assert!(test_db.path().exists());
        assert_eq!(db.count(), 0);
        // TestDb automatically cleaned up on drop
    }

    #[test]
    fn test_create_database_with_data() {
        let test_db = TestDb::new("test_db_with_data");
        let db = test_db.db();

        let file1 = TempFile::create("file1.txt").unwrap();
        let file2 = TempFile::create("file2.txt").unwrap();

        db.insert(file1.path(), vec!["tag1".into(), "tag2".into()])
            .unwrap();
        db.insert(file2.path(), vec!["tag3".into()]).unwrap();

        assert_eq!(db.count(), 2);
        assert!(db.contains(file1.path()).unwrap());
        assert!(db.contains(file2.path()).unwrap());
        // TestDb and TempFiles automatically cleaned up
    }

    #[test]
    fn test_remove_database_by_clearing() {
        let test_db = TestDb::new("test_db_clear");
        let db = test_db.db();

        let file1 = TempFile::create("file1.txt").unwrap();
        let file2 = TempFile::create("file2.txt").unwrap();
        let file3 = TempFile::create("file3.txt").unwrap();

        db.insert(file1.path(), vec!["tag1".into()]).unwrap();
        db.insert(file2.path(), vec!["tag2".into()]).unwrap();
        db.insert(file3.path(), vec!["tag3".into()]).unwrap();

        assert_eq!(db.count(), 3);

        db.clear().unwrap();

        assert_eq!(db.count(), 0);
        assert!(!db.contains(file1.path()).unwrap());
        assert!(!db.contains(file2.path()).unwrap());
        assert!(!db.contains(file3.path()).unwrap());
        assert_eq!(db.list_all_tags().unwrap().len(), 0);
    }

    #[test]
    fn test_remove_database_physically() {
        let test_db_path = "test_db_remove";
        let file = TempFile::create("file.txt").unwrap();

        {
            let db = Database::open(test_db_path).unwrap();
            db.clear().unwrap();
            db.insert(file.path(), vec!["tag".into()]).unwrap();
            assert!(PathBuf::from(test_db_path).exists());
        }

        fs::remove_dir_all(test_db_path).unwrap();

        assert!(!PathBuf::from(test_db_path).exists());
    }

    #[test]
    fn test_reopen_existing_database() {
        let test_db_path = "test_db_reopen";
        let file = TempFile::create("persistent.txt").unwrap();

        {
            let db = Database::open(test_db_path).unwrap();
            db.clear().unwrap();
            db.insert(file.path(), vec!["saved".into()]).unwrap();
            db.flush().unwrap();
        }

        // Mitigate occasional sled lock retention in parallel test runs
        std::thread::sleep(std::time::Duration::from_millis(20));

        {
            let db = Database::open(test_db_path).unwrap();
            assert_eq!(db.count(), 1);
            assert!(db.contains(file.path()).unwrap());
            let tags = db.get_tags(file.path()).unwrap();
            assert_eq!(tags, Some(vec!["saved".into()]));

            db.clear().unwrap();
        }

        let _ = fs::remove_dir_all(test_db_path);
    }

    #[test]
    fn test_create_multiple_databases() {
        let db_paths = ["test_db_multi_1", "test_db_multi_2", "test_db_multi_3"];
        let mut test_dbs = Vec::new();
        let mut temp_files = Vec::new();

        for (i, path) in db_paths.iter().enumerate() {
            let test_db = TestDb::new(path);
            let filename = format!("file{i}.txt");
            let temp_file = TempFile::create(&filename).unwrap();
            test_db
                .db()
                .insert(temp_file.path(), vec![format!("tag{}", i)])
                .unwrap();
            temp_files.push(temp_file);
            test_dbs.push(test_db);
        }

        for (i, test_db) in test_dbs.iter().enumerate() {
            assert_eq!(test_db.db().count(), 1);
            assert!(test_db.db().contains(temp_files[i].path()).unwrap());
        }
    }

    #[test]
    fn test_remove_and_recreate_database() {
        let test_db_path = "test_db_recreate";

        {
            let old_file = TempFile::create("old_file.txt").unwrap();
            let db = Database::open(test_db_path).unwrap();
            db.clear().unwrap();
            db.insert(old_file.path(), vec!["old_tag".into()]).unwrap();
            assert_eq!(db.count(), 1);
        }

        fs::remove_dir_all(test_db_path).unwrap();
        assert!(!PathBuf::from(test_db_path).exists());

        {
            let new_file = TempFile::create("new_file.txt").unwrap();
            let old_file_path = PathBuf::from("old_file.txt");
            let db = Database::open(test_db_path).unwrap();
            assert_eq!(db.count(), 0);

            db.insert(new_file.path(), vec!["new_tag".into()]).unwrap();
            assert_eq!(db.count(), 1);
            assert!(db.contains(new_file.path()).unwrap());
            assert!(!db.contains(&old_file_path).unwrap());

            db.clear().unwrap();
        }

        let _ = fs::remove_dir_all(test_db_path);
    }

    #[test]
    fn test_database_flush_on_drop() {
        let test_db_path = "test_db_flush_drop";
        let file = TempFile::create("file.txt").unwrap();

        {
            let db = Database::open(test_db_path).unwrap();
            db.clear().unwrap();
            db.insert(file.path(), vec!["tag".into()]).unwrap();
        }

        {
            let db = Database::open(test_db_path).unwrap();
            assert!(db.contains(file.path()).unwrap());
            db.clear().unwrap();
        }

        let _ = fs::remove_dir_all(test_db_path);
    }

    #[test]
    fn test_database_operations() {
        let test_db = TestDb::new("test_db");
        let db = test_db.db();

        let file = TempFile::create("test.txt").unwrap();
        let pair = Pair::new(
            file.path().to_path_buf(),
            vec!["tag1".into(), "tag2".into()],
        );
        db.insert_pair(&pair).unwrap();

        let tags = db.get_tags(file.path()).unwrap();
        assert_eq!(tags, Some(vec!["tag1".into(), "tag2".into()]));

        let files = db.find_by_tag("tag1").unwrap();
        assert_eq!(files.len(), 1);
    }

    // ==================== Note Tests ====================

    #[test]
    fn test_set_and_get_note() {
        let test_db = TestDb::new("test_set_and_get_note");
        let db = test_db.db();

        let file = TempFile::create("note_test.txt").unwrap();
        let note = NoteRecord::new("Test note content".to_string());

        // Set note
        db.set_note(file.path(), note.clone()).unwrap();

        // Get note
        let retrieved = db.get_note(file.path()).unwrap();
        assert_eq!(retrieved, Some(note));
    }

    #[test]
    fn test_update_note() {
        let test_db = TestDb::new("test_update_note");
        let db = test_db.db();

        let file = TempFile::create("update_test.txt").unwrap();
        let note1 = NoteRecord::new("Original content".to_string());
        let mut note2 = NoteRecord::new("Updated content".to_string());
        note2.metadata.created_at = note1.metadata.created_at; // Keep same creation time

        // Set initial note
        db.set_note(file.path(), note1).unwrap();

        // Update note
        db.set_note(file.path(), note2).unwrap();

        // Verify update
        let retrieved = db.get_note(file.path()).unwrap().unwrap();
        assert_eq!(retrieved.content, "Updated content");
    }

    #[test]
    fn test_delete_note() {
        let test_db = TestDb::new("test_delete_note");
        let db = test_db.db();

        let file = TempFile::create("delete_test.txt").unwrap();
        let note = NoteRecord::new("To be deleted".to_string());

        // Set note
        db.set_note(file.path(), note).unwrap();
        assert!(db.get_note(file.path()).unwrap().is_some());

        // Delete note
        let deleted = db.delete_note(file.path()).unwrap();
        assert!(deleted);

        // Verify deletion
        assert!(db.get_note(file.path()).unwrap().is_none());

        // Delete again should return false
        let deleted_again = db.delete_note(file.path()).unwrap();
        assert!(!deleted_again);
    }

    #[test]
    fn test_get_nonexistent_note() {
        let test_db = TestDb::new("test_get_nonexistent_note");
        let db = test_db.db();

        let file = TempFile::create("nonexistent.txt").unwrap();
        let result = db.get_note(file.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_list_all_notes() {
        let test_db = TestDb::new("test_list_all_notes");
        let db = test_db.db();

        let file1 = TempFile::create("file1.txt").unwrap();
        let file2 = TempFile::create("file2.txt").unwrap();
        let file3 = TempFile::create("file3.txt").unwrap();

        let note1 = NoteRecord::new("Note 1".to_string());
        let note2 = NoteRecord::new("Note 2".to_string());

        // Add notes to file1 and file2
        db.set_note(file1.path(), note1).unwrap();
        db.set_note(file2.path(), note2).unwrap();

        // file3 has no note

        let all_notes = db.list_all_notes().unwrap();
        assert_eq!(all_notes.len(), 2);

        let paths: Vec<PathBuf> = all_notes.iter().map(|(p, _)| p.clone()).collect();
        assert!(paths.contains(&file1.path().to_path_buf()));
        assert!(paths.contains(&file2.path().to_path_buf()));
        assert!(!paths.contains(&file3.path().to_path_buf()));
    }

    #[test]
    fn test_search_notes_basic() {
        let test_db = TestDb::new("test_search_notes_basic");
        let db = test_db.db();

        let file1 = TempFile::create("rust_file.txt").unwrap();
        let file2 = TempFile::create("python_file.txt").unwrap();
        let _file3 = TempFile::create("empty_file.txt").unwrap();

        db.set_note(
            file1.path(),
            NoteRecord::new("rust programming".to_string()),
        )
        .unwrap();
        db.set_note(
            file2.path(),
            NoteRecord::new("python scripting".to_string()),
        )
        .unwrap();

        // Search for "rust"
        let results = db.search_notes("rust").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, file1.path().to_path_buf());

        // Search for "programming"
        let results = db.search_notes("programming").unwrap();
        assert_eq!(results.len(), 1);

        // Search for non-existent term
        let results = db.search_notes("java").unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_search_notes_case_insensitive() {
        let test_db = TestDb::new("test_search_notes_case");
        let db = test_db.db();

        let file = TempFile::create("test.txt").unwrap();
        db.set_note(
            file.path(),
            NoteRecord::new("Rust Programming Language".to_string()),
        )
        .unwrap();

        // All these should match
        assert_eq!(db.search_notes("rust").unwrap().len(), 1);
        assert_eq!(db.search_notes("RUST").unwrap().len(), 1);
        assert_eq!(db.search_notes("RuSt").unwrap().len(), 1);
        assert_eq!(db.search_notes("programming").unwrap().len(), 1);
    }

    #[test]
    fn test_search_notes_partial_match() {
        let test_db = TestDb::new("test_search_notes_partial");
        let db = test_db.db();

        let file = TempFile::create("test.txt").unwrap();
        db.set_note(
            file.path(),
            NoteRecord::new("async/await in rust".to_string()),
        )
        .unwrap();

        // Partial matches should work
        assert_eq!(db.search_notes("async").unwrap().len(), 1);
        assert_eq!(db.search_notes("await").unwrap().len(), 1);
        assert_eq!(db.search_notes("rus").unwrap().len(), 1);
    }

    #[test]
    fn test_note_with_metadata() {
        let test_db = TestDb::new("test_note_metadata");
        let db = test_db.db();

        let file = TempFile::create("metadata_test.txt").unwrap();
        let note = NoteRecord {
            content: "Test content".to_string(),
            metadata: NoteMeta {
                created_at: 1_234_567_890,
                updated_at: 1_234_567_890,
            },
        };

        db.set_note(file.path(), note).unwrap();
        let retrieved = db.get_note(file.path()).unwrap().unwrap();

        assert_eq!(retrieved.metadata.created_at, 1_234_567_890);
        assert_eq!(retrieved.metadata.updated_at, 1_234_567_890);
    }

    #[test]
    fn test_note_update_content_helper() {
        let mut note = NoteRecord::new("Original".to_string());
        let original_created = note.metadata.created_at;
        let original_updated = note.metadata.updated_at;

        // Sleep to ensure next timestamp will be different
        std::thread::sleep(std::time::Duration::from_secs(1));

        note.update_content("Updated".to_string());

        assert_eq!(note.content, "Updated");
        assert_eq!(note.metadata.created_at, original_created);
        assert!(note.metadata.updated_at >= original_updated);
        // Note: >= instead of > because system time might not advance on all platforms
    }
}
