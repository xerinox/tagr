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

        spawn_detached(&exe)?;

        // Wait for daemon to be ready (IPC socket accepting connections)
        for _ in 0..30 {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            if self.is_running().await? {
                return Ok(());
            }
        }

        Err(DaemonError::StartFailed("Timeout waiting for daemon to start".into()))
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

/// Spawn the daemon as a fully detached background process.
///
/// On Unix this uses the `daemonize` crate which handles fork, setsid,
/// stdio redirection, and optional PID-file creation — replacing our
/// previous manual `pre_exec` + `libc::setsid` approach.
#[cfg(unix)]
fn spawn_detached(exe: &std::path::Path) -> Result<()> {
    let log_path = daemon_log_path();
    let pid_path = daemon_pid_path();

    // Build a daemonize config that forks the current process, then
    // exec's `tagr watch --daemon` in the child.
    //
    // We can't use Daemonize::start() directly because that daemonizes
    // the *current* process. Instead we spawn a child and let *it*
    // daemonize itself. But since the daemon entry point already exists
    // (`tagr watch --daemon`), the simplest approach is to spawn the
    // child directly — the child will daemonize on its own via
    // `daemonize_self()`.
    std::process::Command::new(exe)
        .arg("watch")
        .arg("--daemon")
        .arg("--daemonize")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| DaemonError::StartFailed(format!("Failed to spawn daemon: {e}")))?;

    // The spawned child will call daemonize_self() which does the real
    // fork+setsid. The intermediate child exits immediately, so this
    // spawn returns quickly.
    let _ = (log_path, pid_path);

    Ok(())
}

#[cfg(not(unix))]
fn spawn_detached(exe: &std::path::Path) -> Result<()> {
    std::process::Command::new(exe)
        .arg("watch")
        .arg("--daemon")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| DaemonError::StartFailed(format!("Failed to spawn daemon: {e}")))?;
    Ok(())
}

/// Called by the daemon process itself (when `--daemonize` flag is set)
/// to fork into the background, redirect stdio to the log file, and
/// write a PID file.
///
/// # Errors
///
/// Returns an error if daemonization fails (e.g. cannot write PID file).
#[cfg(unix)]
pub fn daemonize_self() -> Result<()> {
    use daemonize::Daemonize;
    use std::fs::OpenOptions;

    let mut builder = Daemonize::new();

    if let Some(pid_path) = daemon_pid_path() {
        if let Some(parent) = pid_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        builder = builder.pid_file(pid_path);
    }

    if let Some(log_path) = daemon_log_path() {
        if let Some(parent) = log_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        if let Ok(stdout) = OpenOptions::new().create(true).append(true).open(&log_path)
        && let Ok(stderr) = OpenOptions::new().create(true).append(true).open(&log_path) {
            builder = builder.stdout(stdout).stderr(stderr);
        }
    }

    builder
        .start()
        .map_err(|e| DaemonError::StartFailed(format!("Daemonize failed: {e}")))?;

    Ok(())
}

/// Log file path: `~/.local/state/tagr/daemon.log` (XDG state dir).
fn daemon_log_path() -> Option<std::path::PathBuf> {
    dirs::state_dir().map(|d| d.join("tagr").join("daemon.log"))
}

/// PID file path: `~/.local/state/tagr/daemon.pid`.
fn daemon_pid_path() -> Option<std::path::PathBuf> {
    dirs::state_dir().map(|d| d.join("tagr").join("daemon.pid"))
}
