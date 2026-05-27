//! Traits for platform-agnostic daemon management.

use crate::ipc::wire::{Request, Response};
use crate::watch::WatchConfig;
use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DaemonError {
    #[error("Failed to start daemon: {0}")]
    StartFailed(String),

    #[error("Failed to connect to daemon: {0}")]
    ConnectionFailed(String),

    #[error("IPC error: {0}")]
    IpcError(#[from] crate::ipc::IpcError),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, DaemonError>;

/// Abstract interface for managing the background daemon process.
/// Implementations handle platform-specific details (systemd, launchd, manual spawn).
#[async_trait]
pub trait DaemonManager: Send + Sync {
    /// Ensure the daemon is running.
    /// On Linux with systemd, this might verify the unit is active.
    /// On other platforms, this might spawn a detached process.
    async fn ensure_daemon_running(&self, config: &WatchConfig) -> Result<()>;

    /// Send a command to the daemon.
    /// Implementations should handle connecting to the platform-specific IPC channel.
    async fn send_command(&self, cmd: Request) -> Result<Response>;
    
    /// Check if daemon is currently running/active
    async fn is_running(&self) -> Result<bool>;
}
