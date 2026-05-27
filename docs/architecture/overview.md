# Architecture Overview

> **Status**: Design document — describes the current state and target architecture.
> No refactoring has been done yet.

## Current Architecture

Tagr is a ~39k LOC Rust project organized into these top-level modules:

```
src/
├── types/          (planned — core newtypes and shared vocabulary)
├── db/             storage layer (sled, postcard serialization, reverse index)
├── datasource.rs   unified data access (enum: Direct | Remote)
├── search/         filtering logic (hierarchy, patterns, traits)
├── filters/        saved filter CRUD + FilterCriteria type
├── query/          (planned — single query pipeline)
├── store/          (planned — TagStore trait + impls)
├── browse/         browse session domain logic (models, query, actions, filter)
├── ui/             TUI abstraction (traits, types) + ratatui adapter (8k LOC)
├── commands/       CLI command implementations
├── daemon/         watch daemon (tokio, IPC server, file watcher)
├── ipc/            wire protocol types (wincode serialization)
├── cli.rs          clap structs + SearchParams (1.7k LOC)
├── main.rs         entry point + dispatch
├── vtags/          virtual tags (parser, evaluator, cache)
├── schema/         tag schema (aliases, hierarchy config)
├── config/         configuration management
├── keybinds/       keybind config + action executor
├── completions/    shell completion system
├── patterns/       pattern builder (tag/file pattern validation)
├── preview/        file preview generation
├── watch/          watch rule config types
├── discovery/      file discovery traits
├── output/         output formatting (StatusBar, StdoutWriter)
└── lib.rs          Pair struct, TagrError, re-exports
```

## Two Backend Modes

### Direct Mode
CLI or TUI opens the sled database directly. Single-process, single-user.

### Daemon Mode (watch)
The daemon process owns the sled database lock. CLI and TUI communicate
via Unix socket IPC (binary protocol over `interprocess` + `tokio`).

Currently abstracted by `DataSource` (an enum, not a trait), which dispatches
method calls to either `Database` directly or `PersistentClient` over IPC.

## Two Frontend Modes

### CLI (`tagr search`, `tagr tag`, etc.)
Power-user/automation interface. Pipe-friendly output, exit codes, JSON format.
All functionality must work headlessly.

### TUI (`tagr browse`)
Discovery/learning interface. Interactive tag tree, file preview, note viewer.
Shows CLI equivalents to guide users toward automation.

## Layer Diagram (Target)

```
┌─────────────────────────────────────────────────────────────────┐
│ Layer 0: Core Types  (tagr::types)                              │
│  TagName, TagrPath, FilterName, NoteRecord, MatchMode, TagExpr, │
│  QueryCriteria, Pair, ValidationError                           │
│  Zero dependencies on anything else in tagr                     │
└──────────────────────────┬──────────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────────┐
│ Layer 1: Storage + Config Interfaces                             │
│                                                                  │
│  store/mod.rs    trait TagStore: Send + Sync                     │
│  filters/        SavedFilter, FilterManager (TOML I/O)           │
│  schema/         TagSchema (TOML I/O, aliases, hierarchy config) │
│                                                                  │
│  Pure interfaces + config persistence — all use Layer 0 types    │
└──────────────┬───────────────────────────────┬──────────────────┘
               │                               │
┌──────────────▼──────────┐  ┌─────────────────▼─────────────────┐
│ Layer 2a: DirectStore   │  │ Layer 2b: DaemonStore              │
│ Wraps sled Database     │  │ Wraps IPC PersistentClient         │
│ Sync, owns DB handle    │  │ Sync facade over async IPC         │
│                         │  │ Widest-result cache for TUI:       │
│                         │  │  - Narrow = filter cache locally   │
│                         │  │  - Widen beyond cache = IPC query  │
│                         │  │  - ServerEvent diffs update cache  │
│                         │  │  - ConfigReloaded = full invalidate│
└─────────────────────────┘  └───────────────────────────────────┘
               │                               │
┌──────────────▼───────────────────────────────▼──────────────────┐
│ Layer 3: Query Engine  (tagr::query)                             │
│  fn execute(store, criteria, schema) -> Vec<TagrPath>            │
│  Tag expansion, hierarchy filtering, pattern matching, vtags     │
│  THE single pipeline — both CLI and TUI call this                │
└──────────────────────────┬──────────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────────┐
│ Layer 4: Browse Domain  (tagr::browse)                           │
│  BrowseSession — owns Arc<dyn TagStore>, Arc<TagSchema>,         │
│  QueryCriteria (source of truth). Handles actions, domain logic. │
│  No rendering. Accepts initial QueryCriteria from CLI args.      │
│  Exposes compound methods (toggle_and_refresh, set_filter, etc.) │
│  to avoid callers holding refs across mutations (borrow checker). │
└──────────────────────────┬──────────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────────┐
│ Layer 5: Frontends                                               │
│                                                                  │
│  commands/    CLI — parse args → QueryCriteria → store.query()   │
│               Formats output (text/json/quiet). Pipe-friendly.   │
│               After tag mutations: rebuild completion cache.     │
│                                                                  │
│  completions/ Shell completion provider. Reads ONLY from its own │
│               cache file (~/.cache/tagr/completions.cache).      │
│               Never queries TagStore or DB directly.             │
│               Cache rebuilt by CLI/daemon after tag mutations.   │
│                                                                  │
│  ui/          TUI — AppState owns widget state (scroll, focus,   │
│               pane layout). Derives display from BrowseSession.  │
│               Converts TagrPath → DisplayItem at this boundary.  │
│                                                                  │
│  keybinds/    Layer 5 UI concern. Maps keybinds → BrowseAction   │
│               → dispatches to BrowseSession API (domain ops) or  │
│               AppState (navigation/modal). No domain logic here. │
│                                                                  │
│  daemon/      Owns DirectStore, serves IPC, runs watch rules.    │
│               Never imports ui/. Sends domain types over wire.   │
│               After tag mutations: rebuild completion cache.     │
└─────────────────────────────────────────────────────────────────┘
```

### Key Rules

1. **Lower layers NEVER import higher layers** — no `db` importing `cli`
2. **Convert at boundaries only** — daemon sends `Pair`, TUI converts to `DisplayItem`
3. **One query pipeline** — both CLI and TUI call `query::execute()`
4. **Newtypes enforce boundaries** — can't pass `FilterName` where `TagName` expected
5. **`TagStore::query()` is a required method, not a default** — each impl provides its own body.
   `DirectStore::query()` calls `query::execute()` internally. `DaemonStore::query()` does IPC.
   The `store/` module never imports `query/`, keeping the dependency direction clean.

## Async Architecture

| Component | Sync/Async | Rationale |
|-----------|-----------|-----------|
| DirectStore methods | Sync | sled is sync |
| DaemonStore methods | Sync facade | `block_on()` internally; callers don't need async |
| Daemon server loop | Async (tokio) | Multiple connections, file watcher, timers |
| IPC read/write | Async (tokio) | Non-blocking I/O over Unix socket |
| TUI render loop | Sync | ratatui is sync; polls events via `try_recv()` |
| Watch file events | Async (tokio) | notify crate async watcher |

**Why sync trait, not async?**
- The CLI is inherently sync
- The TUI render loop is sync (ratatui)
- DaemonStore handles async internally — it's an implementation detail
- Avoids forcing async through the entire call stack
- No `Arc<Mutex<...>>` needed: TUI state is single-threaded, events flow through channels

## Daemon Internal Architecture

The daemon is the only multi-threaded, async component. It owns the database exclusively
(sled lock), accepts IPC connections, watches the filesystem, and broadcasts events.

### Event-driven single-loop design

All event sources feed into one `mpsc` channel, consumed by a single `tokio::select!` loop.
**The loop owns all mutable state** — no locks needed on the daemon side.

```
                                         ┌──────────────────────────┐
                                         │      Central Loop        │
                                         │  (owns all mutable state)│
                                         │                          │
  ┌─────────────────┐                    │  • DirectStore (DB)      │
  │  File Watcher   │──FsEvents─────────▶│  • WatchConfig           │
  │  (notify crate) │                    │  • conn_writers: HashMap │
  └─────────────────┘                    │  • subscribers: HashSet  │
                                         │  • TagSchema (ArcSwap)   │
  ┌─────────────────┐                    │                          │
  │  IPC Listener   │──NewConnection───▶│                          │
  │  (accept loop)  │                    │                          │
  └─────────────────┘                    └────────┬────────┬────────┘
                                                  │        │
  ┌─────────────────┐                             │        │
  │ Connection Task │──ClientMsg────────▶(merged)  │        │
  │ (per-client     │──ClientDisconnect─▶          │        │
  │  reader/writer) │◀──ServerMessage──────────────┘        │
  └─────────────────┘                                       │
                                                            │
  ┌─────────────────┐                                       │
  │  spawn_blocking │◀──DB writes───────────────────────────┘
  │  (tokio pool)   │   (tag mutations, queries)
  └─────────────────┘
```

### Ownership model (borrow-checker-friendly)

The central loop owns everything directly — no `Arc<Mutex<...>>` needed:

```rust
// All mutable state lives in local variables of the loop function.
// No sharing, no locks, no contention.
let store = DirectStore::new(db);           // DB access
let mut config = WatchConfig::load()?;      // watch rules
let schema = ArcSwap::new(TagSchema::load()?);  // shared with spawn_blocking
let mut conn_writers: HashMap<ConnId, Sender<ServerMessage>> = HashMap::new();
let mut subscribers: HashSet<ConnId> = HashSet::new();

loop {
    tokio::select! {
        // All branches have exclusive access to the above variables.
        // No partial borrow issues — select! branches are mutually exclusive.
    }
}
```

**Why this works in Rust:** `tokio::select!` branches are **mutually exclusive** — only one
runs at a time. The compiler sees the loop body as a single scope with sequential access.
No partial borrow conflicts, no `RefCell`, no `Mutex`.

### Task categories

The daemon has three categories of work:

**1. Messaging (IPC — async, per-connection)**
- Per-connection `connection_task`: reader decodes frames → `DaemonEvent::ClientMsg`,
  writer drains channel → encodes frames onto socket
- Stateless — no shared state, just routing bytes
- Spawned via `tokio::spawn`, communicates only through channels

**2. Request handling (sync, via spawn_blocking)**
- Client queries/mutations dispatched by `execute_wire_request()`
- All DB operations go through `DirectStore` (sync sled calls)
- Heavy queries use `spawn_blocking` so the event loop stays responsive
- Returns `(Response, Option<ServerEvent>)` — response to caller, optional broadcast

**3. Watch workers (filesystem → tag mutations)**
- File watcher callback → `DaemonEvent::FsEvents` → central loop
- `handle_debounced_events()` matches paths against watch rules (pure, no I/O)
- Produces `Vec<(TagrPath, Vec<TagName>)>` work items
- Work items dispatched via `spawn_blocking` with `ServerEvent` broadcast

### Request → Response flow

```
Client                    Connection Task           Central Loop              spawn_blocking
  │                            │                         │                         │
  │─── ClientMessage ─────────▶│                         │                         │
  │                            │─── DaemonEvent ────────▶│                         │
  │                            │    ::ClientMsg          │                         │
  │                            │                         │── execute_wire_request ─▶│
  │                            │                         │   (DB query/mutation)    │
  │                            │                         │◀── (Response, Event?) ───│
  │                            │◀── ServerMessage ───────│                         │
  │◀── Response frame ────────│                         │                         │
  │                            │                         │                         │
  │                            │                         │─── broadcast_event ─────▶ subscribers
```

### Schema in the daemon

The daemon uses `ArcSwap<TagSchema>` for lock-free reads:

```rust
// Central loop loads schema
let schema = Arc::new(ArcSwap::new(Arc::new(TagSchema::load()?)));

// spawn_blocking reads it (lock-free, just an atomic load)
let schema_ref = schema.load();
query::execute(&store, &criteria, &schema_ref)?;

// On alias mutation or ConfigReloaded: swap atomically
let new_schema = Arc::new(TagSchema::load()?);
schema.store(new_schema);
broadcast_event(ServerEvent::ConfigReloaded, ...);
```

No `Mutex` for schema reads. `ArcSwap::load()` is wait-free. Writers (rare) do an
atomic swap — readers never block, never see partial state.

### Event broadcasting

Events flow outward from mutations:

```rust
// 1. Mutation happens (in spawn_blocking or central loop)
db.add_tags(&file, tags)?;

// 2. Central loop broadcasts to subscribers
let event = ServerEvent::FileTagged { file, tags };
for &conn_id in &subscribers {
    if let Some(tx) = conn_writers.get(&conn_id) {
        let _ = tx.try_send(ServerMessage::Event(event.clone()));
        // Non-blocking: drop event if channel full (backpressure)
    }
}
```

**Backpressure:** `try_send` (not `send`) — if a subscriber's channel is full, the event
is dropped rather than blocking the central loop. Slow clients lose events gracefully.

### Bulk operations

Bulk mutations produce aggregate events to avoid flooding subscribers:

| Operation | Event | Rationale |
|-----------|-------|-----------|
| Single tag/untag | `FileTagged` / `FileUntagged` | Fine-grained, instant |
| `rename_tag` | `TagRenamed { old_prefix, new_prefix }` | Semantic — client can update locally |
| `merge_tags` | `TagsMerged { sources, target }` | Semantic — client knows what happened |
| `bulk_tag`, `bulk_untag`, `propagate_*`, `transform_*` | `BulkMutation { added, removed, deleted_files }` | Generic catch-all with diffs |
| watch.toml changed | `ConfigReloaded` | Full invalidation signal |

### What changes from current code

| Current | Target | Why |
|---------|--------|-----|
| `&Database` passed directly | `DirectStore` (impl TagStore) | Trait abstraction |
| `execute_wire_request` returns `(Response, bool, Option<ServerEvent>)` | Same pattern, but uses `QueryCriteria` for search | `WireSearchParams` → `QueryCriteria` over wire |
| `execute_search` duplicates query pipeline | `store.query(&criteria, &schema)` | Single pipeline |
| `load_default_schema()` per search request | `ArcSwap<TagSchema>` loaded once | No per-query I/O |
| `crate::commands::tag::execute()` for mutations | `store.add_tags()` / `store.insert()` directly | No CLI coupling in daemon |
| No bulk events | `TagRenamed`, `TagsMerged`, `BulkMutation` | Efficient client updates |
| `spawn_tag_work` clones `db` per item | Batch into fewer `spawn_blocking` calls | Less overhead |

## Wire Protocol (Target)

Clean break from current protocol — no backwards compatibility. The old `Request`/`Response`
enums are replaced entirely. All types use newtypes (`TagName`, `TagrPath`) which serialize
as their inner `String` via serde.

### Framing

Unchanged: `u32 LE length` + payload. Serialization migrates from wincode to postcard
(aligned with DB serialization migration).

### Client → Daemon

```rust
enum ClientMessage {
    Request { id: u32, payload: Request },
    Subscribe,
    Unsubscribe,
}

enum Request {
    // -- Admin --
    Ping,
    Shutdown,

    // -- Queries --
    /// The single query endpoint. Replaces FindByTag, FindByTags,
    /// FindByTagRegex, SearchFiles, ListFiles, ListAllPaths.
    /// Empty QueryCriteria = list all.
    Query { criteria: QueryCriteria },
    /// Tag names with file counts (for tag tree, autocomplete)
    ListTags,
    /// Tags for a specific file
    GetTags { file: TagrPath },
    /// Prefix search — sled scan_prefix on daemon side
    /// Powers autocomplete, hierarchy expansion, tag tree children
    FindTagsByPrefix { prefix: TagName },
    /// Notes
    GetNote { file: TagrPath },
    ListNotes,

    // -- Single mutations --
    AddTags { file: TagrPath, tags: Vec<TagName> },
    SetTags { file: TagrPath, tags: Vec<TagName> },
    RemoveTags { file: TagrPath, tags: Vec<TagName> },
    RemoveFile { file: TagrPath },
    SetNote { file: TagrPath, content: String },
    DeleteNote { file: TagrPath },

    // -- Bulk mutations --
    BulkTag { files: Vec<TagrPath>, tags: Vec<TagName> },
    BulkUntag { files: Vec<TagrPath>, tags: Vec<TagName> },
    RenameTag { old: TagName, new: TagName },
    MergeTags { sources: Vec<TagName>, target: TagName },
    Cleanup,
}
```

### Daemon → Client

```rust
enum ServerMessage {
    Response { id: u32, payload: Response },
    Event(ServerEvent),
}

enum Response {
    Pong,
    Ok,
    Error(String),
    /// Query results — Vec<Pair> not Vec<TagrPath>, so client has tags for local filtering
    QueryResult(Vec<Pair>),
    TagList(Vec<TagInfo>),
    FileTags(Vec<TagName>),
    /// Prefix search results (tag names only, no counts)
    TagPrefixResult(Vec<TagName>),
    Note(Option<NoteRecord>),
    NoteList(Vec<(TagrPath, NoteRecord)>),
    CleanupResult { removed: u64 },
}

struct TagInfo {
    name: TagName,
    file_count: u64,
}

enum ServerEvent {
    // -- Fine-grained --
    FileTagged { file: TagrPath, tags: Vec<TagName> },
    FileUntagged { file: TagrPath, tags: Vec<TagName> },
    FileRemoved { file: TagrPath },
    NoteChanged { file: TagrPath, content: Option<String> },

    // -- Bulk (semantic) --
    TagRenamed { old_prefix: TagName, new_prefix: TagName },
    TagsMerged { sources: Vec<TagName>, target: TagName },

    // -- Bulk (generic) --
    BulkMutation {
        added: Vec<(TagrPath, Vec<TagName>)>,
        removed: Vec<(TagrPath, Vec<TagName>)>,
        deleted_files: Vec<TagrPath>,
    },

    // -- Config --
    ConfigReloaded,
}
```

### Design notes

1. **`Query` returns `Vec<Pair>`, not `Vec<TagrPath>`** — DaemonStore needs tags for local
   cache filtering. Returning pairs avoids a second round-trip for tag data.
2. **Newtypes serialize as String** — `TagName` and `TagrPath` derive `Serialize/Deserialize`,
   postcard treats them identically to `String`. No wire format overhead.
3. **No filter/schema CRUD over IPC** — filters and schema are TOML files on disk.
   CLI reads/writes them directly. Daemon watches for changes and broadcasts `ConfigReloaded`.
4. **`Subscribe`/`Unsubscribe` remain at `ClientMessage` level** — simple, no filtered
   subscriptions initially. Future: `Subscribe { filter: EventFilter }` for selective events.

## Configuration Architecture

### File layout

```
~/.config/tagr/
├── config.toml         # Preference — [ui], [preview], [notes], [databases]
├── keybinds.toml       # Preference — key mappings (verbose, own file)
├── tag_schema.toml     # Additive — aliases, hierarchy config
├── watch.toml          # Additive — watch rules
└── filters.toml        # Additive — saved filters

~/.cache/tagr/
└── completions.cache   # Machine-generated, not user-edited

~/.local/share/tagr/
└── <db_name>/          # sled database files
```

### Two config categories

**Preference configs** (`config.toml`, `keybinds.toml`):
- Default + override pattern — every field has a sensible default
- Missing file → full defaults. Partial file → specified fields override defaults.
- `#[serde(default)]` on every field, `Default` impl with sensible values
- `tagr setup` generates `config.toml` with **all options commented out** as documentation:
  ```toml
  # Path display format: "absolute" or "relative"
  # path_format = "relative"

  # [ui]
  # backend = "custom"

  # [preview]
  # enabled = true
  # max_file_size = 1048576
  # syntax_highlighting = true
  ```
- User uncomments and changes only what they care about

**Additive configs** (`tag_schema.toml`, `watch.toml`, `filters.toml`):
- Empty by default — no aliases, no rules, no filters
- Entries added/removed via CLI commands (`tagr alias`, `tagr watch add`, `tagr filter save`)
- Missing file = empty state (correct, not an error)
- No "defaults to override" — these are growing collections
- File created on first write, not on setup

### Unified path resolution

Replace 5 separate `config_dir()` / `config_path()` implementations with one struct:

```rust
/// Single source of truth for all tagr filesystem paths.
/// Constructed once in main(), passed down via function parameters.
pub struct ConfigPaths {
    pub config: PathBuf,       // ~/.config/tagr/config.toml
    pub keybinds: PathBuf,     // ~/.config/tagr/keybinds.toml
    pub schema: PathBuf,       // ~/.config/tagr/tag_schema.toml
    pub watch: PathBuf,        // ~/.config/tagr/watch.toml
    pub filters: PathBuf,      // ~/.config/tagr/filters.toml
    pub cache_dir: PathBuf,    // ~/.cache/tagr/
    pub data_dir: PathBuf,     // ~/.local/share/tagr/
}

impl ConfigPaths {
    /// Platform-specific defaults via dirs crate
    pub fn default() -> Result<Self, ConfigError>;
    /// Override base directory (for testing, portable installs)
    pub fn with_base(base: PathBuf) -> Self;
}
```

Lives in Layer 0 (`types/`) — no dependencies, used everywhere.

### Loading pattern

**Preference configs**: `config` crate — supports layered sources (defaults → file → env vars).
Enables `TAGR_QUIET=1`, `TAGR_PREVIEW_ENABLED=false` etc. without custom parsing.

**Additive configs**: plain `toml::from_str()` + `serde` — no layering needed, just collections.

### Daemon watches

Daemon watches `~/.config/tagr/` directory:
- `tag_schema.toml` changed → reload schema via `ArcSwap`, broadcast `ConfigReloaded`
- `watch.toml` changed → reload rules, add new watch roots, retroactive scan
- `filters.toml` / `config.toml` / `keybinds.toml` → no daemon action

### Type deduplication

- `PathFormat` (3 definitions: config, cli, browse) → `types::PathFormat`
- `PreviewConfig` (2 definitions: config, ui::traits) → `config::PreviewConfig`

## Schema Lifecycle

`TagSchema` (aliases, hierarchy overrides) is **read-heavy, write-rare**.

| Context | Load | Reload | Ownership |
|---------|------|--------|-----------|
| CLI commands | Once in `main()` | Never — short-lived | `Arc<TagSchema>` passed down |
| TUI (browse) | Once at session start | After alias mutations: save to disk, reload, swap Arc | `Arc<TagSchema>` — single-threaded, no Mutex |
| Daemon | Once at startup | On `ConfigReloaded` event or alias mutation | `ArcSwap<TagSchema>` or `Mutex<Arc<TagSchema>>` — multi-threaded |

**Rules:**
- Delete all per-query `load_default_schema()` calls (currently 12+ sites)
- Query engine takes `&TagSchema` as parameter — no hidden I/O
- After schema mutations (e.g., alias creation in TUI), write to `tag_schema.toml`, reload from disk, replace the `Arc`
- Schema is passed by reference (`&TagSchema`) into query engine, `expand_tags()`, etc.

## Schema: Alias System

Aliases are **prefix substitutions** defined in `tag_schema.toml`, resolved during query expansion.

```toml
[aliases]
lang = "language"              # lang → language
js = "language:javascript"     # js → language:javascript
```

**Expansion examples:**
```
alias: lang = language
  "lang"           → "language"
  "lang:rust"      → "language:rust"
  "lang:rust:arrays" → "language:rust:arrays"

alias: js = language:javascript
  "js"             → "language:javascript"
  "js:libraries"   → "language:javascript:libraries"
```

Match is **prefix only**, at segment boundaries (`:` delimited). `mylang` does NOT expand.

**Glob patterns (`*`, `**`) are separate from aliases.** They live in `TagName::matches_glob()`
and are used by the query engine and TUI fuzzy picker:
```
tagr search -t "lang:*:arrays"
→ alias expands "lang" to "language"
→ glob "language:*:arrays" matches language:rust:arrays, language:php:arrays
```

**Alias swap:** `tagr alias swap js javascript` atomically:
1. Flips canonical ↔ alias in schema
2. Renames stored tags in DB (`TagRenamed` event)
3. Emits `ConfigReloaded` to reload schema

**No chaining:** Aliases resolve once. `a → b → c` is rejected by `TagSchema::validate()`.

## DaemonStore Caching Strategy

TUI browse mode needs instant feedback on filter changes. IPC round-trips per toggle
are too slow. DaemonStore maintains a **widest-result cache**:

```rust
struct QueryCache {
    widest_criteria: QueryCriteria,          // the least restrictive query we've run
    widest_data: Vec<Pair>,                  // its full result set
    notes: HashMap<TagrPath, NoteRecord>,    // note cache — all notes for result set
}

// DaemonStore uses interior mutability for the cache:
// TagStore trait is &self, but query() must mutate cache on widen.
// RefCell won't work — TagStore: Send + Sync requires thread-safe types.
struct DaemonStore {
    client: PersistentClient,
    cache: RwLock<QueryCache>,              // read lock for narrow, write lock for widen
    event_rx: Mutex<mpsc::Receiver<ServerEvent>>,  // event drain
}
```

**Cache initialization — two IPC calls on first load:**

```rust
// 1. Query pairs (tags + files)
let pairs = self.client.query(&criteria)?;
// 2. Bulk fetch all notes (no per-file IPC ever)
let notes = self.client.list_notes()?;

let mut cache = self.cache.write();
cache.widest_criteria = criteria;
cache.widest_data = pairs;
cache.notes = notes.into_iter().collect();
// From here: filter narrowing, note browsing — all local, zero IPC.
```

After initialization, `NoteChanged` events update the note cache incrementally.
Note lookups (`get_note(&file)`) read directly from `cache.notes` — never IPC.

**Lock discipline — keep lock scopes minimal:**

```rust
// ✅ GOOD: Narrow path — read lock held only during local iteration
fn query_narrow(&self, criteria: &QueryCriteria) -> Vec<Pair> {
    let cache = self.cache.read();        // read lock
    cache.widest_data.iter()
        .filter(|p| criteria.matches_pair(p))
        .cloned()
        .collect()                        // lock drops after collect
}

// ✅ GOOD: Widen path — IPC OUTSIDE lock, brief write lock to swap
fn query_widen(&self, criteria: &QueryCriteria) -> Result<Vec<Pair>> {
    let result = self.client.query(criteria)?;   // IPC — no lock held!
    let mut cache = self.cache.write();          // brief write lock
    cache.widest_criteria = criteria.clone();
    cache.widest_data = result.clone();
    Ok(result)                                   // lock drops
}

// ❌ BAD: Write lock held across IPC round-trip
fn query_widen_bad(&self, criteria: &QueryCriteria) -> Result<Vec<Pair>> {
    let mut cache = self.cache.write();          // write lock acquired
    let result = self.client.query(criteria)?;   // IPC while holding lock!
    cache.widest_data = result;                  // lock held entire time
    Ok(...)
}

// ✅ GOOD: Event drain — two brief locks, never nested
fn drain_events(&self) {
    let events: Vec<_> = {
        let rx = self.event_rx.lock();           // brief Mutex lock
        std::iter::from_fn(|| rx.try_recv().ok()).collect()
    };                                           // Mutex drops
    if !events.is_empty() {
        let mut cache = self.cache.write();      // brief write lock
        for event in events {
            cache.apply_event(event);            // no I/O, just data mutation
        }
    }                                            // write lock drops
}
```

**Rules:**
1. Never hold a lock across I/O (IPC, filesystem, network)
2. Do work outside the lock, lock briefly to read/swap
3. Never nest locks (`event_rx` lock + `cache` lock simultaneously) — deadlock risk

**Query decision:**
1. New criteria is a **subset** of `widest_criteria`? → filter `widest_data` locally (instant)
2. New criteria is **wider**? → IPC query, replace cache with new wider result

**Subset checking is flat-only initially.** For flat AND tag lists, "more tags = narrower"
is trivial to check. For complex `TagExpr` trees, subset detection is boolean satisfiability
(potentially NP-hard). Complex expressions always go to IPC — no local filtering attempted.

**Local filtering lives in Layer 0** — `QueryCriteria::matches_pair(&self, pair: &Pair) -> bool`
evaluates a pair against the criteria without mutation. This avoids `store/` importing `query/`.
Hierarchy expansion was already applied when the widest query ran on the server, so local
filtering is a simple tag membership check against the expanded result set.

**ServerEvent handling — separate event loop, designed for upgrade:**

Initial implementation uses single-loop polling (TUI drains events between frames).
The design supports upgrading to a separate event thread without architectural changes:

```
Ship (single loop):    loop { input → drain_events() → render }
Target (split loops):  Thread 1: loop { input → check dirty → render }
                       Thread 2: loop { recv event → update cache → set dirty }
```

Both patterns work with the same `RwLock<QueryCache>` — the upgrade is mechanical.
The TUI checks a `dirty: AtomicBool` flag to know when to re-query/re-render.

**ServerEvent incremental updates (no full re-query):**
- `FileTagged { file, tags }` → update/add Pair in cache
- `FileUntagged { file, tags }` → update Pair, remove if no longer matches widest
- `FileRemoved { file }` → remove from pair cache AND note cache
- `NoteChanged { file, content: Some(c) }` → upsert into note cache (no re-query)
- `NoteChanged { file, content: None }` → remove from note cache
- `ConfigReloaded` → **full cache invalidation** (schema changed, expansions may differ)

**Result:** After the two initial IPC calls, filter toggling AND note browsing are
fully local. IPC only fires on widen-beyond-cache or data mutation events.
DirectStore doesn't need this cache — sled queries are already local.

## Error Architecture

Error types are a first-class design concern. Well-designed errors eliminate entire
categories of bugs, make testing precise, and give users actionable messages.

### Design principles

1. **Errors describe the problem, not the implementation** — `FileNotFound { file }`
   not `SledError(sled::Error)`. Callers shouldn't know we use sled.
2. **Newtypes eliminate error paths** — `TagName::new()` validates once at construction.
   Functions taking `&TagName` can never receive invalid input. No `InvalidTagName`
   variant needed in downstream errors.
3. **Each module owns its error type** — specific enums with meaningful variants.
   No god-enum that wraps everything.
4. **`thiserror` for library layers, `anyhow` for application layers** — library code
   is called programmatically (plugins, tests, future crates). Application code
   formats errors for humans and exits.
5. **Embed, don't wrap** — instead of wrapping third-party errors, extract the relevant
   information into domain-specific variants. Keeps dependencies out of the public API.

### Error boundary: thiserror vs anyhow

```
┌─────────────────────────────────────────────────────────────────┐
│ Layer 5: anyhow::Result                                         │
│                                                                  │
│  commands/     .context("failed to tag {file}")?                │
│  ui/           .context("render error")?                        │
│  daemon/       .context("IPC handler failed")?                  │
│                                                                  │
│  Adds human-readable context, formats for display, exit codes.  │
│  Never matches on error variants — just propagates + displays.  │
├─────────────────────────────────────────────────────────────────┤
│ Layers 0–4: thiserror enums                                     │
│                                                                  │
│  Specific, matchable, testable error types.                     │
│  Callers can programmatically handle each case.                 │
│  Plugins and tests rely on these for precise error handling.    │
└─────────────────────────────────────────────────────────────────┘
```

The boundary is at the **module public API**. Internal helper functions within a module
may use `anyhow` for convenience, but the module's public functions return `thiserror` types.

### Error types by layer

**Layer 0: `types::ValidationError`** — newtype construction failures.
Eliminates downstream `InvalidInput` variants entirely.

```rust
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ValidationError {
    #[error("tag name is empty")]
    Empty,
    #[error("tag name exceeds {max} characters: {len}")]
    TooLong { len: usize, max: usize },
    #[error("invalid character '{ch}' in tag name at position {pos}")]
    InvalidChar { ch: char, pos: usize },
    #[error("tag name cannot start or end with ':'")]
    LeadingOrTrailingDelimiter,
    #[error("tag name contains empty segment (::)")]
    EmptySegment,
    #[error("'{prefix}' is a reserved virtual tag prefix")]
    ReservedVtagPrefix { prefix: String },
    #[error("path is not valid UTF-8: {path}")]
    InvalidUtf8 { path: String },
}
```

Note: `Clone + PartialEq + Eq` — validation errors are testable with `assert_eq!`.

**Layer 1: `store::StoreError`** — storage operations in domain terms.
Hides sled, postcard, and I/O behind what actually went wrong.

```rust
#[derive(Debug, Error)]
pub enum StoreError {
    #[error("file not found in database: {0}")]
    FileNotFound(TagrPath),

    #[error("tag does not exist: {0}")]
    TagNotFound(TagName),

    #[error("database is locked by another process")]
    DatabaseLocked,

    #[error("storage corrupted at key '{key}': {reason}")]
    StorageCorrupted { key: String, reason: String },

    #[error("database I/O failed: {context}")]
    IoFailed {
        context: String,
        #[source]
        source: std::io::Error,
    },

    #[error("storage backend unavailable: {context}")]
    ConnectionLost { context: String },
}
```

No `#[from] sled::Error`. Sled errors are mapped to domain variants in the `DirectStore`
implementation — the only place that knows about sled. `ConnectionLost` is used by
`DaemonStore` when IPC fails after internal reconnection attempts are exhausted —
callers see "backend unavailable" without knowing the transport is a Unix socket.

```rust
// Inside DirectStore — the ONLY place sled errors are handled
impl TagStore for DirectStore {
    fn find_by_tag(&self, tag: &TagName) -> Result<Vec<TagrPath>, StoreError> {
        self.db.tags.get(tag.as_str().as_bytes())
            .map_err(|e| StoreError::IoFailed {
                context: format!("reading tag index for '{tag}'"),
                source: std::io::Error::new(std::io::ErrorKind::Other, e),
            })?
            // ... deserialization ...
            .ok_or_else(|| StoreError::TagNotFound(tag.clone()))
    }
}
```

If we replace sled with another backend, only `DirectStore` changes. `StoreError`
and all callers remain untouched.

**Layer 1: `schema::SchemaError`** — schema loading/validation.

```rust
#[derive(Debug, Error)]
pub enum SchemaError {
    #[error("failed to read schema file: {path}")]
    ReadFailed { path: PathBuf, #[source] source: std::io::Error },
    #[error("invalid schema TOML: {reason}")]
    InvalidToml { reason: String },
    #[error("alias chain detected: {chain:?}")]
    AliasChain { chain: Vec<String> },
    #[error("alias '{alias}' conflicts with reserved vtag prefix")]
    AliasConflict { alias: String },
}
```

**Layer 1: `filters::FilterError`** — filter CRUD.

```rust
#[derive(Debug, Error)]
pub enum FilterError {
    #[error("filter not found: {0}")]
    NotFound(String),
    #[error("filter name is invalid: {reason}")]
    InvalidName { reason: String },
    #[error("failed to read filters file")]
    ReadFailed { #[source] source: std::io::Error },
    #[error("failed to write filters file")]
    WriteFailed { #[source] source: std::io::Error },
    #[error("invalid TOML in filters file: {reason}")]
    InvalidToml { reason: String },
}
```

**Layer 3: `query::QueryError`** — query execution failures.

```rust
#[derive(Debug, Error)]
pub enum QueryError {
    #[error("invalid tag regex: {pattern}")]
    InvalidRegex { pattern: String, #[source] source: regex::Error },
    #[error("invalid file glob: {pattern}")]
    InvalidGlob { pattern: String, reason: String },
    #[error("virtual tag evaluation failed for {vtag}: {reason}")]
    VtagFailed { vtag: String, reason: String },
    #[error(transparent)]
    Store(#[from] StoreError),
}
```

`QueryError` wraps `StoreError` via `#[from]` — this is acceptable because
`StoreError` is our own domain type (not a third-party leak), and `?` propagation
through the query pipeline is natural.

**Layer 4: `browse::SessionError`** — browse session domain errors.

```rust
#[derive(Debug, Error)]
pub enum SessionError {
    #[error("query failed: {0}")]
    QueryFailed(#[from] QueryError),
    #[error("filter not found: {0}")]
    FilterNotFound(String),
    #[error("no files match current criteria")]
    EmptyResult,
    #[error("command execution failed: {context}")]
    CommandFailed { context: String, #[source] source: std::io::Error },
}
```

**Layer 5: `anyhow::Result`** — application boundary.

```rust
// commands/search.rs — just adds context and exits
pub fn execute(store: &dyn TagStore, args: &SearchArgs) -> anyhow::Result<()> {
    let criteria = QueryCriteria::try_from(args)
        .context("invalid search arguments")?;

    let results = store.query(&criteria, &schema)
        .context("search query failed")?;

    if results.is_empty() {
        // Exit code 1 for "not found" — script composition
        std::process::exit(1);
    }

    for path in &results {
        println!("{path}");
    }
    Ok(())
}
```

### What this eliminates

| Current problem | How the new design fixes it |
|-----------------|----------------------------|
| `TagrError` god-enum (11 variants) | Each layer has its own error. Layer 5 uses `anyhow`. |
| `DbError` leaks `sled::Error`, `bincode::Error` | `StoreError` hides backend. Only `DirectStore` knows sled. |
| `SearchError` wraps `UiError` | Layer violation impossible — `QueryError` can't import ui/. |
| `InvalidInput(String)` everywhere | `TagName::new()` validates at construction. Downstream functions can't receive invalid input. |
| "Database error: sled error" messages | `StoreError::IoFailed { context: "reading tag index for 'rust'" }` — actionable. |
| Two `BrowseError` with same name | One `SessionError` for Layer 4, `anyhow` for Layer 5 UI. |
| Error types leak dependencies | `#[from]` only for our own types. Third-party errors embedded, not wrapped. |

### Testing errors

Specific error types enable precise test assertions:

```rust
#[test]
fn reject_reserved_vtag_prefix() {
    let err = TagName::new("modified:today").unwrap_err();
    assert_eq!(err, ValidationError::ReservedVtagPrefix {
        prefix: "modified".to_string()
    });
}

#[test]
fn missing_file_returns_not_found() {
    let store = MockStore::new();
    let err = store.get_tags(&TagrPath::new("/nonexistent").unwrap()).unwrap_err();
    assert!(matches!(err, StoreError::FileNotFound(_)));
}
```

With `anyhow` you can only test "did it fail?" With `thiserror` you test "did it fail
for the right reason?" — catching regressions that change failure modes.

## Testing Priorities

Not all code is equally critical. A rendering glitch in the TUI is visible and fixable.
A silent corruption in the tag index can destroy a user's entire tagging history with
no indication until they notice missing results weeks later.

### Tier 1: Invariant-critical (exhaustive testing)

These components, if broken, cause **silent data loss or corruption**. Bugs here are
invisible to the user until it's too late. Every edge case must be covered.

**1. Dual-tree consistency (`store/`)**

The core invariant of tagr: the `files` tree and `tags` reverse index must always
agree. If `files["src/main.rs"]` contains tag `"rust"`, then `tags["rust"]` must
contain `"src/main.rs"` — and vice versa.

```
files["a.rs"] = [rust, cli]     tags["rust"] = [a.rs, b.rs]
files["b.rs"] = [rust]          tags["cli"]  = [a.rs]
       ↑                              ↑
       └──────── MUST AGREE ──────────┘
```

Test scenarios:
- Add tags → both trees updated
- Remove tags → file removed from tag index, tag removed from file entry
- Remove last file from a tag → tag key deleted from index (no empty vecs)
- Rename tag → all file entries updated AND tag index key swapped atomically
- Merge tags → source tag's files moved to target, source key deleted
- Concurrent mutations → no partial updates visible
- Crash recovery → sled flush guarantees, no half-written states
- Round-trip: `insert_pair()` then `get_tags()` then `find_by_tag()` → consistent

This is the ONE invariant that, if violated, makes every query return wrong results.

**2. Newtype validation (`types/`)**

`TagName` and `TagrPath` are the input boundary. If invalid data passes construction,
it propagates through the entire system — into the database, over IPC, into configs.

```rust
// If this accepts "rust::cli" (empty segment), the tag index
// stores a key that can never be found by prefix search.
TagName::new("rust::cli")  // MUST return Err(EmptySegment)
```

Test scenarios:
- Every `ValidationError` variant has at least one positive test
- Boundary cases: empty string, max length, max length + 1
- Unicode: valid UTF-8 with special characters, emoji in tags
- Delimiter edge cases: leading `:`, trailing `:`, consecutive `::`, single `:`
- Reserved prefixes: all vtag prefixes (`modified:`, `size:`, `ext:`, etc.)
- `segments()` returns correct parts for all valid names
- `join()` validates the result (joining valid parts can create invalid names)
- `Ord` ordering: verify `:` sorts before alphanumeric (hierarchy correctness)

**3. `QueryCriteria::matches_pair()` (`types/`)**

This function is the local filter in `DaemonStore`'s cache. If it disagrees with
what the daemon's `TagStore` would return, the TUI shows wrong results — and the
user has no way to know.

```
Server returns 500 files (widest query)
    → matches_pair() filters to 120 (narrowed)
    → If matches_pair() is wrong, user sees 118 or 123 files
    → Silent. No error. Just wrong results.
```

Test scenarios:
- Tag inclusion: exact match, hierarchy match (`rust` matches `rust:cli`)
- Tag exclusion: excluded tags correctly filtered out
- File glob patterns: wildcards, directory patterns, case sensitivity
- Regex patterns: valid patterns, special characters
- Combined criteria: tags AND globs AND regex simultaneously
- Empty criteria: matches everything (no false negatives)
- Virtual tag criteria: size, extension, modified time ranges
- **Property**: `matches_pair()` must agree with `TagStore::query()` for all inputs.
  This is a prime candidate for property-based testing with `proptest`.

**4. Wire protocol serialization (`ipc/`)**

If a message serializes differently than it deserializes, the daemon and client
silently misunderstand each other. There's no runtime type checking on the wire.

Test scenarios:
- Round-trip every `Request` variant: serialize → deserialize → assert equal
- Round-trip every `Response` variant
- Round-trip every `ServerEvent` variant
- Newtypes on the wire: `TagName`, `TagrPath` survive serialization
- Empty collections: empty tag lists, empty file lists
- Large payloads: thousands of files in a `QueryResult`
- Malformed input: truncated messages, garbage bytes → clean error, no panic
- Version skew: old client, new daemon (future concern, but test the boundary)

### Tier 2: Correctness-critical (thorough testing)

These components, if broken, produce **visible but potentially confusing errors**.
Users can tell something is wrong, but may not understand why.

**5. Schema resolution (`schema/`)**

Alias chains, hierarchy rules, group membership. Wrong resolution means tags
behave unexpectedly — user tags `"js"` expecting alias to `"javascript"`, but
the alias doesn't fire.

Test scenarios:
- Single alias: `js → javascript`
- Alias chain detection: `a → b → c` → error
- Alias to hierarchical tag: `fe → dev:frontend`
- Group membership: `lang:*` matches `lang:rust`, `lang:go`
- Schema hot-reload: changed alias takes effect without restart
- Conflicting rules: alias name = existing tag name

**6. DaemonStore cache logic (`browse/datasource/`)**

Narrow vs widen decisions, cache invalidation on events. Wrong logic means
unnecessary IPC calls (performance) or stale data (correctness).

Test scenarios:
- Narrow within cache: no IPC, correct results
- Widen beyond cache: IPC fires, cache replaced
- `NoteChanged` event: note cache updated incrementally
- `FileTagged`/`FileUntagged` events: query cache invalidated
- `ConfigReloaded` event: full cache clear
- Concurrent events during query: no data races (lock discipline)

**7. Filter persistence (`filters/`)**

CRUD operations on saved filters. Corruption means users lose saved workflows.

Test scenarios:
- Save → load round-trip
- Delete non-existent filter → clean error
- Export → import round-trip (different machine, different paths)
- TOML special characters in filter names
- Concurrent CLI writes (two `tagr filter save` at once)

### Tier 3: UX-critical (standard testing)

Bugs here are **immediately visible** and **non-destructive**. The user sees
something wrong and can work around it.

**8. CLI output formatting (`commands/`)**
- Quiet/verbose/JSON modes produce correct output
- Exit codes: 0 for success, 1 for not-found
- Pipe-friendly output (no ANSI in quiet mode)

**9. TUI rendering (`ui/`)**
- Key bindings trigger correct actions
- State transitions (tag view ↔ file view)
- Empty state handling (no files, no tags)

**10. Configuration loading (`config/`)**
- Missing config file → defaults
- Partial config → defaults for missing fields
- Invalid TOML → actionable error message

### Testing tools by tier

| Tier | Tool | Why |
|------|------|-----|
| 1 | `proptest` | Property-based: "for ALL valid inputs, invariant holds" |
| 1 | `assert_eq!` on error variants | Exact failure mode verification |
| 1–2 | Unit tests with `MockStore` | Isolate logic from storage backend |
| 1–2 | Integration tests with `TestDb` | Real sled, real serialization |
| 2–3 | `insta` (snapshot testing) | CLI output, TOML serialization |
| 3 | Manual + basic unit tests | Visual correctness, key bindings |

### The property-based testing case

Tier 1 components benefit most from `proptest` because their correctness is
defined as **invariants that must hold for all inputs**, not specific examples:

```rust
proptest! {
    #[test]
    fn dual_tree_consistency(pairs in vec(arbitrary_pair(), 1..100)) {
        let store = TestDb::new();
        for pair in &pairs {
            store.insert_pair(pair).unwrap();
        }
        // Invariant: every tag in files tree exists in tags tree
        for pair in store.list_all().unwrap() {
            for tag in &pair.tags {
                let files = store.find_by_tag(tag).unwrap();
                prop_assert!(files.contains(&pair.file));
            }
        }
    }

    #[test]
    fn matches_pair_agrees_with_store(
        criteria in arbitrary_criteria(),
        pairs in vec(arbitrary_pair(), 1..50)
    ) {
        let store = TestDb::new();
        for pair in &pairs {
            store.insert_pair(pair).unwrap();
        }
        let store_results = store.query(&criteria).unwrap();
        let local_results: Vec<_> = pairs.iter()
            .filter(|p| criteria.matches_pair(p))
            .collect();
        prop_assert_eq!(store_results, local_results);
    }
}
```

These two properties alone catch the most dangerous class of bugs in tagr:
data inconsistency that the user can't see.

## Plugin / Integration API

External tools (editor plugins like `tagr.nvim`, scripts, CI pipelines) are
**Layer 5 frontends** — same as CLI and TUI. The architecture must support them
without special-casing.

### Integration modes

| Mode | Transport | Use case |
|------|-----------|----------|
| CLI + JSON | `tagr ... --format json` | Simple scripts, one-shot queries |
| Structured query | `tagr query --json '<QueryCriteria>'` | Complex queries from editors |
| Event stream | `tagr events --json` | Live updates (NDJSON over stdout) |
| Daemon IPC | Binary protocol over Unix socket | High-performance native plugins |
| Library | `tagr` crate as dependency | Rust-native integrations |

### Endpoints to keep in mind

These don't need to exist now, but the types and traits must support them:

- **Structured query input**: `QueryCriteria` as JSON → results as JSON.
  CLI shouldn't force plugins to construct `-t foo -t bar --mode all` flags.
- **Tag completion**: `tagr complete tags --prefix "lang:" --format json` →
  `[{"name": "lang:rust", "count": 42}, ...]`. For editor autocomplete.
- **Event subscription**: Stream `ServerEvent`s as NDJSON for live sync.
  Plugin sees file tagged/untagged/removed in real-time.
- **Mutation API**: `tagr tag --json '{"file": "...", "tags": [...]}'` —
  structured input for batch operations without shell escaping.

### Design constraints from plugin support

1. **All query logic goes through `QueryCriteria`** — no CLI-flag-only features
2. **`--format json` on every command that produces output** — not just search
3. **`ServerEvent` must be serializable to JSON** — not just binary wire format
4. **`TagStore` trait is the only data interface** — plugins get the same
   abstraction whether they go through CLI, IPC, or library

## Dependency Direction

```
types ← store trait ← query ← browse (session) ← ui (AppState)
         filters                                 ← commands (CLI)
         schema                                  ← daemon (server)

types has zero internal dependencies
store trait depends only on types
filters/ depends only on types (TOML I/O for saved queries)
schema/ depends only on types (TOML I/O for aliases/hierarchy)
query depends on store trait + types + schema
browse depends on store trait + query + types + schema + filters
frontends (ui, commands, daemon) depend on layers below them
daemon depends on store + types (never on ui or browse)
```

## See Also

- [interface-map.md](interface-map.md) — detailed type inventory, conversion map, functionality audit
- [migration-plan.md](migration-plan.md) — phased migration path from current to target
