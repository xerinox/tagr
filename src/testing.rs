//! Testing utilities for tagr
//!
//! This module provides helper types and functions for writing tests,
//! including a `TestDb` wrapper for temporary database management.
//!
//! Uses the standard `tempfile` crate for automatic cleanup.

use crate::db::Database;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Wrapper for a temporary test database that cleans up on drop
///
/// Uses `tempfile::tempdir()` for automatic cleanup. Database is created in a
/// temporary directory that is automatically removed when dropped.
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
    #[allow(dead_code)] // Keeps temp dir alive
    temp_dir: tempfile::TempDir,
    db: Database,
}

impl TestDb {
    /// Create a new test database with a temporary directory
    ///
    /// The database is opened and cleared immediately to ensure a clean state.
    ///
    /// # Panics
    /// Panics if the temporary directory or database cannot be created.
    pub fn new(name: impl AsRef<str>) -> Self {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let path = temp_dir.path().join(name.as_ref());
        let db = Database::open(&path).expect("Failed to open test database");
        db.clear().expect("Failed to clear test database");

        Self { temp_dir, db }
    }

    /// Get a reference to the underlying database
    #[must_use]
    pub const fn db(&self) -> &Database {
        &self.db
    }

    /// Get the path to the test database directory
    #[must_use]
    pub fn path(&self) -> PathBuf {
        self.temp_dir.path().to_path_buf()
    }
}

/// RAII guard for temporary test files using `tempfile` crate
///
/// Automatically removes the file and directory when dropped.
/// Uses `tempfile::tempdir()` for robust, automatic cleanup.
///
/// # Examples
/// ```
/// # use tagr::testing::TempFile;
/// {
///     let temp = TempFile::create("temp.txt").unwrap();
///     assert!(temp.path().exists());
///     // Do something with the file
/// } // File and directory automatically deleted here
/// ```
pub struct TempFile {
    #[allow(dead_code)] // Keeps temp dir alive
    temp_dir: tempfile::TempDir,
    path: PathBuf,
}

impl TempFile {
    /// Create a new temporary file with default content ("test content")
    ///
    /// # Errors
    /// Returns an `io::Error` if the file cannot be created.
    pub fn create(filename: impl AsRef<Path>) -> std::io::Result<Self> {
        let temp_dir = tempfile::tempdir()?;
        let path = temp_dir.path().join(filename.as_ref());
        
        let mut file = std::fs::File::create(&path)?;
        file.write_all(b"test content")?;
        
        Ok(Self { temp_dir, path })
    }

    /// Create a new temporary file with custom content
    ///
    /// # Errors
    /// Returns an `io::Error` if the file cannot be created.
    pub fn create_with_content(
        filename: impl AsRef<Path>,
        content: &[u8],
    ) -> std::io::Result<Self> {
        let temp_dir = tempfile::tempdir()?;
        let path = temp_dir.path().join(filename.as_ref());
        
        let mut file = std::fs::File::create(&path)?;
        file.write_all(content)?;
        
        Ok(Self { temp_dir, path })
    }

    /// Get the path to the temporary file
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
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
        let path_copy = {
            let test_db = TestDb::new("test_db");
            let path = test_db.path();
            assert!(path.exists());
            path.clone()
        };

        assert!(!path_copy.exists());
    }

    #[test]
    fn test_db_with_data() {
        let test_db = TestDb::new("test_testing_db_with_data");
        let db = test_db.db();

        let temp_file = TempFile::create("test_file_for_db.txt").unwrap();
        db.insert(temp_file.path(), vec!["tag1".into(), "tag2".into()])
            .unwrap();

        assert_eq!(db.count(), 1);
        assert!(db.contains(temp_file.path()).unwrap());
    }

    #[test]
    fn test_temp_file_auto_cleanup() {
        let path_copy = {
            let temp = TempFile::create("test_file.txt").unwrap();
            let path = temp.path().to_path_buf();
            assert!(path.exists());
            path
        };

        assert!(!path_copy.exists());
    }

    #[test]
    fn test_temp_file_with_content() {
        let content = b"temporary content";
        let temp = TempFile::create_with_content("test_temp_custom.txt", content).unwrap();

        let read_content = std::fs::read(temp.path()).unwrap();
        assert_eq!(read_content, content);
    }

    #[test]
    fn test_multiple_temp_files() {
        let temp1 = TempFile::create("test_temp_1.txt").unwrap();
        let temp2 = TempFile::create("test_temp_2.txt").unwrap();
        let temp3 = TempFile::create("test_temp_3.txt").unwrap();

        assert!(temp1.path().exists());
        assert!(temp2.path().exists());
        assert!(temp3.path().exists());
    }

    #[test]
    fn test_temp_file_cleanup_on_panic() {
        use std::panic;

        let path_copy = {
            let temp = TempFile::create("test_panic.txt").unwrap();
            let path = temp.path().to_path_buf();
            
            assert!(path.exists());
            
            let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
                assert!(path.exists());
                panic!("Simulated test failure");
            }));
            
            assert!(result.is_err());
            path
        };

        assert!(!path_copy.exists(), "TempFile should be cleaned up even after panic");
    }
}
