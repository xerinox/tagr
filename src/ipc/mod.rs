//! Inter-process communication for daemon-client fallback.
//!
//! When the database is locked by the daemon, CLI commands forward requests via IPC.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

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

/// Generic command forwarding for database lock fallback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcRequest {
    Ping,
    Shutdown,
    Command {
        args: Vec<String>,
        cwd: String,
    },
}

/// Response from daemon for a forwarded command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcResponse {
    /// Command executed successfully with output
    Success(String),

    /// Command failed with error message
    Error(String),
}

impl IpcResponse {
    /// Check if response indicates success
    pub fn is_success(&self) -> bool {
        matches!(self, IpcResponse::Success(_))
    }

    /// Extract output string or error message
    pub fn as_str(&self) -> &str {
        match self {
            IpcResponse::Success(s) | IpcResponse::Error(s) => s,
        }
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
    fn test_ipc_response_success() {
        let resp = IpcResponse::Success("test output".to_string());
        assert!(resp.is_success());
        assert_eq!(resp.as_str(), "test output");
    }

    #[test]
    fn test_ipc_response_error() {
        let resp = IpcResponse::Error("test error".to_string());
        assert!(!resp.is_success());
        assert_eq!(resp.as_str(), "test error");
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
    fn test_ipc_request_command_serde_round_trip() {
        let req = IpcRequest::Command {
            args: vec!["tagr".into(), "search".into(), "-t".into(), "rust".into()],
            cwd: "/home/user/projects".into(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: IpcRequest = serde_json::from_str(&json).unwrap();

        match deserialized {
            IpcRequest::Command { args, cwd } => {
                assert_eq!(args, vec!["tagr", "search", "-t", "rust"]);
                assert_eq!(cwd, "/home/user/projects");
            }
            _ => panic!("Expected Command variant"),
        }
    }

    #[test]
    fn test_ipc_response_serde_round_trip() {
        let success = IpcResponse::Success("file1.txt\nfile2.txt\n".into());
        let json = serde_json::to_string(&success).unwrap();
        let deserialized: IpcResponse = serde_json::from_str(&json).unwrap();
        assert!(deserialized.is_success());
        assert_eq!(deserialized.as_str(), "file1.txt\nfile2.txt\n");

        let error = IpcResponse::Error("not found".into());
        let json = serde_json::to_string(&error).unwrap();
        let deserialized: IpcResponse = serde_json::from_str(&json).unwrap();
        assert!(!deserialized.is_success());
        assert_eq!(deserialized.as_str(), "not found");
    }

    #[test]
    fn test_ipc_socket_path_is_valid() {
        // This should succeed on any Unix/Windows platform
        if let Ok(path) = get_ipc_socket_path() {
            let path_str = path.to_string_lossy();
            assert!(path_str.contains("tagr_daemon"));
        }
    }
}
