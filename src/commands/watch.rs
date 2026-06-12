//! Watch command implementation.
//!
//! Provides subcommands for managing watch rules and the background daemon:
//!
//! - `tagr watch add <patterns> -t <tags>` — add a watch rule
//! - `tagr watch remove <index>` — remove a rule by index
//! - `tagr watch list` — show all configured rules
//! - `tagr watch status` — show daemon state and rule count
//! - `tagr watch start` — start the background daemon
//! - `tagr watch stop` — stop the background daemon
//!
//! For complex filtering criteria, use saved filters:
//! ```bash
//! tagr filter save rust-src --tag rust --file "src/**/*.rs"
//! tagr watch add ~/projects -f rust-src -t auto-indexed
//! ```

use crate::watch::{WatchConfig, WatchRule};
use clap::{Args, Subcommand};
use std::collections::HashSet;
use std::io::Write;

/// Watch mode subcommands
#[derive(Subcommand, Debug, Clone)]
pub enum WatchCommands {
    /// Add a watch rule to auto-tag matching files.
    ///
    /// For complex criteria, create a reusable filter first with
    /// `tagr filter save`, then reference it with `--filter`.
    #[command(visible_alias = "a")]
    Add(WatchAddArgs),

    /// Remove a watch rule by its index (shown in `tagr watch list`)
    #[command(visible_alias = "rm")]
    Remove {
        /// Rule index to remove (1-based, from `tagr watch list`)
        index: usize,
    },

    /// List all configured watch rules
    #[command(visible_alias = "ls")]
    List,

    /// Show daemon status and watch rule summary
    Status,

    /// Start the background daemon
    Start(WatchStartArgs),

    /// Stop the background daemon
    Stop,
}

#[cfg(feature = "dynamic-completions")]
use clap_complete::engine::ArgValueCompleter;

/// Arguments for `tagr watch add`
#[derive(Args, Debug, Clone)]
pub struct WatchAddArgs {
    /// File patterns to watch (supports glob, expands ~).
    ///
    /// Quote patterns containing wildcards to prevent shell expansion:
    ///   tagr watch add '~/notes/*.md' -t notes
    pub patterns: Vec<String>,

    /// Tags to apply when files match
    #[arg(short = 't', long = "tag")]
    #[cfg_attr(feature = "dynamic-completions", arg(add = ArgValueCompleter::new(crate::completions::complete_tags)))]
    pub tags: Vec<String>,

    /// Use a saved filter by name (recommended for complex criteria)
    #[arg(short = 'f', long = "filter")]
    #[cfg_attr(feature = "dynamic-completions", arg(add = ArgValueCompleter::new(crate::completions::complete_filters)))]
    pub filter: Option<String>,

    /// Inline virtual-tag condition; can be repeated (e.g., `-V size:small`)
    #[arg(short = 'V', long = "vtag")]
    #[cfg_attr(feature = "dynamic-completions", arg(add = ArgValueCompleter::new(crate::completions::complete_vtags)))]
    pub vtags: Vec<String>,

    /// Only trigger if the file already has this tag; can be repeated
    #[arg(long = "filter-by-tag")]
    #[cfg_attr(feature = "dynamic-completions", arg(add = ArgValueCompleter::new(crate::completions::complete_tags)))]
    pub filter_by_tags: Vec<String>,
}

impl WatchAddArgs {
    /// Convert CLI args to a `WatchRule`.
    ///
    /// Returns `None` if no patterns were provided.
    #[must_use]
    pub fn to_rule(&self) -> Option<WatchRule> {
        if self.patterns.is_empty() {
            return None;
        }

        Some(WatchRule {
            patterns: self.patterns.clone(),
            tags: self.tags.clone(),
            filter: self.filter.clone(),
            vtags: self.vtags.clone(),
            filter_by_tags: self.filter_by_tags.clone(),
            filter_criteria: None,
        })
    }
}

/// Hidden args used internally when spawning the daemon child process
#[derive(Args, Debug, Clone)]
pub struct WatchStartArgs {
    /// Internal: this process IS the daemon — run the event loop
    #[arg(long, hide = true)]
    pub daemon: bool,

    /// Internal: fork into background via daemonize before starting event loop
    #[arg(long, hide = true)]
    pub daemonize: bool,
}

// ---------------------------------------------------------------------------
// Subcommand handlers
// ---------------------------------------------------------------------------

/// Handle `tagr watch add`
///
/// Saves the rule to `watch.toml` and ensures the daemon is running so it
/// picks up the new rule via hot-reload.
///
/// # Errors
///
/// Returns an error if patterns are empty, config cannot be saved, or the
/// daemon fails to start.
pub fn watch_add(
    args: &WatchAddArgs,
    app_config: &crate::config::TagrConfig,
    quiet: bool,
    writer: &mut impl Write,
) -> anyhow::Result<()> {
    let rule = args.to_rule().ok_or_else(|| {
        anyhow::anyhow!("No file patterns provided. Usage: tagr watch add <patterns> -t <tags>")
    })?;

    // Detect likely shell glob expansion: many literal paths with a common
    // parent and extension suggest the user forgot to quote the pattern.
    if !quiet && looks_like_shell_expansion(&rule.patterns) {
        let suggested = suggest_glob(&rule.patterns);
        writeln!(
            writer,
            "Warning: It looks like your shell expanded a glob into {} individual files.",
            rule.patterns.len(),
        )?;
        writeln!(
            writer,
            "  This works, but a quoted glob is more flexible (catches future files too)."
        )?;
        if let Some(ref glob) = suggested {
            writeln!(
                writer,
                "  Try: tagr watch add '{glob}' -t {}",
                rule.tags.join(" -t ")
            )?;
        } else {
            writeln!(
                writer,
                "  Example: tagr watch add '~/notes/*.org' -t notes:org"
            )?;
        }
        writeln!(writer)?;
    }

    let mut watch_config = WatchConfig::load().unwrap_or_default();
    watch_config.append_rule(rule.clone())?;

    if !quiet {
        writeln!(
            writer,
            "Watch rule saved: patterns={:?} tags={:?}",
            rule.patterns, rule.tags
        )?;
        if let Some(ref filter) = rule.filter {
            writeln!(writer, "  Using saved filter: {filter}")?;
        }
        writeln!(
            writer,
            "Tip: Use `tagr filter save` to create reusable filters for complex criteria."
        )?;
    }

    ensure_daemon_running(app_config, quiet, writer)?;
    Ok(())
}

/// Heuristic: if 5+ patterns share the same parent directory and none contain
/// glob characters, the shell probably expanded a wildcard before tagr saw it.
fn looks_like_shell_expansion(patterns: &[String]) -> bool {
    const THRESHOLD: usize = 5;
    if patterns.len() < THRESHOLD {
        return false;
    }

    let has_any_glob = patterns
        .iter()
        .any(|p| p.contains('*') || p.contains('?') || p.contains('[') || p.contains('{'));
    if has_any_glob {
        return false;
    }

    // Check if they share a common parent directory
    let parents: HashSet<&std::path::Path> = patterns
        .iter()
        .filter_map(|p| std::path::Path::new(p).parent())
        .collect();

    parents.len() == 1
}

/// Reconstruct a probable glob from shell-expanded paths.
///
/// If all paths share one parent directory, uses the dominant extension to
/// produce e.g. `~/notes/*.md`. Falls back to `parent/*` when extensions vary.
fn suggest_glob(patterns: &[String]) -> Option<String> {
    use std::path::Path;

    let parent = Path::new(patterns.first()?).parent()?;

    // Count extensions
    let mut ext_counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for p in patterns {
        let ext = Path::new(p)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        *ext_counts.entry(ext).or_insert(0) += 1;
    }

    let parent_str = parent.to_str()?;

    // Collapse home dir back to ~ for readability
    let parent_display = dirs::home_dir().map_or_else(
        || parent_str.to_owned(),
        |home| {
            let home_str = home.to_string_lossy();
            if parent_str.starts_with(home_str.as_ref()) {
                format!("~{}", &parent_str[home_str.len()..])
            } else {
                parent_str.to_owned()
            }
        },
    );

    // If one extension dominates (≥80%), use it; otherwise use *
    let total = patterns.len();
    let (dominant_ext, count) = ext_counts.iter().max_by_key(|(_, c)| **c)?;
    if *count * 100 / total >= 80 && !dominant_ext.is_empty() {
        Some(format!("{parent_display}/*.{dominant_ext}"))
    } else {
        Some(format!("{parent_display}/*"))
    }
}

/// Handle `tagr watch remove`
///
/// Removes the rule at the given 1-based index from `watch.toml`.
///
/// # Errors
///
/// Returns an error if the index is out of bounds or config cannot be saved.
pub fn watch_remove(index: usize, quiet: bool, writer: &mut impl Write) -> anyhow::Result<()> {
    let mut config = WatchConfig::load().unwrap_or_default();
    let removed = remove_rule_from_config(&mut config, index)?;
    config.save()?;

    if !quiet {
        writeln!(
            writer,
            "Removed rule {index}: patterns={:?} tags={:?}",
            removed.patterns, removed.tags
        )?;
    }

    Ok(())
}

/// Remove a rule by 1-based index from a config, returning the removed rule.
///
/// # Errors
///
/// Returns an error if the index is 0 or out of range.
fn remove_rule_from_config(config: &mut WatchConfig, index: usize) -> anyhow::Result<WatchRule> {
    if index == 0 {
        anyhow::bail!("Rule index is 1-based. Use `tagr watch list` to see rule numbers.");
    }

    let zero_idx = index - 1;

    if zero_idx >= config.rules.len() {
        anyhow::bail!(
            "Rule index {index} out of range. There {} {} rule{}.",
            if config.rules.len() == 1 { "is" } else { "are" },
            config.rules.len(),
            if config.rules.len() == 1 { "" } else { "s" },
        );
    }

    Ok(config.rules.remove(zero_idx))
}

/// Handle `tagr watch list`
///
/// Displays all configured watch rules as a numbered table.
///
/// # Errors
/// Returns I/O errors if writing to the output fails, or config errors if
/// the watch configuration cannot be loaded.
pub fn watch_list(writer: &mut impl Write) -> anyhow::Result<()> {
    let config = WatchConfig::load().unwrap_or_default();
    format_rule_list(&config, writer)
}

/// Format a `WatchConfig` as a numbered table.
///
/// Extracted from `watch_list` so it can be tested without filesystem access.
fn format_rule_list(config: &WatchConfig, writer: &mut impl Write) -> anyhow::Result<()> {
    if config.rules.is_empty() {
        writeln!(writer, "No watch rules configured.")?;
        writeln!(
            writer,
            "Use `tagr watch add <patterns> -t <tags>` to get started."
        )?;
        return Ok(());
    }

    writeln!(
        writer,
        "{:>3}  {:<30} {:<20} Filter",
        "#", "Patterns", "Tags"
    )?;
    writeln!(writer, "{}", "-".repeat(78))?;

    for (i, rule) in config.rules.iter().enumerate() {
        let patterns = rule.patterns.join(", ");
        let tags = rule.tags.join(", ");
        let filter = rule.filter.as_deref().unwrap_or("—");

        // Truncate long values for table display
        let patterns_display = if patterns.len() > 28 {
            format!("{}…", &patterns[..27])
        } else {
            patterns
        };
        let tags_display = if tags.len() > 18 {
            format!("{}…", &tags[..17])
        } else {
            tags
        };

        writeln!(
            writer,
            "{:>3}  {:<30} {:<20} {}",
            i + 1,
            patterns_display,
            tags_display,
            filter
        )?;

        // Show extra details on separate lines if present
        if !rule.vtags.is_empty() {
            writeln!(writer, "     vtags: {}", rule.vtags.join(", "))?;
        }
        if !rule.filter_by_tags.is_empty() {
            writeln!(
                writer,
                "     filter-by-tags: {}",
                rule.filter_by_tags.join(", ")
            )?;
        }
    }

    Ok(())
}

/// Handle `tagr watch status`
///
/// Shows whether the daemon is running, the number of configured rules,
/// and the socket path.
///
/// # Errors
/// Returns I/O errors if writing to the output fails, or errors from
/// daemon status checks and socket path resolution.
pub fn watch_status(writer: &mut impl Write) -> anyhow::Result<()> {
    use crate::daemon::{DaemonManager, PlatformDaemonManager};
    use crate::ipc::get_ipc_socket_path;

    let config = WatchConfig::load().unwrap_or_default();

    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| anyhow::anyhow!("Failed to create async runtime: {e}"))?;
    let is_running = rt
        .block_on(PlatformDaemonManager.is_running())
        .unwrap_or(false);

    if is_running {
        writeln!(writer, "Daemon: running")?;
    } else {
        writeln!(writer, "Daemon: stopped")?;
    }

    writeln!(writer, "Rules:  {} configured", config.rules.len())?;

    match get_ipc_socket_path() {
        Ok(path) => writeln!(writer, "Socket: {}", path.display())?,
        Err(_) => writeln!(writer, "Socket: (unknown)")?,
    }

    Ok(())
}

/// Handle `tagr watch start`
///
/// Starts the daemon if it is not already running.
///
/// # Errors
///
/// Returns an error if the daemon is already running or fails to start.
pub fn watch_start(
    app_config: &crate::config::TagrConfig,
    quiet: bool,
    writer: &mut impl Write,
) -> anyhow::Result<()> {
    use crate::daemon::{DaemonManager, PlatformDaemonManager};

    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| anyhow::anyhow!("Failed to create async runtime: {e}"))?;

    let is_running = rt
        .block_on(PlatformDaemonManager.is_running())
        .unwrap_or(false);
    if is_running {
        anyhow::bail!("Daemon is already running. Use `tagr watch status` to check.");
    }

    if app_config.get_default_database().is_none() {
        anyhow::bail!(
            "No default database configured. The daemon requires one.\n\
             Run: tagr db add <name> <path>"
        );
    }

    if !quiet {
        writeln!(writer, "Starting daemon...")?;
    }

    let watch_config = WatchConfig::load().unwrap_or_default();
    rt.block_on(PlatformDaemonManager.ensure_daemon_running(&watch_config))
        .map_err(|e| anyhow::anyhow!("Failed to start daemon: {e}"))?;

    if !quiet {
        writeln!(writer, "Daemon started.")?;
    }

    Ok(())
}

/// Handle `tagr watch stop`
///
/// Sends a shutdown request to the daemon via IPC and waits for it to exit.
///
/// # Errors
///
/// Returns an error if the daemon is not running or the shutdown request fails.
pub fn watch_stop(quiet: bool, writer: &mut impl Write) -> anyhow::Result<()> {
    use crate::daemon::client::send_request;
    use crate::daemon::{DaemonManager, PlatformDaemonManager};
    use crate::ipc::get_ipc_socket_path;
    use crate::ipc::wire::Request;

    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| anyhow::anyhow!("Failed to create async runtime: {e}"))?;

    let is_running = rt
        .block_on(PlatformDaemonManager.is_running())
        .unwrap_or(false);
    if !is_running {
        // On Linux the daemon may be systemd-managed. Even if the IPC socket is
        // gone, the process might still be alive and holding the sled lock.
        // Attempt a graceful systemd stop so the DB lock is released.
        #[cfg(target_os = "linux")]
        {
            let stopped = std::process::Command::new("systemctl")
                .args(["--user", "stop", "tagr.service"])
                .output()
                .is_ok_and(|o| o.status.success());
            if stopped {
                if !quiet {
                    writeln!(writer, "Daemon stopped (via systemd).")?;
                }
                return Ok(());
            }
        }
        if !quiet {
            writeln!(writer, "No daemon is running.")?;
        }
        return Ok(());
    }

    match rt.block_on(send_request(Request::Shutdown)) {
        Ok(_) => {} // any Ok means shutdown was sent
        Err(e) => anyhow::bail!("Failed to stop daemon: {e}"),
    }

    // Wait for the daemon to delete its socket (confirms clean exit)
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
        writeln!(writer, "Daemon stopped.")?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Ensure the daemon is running, starting it if necessary.
fn ensure_daemon_running(
    app_config: &crate::config::TagrConfig,
    quiet: bool,
    writer: &mut impl Write,
) -> anyhow::Result<()> {
    use crate::daemon::{DaemonManager, PlatformDaemonManager};

    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| anyhow::anyhow!("Failed to create async runtime: {e}"))?;

    let is_running = rt
        .block_on(PlatformDaemonManager.is_running())
        .unwrap_or(false);

    if is_running {
        if !quiet {
            writeln!(
                writer,
                "Daemon is running. New rule will be applied automatically."
            )?;
        }
    } else {
        if app_config.get_default_database().is_none() {
            anyhow::bail!(
                "No default database configured. The daemon requires one.\n\
                 Run: tagr db add <name> <path>"
            );
        }

        if !quiet {
            writeln!(writer, "Starting daemon...")?;
        }

        let watch_config = WatchConfig::load().unwrap_or_default();
        rt.block_on(PlatformDaemonManager.ensure_daemon_running(&watch_config))
            .map_err(|e| anyhow::anyhow!("Failed to start daemon: {e}"))?;

        if !quiet {
            writeln!(writer, "Daemon started.")?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_add_args(patterns: Vec<&str>, tags: Vec<&str>) -> WatchAddArgs {
        WatchAddArgs {
            patterns: patterns.into_iter().map(String::from).collect(),
            tags: tags.into_iter().map(String::from).collect(),
            filter: None,
            vtags: vec![],
            filter_by_tags: vec![],
        }
    }

    fn make_rule(patterns: Vec<&str>, tags: Vec<&str>) -> WatchRule {
        WatchRule {
            patterns: patterns.into_iter().map(String::from).collect(),
            tags: tags.into_iter().map(String::from).collect(),
            filter: None,
            vtags: vec![],
            filter_by_tags: vec![],
            filter_criteria: None,
        }
    }

    fn make_config(rules: Vec<WatchRule>) -> WatchConfig {
        WatchConfig { rules }
    }

    // ---- WatchAddArgs::to_rule ----

    #[test]
    fn test_to_rule_with_patterns_and_tags() {
        let args = make_add_args(vec!["~/docs/*.md"], vec!["docs", "markdown"]);
        let rule = args.to_rule().unwrap();

        assert_eq!(rule.patterns, vec!["~/docs/*.md"]);
        assert_eq!(rule.tags, vec!["docs", "markdown"]);
        assert!(rule.filter.is_none());
        assert!(rule.filter_criteria.is_none());
    }

    #[test]
    fn test_to_rule_empty_patterns_returns_none() {
        let args = make_add_args(vec![], vec!["tag1"]);
        assert!(args.to_rule().is_none());
    }

    #[test]
    fn test_to_rule_preserves_filter() {
        let mut args = make_add_args(vec!["/tmp/*.rs"], vec!["rust"]);
        args.filter = Some("my-filter".into());
        let rule = args.to_rule().unwrap();
        assert_eq!(rule.filter, Some("my-filter".into()));
    }

    #[test]
    fn test_to_rule_preserves_vtags_and_filter_by_tags() {
        let mut args = make_add_args(vec!["/tmp/*.rs"], vec!["rust"]);
        args.vtags = vec!["size:small".into()];
        args.filter_by_tags = vec!["code".into()];
        let rule = args.to_rule().unwrap();

        assert_eq!(rule.vtags, vec!["size:small"]);
        assert_eq!(rule.filter_by_tags, vec!["code"]);
    }

    #[test]
    fn test_to_rule_empty_tags_still_creates_rule() {
        let args = make_add_args(vec!["/tmp/*.txt"], vec![]);
        let rule = args.to_rule().unwrap();
        assert!(rule.tags.is_empty());
        assert_eq!(rule.patterns, vec!["/tmp/*.txt"]);
    }

    // ---- format_rule_list ----

    #[test]
    fn test_list_empty_config() {
        let config = WatchConfig::default();
        let mut buf = Vec::new();
        format_rule_list(&config, &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();

        assert!(output.contains("No watch rules configured."));
        assert!(output.contains("tagr watch add"));
    }

    #[test]
    fn test_list_single_rule() {
        let config = make_config(vec![make_rule(vec!["~/docs/*.md"], vec!["docs"])]);
        let mut buf = Vec::new();
        format_rule_list(&config, &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();

        assert!(output.contains("#"));
        assert!(output.contains("Patterns"));
        assert!(output.contains("Tags"));
        assert!(output.contains("Filter"));
        assert!(output.contains("~/docs/*.md"));
        assert!(output.contains("docs"));
        // No filter → shows em-dash
        assert!(output.contains("—"));
    }

    #[test]
    fn test_list_with_filter() {
        let mut rule = make_rule(vec!["~/src/*.rs"], vec!["rust"]);
        rule.filter = Some("rust-src".into());
        let config = make_config(vec![rule]);
        let mut buf = Vec::new();
        format_rule_list(&config, &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();

        assert!(output.contains("rust-src"));
    }

    #[test]
    fn test_list_shows_vtags_and_filter_by_tags() {
        let mut rule = make_rule(vec!["*.py"], vec!["python"]);
        rule.vtags = vec!["size:small".into(), "modified:today".into()];
        rule.filter_by_tags = vec!["code".into()];
        let config = make_config(vec![rule]);
        let mut buf = Vec::new();
        format_rule_list(&config, &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();

        assert!(output.contains("vtags: size:small, modified:today"));
        assert!(output.contains("filter-by-tags: code"));
    }

    #[test]
    fn test_list_multiple_rules_numbered() {
        let config = make_config(vec![
            make_rule(vec!["*.rs"], vec!["rust"]),
            make_rule(vec!["*.py"], vec!["python"]),
            make_rule(vec!["*.md"], vec!["docs"]),
        ]);
        let mut buf = Vec::new();
        format_rule_list(&config, &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = output.lines().collect();

        // Header + separator + 3 rules = 5 lines
        assert!(lines.len() >= 5);
        // Check 1-based numbering
        assert!(lines[2].contains("  1"));
        assert!(lines[3].contains("  2"));
        assert!(lines[4].contains("  3"));
    }

    #[test]
    fn test_list_truncates_long_patterns() {
        let long_pattern = "a".repeat(40);
        let config = make_config(vec![make_rule(vec![&long_pattern], vec!["t"])]);
        let mut buf = Vec::new();
        format_rule_list(&config, &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();

        // Long pattern should be truncated with ellipsis
        assert!(output.contains('…'));
        assert!(!output.contains(&long_pattern));
    }

    #[test]
    fn test_list_truncates_long_tags() {
        let long_tag = "b".repeat(25);
        let config = make_config(vec![make_rule(vec!["*.rs"], vec![&long_tag])]);
        let mut buf = Vec::new();
        format_rule_list(&config, &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();

        assert!(output.contains('…'));
    }

    // ---- remove_rule_from_config ----

    #[test]
    fn test_remove_zero_index_error() {
        let mut config = make_config(vec![make_rule(vec!["*.rs"], vec!["rust"])]);
        let result = remove_rule_from_config(&mut config, 0);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("1-based"));
    }

    #[test]
    fn test_remove_out_of_range_error() {
        let mut config = make_config(vec![make_rule(vec!["*.rs"], vec!["rust"])]);
        let result = remove_rule_from_config(&mut config, 2);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("out of range"));
        assert!(err.contains("1 rule"));
    }

    #[test]
    fn test_remove_out_of_range_plural() {
        let mut config = make_config(vec![
            make_rule(vec!["*.rs"], vec!["rust"]),
            make_rule(vec!["*.py"], vec!["python"]),
        ]);
        let result = remove_rule_from_config(&mut config, 5);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("2 rules"));
    }

    #[test]
    fn test_remove_empty_config() {
        let mut config = WatchConfig::default();
        let result = remove_rule_from_config(&mut config, 1);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("out of range"));
    }

    #[test]
    fn test_remove_first_rule() {
        let mut config = make_config(vec![
            make_rule(vec!["*.rs"], vec!["rust"]),
            make_rule(vec!["*.py"], vec!["python"]),
        ]);
        let removed = remove_rule_from_config(&mut config, 1).unwrap();
        assert_eq!(removed.tags, vec!["rust"]);
        assert_eq!(config.rules.len(), 1);
        assert_eq!(config.rules[0].tags, vec!["python"]);
    }

    #[test]
    fn test_remove_last_rule() {
        let mut config = make_config(vec![
            make_rule(vec!["*.rs"], vec!["rust"]),
            make_rule(vec!["*.py"], vec!["python"]),
        ]);
        let removed = remove_rule_from_config(&mut config, 2).unwrap();
        assert_eq!(removed.tags, vec!["python"]);
        assert_eq!(config.rules.len(), 1);
        assert_eq!(config.rules[0].tags, vec!["rust"]);
    }

    #[test]
    fn test_remove_only_rule() {
        let mut config = make_config(vec![make_rule(vec!["*.md"], vec!["docs"])]);
        let removed = remove_rule_from_config(&mut config, 1).unwrap();
        assert_eq!(removed.tags, vec!["docs"]);
        assert!(config.rules.is_empty());
    }

    // ---- watch_remove (integration with writer) ----

    #[test]
    fn test_watch_remove_zero_index_writer() {
        let mut buf = Vec::new();
        let result = watch_remove(0, true, &mut buf);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("1-based"));
    }

    // ---- looks_like_shell_expansion ----

    #[test]
    fn test_shell_expansion_detected() {
        let patterns: Vec<String> = (0..6)
            .map(|i| format!("/home/user/notes/file{i}.md"))
            .collect();
        assert!(looks_like_shell_expansion(&patterns));
    }

    #[test]
    fn test_shell_expansion_not_detected_with_glob() {
        let patterns = vec!["/home/user/notes/*.md".to_string()];
        assert!(!looks_like_shell_expansion(&patterns));
    }

    #[test]
    fn test_shell_expansion_not_detected_few_files() {
        let patterns: Vec<String> = (0..3).map(|i| format!("/tmp/file{i}.txt")).collect();
        assert!(!looks_like_shell_expansion(&patterns));
    }

    #[test]
    fn test_shell_expansion_not_detected_different_dirs() {
        let patterns = vec![
            "/a/file1.txt".to_string(),
            "/b/file2.txt".to_string(),
            "/c/file3.txt".to_string(),
            "/d/file4.txt".to_string(),
            "/e/file5.txt".to_string(),
        ];
        assert!(!looks_like_shell_expansion(&patterns));
    }

    // ---- suggest_glob ----

    #[test]
    fn test_suggest_glob_same_extension() {
        let patterns: Vec<String> = (0..5).map(|i| format!("/tmp/notes/file{i}.md")).collect();
        let suggestion = suggest_glob(&patterns).unwrap();
        assert_eq!(suggestion, "/tmp/notes/*.md");
    }

    #[test]
    fn test_suggest_glob_mixed_extensions() {
        let patterns = vec![
            "/tmp/dir/a.md".to_string(),
            "/tmp/dir/b.txt".to_string(),
            "/tmp/dir/c.rs".to_string(),
            "/tmp/dir/d.py".to_string(),
            "/tmp/dir/e.go".to_string(),
        ];
        let suggestion = suggest_glob(&patterns).unwrap();
        assert_eq!(suggestion, "/tmp/dir/*");
    }

    #[test]
    fn test_suggest_glob_empty() {
        let patterns: Vec<String> = vec![];
        assert!(suggest_glob(&patterns).is_none());
    }
}
