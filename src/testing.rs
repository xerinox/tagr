//! Testing utilities for tagr
//!
//! This module provides helper types and functions for writing tests,
//! including a `TestDb` wrapper for temporary database management.
//!
//! Only available when compiled with `cfg(test)`.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use crate::db::Database;

/// Wrapper for a temporary test database that cleans up on drop
///
/// Automatically removes the database directory when the wrapper goes out of scope,
/// ensuring tests don't leave artifacts behind.
///
/// # Examples
/// ```
/// # use tagr::testing::TestDb;
/// let test_db = TestDb::new("my_test_db");
/// let db = test_db.db();
/// 
/// db.insert("file.txt", vec!["tag1".into()]).unwrap();
/// assert_eq!(db.count(), 1);
/// // Database automatically cleaned up when test_db is dropped
/// ```
pub struct TestDb {
    path: PathBuf,
    db: Database,
}

impl TestDb {
    /// Create a new test database at the specified path
    ///
    /// The database is opened and cleared immediately to ensure a clean state.
    ///
    /// # Panics
    /// Panics if the database cannot be opened or cleared.
    pub fn new(path: impl AsRef<Path>) -> Self {
        let path = PathBuf::from(path.as_ref());
        let db = Database::open(&path).expect("Failed to open test database");
        db.clear().expect("Failed to clear test database");
        
        Self { path, db }
    }

    /// Get a reference to the underlying database
    #[must_use]
    pub const fn db(&self) -> &Database {
        &self.db
    }

    /// Get the path to the test database
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDb {
    fn drop(&mut self) {
        // Clear the database first to ensure clean shutdown
        let _ = self.db.clear();
        
        // Remove the database directory
        // Ignore errors during cleanup - best effort
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Create a test file with default content
///
/// Creates a file at the specified path with "test content" written to it.
/// Useful for setting up test fixtures that need real files.
///
/// # Errors
/// Returns an `io::Error` if the file cannot be created or written.
///
/// # Examples
/// ```
/// # use tagr::testing::create_test_file;
/// create_test_file("test_file.txt").unwrap();
/// assert!(std::path::Path::new("test_file.txt").exists());
/// std::fs::remove_file("test_file.txt").unwrap();
/// ```
pub fn create_test_file(path: impl AsRef<Path>) -> std::io::Result<()> {
    let mut file = fs::File::create(path)?;
    file.write_all(b"test content")?;
    Ok(())
}

/// Create a test file with custom content
///
/// Creates a file at the specified path with the provided content.
///
/// # Errors
/// Returns an `io::Error` if the file cannot be created or written.
///
/// # Examples
/// ```
/// # use tagr::testing::create_test_file_with_content;
/// create_test_file_with_content("config.toml", b"key = value").unwrap();
/// std::fs::remove_file("config.toml").unwrap();
/// ```
pub fn create_test_file_with_content(path: impl AsRef<Path>, content: &[u8]) -> std::io::Result<()> {
    let mut file = fs::File::create(path)?;
    file.write_all(content)?;
    Ok(())
}

/// RAII guard for temporary test files
///
/// Automatically removes the file when dropped, ensuring test cleanup.
///
/// # Examples
/// ```
/// # use tagr::testing::TempFile;
/// {
///     let temp = TempFile::create("temp.txt").unwrap();
///     assert!(temp.path().exists());
///     // Do something with the file
/// } // File automatically deleted here
/// ```
pub struct TempFile {
    path: PathBuf,
    temp_dir: Option<PathBuf>,
}

impl TempFile {
    /// Create a new temporary file with default content
    ///
    /// Creates the file in a unique temporary directory to avoid collisions between parallel tests.
    ///
    /// # Errors
    /// Returns an `io::Error` if the file cannot be created.
    pub fn create(filename: impl AsRef<Path>) -> std::io::Result<Self> {
        use std::time::{SystemTime, UNIX_EPOCH};
        
        // Create unique temp dir using timestamp + thread id
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let thread_id = std::thread::current().id();
        let temp_dir = PathBuf::from(format!("test_tmp_{}_{:?}", timestamp, thread_id));
        fs::create_dir_all(&temp_dir)?;
        
        let path = temp_dir.join(filename.as_ref());
        create_test_file(&path)?;
        Ok(Self { path, temp_dir: Some(temp_dir) })
    }

    /// Create a new temporary file with custom content
    ///
    /// Creates the file in a unique temporary directory to avoid collisions between parallel tests.
    ///
    /// # Errors
    /// Returns an `io::Error` if the file cannot be created.
    pub fn create_with_content(filename: impl AsRef<Path>, content: &[u8]) -> std::io::Result<Self> {
        use std::time::{SystemTime, UNIX_EPOCH};
        
        // Create unique temp dir using timestamp + thread id
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let thread_id = std::thread::current().id();
        let temp_dir = PathBuf::from(format!("test_tmp_{}_{:?}", timestamp, thread_id));
        fs::create_dir_all(&temp_dir)?;
        
        let path = temp_dir.join(filename.as_ref());
        create_test_file_with_content(&path, content)?;
        Ok(Self { path, temp_dir: Some(temp_dir) })
    }

    /// Get the path to the temporary file
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        // Best effort cleanup - ignore errors
        let _ = fs::remove_file(&self.path);
        if let Some(ref temp_dir) = self.temp_dir {
            let _ = fs::remove_dir(temp_dir);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_db_basic() {
        let test_db = TestDb::new("test_testing_db_basic");
        let db = test_db.db();
        
        assert_eq!(db.count(), 0);
        assert!(test_db.path().exists());
    }

    #[test]
    fn test_db_cleanup() {
        let path = PathBuf::from("test_testing_db_cleanup");
        
        {
            let test_db = TestDb::new(&path);
            assert!(path.exists());
            let _ = test_db; // Use test_db to avoid unused variable warning
        }
        
        // Database should be cleaned up after drop
        assert!(!path.exists());
    }

    #[test]
    fn test_db_with_data() {
        let test_db = TestDb::new("test_testing_db_with_data");
        let db = test_db.db();
        
        let temp_file = TempFile::create("test_file_for_db.txt").unwrap();
        db.insert(temp_file.path(), vec!["tag1".into(), "tag2".into()]).unwrap();
        
        assert_eq!(db.count(), 1);
        assert!(db.contains(temp_file.path()).unwrap());
    }

    #[test]
    fn test_create_test_file_basic() {
        let path = "test_file_basic.txt";
        create_test_file(path).unwrap();
        
        assert!(Path::new(path).exists());
        
        let content = fs::read_to_string(path).unwrap();
        assert_eq!(content, "test content");
        
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn test_create_test_file_with_custom_content() {
        let path = "test_file_custom.txt";
        let custom_content = b"custom test data";
        
        create_test_file_with_content(path, custom_content).unwrap();
        
        let content = fs::read(path).unwrap();
        assert_eq!(content, custom_content);
        
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn test_temp_file_auto_cleanup() {
        let path = PathBuf::from("test_temp_file_cleanup.txt");
        
        {
            let temp = TempFile::create(&path).unwrap();
            assert!(temp.path().exists());
        }
        
        // File should be cleaned up after drop
        assert!(!path.exists());
    }

    #[test]
    fn test_temp_file_with_content() {
        let content = b"temporary content";
        let temp = TempFile::create_with_content("test_temp_custom.txt", content).unwrap();
        
        let read_content = fs::read(temp.path()).unwrap();
        assert_eq!(read_content, content);
        
        // temp dropped here, file cleaned up
    }

    #[test]
    fn test_multiple_temp_files() {
        let temp1 = TempFile::create("test_temp_1.txt").unwrap();
        let temp2 = TempFile::create("test_temp_2.txt").unwrap();
        let temp3 = TempFile::create("test_temp_3.txt").unwrap();
        
        assert!(temp1.path().exists());
        assert!(temp2.path().exists());
        assert!(temp3.path().exists());
        
        // All cleaned up on drop
    }
}
