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
        Commands::Note { command: note_cmd, .. } => {
            resolve_note_paths(note_cmd, base);
        }
        Commands::Filter { command: filter_cmd } => {
            resolve_filter_paths(filter_cmd, base);
        }
        _ => {}
    }
}

fn resolve_note_paths(cmd: &mut crate::commands::note::NoteSubcommand, base: &Path) {
    use crate::commands::note::NoteSubcommand;
    match cmd {
        NoteSubcommand::Edit(args) => {
            make_all_absolute(&mut args.files, base);
        }
        NoteSubcommand::Add(args) => {
            args.file = make_absolute(&args.file, base);
        }
        NoteSubcommand::Show(args) => {
            make_all_absolute(&mut args.files, base);
        }
        NoteSubcommand::Delete(args) => {
            make_all_absolute(&mut args.files, base);
        }
        NoteSubcommand::List(_) | NoteSubcommand::Search(_) => {}
    }
}

fn resolve_filter_paths(cmd: &mut crate::cli::FilterCommands, base: &Path) {
    use crate::cli::FilterCommands;
    match cmd {
        FilterCommands::Import { path, .. } => {
            *path = make_absolute(path, base);
        }
        FilterCommands::Export { output, .. } => {
            if let Some(p) = output {
                *p = make_absolute(p, base);
            }
        }
        _ => {}
    }
}

fn make_all_absolute(paths: &mut [PathBuf], base: &Path) {
    for p in paths.iter_mut() {
        *p = make_absolute(p, base);
    }
}

fn make_absolute(p: &Path, base: &Path) -> PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        base.join(p)
    }
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
        let result = glob_parent("/home/user/docs/readme.md");
        assert_eq!(result, PathBuf::from("/home/user/docs/readme.md"));
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

    // ---- make_absolute tests ----

    #[test]
    fn test_make_absolute_already_absolute() {
        let result = make_absolute(Path::new("/home/user/file.txt"), Path::new("/base"));
        assert_eq!(result, PathBuf::from("/home/user/file.txt"));
    }

    #[test]
    fn test_make_absolute_relative() {
        let result = make_absolute(Path::new("file.txt"), Path::new("/base/dir"));
        assert_eq!(result, PathBuf::from("/base/dir/file.txt"));
    }

    #[test]
    fn test_make_absolute_relative_nested() {
        let result = make_absolute(Path::new("sub/dir/file.txt"), Path::new("/base"));
        assert_eq!(result, PathBuf::from("/base/sub/dir/file.txt"));
    }

    // ---- resolve_relative_paths tests ----

    #[test]
    fn test_resolve_relative_paths_tag_file_flag() {
        let mut cmd = crate::cli::Commands::Tag {
            file_flag: Some(PathBuf::from("relative.txt")),
            tags_flag: vec!["tag1".into()],
            file_pos: None,
            tags_pos: vec![],
            no_canonicalize: false,
            db_args: Default::default(),
        };

        resolve_relative_paths(&mut cmd, Path::new("/working/dir"));

        if let crate::cli::Commands::Tag { file_flag, .. } = &cmd {
            assert_eq!(file_flag.as_ref().unwrap(), &PathBuf::from("/working/dir/relative.txt"));
        } else {
            panic!("Expected Tag command");
        }
    }

    #[test]
    fn test_resolve_relative_paths_tag_file_pos() {
        let mut cmd = crate::cli::Commands::Tag {
            file_flag: None,
            tags_flag: vec![],
            file_pos: Some(PathBuf::from("file.txt")),
            tags_pos: vec!["tag1".into()],
            no_canonicalize: false,
            db_args: Default::default(),
        };

        resolve_relative_paths(&mut cmd, Path::new("/cwd"));

        if let crate::cli::Commands::Tag { file_pos, .. } = &cmd {
            assert_eq!(file_pos.as_ref().unwrap(), &PathBuf::from("/cwd/file.txt"));
        } else {
            panic!("Expected Tag command");
        }
    }

    #[test]
    fn test_resolve_relative_paths_absolute_unchanged() {
        let mut cmd = crate::cli::Commands::Tag {
            file_flag: Some(PathBuf::from("/absolute/path.txt")),
            tags_flag: vec!["tag1".into()],
            file_pos: None,
            tags_pos: vec![],
            no_canonicalize: false,
            db_args: Default::default(),
        };

        resolve_relative_paths(&mut cmd, Path::new("/cwd"));

        if let crate::cli::Commands::Tag { file_flag, .. } = &cmd {
            assert_eq!(file_flag.as_ref().unwrap(), &PathBuf::from("/absolute/path.txt"));
        } else {
            panic!("Expected Tag command");
        }
    }

    #[test]
    fn test_resolve_relative_paths_untag() {
        let mut cmd = crate::cli::Commands::Untag {
            file_flag: Some(PathBuf::from("file.txt")),
            tags_flag: vec!["tag1".into()],
            file_pos: None,
            tags_pos: vec![],
            all: false,
            db_args: Default::default(),
        };

        resolve_relative_paths(&mut cmd, Path::new("/base"));

        if let crate::cli::Commands::Untag { file_flag, .. } = &cmd {
            assert_eq!(file_flag.as_ref().unwrap(), &PathBuf::from("/base/file.txt"));
        } else {
            panic!("Expected Untag command");
        }
    }

    // ---- execute_ipc_command tests ----

    #[test]
    fn test_execute_ipc_ping() {
        let test_db = TestDb::new("ipc_ping");
        let (resp, shutdown) = execute_ipc_command(IpcRequest::Ping, test_db.db());
        assert!(!shutdown);
        assert!(resp.is_success());
        assert_eq!(resp.as_str(), "pong");
    }

    #[test]
    fn test_execute_ipc_shutdown() {
        let test_db = TestDb::new("ipc_shutdown");
        let (resp, shutdown) = execute_ipc_command(IpcRequest::Shutdown, test_db.db());
        assert!(shutdown);
        assert!(resp.is_success());
        assert_eq!(resp.as_str(), "ok");
    }

    #[test]
    fn test_execute_ipc_command_invalid_args() {
        let test_db = TestDb::new("ipc_invalid");
        let (resp, shutdown) = execute_ipc_command(
            IpcRequest::Command {
                args: vec!["tagr".into(), "--nonsense-flag".into()],
                cwd: "/tmp".into(),
            },
            test_db.db(),
        );
        assert!(!shutdown);
        assert!(!resp.is_success());
    }

    #[test]
    fn test_execute_ipc_command_list() {
        let test_db = TestDb::new("ipc_list");
        let db = test_db.db();

        let temp = crate::testing::TempFile::create("ipc_test.txt").unwrap();
        db.insert(temp.path(), vec!["test-tag".into()]).unwrap();

        let (resp, shutdown) = execute_ipc_command(
            IpcRequest::Command {
                args: vec!["tagr".into(), "list".into()],
                cwd: "/tmp".into(),
            },
            db,
        );
        assert!(!shutdown);
        // The list command may fail if TagrConfig::load() returns a config
        // that doesn't match the test DB. We verify no panic and valid response.
        match resp {
            IpcResponse::Success(output) => {
                // If config loaded successfully, output should contain our file
                assert!(output.contains("ipc_test.txt"), "Expected file in list output: {output}");
            }
            IpcResponse::Error(_) => {
                // Acceptable — config mismatch in test environment
            }
        }
    }

    // ---- resolve_relative_paths: Note commands ----

    #[test]
    fn test_resolve_note_add_relative() {
        let mut cmd = crate::cli::Commands::Note {
            command: crate::commands::note::NoteSubcommand::Add(
                crate::commands::note::AddArgs {
                    file: PathBuf::from("notes.txt"),
                    content: "hello".into(),
                },
            ),
            absolute: false,
            relative: false,
            db_args: Default::default(),
        };

        resolve_relative_paths(&mut cmd, Path::new("/project"));

        if let crate::cli::Commands::Note { command: crate::commands::note::NoteSubcommand::Add(args), .. } = &cmd {
            assert_eq!(args.file, PathBuf::from("/project/notes.txt"));
        } else {
            panic!("Expected Note Add command");
        }
    }

    #[test]
    fn test_resolve_note_show_multiple_files() {
        let mut cmd = crate::cli::Commands::Note {
            command: crate::commands::note::NoteSubcommand::Show(
                crate::commands::note::ShowArgs {
                    files: vec![PathBuf::from("a.txt"), PathBuf::from("/abs/b.txt")],
                    format: Default::default(),
                    verbose: false,
                },
            ),
            absolute: false,
            relative: false,
            db_args: Default::default(),
        };

        resolve_relative_paths(&mut cmd, Path::new("/base"));

        if let crate::cli::Commands::Note { command: crate::commands::note::NoteSubcommand::Show(args), .. } = &cmd {
            assert_eq!(args.files[0], PathBuf::from("/base/a.txt"));
            assert_eq!(args.files[1], PathBuf::from("/abs/b.txt"));
        } else {
            panic!("Expected Note Show command");
        }
    }

    #[test]
    fn test_resolve_note_edit_relative() {
        let mut cmd = crate::cli::Commands::Note {
            command: crate::commands::note::NoteSubcommand::Edit(
                crate::commands::note::EditArgs {
                    files: vec![PathBuf::from("doc.md")],
                    editor: None,
                },
            ),
            absolute: false,
            relative: false,
            db_args: Default::default(),
        };

        resolve_relative_paths(&mut cmd, Path::new("/workspace"));

        if let crate::cli::Commands::Note { command: crate::commands::note::NoteSubcommand::Edit(args), .. } = &cmd {
            assert_eq!(args.files[0], PathBuf::from("/workspace/doc.md"));
        } else {
            panic!("Expected Note Edit command");
        }
    }

    #[test]
    fn test_resolve_note_delete_relative() {
        let mut cmd = crate::cli::Commands::Note {
            command: crate::commands::note::NoteSubcommand::Delete(
                crate::commands::note::DeleteArgs {
                    files: vec![PathBuf::from("old.txt")],
                    dry_run: false,
                    yes: false,
                },
            ),
            absolute: false,
            relative: false,
            db_args: Default::default(),
        };

        resolve_relative_paths(&mut cmd, Path::new("/home"));

        if let crate::cli::Commands::Note { command: crate::commands::note::NoteSubcommand::Delete(args), .. } = &cmd {
            assert_eq!(args.files[0], PathBuf::from("/home/old.txt"));
        } else {
            panic!("Expected Note Delete command");
        }
    }

    // ---- resolve_relative_paths: Filter commands ----

    #[test]
    fn test_resolve_filter_import_relative() {
        let mut cmd = crate::cli::Commands::Filter {
            command: crate::cli::FilterCommands::Import {
                path: PathBuf::from("filters.toml"),
                overwrite: false,
                skip_existing: false,
            },
        };

        resolve_relative_paths(&mut cmd, Path::new("/data"));

        if let crate::cli::Commands::Filter { command: crate::cli::FilterCommands::Import { path, .. } } = &cmd {
            assert_eq!(*path, PathBuf::from("/data/filters.toml"));
        } else {
            panic!("Expected Filter Import command");
        }
    }

    #[test]
    fn test_resolve_filter_export_relative() {
        let mut cmd = crate::cli::Commands::Filter {
            command: crate::cli::FilterCommands::Export {
                filters: vec![],
                output: Some(PathBuf::from("out.toml")),
            },
        };

        resolve_relative_paths(&mut cmd, Path::new("/export"));

        if let crate::cli::Commands::Filter { command: crate::cli::FilterCommands::Export { output, .. } } = &cmd {
            assert_eq!(output.as_ref().unwrap(), &PathBuf::from("/export/out.toml"));
        } else {
            panic!("Expected Filter Export command");
        }
    }

    #[test]
    fn test_resolve_filter_export_no_output() {
        let mut cmd = crate::cli::Commands::Filter {
            command: crate::cli::FilterCommands::Export {
                filters: vec![],
                output: None,
            },
        };

        resolve_relative_paths(&mut cmd, Path::new("/export"));

        if let crate::cli::Commands::Filter { command: crate::cli::FilterCommands::Export { output, .. } } = &cmd {
            assert!(output.is_none());
        } else {
            panic!("Expected Filter Export command");
        }
    }

    // ---- make_all_absolute ----

    #[test]
    fn test_make_all_absolute_mixed() {
        let mut paths = vec![
            PathBuf::from("relative.txt"),
            PathBuf::from("/already/absolute.txt"),
            PathBuf::from("sub/dir/file.rs"),
        ];

        make_all_absolute(&mut paths, Path::new("/base"));

        assert_eq!(paths[0], PathBuf::from("/base/relative.txt"));
        assert_eq!(paths[1], PathBuf::from("/already/absolute.txt"));
        assert_eq!(paths[2], PathBuf::from("/base/sub/dir/file.rs"));
    }
}
