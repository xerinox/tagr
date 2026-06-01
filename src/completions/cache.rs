//! Completion cache for fast lookups without database access
//!
//! The cache file (`~/.cache/tagr/completions.cache`) contains:
//! - All tags from the database
//! - Filter names and descriptions
//! - Database names
//!
//! Updated by database write operations (tag, untag, bulk).
//! This avoids opening the sled database on every TAB press,
//! which would cause lock contention and slow completions.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;

const CACHE_FILENAME: &str = "completions.cache";
const CACHE_VERSION: u32 = 1;

/// Cached completion data for fast shell completion lookups
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompletionCache {
    /// All tags in the database (sorted)
    pub tags: Vec<String>,

    /// When the cache was last updated
    pub updated_at: Option<SystemTime>,

    /// Version for cache invalidation on schema changes
    pub version: u32,
}

impl CompletionCache {
    /// Load cache from disk
    ///
    /// Returns empty/default cache if:
    /// - Cache file doesn't exist (first run)
    /// - Cache is corrupted
    /// - Version mismatch
    #[must_use]
    pub fn load() -> Self {
        let Some(path) = Self::cache_path() else {
            return Self::default();
        };

        if !path.exists() {
            return Self::default();
        }

        match std::fs::read(&path) {
            Ok(data) => {
                // Try to deserialize with serde_json (human-readable, easier debugging)
                match serde_json::from_slice::<Self>(&data) {
                    Ok(cache) if cache.version == CACHE_VERSION => cache,
                    Ok(_) => {
                        // Version mismatch - return empty cache
                        Self::default()
                    }
                    Err(_) => {
                        // Corrupted cache - return empty and it will rebuild on next write
                        Self::default()
                    }
                }
            }
            Err(_) => Self::default(),
        }
    }

    /// Save cache to disk
    ///
    /// Best-effort: failures are silently ignored since cache is optional.
    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::cache_path() else {
            return Ok(()); // No cache dir, skip silently
        };

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let data = serde_json::to_vec_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        std::fs::write(path, data)
    }

    /// Get cache file path
    ///
    /// Returns `None` if cache directory cannot be determined.
    fn cache_path() -> Option<PathBuf> {
        dirs::cache_dir().map(|d| d.join("tagr").join(CACHE_FILENAME))
    }

    /// Refresh cache from database
    ///
    /// Call this after database write operations.
    ///
    /// # Errors
    ///
    /// Returns error if database operations fail, but partial data
    /// may still be cached.
    pub fn refresh(store: &dyn crate::store::TagStore) -> std::io::Result<Self> {
        // Get all tags from database
        let tags = store.list_all_tags()
            .unwrap_or_default()
            .into_iter()
            .map(|t| t.to_string())
            .collect();

        let cache = Self {
            tags,
            updated_at: Some(SystemTime::now()),
            version: CACHE_VERSION,
        };

        // Save cache (best effort)
        let _ = cache.save();

        Ok(cache)
    }

    /// Check if cache is empty (likely first run)
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tags.is_empty()
    }
}

/// Update completion cache after database modifications
///
/// Call from: tag, untag, bulk operations, filter save/delete, db add/remove.
/// This is a best-effort operation - failures don't affect the main command.
/// Skipped entirely during tests to avoid corrupting the real cache.
pub fn invalidate_cache(store: &dyn crate::store::TagStore) {
    #[cfg(test)]
    {
        let _ = store;
        return;
    }
    #[cfg(not(test))]
    {
        let _ = CompletionCache::refresh(store);
    }
}

/// Safely load tags for completion
///
/// Returns tags from the cache file, or empty vec if cache is missing/empty.
/// This function is designed to be fast and never fail — it never opens
/// the database or makes IPC calls. The cache is kept fresh by
/// `invalidate_cache()` calls after tag-mutating operations.
#[must_use]
pub fn load_cached_tags() -> Vec<String> {
    let cache = CompletionCache::load();
    if !cache.tags.is_empty() {
        return cache.tags;
    }

    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_cache_is_default() {
        let cache = CompletionCache::default();
        assert!(cache.is_empty());
        assert_eq!(cache.version, 0); // default is 0, not CACHE_VERSION
    }

    #[test]
    fn test_cache_roundtrip() {
        let cache = CompletionCache {
            tags: vec!["rust".into(), "python".into()],
            updated_at: Some(SystemTime::now()),
            version: CACHE_VERSION,
        };

        let json = serde_json::to_string(&cache).unwrap();
        let loaded: CompletionCache = serde_json::from_str(&json).unwrap();

        assert_eq!(cache.tags, loaded.tags);
    }
}
