//! Completion cache for fast lookups without database access
//!
//! The cache file (`~/.cache/tagr/completions-<dbname>.cache`) stores
//! all tags from a specific database.
//!
//! Updated by database write operations (tag, untag, bulk).
//! Keyed by database name so switching databases doesn't serve stale data.
//! On first miss (no cache for the active DB), the database is opened once
//! to seed the cache; subsequent Tab presses read the file only.
//!
//! This avoids opening the sled database on every TAB press,
//! which would cause lock contention and slow completions.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;

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

    /// Database name this cache was built from
    #[serde(default)]
    pub db_name: String,
}

impl CompletionCache {
    /// Load cache from disk for a specific database
    ///
    /// Returns empty/default cache if the file doesn't exist, is corrupted,
    /// or has a version mismatch.
    #[must_use]
    pub fn load_for_db(db_name: &str) -> Self {
        let Some(path) = Self::cache_path_for_db(db_name) else {
            return Self::default();
        };

        if !path.exists() {
            return Self::default();
        }

        std::fs::read(&path)
            .ok()
            .and_then(|data| match serde_json::from_slice::<Self>(&data) {
                Ok(cache) if cache.version == CACHE_VERSION => Some(cache),
                _ => None,
            })
            .unwrap_or_default()
    }

    /// Load cache using the default database from config
    #[must_use]
    pub fn load() -> Self {
        let db_name = resolve_default_db_name().unwrap_or_default();
        Self::load_for_db(&db_name)
    }

    /// Save cache to disk (keyed by `self.db_name`)
    ///
    /// Best-effort: failures are silently ignored since cache is optional.
    ///
    /// # Errors
    ///
    /// Returns `io::Error` if the cache directory cannot be created or the
    /// file cannot be written.
    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::cache_path_for_db(&self.db_name) else {
            return Ok(());
        };

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let data = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;

        std::fs::write(path, data)
    }

    /// Cache file path for a specific database name
    fn cache_path_for_db(db_name: &str) -> Option<PathBuf> {
        let sanitized = db_name.replace(['/', '\\', '\0'], "_");
        let filename = if sanitized.is_empty() {
            "completions.cache".to_string()
        } else {
            format!("completions-{sanitized}.cache")
        };
        dirs::cache_dir().map(|d| d.join("tagr").join(filename))
    }

    /// Refresh cache from a store, resolving the DB name from config
    ///
    /// # Errors
    ///
    /// Returns `io::Error` if the cache cannot be saved.
    pub fn refresh(store: &dyn crate::store::TagStore) -> std::io::Result<Self> {
        let db_name = resolve_default_db_name().unwrap_or_default();
        Self::refresh_for_db(store, &db_name)
    }

    /// Refresh cache for a specific database name
    ///
    /// # Errors
    ///
    /// Returns `io::Error` if the cache cannot be saved.
    pub fn refresh_for_db(
        store: &dyn crate::store::TagStore,
        db_name: &str,
    ) -> std::io::Result<Self> {
        let tags = store
            .list_all_tags()
            .unwrap_or_default()
            .into_iter()
            .map(|t| t.to_string())
            .collect();

        let cache = Self {
            tags,
            updated_at: Some(SystemTime::now()),
            version: CACHE_VERSION,
            db_name: db_name.to_string(),
        };

        let _ = cache.save();
        Ok(cache)
    }

    /// Check if cache is empty (likely first run)
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.tags.is_empty()
    }
}

/// Update completion cache after database modifications
///
/// Call from: tag, untag, bulk operations, filter save/delete, db add/remove.
/// Best-effort — failures don't affect the main command.
/// Skipped entirely during tests to avoid corrupting the real cache.
pub fn invalidate_cache(store: &dyn crate::store::TagStore) {
    #[cfg(test)]
    {
        let _ = store;
    }
    #[cfg(not(test))]
    {
        let _ = CompletionCache::refresh(store);
    }
}

/// Load tags for completion
///
/// Reads tags from the DB-specific cache file. On first miss (no cache for
/// the active database), opens the database once to seed the cache.
/// Returns empty vec as last resort.
#[must_use]
pub fn load_cached_tags() -> Vec<String> {
    let db_name = resolve_default_db_name().unwrap_or_default();
    let cache = CompletionCache::load_for_db(&db_name);

    if !cache.tags.is_empty() {
        return cache.tags;
    }

    // Cold start: seed the cache from the live database (one-time cost)
    seed_cache_from_db(&db_name).unwrap_or_default()
}

/// Resolve the default database name from config.
fn resolve_default_db_name() -> Option<String> {
    crate::config::TagrConfig::load()
        .ok()
        .and_then(|c| c.get_default_database().map(ToString::to_string))
}

/// Open the database and build the initial cache. Only called once per DB
/// when no cache file exists yet.
fn seed_cache_from_db(db_name: &str) -> Option<Vec<String>> {
    let config = crate::config::TagrConfig::load().ok()?;
    let db_path = config.get_database(db_name)?;
    let store = crate::store::DirectStore::open(db_path).ok()?;

    let cache = CompletionCache::refresh_for_db(&store, db_name).ok()?;
    Some(cache.tags)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_cache_is_default() {
        let cache = CompletionCache::default();
        assert!(cache.is_empty());
        assert_eq!(cache.version, 0);
    }

    #[test]
    fn test_cache_roundtrip() {
        let cache = CompletionCache {
            tags: vec!["rust".into(), "python".into()],
            updated_at: Some(SystemTime::now()),
            version: CACHE_VERSION,
            db_name: "test-db".into(),
        };

        let json = serde_json::to_string(&cache).unwrap();
        let loaded: CompletionCache = serde_json::from_str(&json).unwrap();

        assert_eq!(cache.tags, loaded.tags);
        assert_eq!(cache.db_name, loaded.db_name);
    }

    #[test]
    fn test_cache_path_sanitization() {
        let path = CompletionCache::cache_path_for_db("my/db");
        assert!(path.is_some());
        let p = path.unwrap();
        assert!(p.file_name().unwrap().to_str().unwrap().contains("my_db"));
    }

    #[test]
    fn test_empty_db_name_uses_default_filename() {
        let path = CompletionCache::cache_path_for_db("");
        assert!(path.is_some());
        let p = path.unwrap();
        assert_eq!(p.file_name().unwrap(), "completions.cache");
    }
}
