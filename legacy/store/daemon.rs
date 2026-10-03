//! `DaemonStore` — IPC-backed `TagStore` with widest-result cache.
//!
//! Wraps [`PersistentClient`](crate::daemon::client::PersistentClient) behind
//! the [`TagStore`](super::TagStore) trait. A **widest-result cache** makes
//! narrowing filter operations instant (local iteration) while widening triggers
//! an IPC query to the daemon.
//!
//! # Cache Design
//!
//! The cache holds the result of the least-restrictive query seen so far.
//! When the TUI toggles a tag filter:
//!
//! - **Narrower** criteria (more tags selected) → filter `widest_data` locally
//! - **Wider** criteria (fewer tags, or different dimensions) → IPC query, replace cache
//!
//! Server-push events (`FileTagged`, `NoteChanged`, etc.) are drained between
//! TUI frames via [`drain_events()`](DaemonStore::drain_events) and applied
//! incrementally — no full re-query needed.
//!
//! # Lock Discipline
//!
//! - `RwLock<Option<QueryCache>>` — read lock for narrow path, write lock for widen/events
//! - `Mutex<mpsc::Receiver>` — brief lock to drain events
//! - **Never** hold a lock across I/O (IPC, filesystem)
//! - **Never** nest locks (`event_rx` + cache simultaneously)
//!
//! # Thread Safety
//!
//! `DaemonStore` is `Send + Sync` because `PersistentClient` uses `Arc<Mutex<...>>`
//! internally and `Runtime` is thread-safe. The `block_on()` calls are serialized
//! per-connection.

use std::collections::HashMap;
use std::sync::{Mutex, RwLock};

use tokio::sync::mpsc;

use crate::daemon::client::PersistentClient;
use crate::daemon::traits::DaemonError;
use crate::ipc::wire::{Request, Response, ServerEvent, WireQueryCriteria};
use crate::schema::types::TagSchema;
use crate::types::{NoteRecord, Pair, QueryCriteria, TagName, TagrPath};

use super::{Result, StoreError, TagStore};

/// Cached query result — the widest (least restrictive) query seen so far.
///
/// Narrowing operations filter `widest_data` locally; widening replaces the
/// entire cache with a fresh IPC result.
struct QueryCache {
    /// The least restrictive query criteria we've issued.
    widest_criteria: QueryCriteria,
    /// Full result set from that query (file + tags pairs).
    widest_data: Vec<Pair>,
    /// All notes for the result set — updated incrementally via events.
    notes: HashMap<TagrPath, NoteRecord>,
}

impl QueryCache {
    /// Apply a server-push event to the cache incrementally.
    ///
    /// Avoids full re-queries for most mutations. `ConfigReloaded` is the
    /// exception — it invalidates everything because schema/expansions may differ.
    fn apply_event(&mut self, event: ServerEvent) {
        match event {
            ServerEvent::FileTagged { file, tags } => {
                let Ok(path) = TagrPath::new(&file) else {
                    return;
                };
                let tag_names: Vec<TagName> =
                    tags.iter().filter_map(|t| TagName::new(t).ok()).collect();

                if let Some(pair) = self.widest_data.iter_mut().find(|p| p.file == path) {
                    pair.tags = tag_names;
                } else {
                    self.widest_data.push(Pair::new(path, tag_names));
                }
            }
            ServerEvent::FileUntagged { file, tags } => {
                let Ok(path) = TagrPath::new(&file) else {
                    return;
                };
                let removed: std::collections::HashSet<&str> =
                    tags.iter().map(String::as_str).collect();

                if let Some(pair) = self.widest_data.iter_mut().find(|p| p.file == path) {
                    pair.tags.retain(|t| !removed.contains(t.as_str()));
                }
            }
            ServerEvent::FileRemoved { file } => {
                let Ok(path) = TagrPath::new(&file) else {
                    return;
                };
                self.widest_data.retain(|p| p.file != path);
                self.notes.remove(&path);
            }
            ServerEvent::NoteChanged { file, content } => {
                let Ok(path) = TagrPath::new(&file) else {
                    return;
                };
                match content {
                    Some(c) => {
                        self.notes.insert(path, NoteRecord::new(c));
                    }
                    None => {
                        self.notes.remove(&path);
                    }
                }
            }
            ServerEvent::ConfigReloaded => {
                self.widest_data.clear();
                self.notes.clear();
                self.widest_criteria = QueryCriteria::default();
            }
        }
    }
}

/// IPC-backed tag store with widest-result cache.
///
/// Created via [`DaemonStore::connect()`], which establishes a persistent
/// connection and subscribes to server-push events. The event receiver is
/// owned internally — call [`drain_events()`](Self::drain_events) between
/// TUI frames to apply incremental updates.
pub struct DaemonStore {
    rt: tokio::runtime::Runtime,
    client: PersistentClient,
    /// `None` before first query, `Some(...)` after initialization.
    cache: RwLock<Option<QueryCache>>,
    /// Server-push event receiver — drained by [`drain_events()`](Self::drain_events).
    event_rx: Mutex<mpsc::Receiver<ServerEvent>>,
}

impl std::fmt::Debug for DaemonStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DaemonStore").finish_non_exhaustive()
    }
}

impl DaemonStore {
    /// Connect to the daemon and subscribe to events.
    ///
    /// The event receiver is kept internally for cache updates via
    /// [`drain_events()`](Self::drain_events). The caller no longer
    /// receives it — `DaemonStore` owns the full event lifecycle.
    ///
    /// # Errors
    ///
    /// Returns `StoreError::ConnectionLost` if the runtime cannot be created
    /// or the daemon is unreachable.
    pub fn connect() -> std::result::Result<Self, StoreError> {
        let rt = tokio::runtime::Runtime::new().map_err(|e| StoreError::ConnectionLost {
            context: format!("failed to create tokio runtime: {e}"),
        })?;

        let (client, event_rx) = rt
            .block_on(PersistentClient::connect())
            .map_err(|e| map_daemon_error(&e, "connecting to daemon"))?;

        Ok(Self {
            rt,
            client,
            cache: RwLock::new(None),
            event_rx: Mutex::new(event_rx),
        })
    }

    /// Create a `DaemonStore` from pre-existing runtime and client.
    ///
    /// Creates a dummy event channel since no subscription exists.
    #[must_use]
    pub fn from_parts(rt: tokio::runtime::Runtime, client: PersistentClient) -> Self {
        let (_tx, rx) = mpsc::channel(1);
        Self {
            rt,
            client,
            cache: RwLock::new(None),
            event_rx: Mutex::new(rx),
        }
    }

    /// Drain pending server events and update the cache.
    ///
    /// Called by the TUI between render frames. Non-blocking — processes
    /// whatever events are available, then returns immediately.
    ///
    /// Lock discipline: acquires `event_rx` mutex briefly, releases it,
    /// then acquires `cache` write lock if events were drained. Never nested.
    pub fn drain_events(&self) {
        let events: Vec<ServerEvent> = {
            let Ok(mut rx) = self.event_rx.lock() else {
                return;
            };
            std::iter::from_fn(|| rx.try_recv().ok()).collect()
        }; // Mutex drops

        if events.is_empty() {
            return;
        }

        let Ok(mut cache) = self.cache.write() else {
            return;
        };
        if let Some(ref mut qc) = *cache {
            for event in events {
                qc.apply_event(event);
            }
        }
    }

    /// Send an IPC request and return the response.
    fn send(&self, req: Request) -> std::result::Result<Response, StoreError> {
        self.rt
            .block_on(self.client.request(req))
            .map_err(|e| map_daemon_error(&e, "sending IPC request"))
    }
}

/// Create a `StoreError` for a poisoned lock.
fn poison_error() -> StoreError {
    StoreError::ConnectionLost {
        context: "internal lock poisoned".to_string(),
    }
}

/// Map `DaemonError` to `StoreError` — never leak IPC implementation details.
fn map_daemon_error(err: &DaemonError, context: &str) -> StoreError {
    match err {
        DaemonError::ConnectionFailed(msg) | DaemonError::StartFailed(msg) => {
            StoreError::ConnectionLost {
                context: format!("{context}: {msg}"),
            }
        }
        DaemonError::IpcError(ipc_err) => StoreError::ConnectionLost {
            context: format!("{context}: {ipc_err}"),
        },
        DaemonError::IoError(io_err) => StoreError::IoFailed {
            context: context.to_string(),
            source: std::io::Error::new(io_err.kind(), io_err.to_string()),
        },
    }
}

/// Convert wire note entry to `NoteRecord`.
fn wire_note_to_record(entry: &crate::ipc::wire::WireNoteEntry) -> NoteRecord {
    let mut note = NoteRecord::new(entry.content.clone());
    note.metadata.created_at = entry.created_at;
    note.metadata.updated_at = entry.updated_at;
    note
}

/// Convert a response error string to `StoreError`.
fn response_error(msg: &str, context: &str) -> StoreError {
    StoreError::ConnectionLost {
        context: format!("{context}: daemon returned error: {msg}"),
    }
}

/// Convert an unexpected response variant to `StoreError`.
fn unexpected_response(resp: &Response, context: &str) -> StoreError {
    StoreError::ConnectionLost {
        context: format!("{context}: unexpected response: {resp:?}"),
    }
}

/// Parse a raw path string from the wire into `TagrPath`.
fn wire_path_to_tagrpath(path: &str, context: &str) -> Result<TagrPath> {
    TagrPath::new(path).map_err(|e| StoreError::StorageCorrupted {
        key: context.to_string(),
        reason: format!("invalid path from daemon: '{path}': {e}"),
    })
}

/// Parse a raw tag string from the wire into `TagName`.
fn wire_tag_to_tagname(tag: &str, context: &str) -> Result<TagName> {
    TagName::new(tag).map_err(|e| StoreError::StorageCorrupted {
        key: context.to_string(),
        reason: format!("invalid tag from daemon: '{tag}': {e}"),
    })
}

/// Convert a `Vec<String>` of file paths from the wire to `Vec<TagrPath>`.
fn wire_paths_to_tagrpaths(paths: &[String], context: &str) -> Result<Vec<TagrPath>> {
    paths
        .iter()
        .map(|p| wire_path_to_tagrpath(p, context))
        .collect()
}

impl TagStore for DaemonStore {
    fn list_all_tags(&self) -> Result<Vec<TagName>> {
        match self.send(Request::ListTags)? {
            Response::Tags(tags) => tags
                .iter()
                .map(|t| wire_tag_to_tagname(&t.name, "list_all_tags"))
                .collect(),
            Response::Error(e) => Err(response_error(&e, "list_all_tags")),
            other => Err(unexpected_response(&other, "list_all_tags")),
        }
    }

    #[allow(clippy::cast_possible_truncation)] // wire uses u64, file counts won't exceed usize
    fn list_tags_with_counts(&self) -> Result<Vec<(TagName, usize)>> {
        match self.send(Request::ListTags)? {
            Response::Tags(tags) => tags
                .iter()
                .map(|t| {
                    wire_tag_to_tagname(&t.name, "list_tags_with_counts")
                        .map(|name| (name, t.file_count as usize))
                })
                .collect(),
            Response::Error(e) => Err(response_error(&e, "list_tags_with_counts")),
            other => Err(unexpected_response(&other, "list_tags_with_counts")),
        }
    }

    fn list_all_files(&self) -> Result<Vec<TagrPath>> {
        match self.send(Request::ListAllPaths)? {
            Response::FilePaths(paths) => wire_paths_to_tagrpaths(&paths, "list_all_files"),
            Response::Error(e) => Err(response_error(&e, "list_all_files")),
            other => Err(unexpected_response(&other, "list_all_files")),
        }
    }

    fn list_all(&self) -> Result<Vec<Pair>> {
        match self.send(Request::ListFiles)? {
            Response::Files(pairs) => pairs
                .iter()
                .map(|p| {
                    let file = wire_path_to_tagrpath(&p.file, "list_all")?;
                    let tags = p
                        .tags
                        .iter()
                        .map(|t| wire_tag_to_tagname(t, "list_all"))
                        .collect::<Result<Vec<_>>>()?;
                    Ok(Pair::new(file, tags))
                })
                .collect(),
            Response::Error(e) => Err(response_error(&e, "list_all")),
            other => Err(unexpected_response(&other, "list_all")),
        }
    }

    fn get_tags(&self, file: &TagrPath) -> Result<Option<Vec<TagName>>> {
        let req = Request::GetTags {
            file: file.to_string(),
        };
        match self.send(req)? {
            Response::FileTags(tags) if tags.is_empty() => Ok(None),
            Response::FileTags(tags) => {
                let names = tags
                    .iter()
                    .map(|t| wire_tag_to_tagname(t, "get_tags"))
                    .collect::<Result<Vec<_>>>()?;
                Ok(Some(names))
            }
            Response::Error(e) => Err(response_error(&e, "get_tags")),
            other => Err(unexpected_response(&other, "get_tags")),
        }
    }

    fn find_by_tag(&self, tag: &TagName) -> Result<Vec<TagrPath>> {
        let req = Request::FindByTag {
            tag: tag.to_string(),
        };
        match self.send(req)? {
            Response::FilePaths(paths) => wire_paths_to_tagrpaths(&paths, "find_by_tag"),
            Response::Error(e) => Err(response_error(&e, "find_by_tag")),
            other => Err(unexpected_response(&other, "find_by_tag")),
        }
    }

    fn find_by_all_tags(&self, tags: &[TagName]) -> Result<Vec<TagrPath>> {
        let req = Request::FindByTags {
            tags: tags.iter().map(ToString::to_string).collect(),
            match_all: true,
        };
        match self.send(req)? {
            Response::FilePaths(paths) => wire_paths_to_tagrpaths(&paths, "find_by_all_tags"),
            Response::Error(e) => Err(response_error(&e, "find_by_all_tags")),
            other => Err(unexpected_response(&other, "find_by_all_tags")),
        }
    }

    fn find_by_any_tag(&self, tags: &[TagName]) -> Result<Vec<TagrPath>> {
        let req = Request::FindByTags {
            tags: tags.iter().map(ToString::to_string).collect(),
            match_all: false,
        };
        match self.send(req)? {
            Response::FilePaths(paths) => wire_paths_to_tagrpaths(&paths, "find_by_any_tag"),
            Response::Error(e) => Err(response_error(&e, "find_by_any_tag")),
            other => Err(unexpected_response(&other, "find_by_any_tag")),
        }
    }

    fn find_by_tag_regex(&self, pattern: &str) -> Result<Vec<TagrPath>> {
        let req = Request::FindByTagRegex {
            pattern: pattern.to_owned(),
        };
        match self.send(req)? {
            Response::FilePaths(paths) => wire_paths_to_tagrpaths(&paths, "find_by_tag_regex"),
            Response::Error(e) => Err(response_error(&e, "find_by_tag_regex")),
            other => Err(unexpected_response(&other, "find_by_tag_regex")),
        }
    }

    fn tag_exists(&self, tag: &TagName) -> Result<bool> {
        // No dedicated wire request — check if find_by_tag returns any files
        let files = self.find_by_tag(tag)?;
        Ok(!files.is_empty())
    }

    fn find_tags_by_prefix(&self, prefix: &TagName) -> Result<Vec<TagName>> {
        // No dedicated wire request — filter list_all_tags locally
        let all = self.list_all_tags()?;
        let prefix_str = prefix.as_str();
        Ok(all
            .into_iter()
            .filter(|t| t.as_str().starts_with(prefix_str))
            .collect())
    }

    fn insert(&self, file: &TagrPath, tags: Vec<TagName>) -> Result<()> {
        let req = Request::SetTags {
            file: file.to_string(),
            tags: tags.into_iter().map(|t| t.to_string()).collect(),
        };
        match self.send(req)? {
            Response::Ok => Ok(()),
            Response::Error(e) => Err(response_error(&e, "insert")),
            other => Err(unexpected_response(&other, "insert")),
        }
    }

    fn add_tags(&self, file: &TagrPath, tags: Vec<TagName>) -> Result<()> {
        let req = Request::AddTags {
            file: file.to_string(),
            tags: tags.into_iter().map(|t| t.to_string()).collect(),
        };
        match self.send(req)? {
            Response::Ok => Ok(()),
            Response::Error(e) => Err(response_error(&e, "add_tags")),
            other => Err(unexpected_response(&other, "add_tags")),
        }
    }

    fn remove_tags(&self, file: &TagrPath, tags: &[TagName]) -> Result<()> {
        let req = Request::RemoveTags {
            file: file.to_string(),
            tags: tags.iter().map(ToString::to_string).collect(),
            all: false,
        };
        match self.send(req)? {
            Response::Ok => Ok(()),
            Response::Error(e) => Err(response_error(&e, "remove_tags")),
            other => Err(unexpected_response(&other, "remove_tags")),
        }
    }

    fn remove_file(&self, file: &TagrPath) -> Result<bool> {
        let req = Request::DeleteFromDb {
            file: file.to_string(),
        };
        match self.send(req)? {
            Response::Ok => Ok(true),
            Response::Error(e) if e.contains("not found") => Ok(false),
            Response::Error(e) => Err(response_error(&e, "remove_file")),
            other => Err(unexpected_response(&other, "remove_file")),
        }
    }

    fn remove_tag_globally(&self, tag: &TagName) -> Result<u64> {
        // No dedicated wire request — remove tag from each file individually
        let files = self.find_by_tag(tag)?;
        let count = files.len() as u64;
        for file in &files {
            self.remove_tags(file, std::slice::from_ref(tag))?;
        }
        Ok(count)
    }

    fn get_note(&self, file: &TagrPath) -> Result<Option<NoteRecord>> {
        // Try cache first — avoids IPC when cache is populated
        {
            let cache = self.cache.read().map_err(|_| poison_error())?;
            if let Some(ref qc) = *cache {
                return Ok(qc.notes.get(file).cloned());
            }
        }
        // Fallback to IPC if cache not initialized
        let req = Request::GetNote {
            file: file.to_string(),
        };
        match self.send(req)? {
            Response::Note(Some(entry)) => Ok(Some(wire_note_to_record(&entry))),
            Response::Note(None) => Ok(None),
            Response::Error(e) => Err(response_error(&e, "get_note")),
            other => Err(unexpected_response(&other, "get_note")),
        }
    }

    fn set_note(&self, file: &TagrPath, note: &NoteRecord) -> Result<()> {
        let req = Request::SetNote {
            file: file.to_string(),
            content: note.content.clone(),
        };
        match self.send(req)? {
            Response::Ok => Ok(()),
            Response::Error(e) => Err(response_error(&e, "set_note")),
            other => Err(unexpected_response(&other, "set_note")),
        }
    }

    fn delete_note(&self, file: &TagrPath) -> Result<bool> {
        let req = Request::DeleteNote {
            file: file.to_string(),
        };
        match self.send(req)? {
            Response::Ok => Ok(true),
            Response::Error(e) => Err(response_error(&e, "delete_note")),
            other => Err(unexpected_response(&other, "delete_note")),
        }
    }

    fn list_all_notes(&self) -> Result<Vec<(TagrPath, NoteRecord)>> {
        // Use cache if populated
        {
            let cache = self.cache.read().map_err(|_| poison_error())?;
            if let Some(ref qc) = *cache {
                return Ok(qc
                    .notes
                    .iter()
                    .map(|(path, note)| (path.clone(), note.clone()))
                    .collect());
            }
        }
        // Fallback to IPC
        match self.send(Request::ListNotes)? {
            Response::Notes(entries) => entries
                .iter()
                .map(|e| {
                    let path = wire_path_to_tagrpath(&e.path, "list_all_notes")?;
                    Ok((path, wire_note_to_record(e)))
                })
                .collect(),
            Response::Error(e) => Err(response_error(&e, "list_all_notes")),
            other => Err(unexpected_response(&other, "list_all_notes")),
        }
    }

    fn search_notes(&self, query: &str) -> Result<Vec<(TagrPath, NoteRecord)>> {
        // Use cache if populated — filter locally
        {
            let cache = self.cache.read().map_err(|_| poison_error())?;
            if let Some(ref qc) = *cache {
                let query_lower = query.to_lowercase();
                return Ok(qc
                    .notes
                    .iter()
                    .filter(|(_, n)| n.content.to_lowercase().contains(&query_lower))
                    .map(|(path, note)| (path.clone(), note.clone()))
                    .collect());
            }
        }
        // Fallback to IPC + local filter
        let all = self.list_all_notes()?;
        let query_lower = query.to_lowercase();
        Ok(all
            .into_iter()
            .filter(|(_, n)| n.content.to_lowercase().contains(&query_lower))
            .collect())
    }

    fn query(&self, criteria: &QueryCriteria, _schema: &TagSchema) -> Result<Vec<TagrPath>> {
        // Narrow path — filter cached widest result locally
        {
            let cache = self.cache.read().map_err(|_| poison_error())?;
            if let Some(ref qc) = *cache
                && criteria.is_narrower_than(&qc.widest_criteria)
            {
                return Ok(qc
                    .widest_data
                    .iter()
                    .filter(|p| criteria.matches_pair(p))
                    .map(|p| p.file.clone())
                    .collect());
            }
        } // read lock drops

        // Widen path — IPC outside any lock
        let wire_criteria = WireQueryCriteria::from(criteria);
        let req = Request::Query {
            criteria: wire_criteria,
        };
        let pairs: Vec<Pair> = match self.send(req)? {
            Response::Files(wire_pairs) => wire_pairs
                .iter()
                .map(|p| {
                    let file = wire_path_to_tagrpath(&p.file, "query")?;
                    let tags = p
                        .tags
                        .iter()
                        .map(|t| wire_tag_to_tagname(t, "query"))
                        .collect::<Result<Vec<_>>>()?;
                    Ok(Pair::new(file, tags))
                })
                .collect::<Result<Vec<Pair>>>()?,
            Response::Error(e) => return Err(response_error(&e, "query")),
            other => return Err(unexpected_response(&other, "query")),
        };

        let result_paths: Vec<TagrPath> = pairs.iter().map(|p| p.file.clone()).collect();

        // Fetch notes on first cache population (two IPC calls on init)
        let is_first_load = {
            let cache = self.cache.read().map_err(|_| poison_error())?;
            cache.is_none()
        };
        let note_data = if is_first_load {
            match self.send(Request::ListNotes) {
                Ok(Response::Notes(entries)) => entries
                    .iter()
                    .filter_map(|e| {
                        let path = wire_path_to_tagrpath(&e.path, "cache_notes").ok()?;
                        Some((path, wire_note_to_record(e)))
                    })
                    .collect(),
                _ => HashMap::new(),
            }
        } else {
            HashMap::new()
        };

        // Brief write lock to swap cache
        {
            let mut cache = self.cache.write().map_err(|_| poison_error())?;
            match *cache {
                Some(ref mut qc) => {
                    qc.widest_criteria = criteria.clone();
                    qc.widest_data = pairs;
                }
                None => {
                    *cache = Some(QueryCache {
                        widest_criteria: criteria.clone(),
                        widest_data: pairs,
                        notes: note_data,
                    });
                }
            }
        }

        Ok(result_paths)
    }
}
