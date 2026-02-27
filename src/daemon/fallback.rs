use crate::daemon::traits::{DaemonManager, Result, DaemonError};
use crate::ipc::{IpcRequest, IpcResponse};
use crate::watch::WatchConfig;
use async_trait::async_trait;
use crate::daemon::client::send_request;

pub struct FallbackDaemonManager;

#[async_trait]
impl DaemonManager for FallbackDaemonManager {
    async fn ensure_daemon_running(&self, _config: &WatchConfig) -> Result<()> {
        if self.is_running().await? {
            return Ok(());
        }

        let exe = std::env::current_exe()?;
        
        // Spawn detached process
        // On Unix, simple spawn leaves it as child of init if parent exits? 
        // We might need double fork or nohup equivalent.
        // For now, simple spawn.
        std::process::Command::new(exe)
            .arg("watch")
            .arg("--daemon")
            .spawn()?;
            
        // Wait for it to be ready
        for _ in 0..20 {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            if self.is_running().await? {
                return Ok(());
            }
        }
        
        Err(DaemonError::StartFailed("Timeout waiting for daemon".into()))
    }

    async fn send_command(&self, cmd: IpcRequest) -> Result<IpcResponse> {
        send_request(cmd).await
    }

    async fn is_running(&self) -> Result<bool> {
        match send_request(IpcRequest::Ping).await {
            Ok(_) => Ok(true),
            Err(DaemonError::ConnectionFailed(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }
}
