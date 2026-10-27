use moka::sync::Cache;
use std::fs::Permissions;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub struct MetadataCache {
    cache: Cache<PathBuf, FileMetadata>,
}

#[derive(Debug, Clone)]
pub struct FileMetadata {
    pub size: u64,
    pub modified: SystemTime,
    pub created: Option<SystemTime>,
    pub accessed: Option<SystemTime>,
    pub is_file: bool,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub permissions: Permissions,
}

impl MetadataCache {
    #[must_use]
    pub fn new(ttl: Duration) -> Self {
        Self {
            cache: Cache::builder().time_to_idle(ttl).build(),
        }
    }

    /// # Errors
    /// Returns an error if the file metadata cannot be read from the filesystem.
    pub fn get(&mut self, path: &Path) -> io::Result<FileMetadata> {
        if let Some(metadata) = self.cache.get(&path.to_path_buf()) {
            return Ok(metadata);
        }

        let metadata = Self::fetch_metadata(path)?;
        self.cache.insert(path.to_path_buf(), metadata.clone());

        Ok(metadata)
    }

    fn fetch_metadata(path: &Path) -> io::Result<FileMetadata> {
        let metadata = std::fs::metadata(path)?;

        Ok(FileMetadata {
            size: metadata.len(),
            modified: metadata.modified()?,
            created: metadata.created().ok(),
            accessed: metadata.accessed().ok(),
            is_file: metadata.is_file(),
            is_dir: metadata.is_dir(),
            is_symlink: metadata.is_symlink(),
            permissions: metadata.permissions(),
        })
    }

    pub fn cleanup(&mut self) {
        // Moka automatically handles TTL-based eviction
        self.cache.run_pending_tasks();
    }

    pub fn clear(&mut self) {
        self.cache.invalidate_all();
    }
}
