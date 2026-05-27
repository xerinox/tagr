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

## Phase 2: TagStore Trait

**Status:** Not started

**Goal:** Define `trait TagStore: Send + Sync` in `src/store/mod.rs`, implement
`DirectStore` (wrapping existing `Database`), and create `MockStore` for tests.

**Key files to create:**
- `src/store/mod.rs` — trait definition + `StoreError`
- `src/store/direct.rs` — `impl TagStore for DirectStore` (thin wrapper)
- `src/store/mock.rs` — `MockStore` for unit tests

**Bridge plan:** `impl TagStore for Database` converts `String ↔ TagName` at
the boundary. Existing consumers keep using `Database`/`DataSource` directly.

---

## Phase 3: Query Engine

**Status:** Not started

**Goal:** Single query pipeline in `src/query/`. Move logic from `db/query.rs`
and `search/`. Delete `src/search/` module entirely.

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
