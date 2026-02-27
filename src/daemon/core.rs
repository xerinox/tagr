//! Core daemon logic: event loop, file watching, and IPC handling.

use crate::db::Database;
use crate::filters::{FilterCriteria, FilterManager, get_filter_path};
use crate::ipc::{IpcRequest, IpcResponse, get_ipc_socket_path};
use crate::watch::matcher::{FilterEvaluator, matches_patterns};
use crate::watch::{WatchConfig, WatchRule};
use anyhow::{Context, Result};
use interprocess::local_socket::tokio::prelude::LocalSocketStream;
use interprocess::local_socket::traits::tokio::Listener;
use interprocess::local_socket::{GenericFilePath, ListenerOptions, ToFsName};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader as TokioBufReader};

/// Run the daemon event loop (blocking — starts its own async runtime).
///
/// # Errors
///
/// Returns an error if the async runtime or IPC listener cannot be created.
pub fn run(db: &Database) -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async_run(db))
}

async fn async_run(db: &Database) -> Result<()> {
    println!("Daemon started. PID: {}", std::process::id());

    // Bind IPC socket first so clients can detect us immediately.
    let socket_path = get_ipc_socket_path()?;
    #[cfg(unix)]
    if socket_path.exists() {
        std::fs::remove_file(&socket_path).ok();
    }
    let listener = ListenerOptions::new()
        .name(socket_path.clone().to_fs_name::<GenericFilePath>()?)
        .create_tokio()?;
    println!("IPC socket: {:?}", socket_path);

    // Setup the notify file watcher with an async channel.
    let (tx, mut rx) = tokio::sync::mpsc::channel(256);
    let tx_clone = tx.clone();
    let mut watcher = RecommendedWatcher::new(
        move |res| {
            let _ = tx_clone.blocking_send(res);
        },
        Config::default(),
    )?;

    // Compute the canonical path to watch.toml so we can detect config changes.
    let config_path = WatchConfig::config_path().unwrap_or_else(|_| PathBuf::from("watch.toml"));

    // Watch the config directory non-recursively so we notice when watch.toml changes.
    if let Some(config_dir) = config_path.parent() {
        if !config_dir.exists() {
            std::fs::create_dir_all(config_dir).ok();
        }
        if let Err(e) = watcher.watch(config_dir, RecursiveMode::NonRecursive) {
            eprintln!("Warning: could not watch config directory {:?}: {}", config_dir, e);
        }
    }

    // Load initial config and start watching the declared paths.
    let mut current_config = WatchConfig::load().unwrap_or_default();
    resolve_all_rules(&mut current_config.rules);
    let mut watched_roots: HashSet<PathBuf> = HashSet::new();
    // Debounce: track the last time watch.toml was reloaded.
    let mut last_config_reload: Option<Instant> = None;
    // Debounce: track last tag event per file path to suppress inotify event storms.
    let mut last_file_events: HashMap<PathBuf, Instant> = HashMap::new();
    let mut filter_evaluator = FilterEvaluator::new();
    println!("Loaded config: {} rules", current_config.rules.len());
    add_new_watch_roots(&mut watcher, &current_config, &mut watched_roots);

    println!("Daemon ready.");

    let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);

    loop {
        tokio::select! {
            Some(res) = rx.recv() => {
                match res {
                    Ok(event) => {
                        let work = handle_event(
                            event,
                            &mut current_config,
                            &config_path,
                            &mut watcher,
                            &mut watched_roots,
                            &mut last_config_reload,
                            &mut last_file_events,
                            db,
                            &mut filter_evaluator,
                        );
                        // Spawn DB writes so the event loop (and IPC accept) stay responsive.
                        for (path, tags) in work {
                            let db_ref = db.clone();
                            tokio::spawn(async move {
                                println!("Auto-tagging {:?} with {:?}", path, tags);
                                let result = tokio::task::spawn_blocking(move || {
                                    let mut stdout = std::io::stdout();
                                    crate::commands::tag::execute(
                                        &db_ref,
                                        Some(path.clone()),
                                        &tags,
                                        false,
                                        true,
                                        &mut stdout,
                                    )
                                    .map_err(|e| (path, e))
                                })
                                .await;
                                match result {
                                    Ok(Err((path, e))) => eprintln!("Auto-tag failed for {:?}: {}", path, e),
                                    Err(e) => eprintln!("Spawn error: {}", e),
                                    Ok(Ok(())) => {}
                                }
                            });
                        }
                    }
                    Err(e) => eprintln!("Watcher error: {}", e),
                }
            }
            Ok(conn) = listener.accept() => {
                let db_ref = db.clone();
                let tx = shutdown_tx.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_ipc_connection(conn, &db_ref, tx).await {
                        eprintln!("IPC error: {}", e);
                    }
                });
            }
            _ = shutdown_rx.recv() => {
                println!("Shutdown requested. Stopping daemon.");
                break;
            }
        }
    }

    // Delete the socket so callers waiting on it know the daemon has fully stopped.
    // This happens before the sled DB is dropped, giving a reliable sync point.
    let _ = std::fs::remove_file(&socket_path);
    Ok(())
}

/// Handle a single filesystem event.
///
/// Returns a list of `(path, tags)` pairs that should be applied to the database.
/// The caller is responsible for spawning the actual DB writes off the event loop
/// so that IPC connections are never blocked.
#[allow(clippy::too_many_arguments)]
fn handle_event(
    event: Event,
    config: &mut WatchConfig,
    config_path: &Path,
    watcher: &mut RecommendedWatcher,
    watched_roots: &mut HashSet<PathBuf>,
    last_config_reload: &mut Option<Instant>,
    last_file_events: &mut HashMap<PathBuf, Instant>,
    db: &Database,
    filter_evaluator: &mut FilterEvaluator,
) -> Vec<(PathBuf, Vec<String>)> {
    // If any path in the event IS the config file, reload config — with debounce.
    // A single `fs::write` to watch.toml triggers two inotify events
    // (Modify(Data) + Access(Close(Write))); we suppress the second one.
    if event.paths.iter().any(|p| p == config_path) {
        let now = Instant::now();
        let too_soon = last_config_reload
            .map(|prev| now.duration_since(prev) < Duration::from_millis(500))
            .unwrap_or(false);
        if too_soon {
            return vec![];
        }
        *last_config_reload = Some(now);
        println!("watch.toml changed, reloading config...");
        match WatchConfig::load() {
            Ok(mut new_config) => {
                resolve_all_rules(&mut new_config.rules);
                *config = new_config;
                println!("Config reloaded: {} rules", config.rules.len());
                add_new_watch_roots(watcher, config, watched_roots);
            }
            Err(e) => eprintln!("Failed to reload watch config: {}", e),
        }
        return vec![];
    }

    // For Create / Modify / Close-Write events, apply matching rules.
    let is_relevant = matches!(
        event.kind,
        EventKind::Create(_)
            | EventKind::Modify(_)
            | EventKind::Access(notify::event::AccessKind::Close(
                notify::event::AccessMode::Write
            ))
    );
    if !is_relevant {
        return vec![];
    }

    let now = Instant::now();
    let mut work: Vec<(PathBuf, Vec<String>)> = Vec::new();
    for path in &event.paths {
        // Debounce: skip if we already processed this path within 300ms.
        // A single file operation (touch, write) fires 3–6 inotify events;
        // we only need to act on the first one.
        let too_soon = last_file_events
            .get(path)
            .map(|&prev| now.duration_since(prev) < Duration::from_millis(300))
            .unwrap_or(false);
        if too_soon {
            continue;
        }
        last_file_events.insert(path.clone(), now);

        for rule in &config.rules {
            if rule.tags.is_empty() {
                continue;
            }
            if !matches_patterns(path, &rule.patterns) {
                continue;
            }
            // Check vtags / filter_by_tags / saved-filter criteria when present.
            if let Some(criteria) = &rule.filter_criteria {
                if !filter_evaluator.matches(path, criteria, db) {
                    continue;
                }
            }
            work.push((path.clone(), rule.tags.clone()));
        }
    }
    work
}

/// Resolve each rule's named filter + inline vtags + filter_by_tags into
/// its `filter_criteria` field so that the hot path only does evaluation,
/// not I/O.
fn resolve_all_rules(rules: &mut [WatchRule]) {
    let filter_manager = get_filter_path()
        .ok()
        .map(FilterManager::new);

    for rule in rules.iter_mut() {
        let mut criteria = FilterCriteria::default();

        // Merge any saved/named filter first so inline flags can override.
        if let Some(name) = &rule.filter {
            if let Some(ref fm) = filter_manager {
                match fm.get(name) {
                    Ok(f) => criteria = f.criteria.clone(),
                    Err(e) => eprintln!("Watch: could not load filter '{name}': {e}"),
                }
            }
        }

        // Inline vtag conditions (AND with whatever the named filter required).
        criteria.virtual_tags.extend(rule.vtags.iter().cloned());

        // DB-tag gate: file must already carry all of these tags.
        criteria.tags.extend(rule.filter_by_tags.iter().cloned());

        let is_empty = criteria.tags.is_empty()
            && criteria.virtual_tags.is_empty()
            && criteria.file_patterns.is_empty()
            && criteria.excludes.is_empty();

        rule.filter_criteria = if is_empty { None } else { Some(criteria) };
    }
}

/// Register any watch roots from the config that are not yet watched.
/// We never remove roots to avoid races; a daemon restart cleans up.
fn add_new_watch_roots(
    watcher: &mut RecommendedWatcher,
    config: &WatchConfig,
    watched: &mut HashSet<PathBuf>,
) {
    for rule in &config.rules {
        for pattern in &rule.patterns {
            let root = glob_parent(pattern);
            if watched.contains(&root) {
                continue;
            }
            if !root.exists() {
                eprintln!(
                    "Watch root {:?} does not exist yet (will not be watched until it is created)",
                    root
                );
                continue;
            }
            println!("Watching directory: {:?}", root);
            match watcher.watch(&root, RecursiveMode::Recursive) {
                Ok(()) => {
                    watched.insert(root);
                }
                Err(e) => eprintln!("Failed to watch {:?}: {}", root, e),
            }
        }
    }
}

/// Extract the non-glob prefix of a pattern as the directory to watch.
/// Expands a leading `~/` to the user's home directory.
fn glob_parent(pattern: &str) -> PathBuf {
    let expanded = if pattern.starts_with("~/") {
        dirs::home_dir()
            .map(|h| pattern.replacen('~', h.to_string_lossy().as_ref(), 1))
            .unwrap_or_else(|| pattern.to_string())
    } else {
        pattern.to_string()
    };

    let path = PathBuf::from(&expanded);
    let mut p = path.as_path();

    loop {
        let s = p.to_string_lossy();
        if !s.contains('*') && !s.contains('?') && !s.contains('[') && !s.contains('{') {
            return p.to_path_buf();
        }
        match p.parent() {
            Some(parent) => p = parent,
            None => return PathBuf::from("."),
        }
    }
}

// ---------------------------------------------------------------------------
// IPC
// ---------------------------------------------------------------------------

async fn handle_ipc_connection(
    conn: LocalSocketStream,
    db: &Database,
    shutdown_tx: tokio::sync::mpsc::Sender<()>,
) -> Result<()> {
    let (reader, mut writer) = tokio::io::split(conn);
    let mut reader = TokioBufReader::new(reader);

    let mut line = String::new();
    reader.read_line(&mut line).await?;
    if line.is_empty() {
        return Ok(());
    }

    let req: IpcRequest = serde_json::from_str(&line).context("Failed to parse IPC request")?;
    let (resp, should_shutdown) = execute_ipc_command(req, db);

    let resp_str = serde_json::to_string(&resp)?;
    writer.write_all(resp_str.as_bytes()).await?;
    writer.write_all(b"\n").await?;

    if should_shutdown {
        let _ = shutdown_tx.send(()).await;
    }
    Ok(())
}

fn execute_ipc_command(req: IpcRequest, db: &Database) -> (IpcResponse, bool) {
    match req {
        IpcRequest::Ping => (IpcResponse::Success("pong".to_string()), false),
        IpcRequest::Shutdown => (IpcResponse::Success("ok".to_string()), true),
        IpcRequest::Command { args, cwd } => {
            use clap::Parser as _;
            let cli = match crate::cli::Cli::try_parse_from(&args) {
                Ok(c) => c,
                Err(e) => return (IpcResponse::Error(e.to_string()), false),
            };

            let mut command = cli.get_command();
            let base = PathBuf::from(cwd);
            resolve_relative_paths(&mut command, &base);

            let config = match crate::config::TagrConfig::load() {
                Ok(c) => c,
                Err(e) => return (IpcResponse::Error(e.to_string()), false),
            };

            let quiet = cli.quiet || config.quiet;
            let path_format = if let Some(cli_format) = cli.get_path_format() {
                match cli_format {
                    crate::cli::PathFormat::Absolute => crate::config::PathFormat::Absolute,
                    crate::cli::PathFormat::Relative => crate::config::PathFormat::Relative,
                }
            } else {
                config.path_format
            };

            let mut buf = Vec::new();
            match crate::commands::dispatch_command(
                &command,
                db,
                &config,
                path_format,
                quiet,
                &mut buf,
            ) {
                Ok(()) => (IpcResponse::Success(String::from_utf8_lossy(&buf).into_owned()), false),
                Err(e) => (IpcResponse::Error(e.to_string()), false),
            }
        }
    }
}

fn resolve_relative_paths(command: &mut crate::cli::Commands, base: &Path) {
    use crate::cli::Commands;
    match command {
        Commands::Tag { file_flag, file_pos, .. } => {
            if let Some(p) = file_flag {
                *p = make_absolute(p, base);
            }
            if let Some(p) = file_pos {
                *p = make_absolute(p, base);
            }
        }
        Commands::Untag { file_flag, file_pos, .. } => {
            if let Some(p) = file_flag {
                *p = make_absolute(p, base);
            }
            if let Some(p) = file_pos {
                *p = make_absolute(p, base);
            }
        }
        _ => {}
    }
}

fn make_absolute(p: &Path, base: &Path) -> PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        base.join(p)
    }
}
