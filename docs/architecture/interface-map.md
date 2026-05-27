# Interface Map — Current State Audit

> Complete inventory of types, conversions, and functionality locations.

## 1. Type Inventory

### Types Representing "Search/Filter Criteria"

| Type | Location | Role | Serializable |
|------|----------|------|-------------|
| `SearchCriteriaArgs` | `cli.rs:887` | Clap derive struct — raw CLI flags | No |
| `SearchParams` | `cli.rs:83` | Canonical in-memory query DTO | Yes (serde) |
| `FilterCriteria` | `filters/types.rs:34` | TOML-stored saved filters | Yes (serde) |
| `ActiveFilter` | `browse/filter.rs:16` | TUI runtime wrapper around FilterCriteria | No |
| `WireSearchParams` | `ipc/wire.rs:138` | IPC serialization of SearchParams | Yes (serde) |
| `RefineSearchCriteria` | `ui/traits.rs:8` | Input config for refine search overlay | No |
| `RefinedSearchCriteria` | `ui/types.rs:128` | Output from refine search overlay | No |

**Target**: `QueryCriteria` (in `types/`) replaces SearchParams, ActiveFilter, WireSearchParams
as the "what to search for" type. Toggle methods (include/exclude) live on `QueryCriteria`.
`SavedFilter { name: FilterName, description: String, criteria: QueryCriteria }` (in `filters/`)
replaces `FilterCriteria` + `Filter` — metadata stays separate from search params.
RefineSearchCriteria and RefinedSearchCriteria merge into one.

### Duplicate Enums (same 2 variants, different names)

| Enum | Location | Variants |
|------|----------|----------|
| `SearchMode` | `cli.rs:72` | All, Any |
| `SearchMode` | `browse/models.rs:132` | All, Any |
| `TagMode` | `filters/types.rs:298` | All, Any |
| `FileMode` | `filters/types.rs:327` | All, Any |
| `WireSearchMode` | `ipc/wire.rs:155` | All, Any |

**Target**: One `MatchMode` enum.

### Duplicate Type Names

| Type | Locations |
|------|-----------|
| `PathFormat` | `cli.rs:54`, `config/mod.rs:22`, `browse/session.rs:100` |
| `BrowseError` | `browse/ui.rs:705`, `browse/session.rs:54` |
| `SearchMode` | `cli.rs:72`, `browse/models.rs:132` |

### Existing Newtypes

| Type | Location | Used in |
|------|----------|---------|
| `PathKey(PathBuf)` | `db/types.rs:39` | `db/mod.rs` only (4 sites) |
| `PathString(String)` | `db/types.rs:82` | `db/mod.rs` only (2 sites) |

Both are confined to the db layer. No newtypes exist for tag names or filter names.

### Raw String Usage (should be newtypes)

**Tag names as raw strings (~25 functions):**
- `db`: `find_by_tag(&str)`, `find_by_all_tags(&[String])`, `find_by_any_tag(&[String])`,
  `tag_exists(&str)`, `list_all_tags() → Vec<String>`, `remove_tag_globally(&str)`,
  `insert(_, Vec<String>)`, `add_tags(_, Vec<String>)`, `get_tags(_) → Option<Vec<String>>`
- `datasource`: mirrors all db methods with same string types
- `filters`: `FilterCriteria.tags: Vec<String>`, `.excludes: Vec<String>`
- `ui`: `ItemMetadata.tags: Vec<String>`, `TagTreeState.selected_tags: HashSet<String>`
- `browse`: `ActiveFilter` toggle methods take `String`

**File paths as strings:**
- IPC wire types use `String` for all paths (wincode doesn't support PathBuf)
- `datasource.rs` converts `Path → String` via `to_string_lossy()` at IPC boundary
- `db/query.rs:139` uses `Vec<(String, Vec<String>)>` for files-with-tags

**Filter names as strings:**
- `Filter.name: String`, `FilterManager.get(&str)`, `validate_filter_name(&str)`

---

## 2. Conversion Map (From/TryFrom)

### Search Criteria Conversions

```
SearchCriteriaArgs ──From(&)──▶ SearchParams           cli.rs:266
SearchParams ──From──────────▶ FilterCriteria          cli.rs:209  ⚠️ LOSSY (drops query, no_hierarchy)
SearchParams ──From(&)────────▶ FilterCriteria          cli.rs:230  ⚠️ LOSSY
FilterCriteria ──From(&)──────▶ SearchParams            cli.rs:247  ⚠️ LOSSY (sets query=None)
SearchParams ──From──────────▶ ActiveFilter             browse/filter.rs:291 (via FilterCriteria)
SearchParams ──From(&)────────▶ ActiveFilter             browse/filter.rs:299
ActiveFilter ──From(&)────────▶ SearchParams             browse/filter.rs:308 (via FilterCriteria)
SearchParams ──From(&)────────▶ WireSearchParams         ipc/wire.rs:164
WireSearchParams ──From(&)────▶ SearchParams             ipc/wire.rs:183
SearchParams ──From──────────▶ WireSearchParams         ipc/wire.rs:269
WireSearchParams ──From────────▶ SearchParams             ipc/wire.rs:288
```

**⚠️ Lossy conversions use `From` instead of named methods.** `SearchParams ↔ FilterCriteria`
drops `query`, `no_hierarchy`, and `glob_files`. This violates the `From` contract (should be
lossless) and causes bugs when round-tripping.

### Mode Enum Conversions

```
SearchMode (cli) ◄──From──▶ TagMode        filters/types.rs:306,315
SearchMode (cli) ◄──From──▶ FileMode       filters/types.rs:335,344
SearchMode (cli) ◄──From──▶ WireSearchMode ipc/wire.rs:202,211
SearchMode (cli) ◄──From──▶ SearchMode (browse)  browse/query.rs:249,258
```

All are trivially `All↔All, Any↔Any`. Pure boilerplate from having 5 identical enums.

### Wire Type Conversions

```
Pair ◄──From──▶ WireFilePair       ipc/wire.rs:220-244
TagInfo ◄──From──▶ WireTagInfo     ipc/wire.rs:250-259
```

### Display Conversions

```
TagrItem ──From(&)──▶ DisplayItem            browse/models.rs:620  (basic, no context)
TagrItem ──From──────▶ DisplayItem            browse/models.rs:627  (delegates to &)
config::PreviewConfig ──From──▶ ui::PreviewConfig   ui/traits.rs:170
config::PathFormat ──From──▶ browse::PathFormat     commands/browse.rs:20
```

### TryFrom Conversions

```
PathBuf ──TryFrom──▶ PathString    db/types.rs:84
&Path ──TryFrom──▶ PathString      db/types.rs:94
PathKey ──TryFrom──▶ Vec<u8>       db/types.rs:41
&str ──TryFrom──▶ VirtualTag       vtags/parser.rs:24
&str ──TryFrom──▶ TimeCondition    vtags/types.rs:36
&str ──TryFrom──▶ ExtTypeCategory  vtags/types.rs:142
&str ──TryFrom──▶ RangeCondition   vtags/types.rs:165
&str ──TryFrom──▶ PermissionCondition vtags/types.rs:208
&str ──TryFrom──▶ GitCondition     vtags/types.rs:234
```

---

## 3. Functionality Locations

### Pattern: From<T> — lossless, context-free conversions
| Conversion | Location |
|-----------|----------|
| `TagrItem → DisplayItem` | `browse/models.rs:620` |
| `SearchCriteriaArgs → SearchParams` | `cli.rs:266` |
| `config types → ui types` | `ui/traits.rs:170` |
| Wire types ↔ domain types | `ipc/wire.rs` |

### Pattern: Named methods — context-dependent or non-standard conversions
| Method | Location | Why not From |
|--------|----------|-------------|
| `TagrItem::to_display_item_detailed()` | `browse/models.rs:638` | One of multiple possible formats |
| `BrowseController::format_for_display()` | `browse/ui.rs:404` | Needs session, schema, phase |
| `ActiveFilter::from_saved()` | `browse/filter.rs:49` | Named for clarity |
| `ActiveFilter::from_criteria()` | `browse/filter.rs:43` | Named for clarity |

### Pattern: Free functions — business logic on data
| Function | Location | Role |
|----------|----------|------|
| `query::apply_search_params(db, params)` | `db/query.rs:53` | Core query pipeline |
| `search::expand_tags(tags, schema, db)` | `search/mod.rs:62` | Tag expansion |
| `hierarchy::filter_by_hierarchy(files, inc, exc)` | `search/hierarchy.rs:244` | Specificity rules |
| `hierarchy::should_include_file(tags, inc, exc)` | `search/hierarchy.rs:172` | Per-file check |
| `hierarchy::pattern_matches(pattern, tag)` | `search/hierarchy.rs:74` | Prefix matching |
| `hierarchy::tag_depth(tag)` | `search/hierarchy.rs:39` | Hierarchy depth |
| `hierarchy::hierarchy_root(tag)` | `search/hierarchy.rs:55` | Root extraction |
| `filter::by_patterns(files, patterns, regex, all)` | `search/filter.rs:42` | Glob/regex filtering |
| `browse::query::get_tags_with_counts(ds)` | `browse/query.rs` | Data retrieval |
| `browse::query::get_notes_only_files(ds)` | `browse/query.rs` | Notes-only virtual tag |

### Pattern: Struct/trait methods — stateful operations
| Method | Location | Role |
|--------|----------|------|
| `ActiveFilter::toggle_include_tag()` | `browse/filter.rs:98` | Mutate filter state |
| `ActiveFilter::toggle_exclude_tag()` | `browse/filter.rs:115` | Mutate filter state |
| `ActiveFilter::merge()` | `browse/filter.rs:190` | Combine criteria |
| `FilterManager::create/get/delete()` | `filters/operations.rs` | Saved filter CRUD |
| `AppState::update_file_preview()` | `ui/ratatui_adapter/state.rs:759` | Re-query + update display |
| `AppState::toggle_tag_filter()` | `state.rs:1457` | Toggle with child propagation |
| `AppState::sync_tag_tree_from_filter()` | `state.rs:980` | Sync UI ← filter |
| `AppState::sync_filter_from_tag_tree()` | `state.rs:997` | Sync filter ← UI |
| `TagTreeState::get_all_descendant_tags()` | `tag_tree.rs:459` | Tree traversal (UI) |
| `TagTreeState::build_from_tags()` | `tag_tree.rs:188` | Build tree structure |
| `DataSource::find_by_tag()` etc. | `datasource.rs` | Dispatch Direct/Remote |
| `Database::find_by_tag()` etc. | `db/mod.rs` | Sled queries |

---

## 4. DisplayItem Construction Sites

| Site | Method | Context needed | Domain model |
|------|--------|----------------|-------------|
| `browse/models.rs:620` | `From<&TagrItem>` | None | TagrItem → DisplayItem |
| `browse/models.rs:638` | `.to_display_item_detailed()` | None | TagrItem → DisplayItem (with count) |
| `browse/ui.rs:409` | `format_for_display()` | Session, schema, phase | TagrItem → DisplayItem (rich) |
| `state.rs:863` | `DisplayItem::new()` inline | Path string + note cache | **No domain model** — raw string |

**Problem:** `state.rs:863` constructs DisplayItems from raw path strings, bypassing
the TagrItem domain model entirely. The flow is:

```
Current:   selected_tags → inline DB queries → Vec<String> → DisplayItem::new()
Should be: selected_tags → QueryCriteria → query engine → Vec<TagrPath> → DisplayItem
```

---

## 5. Duplicated Logic

### Query Pipeline (two separate implementations)

| Feature | `apply_search_params` (db/query.rs) | `update_file_preview` (state.rs) |
|---------|-------------------------------------|----------------------------------|
| Tag mode (ANY/ALL) | Configurable | Hardcoded ANY |
| Hierarchy expansion | `expand_tags()` — full | `expand_synonyms()` — synonyms only |
| Hierarchy matching | `filter_by_hierarchy()` with specificity | None |
| File patterns | `by_patterns()` glob/regex | Not supported |
| Virtual tags | Evaluated | Not supported |
| Regex tags | `find_by_tag_regex()` | Not supported |
| Tag exclusion | Hierarchical specificity rules | Simple `contains` check |
| Schema loading | Per-query `load_default_schema()` | Cached in `self.tag_schema` |
| Store abstraction | Takes `&Database` directly | Uses `DataSource` methods |

### Hierarchy Logic (three implementations)

| Location | Method | What it does |
|----------|--------|-------------|
| `search::expand_tags()` | DB + schema aware | Full synonym + hierarchy + prefix expansion |
| `state.rs:786` | Inline | `expand_synonyms()` only — no hierarchy |
| `tag_tree.rs:459` | `get_all_descendant_tags()` | Tree-structure based — UI only |

### Merge Semantics (two implementations)

| Type | Method | Behavior |
|------|--------|----------|
| `SearchParams` | `.merge()` at `cli.rs:171` | Overrides modes from `other` |
| `FilterCriteria` | `.merge()` at `filters/types.rs:95` | Preserves receiver's modes |

Callers work around the difference with manual mode-preservation logic
(`commands/search.rs:66-83`).

---

## 6. Inverted Dependencies

| Dependency | Problem |
|-----------|---------|
| `db/query.rs` imports `cli::SearchParams` | Storage layer depends on CLI layer |
| `db/query.rs` imports `cli::SearchMode` | Storage layer depends on CLI layer |
| `search/traits.rs` imports `cli::SearchParams` | Domain logic depends on CLI layer |
| `ui/ratatui_adapter/state.rs` imports `browse::ActiveFilter` | Generic UI adapter knows about browse domain |

**Target:** `SearchParams` and `SearchMode` move to `types/`, breaking the inversion.

---

## 7. DataSource Architecture

### Current: Enum dispatch

```rust
pub enum DataSource {
    Direct(Database),
    Remote { rt: Runtime, client: PersistentClient },
}
```

Every method does `match self { Direct => ..., Remote => ... }`. 15+ methods with identical
match structure.

### Problems
- Not mockable for tests (no trait)
- `as_database() → Option<&Database>` leaks the abstraction
- `commands/search.rs` bypasses DataSource entirely (`db::query::apply_search_params(db, &params)`)
- `commands/bulk/tag_ops.rs` also bypasses DataSource (3 sites)
- `search/filter.rs` uses `&Database` directly for tag exclusion

### Target: Trait

```rust
pub trait TagStore: Send + Sync {
    fn find_by_tag(&self, tag: &TagName) -> Result<Vec<TagrPath>>;
    fn query(&self, criteria: &QueryCriteria, schema: &TagSchema) -> Result<Vec<TagrPath>>;
    // ...
}
```

- `DirectStore` (sled), `DaemonStore` (IPC), `MockStore` (tests)
- All consumers take `&dyn TagStore` or `Arc<dyn TagStore>`
- DaemonStore overrides `query()` for single-round-trip IPC

---

## 8. Error Type Inventory

| Error | Location | Wraps |
|-------|----------|-------|
| `DbError` | `db/error.rs` | sled, postcard (migrating from bincode), InvalidInput, PathError |
| `DataSourceError` | `datasource.rs:16` | DbError, IpcError |
| `SearchError` | `search/error.rs` | DbError, UiError ⚠️ |
| `FilterError` | `filters/error.rs` | IoError, ConfigError, toml |
| `UiError` | `ui/error.rs` | BuildError, InterruptedError, IoError |
| `PreviewError` | `preview/error.rs` | IoError |
| `PatternError` | `patterns/error.rs` | InvalidGlob |
| `SchemaError` | `schema/error.rs` | IoError, toml |
| `IpcError` | `ipc/mod.rs:20` | ConnectionFailed, SerializationError |
| `DaemonError` | `daemon/traits.rs:9` | various |
| `BrowseError` | `browse/session.rs:54` | DataSourceError, config |
| `BrowseError` | `browse/ui.rs:705` | DataSourceError, session ⚠️ duplicate name |
| `TagrError` | `lib.rs:37` | All of the above via `#[from]` |

**Issues:**
- `SearchError` wraps `UiError` — domain error coupled to presentation
- Two `BrowseError` enums with the same name in different modules
- `DataSourceError` may be absorbed into a unified `StoreError`

---

## 9. Module Coupling Summary

### Clean ✅
- `ui/` → `db/`: Zero direct imports (goes through DataSource)
- `commands/` → `ui/`: Zero imports of ui internals
- Daemon architecture: No duplication, clean IPC boundary
- Error conversions: All use `thiserror` `#[from]`, no manual impls

### Problematic 🔴
- `db` imports `cli` (SearchParams, SearchMode) — inverted dependency
- `search/` imports `cli` (SearchParams) — inverted dependency
- `ui/state.rs` imports `browse::ActiveFilter` — generic adapter knows about browse
- `search/filter.rs` takes `&Database` directly — bypasses DataSource
- `commands/search.rs` takes `&Database` — bypasses DataSource
- `commands/bulk/` takes `&Database` — bypasses DataSource (3 sites)

### Confusing 🟡
- `search/` module named "interactive search" but contains pure filtering logic
- `search/` and `filters/` are two filtering modules with overlapping concerns
- `browse/filter.rs` contains `ActiveFilter` (a filter module inside browse)
