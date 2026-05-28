//! `DaemonStore` — IPC-backed `TagStore` implementation.
//!
//! Wraps [`PersistentClient`](crate::daemon::client::PersistentClient) behind
//! the [`TagStore`](super::TagStore) trait. Every method does a synchronous
//! `block_on()` IPC round-trip to the daemon.
//!
//! This is the pass-through implementation — no local caching. A future iteration
//! will add a widest-result cache (`RwLock<QueryCache>`) for instant TUI filtering.
//!
//! # Thread Safety
//!
//! `DaemonStore` is `Send + Sync` because `PersistentClient` uses `Arc<Mutex<...>>`
//! internally and `Runtime` is thread-safe. The `block_on()` calls are serialized
//! per-connection.

use tokio::sync::mpsc;

use crate::daemon::client::PersistentClient;
use crate::daemon::traits::DaemonError;
use crate::ipc::wire::{Request, Response, ServerEvent, WireSearchParams};
use crate::schema::types::TagSchema;
use crate::types::{NoteRecord, Pair, QueryCriteria, TagExpr, TagName, TagrPath};

use super::{Result, StoreError, TagStore};

/// IPC-backed tag store — delegates all operations to the watch daemon.
///
/// Created via [`DaemonStore::connect()`], which establishes a persistent
/// connection and subscribes to server-push events.
pub struct DaemonStore {
    rt: tokio::runtime::Runtime,
    client: PersistentClient,
}

impl std::fmt::Debug for DaemonStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DaemonStore").finish_non_exhaustive()
    }
}

impl DaemonStore {
    /// Connect to the daemon and subscribe to events.
    ///
    /// Returns the store and a channel receiver for [`ServerEvent`] push
    /// notifications. The caller (usually `main.rs`) owns the event channel
    /// and passes it to the TUI event loop.
    ///
    /// # Errors
    ///
    /// Returns `StoreError::ConnectionLost` if the runtime cannot be created
    /// or the daemon is unreachable.
    pub fn connect() -> std::result::Result<(Self, mpsc::Receiver<ServerEvent>), StoreError> {
        let rt = tokio::runtime::Runtime::new().map_err(|e| StoreError::ConnectionLost {
            context: format!("failed to create tokio runtime: {e}"),
        })?;

        let (client, event_rx) = rt
            .block_on(PersistentClient::connect())
            .map_err(|e| map_daemon_error(&e, "connecting to daemon"))?;

        Ok((Self { rt, client }, event_rx))
    }

    /// Create a `DaemonStore` from pre-existing runtime and client.
    ///
    /// Used during the `DataSource` → `TagStore` migration to wrap legacy
    /// `DataSource::Remote { rt, client }` values without reconnecting.
    #[must_use]
    pub fn from_parts(rt: tokio::runtime::Runtime, client: PersistentClient) -> Self {
        Self { rt, client }
    }

    /// Send an IPC request and return the response.
    fn send(&self, req: Request) -> std::result::Result<Response, StoreError> {
        self.rt
            .block_on(self.client.request(req))
            .map_err(|e| map_daemon_error(&e, "sending IPC request"))
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
fn response_error(msg: String, context: &str) -> StoreError {
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
fn wire_paths_to_tagrpaths(paths: Vec<String>, context: &str) -> Result<Vec<TagrPath>> {
    paths.iter().map(|p| wire_path_to_tagrpath(p, context)).collect()
}

impl TagStore for DaemonStore {
    fn list_all_tags(&self) -> Result<Vec<TagName>> {
        match self.send(Request::ListTags)? {
            Response::Tags(tags) => tags
                .iter()
                .map(|t| wire_tag_to_tagname(&t.name, "list_all_tags"))
                .collect(),
            Response::Error(e) => Err(response_error(e, "list_all_tags")),
            other => Err(unexpected_response(&other, "list_all_tags")),
        }
    }

    fn list_tags_with_counts(&self) -> Result<Vec<(TagName, usize)>> {
        match self.send(Request::ListTags)? {
            Response::Tags(tags) => tags
                .iter()
                .map(|t| {
                    wire_tag_to_tagname(&t.name, "list_tags_with_counts")
                        .map(|name| (name, t.file_count as usize))
                })
                .collect(),
            Response::Error(e) => Err(response_error(e, "list_tags_with_counts")),
            other => Err(unexpected_response(&other, "list_tags_with_counts")),
        }
    }

    fn list_all_files(&self) -> Result<Vec<TagrPath>> {
        match self.send(Request::ListAllPaths)? {
            Response::FilePaths(paths) => wire_paths_to_tagrpaths(paths, "list_all_files"),
            Response::Error(e) => Err(response_error(e, "list_all_files")),
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
            Response::Error(e) => Err(response_error(e, "list_all")),
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
            Response::Error(e) => Err(response_error(e, "get_tags")),
            other => Err(unexpected_response(&other, "get_tags")),
        }
    }

    fn find_by_tag(&self, tag: &TagName) -> Result<Vec<TagrPath>> {
        let req = Request::FindByTag {
            tag: tag.to_string(),
        };
        match self.send(req)? {
            Response::FilePaths(paths) => wire_paths_to_tagrpaths(paths, "find_by_tag"),
            Response::Error(e) => Err(response_error(e, "find_by_tag")),
            other => Err(unexpected_response(&other, "find_by_tag")),
        }
    }

    fn find_by_all_tags(&self, tags: &[TagName]) -> Result<Vec<TagrPath>> {
        let req = Request::FindByTags {
            tags: tags.iter().map(ToString::to_string).collect(),
            match_all: true,
        };
        match self.send(req)? {
            Response::FilePaths(paths) => wire_paths_to_tagrpaths(paths, "find_by_all_tags"),
            Response::Error(e) => Err(response_error(e, "find_by_all_tags")),
            other => Err(unexpected_response(&other, "find_by_all_tags")),
        }
    }

    fn find_by_any_tag(&self, tags: &[TagName]) -> Result<Vec<TagrPath>> {
        let req = Request::FindByTags {
            tags: tags.iter().map(ToString::to_string).collect(),
            match_all: false,
        };
        match self.send(req)? {
            Response::FilePaths(paths) => wire_paths_to_tagrpaths(paths, "find_by_any_tag"),
            Response::Error(e) => Err(response_error(e, "find_by_any_tag")),
            other => Err(unexpected_response(&other, "find_by_any_tag")),
        }
    }

    fn find_by_tag_regex(&self, pattern: &str) -> Result<Vec<TagrPath>> {
        let req = Request::FindByTagRegex {
            pattern: pattern.to_owned(),
        };
        match self.send(req)? {
            Response::FilePaths(paths) => wire_paths_to_tagrpaths(paths, "find_by_tag_regex"),
            Response::Error(e) => Err(response_error(e, "find_by_tag_regex")),
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
            Response::Error(e) => Err(response_error(e, "insert")),
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
            Response::Error(e) => Err(response_error(e, "add_tags")),
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
            Response::Error(e) => Err(response_error(e, "remove_tags")),
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
            Response::Error(e) => Err(response_error(e, "remove_file")),
            other => Err(unexpected_response(&other, "remove_file")),
        }
    }

    fn remove_tag_globally(&self, tag: &TagName) -> Result<u64> {
        // No dedicated wire request — remove tag from each file individually
        let files = self.find_by_tag(tag)?;
        let count = files.len() as u64;
        for file in &files {
            self.remove_tags(file, &[tag.clone()])?;
        }
        Ok(count)
    }

    fn get_note(&self, file: &TagrPath) -> Result<Option<NoteRecord>> {
        let req = Request::GetNote {
            file: file.to_string(),
        };
        match self.send(req)? {
            Response::Note(Some(entry)) => Ok(Some(wire_note_to_record(&entry))),
            Response::Note(None) => Ok(None),
            Response::Error(e) => Err(response_error(e, "get_note")),
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
            Response::Error(e) => Err(response_error(e, "set_note")),
            other => Err(unexpected_response(&other, "set_note")),
        }
    }

    fn delete_note(&self, file: &TagrPath) -> Result<bool> {
        let req = Request::DeleteNote {
            file: file.to_string(),
        };
        match self.send(req)? {
            Response::Ok => Ok(true),
            Response::Error(e) => Err(response_error(e, "delete_note")),
            other => Err(unexpected_response(&other, "delete_note")),
        }
    }

    fn list_all_notes(&self) -> Result<Vec<(TagrPath, NoteRecord)>> {
        match self.send(Request::ListNotes)? {
            Response::Notes(entries) => entries
                .iter()
                .map(|e| {
                    let path = wire_path_to_tagrpath(&e.path, "list_all_notes")?;
                    Ok((path, wire_note_to_record(e)))
                })
                .collect(),
            Response::Error(e) => Err(response_error(e, "list_all_notes")),
            other => Err(unexpected_response(&other, "list_all_notes")),
        }
    }

    fn search_notes(&self, query: &str) -> Result<Vec<(TagrPath, NoteRecord)>> {
        // Filter client-side until the wire protocol gains a SearchNotes request (Phase 5.7)
        let all = self.list_all_notes()?;
        let query_lower = query.to_lowercase();
        Ok(all
            .into_iter()
            .filter(|(_, n)| n.content.to_lowercase().contains(&query_lower))
            .collect())
    }

    fn query(&self, criteria: &QueryCriteria, _schema: &TagSchema) -> Result<Vec<TagrPath>> {
        // Pass-through: convert QueryCriteria to WireSearchParams and send over IPC.
        // Future: DaemonStore will send QueryCriteria directly when the wire protocol
        // is upgraded to support it. For now, use the legacy WireSearchParams conversion.
        let wire_params = wire_search_params_from_criteria(criteria);
        let req = Request::SearchFiles {
            params: wire_params,
        };
        match self.send(req)? {
            Response::Files(pairs) => pairs
                .iter()
                .map(|p| wire_path_to_tagrpath(&p.file, "query"))
                .collect(),
            Response::Error(e) => Err(response_error(e, "query")),
            other => Err(unexpected_response(&other, "query")),
        }
    }
}

/// Build a `WireSearchParams` from `QueryCriteria` (lossy conversion).
///
/// This is temporary — the wire protocol will eventually accept `QueryCriteria`
/// directly. For now, extract the flat include/exclude tags and file patterns.
fn wire_search_params_from_criteria(criteria: &QueryCriteria) -> WireSearchParams {
    use crate::ipc::wire::WireSearchMode;

    let include_tags: Vec<String> = criteria
        .flat_include_tags()
        .unwrap_or_default()
        .into_iter()
        .map(ToString::to_string)
        .collect();

    let exclude_tags: Vec<String> = criteria
        .flat_exclude_tags()
        .unwrap_or_default()
        .into_iter()
        .map(ToString::to_string)
        .collect();

    // Determine tag mode from the tag expression structure:
    // And([...]) → All, Or([...]) → Any, single Tag → All (irrelevant for one tag)
    let tag_mode = match &criteria.tag_expr {
        Some(TagExpr::Or(_)) => WireSearchMode::Any,
        _ => WireSearchMode::All,
    };

    WireSearchParams {
        query: criteria.query.clone(),
        tags: include_tags,
        file_patterns: criteria.file_patterns.clone(),
        exclude_tags,
        virtual_tags: criteria.virtual_tags.clone(),
        tag_mode,
        file_mode: WireSearchMode::All,
        virtual_mode: WireSearchMode::All,
        no_hierarchy: !criteria.expand_hierarchy,
        regex_tag: criteria.regex_tags,
        regex_file: criteria.regex_files,
        glob_files: !criteria.regex_files,
    }
}
