//! Database wrapper module for tagr
//! 
//! Provides a clean API for storing and retrieving file-tag pairings
//! using sled as the embedded database backend.
//! 
//! Uses multiple sled trees for efficient indexing:
//! - `files`: Main tree mapping file paths to tags
//! - `tags`: Reverse index mapping tags to file paths

use sled::{Db, Tree};
use std::path::{Path, PathBuf};
use std::collections::HashSet;
use bincode;
use crate::Pair;

pub mod error;
pub mod types;

pub use error::DbError;
pub use types::{PathKey, PathString};

/// Database wrapper that encapsulates all database operations
/// 
/// Uses two trees for efficient bidirectional lookups:
/// - `files` tree: `file_path` -> `Vec<tag>`
/// - `tags` tree: tag -> Vec<`file_path`>
pub struct Database {
    db: Db,
    files: Tree,  // file -> tags mapping
    tags: Tree,   // tag -> files reverse index
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
        Ok(Self { db, files, tags })
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
            self.remove_from_tag_index(file_path.as_str(), &old_tags)?;
        }
        
        let key = bincode::encode_to_vec(&pair.file, bincode::config::standard())?;
        let value = bincode::encode_to_vec(&pair.tags, bincode::config::standard())?;
        self.files.insert(key, value)?;
        
        self.add_to_tag_index(file_path.as_str(), &pair.tags)?;
        
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
            None => Ok(None)
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
            None => Ok(None)
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
            self.remove_from_tag_index(file_path.as_str(), &tags)?;
        }
        
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
        
        // Use HashSet for efficient deduplication
        let mut tag_set: HashSet<String> = existing.into_iter().collect();
        tag_set.extend(new_tags);
        
        self.insert(path, tag_set.into_iter().collect())
    }

    /// Remove specific tags from a file
    /// 
    /// # Arguments
    /// * `file` - Path to the file
    /// * `tags_to_remove` - Tags to remove
    /// 
    /// # Errors
    /// 
    /// Returns `DbError` if database operations fail or if updating the file entry fails.
    pub fn remove_tags<P: AsRef<Path>>(&self, file: P, tags_to_remove: &[String]) -> Result<(), DbError> {
        let path = file.as_ref();
        if let Some(mut tags) = self.get_tags(path)? {
            tags.retain(|tag| !tags_to_remove.contains(tag));
            
            if tags.is_empty() {
                self.remove(path)?;
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
            None => Ok(Vec::new())
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
        
        let mut file_sets: Vec<HashSet<String>> = tags.iter()
            .map(|tag| {
                self.find_by_tag(tag).map(|files| {
                    files.into_iter()
                        .filter_map(|p| p.to_str().map(String::from))
                        .collect()
                })
            })
            .collect::<Result<_, _>>()?;
        
        let first_set = file_sets.remove(0);
        let result: HashSet<_> = first_set.into_iter()
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
        let mut tag_vec: Vec<String> = self.tags.iter()
            .filter_map(|result| {
                result.ok().and_then(|(key, _)| {
                    String::from_utf8(key.to_vec()).ok()
                })
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
                && remaining_tags.is_empty() {
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
                None => Vec::new()
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
    use std::fs;
    use std::io::Write;

    // Helper function to create a test file
    fn create_test_file(path: &str) -> std::io::Result<()> {
        let mut file = fs::File::create(path)?;
        file.write_all(b"test content")?;
        Ok(())
    }

    #[test]
    fn test_create_database() {
        let test_db_path = "test_db_create";
        
        let db = Database::open(test_db_path).unwrap();
        
        assert!(PathBuf::from(test_db_path).exists());
        assert_eq!(db.count(), 0);
        
        // Clean up
        db.clear().unwrap();
        drop(db);
        let _ = fs::remove_dir_all(test_db_path);
    }

    #[test]
    fn test_create_database_with_data() {
        let test_db_path = "test_db_with_data";
        
        let db = Database::open(test_db_path).unwrap();
        db.clear().unwrap();
        
        create_test_file("file1.txt").unwrap();
        create_test_file("file2.txt").unwrap();
        
        db.insert("file1.txt", vec!["tag1".into(), "tag2".into()]).unwrap();
        db.insert("file2.txt", vec!["tag3".into()]).unwrap();
        
        assert_eq!(db.count(), 2);
        assert!(db.contains("file1.txt").unwrap());
        assert!(db.contains("file2.txt").unwrap());
        
        // Clean up
        db.clear().unwrap();
        drop(db);
        let _ = fs::remove_dir_all(test_db_path);
        let _ = fs::remove_file("file1.txt");
        let _ = fs::remove_file("file2.txt");
    }

    #[test]
    fn test_remove_database_by_clearing() {
        let test_db_path = "test_db_clear";
        
        let db = Database::open(test_db_path).unwrap();
        db.clear().unwrap();
        
        create_test_file("file1.txt").unwrap();
        create_test_file("file2.txt").unwrap();
        create_test_file("file3.txt").unwrap();
        
        db.insert("file1.txt", vec!["tag1".into()]).unwrap();
        db.insert("file2.txt", vec!["tag2".into()]).unwrap();
        db.insert("file3.txt", vec!["tag3".into()]).unwrap();
        
        assert_eq!(db.count(), 3);
        
        db.clear().unwrap();
        
        assert_eq!(db.count(), 0);
        assert!(!db.contains("file1.txt").unwrap());
        assert!(!db.contains("file2.txt").unwrap());
        assert!(!db.contains("file3.txt").unwrap());
        assert_eq!(db.list_all_tags().unwrap().len(), 0);
        
        // Clean up
        drop(db);
        let _ = fs::remove_dir_all(test_db_path);
        let _ = fs::remove_file("file1.txt");
        let _ = fs::remove_file("file2.txt");
        let _ = fs::remove_file("file3.txt");
    }

    #[test]
    fn test_remove_database_physically() {
        let test_db_path = "test_db_remove";
        
        {
            let db = Database::open(test_db_path).unwrap();
            db.clear().unwrap();
            create_test_file("file.txt").unwrap();
            db.insert("file.txt", vec!["tag".into()]).unwrap();
            assert!(PathBuf::from(test_db_path).exists());
        }
        
        fs::remove_dir_all(test_db_path).unwrap();
        
        assert!(!PathBuf::from(test_db_path).exists());
        let _ = fs::remove_file("file.txt");
    }

    #[test]
    fn test_reopen_existing_database() {
        let test_db_path = "test_db_reopen";
        
        {
            let db = Database::open(test_db_path).unwrap();
            db.clear().unwrap();
            create_test_file("persistent.txt").unwrap();
            db.insert("persistent.txt", vec!["saved".into()]).unwrap();
            db.flush().unwrap();
        }
        
        {
            let db = Database::open(test_db_path).unwrap();
            assert_eq!(db.count(), 1);
            assert!(db.contains("persistent.txt").unwrap());
            let tags = db.get_tags("persistent.txt").unwrap();
            assert_eq!(tags, Some(vec!["saved".into()]));
            
            // Clean up
            db.clear().unwrap();
        }
        
        let _ = fs::remove_dir_all(test_db_path);
        let _ = fs::remove_file("persistent.txt");
    }

    #[test]
    fn test_create_multiple_databases() {
        let db_paths = vec!["test_db_multi_1", "test_db_multi_2", "test_db_multi_3"];
        let mut databases = Vec::new();
        
        for (i, path) in db_paths.iter().enumerate() {
            let db = Database::open(path).unwrap();
            db.clear().unwrap();
            let filename = format!("file{}.txt", i);
            create_test_file(&filename).unwrap();
            db.insert(&filename, vec![format!("tag{}", i)]).unwrap();
            databases.push(db);
        }
        
        for (i, db) in databases.iter().enumerate() {
            assert_eq!(db.count(), 1);
            assert!(db.contains(format!("file{}.txt", i)).unwrap());
        }
        
        // Clean up
        for (db, path) in databases.into_iter().zip(db_paths.iter()) {
            db.clear().unwrap();
            drop(db);
            let _ = fs::remove_dir_all(path);
        }
        for i in 0..3 {
            let _ = fs::remove_file(format!("file{}.txt", i));
        }
    }

    #[test]
    fn test_remove_and_recreate_database() {
        let test_db_path = "test_db_recreate";
        
        {
            let db = Database::open(test_db_path).unwrap();
            db.clear().unwrap();
            create_test_file("old_file.txt").unwrap();
            db.insert("old_file.txt", vec!["old_tag".into()]).unwrap();
            assert_eq!(db.count(), 1);
        }
        
        fs::remove_dir_all(test_db_path).unwrap();
        assert!(!PathBuf::from(test_db_path).exists());
        let _ = fs::remove_file("old_file.txt");
        
        {
            let db = Database::open(test_db_path).unwrap();
            assert_eq!(db.count(), 0);
            
            create_test_file("new_file.txt").unwrap();
            db.insert("new_file.txt", vec!["new_tag".into()]).unwrap();
            assert_eq!(db.count(), 1);
            assert!(db.contains("new_file.txt").unwrap());
            assert!(!db.contains("old_file.txt").unwrap());
            
            db.clear().unwrap();
        }
        
        let _ = fs::remove_dir_all(test_db_path);
        let _ = fs::remove_file("new_file.txt");
    }

    #[test]
    fn test_database_flush_on_drop() {
        let test_db_path = "test_db_flush_drop";
        
        {
            let db = Database::open(test_db_path).unwrap();
            db.clear().unwrap();
            create_test_file("file.txt").unwrap();
            db.insert("file.txt", vec!["tag".into()]).unwrap();
        }
        
        {
            let db = Database::open(test_db_path).unwrap();
            assert!(db.contains("file.txt").unwrap());
            db.clear().unwrap();
        }
        
        let _ = fs::remove_dir_all(test_db_path);
        let _ = fs::remove_file("file.txt");
    }

    #[test]
    fn test_database_operations() {
        let db = Database::open("test_db").unwrap();
        db.clear().unwrap();

        create_test_file("test.txt").unwrap();
        let pair = Pair::new(PathBuf::from("test.txt"), vec!["tag1".into(), "tag2".into()]);
        db.insert_pair(&pair).unwrap();
        
        let tags = db.get_tags("test.txt").unwrap();
        assert_eq!(tags, Some(vec!["tag1".into(), "tag2".into()]));

        let files = db.find_by_tag("tag1").unwrap();
        assert_eq!(files.len(), 1);

        db.clear().unwrap();
        let _ = fs::remove_file("test.txt");
    }
}
