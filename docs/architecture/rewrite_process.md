# Rewrite Process Log

> Living document tracking Phase 1–6 of the tagr v1.0.0 architecture rewrite.
> Updated at the end of each phase.

## Documents

| Doc | Purpose |
|-----|---------|
| [`overview.md`](overview.md) | Target architecture — layer diagram, daemon internals, wire protocol, caching, error types, testing priorities |
| [`migration-plan.md`](migration-plan.md) | 6-phase plan, type designs, borrowing strategy, decision log (14 decisions) |
| [`interface-map.md`](interface-map.md) | Current state audit — type inventory, conversion map, coupling problems |
| **This file** | What actually happened — per-phase log of work done, decisions made, numbers |

---

## Phase 1: Core Types ✅

**Branch:** `refactor/v1.0`
**Commit:** `28a544a` — `feat(types): add Layer 0 core types`
**Date:** 2026-05-27

### What shipped

| File | Lines | Purpose |
|------|-------|---------|
| `src/types/mod.rs` | 59 | Module root, re-exports, constants (`HIERARCHY_DELIMITER`, `RESERVED_VTAG_PREFIXES`) |
| `src/types/tag_name.rs` | 256 | `TagName` — validated tag identifier with hierarchy methods |
| `src/types/tagr_path.rs` | 106 | `TagrPath` — UTF-8-validated file path |
| `src/types/filter_name.rs` | 106 | `FilterName` — validated saved-filter identifier |
| `src/types/query.rs` | 395 | `QueryCriteria`, `TagExpr`, `MatchMode` |
| `src/types/pair.rs` | 39 | `Pair { file: TagrPath, tags: Vec<TagName> }` |
| `src/types/error.rs` | 80 | `ValidationError` enum (Clone + PartialEq + Eq) |
| `src/types/tests.rs` | 737 | 107 tests |
| **Total** | **1,778** | |

### Types introduced

- **`TagName`** — `new()` validates: non-empty, max 128 chars, alphanumeric + `-_.:`,
  no leading/trailing `:`, no `::`, no reserved vtag prefixes. Hierarchy methods:
  `depth()`, `root()`, `parent()`, `segments()`, `join()`, `is_child_of()`,
  `matches_pattern()`, `matches_glob()`. Implements `AsRef<str>`, `Borrow<str>`,
  `Display`, `Ord`, `Serialize`/`Deserialize`. No `Deref<Target=str>`.

- **`TagrPath`** — `new()` validates UTF-8 only. Stored as-given (CLI canonicalizes
  before construction). `from_string()` for pre-validated strings. Implements
  `AsRef<str>`, `AsRef<Path>`, `Borrow<str>`, `Display`.

- **`FilterName`** — `new()` validates: non-empty, max 64 chars, alphanumeric + `-_`.

- **`TagExpr`** — `Tag(TagName) | Not(Box<Self>) | And(Vec<Self>) | Or(Vec<Self>)`.
  `matches(&[TagName])` evaluates with hierarchy-aware prefix matching.

- **`QueryCriteria`** — unified search params with `tag_expr`, `file_patterns`,
  `virtual_tags`, `query`. Toggle methods for flat include/exclude, mode switching.
  `matches_pair()` for DaemonStore local cache filtering. `to_cli_string()` for
  TUI status bar.

- **`MatchMode`** — `All | Any`, replacing 5 duplicate enums.

- **`Pair`** — `{ file: TagrPath, tags: Vec<TagName> }`, replacing old `Pair` with
  raw `PathBuf` and `Vec<String>`.

- **`ValidationError`** — `Empty`, `TooLong`, `InvalidChar`, `LeadingOrTrailingDelimiter`,
  `EmptySegment`, `ReservedVtagPrefix`, `InvalidUtf8`. All variants include `NameKind`
  (Tag or Filter) for clear error messages.

### Test coverage

107 tests across 10 test modules:
- `tag_name_validation` (13) — every `ValidationError` variant, boundary cases, reserved prefixes
- `tag_name_hierarchy` (9) — depth, root, parent, segments, join, is_child_of, matches_pattern, ord
- `tag_name_glob` (6) — `*` single segment, `**` multi-segment, exact, no-match
- `tagr_path_tests` (4) — construction, from_string, display, into_path_buf
- `filter_name_tests` (4) — valid, empty, too long, special chars
- `match_mode_tests` (2) — default, display
- `tag_expr_tests` (5) — single tag, hierarchy, not, and, or, complex nested
- `query_criteria_tests` (8) — empty, toggle include/exclude, mixed, mode toggle, complex
- `matches_pair_tests` (6) — empty criteria, tag inclusion/exclusion, hierarchy, glob, combined
- `serde_tests` (4) — JSON round-trip for TagName, TagrPath, QueryCriteria, Pair
- `pair_tests` (1) — construction

### Clippy status

Zero warnings from `clippy::pedantic + clippy::nursery` in `src/types/`.
One `#[allow]`: `missing_const_for_fn` on `is_empty()` — `Vec::is_empty()` is not const-stable.

### Design notes

- **No bridges yet.** Old types (`Pair` in `lib.rs`, `SearchParams` in `cli.rs`,
  etc.) are untouched. Phase 1 only adds `src/types/` alongside existing code.
  The `From<&SearchParams> for QueryCriteria` bridges are deferred to Phase 2–3
  when consumers start using the new types.

- **`toggle_exclude_tag` takes `&TagName`** instead of owned `TagName` (clippy
  `needless_pass_by_value`). The tag is only cloned when actually inserted into
  the expression — no allocation on removal.

- **`collapse_tag_expr` helper** avoids double-mutable-borrow when removing
  from an And/Or vec and then collapsing. Re-matches after removal rather than
  taking `&mut Vec` from inside `&mut Option`.

- **`matches_glob` uses recursive segment matching.** `*` = one segment,
  `**` = one or more. Simple stack-based recursion, no regex compilation.

- **Reserved vtag prefixes**: `modified`, `created`, `accessed`, `size`, `ext`,
  `ext-type`, `dir`, `path`, `depth`, `perm`, `lines`, `git`. Derived from
  the vtag parser's match arms.

### Verification

```
cargo build        ✅  clean
cargo test         ✅  719 pass (107 new + 612 existing)
cargo clippy       ✅  0 warnings in src/types/
```

---

## Phase 2: TagStore Trait ✅

**Branch:** `refactor/v1.0`
**Commit:** `c3bd19f` — `feat(store): add TagStore trait, DirectStore, MockStore`
**Date:** 2026-05-27

### What shipped

| File | Lines | Purpose |
|------|-------|---------|
| `src/store/mod.rs` | 295 | `TagStore` trait (20 methods), `StoreError` (6 variants), re-exports |
| `src/store/direct.rs` | 374 | `DirectStore` — sled bridge, `DbError` → `StoreError` mapping |
| `src/store/mock.rs` | 311 | `MockStore` — `HashMap`-based, uses `matches_pair()` for `query()` |
| `src/store/tests.rs` | 533 | 38 tests (MockStore unit + DirectStore integration) |
| `src/types/note.rs` | 65 | `NoteRecord` + `NoteMeta` moved from `db/types.rs` to Layer 0 |
| `src/types/tag_name.rs` | +16 | `TryFrom<&str>` + `TryFrom<String>` for `TagName` |
| `src/db/mod.rs` | +41 | `list_tags_with_counts()` + `find_tags_by_prefix()` helpers |
| `src/db/types.rs` | -59/+8 | Re-exports `NoteRecord`/`NoteMeta` from `types/` for backward compat |
| **Total** | **~1,646** | (1,578 new + 68 net in existing files) |

### Types introduced

- **`TagStore`** — `trait TagStore: Send + Sync`, 20 methods, all `&self`.
  DaemonStore-ready: interior mutability via `RwLock`, `ConnectionLost` error
  variant for IPC failures. `query()` is required (no default impl, Decision #11).

- **`StoreError`** — 6 domain variants: `FileNotFound(TagrPath)`,
  `TagNotFound(TagName)`, `DatabaseLocked`, `StorageCorrupted { key, reason }`,
  `IoFailed { context, source }`, `ConnectionLost { context }`. No `#[from]`
  on third-party types — only `DirectStore` maps sled errors.

- **`DirectStore`** — wraps `Database`, converts `String` ↔ `TagName` and
  `PathBuf` ↔ `TagrPath` at the boundary. `inner()` escape hatch for
  unmigrated code (removed after Phase 5). `query()` handles flat-tag subset
  (full pipeline deferred to Phase 3).

- **`MockStore`** — `HashMap<TagrPath, Vec<TagName>>` + notes map. Constructors:
  `new()`, `with_pairs(Vec<Pair>)`, `with_tags(&[(&str, &[&str])])`.
  `query()` uses `QueryCriteria::matches_pair()` — same local filter function
  `DaemonStore` will use for its narrow-path cache.

### Database helpers added

- `list_tags_with_counts()` — iterates reverse tag index, deserializes file
  lists to count entries. Sorted by tag name.
- `find_tags_by_prefix()` — sled `scan_prefix()` on tag tree, O(log n + k).
  Powers hierarchy expansion, autocomplete, alias resolution.

### NoteRecord migration

Moved `NoteRecord` + `NoteMeta` from `src/db/types.rs` to `src/types/note.rs`
(Layer 0). Kept `bincode::Encode`/`Decode` derives (will be removed when
bincode → postcard migration happens). `db/types.rs` re-exports both types
for backward compatibility — existing `use crate::db::NoteRecord` still works.

### Test coverage

38 new tests across 2 test modules:

**MockStore (20):**
- `empty_store`, `with_tags_constructor`, `with_pairs_constructor`
- `list_all_tags_sorted`, `list_tags_with_counts`, `list_all_files_sorted`, `list_all_pairs`
- `get_tags_existing_file`, `get_tags_missing_file`
- `find_by_tag`, `find_by_all_tags`, `find_by_any_tag`, `find_by_tag_regex`
- `tag_exists`, `find_tags_by_prefix`
- `query_empty_criteria_returns_all`, `query_single_tag`, `query_and_tags`,
  `query_or_tags`, `query_not_tag`, `query_file_pattern`
- `store_error_file_not_found`, `store_error_connection_lost`

**DirectStore (16):**
- `insert_and_get_tags`, `get_tags_missing_file`
- `list_all_tags`, `list_tags_with_counts`
- `find_by_tag`, `find_by_all_tags`, `add_and_remove_tags`
- `remove_file`, `remove_tag_globally`
- `notes_crud`, `list_all_notes`, `find_tags_by_prefix`
- `query_empty_criteria`, `query_single_tag`, `query_with_not`

### Clippy status

Zero warnings from `clippy::pedantic + clippy::nursery` in `src/store/` and `src/types/note.rs`.
One `#[allow]`: `module_name_repetitions` on `StoreError` — `store::StoreError` reads
more clearly than `store::Error` given the multi-module error architecture.

### Design notes

- **Boundary conversion helpers** — `path_to_tagrpath()` and `string_to_tagname()`
  map database corruption (invalid UTF-8 paths, invalid tag names in stored data)
  to `StoreError::StorageCorrupted`. These are not validation errors (data was
  validated on insert) — they signal database corruption.

- **`query()` flat-tag subset** — DirectStore extracts leaf `Tag` nodes from
  `TagExpr`, routes to `find_by_all_tags`/`find_by_any_tag`. `Not` nodes trigger
  a post-filter pass via `matches_pair()`. Full pipeline (hierarchy expansion,
  file patterns, vtags, regex tags) deferred to Phase 3.

- **MockStore mutations return errors** — `insert()`, `add_tags()`, etc. return
  `StoreError::IoFailed` because `TagStore` methods are `&self` and `MockStore`
  doesn't use interior mutability. Test state is set up via constructors.
  This mirrors the real architecture: stores are configured at creation, mutations
  go through `&self` with internal thread-safety.

- **`TryFrom` on `TagName`** — added `TryFrom<&str>` and `TryFrom<String>`
  for idiomatic conversions alongside `TagName::new()`.

### Verification

```
cargo build        ✅  clean
cargo test         ✅  761 pass (38 new + 723 existing)
cargo clippy       ✅  0 warnings in src/store/, src/types/note.rs
```

---

## Phase 3: Query Engine ✅

**Status:** Complete

**Goal:** Single query pipeline in `src/query/`. Move logic from `db/query.rs`
and `search/`. Delete `src/search/` module entirely.

### Changes

**Created:**
- `src/query/mod.rs` — Full pipeline: `execute()`, `expand_tags()`, tag expr
  evaluation, hierarchy expansion, regex, vtag filtering (~490 lines)
- `src/query/hierarchy.rs` — Moved from `search/hierarchy.rs`, generified with
  `impl AsRef<str>` for `TagName`/`String` flexibility
- `src/query/patterns.rs` — Moved from `search/filter.rs`, simplified to pure
  `filter_by_patterns()` function (removed `PathFilterExt`/`PathTagFilterExt` traits)
- `src/query/error.rs` — Moved `SearchError` from `search/error.rs`
- `src/query/tests.rs` — 12 `MockStore`-based tests covering empty criteria,
  single tag, AND/OR, NOT, file patterns, regex, hierarchy, combined criteria

**Modified:**
- `src/store/direct.rs` — `query()` delegates to `crate::query::execute()` (one-liner)
- `src/db/query.rs` — Rewritten as thin bridge: `SearchParams → QueryCriteria →
  query::execute()` via `DirectStore`. All 6+ existing callers unchanged.
- `src/browse/query.rs` — `filter_items_in_memory()` rewritten with inline
  hierarchy-aware filtering (replaced `FilterExt` trait)
- `src/browse/models.rs` — Removed `AsFileTagPair` impl for `TagrItem`
- `src/daemon/core.rs` — Updated `expand_tags` import + wrapped `Database` in
  `DirectStore` for `&dyn TagStore` compatibility
- `src/lib.rs` — Removed `pub mod search;`, removed `AsFileTagPair` impl for
  `Pair`, updated `TagrError::SearchError` to use `query::SearchError`

**Deleted:**
- `src/search/` — All 6 files (mod.rs, hierarchy.rs, filter.rs, traits.rs,
  error.rs, error_tests.rs)

### Key Decisions

1. **`AsRef<str>` ambiguity**: `TagrPath` implements both `AsRef<str>` and
   `AsRef<Path>`, making `.as_ref()` calls ambiguous. Rule: always use
   `.as_str()` or `.as_path()` explicitly, never `.as_ref()` on newtypes.

2. **`TagName` validation vs regex patterns**: `TagName::new()` rejects regex
   metacharacters (`*`, `.`). When `regex_tags = true`, patterns go through the
   `query` free-text field, not `TagExpr::Tag`.

3. **Hierarchy expansion vs prefix matching**: `expand_tags()` uses
   `schema.expand_synonyms()` for synonym resolution only; hierarchy prefix
   matching is handled by `hierarchy::pattern_matches()` at filter time.
   This avoids AND-mode breakage where expanding "lang" to all children would
   require matching ALL children.

4. **Bridge pattern**: `db/query.rs::apply_search_params()` converts
   `SearchParams → QueryCriteria` and delegates to `query::execute()`.
   Temporary — deleted when `SearchParams` is removed in Phase 5/6.

### Verification

- `cargo build` — clean (no warnings)
- `cargo test` — 735 tests pass (643 unit + 59 integration + 33 doc)
- `cargo clippy -- -W clippy::pedantic -W clippy::nursery` — zero warnings in
  `src/query/`, `src/db/query.rs`, `src/browse/query.rs`

---

## Phase 4: DirectStore + DaemonStore

**Status:** Not started

**Goal:** Split `DataSource` enum into `DirectStore` + `DaemonStore` trait impls.
Delete `datasource.rs`.

---

## Phase 5: Adopt Newtypes Throughout

**Status:** Not started

**Goal:** Replace raw `String`/`PathBuf` with `TagName`/`TagrPath` at all
boundaries. Delete bridge conversions.

---

## Phase 6: Delete Old Types & Cleanup

**Status:** Not started

**Goal:** Remove `SearchParams`, `FilterCriteria`, `ActiveFilter`,
`WireSearchParams`, old `Pair`, `DataSource`, `PathString`. Merge confused
modules. Final cleanup.
