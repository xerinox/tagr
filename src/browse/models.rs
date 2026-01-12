//! Data models for browse functionality
//!
//! These are pure data structures with minimal logic. Conversions are handled
//! via From/TryFrom traits. Direct field access is used for comparisons and
//! filtering (idiomatic Rust style).

use crate::Pair;
use crate::db::{Database, DbError};
use crate::ui::DisplayItem;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

// ============================================================================
// Core Domain Types
// ============================================================================

/// Universal domain entity representing tags or files
///
/// This is the core type used throughout the business logic layer.
/// The name reflects its role as a fundamental domain model, not just UI.
///
/// # Design Philosophy
///
/// - **Data only**: No business logic methods
/// - **Direct access**: Use `item.metadata.created > other_time` not helper methods
/// - **Type safety**: Enum variants distinguish tags from files
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagrItem {
    /// Unique identifier (tag name for tags, path string for files)
    pub id: String,

    /// Display name (tag name or filename)
    pub name: String,

    /// Type-specific metadata
    pub metadata: ItemMetadata,
}

/// Type-specific metadata for tags or files
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemMetadata {
    /// Tag entity with statistics
    Tag(TagMetadata),

    /// File entity with tags and cached filesystem metadata
    File(FileMetadata),
}

/// Metadata for tag entities
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagMetadata {
    /// Number of files with this tag
    pub file_count: usize,
}

/// Metadata for file entities
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMetadata {
    /// Absolute file path
    pub path: PathBuf,

    /// Tags associated with this file (from database)
    pub tags: Vec<String>,

    /// Cached filesystem metadata
    pub cached: CachedMetadata,
}

/// Cached filesystem metadata
///
/// Mirrors Tagr's vtags cache structure. Has TTL to avoid excessive syscalls.
/// All fields are public for direct access (idiomatic Rust).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedMetadata {
    /// Whether the file exists on disk
    pub exists: bool,

    /// File size in bytes (None if file doesn't exist)
    pub size: Option<u64>,

    /// Last modified time (None if file doesn't exist)
    pub modified: Option<SystemTime>,

    /// Unix permissions mode (None on non-Unix or if file doesn't exist)
    #[cfg(unix)]
    pub permissions: Option<u32>,

    /// File extension (normalized to lowercase, without dot)
    pub extension: Option<String>,

    /// MIME type (if detectable)
    pub mime_type: Option<String>,

    /// When this metadata was cached
    pub cached_at: SystemTime,
}

// ============================================================================
// Selection State Types
// ============================================================================

/// Current state of a browse session
///
/// Tracks selections and search criteria as user progresses through workflow.
#[derive(Debug, Clone)]
pub struct SelectionState {
    /// Phase 1: Selected tags
    pub selected_tags: Vec<String>,

    /// Phase 2: Files matching tag selection (cached query result)
    pub available_files: Vec<TagrItem>,

    /// Phase 2: User-selected files
    pub selected_files: Vec<PathBuf>,

    /// Search mode (how to combine multiple tags)
    pub search_mode: SearchMode,

    /// Metadata cache for performance
    pub metadata_cache: MetadataCache,
}

/// How to combine multiple selected tags
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    /// Match files with ANY of the selected tags (OR logic)
    Any,

    /// Match files with ALL of the selected tags (AND logic)
    All,
}

/// Cache for file metadata to avoid repeated syscalls
///
/// Matches Tagr's vtags cache pattern but owned by browse session.
#[derive(Debug, Clone)]
pub struct MetadataCache {
    /// Cached metadata by file path
    entries: HashMap<PathBuf, CachedMetadata>,

    /// TTL for cache entries (default: 300 seconds)
    pub ttl: std::time::Duration,
}

/// Cache statistics
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheStats {
    pub total_entries: usize,
    pub expired_entries: usize,
    pub valid_entries: usize,
}

// ============================================================================
// Action Result Types
// ============================================================================

/// Result of executing a business logic action
///
/// Pure data structure with no presentation concerns (no colors, emojis).
/// UI layer converts this to formatted messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionOutcome {
    /// Action succeeded on all items
    Success {
        affected_count: usize,
        details: String,
    },

    /// Action succeeded on some items, failed on others
    Partial {
        succeeded: usize,
        failed: usize,
        errors: Vec<String>,
    },

    /// Action completely failed
    Failed(String),

    /// User cancelled the action
    Cancelled,

    /// Action requires additional user input
    NeedsInput {
        prompt: String,
        action_id: String,
        context: ActionContext,
    },

    /// Action requires confirmation
    NeedsConfirmation {
        message: String,
        action_id: String,
        context: ActionContext,
    },
}

/// Context for resumable actions
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionContext {
    /// Files to operate on
    pub files: Vec<PathBuf>,

    /// Additional data specific to the action
    pub data: ActionData,
}

/// Action-specific data
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionData {
    /// No additional data
    None,

    /// Tags operation (add/remove)
    Tags(Vec<String>),

    /// Copy operation
    CopyDestination(PathBuf),

    /// Custom command execution
    Command(String),

    /// Current search criteria for refinement
    SearchCriteria(SearchCriteriaData),
}

/// Search criteria data for refine search action
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchCriteriaData {
    /// Current included tags
    pub tags: Vec<String>,
    /// Current excluded tags
    pub exclude_tags: Vec<String>,
    /// Current file patterns
    pub file_patterns: Vec<String>,
    /// Current virtual tags
    pub virtual_tags: Vec<String>,
}

// ============================================================================
// Implementations - Construction
// ============================================================================

impl TagrItem {
    /// Create a tag item
    #[must_use]
    pub fn tag(name: String, file_count: usize) -> Self {
        Self {
            id: name.clone(),
            name,
            metadata: ItemMetadata::Tag(TagMetadata { file_count }),
        }
    }

    /// Create a file item
    #[must_use]
    pub fn file(path: PathBuf, tags: Vec<String>, cached: CachedMetadata) -> Self {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("<unknown>")
            .to_string();

        Self {
            id: path.display().to_string(),
            name,
            metadata: ItemMetadata::File(FileMetadata { path, tags, cached }),
        }
    }

    /// Get file path if this is a file item
    #[must_use]
    pub const fn as_file_path(&self) -> Option<&PathBuf> {
        match &self.metadata {
            ItemMetadata::File(FileMetadata { path, .. }) => Some(path),
            ItemMetadata::Tag(_) => None,
        }
    }

    /// Get tags if this is a file item
    #[must_use]
    pub fn file_tags(&self) -> Option<&[String]> {
        match &self.metadata {
            ItemMetadata::File(FileMetadata { tags, .. }) => Some(tags),
            ItemMetadata::Tag(_) => None,
        }
    }
}

impl crate::search::AsFileTagPair for TagrItem {
    fn as_pair(&self) -> crate::search::FileTagPair<'_> {
        match &self.metadata {
            ItemMetadata::File(FileMetadata { tags, .. }) => {
                crate::search::FileTagPair::new(&self.id, tags)
            }
            ItemMetadata::Tag(_) => {
                // Tags don't have associated files, return empty
                crate::search::FileTagPair::new(&self.id, &[])
            }
        }
    }
}

impl From<&Path> for CachedMetadata {
    fn from(path: &Path) -> Self {
        let exists = path.exists();
        let cached_at = SystemTime::now();

        if !exists {
            return Self {
                exists: false,
                size: None,
                modified: None,
                #[cfg(unix)]
                permissions: None,
                extension: None,
                mime_type: None,
                cached_at,
            };
        }

        let metadata = std::fs::metadata(path).ok();
        let size = metadata.as_ref().map(std::fs::Metadata::len);
        let modified = metadata.as_ref().and_then(|m| m.modified().ok());

        #[cfg(unix)]
        let permissions = {
            use std::os::unix::fs::PermissionsExt;
            metadata.as_ref().map(|m| m.permissions().mode())
        };

        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase);

        let mime_type = Self::detect_mime_type(path);

        Self {
            exists,
            size,
            modified,
            #[cfg(unix)]
            permissions,
            extension,
            mime_type,
            cached_at,
        }
    }
}

impl CachedMetadata {
    /// Check if cache has expired
    #[must_use]
    pub fn is_expired(&self, ttl: std::time::Duration) -> bool {
        SystemTime::now()
            .duration_since(self.cached_at)
            .map_or(true, |age| age > ttl)
    }

    /// Refresh metadata from filesystem
    pub fn refresh(&mut self, path: &Path) {
        *self = path.into();
    }

    fn detect_mime_type(path: &Path) -> Option<String> {
        // Simple extension-based detection
        path.extension()
            .and_then(|e| e.to_str())
            .and_then(|ext| match ext.to_lowercase().as_str() {
                "txt" => Some("text/plain"),
                "rs" => Some("text/x-rust"),
                "md" => Some("text/markdown"),
                "json" => Some("application/json"),
                "toml" => Some("application/toml"),
                "yaml" | "yml" => Some("application/yaml"),
                "png" => Some("image/png"),
                "jpg" | "jpeg" => Some("image/jpeg"),
                "gif" => Some("image/gif"),
                "svg" => Some("image/svg+xml"),
                _ => None,
            })
            .map(String::from)
    }
}

impl Default for CachedMetadata {
    fn default() -> Self {
        Self {
            exists: false,
            size: None,
            modified: None,
            #[cfg(unix)]
            permissions: None,
            extension: None,
            mime_type: None,
            cached_at: SystemTime::now(),
        }
    }
}

impl SelectionState {
    /// Create new empty state
    #[must_use]
    pub fn new() -> Self {
        Self {
            selected_tags: Vec::new(),
            available_files: Vec::new(),
            selected_files: Vec::new(),
            search_mode: SearchMode::Any,
            metadata_cache: MetadataCache::new(),
        }
    }

    /// Clear all selections
    pub fn clear(&mut self) {
        self.selected_tags.clear();
        self.available_files.clear();
        self.selected_files.clear();
    }
}

impl Default for SelectionState {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchMode {
    /// Get description for UI
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::Any => "ANY (files with any of these tags)",
            Self::All => "ALL (files with all of these tags)",
        }
    }

    /// Toggle between modes
    pub const fn toggle(&mut self) {
        *self = match self {
            Self::Any => Self::All,
            Self::All => Self::Any,
        };
    }
}

impl MetadataCache {
    /// Create new cache with default TTL (300s)
    #[must_use]
    pub fn new() -> Self {
        Self::with_ttl(std::time::Duration::from_secs(300))
    }

    /// Create cache with custom TTL
    #[must_use]
    pub fn with_ttl(ttl: std::time::Duration) -> Self {
        Self {
            entries: HashMap::new(),
            ttl,
        }
    }

    /// Get metadata from cache or fetch and cache it
    pub fn get_or_insert(&mut self, path: &Path) -> CachedMetadata {
        if let Some(cached) = self.entries.get(path)
            && !cached.is_expired(self.ttl)
        {
            return cached.clone();
        }

        let metadata: CachedMetadata = path.into();
        self.entries.insert(path.to_path_buf(), metadata.clone());
        metadata
    }

    /// Get metadata from cache without fetching
    #[must_use]
    pub fn get(&self, path: &Path) -> Option<&CachedMetadata> {
        self.entries.get(path).filter(|m| !m.is_expired(self.ttl))
    }

    /// Invalidate a specific entry
    pub fn invalidate(&mut self, path: &Path) {
        self.entries.remove(path);
    }

    /// Invalidate all entries
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Remove expired entries
    pub fn prune_expired(&mut self) {
        self.entries
            .retain(|_, metadata| !metadata.is_expired(self.ttl));
    }

    /// Get cache statistics
    #[must_use]
    pub fn stats(&self) -> CacheStats {
        let total = self.entries.len();
        let expired = self
            .entries
            .values()
            .filter(|m| m.is_expired(self.ttl))
            .count();

        CacheStats {
            total_entries: total,
            expired_entries: expired,
            valid_entries: total - expired,
        }
    }
}

impl Default for MetadataCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ActionOutcome {
    /// Check if outcome represents success
    #[must_use]
    pub const fn is_success(&self) -> bool {
        matches!(self, Self::Success { .. } | Self::Partial { .. })
    }

    /// Check if outcome requires user interaction
    #[must_use]
    pub const fn needs_user_input(&self) -> bool {
        matches!(
            self,
            Self::NeedsInput { .. } | Self::NeedsConfirmation { .. }
        )
    }

    /// Get affected count (if applicable)
    #[must_use]
    pub const fn affected_count(&self) -> Option<usize> {
        match self {
            Self::Success { affected_count, .. }
            | Self::Partial {
                succeeded: affected_count,
                ..
            } => Some(*affected_count),
            _ => None,
        }
    }
}

// ============================================================================
// Conversions - Database -> Domain Models
// ============================================================================

/// Context for converting Pair to `TagrItem`
pub struct PairWithCache<'a> {
    pub pair: Pair,
    pub cache: &'a mut MetadataCache,
}

/// Context for converting path to `TagrItem` with database lookup
pub struct PathWithDb<'a> {
    pub path: PathBuf,
    pub db: &'a Database,
    pub cache: &'a mut MetadataCache,
}

/// Context for converting tag name to `TagrItem` with database lookup
pub struct TagWithDb<'a> {
    pub tag: String,
    pub db: &'a Database,
}

/// Convert database Pair to `TagrItem` using cache
impl<'a> From<PairWithCache<'a>> for TagrItem {
    fn from(ctx: PairWithCache<'a>) -> Self {
        let cached = ctx.cache.get_or_insert(&ctx.pair.file);
        Self::file(ctx.pair.file, ctx.pair.tags, cached)
    }
}

/// Convert path with database context to `TagrItem`
impl<'a> TryFrom<PathWithDb<'a>> for TagrItem {
    type Error = DbError;

    fn try_from(ctx: PathWithDb<'a>) -> Result<Self, Self::Error> {
        let tags = ctx.db.get_tags(&ctx.path)?.unwrap_or_default();
        let cached = ctx.cache.get_or_insert(&ctx.path);
        Ok(Self::file(ctx.path, tags, cached))
    }
}

/// Convert tag name with database context to `TagrItem`
impl<'a> TryFrom<TagWithDb<'a>> for TagrItem {
    type Error = DbError;

    fn try_from(ctx: TagWithDb<'a>) -> Result<Self, Self::Error> {
        let file_count = ctx
            .db
            .find_by_tag(&ctx.tag)
            .map(|files| files.len())
            .unwrap_or(0);
        Ok(Self::tag(ctx.tag, file_count))
    }
}

// ============================================================================
// Conversions - Domain Models -> UI Types
// ============================================================================

/// Convert `TagrItem` to `DisplayItem` with basic formatting
impl From<&TagrItem> for DisplayItem {
    fn from(item: &TagrItem) -> Self {
        Self::new(item.id.clone(), item.name.clone(), item.name.clone())
    }
}

/// Convert `TagrItem` to `DisplayItem` (owned version)
impl From<TagrItem> for DisplayItem {
    fn from(item: TagrItem) -> Self {
        Self::from(&item)
    }
}

impl TagrItem {
    /// Convert to `DisplayItem` with detailed text (for tags: shows file count)
    ///
    /// This is a method rather than a From impl since it's a non-standard formatting
    #[must_use]
    pub fn to_display_item_detailed(&self) -> DisplayItem {
        match &self.metadata {
            ItemMetadata::Tag(TagMetadata { file_count }) => {
                let display = format!("{} ({})", self.name, file_count);
                DisplayItem::new(self.id.clone(), display.clone(), display)
            }
            ItemMetadata::File(_) => DisplayItem::from(self),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tagr_item_tag_creation() {
        let item = TagrItem::tag("rust".to_string(), 42);

        assert_eq!(item.id, "rust");
        assert_eq!(item.name, "rust");
        assert!(matches!(item.metadata, ItemMetadata::Tag(_)));

        if let ItemMetadata::Tag(TagMetadata { file_count }) = item.metadata {
            assert_eq!(file_count, 42);
        }
    }

    #[test]
    fn test_tagr_item_file_creation() {
        let path = PathBuf::from("/tmp/test.txt");
        let tags = vec!["rust".to_string(), "test".to_string()];
        let cached = CachedMetadata::default();

        let item = TagrItem::file(path.clone(), tags.clone(), cached);

        assert_eq!(item.id, "/tmp/test.txt");
        assert_eq!(item.name, "test.txt");
        assert_eq!(item.as_file_path(), Some(&path));
        assert_eq!(item.file_tags(), Some(tags.as_slice()));
    }

    #[test]
    fn test_cached_metadata_nonexistent() {
        let path = PathBuf::from("/nonexistent/file.txt");
        let cached: CachedMetadata = path.as_path().into();

        assert!(!cached.exists);
        assert_eq!(cached.size, None);
        assert_eq!(cached.modified, None);
    }

    #[test]
    fn test_cached_metadata_expiry() {
        let mut cached = CachedMetadata::default();
        let ttl = std::time::Duration::from_secs(1);

        assert!(!cached.is_expired(ttl));

        // Simulate old cache
        cached.cached_at = SystemTime::now() - std::time::Duration::from_secs(2);
        assert!(cached.is_expired(ttl));
    }

    #[test]
    fn test_metadata_cache_operations() {
        let mut cache = MetadataCache::with_ttl(std::time::Duration::from_secs(300));
        let path = PathBuf::from("/tmp/test.txt");

        let meta1 = cache.get_or_insert(&path);
        assert_eq!(cache.stats().total_entries, 1);

        let meta2 = cache.get_or_insert(&path);
        assert_eq!(meta1.cached_at, meta2.cached_at);

        cache.invalidate(&path);
        assert_eq!(cache.stats().total_entries, 0);
    }

    #[test]
    fn test_search_mode_toggle() {
        let mut mode = SearchMode::Any;
        assert_eq!(mode, SearchMode::Any);

        mode.toggle();
        assert_eq!(mode, SearchMode::All);

        mode.toggle();
        assert_eq!(mode, SearchMode::Any);
    }

    #[test]
    fn test_selection_state_clear() {
        let mut state = SelectionState::new();
        state.selected_tags.push("rust".to_string());
        state.selected_files.push(PathBuf::from("/tmp/test.txt"));

        state.clear();

        assert!(state.selected_tags.is_empty());
        assert!(state.selected_files.is_empty());
    }

    #[test]
    fn test_action_outcome_checks() {
        let success = ActionOutcome::Success {
            affected_count: 5,
            details: "Added tags".to_string(),
        };
        assert!(success.is_success());
        assert_eq!(success.affected_count(), Some(5));

        let failed = ActionOutcome::Failed("Error".to_string());
        assert!(!failed.is_success());
        assert_eq!(failed.affected_count(), None);

        let needs_input = ActionOutcome::NeedsInput {
            prompt: "Enter tags".to_string(),
            action_id: "add_tag".to_string(),
            context: ActionContext {
                files: vec![],
                data: ActionData::None,
            },
        };
        assert!(needs_input.needs_user_input());
    }

    #[test]
    fn test_mime_type_detection() {
        let test_cases = vec![
            (PathBuf::from("test.rs"), Some("text/x-rust")),
            (PathBuf::from("test.txt"), Some("text/plain")),
            (PathBuf::from("test.json"), Some("application/json")),
            (PathBuf::from("test.unknown"), None),
        ];

        for (path, expected) in test_cases {
            let detected = CachedMetadata::detect_mime_type(&path);
            assert_eq!(detected.as_deref(), expected, "Failed for {path:?}");
        }
    }

    #[test]
    fn test_idiomatic_field_access() {
        // Demonstrate idiomatic direct field access for comparisons
        let item1 = TagrItem::file(
            PathBuf::from("/tmp/old.txt"),
            vec![],
            CachedMetadata {
                modified: Some(SystemTime::UNIX_EPOCH),
                ..Default::default()
            },
        );

        let item2 = TagrItem::file(
            PathBuf::from("/tmp/new.txt"),
            vec![],
            CachedMetadata {
                modified: Some(SystemTime::now()),
                ..Default::default()
            },
        );

        // Direct field access for comparison - idiomatic Rust
        if let (ItemMetadata::File(meta1), ItemMetadata::File(meta2)) =
            (&item1.metadata, &item2.metadata)
        {
            assert!(meta1.cached.modified < meta2.cached.modified);
        }
    }

    #[test]
    fn test_from_trait_conversions() {
        let _db = crate::testing::TestDb::new("test_conversions");
        let mut cache = MetadataCache::new();

        let pair = crate::Pair {
            file: PathBuf::from("/tmp/test.txt"),
            tags: vec!["rust".to_string()],
        };

        let item = TagrItem::from(PairWithCache {
            pair,
            cache: &mut cache,
        });

        assert_eq!(item.name, "test.txt");
        assert_eq!(item.file_tags(), Some(&["rust".to_string()][..]));

        let display_item = DisplayItem::from(&item);
        assert_eq!(display_item.key, item.id);
        assert_eq!(display_item.display, item.name);

        let pairs = vec![
            crate::Pair {
                file: PathBuf::from("/tmp/file1.txt"),
                tags: vec!["tag1".to_string()],
            },
            crate::Pair {
                file: PathBuf::from("/tmp/file2.txt"),
                tags: vec!["tag2".to_string()],
            },
        ];

        let items: Vec<TagrItem> = pairs
            .into_iter()
            .map(|pair| {
                TagrItem::from(PairWithCache {
                    pair,
                    cache: &mut cache,
                })
            })
            .collect();

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name, "file1.txt");
        assert_eq!(items[1].name, "file2.txt");
    }
}
