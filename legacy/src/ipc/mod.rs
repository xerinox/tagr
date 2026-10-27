//! Inter-process communication for daemon-client data exchange.
//!
//! The IPC protocol uses typed request/response messages so the daemon
//! deals only in data — the CLI/TUI client handles all presentation.
//!
//! ## Modules
//!
//! - [`wire`] — Binary framed protocol (wincode + length-prefixed frames)
//!   for persistent bidirectional connections.

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

/// Tag with its associated file count.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TagInfo {
    pub name: String,
    pub file_count: usize,
}

/// Get the IPC socket path for this platform
///
/// # Errors
/// Returns [`IpcError::ConnectionFailed`] if the platform runtime directory cannot be determined.
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
    fn test_ipc_socket_path_is_valid() {
        if let Ok(path) = get_ipc_socket_path() {
            let path_str = path.to_string_lossy();
            assert!(path_str.contains("tagr_daemon"));
        }
    }
}
