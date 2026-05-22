//! Inter-process communication for daemon-client data exchange.
//!
//! The IPC protocol uses typed request/response messages so the daemon
//! deals only in data — the CLI/TUI client handles all presentation.
//!
//! ## Modules
//!
//! - [`wire`] — Binary framed protocol (wincode + length-prefixed frames)
//!   for persistent bidirectional connections.

use crate::Pair;
use crate::cli::SearchParams;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

pub mod wire;

#[derive(Debug, Error)]
pub enum IpcError {
    #[error("IPC connection failed: {0}")]
    ConnectionFailed(String),

    #[error("IPC request timeout")]
    Timeout,

    #[error("IPC serialization error: {0}")]
    SerializationError(String),

    #[error("IPC response error: {0}")]
    RemoteError(String),

    #[error("Daemon not responding")]
    DaemonNotResponding,
}

pub type Result<T> = std::result::Result<T, IpcError>;

/// Typed IPC requests — the daemon matches on these and returns data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcRequest {
    /// Health check
    Ping,
    /// Graceful shutdown
    Shutdown,

    // -- Queries --
    /// List all tags with file counts
    ListTags,
    /// List all file-tag pairs
    ListFiles,
    /// List just file paths (no tags)
    ListAllPaths,
    /// Search files matching criteria
    SearchFiles { params: SearchParams },
    /// Get tags for a single file
    GetTags { file: PathBuf },
    /// Find files with a specific tag
    FindByTag { tag: String },
    /// Find files matching multiple tags
    FindByTags { tags: Vec<String>, match_all: bool },
    /// Find files with tags matching a regex
    FindByTagRegex { pattern: String },
    /// List all files that have notes
    ListNotes,

    // -- Mutations --
    /// Add tags to a file
    AddTags { file: PathBuf, tags: Vec<String> },
    /// Replace all tags for a file
    SetTags { file: PathBuf, tags: Vec<String> },
    /// Remove tags from a file (`all = true` removes every tag)
    RemoveTags { file: PathBuf, tags: Vec<String>, all: bool },
    /// Delete a file entry from the database
    DeleteFromDb { file: PathBuf },
    /// Run cleanup (remove missing files / empty tag entries)
    Cleanup,
}

/// Tag with its associated file count.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TagInfo {
    pub name: String,
    pub file_count: usize,
}

/// Typed IPC responses — structured data, never formatted text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcResponse {
    /// Ping reply
    Pong,
    /// Mutation succeeded (no payload)
    Ok,
    /// List of tags with counts
    Tags(Vec<TagInfo>),
    /// List of file-tag pairs
    Files(Vec<Pair>),
    /// Tags for a single file
    FileTags(Vec<String>),
    /// List of file paths (no tags attached)
    FilePaths(Vec<PathBuf>),
    /// Notes with their file paths
    Notes(Vec<NoteEntry>),
    /// Cleanup result
    CleanupResult { removed: usize },
    /// Error with description
    Error(String),
}

/// A note entry with its associated file path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteEntry {
    pub path: PathBuf,
    pub note: crate::db::NoteRecord,
}

impl IpcResponse {
    /// Check if response indicates success (non-error)
    pub fn is_success(&self) -> bool {
        !matches!(self, IpcResponse::Error(_))
    }
}

/// Get the IPC socket path for this platform
pub fn get_ipc_socket_path() -> Result<PathBuf> {
    #[cfg(unix)]
    {
        // Use runtime directory on Unix systems
        let runtime_dir = dirs::runtime_dir()
            .ok_or_else(|| IpcError::ConnectionFailed("No runtime directory found".to_string()))?;
        Ok(runtime_dir.join("tagr_daemon.sock"))
    }

    #[cfg(windows)]
    {
        // Windows will use named pipes, path not needed for now
        Ok(PathBuf::from("\\\\.\\pipe\\tagr_daemon"))
    }

    #[cfg(not(any(unix, windows)))]
    {
        Err(IpcError::ConnectionFailed(
            "IPC not supported on this platform".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipc_response_success_variants() {
        assert!(IpcResponse::Pong.is_success());
        assert!(IpcResponse::Ok.is_success());
        assert!(IpcResponse::Tags(vec![]).is_success());
        assert!(IpcResponse::Files(vec![]).is_success());
        assert!(!IpcResponse::Error("bad".into()).is_success());
    }

    #[test]
    fn test_ipc_request_ping_serde_round_trip() {
        let req = IpcRequest::Ping;
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: IpcRequest = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, IpcRequest::Ping));
    }

    #[test]
    fn test_ipc_request_shutdown_serde_round_trip() {
        let req = IpcRequest::Shutdown;
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: IpcRequest = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, IpcRequest::Shutdown));
    }

    #[test]
    fn test_ipc_search_files_round_trip() {
        let req = IpcRequest::SearchFiles {
            params: SearchParams {
                tags: vec!["rust".into(), "wasm".into()],
                ..SearchParams::default()
            },
        };
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: IpcRequest = serde_json::from_str(&json).unwrap();
        match deserialized {
            IpcRequest::SearchFiles { params } => {
                assert_eq!(params.tags, vec!["rust", "wasm"]);
            }
            _ => panic!("Expected SearchFiles variant"),
        }
    }

    #[test]
    fn test_ipc_add_tags_round_trip() {
        let req = IpcRequest::AddTags {
            file: PathBuf::from("/home/user/test.rs"),
            tags: vec!["rust".into(), "src".into()],
        };
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: IpcRequest = serde_json::from_str(&json).unwrap();
        match deserialized {
            IpcRequest::AddTags { file, tags } => {
                assert_eq!(file, PathBuf::from("/home/user/test.rs"));
                assert_eq!(tags, vec!["rust", "src"]);
            }
            _ => panic!("Expected AddTags variant"),
        }
    }

    #[test]
    fn test_ipc_tags_response_round_trip() {
        let resp = IpcResponse::Tags(vec![
            TagInfo { name: "rust".into(), file_count: 42 },
            TagInfo { name: "python".into(), file_count: 7 },
        ]);
        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: IpcResponse = serde_json::from_str(&json).unwrap();
        match deserialized {
            IpcResponse::Tags(tags) => {
                assert_eq!(tags.len(), 2);
                assert_eq!(tags[0].name, "rust");
                assert_eq!(tags[0].file_count, 42);
            }
            _ => panic!("Expected Tags variant"),
        }
    }

    #[test]
    fn test_ipc_files_response_round_trip() {
        let resp = IpcResponse::Files(vec![
            Pair::new(PathBuf::from("file1.txt"), vec!["a".into()]),
            Pair::new(PathBuf::from("file2.txt"), vec!["b".into(), "c".into()]),
        ]);
        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: IpcResponse = serde_json::from_str(&json).unwrap();
        match deserialized {
            IpcResponse::Files(files) => {
                assert_eq!(files.len(), 2);
                assert_eq!(files[0].file, PathBuf::from("file1.txt"));
                assert_eq!(files[1].tags, vec!["b", "c"]);
            }
            _ => panic!("Expected Files variant"),
        }
    }

    #[test]
    fn test_ipc_socket_path_is_valid() {
        if let Ok(path) = get_ipc_socket_path() {
            let path_str = path.to_string_lossy();
            assert!(path_str.contains("tagr_daemon"));
        }
    }
}
