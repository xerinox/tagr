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

    /// Filter names with descriptions
    pub filters: Vec<(String, Option<String>)>,

    /// Database names (name, is_default)
    pub databases: Vec<(String, bool)>,

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

    /// Refresh cache from database and config
    ///
    /// Call this after database write operations.
    ///
    /// # Errors
    ///
    /// Returns error if database operations fail, but partial data
    /// may still be cached.
    pub fn refresh(db: &crate::db::Database) -> std::io::Result<Self> {
        use crate::config::TagrConfig;
        use crate::filters::{FilterManager, get_filter_path};

        // Get all tags from database
        let tags = db.list_all_tags().unwrap_or_default();

        // Get filter names and descriptions
        let filters = get_filter_path()
            .ok()
            .and_then(|path| {
                let manager = FilterManager::new(path);
                manager.list().ok()
            })
            .map(|filter_list| {
                filter_list
                    .iter()
                    .map(|f| {
                        let desc = if f.description.is_empty() {
                            None
                        } else {
                            Some(f.description.clone())
                        };
                        (f.name.clone(), desc)
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Get database names
        let databases = TagrConfig::load()
            .map(|c| {
                let default = c.get_default_database().cloned();
                c.list_databases()
                    .iter()
                    .map(|name| {
                        let is_default = default.as_ref() == Some(name);
                        ((*name).clone(), is_default)
                    })
                    .collect()
            })
            .unwrap_or_default();

        let cache = Self {
            tags,
            filters,
            databases,
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
        self.tags.is_empty() && self.filters.is_empty() && self.databases.is_empty()
    }
}

/// Update completion cache after database modifications
///
/// Call from: tag, untag, bulk operations, filter save/delete, db add/remove.
/// This is a best-effort operation - failures don't affect the main command.
pub fn invalidate_cache(db: &crate::db::Database) {
    // Best effort - don't fail the main operation if cache update fails
    let _ = CompletionCache::refresh(db);
}

/// Invalidate cache for filter changes only
///
/// Call from: filter create/delete/rename/import.
/// Lighter weight than full refresh - only updates filter portion.
pub fn invalidate_filter_cache() {
    let mut cache = CompletionCache::load();

    // Reload only filters
    if let Some(filters) = crate::filters::get_filter_path()
        .ok()
        .and_then(|path| {
            let manager = crate::filters::FilterManager::new(path);
            manager.list().ok()
        })
        .map(|filter_list| {
            filter_list
                .iter()
                .map(|f| {
                    let desc = if f.description.is_empty() {
                        None
                    } else {
                        Some(f.description.clone())
                    };
                    (f.name.clone(), desc)
                })
                .collect()
        })
    {
        cache.filters = filters;
        cache.updated_at = Some(SystemTime::now());
        let _ = cache.save();
    }
}

/// Invalidate cache for database configuration changes
///
/// Call from: db add/remove/set-default.
/// Lighter weight than full refresh - only updates database portion.
pub fn invalidate_database_cache() {
    use crate::config::TagrConfig;

    let mut cache = CompletionCache::load();

    // Reload only databases
    if let Ok(config) = TagrConfig::load() {
        let default = config.get_default_database().cloned();
        cache.databases = config
            .list_databases()
            .iter()
            .map(|name| {
                let is_default = default.as_ref() == Some(name);
                ((*name).clone(), is_default)
            })
            .collect();
        cache.updated_at = Some(SystemTime::now());
        let _ = cache.save();
    }
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
fn try_load_from_database() -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
    use crate::config::TagrConfig;
    use crate::db::Database;

    // Check if config exists
    let config = TagrConfig::load()?;

    let db_path = config
        .get_default_database()
        .and_then(|name| config.get_database(name).cloned())
        .or_else(|| dirs::data_local_dir().map(|d| d.join("tagr").join("default")));

    let Some(db_path) = db_path else {
        return Ok(Vec::new()); // No database configured
    };

    if !db_path.exists() {
        return Ok(Vec::new()); // Database doesn't exist yet
    }

    // Try to open database
    let db = Database::open(&db_path)?;
    let tags = db.list_all_tags()?;

    // Update cache for next time
    let _ = CompletionCache::refresh(&db);

    Ok(tags)
}

/// Safely load filter names for completion
#[must_use]
pub fn load_cached_filters() -> Vec<(String, Option<String>)> {
    let cache = CompletionCache::load();
    if !cache.filters.is_empty() {
        return cache.filters;
    }

    // Fallback: load directly from filter manager
    crate::filters::get_filter_path()
        .ok()
        .and_then(|path| {
            let manager = crate::filters::FilterManager::new(path);
            manager.list().ok()
        })
        .map(|filter_list| {
            filter_list
                .iter()
                .map(|f| {
                    let desc = if f.description.is_empty() {
                        None
                    } else {
                        Some(f.description.clone())
                    };
                    (f.name.clone(), desc)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Safely load database names for completion
#[must_use]
pub fn load_cached_databases() -> Vec<(String, bool)> {
    let cache = CompletionCache::load();
    if !cache.databases.is_empty() {
        return cache.databases;
    }

    // Fallback: load directly from config
    crate::config::TagrConfig::load()
        .map(|c| {
            let default = c.get_default_database().cloned();
            c.list_databases()
                .iter()
                .map(|name| {
                    let is_default = default.as_ref() == Some(name);
                    ((*name).clone(), is_default)
                })
                .collect()
        })
        .unwrap_or_default()
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
            filters: vec![("my-filter".into(), Some("description".into()))],
            databases: vec![("default".into(), true)],
            updated_at: Some(SystemTime::now()),
            version: CACHE_VERSION,
        };

        let json = serde_json::to_string(&cache).unwrap();
        let loaded: CompletionCache = serde_json::from_str(&json).unwrap();

        assert_eq!(cache.tags, loaded.tags);
        assert_eq!(cache.filters, loaded.filters);
        assert_eq!(cache.databases, loaded.databases);
    }
}
