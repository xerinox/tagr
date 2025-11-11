use std::collections::HashMap;
use std::fs::Permissions;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

pub struct MetadataCache {
    cache: HashMap<PathBuf, CachedEntry>,
    ttl: Duration,
}

struct CachedEntry {
    metadata: FileMetadata,
    cached_at: Instant,
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
    pub fn new(ttl: Duration) -> Self {
        Self {
            cache: HashMap::new(),
            ttl,
        }
    }

    pub fn get(&mut self, path: &Path) -> io::Result<FileMetadata> {
        let now = Instant::now();

        if let Some(entry) = self.cache.get(path) {
            if now.duration_since(entry.cached_at) < self.ttl {
                return Ok(entry.metadata.clone());
            }
        }

        let metadata = Self::fetch_metadata(path)?;

        self.cache.insert(
            path.to_path_buf(),
            CachedEntry {
                metadata: metadata.clone(),
                cached_at: now,
            },
        );

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
        let now = Instant::now();
        self.cache
            .retain(|_, entry| now.duration_since(entry.cached_at) < self.ttl);
    }

    pub fn clear(&mut self) {
        self.cache.clear();
    }
}
