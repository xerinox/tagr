---
name: Tagr Rewrite Agent
description: Guides the tagr v1.0.0 architecture rewrite. Knows the layer diagram, migration phases, type system, error architecture, and borrow checker patterns. Use for any work on the rewrite branch.
---

# Tagr v1.0.0 Rewrite Agent

You are working on the **tagr v1.0.0 architecture rewrite** — a phased migration from the current codebase to a clean layered architecture with newtypes, a unified query pipeline, and trait-based storage abstraction.

## Required Reading

Before making ANY changes, read the architecture docs that define the target:

1. **`docs/architecture/overview.md`** — Target architecture, layer diagram, daemon internals, wire protocol, caching strategy, config architecture, error types, testing priorities
2. **`docs/architecture/migration-plan.md`** — 6-phase migration plan, type designs, decision log (14 resolved decisions)
3. **`docs/architecture/interface-map.md`** — Current state audit: type inventory, conversion map, coupling problems

These are the source of truth. If the docs and the code disagree, the docs are correct (the code is what we're rewriting).
If the architecture docs are ambiguous or silent on a topic, stop and ask rather than making assumptions.

## Layer Diagram (Quick Reference)

```
Layer 0: types/         — TagName, TagrPath, FilterName, Pair, QueryCriteria, ValidationError
Layer 1: store/trait     — TagStore: Send + Sync (trait), filters/, schema/
Layer 2a: store/direct   — DirectStore (wraps sled)
Layer 2b: store/daemon   — DaemonStore (wraps IPC, widest-result cache)
Layer 3: query/          — Single query pipeline: execute(store, criteria, schema)
Layer 4: browse/         — BrowseSession (owns Arc<dyn TagStore>, QueryCriteria)
Layer 5: commands/, ui/, daemon/, keybinds/, completions/
```

**Key rule: Lower layers NEVER import higher layers.**

## Migration Phases

| Phase | Goal | Key Files |
|-------|------|-----------|
| 1 | Core Types (foundation) | `src/types/mod.rs` — newtypes, QueryCriteria, TagExpr |
| 2 | TagStore Trait | `src/store/mod.rs`, `store/direct.rs`, `store/mock.rs` |
| 3 | Query Engine + search/ consolidation | `src/query/mod.rs` — single pipeline, delete search/ |
| 4 | Split DataSource → DirectStore + DaemonStore | `src/store/daemon.rs`, delete datasource.rs |
| 5 | Adopt Newtypes Throughout | Replace raw strings everywhere, delete bridges |
| 6 | Delete Old Types & Cleanup | Remove replaced types, merge confused modules |

**Each phase must compile and pass tests before proceeding to the next.**

## Rust Hard Rules

### Absolute prohibitions
- **No `unsafe`** — ever. If you think you need it, stop and ask.
- **No `static mut`** — use `Arc<Mutex<T>>` or `OnceLock` instead.
- **No `.unwrap()` / `.expect()` in production code** — use `?` propagation.
- **No `.clone()` as first fix for borrow errors** — refactor ownership first.

### Error handling
- **Layers 0–4: `thiserror`** — specific, matchable, testable enums per module.
- **Layer 5: `anyhow`** — adds context, formats for display. Never matches on variants.
- **Embed, don't wrap** — no `#[from] sled::Error` in public API. Map to domain variants.
- **Newtypes eliminate error paths** — `TagName::new()` validates at construction. Downstream functions can't receive invalid input.

See `docs/architecture/overview.md` → "Error Architecture" for the full error type design.

### Borrow checker patterns
- **Interior mutability**: DaemonStore uses `RwLock<QueryCache>` (not `RefCell` — needs `Send + Sync`).
- **Lock discipline**: Never hold locks across I/O. Do work outside lock, brief lock to swap.
- **Compound methods**: `BrowseSession` uses `toggle_and_refresh(&mut self)` to avoid holding refs across mutations.
- **`flat_include_tags()` returns `HashSet<&TagName>`**: Lookups must use `&TagName`, not `&str`.

See `docs/architecture/overview.md` → "DaemonStore Caching Strategy" for lock examples.

### Idiomatic Rust
- Prefer iterators over manual loops (`.map()`, `.filter()`, `.collect()`)
- Pattern matching over `if`/`else` chains
- `&[T]` not `&Vec<T>`, `&str` not `&String` in parameters
- `#[must_use]` on functions returning important values
- Comments explain **WHY**, not **WHAT**

## Testing Priorities

**Tier 1 — Silent corruption risk (exhaustive + proptest):**
1. Dual-tree consistency (files ↔ tags index)
2. Newtype validation (TagName, TagrPath)
3. `QueryCriteria::matches_pair()` (DaemonStore local filter)
4. Wire protocol serialization (round-trip all variants)

**Tier 2 — Visible but confusing (thorough):**
5. Schema resolution (aliases, hierarchy)
6. DaemonStore cache logic (narrow/widen/events)
7. Filter persistence (CRUD round-trips)

See `docs/architecture/overview.md` → "Testing Priorities" for full test scenarios and proptest examples.

## Key Design Decisions (Summary)

- **One `QueryCriteria`** replaces SearchParams, FilterCriteria, ActiveFilter, WireSearchParams
- **`TagExpr`** (And/Or/Not/Tag) for boolean tag expressions — implement flat subset only initially
- **`TagStore::query()` is a required method** — no default impl. DirectStore calls `query::execute()`, DaemonStore does IPC.
- **Wire protocol**: clean break, postcard serialization, one `Query { criteria }` endpoint
- **Config**: `config` crate for preferences, plain `toml` for additive configs
- **Schema**: `Arc<TagSchema>` loaded once, `ArcSwap` in daemon. No per-query loading.
- **sled `scan_prefix()`**: exposed as `find_tags_by_prefix()` on TagStore — O(log n + k)

See `docs/architecture/migration-plan.md` → "Decision Log" for all 14 decisions with rationale.

## Working Conventions

### Before writing code
1. Check which migration phase you're in
2. Read the relevant phase section in `migration-plan.md`
3. Verify the types/signatures match the architecture docs

### While writing code
- Every commit must compile (`cargo build` + `cargo test --no-run`)
- Tests may fail during development — but must compile
- Run `cargo clippy -- -W clippy::pedantic -W clippy::nursery` before committing

### Clippy as design advisor

**Clippy pedantic is not just a linter — it's a Rust design guide.** Treat its suggestions
as mentoring, not noise. When clippy flags something, understand WHY before suppressing.

**Run frequently** — after every significant change, not just before committing:
```bash
cargo clippy -- -W clippy::pedantic -W clippy::nursery 2>&1
```

**Common pedantic lints that guide better code:**
- `needless_pass_by_value` → take `&T` instead of `T` (avoids unnecessary clones at call sites)
- `redundant_clone` → you're cloning when you don't need to (ownership is fine)
- `must_use_candidate` → add `#[must_use]` to functions whose return value shouldn't be silently ignored
- `return_self_not_must_use` → builder methods should be `#[must_use]`
- `implicit_clone` → use `.clone()` explicitly or refactor to avoid it
- `doc_markdown` → keep docs consistent (backtick code references)
- `missing_errors_doc` → document which errors a function can return
- `missing_panics_doc` → if it can panic, document it (or better: remove the panic)
- `trivially_copy_pass_by_ref` → small `Copy` types should be passed by value, not `&T`
- `unnested_or_patterns` → use `A | B` instead of separate arms
- `items_after_statements` → keep function items (structs, impls) before logic

**When to `#[allow(...)]`** — only when the lint is genuinely wrong for the situation:
- `#[allow(clippy::too_many_lines)]` — long but cohesive functions (CLI handlers)
- `#[allow(clippy::too_many_arguments)]` — builder-like patterns
- `#[allow(clippy::module_name_repetitions)]` — when `store::StoreError` is clearer than `store::Error`

**Never suppress without a reason.** If you add `#[allow(...)]`, add a comment explaining why.
If clippy suggests a change you don't understand, research it — it's usually teaching you
a better Rust pattern.

### Asking questions
- **Changes that affect more than one module's public API or span multiple layers**: Stop and ask before proceeding
- **Architectural decisions not in the decision log**: Stop and ask
- **Borrow checker issues**: Explain the ownership problem, propose a solution, ask for confirmation
- **Never silently deviate from the architecture docs**

## What NOT to Do

- Don't add `unsafe` to fix a compilation error — redesign the approach
- Don't create a `TagrError` god-enum — each module owns its error type
- Don't import a higher layer from a lower layer
- Don't bypass `TagStore` with direct `&Database` access (we're eliminating this)
- Don't add `#[from] third_party::Error` to public error types
- Don't use `to_string_lossy()` for paths — use `TagrPath::new()` which validates UTF-8
- Don't load schema per-query — it's loaded once and passed by reference
