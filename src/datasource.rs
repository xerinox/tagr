//! Unified data access layer for both direct database and IPC-proxied operations.
//!
//! `DataSource` abstracts away whether data comes from a local sled database
//! or from the watch daemon over IPC. This lets `BrowseSession` and other
//! consumers work identically in both modes.

use std::path::{Path, PathBuf};

use crate::db::{Database, DbError, NoteRecord};
use crate::ipc::IpcError;
use crate::ipc::wire::{Request, Response, WireTagInfo};
use crate::Pair;

/// Errors that can occur during data source operations.
#[derive(Debug, thiserror::Error)]
pub enum DataSourceError {
    #[error("Database error: {0}")]
    Db(#[from] DbError),

    #[error("IPC error: {0}")]
    Ipc(#[from] IpcError),

    #[error("Unexpected response from daemon: {0}")]
    UnexpectedResponse(String),
}

pub type Result<T> = std::result::Result<T, DataSourceError>;

/// Unified data access — either direct database calls or daemon IPC.
pub enum DataSource {
    /// Local database access (no daemon running).
    Direct(Database),

    /// Remote access via IPC to the watch daemon.
    Remote {
        rt: tokio::runtime::Runtime,
    },
}

impl std::fmt::Debug for DataSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Direct(_) => f.debug_tuple("Direct").field(&"<Database>").finish(),
            Self::Remote { .. } => f.debug_struct("Remote").finish_non_exhaustive(),
        }
    }
}

impl DataSource {
    /// Create a direct data source wrapping an open database.
    pub fn direct(db: Database) -> Self {
        Self::Direct(db)
    }

    /// Create a remote data source that talks to the daemon over IPC.
    ///
    /// # Errors
    /// Returns an error if the tokio runtime cannot be created.
    pub fn remote() -> std::result::Result<Self, std::io::Error> {
        let rt = tokio::runtime::Runtime::new()?;
        Ok(Self::Remote { rt })
    }

    /// Get reference to the inner database, if this is a direct data source.
    ///
    /// Returns `None` for remote data sources. Used by the TUI preview
    /// system which needs direct database access for note previews.
    #[must_use]
    pub const fn as_database(&self) -> Option<&Database> {
        match self {
            Self::Direct(db) => Some(db),
            Self::Remote { .. } => None,
        }
    }

    // -- Queries --

    /// List all tags with their file counts.
    pub fn list_tags(&self) -> Result<Vec<WireTagInfo>> {
        match self {
            Self::Direct(db) => {
                let tag_names = db.list_all_tags()?;
                let tags = tag_names
                    .into_iter()
                    .map(|name| {
                        let file_count = db.find_by_tag(&name).map_or(0, |f| f.len()) as u64;
                        WireTagInfo { name, file_count }
                    })
                    .collect();
                Ok(tags)
            }
            Self::Remote { rt } => match self.send(rt, Request::ListTags)? {
                Response::Tags(tags) => Ok(tags),
                other => Err(unexpected(&other)),
            },
        }
    }

    /// List all tag names (without counts).
    pub fn list_all_tags(&self) -> Result<Vec<String>> {
        match self {
            Self::Direct(db) => Ok(db.list_all_tags()?),
            Self::Remote { .. } => {
                Ok(self.list_tags()?.into_iter().map(|t| t.name).collect())
            }
        }
    }

    /// List all file-tag pairs.
    pub fn list_all(&self) -> Result<Vec<Pair>> {
        match self {
            Self::Direct(db) => Ok(db.list_all()?),
            Self::Remote { rt } => match self.send(rt, Request::ListFiles)? {
                Response::Files(pairs) => {
                    Ok(pairs.into_iter().map(Pair::from).collect())
                }
                other => Err(unexpected(&other)),
            },
        }
    }

    /// List all file paths (without tags).
    pub fn list_all_files(&self) -> Result<Vec<PathBuf>> {
        match self {
            Self::Direct(db) => Ok(db.list_all_files()?),
            Self::Remote { rt } => match self.send(rt, Request::ListAllPaths)? {
                Response::FilePaths(paths) => {
                    Ok(paths.into_iter().map(PathBuf::from).collect())
                }
                other => Err(unexpected(&other)),
            },
        }
    }

    /// Get tags for a single file.
    pub fn get_tags<P: AsRef<Path>>(&self, file: P) -> Result<Option<Vec<String>>> {
        match self {
            Self::Direct(db) => Ok(db.get_tags(file)?),
            Self::Remote { rt } => {
                let req = Request::GetTags {
                    file: file.as_ref().to_string_lossy().into_owned(),
                };
                match self.send(rt, req)? {
                    Response::FileTags(tags) if tags.is_empty() => Ok(None),
                    Response::FileTags(tags) => Ok(Some(tags)),
                    other => Err(unexpected(&other)),
                }
            }
        }
    }

    /// Find files with a specific tag.
    pub fn find_by_tag(&self, tag: &str) -> Result<Vec<PathBuf>> {
        match self {
            Self::Direct(db) => Ok(db.find_by_tag(tag)?),
            Self::Remote { rt } => {
                let req = Request::FindByTag {
                    tag: tag.to_owned(),
                };
                match self.send(rt, req)? {
                    Response::FilePaths(paths) => {
                        Ok(paths.into_iter().map(PathBuf::from).collect())
                    }
                    other => Err(unexpected(&other)),
                }
            }
        }
    }

    /// Find files matching all given tags.
    pub fn find_by_all_tags(&self, tags: &[String]) -> Result<Vec<PathBuf>> {
        match self {
            Self::Direct(db) => Ok(db.find_by_all_tags(tags)?),
            Self::Remote { rt } => {
                let req = Request::FindByTags {
                    tags: tags.to_vec(),
                    match_all: true,
                };
                match self.send(rt, req)? {
                    Response::FilePaths(paths) => {
                        Ok(paths.into_iter().map(PathBuf::from).collect())
                    }
                    other => Err(unexpected(&other)),
                }
            }
        }
    }

    /// Find files matching any of the given tags.
    pub fn find_by_any_tag(&self, tags: &[String]) -> Result<Vec<PathBuf>> {
        match self {
            Self::Direct(db) => Ok(db.find_by_any_tag(tags)?),
            Self::Remote { rt } => {
                let req = Request::FindByTags {
                    tags: tags.to_vec(),
                    match_all: false,
                };
                match self.send(rt, req)? {
                    Response::FilePaths(paths) => {
                        Ok(paths.into_iter().map(PathBuf::from).collect())
                    }
                    other => Err(unexpected(&other)),
                }
            }
        }
    }

    /// Find files with tags matching a regex.
    pub fn find_by_tag_regex(&self, pattern: &str) -> Result<Vec<PathBuf>> {
        match self {
            Self::Direct(db) => Ok(db.find_by_tag_regex(pattern)?),
            Self::Remote { rt } => {
                let req = Request::FindByTagRegex {
                    pattern: pattern.to_owned(),
                };
                match self.send(rt, req)? {
                    Response::FilePaths(paths) => {
                        Ok(paths.into_iter().map(PathBuf::from).collect())
                    }
                    other => Err(unexpected(&other)),
                }
            }
        }
    }

    /// List all files that have notes.
    pub fn list_all_notes(&self) -> Result<Vec<(PathBuf, NoteRecord)>> {
        match self {
            Self::Direct(db) => Ok(db.list_all_notes()?),
            Self::Remote { rt } => match self.send(rt, Request::ListNotes)? {
                Response::Notes(entries) => {
                    Ok(entries
                        .into_iter()
                        .map(|e| {
                            (
                                PathBuf::from(e.path),
                                NoteRecord::new(e.content),
                            )
                        })
                        .collect())
                }
                other => Err(unexpected(&other)),
            },
        }
    }

    // -- Mutations --

    /// Replace all tags for a file (insert/overwrite semantics).
    pub fn insert<P: AsRef<Path>>(&self, file: P, tags: Vec<String>) -> Result<()> {
        match self {
            Self::Direct(db) => Ok(db.insert(file, tags)?),
            Self::Remote { rt } => {
                let req = Request::SetTags {
                    file: file.as_ref().to_string_lossy().into_owned(),
                    tags,
                };
                match self.send(rt, req)? {
                    Response::Ok => Ok(()),
                    Response::Error(e) => Err(DataSourceError::UnexpectedResponse(e)),
                    other => Err(unexpected(&other)),
                }
            }
        }
    }

    /// Remove a file entry from the database.
    pub fn remove<P: AsRef<Path>>(&self, file: P) -> Result<bool> {
        match self {
            Self::Direct(db) => Ok(db.remove(file)?),
            Self::Remote { rt } => {
                let req = Request::DeleteFromDb {
                    file: file.as_ref().to_string_lossy().into_owned(),
                };
                match self.send(rt, req)? {
                    Response::Ok => Ok(true),
                    Response::Error(e) if e.contains("not found") => Ok(false),
                    Response::Error(e) => Err(DataSourceError::UnexpectedResponse(e)),
                    other => Err(unexpected(&other)),
                }
            }
        }
    }

    // -- Notes --

    /// Get a note for a specific file.
    pub fn get_note<P: AsRef<Path>>(&self, file: P) -> Result<Option<NoteRecord>> {
        match self {
            Self::Direct(db) => Ok(db.get_note(file)?),
            Self::Remote { rt } => {
                let req = Request::GetNote {
                    file: file.as_ref().to_string_lossy().into_owned(),
                };
                match self.send(rt, req)? {
                    Response::Note(Some(entry)) => Ok(Some(NoteRecord::new(entry.content))),
                    Response::Note(None) => Ok(None),
                    other => Err(unexpected(&other)),
                }
            }
        }
    }

    /// Set a note for a specific file.
    pub fn set_note<P: AsRef<Path>>(&self, file: P, note: &NoteRecord) -> Result<()> {
        match self {
            Self::Direct(db) => Ok(db.set_note(file, note)?),
            Self::Remote { rt } => {
                let req = Request::SetNote {
                    file: file.as_ref().to_string_lossy().into_owned(),
                    content: note.content.clone(),
                };
                match self.send(rt, req)? {
                    Response::Ok => Ok(()),
                    Response::Error(e) => Err(DataSourceError::UnexpectedResponse(e)),
                    other => Err(unexpected(&other)),
                }
            }
        }
    }

    /// Delete a note for a specific file.
    pub fn delete_note<P: AsRef<Path>>(&self, file: P) -> Result<bool> {
        match self {
            Self::Direct(db) => Ok(db.delete_note(file)?),
            Self::Remote { rt } => {
                let req = Request::DeleteNote {
                    file: file.as_ref().to_string_lossy().into_owned(),
                };
                match self.send(rt, req)? {
                    Response::Ok => Ok(true),
                    Response::Error(e) => Err(DataSourceError::UnexpectedResponse(e)),
                    other => Err(unexpected(&other)),
                }
            }
        }
    }

    // -- Search --

    /// Apply search parameters and return matching file paths.
    ///
    /// For Direct mode, delegates to `db::query::apply_search_params`.
    /// For Remote mode, sends a `SearchFiles` IPC request.
    pub fn apply_search_params(&self, params: &crate::cli::SearchParams) -> Result<Vec<PathBuf>> {
        match self {
            Self::Direct(db) => {
                Ok(crate::db::query::apply_search_params(db, params)?)
            }
            Self::Remote { rt } => {
                let wire_params = crate::ipc::wire::WireSearchParams::from(params);
                let req = Request::SearchFiles { params: wire_params };
                match self.send(rt, req)? {
                    Response::Files(pairs) => {
                        Ok(pairs.into_iter().map(|p| PathBuf::from(p.file)).collect())
                    }
                    other => Err(unexpected(&other)),
                }
            }
        }
    }

    // -- Internal --

    /// Send a typed IPC request and return the response.
    fn send(
        &self,
        rt: &tokio::runtime::Runtime,
        req: Request,
    ) -> std::result::Result<Response, IpcError> {
        use crate::daemon::client::send_request;

        rt.block_on(send_request(req)).map_err(|e| {
            IpcError::RemoteError(e.to_string())
        })
    }
}

fn unexpected(resp: &Response) -> DataSourceError {
    DataSourceError::UnexpectedResponse(format!("Unexpected IPC response: {resp:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_direct_list_tags_empty_db() {
        let temp = tempfile::tempdir().unwrap();
        let db = Database::open(temp.path()).unwrap();
        let ds = DataSource::direct(db);

        let tags = ds.list_tags().unwrap();
        assert!(tags.is_empty());
    }

    #[test]
    fn test_direct_get_tags_missing_file() {
        let temp = tempfile::tempdir().unwrap();
        let db = Database::open(temp.path()).unwrap();
        let ds = DataSource::direct(db);

        let result = ds.get_tags("/nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_direct_insert_and_get() {
        let temp = tempfile::tempdir().unwrap();
        let db = Database::open(temp.path()).unwrap();
        let ds = DataSource::direct(db);

        let file = temp.path().join("test.txt");
        std::fs::write(&file, "content").unwrap();

        ds.insert(&file, vec!["a".into(), "b".into()]).unwrap();

        let tags = ds.get_tags(&file).unwrap().unwrap();
        assert_eq!(tags.len(), 2);
        assert!(tags.contains(&"a".to_string()));
        assert!(tags.contains(&"b".to_string()));
    }

    #[test]
    fn test_direct_find_by_tag() {
        let temp = tempfile::tempdir().unwrap();
        let db = Database::open(temp.path()).unwrap();
        let ds = DataSource::direct(db);

        let a = temp.path().join("a.txt");
        let b = temp.path().join("b.txt");
        std::fs::write(&a, "a").unwrap();
        std::fs::write(&b, "b").unwrap();

        ds.insert(&a, vec!["rust".into()]).unwrap();
        ds.insert(&b, vec!["rust".into(), "wasm".into()]).unwrap();

        let found = ds.find_by_tag("rust").unwrap();
        assert_eq!(found.len(), 2);

        let wasm = ds.find_by_tag("wasm").unwrap();
        assert_eq!(wasm.len(), 1);
    }

    #[test]
    fn test_direct_remove() {
        let temp = tempfile::tempdir().unwrap();
        let db = Database::open(temp.path()).unwrap();
        let ds = DataSource::direct(db);

        let file = temp.path().join("rm.txt");
        std::fs::write(&file, "content").unwrap();

        ds.insert(&file, vec!["tag".into()]).unwrap();
        assert!(ds.remove(&file).unwrap());
        assert!(!ds.remove(&file).unwrap());
    }

    #[test]
    fn test_direct_list_all() {
        let temp = tempfile::tempdir().unwrap();
        let db = Database::open(temp.path()).unwrap();
        let ds = DataSource::direct(db);

        let x = temp.path().join("x.txt");
        let y = temp.path().join("y.txt");
        std::fs::write(&x, "x").unwrap();
        std::fs::write(&y, "y").unwrap();

        ds.insert(&x, vec!["t1".into()]).unwrap();
        ds.insert(&y, vec!["t2".into()]).unwrap();

        let pairs = ds.list_all().unwrap();
        assert_eq!(pairs.len(), 2);
    }
}
