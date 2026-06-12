# Migration Plan

> Phased migration from current architecture to target.
> Each phase compiles and passes tests before proceeding.

## Prerequisite: Ownership & Borrowing Strategy

Good borrowing patterns require good architecture. The current codebase has
structural issues that **force** unnecessary cloning:

### Current Copying Hot Spots

**1. Bidirectional sync copies entire tag sets every toggle (~4 allocations):**
```rust
// sync_tag_tree_from_filter: Vec → HashSet (clone each string)
tree.selected_tags = self.active_filter.criteria.tags.iter().cloned().collect();
// sync_filter_from_tag_tree: HashSet → Vec (clone each string back)
self.active_filter.criteria.tags = tree.selected_tags.iter().cloned().collect();
```
Root cause: two data structures (`ActiveFilter.criteria.tags: Vec<String>` and
`TagTreeState.selected_tags: HashSet<String>`) store the same data in different
shapes. Every toggle copies between them.

**Fix**: Single source of truth. `QueryCriteria` owns the tag expression as `TagExpr`.
The tag tree reads from it directly via `flat_include_tags()` — no sync needed.

**2. Tag names are `String` — every function boundary clones:**
```rust
// toggle_include_tag takes String (owned), so caller must clone
active_filter.toggle_include_tag(tag.clone());
// find_by_tag takes &str, but returns Vec<PathBuf> which gets collected into Vec<String>
```
Fix: `TagName` newtype with `AsRef<str>` + `Borrow<str>`. Functions take `&TagName`,
HashSets support `.contains("foo")` via `Borrow`. Ownership only transfers when inserting.

**3. keybinds/executor.rs clones `current_file` 7 times identically:**
```rust
context.current_file.iter().map(|p| (*p).clone()).collect()  // 7 identical lines
```
Fix: Extract to a method, consider `Cow<[PathBuf]>` or `&[PathBuf]` return.

**4. `format_for_display` calls `path.canonicalize()` per file (syscall + alloc):**
```rust
file_meta.path.canonicalize().ok().and_then(|canonical| {
    self.session.data_source().get_note(&canonical).ok().flatten()
})
```
Already fixed in note cache (pre-indexed canonical paths), but shows the pattern:
filesystem operations in display code cause allocations.

**5. 121 `Vec<String>` parameters vs 27 `&[String]`:**
Many functions take `Vec<String>` ownership when they only need to read. With newtypes,
the signature becomes `&[TagName]` — zero-copy, can't accidentally modify.

### Borrowing Design Principles

1. **Newtypes enable borrowing** — `TagName` implements `Borrow<str>` so
   `HashSet<TagName>.contains("rust")` works without allocating a TagName.

2. **Store trait returns owned data** — query results cross thread/async boundaries,
   so `fn find_by_tag(&self, tag: &TagName) -> Result<Vec<TagrPath>>` returns owned.
   Callers can borrow the results (iterate by reference).

3. **Query engine takes references, returns owned** —
   `fn execute(store: &dyn TagStore, criteria: &QueryCriteria, ...) -> Vec<TagrPath>`.
   Criteria is borrowed (caller keeps it), results are owned (caller stores them).

4. **Display conversion borrows domain data** —
   `fn to_display_item(&self, path: &TagrPath, note_cache: &HashMap<...>) -> DisplayItem`.
   The display item owns its strings (needed for ratatui), but the conversion borrows inputs.

5. **Single source of truth eliminates sync copies** — `QueryCriteria` IS the filter state.
   The tag tree reads `criteria.tags` directly. No bidirectional sync.

6. **IPC boundary is the one place that must own** — `QueryCriteria` is serialized/deserialized
   across the socket. The daemon gets an owned copy. This is inherent and correct.

### Target Signature Patterns

```rust
// Store: borrows input, returns owned (crosses boundaries)
fn find_by_tag(&self, tag: &TagName) -> Result<Vec<TagrPath>>;
fn get_tags(&self, file: &TagrPath) -> Result<Option<Vec<TagName>>>;

// Query engine: borrows everything, returns owned results
fn execute(store: &dyn TagStore, criteria: &QueryCriteria, schema: &TagSchema) -> Result<Vec<TagrPath>>;
fn expand_tags(tags: &[TagName], schema: &TagSchema, store: &dyn TagStore) -> Result<Vec<TagName>>;
fn filter_by_hierarchy(files: &[(TagrPath, Vec<TagName>)], inc: &[TagName], exc: &[TagName]) -> Vec<TagrPath>;

// Mutations: take ownership of what gets stored
fn insert(&self, file: &TagrPath, tags: Vec<TagName>) -> Result<()>;  // tags move into DB
fn add_tags(&self, file: &TagrPath, tags: Vec<TagName>) -> Result<()>;

// Criteria mutation: borrows self, takes ownership of new tag
fn toggle_include_tag(&mut self, tag: TagName) -> bool;  // tag moves into the set

// Display: borrows domain data, returns owned display strings
fn path_to_display_item(&self, path: &TagrPath) -> DisplayItem;

// CLI boundary: parses and validates into owned newtypes
fn try_from(args: &SearchCriteriaArgs) -> Result<QueryCriteria, ValidationError>;
```

---

## Phase 1: Core Types (foundation)

**Goal**: Create the shared vocabulary. Nothing else changes.

### New files
- `src/types/mod.rs` — `TagName`, `TagrPath`, `FilterName`, `MatchMode`, `PathFormat`, `TagExpr`, `QueryCriteria`, `Pair`, `ValidationError`

### `TagName` design
```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub struct TagName(String);

impl TagName {
    pub fn new(s: impl Into<String>) -> Result<Self, ValidationError>;
    pub fn as_str(&self) -> &str;

    // Hierarchy — these currently live as free functions in search/hierarchy.rs
    pub fn depth(&self) -> usize;                       // "a:b:c" → 3
    pub fn root(&self) -> &str;                         // "a:b:c" → "a"
    pub fn parent(&self) -> Option<TagName>;             // "a:b:c" → Some("a:b")
    pub fn segments(&self) -> impl Iterator<Item = &str>; // "a:b:c" → ["a", "b", "c"]
    pub fn join(&self, child: &str) -> Result<TagName, ValidationError>;  // "a:b".join("c") → "a:b:c"
    pub fn is_child_of(&self, parent: &TagName) -> bool;
    pub fn matches_pattern(&self, pattern: &TagName) -> bool;

    // Glob matching on colon-separated segments
    // * matches exactly one segment, ** matches one or more
    // "language:*:arrays" matches "language:rust:arrays" but not "math:arrays"
    // "*:rust" matches "language:rust" but not "metal:iron:rust"
    // "**:rust" matches "language:rust" AND "metal:iron:rust"
    pub fn matches_glob(&self, pattern: &str) -> bool;
}

// Derives: Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize
// Ord is lexicographic on inner String — correct for hierarchy sorting since
// ':' (0x3A) sorts before all alphanumeric chars (parents before children).
impl AsRef<str> for TagName { ... }
impl Borrow<str> for TagName { ... }  // enables HashSet<TagName>.contains("foo")
impl Display for TagName { ... }
// NO Deref<Target=str> — forces .as_str() for raw string access,
// keeping TagName methods (depth, is_child_of) as the obvious choice.
```

### `QueryCriteria` design
```rust
/// Boolean expression tree for tag matching.
/// Supports arbitrary nesting: (A & (B | C) & !D)
/// Simple cases map trivially: a flat tag list with AND/OR mode
/// becomes And(vec![Tag(a), Tag(b)]) or Or(vec![Tag(a), Tag(b)]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TagExpr {
    /// Matches a single tag (leaf node)
    Tag(TagName),
    /// Negation — matches files that do NOT match the inner expression
    Not(Box<TagExpr>),
    /// Conjunction — all sub-expressions must match
    And(Vec<TagExpr>),
    /// Disjunction — any sub-expression must match
    Or(Vec<TagExpr>),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryCriteria {
    /// Tag filter expression (None = no tag filtering, matches all files)
    /// Simple: And(vec![Tag("rust"), Tag("web")])
    /// Complex: And(vec![Tag("recipe:cakes"), Or(vec![Tag("sweet"), Tag("fruity")]), Not(Tag("nuts"))])
    pub tag_expr: Option<TagExpr>,
    pub regex_tags: bool,
    pub expand_hierarchy: bool,  // default: true

    pub file_patterns: Vec<String>,
    pub file_mode: MatchMode,
    pub regex_files: bool,

    pub virtual_tags: Vec<String>,
    pub virtual_mode: MatchMode,

    pub query: Option<String>,
}

// Convenience methods for flat tag operations (initial TUI/CLI usage)
impl QueryCriteria {
    /// Add a tag to the include set (flat AND/OR mode)
    pub fn toggle_include_tag(&mut self, tag: TagName) -> bool;
    /// Add a tag to the exclude set (wraps in Not)
    pub fn toggle_exclude_tag(&mut self, tag: TagName) -> bool;
    /// Switch between AND/OR for the top-level tag expression
    pub fn toggle_tag_mode(&mut self);
    /// Extract flat tag list (for TUI display) — returns None if expr is complex.
    /// NOTE: Returns HashSet<&TagName>, NOT HashSet<TagName>. Lookups must use
    /// &TagName, not &str — `&TagName: Borrow<str>` is NOT satisfied (only
    /// `TagName: Borrow<str>` is). Tag tree nodes must store TagName for O(1) lookup.
    pub fn flat_include_tags(&self) -> Option<HashSet<&TagName>>;
    /// Extract flat exclude list — returns None if expr is complex (same Borrow note)
    pub fn flat_exclude_tags(&self) -> Option<HashSet<&TagName>>;
    pub fn is_empty(&self) -> bool;
    pub fn to_cli_string(&self) -> String;
}
```

### Temporary bridges (deleted in Phase 5)
```rust
impl From<&SearchParams> for QueryCriteria { ... }
impl From<&QueryCriteria> for SearchParams { ... }
impl From<&FilterCriteria> for QueryCriteria { ... }
impl From<&QueryCriteria> for FilterCriteria { ... }
```

### Validation
- Bring existing tests from `search/hierarchy.rs` for tag depth/root/pattern matching
- Add tests for `TagName` validation, `QueryCriteria` toggle methods
- All existing tests still pass (old types untouched)

---

## Phase 2: TagStore Trait (abstraction)

**Goal**: Define the storage interface. Existing code still compiles.

### New files
- `src/store/mod.rs` — `trait TagStore`, `StoreError`
- `src/store/direct.rs` — `impl TagStore for Database` (thin wrapper)
- `src/store/mock.rs` — `MockStore` for tests

### Trait design
```rust
pub trait TagStore: Send + Sync {
    // Queries
    fn list_all_tags(&self) -> Result<Vec<TagName>>;
    fn list_tags_with_counts(&self) -> Result<Vec<(TagName, usize)>>;
    fn list_all_files(&self) -> Result<Vec<TagrPath>>;
    fn list_all(&self) -> Result<Vec<Pair>>;
    fn get_tags(&self, file: &TagrPath) -> Result<Option<Vec<TagName>>>;
    fn find_by_tag(&self, tag: &TagName) -> Result<Vec<TagrPath>>;
    fn find_by_all_tags(&self, tags: &[TagName]) -> Result<Vec<TagrPath>>;
    fn find_by_any_tag(&self, tags: &[TagName]) -> Result<Vec<TagrPath>>;
    fn find_by_tag_regex(&self, pattern: &str) -> Result<Vec<TagrPath>>;
    fn tag_exists(&self, tag: &TagName) -> Result<bool>;

    /// Prefix search — leverages sled's scan_prefix() for O(log n + k) performance.
    /// "language" → [language:rust, language:python, language:go, ...]
    /// Powers: hierarchy expansion, tag tree children, autocomplete, alias resolution.
    /// DirectStore: single scan_prefix() call on tags tree.
    /// DaemonStore: IPC call (could be cached alongside QueryCache).
    fn find_tags_by_prefix(&self, prefix: &TagName) -> Result<Vec<TagName>>;

    // Mutations
    fn insert(&self, file: &TagrPath, tags: Vec<TagName>) -> Result<()>;
    fn add_tags(&self, file: &TagrPath, tags: Vec<TagName>) -> Result<()>;
    fn remove_tags(&self, file: &TagrPath, tags: &[TagName]) -> Result<()>;
    fn remove_file(&self, file: &TagrPath) -> Result<bool>;
    fn remove_tag_globally(&self, tag: &TagName) -> Result<u64>;

    // Notes
    fn get_note(&self, file: &TagrPath) -> Result<Option<NoteRecord>>;
    fn set_note(&self, file: &TagrPath, note: &NoteRecord) -> Result<()>;
    fn delete_note(&self, file: &TagrPath) -> Result<bool>;
    fn list_all_notes(&self) -> Result<Vec<(TagrPath, NoteRecord)>>;

    // Composite query — each impl provides its own body (NO default impl).
    // DirectStore calls query::execute() internally.
    // DaemonStore serializes criteria over IPC.
    // This keeps store/ free of query/ imports.
    fn query(&self, criteria: &QueryCriteria, schema: &TagSchema) -> Result<Vec<TagrPath>>;
}
```

### MockStore
```rust
pub struct MockStore {
    files: HashMap<TagrPath, Vec<TagName>>,
    notes: HashMap<TagrPath, NoteRecord>,
}

impl MockStore {
    pub fn new() -> Self;
    pub fn with_pairs(pairs: Vec<Pair>) -> Self;
    pub fn with_tags(tags: &[(&str, &[&str])]) -> Self;  // convenience
}
```

### What changes
- `impl TagStore for Database` — wrapper methods that convert String ↔ TagName at boundary
- Existing code continues using `Database` / `DataSource` directly
- MockStore enables unit tests without sled

---

## Phase 3: Query Engine + `search/` Consolidation

**Goal**: Single query pipeline. Both CLI and TUI use it. Delete `search/` module entirely.

### New files
- `src/query/mod.rs` — `execute()`, `expand_tags()`, `filter_by_hierarchy()`, `filter_by_patterns()`
- `src/query/hierarchy.rs` — hierarchy matching, specificity rules (from `search/hierarchy.rs`)
- `src/query/patterns.rs` — glob/regex file filtering (from `search/filter.rs`)

### What moves
- `db::query::apply_search_params()` logic → `query::execute()`
- `search::expand_tags()` → `query::expand_tags()` (takes `&dyn TagStore` not `&Database`)
- `search::hierarchy::*` → `query::hierarchy::*`
- `search::filter::*` → `query::patterns::*`
- `search::traits::*` → **deleted** — `AsFileTagPair` trait replaced by concrete `Pair`/newtype usage

### What's deleted
- `src/search/` — entire module removed (all contents moved to `query/`)
- `src/db/query.rs` — `apply_search_params()` replaced by `query::execute()`

### What changes
- `commands/search.rs` calls `store.query(&criteria, &schema)` instead of `apply_search_params(db, &params)`
- `state.rs::update_file_preview()` calls `store.query(&criteria, &schema)` instead of inline logic
- `state.rs::calculate_matching_files_with_exclusions()` — deleted, replaced by `store.query()`
- Schema loading moves to startup (cached), not per-query
- All `use crate::search::*` imports → `use crate::query::*`

### Tests
- Comprehensive tests using `MockStore`:
  - Parent tag expands to children
  - Hierarchy specificity (deeper tag overrides shallower)
  - Exclude wins across hierarchies
  - ANY vs ALL mode
  - File pattern filtering
  - Virtual tag filtering
  - Regex tag matching
  - Empty criteria → match all files (default behavior)
  - Round-trip: CLI args → QueryCriteria → query → results

---

## Phase 4: Split DataSource into DirectStore + DaemonStore

**Goal**: DataSource enum becomes TagStore trait implementations.

### New files
- `src/store/daemon.rs` — `DaemonStore` implementing `TagStore`

### What changes
- `DataSource::Direct(db)` → `DirectStore` (impl TagStore)
- `DataSource::Remote { rt, client }` → `DaemonStore` (impl TagStore)
- `DataSource` enum deleted
- All consumers change from `Arc<DataSource>` → `Arc<dyn TagStore>`
- `DaemonStore::query()` overrides default to send full QueryCriteria over IPC

### Additional call sites beyond DataSource
These modules use `&Database` directly, bypassing `DataSource`:
- `commands/dispatch.rs` — passes `&Database` to all non-browse commands
- `watch/matcher.rs` — `FilterEvaluator::matches()` takes `&Database`
- `completions/cache.rs` — cache invalidation calls `db.list_all_tags()`
- `commands/bulk/` — 3+ sites use `&Database` for rename/merge/propagate

All must change to `&dyn TagStore` in this phase.

### `as_database()` elimination
`DataSource::as_database() → Option<&Database>` is dead code — defined but never called.
It can be deleted with the `DataSource` enum in Phase 4 with no migration needed.

---

## Phase 5: Adopt Newtypes Throughout

**Goal**: Replace raw strings with newtypes at all boundaries.

### What changes
- `Database` methods: `&str` → `&TagName`, `Vec<String>` → `Vec<TagName>`
- `TagTreeState.selected_tags: HashSet<String>` → **deleted** (derived from `QueryCriteria.tag_expr`)
- `ActiveFilter` → **deleted** (replaced by `QueryCriteria` methods)
- IPC wire: `QueryCriteria` serializes directly (TagName is `Serialize`)
- CLI boundary: `TryFrom<&SearchCriteriaArgs> for QueryCriteria` validates strings → TagName
- Ad-hoc `(PathBuf, Vec<String>)` in daemon → `Pair`

### Delete temporary bridges
- Remove `From<SearchParams> for QueryCriteria` and reverse
- Remove `From<FilterCriteria> for QueryCriteria` and reverse

---

## Phase 6: Delete Old Types & Cleanup

**Goal**: Remove all replaced types and merge confused modules.

### Types deleted
| Type | Replacement |
|------|-------------|
| `SearchParams` (cli.rs) | `QueryCriteria` |
| `FilterCriteria` (filters/types.rs) | `QueryCriteria` (search params) + `SavedFilter` (name/description wrapper) |
| `FilterCriteriaBuilder` | `QueryCriteria::builder()` |
| `ActiveFilter` (browse/filter.rs) | Methods on `QueryCriteria` |
| `WireSearchParams` (ipc/wire.rs) | `QueryCriteria` serializes directly |
| `WireSearchMode` (ipc/wire.rs) | `MatchMode` serializes directly |
| `TagMode` (filters/types.rs) | `MatchMode` |
| `FileMode` (filters/types.rs) | `MatchMode` |
| `SearchMode` (cli.rs) | `MatchMode` |
| `SearchMode` (browse/models.rs) | `MatchMode` |
| `PathFormat` (config, cli, browse) | `types::PathFormat` (one location — currently defined 3x) |
| `PreviewConfig` (config, ui::traits) | `types::PreviewConfig` or `config::PreviewConfig` (currently 2x with manual From) |
| `RefineSearchCriteria` (ui/traits.rs) | Merged with `RefinedSearchCriteria` |
| `BrowseError` (browse/ui.rs) | Merged with `BrowseError` (session.rs) |
| `PathString` (db/types.rs) | `TagrPath` |
| `DataSource` (datasource.rs) | `dyn TagStore` |

### Module changes
- `browse/filter.rs` → deleted (ActiveFilter gone, methods on QueryCriteria)
- `datasource.rs` → deleted (replaced by `store/`)
- `search/` already deleted in Phase 3

### Sync elimination
`sync_tag_tree_from_filter()` and `sync_filter_from_tag_tree()` now operate
directly on `TagName` values — no String↔TagName conversion bridges needed.
The tag tree's `selected_tags` and `excluded_tags` use `HashSet<TagName>`,
and `QueryCriteria`'s `flat_include_tags()`/`flat_exclude_tags()` return
`HashSet<&TagName>`, so syncing is a direct clone without fallible parsing.

---

## Decision Log

Decisions to be made before/during implementation:

| # | Question | Status |
|---|----------|--------|
| 1 | `TagName` validation rules (allowed chars) | **Decided:** alphanumeric + `-_:.`, max 128 chars, no leading/trailing `:`, no `::`, no whitespace, case-sensitive. Vtag prefixes (`size`, `ext`, `modified`, `created`, `accessed`, `perm`, `git`, `path`, `type`) are **reserved** — `TagName::new()` rejects them with `ValidationError::ReservedVtagPrefix`. |
| 2 | `TagrPath` — canonicalize on construction or store as-given | **Decided:** Store as-given, canonicalize at CLI boundary. `TagrPath::new()` validates UTF-8 only. CLI commands call `std::fs::canonicalize()` before creating `TagrPath` — rejects non-existent files with an error. Future: content hash for move tracking (separate concern, store layer). |
| 3 | Should saved filters support storing `query` field | **Decided: yes.** Saved filters persist all `QueryCriteria` fields including `query`. Merge semantics are **intersect** (AND): CLI args narrow the saved filter. Both tag expressions are ANDed, both queries must match, vtags and file patterns union. Modes: CLI overlay wins if explicitly set. |
| 4 | Phase ordering — foundation first then stabilize, or straight through | **Decided:** Foundation first. Each phase compiles and passes tests before proceeding. Phase 3 absorbs search/ cleanup (no hollow modules). |
| 5 | `QueryCriteria.tags` — `Vec<TagName>` or `HashSet<TagName>` | **Decided: `TagExpr`** — expression tree replaces flat tag collections. `flat_include_tags()` returns `Option<HashSet<&TagName>>` for TUI rendering (O(1) contains). Custom serde for deterministic serialization. |
| 6 | `Deref<Target=str>` on `TagName` | **Decided: no** — use `AsRef<str>` + `Borrow<str>` + `Display`. Explicit `.as_str()` for raw string access. |
| 7 | Serialization: migrate from bincode to postcard | **Decided: postcard** |
| 8 | DB schema: keep current 2-tree string-key design | **Decided: yes** — newtypes are compile-time only, no DB migration needed for types. Serialization migration (bincode→postcard) is separate. |
| 9 | `Pair` stays as DTO with newtypes: `Pair { file: TagrPath, tags: Vec<TagName> }` | **Decided: yes** |
| 10 | `QueryCriteria` composition: `SavedFilter { name, description, criteria }` wraps it | **Decided: yes** |
| 11 | `TagStore::query()` is required, no default impl | **Decided: yes** |
| 12 | Schema lifecycle: load once, reload on mutation/config change, `Arc<TagSchema>` | **Decided: yes** — CLI loads once, TUI swaps Arc after alias edits, daemon uses ArcSwap. No per-query `load_default_schema()`. |
| 13 | Tag expression tree (`TagExpr`) in `QueryCriteria` | **Decided: yes** — design with `TagExpr` (And/Or/Not/Tag) now, implement only flat subset initially. Enables future `-Q "(A & (B | C) & !D)"` syntax. |
| 14 | DaemonStore caching: widest-result cache, local filtering on narrow, IPC on widen | **Decided: yes** — ServerEvent diffs update cache incrementally. Only `ConfigReloaded` forces full invalidation. |
