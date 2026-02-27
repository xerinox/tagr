//! Watch command implementation.

use crate::watch::{WatchConfig, WatchRule};
use clap::Args;
use std::path::PathBuf;

/// Monitor files for changes and auto-tag them
#[derive(Args, Debug, Clone)]
pub struct WatchArgs {
    /// File patterns to watch (supports glob, expands ~)
    /// If --stop is specified, this is optional.
    pub patterns: Vec<PathBuf>,
    
    /// Tags to apply when files match
    #[arg(short = 't', long = "tag")]
    pub tags: Vec<String>,
    
    /// Use a saved filter by name (e.g., `--filter my-filter`)
    #[arg(short = 'f', long = "filter")]
    pub filter: Option<String>,

    /// Inline virtual-tag condition; can be repeated (e.g., `-V size:small -V modified:today`)
    #[arg(short = 'V', long = "vtag")]
    pub vtags: Vec<String>,

    /// Only trigger if the file ALREADY has this tag in the database; can be repeated
    #[arg(long = "filter-by-tag")]
    pub filter_by_tags: Vec<String>,

    /// Stop the background daemon (fallback mode only)
    #[arg(long = "stop", conflicts_with_all = ["patterns", "tags", "filter", "vtags", "filter_by_tags"])]
    pub stop: bool,

    /// Internal: Run as daemon process
    #[arg(long, hide = true)]
    pub daemon: bool,
}

impl WatchArgs {
    /// Convert CLI args to WatchRule
    pub fn to_rule(&self) -> Option<WatchRule> {
        if self.patterns.is_empty() {
            return None;
        }

        Some(WatchRule {
            patterns: self.patterns.iter().map(|p| p.to_string_lossy().to_string()).collect(),
            tags: self.tags.clone(),
            filter: self.filter.clone(),
            vtags: self.vtags.clone(),
            filter_by_tags: self.filter_by_tags.clone(),
            filter_criteria: None, // Resolved by daemon
        })
    }
}

/// Handle `tagr watch` from the CLI (non-daemon path).
///
/// Updates `watch.toml` with the new rule and ensures the daemon is running
/// so it can pick up the change via hot-reload.
///
/// # Errors
///
/// Returns an error if the watch config cannot be saved or the daemon fails to start.
pub fn watch_cli(
    args: &WatchArgs,
    app_config: &crate::config::TagrConfig,
    quiet: bool,
) -> anyhow::Result<()> {
    if args.stop {
        use crate::daemon::client::send_request;
        use crate::ipc::{IpcRequest, IpcResponse, get_ipc_socket_path};
        use crate::daemon::{DaemonManager, PlatformDaemonManager};

        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| anyhow::anyhow!("Failed to create async runtime: {e}"))?;

        let is_running = rt.block_on(PlatformDaemonManager.is_running()).unwrap_or(false);
        if !is_running {
            if !quiet {
                println!("No daemon is running.");
            }
            return Ok(());
        }

        match rt.block_on(send_request(IpcRequest::Shutdown)) {
            Ok(IpcResponse::Success(_)) | Ok(IpcResponse::Error(_)) => {}
            Err(e) => anyhow::bail!("Failed to stop daemon: {}", e),
        }

        // Wait for the daemon to delete its socket (confirms the process has
        // exited and the sled DB lock has been released).
        let socket_path = get_ipc_socket_path()
            .map_err(|e| anyhow::anyhow!("Could not determine socket path: {e}"))?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while std::time::Instant::now() < deadline {
            if !socket_path.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        if !quiet {
            println!("Daemon stopped.");
        }
        return Ok(());
    }

    use crate::daemon::{DaemonManager, PlatformDaemonManager};

    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| anyhow::anyhow!("Failed to create async runtime: {e}"))?;

    // Check daemon state BEFORE writing config to avoid a race where the
    // hot-reload processing briefly blocks the IPC accept loop.
    let is_running = rt.block_on(PlatformDaemonManager.is_running()).unwrap_or(false);

    // Persist the new rule (if any) so the daemon picks it up via hot-reload
    // (when already running) or finds it on startup (when starting fresh).
    if let Some(rule) = args.to_rule() {
        let mut watch_config = WatchConfig::load().unwrap_or_default();
        watch_config.append_rule(rule.clone())?;
        if !quiet {
            println!("Watch rule saved: patterns={:?} tags={:?}", rule.patterns, rule.tags);
        }
    }

    if is_running {
        if args.patterns.is_empty() {
            // `tagr watch` with no args is an explicit "start daemon" request;
            // error if it's already running so scripts/users know.
            anyhow::bail!("Daemon is already running.");
        }
        if !quiet {
            println!("Daemon is running. New rule will be applied automatically.");
        }
    } else {
        if !quiet {
            println!("Starting daemon...");
        }

        // Verify a database is configured before attempting to start the daemon.
        if app_config.get_default_database().is_none() {
            anyhow::bail!(
                "No default database configured. The daemon requires one.\n\
                 Run: tagr db add <name> <path>"
            );
        }

        let watch_config = WatchConfig::load().unwrap_or_default();
        rt.block_on(PlatformDaemonManager.ensure_daemon_running(&watch_config))
            .map_err(|e| anyhow::anyhow!("Failed to start daemon: {e}"))?;

        if !quiet {
            println!("Daemon started.");
        }
    }

    Ok(())
}
