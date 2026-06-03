//! Core daemon logic: event loop, file watching, and IPC handling.

use crate::db::Database;
use crate::filters::{FilterManager, get_filter_path};
use crate::store::DirectStore;
use crate::types::QueryCriteria;
use crate::ipc::get_ipc_socket_path;
use crate::ipc::wire::{
    self, ClientMessage, Request, Response, ServerEvent, ServerMessage,
    WireFilePair, WireNoteEntry, WireTagInfo,
};
use crate::watch::matcher::{FilterEvaluator, matches_patterns};
use crate::watch::{WatchConfig, WatchRule};
use anyhow::{Context, Result};
use interprocess::local_socket::traits::tokio::Listener;
use interprocess::local_socket::{GenericFilePath, ListenerOptions, ToFsName};
use log::{debug, error, info, warn};
use notify::{EventKind, RecursiveMode};
use notify_debouncer_full::{DebouncedEvent, Debouncer, new_debouncer};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// Guard that removes the IPC socket file when dropped.
/// Ensures cleanup happens on normal exit, IPC shutdown, signal, or panic.
struct SocketGuard(PathBuf);

impl Drop for SocketGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Unique identifier for each IPC connection.
type ConnId = u64;

/// All event sources feed into this enum via a single mpsc channel.
enum DaemonEvent {
    /// Debounced filesystem events from notify-debouncer-full.
    FsEvents(Vec<DebouncedEvent>),
    /// A debouncer error (e.g. watch failure).
    FsError(Vec<notify::Error>),
    /// A framed message arrived from an IPC connection.
    ClientMsg { conn_id: ConnId, msg: ClientMessage },
    /// A connection was closed (EOF or error).
    ClientDisconnect { conn_id: ConnId },
    /// A new IPC connection was accepted.
    NewConnection {
        conn_id: ConnId,
        writer_tx: mpsc::Sender<ServerMessage>,
    },
}

/// Run the daemon event loop (blocking — starts its own async runtime).
///
/// # Errors
///
/// Returns an error if the async runtime or IPC listener cannot be created.
pub fn run(db: &Database) -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    let result = rt.block_on(async_run(db));
    if let Err(ref e) = result {
        error!("Daemon shutting down due to fatal error: {e:#}");
    } else {
        info!("Daemon stopped.");
    }
    result
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

#[allow(clippy::too_many_lines)]
async fn async_run(db: &Database) -> Result<()> {
    info!("Daemon started. PID: {}", std::process::id());

    // Bind IPC socket first so clients can detect us immediately.
    let socket_path = get_ipc_socket_path()
        .context("failed to resolve IPC socket path")?;
    #[cfg(unix)]
    if socket_path.exists() {
        std::fs::remove_file(&socket_path).ok();
    }
    let listener = ListenerOptions::new()
        .name(socket_path.clone().to_fs_name::<GenericFilePath>()?)
        .create_tokio()
        .context("failed to bind IPC socket — is another daemon already running?")?;

    let _socket_guard = SocketGuard(socket_path.clone());
    info!("IPC socket: {}", socket_path.display());

    // Central event channel — all sources (FS watcher, IPC connections) feed here.
    let (event_tx, mut event_rx) = mpsc::channel::<DaemonEvent>(512);

    // Setup the debounced file watcher, routing events into the central channel.
    let fs_tx = event_tx.clone();
    let mut debouncer = new_debouncer(
        Duration::from_millis(500),
        None,
        move |result: notify_debouncer_full::DebounceEventResult| {
            match result {
                Ok(events) => {
                    let _ = fs_tx.blocking_send(DaemonEvent::FsEvents(events));
                }
                Err(errors) => {
                    let _ = fs_tx.blocking_send(DaemonEvent::FsError(errors));
                }
            }
        },
    )
    .context("failed to initialize filesystem watcher")?;

    let config_path = WatchConfig::config_path().unwrap_or_else(|_| PathBuf::from("watch.toml"));
    if let Some(config_dir) = config_path.parent() {
        if !config_dir.exists() {
            std::fs::create_dir_all(config_dir).ok();
        }
        if let Err(e) = debouncer.watch(config_dir, RecursiveMode::NonRecursive) {
            warn!("Could not watch config directory {}: {e}", config_dir.display());
        }
    }

    let mut current_config = WatchConfig::load().unwrap_or_default();
    resolve_all_rules(&mut current_config.rules);
    let mut watched_roots: HashSet<PathBuf> = HashSet::new();
    let mut last_config_reload: Option<Instant> = None;
    let mut filter_evaluator = FilterEvaluator::new();
    info!("Loaded config: {} rules", current_config.rules.len());
    add_new_watch_roots(&mut debouncer, &current_config, &mut watched_roots);

    let store = DirectStore::new(db.clone());
    let initial_work = retroactive_scan(&current_config, &store, &mut filter_evaluator);
    spawn_tag_work(initial_work, db);

    info!("Daemon ready.");

    // Per-connection writers for sending responses and events back.
    let mut conn_writers: HashMap<ConnId, mpsc::Sender<ServerMessage>> = HashMap::new();
    // Connections that have subscribed to push events.
    let mut subscribers: HashSet<ConnId> = HashSet::new();
    let mut next_conn_id: ConnId = 0;

    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);

    let sigterm = sigterm_or_pending();
    tokio::pin!(sigterm);

    loop {
        tokio::select! {
            // Accept new IPC connections.
            Ok(conn) = listener.accept() => {
                let conn_id = next_conn_id;
                next_conn_id += 1;

                // Per-connection channel for outbound messages.
                let (writer_tx, writer_rx) = mpsc::channel::<ServerMessage>(64);

                // Notify central loop about the new connection.
                let _ = event_tx.send(DaemonEvent::NewConnection {
                    conn_id,
                    writer_tx: writer_tx.clone(),
                }).await;

                // Spawn the per-connection read/write tasks.
                let read_tx = event_tx.clone();
                tokio::spawn(connection_task(conn, conn_id, read_tx, writer_rx));
            }

            // Process all events from the central channel.
            Some(event) = event_rx.recv() => {
                match event {
                    DaemonEvent::FsEvents(debounced_events) => {
                        let work = handle_debounced_events(
                            &debounced_events,
                            &mut current_config,
                            &config_path,
                            &mut debouncer,
                            &mut watched_roots,
                            &mut last_config_reload,
                            db,
                            &mut filter_evaluator,
                        );
                        spawn_tag_work_with_events(work, db, &subscribers, &conn_writers);
                    }
                    DaemonEvent::FsError(errors) => {
                        for e in &errors {
                            warn!("Watcher error: {e}");
                        }
                    }
                    DaemonEvent::NewConnection { conn_id, writer_tx } => {
                        conn_writers.insert(conn_id, writer_tx);
                    }
                    DaemonEvent::ClientMsg { conn_id, msg } => {
                        let should_shutdown = handle_client_message(
                            conn_id, msg, db,
                            &conn_writers, &mut subscribers,
                        ).await;
                        if should_shutdown {
                            info!("Shutdown requested via IPC.");
                            break;
                        }
                    }
                    DaemonEvent::ClientDisconnect { conn_id } => {
                        conn_writers.remove(&conn_id);
                        subscribers.remove(&conn_id);
                    }
                }
            }

            _ = &mut ctrl_c => {
                info!("Received SIGINT. Stopping daemon.");
                break;
            }
            () = &mut sigterm => {
                info!("Received SIGTERM. Stopping daemon.");
                break;
            }
        }
    }

    Ok(())
}

/// Per-connection task: splits the stream into a reader and writer.
/// The reader decodes `ClientMessage` frames and forwards them to the central
/// event channel. The writer receives `ServerMessage`s from its own mpsc
/// channel and encodes them onto the wire.
async fn connection_task(
    conn: interprocess::local_socket::tokio::prelude::LocalSocketStream,
    conn_id: ConnId,
    event_tx: mpsc::Sender<DaemonEvent>,
    mut writer_rx: mpsc::Receiver<ServerMessage>,
) {
    let (reader, writer) = tokio::io::split(conn);

    // Writer task: drains the per-connection channel and sends frames.
    let write_handle = tokio::spawn(async move {
        let mut writer = writer;
        while let Some(msg) = writer_rx.recv().await {
            if wire::write_frame(&mut writer, &msg).await.is_err() {
                break;
            }
        }
    });

    // Reader loop: decode frames and forward to central event channel.
    let mut reader = reader;
    loop {
        let frame_result: std::result::Result<Option<ClientMessage>, _> =
            wire::read_frame(&mut reader).await;
        match frame_result {
            Ok(Some(msg)) => {
                if event_tx
                    .send(DaemonEvent::ClientMsg { conn_id, msg })
                    .await
                    .is_err()
                {
                    break;
                }
            }
            // EOF or frame error — client disconnected.
            Ok(None) | Err(_) => break,
        }
    }

    let _ = event_tx.send(DaemonEvent::ClientDisconnect { conn_id }).await;
    write_handle.abort();
}

/// Process a single client message: dispatch request or manage subscriptions.
/// Returns `true` if the daemon should shut down.
async fn handle_client_message(
    conn_id: ConnId,
    msg: ClientMessage,
    db: &Database,
    conn_writers: &HashMap<ConnId, mpsc::Sender<ServerMessage>>,
    subscribers: &mut HashSet<ConnId>,
) -> bool {
    match msg {
        ClientMessage::Request { id, payload } => {
            let is_tag_mutation = matches!(
                payload,
                Request::AddTags { .. } | Request::SetTags { .. } | Request::RemoveTags { .. }
            );
            let (response, should_shutdown, event) = execute_wire_request(payload, db);
            #[cfg(feature = "dynamic-completions")]
            let tag_mutation_ok = is_tag_mutation && matches!(response, Response::Ok);
            #[cfg(not(feature = "dynamic-completions"))]
            let _ = is_tag_mutation;
            if let Some(tx) = conn_writers.get(&conn_id) {
                let _ = tx.send(ServerMessage::Response { id, payload: response }).await;
            }
            if let Some(evt) = event {
                broadcast_event(&evt, subscribers, conn_writers);
            }
            #[cfg(feature = "dynamic-completions")]
            if tag_mutation_ok {
                let store = crate::store::DirectStore::new(db.clone());
                crate::completions::invalidate_cache(&store);
            }
            should_shutdown
        }
        ClientMessage::Subscribe => {
            subscribers.insert(conn_id);
            false
        }
        ClientMessage::Unsubscribe => {
            subscribers.remove(&conn_id);
            false
        }
    }
}

/// Broadcast a `ServerEvent` to all subscribed connections.
/// Silently drops events for connections whose channel is full or closed.
fn broadcast_event(
    event: &ServerEvent,
    subscribers: &HashSet<ConnId>,
    conn_writers: &HashMap<ConnId, mpsc::Sender<ServerMessage>>,
) {
    let msg = ServerMessage::Event(event.clone());
    for &conn_id in subscribers {
        if let Some(tx) = conn_writers.get(&conn_id) {
            // Non-blocking: if the channel is full we drop the event rather
            // than blocking the central loop.
            let _ = tx.try_send(msg.clone());
        }
    }
}

/// Like `spawn_tag_work` but also broadcasts `FileTagged` events to subscribers.
fn spawn_tag_work_with_events(
    work: Vec<(PathBuf, Vec<String>)>,
    db: &Database,
    subscribers: &HashSet<ConnId>,
    conn_writers: &HashMap<ConnId, mpsc::Sender<ServerMessage>>,
) {
    for (path, tags) in work {
        let db_ref = db.clone();
        let event = ServerEvent::FileTagged {
            file: path.to_string_lossy().into_owned(),
            tags: tags.clone(),
        };
        broadcast_event(&event, subscribers, conn_writers);
        #[cfg(feature = "dynamic-completions")]
        let db_for_cache = db.clone();
        tokio::spawn(async move {
            debug!("Auto-tagging {} with {tags:?}", path.display());
            let result = tokio::task::spawn_blocking(move || {
                let store = crate::store::DirectStore::new(db_ref);
                let mut stdout = std::io::stdout();
                crate::commands::tag::execute(
                    &store,
                    Some(path.clone()),
                    &tags,
                    false,
                    true,
                    &mut stdout,
                )
                .map_err(|e| Box::new((path, e)))
            })
            .await;
            match result {
                Ok(Err(boxed)) => error!("Auto-tag failed for {}: {}", boxed.0.display(), boxed.1),
                Err(e) => error!("Spawn error: {e}"),
                Ok(Ok(())) => {
                    #[cfg(feature = "dynamic-completions")]
                    {
                        let store = crate::store::DirectStore::new(db_for_cache);
                        crate::completions::invalidate_cache(&store);
                    }
                }
            }
        });
    }
}

/// Handle a batch of debounced filesystem events.
///
/// Returns a list of `(path, tags)` pairs that should be applied to the database.
/// The caller is responsible for spawning the actual DB writes off the event loop
/// so that IPC connections are never blocked.
#[allow(clippy::too_many_arguments)]
fn handle_debounced_events(
    events: &[DebouncedEvent],
    config: &mut WatchConfig,
    config_path: &Path,
    debouncer: &mut Debouncer<notify::RecommendedWatcher, notify_debouncer_full::NoCache>,
    watched_roots: &mut HashSet<PathBuf>,
    last_config_reload: &mut Option<Instant>,
    db: &Database,
    filter_evaluator: &mut FilterEvaluator,
) -> Vec<(PathBuf, Vec<String>)> {
    let mut work: Vec<(PathBuf, Vec<String>)> = Vec::new();
    let store = DirectStore::new(db.clone());

    for debounced in events {
        let event = &debounced.event;

        // If any path in the event IS the config file, reload config — with debounce.
        if event.paths.iter().any(|p| p == config_path) {
            let now = Instant::now();
            let too_soon = last_config_reload
                .is_some_and(|prev| now.duration_since(prev) < Duration::from_millis(500));
            if too_soon {
                continue;
            }
            *last_config_reload = Some(now);
            info!("watch.toml changed, reloading config...");
            match WatchConfig::load() {
                Ok(mut new_config) => {
                    resolve_all_rules(&mut new_config.rules);
                    *config = new_config;
                    info!("Config reloaded: {} rules", config.rules.len());
                    add_new_watch_roots(debouncer, config, watched_roots);
                    work.extend(retroactive_scan(config, &store, filter_evaluator));
                }
                Err(e) => error!("Failed to reload watch config: {e}"),
            }
            continue;
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
            continue;
        }

        for path in &event.paths {
            for rule in &config.rules {
                if rule.tags.is_empty() {
                    continue;
                }
                if !matches_patterns(path, &rule.patterns) {
                    continue;
                }
                if let Some(criteria) = &rule.filter_criteria
                    && !filter_evaluator.matches(path, criteria, &store)
                {
                    continue;
                }
                work.push((path.clone(), rule.tags.clone()));
            }
        }
    }
    work
}

/// Resolve each rule's named filter + inline vtags + `filter_by_tags` into
/// its `filter_criteria` field so that the hot path only does evaluation,
/// not I/O.
fn resolve_all_rules(rules: &mut [WatchRule]) {
    let filter_manager = get_filter_path()
        .ok()
        .map(FilterManager::new);

    for rule in rules.iter_mut() {
        let mut criteria = QueryCriteria::default();

        // Merge any saved/named filter first so inline flags can override.
        if let Some(name) = &rule.filter
            && let Some(ref fm) = filter_manager
        {
            match fm.get(name) {
                Ok(f) => criteria = f.criteria,
                Err(e) => warn!("Watch: could not load filter '{name}': {e}"),
            }
        }

        // Inline vtag conditions (AND with whatever the named filter required).
        criteria.virtual_tags.extend(rule.vtags.iter().cloned());

        // DB-tag gate: file must already carry all of these tags.
        if !rule.filter_by_tags.is_empty() {
            use crate::types::{TagExpr, TagName};
            let tag_exprs: Vec<TagExpr> = rule.filter_by_tags.iter()
                .filter_map(|t| TagName::new(t).ok().map(TagExpr::Tag))
                .collect();

            if !tag_exprs.is_empty() {
                let new_expr = if tag_exprs.len() == 1 {
                    tag_exprs.into_iter().next().unwrap_or_else(|| unreachable!())
                } else {
                    TagExpr::And(tag_exprs)
                };
                criteria.tag_expr = Some(match criteria.tag_expr.take() {
                    Some(existing) => TagExpr::And(vec![existing, new_expr]),
                    None => new_expr,
                });
            }
        }

        rule.filter_criteria = if criteria.is_empty() { None } else { Some(criteria) };
    }
}

/// Register any watch roots from the config that are not yet watched.
/// We never remove roots to avoid races; a daemon restart cleans up.
fn add_new_watch_roots(
    debouncer: &mut Debouncer<notify::RecommendedWatcher, notify_debouncer_full::NoCache>,
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
                warn!(
                    "Watch root {} does not exist yet (will not be watched until it is created)",
                    root.display()
                );
                continue;
            }
            info!("Watching directory: {}", root.display());
            match debouncer.watch(&root, RecursiveMode::Recursive) {
                Ok(()) => {
                    watched.insert(root);
                }
                Err(e) => error!("Failed to watch {}: {e}", root.display()),
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
            .map_or_else(|| pattern.to_string(), |h| pattern.replacen('~', h.to_string_lossy().as_ref(), 1))
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
    store: &dyn crate::store::TagStore,
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
                    .filter_map(Result::ok)
                    .filter(|p| p.is_file())
                    .collect()
            } else {
                // Literal path — treat it as a single file.
                let p = PathBuf::from(&expanded);
                if p.is_file() { vec![p] } else { vec![] }
            };

            for path in &paths {
                // Skip if the file already carries all the rule's tags.
                if let Ok(tagr_path) = crate::types::TagrPath::new(path)
                    && let Ok(Some(existing)) = store.get_tags(&tagr_path)
                    && rule.tags.iter().all(|t| {
                        existing.iter().any(|e| e.as_str() == t)
                    })
                {
                    continue;
                }

                // Honour filter criteria if any.
                if let Some(criteria) = &rule.filter_criteria
                    && !filter_evaluator.matches(path, criteria, store)
                {
                    continue;
                }

                work.push((path.clone(), rule.tags.clone()));
            }
        }
    }

    if !work.is_empty() {
        info!("Retroactive scan: {} files to tag", work.len());
    }

    work
}

/// Spawn tag operations as blocking tasks so the event loop stays responsive.
fn spawn_tag_work(work: Vec<(PathBuf, Vec<String>)>, db: &Database) {
    for (path, tags) in work {
        let db_ref = db.clone();
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                let store = crate::store::DirectStore::new(db_ref);
                let mut stdout = std::io::stdout();
                crate::commands::tag::execute(
                    &store,
                    Some(path.clone()),
                    &tags,
                    false,
                    true,
                    &mut stdout,
                )
                .map_err(|e| Box::new((path, e)))
            })
            .await;
            match result {
                Ok(Err(boxed)) => error!("Retroactive tag failed for {}: {}", boxed.0.display(), boxed.1),
                Err(e) => error!("Spawn error: {e}"),
                Ok(Ok(())) => {}
            }
        });
    }
}

/// Expand a leading `~/` to the user's home directory.
fn expand_tilde(pattern: &str) -> String {
    if pattern.starts_with("~/") {
        dirs::home_dir()
            .map_or_else(|| pattern.to_string(), |h| pattern.replacen('~', h.to_string_lossy().as_ref(), 1))
    } else {
        pattern.to_string()
    }
}

// ---------------------------------------------------------------------------
// IPC — Wire protocol request dispatch
// ---------------------------------------------------------------------------

/// Execute a wire protocol request and return the response.
/// Returns `(Response, should_shutdown, optional_event_to_broadcast)`.
#[allow(clippy::too_many_lines)]
fn execute_wire_request(req: Request, db: &Database) -> (Response, bool, Option<ServerEvent>) {
    match req {
        Request::Ping => (Response::Pong, false, None),
        Request::Shutdown => (Response::Ok, true, None),

        Request::ListTags => {
            match db.list_all_tags() {
                Ok(tag_names) => {
                    let tags: Vec<_> = tag_names
                        .into_iter()
                        .map(|name| {
                            let file_count = db.find_by_tag(&name).map_or(0, |f| f.len()) as u64;
                            WireTagInfo { name, file_count }
                        })
                        .collect();
                    (Response::Tags(tags), false, None)
                }
                Err(e) => (Response::Error(e.to_string()), false, None),
            }
        }

        Request::ListFiles => {
            match db.list_all() {
                Ok(pairs) => {
                    let wire_pairs = pairs.into_iter().map(WireFilePair::from).collect();
                    (Response::Files(wire_pairs), false, None)
                }
                Err(e) => (Response::Error(e.to_string()), false, None),
            }
        }

        Request::Query { criteria } => {
            use crate::store::TagStore as _;
            match crate::types::QueryCriteria::try_from(&criteria) {
                Ok(qc) => {
                    let store = DirectStore::new(db.clone());
                    let schema = crate::schema::load_default_schema()
                        .ok()
                        .unwrap_or_default();
                    match store.query(&qc, &schema) {
                        Ok(paths) => {
                            let wire_pairs: Vec<WireFilePair> = paths
                                .into_iter()
                                .filter_map(|p| {
                                    let tags = store.get_tags(&p).ok()?.unwrap_or_default();
                                    Some(WireFilePair {
                                        file: p.to_string(),
                                        tags: tags.into_iter().map(|t| t.to_string()).collect(),
                                    })
                                })
                                .collect();
                            (Response::Files(wire_pairs), false, None)
                        }
                        Err(e) => (Response::Error(e.to_string()), false, None),
                    }
                }
                Err(e) => (
                    Response::Error(format!("Invalid query criteria: {e}")),
                    false,
                    None,
                ),
            }
        }

        Request::GetTags { file } => {
            let path = PathBuf::from(&file);
            match db.get_tags(&path) {
                Ok(Some(tags)) => (Response::FileTags(tags), false, None),
                Ok(None) => (Response::FileTags(vec![]), false, None),
                Err(e) => (Response::Error(e.to_string()), false, None),
            }
        }

        Request::GetNote { file } => {
            let path = PathBuf::from(&file);
            match db.get_note(&path) {
                Ok(note) => {
                    let wire = note.map(|n| WireNoteEntry {
                        path: file,
                        content: n.content,
                        created_at: n.metadata.created_at,
                        updated_at: n.metadata.updated_at,
                    });
                    (Response::Note(wire), false, None)
                }
                Err(e) => (Response::Error(e.to_string()), false, None),
            }
        }

        Request::FindByTag { tag } => {
            match db.find_by_tag(&tag) {
                Ok(paths) => {
                    let string_paths = paths.iter().map(|p| p.to_string_lossy().into_owned()).collect();
                    (Response::FilePaths(string_paths), false, None)
                }
                Err(e) => (Response::Error(e.to_string()), false, None),
            }
        }

        Request::FindByTags { tags, match_all } => {
            let result = if match_all {
                db.find_by_all_tags(&tags)
            } else {
                db.find_by_any_tag(&tags)
            };
            match result {
                Ok(paths) => {
                    let string_paths = paths.iter().map(|p| p.to_string_lossy().into_owned()).collect();
                    (Response::FilePaths(string_paths), false, None)
                }
                Err(e) => (Response::Error(e.to_string()), false, None),
            }
        }

        Request::FindByTagRegex { pattern } => {
            match db.find_by_tag_regex(&pattern) {
                Ok(paths) => {
                    let string_paths = paths.iter().map(|p| p.to_string_lossy().into_owned()).collect();
                    (Response::FilePaths(string_paths), false, None)
                }
                Err(e) => (Response::Error(e.to_string()), false, None),
            }
        }

        Request::ListAllPaths => {
            match db.list_all_files() {
                Ok(paths) => {
                    let string_paths = paths.iter().map(|p| p.to_string_lossy().into_owned()).collect();
                    (Response::FilePaths(string_paths), false, None)
                }
                Err(e) => (Response::Error(e.to_string()), false, None),
            }
        }

        Request::ListNotes => {
            match db.list_all_notes() {
                Ok(notes) => {
                    let entries = notes
                        .into_iter()
                        .map(|(path, note)| WireNoteEntry {
                            path: path.to_string_lossy().into_owned(),
                            content: note.content,
                            created_at: note.metadata.created_at,
                            updated_at: note.metadata.updated_at,
                        })
                        .collect();
                    (Response::Notes(entries), false, None)
                }
                Err(e) => (Response::Error(e.to_string()), false, None),
            }
        }

        Request::AddTags { file, tags } => {
            let path = PathBuf::from(&file);
            let store = crate::store::DirectStore::new(db.clone());
            let mut stdout = std::io::stdout();
            match crate::commands::tag::execute(&store, Some(path), &tags, false, true, &mut stdout) {
                Ok(()) => (Response::Ok, false, None),
                Err(e) => (Response::Error(e.to_string()), false, None),
            }
        }

        Request::SetTags { file, tags } => {
            let path = PathBuf::from(&file);
            match db.insert(&path, tags) {
                Ok(()) => (Response::Ok, false, None),
                Err(e) => (Response::Error(e.to_string()), false, None),
            }
        }

        Request::RemoveTags { file, tags, all } => {
            let path = PathBuf::from(&file);
            let store = crate::store::DirectStore::new(db.clone());
            let mut stdout = std::io::stdout();
            match crate::commands::tag::untag(&store, Some(path), &tags, all, true, &mut stdout) {
                Ok(()) => (Response::Ok, false, None),
                Err(e) => (Response::Error(e.to_string()), false, None),
            }
        }

        Request::SetNote { file, content } => {
            let path = PathBuf::from(&file);
            let note = crate::types::NoteRecord::new(content.clone());
            match db.set_note(&path, &note) {
                Ok(()) => {
                    let event = ServerEvent::NoteChanged { file, content: Some(content) };
                    (Response::Ok, false, Some(event))
                }
                Err(e) => (Response::Error(e.to_string()), false, None),
            }
        }

        Request::DeleteNote { file } => {
            let path = PathBuf::from(&file);
            match db.delete_note(&path) {
                Ok(_) => {
                    let event = ServerEvent::NoteChanged { file, content: None };
                    (Response::Ok, false, Some(event))
                }
                Err(e) => (Response::Error(e.to_string()), false, None),
            }
        }

        Request::DeleteFromDb { file } => {
            let path = PathBuf::from(&file);
            match db.remove(&path) {
                Ok(true) => (Response::Ok, false, None),
                Ok(false) => (Response::Error(format!("File not found in database: {file}")), false, None),
                Err(e) => (Response::Error(e.to_string()), false, None),
            }
        }

        Request::Cleanup => {
            let mut removed = 0usize;
            match db.list_all() {
                Ok(pairs) => {
                    for pair in &pairs {
                        let path = std::path::Path::new(pair.file.as_str());
                        if !path.exists() && db.remove(path).is_ok() {
                            removed += 1;
                        }
                    }
                    (Response::CleanupResult { removed: removed as u64 }, false, None)
                }
                Err(e) => (Response::Error(e.to_string()), false, None),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    // ---- execute_wire_request tests ----

    #[test]
    fn test_execute_wire_ping() {
        let test_db = TestDb::new("wire_ping");
        let (resp, shutdown, _) = execute_wire_request(Request::Ping, test_db.db());
        assert!(!shutdown);
        assert!(matches!(resp, Response::Pong));
    }

    #[test]
    fn test_execute_wire_shutdown() {
        let test_db = TestDb::new("wire_shutdown");
        let (resp, shutdown, _) = execute_wire_request(Request::Shutdown, test_db.db());
        assert!(shutdown);
        assert!(matches!(resp, Response::Ok));
    }

    #[test]
    fn test_execute_wire_list_tags() {
        let test_db = TestDb::new("wire_list_tags");
        let db = test_db.db();

        let temp = crate::testing::TempFile::create("wire_tags.txt").unwrap();
        db.insert(temp.path(), vec!["alpha".into(), "beta".into()]).unwrap();

        let (resp, _, _) = execute_wire_request(Request::ListTags, db);
        match resp {
            Response::Tags(tags) => {
                let names: Vec<_> = tags.iter().map(|t| t.name.as_str()).collect();
                assert!(names.contains(&"alpha"));
                assert!(names.contains(&"beta"));
            }
            _ => panic!("Expected Tags response, got: {resp:?}"),
        }
    }

    #[test]
    fn test_execute_wire_list_files() {
        let test_db = TestDb::new("wire_list_files");
        let db = test_db.db();

        let temp = crate::testing::TempFile::create("wire_files.txt").unwrap();
        db.insert(temp.path(), vec!["tag1".into()]).unwrap();

        let (resp, _, _) = execute_wire_request(Request::ListFiles, db);
        match resp {
            Response::Files(files) => {
                assert_eq!(files.len(), 1);
                assert!(files[0].tags.contains(&"tag1".to_string()));
            }
            _ => panic!("Expected Files response, got: {resp:?}"),
        }
    }

    #[test]
    fn test_execute_wire_get_tags() {
        let test_db = TestDb::new("wire_get_tags");
        let db = test_db.db();

        let temp = crate::testing::TempFile::create("wire_gettags.txt").unwrap();
        db.insert(temp.path(), vec!["x".into(), "y".into()]).unwrap();

        let (resp, _, _) = execute_wire_request(
            Request::GetTags { file: temp.path().to_string_lossy().into_owned() },
            db,
        );
        match resp {
            Response::FileTags(tags) => {
                assert!(tags.contains(&"x".to_string()));
                assert!(tags.contains(&"y".to_string()));
            }
            _ => panic!("Expected FileTags response, got: {resp:?}"),
        }
    }

    #[test]
    fn test_execute_wire_get_tags_missing_file() {
        let test_db = TestDb::new("wire_get_tags_missing");
        let (resp, _, _) = execute_wire_request(
            Request::GetTags { file: "/nonexistent/file.txt".into() },
            test_db.db(),
        );
        match resp {
            Response::FileTags(tags) => assert!(tags.is_empty()),
            _ => panic!("Expected empty FileTags, got: {resp:?}"),
        }
    }
}
