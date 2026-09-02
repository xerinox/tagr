# Tagr Handcrafted Rewrite — Overview & Starting Guide

> **Status:** planning document, written before any rewrite work has begun.
> **Audience:** you, six months from now, wondering why you started this.

## The actual goal

This is not a code-quality project. If it were, incremental in-place cleanup would
win: same structural benefit, working binary throughout, no risk of a half-finished
rewrite.

The goal is **reclaiming the codebase** — getting back down into it, understanding it
again, and making the architecture decisions deliberately this time instead of
inheriting them. The better code is a side effect. The understanding is the product.

That has consequences the rest of this document is built around:

- **Interest is the scarce resource, not time.** The plan is sequenced to keep something interesting in front of you, because a stalled rewrite is worse than no rewrite.
- **The ADRs matter more than the code.** See [Architecture decision records](#architecture-decision-records).
- **Hand-craft selectively.** Design-heavy modules get the full treatment; plumbing gets transcribed fast so the budget survives to reach the interesting parts.

### The autogen boundary

Pre-committed, so it isn't a judgement call at 2am in month three.

**Allowed:**
- LLM as rubber duck — reasoning through tradeoffs, poking holes, explaining a crate's API, "what am I missing about sled's flush semantics".
- Autocomplete, 1–3 lines. Finishing a match arm, not writing a function.

**Not allowed:**
- LLM-written functions, types, or modules.
- LLM-written architecture decisions. The subtle one: asking "how should I structure X?" and adopting the answer is the same violation as generating the code, just harder to notice. Ask for *options and tradeoffs*; make the call yourself and write the ADR in your own words.

## Flagged findings — independent of the rewrite

Three things surfaced during the audit that stand on their own. They do not depend on
the rewrite happening, and two of them are actionable today.

### F1 — Dead code with zero production call sites (act now)

**Severity: high · Effort: trivial · Blocks nothing**

- [src/discovery/](../../src/discovery/) — `DiscoveryKind` enum + `FileDiscovery` trait. No implementors, no callers, still `pub mod` in [src/lib.rs](../../src/lib.rs).
- `ActionExecutor` in [src/keybinds/executor.rs](../../src/keybinds/executor.rs) — ~470 lines, referenced only by its own tests. It is the third of three parallel `BrowseAction` dispatch systems.

~500 lines deletable in a single commit. Do this regardless of whether the rewrite
proceeds; it removes a maintenance surface that is currently pure cost.

### F2 — `FuzzyFinder` is an abstraction fighting its only implementor

**Severity: high · Effort: large · Informs the rewrite's core rule**

Two symptoms, both in [src/ui/ratatui_adapter/finder.rs](../../src/ui/ratatui_adapter/finder.rs):

1. `FinderConfig` carries `database: Option<Arc<dyn TagStore>>` — the store is smuggled *back through* the abstraction that was supposed to hide it.
2. `RatatuiFinder` holds a `RefCell` purely because `FuzzyFinder` takes `&self`.

The trait has exactly two implementors: `RatatuiFinder` and a test mock. When an
abstraction forces both a back-channel and interior mutability on its only real
implementor, the abstraction is wrong. This is the strongest single piece of evidence
for the rewrite rule **"extract a trait on the second real implementor, not the
first"** — cited again in Part 3.

### F3 — `overview.md` does not describe the current code

**Severity: medium · Effort: trivial · Correctness of documentation**

[overview.md](overview.md) specifies postcard on the wire, a unified `ConfigPaths`
type, and `anyhow` at layer 5. The code uses `wincode` on the wire, has five separate
config-path helpers, and uses `TagrError` in commands. The repo's own
`copilot-instructions.md` also still claims version 0.4.0 against a `Cargo.toml` at
`1.0.0-alpha.1`.

Anyone — human or agent — reading these as current state will make wrong decisions.
Either annotate them as aspirational or correct them, but do it before more work
builds on them.

## Why this document exists

Tagr today is ~38k lines of Rust across ~95 files, of which roughly 35–40% is test
code. Most of it was produced with heavy AI assistance. It works, it is layered, it
has 700+ passing tests — and it is also full of the specific failure mode that
AI-assisted development produces: **plausible structure without conviction**.
Abstractions exist because an abstraction seemed appropriate, not because a second
implementor ever appeared. Types are duplicated because each generation pass
re-derived them locally. Documentation describes a target architecture that the code
does not implement.

The goal of the rewrite is not "less code" as an end in itself. The goal is that
**every line exists because you decided it should**, and you can explain why.

## Relationship to the existing architecture docs

Read these first, but read them with the right frame:

| Doc | What it actually is | How to use it |
| --- | --- | --- |
| [overview.md](overview.md) | An **aspirational** target architecture (6 layers, postcard wire, `ConfigPaths`, anyhow at layer 5). Parts of it were never implemented. | Treat as a design proposal, not as documentation of current behaviour. Steal the layering rules; verify every factual claim against the code. |
| [migration-plan.md](migration-plan.md) | Ownership/borrowing analysis + phased plan + 14-entry decision log. | The **decision log is the most valuable artifact in the repo**. Re-litigate each decision consciously; most will survive. |
| [interface-map.md](interface-map.md) | Pre-rewrite audit of duplicate/competing types. | Use as the "do not recreate these mistakes" checklist. |
| [rewrite_process.md](rewrite_process.md) | Log of phases 1–6 that already happened. | Historical. Tells you what was already deleted so you don't reintroduce it. |

The rest of `docs/` (27 files, 8 of them `implementation-plan-*.md`, plus
`pr-description.md`, `review-issue-15.md`, `f1-help-menu-audit.md`) is planning
sediment from AI-driven feature work. **Archive it into `docs/history/` before you
start.** It will otherwise contradict everything you write.

---

## Part 1 — What exists today

Modules are grouped by how much of them is worth keeping.

### Tier A — Keep the design, rewrite by hand (the good core)

These have earned their place. Rewriting them is about tightening, not rethinking.

| Module | ~LOC | What it does |
| --- | --- | --- |
| [src/types/](../../src/types/) | 1,900 | Vocabulary layer: `TagName`, `TagrPath`, `FilterName`, `Pair`, `NoteMeta`, `MatchMode`, `TagExpr`, `QueryCriteria`. Zero intra-crate deps. |
| [src/db/](../../src/db/) | 1,400 | sled wrapper. Two trees (`files`, `tags` reverse index) plus notes. This is the actual performance story. |
| [src/query/](../../src/query/) | 1,000 | The single query pipeline: tag expansion, hierarchy specificity, glob/regex path filtering, vtag application. |
| [src/schema/](../../src/schema/) | 450 | `TagSchema`: aliases, hierarchy config, synonym expansion. |
| [src/vtags/](../../src/vtags/) | 1,300 | Virtual tags computed from filesystem/git metadata. Genuinely distinctive feature. |
| [src/filters/](../../src/filters/) | 1,100 | Saved filters (TOML). ~250 lines of this is legacy serde shims — see Tier C. |

### Tier B — Keep the capability, redesign the structure

The feature is wanted; the current shape is a committee decision.

| Module | ~LOC | The problem |
| --- | --- | --- |
| [src/store/](../../src/store/) | 2,000 | `TagStore` is a ~20-method trait with 3 impls (`DirectStore`, `DaemonStore`, `MockStore`). The trait is defensible, but 20 methods is a symptom — it grew one method per calling site rather than being designed. |
| [src/commands/](../../src/commands/) | 6,900 | The CLI surface. Sound in principle, but `bulk/` alone is 2,900 lines and `dispatch.rs` is a fan-out with no shared shape between commands. |
| [src/cli.rs](../../src/cli.rs) | 1,690 | One file holding every clap struct in the program. `Commands` is a 270-line enum followed by a 250-line `impl`. |
| [src/daemon/](../../src/daemon/) + [src/ipc/](../../src/ipc/) | 2,100 | Design is right (single `tokio::select!` loop owning all state). `execute_wire_request()` is a 250-line match. Wire format is `wincode` despite `postcard` being a dependency and the docs specifying postcard. |
| [src/keybinds/](../../src/keybinds/) | 1,700 | Configurable keybinds are a real feature. But `ActionExecutor` (470 lines) has **zero production call sites**. |
| [src/preview/](../../src/preview/) | 700 | Preview generation exists twice — here and in `ui/ratatui_adapter/styled_preview.rs`. |
| [src/config/](../../src/config/) | 430 | Works, but there are five separate config-path helpers and a `config ↔ ui ↔ preview` import cycle. |
| [src/watch/](../../src/watch/) | 340 | Fine. Coupled to daemon lifecycle through `commands/watch.rs`. |
| [src/completions/](../../src/completions/) | 700 | Dynamic shell completions with a versioned file cache. Good feature, over-trait'd. |

### Tier C — Delete outright

Do not port these. Write down here why, so you don't rebuild them by reflex.

| Thing | ~LOC | Why it goes |
| --- | --- | --- |
| [src/discovery/](../../src/discovery/) | 30 | `DiscoveryKind` + `FileDiscovery` trait. **No implementors. No callers.** Pure speculation. |
| `keybinds::executor::ActionExecutor` | 470 | Third parallel action-dispatch system. Only referenced by its own tests. |
| [src/browse/ui.rs](../../src/browse/ui.rs) `BrowseController<F: FuzzyFinder>` | 820 | The skim-era generic two-phase browse loop, still wired up in `commands/browse.rs`, running alongside the ratatui frontend that reimplements all of it. |
| `filters::types` legacy shims | 250 | `FilterCriteria`, `TagMode`, `FileMode`, `Raw*` + 6 `From` impls, existing solely for backward compat of a **pre-1.0** TOML format. You have not shipped 1.0. Break the format. |
| `TagrError` god-enum | — | 13 `#[from]` variants wrapping every module. The layering doc forbids exactly this. |
| Root artifacts | — | `file.txt`, `ignore.txt`, `lcov.info`, `checks.md`, `tmp/sled_lock_test.rs`, `tmp/bulk_test/`. |

### The elephant: `src/ui/` (~7,800 lines, the largest module)

This is where the rewrite will actually be decided.

- [state.rs](../../src/ui/ratatui_adapter/state.rs) — ~1,950 lines, a ~1,400-line `impl` block on one struct.
- [finder.rs](../../src/ui/ratatui_adapter/finder.rs) — ~1,300 lines, ~1,090-line `impl` block.
- [widgets/](../../src/ui/ratatui_adapter/widgets/) — ~3,000 lines across 11 widgets, `tag_tree.rs` alone ~930.
- Plus `traits.rs`, `types.rs`, `input.rs`, `output.rs`, `mock.rs`, `error.rs` — an abstraction layer over a single real backend.

**The tell:** the `FuzzyFinder` trait has exactly two implementors — `RatatuiFinder`
and a test mock — and `RatatuiFinder` needs `FinderConfig.database:
Option<Arc<dyn TagStore>>` to smuggle the store *back through* the abstraction that
was supposed to hide it. There is also a `RefCell` inside `RatatuiFinder` purely
because `FuzzyFinder` takes `&self`. When an abstraction forces both a back-channel
and interior mutability on its only real implementor, the abstraction is wrong.

### Duplication inventory (the AI fingerprint)

Each of these is the same concept independently re-derived:

- `FileMetadata` — **three** definitions: `browse/models.rs`, `preview/types.rs`, `vtags/cache.rs`
- `MetadataCache` — two: `browse/models.rs`, `vtags/cache.rs`
- `ItemMetadata` — two: `browse/models.rs`, `ui/types.rs`
- `ActionContext` — two: `browse/models.rs`, `keybinds/executor.rs`
- `BrowseError` — two: `browse/ui.rs`, `browse/session.rs` (a prior phase *decided to keep both*)
- `MockFinder` — two: `ui/mock.rs` and a private one in `browse/ui.rs`
- `format_timestamp` — three: twice in `commands/note.rs`, once in `preview/types.rs`
- Action dispatch — three: `BrowseSession::execute_action`, `AppState::execute_action`, `ActionExecutor::execute`
- Preview generation — two: `preview::PreviewGenerator`, `ui::…::StyledPreviewGenerator`
- Error convention — two: `anyhow` in `commands/watch.rs`, `TagrError` everywhere else in the same layer

### Dependency cycles to break

The layering is *mostly* clean, but:

- **`ui ↔ keybinds ↔ browse`** — a genuine 3-way cycle.
- **`config ↔ ui ↔ preview`** — `config` imports `ui::PreviewPosition`, `preview` imports `ui::PreviewConfig`, `ui::traits` imports `config::PreviewConfig`.
- `ui/…/finder.rs` and `keybinds/executor.rs` both import `commands::note::create_temp_note_file` — a CLI helper leaking into the TUI.
- `store/daemon.rs` imports `daemon::client` — the storage layer depends on the daemon frontend, inverting the stated layering.
- `types/tests.rs` imports `ipc::wire::ServerEvent` — layer 0 test reaching into layer 2.

### Dependency budget

44 direct dependencies, including `tokio` with `features = ["full"]`. Candidates for
elimination during the rewrite:

- `anyhow` — used in exactly one command module; pick one error convention.
- `heck`, `strsim` — check whether the case-conversion and fuzzy-distance uses justify crates.
- `config` (0.15) — you also have `toml` and `serde`; two config-loading stacks.
- `minus` — a pager, alongside a full ratatui TUI.
- `tokio` `full` — the daemon needs `rt`, `net`, `sync`, `time`, `macros`, not the kitchen sink.
- `wincode` **or** `postcard` — currently both. Pick one.

---

## Part 2 — Where to begin

### Strategy: parallel crate, not in-place refactor

Do **not** refactor `src/` in place. You will spend the whole time keeping 700 tests
green against code you are trying to delete, and you will end up preserving the
existing shape by accident — which is the entire thing you are trying to escape.

Instead:

```
tagr/
  Cargo.toml          # workspace
  legacy/             # today's crate, moved, frozen, still builds
  tagr/               # the handcrafted crate, starts empty
```

Rules:

1. `legacy/` is **reference material only**. You may read it. You may not `use` it from `tagr/`.
2. **Never copy-paste from `legacy/`.** Read the old implementation, close the file, then type the new one. If you cannot retype it, you do not understand it, and that is the signal to stop and think.
3. `tagr/` must compile and pass its own tests at every commit.
4. Delete `legacy/` when `tagr/` reaches parity. Do not keep it "just in case".

### How to use `legacy/`: design first, read second

The retyping rule defends against copy-paste but not against **anchoring**. If you
read `QueryCriteria` before deciding what a query needs to express, you will
reconstruct `QueryCriteria` — slightly cleaner, same shape, same seams. That produces
a hand-*transcribed* tagr, which is not what you are after.

For design-heavy modules — `types/`, `db/`, `query/`, and the TUI architecture — use
this order instead:

1. Write down what the layer must do, from the requirement, **without opening `legacy/`**.
2. Design it. Record the decision.
3. Implement it.
4. *Now* read the legacy implementation, as a code review of your own work — looking for edge cases you missed.
5. Fold in what is genuinely load-bearing. Ignore the rest.

Legacy becomes a test oracle rather than a template.

For plumbing — clap wiring, note formatting, TOML parsing, `bulk/` — this is overkill.
Read it, retype it, move on. That code is mechanical and the legacy version already
encodes real bug fixes. **Do not spend the motivation budget hand-crafting argument
parsers.**

### Architecture decision records

`docs/adr/ADR-NNN-short-title.md`, sequentially numbered, one file per decision, from
day one. Copy [ADR-000-template.md](../adr/ADR-000-template.md); it carries the house
rules in a trailing comment.

Three rules:

- **Write the ADR before implementing**, not after. Post-hoc records rationalise; pre-hoc records decide. This is the whole value; an ADR written after the code is just a changelog entry with more ceremony.
- **If you cannot name a rejected option, you have not made a decision** — you have copied one. Stop and find the alternative before writing the file.
- **Supersede, never edit.** When a decision changes, mark the old one `Superseded by NNNN` and write a new file. The trail of *changed* minds is where the re-understanding actually lives; overwriting it destroys the record of what you learned.

Candidate ADR-0001 through 0005, all of which the current code decided implicitly and
none of which you have consciously ratified: sled vs. an alternative store; whether
`TagStore` should exist as a trait at all; sync trait vs. async; one wire format;
error strategy (per-module `thiserror` and where conversion happens, versus the
`TagrError` god-enum).

For prior art on the bar to hit, [migration-plan.md](migration-plan.md) carries a
14-entry decision log — the most useful thing in `docs/`, and the reason this rewrite
is worth doing as records rather than as commits.
### Layers and slices are orthogonal

These are not competing plans, and they are not the same kind of thing.

**Layers are physical and permanent.** They are directories in `src/`, enforced by the
compiler. They exist from the first commit and they never go away.

**Slices are temporal and disposable.** They are units of work. A slice is not a place
in the code — it is a batch of changes that gets *distributed into* the layers it
touches. Once merged, a slice leaves no trace in the module tree.

So the module tree looks roughly the same on day one as on day two hundred:

```
src/
  types/      # vocabulary — no intra-crate deps
  store/      # persistence
  query/      # the pipeline
  commands/   # CLI
  ui/         # TUI
  main.rs
```

...and Slice 1 (`tagr tag foo.txt work` → `tagr search work`) is not a directory. It is:

| Layer | What Slice 1 puts there |
| --- | --- |
| `types/` | `TagName` and nothing else |
| `store/` | `open`, `add_tag`, `tags_for`, `files_with_tag` |
| `query/` | one function: exact-tag lookup, no expressions |
| `commands/` | `tag.rs`, `search.rs` |
| `main.rs` | clap wiring for two subcommands |

Slice 2 then adds `TagExpr` to `types/`, the reverse index to `store/`, and expression
evaluation to `query/` — same directories, more depth. Nothing moves.

Why the boundaries go in first, empty:

- **Establishing a boundary early is free; establishing it late is a rewrite.** The three duplicate `FileMetadata` types exist because nobody decided where metadata lived until code in three places already needed it.
- **Thin layers make violations visible.** When a layer is 40 lines, an inappropriate dependency is obvious. When it is 1,900, it isn't — which is how `ui ↔ keybinds ↔ browse` became a cycle.
- **The compiler polices it, not your attention.** Separate modules, `pub(crate)` discipline, no upward `use`.

Two constraints on "minimal":

- **Minimal, not stubbed.** A thin layer must be *complete for the current slice*. No `todo!()`, no methods that panic, no "I'll wire this up later". Thin means fewer features, not unfinished ones.
- **The boundary is ADR'd; the contents are free.** Commit to *where the seam is* and *what crosses it*. Do not commit to internals — those are meant to grow. If a later slice makes you move a seam, that is a new ADR superseding the old one, and that is a healthy outcome, not a failure.

**The layer model itself is a decision you have not made yet.** [overview.md](overview.md)
specifies six layers (types → interfaces → stores → query → browse domain → frontends)
with five hard rules. That model was largely inherited rather than chosen, and the code
already violates it: `store/daemon.rs` imports from `daemon/`, `config` imports from
`ui`. Before Slice 1, ratify it, amend it, or replace it — as an ADR, with the
alternatives named. Adopting it by default is exactly the thing this rewrite exists to
stop doing.

### The ordering: slices through the layers

Strict bottom-up (finish `types/`, then finish `db/`, then finish `query/`…) is
dependency-correct and it is a trap. It front-loads the ~15% that is enjoyable and
back-loads the ~85% that is slog, with no running program until well past the halfway
point. That is precisely the stall mode.

Instead, every slice cuts through **all** the layers it needs, and later slices deepen
layers that already exist rather than introducing them.

**Slice 0 — Housekeeping (before any code)**
- F1 deletions, root junk, `docs/` → `docs/history/`.
- Write down, in one page, **what tagr is for**. Not features — purpose. Every later "should I keep this?" question resolves against that page.
- Set up `docs/adr/`. ADR-0001: the layer model — ratified, amended, or replaced, with alternatives named.

**Slice 1 — The spine.** `tagr tag foo.txt work` → `tagr search work` → prints a path.
Every layer present, every layer minimal: one newtype, a handful of store methods, a
query engine that only does exact-tag lookup, two commands, one output format. No
trait abstractions yet — a concrete store type is correct here; extract `TagStore` in
Slice 8 if the daemon actually needs a second implementor.
This is the most important slice: it fixes the seams, and you get a working binary in
week one.

**Slice 2 — Depth in the core.** The layers from Slice 1, filled in.
Tag expressions and match modes in `types`; the reverse index and its *both trees
always agree* invariant, enforced by a choke point rather than by discipline at every
call site; the single query pipeline (expand → evaluate → hierarchy → path patterns),
one entry point, a second query path is a bug. This is where the design-first rule
earns its keep, and where you decide whether sled stays at all.

**Slice 3 — CLI breadth.** `untag`, `list`, `tags`, `cleanup`, output formats,
`--quiet`/`--format json`, exit codes. Get the headless path completely right — per
the project philosophy the CLI is the product and the TUI is a teaching layer over it,
and the codebase should reflect that ratio. Today it does not: `ui/` is 7,800 lines,
`commands/` is 6,900.
Gate: `tests/cli_conformance_test.rs` equivalents passing against the real binary.

**Slice 4 — Distinctive features.** `schema/` (aliases, hierarchy) and `vtags/`.
The things that make tagr tagr rather than a key-value store with a CLI. Both are new
layers — decide where they sit in the model before writing them, not after.

**Slice 5 — Stored state.** `filter`, `note`, `alias`. Break the pre-1.0 TOML format;
do not port the legacy serde shims.

**Slice 6 — TUI.** A single concrete `ratatui` application with **no trait abstraction
at all**. Not `FuzzyFinder`, not `UserInput`, not `OutputWriter`. If a second frontend
ever genuinely materialises, extract the trait *then*, informed by two real
implementors instead of one and a mock. This is the largest slice and the one where
the architecture decisions are most yours to make — budget accordingly.

**Slice 7 — `bulk/`.** Mechanical, derivative of everything below it, ~2,900 lines in
legacy. Transcription tier.

**Slice 8 — daemon / watch.** Last, because it is an optimisation over a system that
must already work without it. This is also where the store abstraction gets earned:
you now have two real implementors and can extract the trait from evidence rather than
from anticipation. Keep the single-`select!`-loop-owns-all-state design; that decision
was correct. Pick one wire format.

### What "done" looks like per slice

A slice is complete when:

- The binary still works, end to end. **Non-negotiable from Slice 1 onward.**
- No layer gained an upward dependency. Check it; don't assume it.
- It compiles with `cargo clippy -- -W clippy::pedantic -W clippy::nursery` clean.
- Its tests pass, and you can state what each test is protecting against.
- You can explain every public item without re-reading it.
- Every non-obvious decision in it has an ADR, written before the code.
- Nothing in it exists "for later".

---

## Part 3 — What to remember while rewriting

### The core discipline

**Write the call site first.** Before defining a type or a function, write the code
that uses it. Most of the current bloat exists because implementations were generated
before anyone knew how they would be called, so they were made general enough to
cover every guess.

**One implementor means no trait.** `FileDiscovery` has zero. `UserInput` has one.
`FuzzyFinder` has one plus a mock. A test mock is not a second implementor — it is a
sign you should be testing at a different level, or using a real in-memory instance.
The rule: extract an abstraction on the *second* real implementor, not the first.

**Duplication is cheaper than the wrong abstraction — but only once.** Three
`FileMetadata` definitions is not "cheap duplication", it is three sources of truth.
When you write the same concept a second time, note it. On the third, unify.

**Delete before you add.** Every step should have a net-negative or flat line count
compared to the equivalent legacy code, unless you can name what the extra lines buy.

### Rust-specific traps this codebase fell into

**The `.clone()` epidemic.** The migration plan already documented it: `String` tag
names cloned at every boundary, 7 identical `current_file` clones in one file, "121
`Vec<String>` params vs 27 `&[String]`". Rule: **borrow in, own out.** Query
functions borrow everything; mutations take ownership; storage returns owned.

**`&Vec<T>` / `&String` in signatures.** Always `&[T]` and `&str`.

**Interior mutability as an escape hatch.** The `RefCell` in `RatatuiFinder` exists
because a trait signature was wrong. When you reach for `RefCell` in single-threaded
code, treat it as a compiler-mentored signal that your ownership design is off, not
as a fix.

**God enums.** `TagrError` with 13 `#[from]` variants means every caller must handle
every failure in the program. Per-module error types, and convert deliberately at
layer boundaries. Embed, don't wrap-everything.

**Long `impl` blocks.** A 1,400-line `impl` on one struct means that struct is doing
several jobs. `AppState` is the canonical example. Split by responsibility, not by
line count.

**Feature-flag stubs.** `styled_preview.rs` carries a duplicate no-op
`StyledPreviewGenerator` for non-syntect builds. Either the feature flag earns its
keep or it goes; maintaining two implementations of a cosmetic feature does not.

### Testing philosophy

Today: ~68 `#[cfg(test)]` modules, four files that exist *only* to hold tests
(`types/tests.rs` ~1,000 lines, `bulk/tests.rs` ~1,300), and 33 doctests driven by
extremely long `//!` module prose. Roughly 35–40% of `src/` is test code.

That is not automatically wrong, but a lot of it is testing generated code against
generated expectations. For the rewrite:

- **Integration tests over unit tests.** `tests/cli_conformance_test.rs` and
  `tests/daemon_test.rs` are the most valuable tests in the repo — they spawn the
  real binary and assert observable behaviour, including direct-vs-daemon parity.
  Those contracts survive a rewrite. Unit tests on internal helpers mostly do not.
- **Every test protects against a specific failure.** If you cannot name it, delete
  the test.
- **Doctests should be documentation you'd actually write**, not a way to inflate the
  count. `ui/mod.rs` is 170 lines of which ~150 are ASCII-art prose. Don't do that.
- Consider `insta` for snapshot-testing CLI output; the conformance tests are
  currently doing this by hand.

### Documentation philosophy

The current `//!` blocks describe architecture that partly does not exist. The
existing repo convention already says it: comments explain **why**, not **what**.
Extend that to modules — a module doc should say what the module is for and what
invariants it maintains, not draw a diagram of the whole program. Architecture belongs
in `docs/`, and `docs/` must be updated or deleted when it stops being true.

### The honest risk

The most likely failure mode is **stalling in the pleasant part**. The core types and
query engine are small, self-contained, and satisfying to perfect. The CLI breadth and
the TUI are ~15k lines of work that is mostly tedious. If you find yourself polishing
`TagName` for the fourth week, that is the failure mode arriving — and the vertical
slice ordering exists specifically to make it visible early.

Mitigations, in order of importance:

1. **A working binary from Slice 1 onward, every commit.** A rewrite that produces a beautiful `types/` and nothing else is strictly worse than the code you have now.
2. **Timebox each slice.** Not to hit a date — to notice when a slice has stopped being about the slice.
3. **Transcribe the plumbing.** The tedium tax is unavoidable; do not voluntarily increase it by hand-designing code that has no design in it.

The second failure mode is scope creep — "while I'm in here, I'll also add…". The
success criterion is *feature parity, less code, decisions you made*. New features go
in a list, and the list waits.

The third is subtler: **losing the plot on why you're doing this**. If it stops being
interesting, the project has failed at its actual purpose regardless of how the code
looks. That is a legitimate reason to stop, switch approach, or reorder — and a better
one than pushing through on discipline alone.

---

## Quick reference: rewrite checklist

**Pre-work (independent of the rewrite)**

- [ ] **F1** — delete `src/discovery/` and `keybinds::executor::ActionExecutor` (~500 dead lines)
- [ ] **F3** — annotate or correct `overview.md`; fix the stale version in `copilot-instructions.md`
- [ ] Archive `docs/` planning artifacts to `docs/history/`
- [ ] Delete root junk (`file.txt`, `ignore.txt`, `lcov.info`, `tmp/`)

**Slice 0 — setup**

- [ ] Write the one-page "what tagr is for"
- [ ] Set up `docs/adr/`; ADR-0001 = the layer model (ratify, amend, or replace)
- [ ] Set up workspace: `legacy/` (frozen) + `tagr/` (empty)

**Slices**

- [ ] Slice 1 — the spine: `tag` → `search` → path on stdout (**working binary**)
- [ ] Slice 2 — depth in `types/`, `db/`, `query/` (design first; read legacy last)
- [ ] Slice 3 — CLI breadth, output formats, exit codes
- [ ] Slice 4 — `schema/`, `vtags/`
- [ ] Slice 5 — `filter`, `note`, `alias` (break the pre-1.0 TOML format)
- [ ] Slice 6 — TUI, concrete, zero traits
- [ ] Slice 7 — `bulk/` (transcription tier)
- [ ] Slice 8 — daemon / watch; pick one wire format

**Close-out**

- [ ] Delete `legacy/`
- [ ] Audit dependency list; drop `anyhow` or `TagrError`, `wincode` or `postcard`, narrow `tokio` features

Per-module gate, applied every time:

1. Can I explain why this module exists in one sentence?
2. Does every trait here have two real implementors?
3. Does every type here have exactly one definition in the crate?
4. Did I write the call site before the implementation?
5. Did I decide this, or inherit it? If decided — does it have an ADR?
6. Is this smaller than the legacy equivalent, or can I say what the extra buys?
