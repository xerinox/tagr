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

/// Guard that removes the IPC socket file when dropped.
/// Ensures cleanup happens on normal exit, IPC shutdown, signal, or panic.
struct SocketGuard(PathBuf);

impl Drop for SocketGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Run the daemon event loop (blocking — starts its own async runtime).
///
/// # Errors
///
/// Returns an error if the async runtime or IPC listener cannot be created.
pub fn run(db: &Database) -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async_run(db))
}

/// Returns a future that resolves on SIGTERM (Unix) or never resolves (other platforms).
#[cfg(unix)]
async fn sigterm_or_pending() {
    let mut sig = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("failed to register SIGTERM handler");
    sig.recv().await;
}

/// Returns a future that never resolves (SIGTERM not available on this platform).
#[cfg(not(unix))]
async fn sigterm_or_pending() {
    std::future::pending::<()>().await;
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

    // Guard ensures socket is cleaned up on normal exit, signal, or panic.
    let _socket_guard = SocketGuard(socket_path.clone());

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

    // Retroactively tag existing files that match the initial rules.
    let initial_work = retroactive_scan(&current_config, db, &mut filter_evaluator);
    spawn_tag_work(initial_work, db);

    println!("Daemon ready.");

    let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);

    // Cross-platform Ctrl-C (SIGINT on Unix, Ctrl-C on Windows).
    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);

    // SIGTERM on Unix; a never-completing future on other platforms.
    let sigterm = sigterm_or_pending();
    tokio::pin!(sigterm);

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
            _ = &mut ctrl_c => {
                println!("Received SIGINT. Stopping daemon.");
                break;
            }
            _ = &mut sigterm => {
                println!("Received SIGTERM. Stopping daemon.");
                break;
            }
            _ = shutdown_rx.recv() => {
                println!("Shutdown requested. Stopping daemon.");
                break;
            }
        }
    }

    // SocketGuard removes the socket file on drop, which happens here
    // before the sled DB is dropped — giving callers a reliable sync point.
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
                // Retroactively tag files matching new/changed rules.
                return retroactive_scan(config, db, filter_evaluator);
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
///
/// For literal file paths (no glob characters), returns the parent directory
/// so that inotify watches the containing directory rather than a single file.
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

    let s = p.to_string_lossy();
    let has_glob = s.contains('*') || s.contains('?') || s.contains('[') || s.contains('{');

    if !has_glob {
        // Literal path — watch the parent directory so we catch sibling
        // creates and modifications, not just a single file.
        return p.parent().map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    }

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
// Retroactive scanning
// ---------------------------------------------------------------------------

/// Scan the filesystem for files that match the configured watch rules and
/// return `(path, tags)` pairs for files that don't already have the required
/// tags.  This is called on daemon startup and on every config reload so that
/// existing files are tagged immediately — the user doesn't have to modify a
/// file just to trigger the rule.
fn retroactive_scan(
    config: &WatchConfig,
    db: &Database,
    filter_evaluator: &mut FilterEvaluator,
) -> Vec<(PathBuf, Vec<String>)> {
    let mut work: Vec<(PathBuf, Vec<String>)> = Vec::new();

    for rule in &config.rules {
        if rule.tags.is_empty() {
            continue;
        }

        for pattern in &rule.patterns {
            let expanded = expand_tilde(pattern);
            let has_glob = expanded.contains('*')
                || expanded.contains('?')
                || expanded.contains('[')
                || expanded.contains('{');

            let paths: Vec<PathBuf> = if has_glob {
                // Expand the glob to concrete file paths.
                glob::glob(&expanded)
                    .into_iter()
                    .flatten()
                    .filter_map(|r| r.ok())
                    .filter(|p| p.is_file())
                    .collect()
            } else {
                // Literal path — treat it as a single file.
                let p = PathBuf::from(&expanded);
                if p.is_file() { vec![p] } else { vec![] }
            };

            for path in &paths {
                // Skip if the file already carries all the rule's tags.
                if let Ok(Some(existing)) = db.get_tags(path) {
                    if rule.tags.iter().all(|t| existing.contains(t)) {
                        continue;
                    }
                }

                // Honour filter criteria if any.
                if let Some(criteria) = &rule.filter_criteria {
                    if !filter_evaluator.matches(path, criteria, db) {
                        continue;
                    }
                }

                work.push((path.clone(), rule.tags.clone()));
            }
        }
    }

    if !work.is_empty() {
        println!("Retroactive scan: {} files to tag", work.len());
    }

    work
}

/// Spawn tag operations as blocking tasks so the event loop stays responsive.
fn spawn_tag_work(work: Vec<(PathBuf, Vec<String>)>, db: &Database) {
    for (path, tags) in work {
        let db_ref = db.clone();
        tokio::spawn(async move {
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
                Ok(Err((path, e))) => eprintln!("Retroactive tag failed for {:?}: {}", path, e),
                Err(e) => eprintln!("Spawn error: {}", e),
                Ok(Ok(())) => {}
            }
        });
    }
}

/// Expand a leading `~/` to the user's home directory.
fn expand_tilde(pattern: &str) -> String {
    if pattern.starts_with("~/") {
        dirs::home_dir()
            .map(|h| pattern.replacen('~', h.to_string_lossy().as_ref(), 1))
            .unwrap_or_else(|| pattern.to_string())
    } else {
        pattern.to_string()
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
        IpcRequest::Ping => (IpcResponse::Pong, false),
        IpcRequest::Shutdown => (IpcResponse::Ok, true),

        IpcRequest::ListTags => {
            match db.list_all_tags() {
                Ok(tag_names) => {
                    let tags: Vec<_> = tag_names
                        .into_iter()
                        .map(|name| {
                            let file_count = db.find_by_tag(&name).map_or(0, |f| f.len());
                            crate::ipc::TagInfo { name, file_count }
                        })
                        .collect();
                    (IpcResponse::Tags(tags), false)
                }
                Err(e) => (IpcResponse::Error(e.to_string()), false),
            }
        }

        IpcRequest::ListFiles => {
            match db.list_all() {
                Ok(pairs) => (IpcResponse::Files(pairs), false),
                Err(e) => (IpcResponse::Error(e.to_string()), false),
            }
        }

        IpcRequest::SearchFiles { params } => {
            match execute_search(db, &params) {
                Ok(pairs) => (IpcResponse::Files(pairs), false),
                Err(e) => (IpcResponse::Error(e.to_string()), false),
            }
        }

        IpcRequest::GetTags { file } => {
            match db.get_tags(&file) {
                Ok(Some(tags)) => (IpcResponse::FileTags(tags), false),
                Ok(None) => (IpcResponse::FileTags(vec![]), false),
                Err(e) => (IpcResponse::Error(e.to_string()), false),
            }
        }

        IpcRequest::FindByTag { tag } => {
            match db.find_by_tag(&tag) {
                Ok(paths) => (IpcResponse::FilePaths(paths), false),
                Err(e) => (IpcResponse::Error(e.to_string()), false),
            }
        }

        IpcRequest::FindByTags { tags, match_all } => {
            let result = if match_all {
                db.find_by_all_tags(&tags)
            } else {
                db.find_by_any_tag(&tags)
            };
            match result {
                Ok(paths) => (IpcResponse::FilePaths(paths), false),
                Err(e) => (IpcResponse::Error(e.to_string()), false),
            }
        }

        IpcRequest::FindByTagRegex { pattern } => {
            match db.find_by_tag_regex(&pattern) {
                Ok(paths) => (IpcResponse::FilePaths(paths), false),
                Err(e) => (IpcResponse::Error(e.to_string()), false),
            }
        }

        IpcRequest::ListAllPaths => {
            match db.list_all_files() {
                Ok(paths) => (IpcResponse::FilePaths(paths), false),
                Err(e) => (IpcResponse::Error(e.to_string()), false),
            }
        }

        IpcRequest::ListNotes => {
            match db.list_all_notes() {
                Ok(notes) => {
                    let entries = notes
                        .into_iter()
                        .map(|(path, note)| crate::ipc::NoteEntry { path, note })
                        .collect();
                    (IpcResponse::Notes(entries), false)
                }
                Err(e) => (IpcResponse::Error(e.to_string()), false),
            }
        }

        IpcRequest::AddTags { file, tags } => {
            let mut stdout = std::io::stdout();
            match crate::commands::tag::execute(db, Some(file), &tags, false, true, &mut stdout) {
                Ok(()) => (IpcResponse::Ok, false),
                Err(e) => (IpcResponse::Error(e.to_string()), false),
            }
        }

        IpcRequest::SetTags { file, tags } => {
            match db.insert(&file, tags) {
                Ok(()) => (IpcResponse::Ok, false),
                Err(e) => (IpcResponse::Error(e.to_string()), false),
            }
        }

        IpcRequest::RemoveTags { file, tags, all } => {
            let mut stdout = std::io::stdout();
            match crate::commands::tag::untag(db, Some(file), &tags, all, true, &mut stdout) {
                Ok(()) => (IpcResponse::Ok, false),
                Err(e) => (IpcResponse::Error(e.to_string()), false),
            }
        }

        IpcRequest::DeleteFromDb { file } => {
            match db.remove(&file) {
                Ok(true) => (IpcResponse::Ok, false),
                Ok(false) => (IpcResponse::Error(format!("File not found in database: {}", file.display())), false),
                Err(e) => (IpcResponse::Error(e.to_string()), false),
            }
        }

        IpcRequest::Cleanup => {
            let mut removed = 0usize;
            match db.list_all() {
                Ok(pairs) => {
                    for pair in &pairs {
                        if !pair.file.exists() && db.remove(&pair.file).is_ok() {
                            removed += 1;
                        }
                    }
                    (IpcResponse::CleanupResult { removed }, false)
                }
                Err(e) => (IpcResponse::Error(e.to_string()), false),
            }
        }
    }
}

/// Execute a search query and return matching file-tag pairs.
fn execute_search(db: &Database, params: &crate::cli::SearchParams) -> std::result::Result<Vec<crate::Pair>, crate::db::DbError> {
    use crate::search::expand_tags;

    let schema = crate::schema::load_default_schema().ok().unwrap_or_default();

    if params.tags.is_empty() && params.file_patterns.is_empty() && params.virtual_tags.is_empty() {
        return db.list_all();
    }

    let mut results: Vec<crate::Pair> = if params.tags.is_empty() {
        db.list_all()?
    } else {
        let expanded = expand_tags(&params.tags, &schema, db, !params.no_hierarchy)?;
        let mut files = std::collections::HashSet::new();
        for tag in &expanded {
            for path in db.find_by_tag(tag)? {
                files.insert(path);
            }
        }
        files
            .into_iter()
            .filter_map(|path| {
                db.get_tags(&path).ok().flatten().map(|tags| crate::Pair::new(path, tags))
            })
            .collect()
    };

    // Apply file pattern filters
    if !params.file_patterns.is_empty() {
        results.retain(|pair| {
            let path_str = pair.file.to_string_lossy();
            params.file_patterns.iter().any(|p| path_str.contains(p))
        });
    }

    // Apply exclude tags
    if !params.exclude_tags.is_empty() {
        results.retain(|pair| {
            !params.exclude_tags.iter().any(|ex| pair.tags.contains(ex))
        });
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::{IpcRequest, IpcResponse};
    use crate::testing::TestDb;

    // ---- glob_parent tests ----

    #[test]
    fn test_glob_parent_simple_wildcard() {
        let result = glob_parent("/home/user/docs/*.md");
        assert_eq!(result, PathBuf::from("/home/user/docs"));
    }

    #[test]
    fn test_glob_parent_recursive_wildcard() {
        let result = glob_parent("/home/user/projects/**/*.rs");
        assert_eq!(result, PathBuf::from("/home/user/projects"));
    }

    #[test]
    fn test_glob_parent_no_glob() {
        // Literal file path → returns parent directory so we watch the dir,
        // not the individual file.
        let result = glob_parent("/home/user/docs/readme.md");
        assert_eq!(result, PathBuf::from("/home/user/docs"));
    }

    #[test]
    fn test_glob_parent_glob_in_middle() {
        let result = glob_parent("/home/*/docs/*.md");
        assert_eq!(result, PathBuf::from("/home"));
    }

    #[test]
    fn test_glob_parent_question_mark() {
        let result = glob_parent("/tmp/file?.txt");
        assert_eq!(result, PathBuf::from("/tmp"));
    }

    #[test]
    fn test_glob_parent_bracket_pattern() {
        let result = glob_parent("/tmp/[abc].txt");
        assert_eq!(result, PathBuf::from("/tmp"));
    }

    #[test]
    fn test_glob_parent_tilde_expansion() {
        let result = glob_parent("~/docs/*.md");
        if let Some(home) = dirs::home_dir() {
            assert_eq!(result, home.join("docs"));
        }
    }

    #[test]
    fn test_glob_parent_pure_glob() {
        // A glob with no directory prefix resolves to empty string (no parent)
        let result = glob_parent("*.txt");
        assert!(
            result == PathBuf::from(".") || result == PathBuf::from(""),
            "Expected '.' or '', got: {:?}",
            result
        );
    }

    // ---- execute_ipc_command tests ----

    #[test]
    fn test_execute_ipc_ping() {
        let test_db = TestDb::new("ipc_ping");
        let (resp, shutdown) = execute_ipc_command(IpcRequest::Ping, test_db.db());
        assert!(!shutdown);
        assert!(matches!(resp, IpcResponse::Pong));
    }

    #[test]
    fn test_execute_ipc_shutdown() {
        let test_db = TestDb::new("ipc_shutdown");
        let (resp, shutdown) = execute_ipc_command(IpcRequest::Shutdown, test_db.db());
        assert!(shutdown);
        assert!(matches!(resp, IpcResponse::Ok));
    }

    #[test]
    fn test_execute_ipc_list_tags() {
        let test_db = TestDb::new("ipc_list_tags");
        let db = test_db.db();

        let temp = crate::testing::TempFile::create("ipc_tags.txt").unwrap();
        db.insert(temp.path(), vec!["alpha".into(), "beta".into()]).unwrap();

        let (resp, _) = execute_ipc_command(IpcRequest::ListTags, db);
        match resp {
            IpcResponse::Tags(tags) => {
                let names: Vec<_> = tags.iter().map(|t| t.name.as_str()).collect();
                assert!(names.contains(&"alpha"));
                assert!(names.contains(&"beta"));
            }
            _ => panic!("Expected Tags response, got: {resp:?}"),
        }
    }

    #[test]
    fn test_execute_ipc_list_files() {
        let test_db = TestDb::new("ipc_list_files");
        let db = test_db.db();

        let temp = crate::testing::TempFile::create("ipc_files.txt").unwrap();
        db.insert(temp.path(), vec!["tag1".into()]).unwrap();

        let (resp, _) = execute_ipc_command(IpcRequest::ListFiles, db);
        match resp {
            IpcResponse::Files(files) => {
                assert_eq!(files.len(), 1);
                assert!(files[0].tags.contains(&"tag1".to_string()));
            }
            _ => panic!("Expected Files response, got: {resp:?}"),
        }
    }

    #[test]
    fn test_execute_ipc_get_tags() {
        let test_db = TestDb::new("ipc_get_tags");
        let db = test_db.db();

        let temp = crate::testing::TempFile::create("ipc_gettags.txt").unwrap();
        db.insert(temp.path(), vec!["x".into(), "y".into()]).unwrap();

        let (resp, _) = execute_ipc_command(
            IpcRequest::GetTags { file: temp.path().to_path_buf() },
            db,
        );
        match resp {
            IpcResponse::FileTags(tags) => {
                assert!(tags.contains(&"x".to_string()));
                assert!(tags.contains(&"y".to_string()));
            }
            _ => panic!("Expected FileTags response, got: {resp:?}"),
        }
    }

    #[test]
    fn test_execute_ipc_get_tags_missing_file() {
        let test_db = TestDb::new("ipc_get_tags_missing");
        let (resp, _) = execute_ipc_command(
            IpcRequest::GetTags { file: PathBuf::from("/nonexistent/file.txt") },
            test_db.db(),
        );
        match resp {
            IpcResponse::FileTags(tags) => assert!(tags.is_empty()),
            _ => panic!("Expected empty FileTags, got: {resp:?}"),
        }
    }
}
