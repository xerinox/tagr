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

/// Invalidate cache for filter changes only
///
/// No-op now as filters are not cached. Kept for API compatibility.
pub fn invalidate_filter_cache() {
    // No-op: filters are loaded dynamically in completers
}

/// Invalidate cache for database configuration changes
///
/// No-op now as databases are not cached. Kept for API compatibility.
pub fn invalidate_database_cache() {
    // No-op: databases are loaded dynamically in completers
}

/// Safely load tags for completion
///
/// Returns empty vec on any error (missing config, no DB, first run, etc.)
/// This function is designed to be fast and never fail.
#[must_use]
pub fn load_cached_tags() -> Vec<String> {
    // Try cache first (fast path)
    let cache = CompletionCache::load();
    if !cache.tags.is_empty() {
        return cache.tags;
    }

    // Cache miss - try loading from database
    // This is slower but handles first-run after tagging
    if let Ok(tags) = try_load_from_database() {
        if !tags.is_empty() {
            return tags;
        }
    }

    // No data available - return empty (first run, no tags yet)
    Vec::new()
}

/// Attempt to load tags directly from database
///
/// Used as fallback when cache is empty/missing.
/// If the DB is locked (daemon running), falls back to IPC.
fn try_load_from_database() -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
    use crate::config::TagrConfig;
    use crate::db::Database;

    let config = TagrConfig::load()?;

    let db_path = config
        .get_default_database()
        .and_then(|name| config.get_database(name).cloned())
        .or_else(|| dirs::data_local_dir().map(|d| d.join("tagr").join("default")));

    let Some(db_path) = db_path else {
        return Ok(Vec::new());
    };

    if !db_path.exists() {
        return Ok(Vec::new());
    }

    // Try direct DB access first; if locked, fall back to IPC
    match Database::open(&db_path) {
        Ok(db) => {
            let tags = db.list_all_tags()?;
            let store = crate::store::DirectStore::new(db);
            let _ = CompletionCache::refresh(&store);
            Ok(tags)
        }
        Err(_) => try_load_via_ipc(),
    }
}

/// Load tags via IPC when the daemon holds the DB lock.
fn try_load_via_ipc() -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
    use crate::daemon::client::send_request;
    use crate::ipc::wire::{Request, Response};

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let response = rt.block_on(send_request(Request::ListTags))?;
    match response {
        Response::Tags(tags) => Ok(tags.into_iter().map(|t| t.name).collect()),
        Response::Error(e) => Err(e.into()),
        _ => Ok(Vec::new()),
    }
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
