use crate::daemon::traits::{DaemonManager, Result, DaemonError};
use crate::ipc::wire::{Request, Response};
use crate::watch::WatchConfig;
use async_trait::async_trait;
use crate::daemon::client::send_request;
use crate::daemon::fallback::FallbackDaemonManager;

pub struct LinuxDaemonManager;

#[async_trait]
impl DaemonManager for LinuxDaemonManager {
    async fn ensure_daemon_running(&self, config: &WatchConfig) -> Result<()> {
        if self.is_running().await? {
            return Ok(());
        }

        let status = std::process::Command::new("systemctl")
            .arg("--user")
            .arg("start")
            .arg("tagr.service")
            .output();
            
        if let Ok(output) = status && output.status.success() {
            for _ in 0..10 {
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                if self.is_running().await? {
                    return Ok(());
                }
            }
        }
        
        FallbackDaemonManager.ensure_daemon_running(config).await
    }

    async fn send_command(&self, cmd: Request) -> Result<Response> {
        send_request(cmd).await
    }
    
    async fn is_running(&self) -> Result<bool> {
        match send_request(Request::Ping).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}

/// Install systemd service and socket unit files for the tagr daemon.
///
/// # Errors
///
/// Returns `DaemonError::StartFailed` if the config directory cannot be
/// determined, or `DaemonError::Io` on filesystem failures.
pub fn install_systemd_units() -> Result<()> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| DaemonError::StartFailed("Could not determine config directory".into()))?;
    let systemd_dir = config_dir.join("systemd").join("user");
    std::fs::create_dir_all(&systemd_dir)?;
    
    let exe = std::env::current_exe()?;
    let exe_path = exe.to_string_lossy();
    
    // Service file
    let service_content = format!(r"[Unit]
Description=Tagr File Watcher Daemon
Documentation=https://github.com/xerinox/tagr

[Service]
ExecStart={exe_path} watch --daemon
Restart=on-failure
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=default.target
");

    // Socket file
    // Note: %t resolves to XDG_RUNTIME_DIR.
    // We assume IPC socket is at runtime_dir/tagr_daemon.sock
    let socket_content = r"[Unit]
Description=Tagr Daemon Socket

[Socket]
ListenStream=%t/tagr_daemon.sock
Accept=no

[Install]
WantedBy=sockets.target
";

    std::fs::write(systemd_dir.join("tagr.service"), service_content)?;
    std::fs::write(systemd_dir.join("tagr.socket"), socket_content)?;
    
    // Reload and enable
    let _ = std::process::Command::new("systemctl")
        .arg("--user")
        .arg("daemon-reload")
        .status();
        
    let _ = std::process::Command::new("systemctl")
        .arg("--user")
        .arg("enable")
        .arg("--now")
        .arg("tagr.socket")
        .status();
        
    Ok(())
}

