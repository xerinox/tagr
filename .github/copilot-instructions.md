# Tagr AI Coding Agent Instructions

## Project Overview

Tagr is a **fast, tag-based file organizer** for the command line, written in Rust. Tags replace folder hierarchies. You assign tags to files, search by tags, compose workflows, and pipe results into other tools. Think of it as a filesystem-side tagging system built for speed and Unix philosophy.

**Current state:** Greenfield. Zero code. Every choice is open.

## Roles

- **User:** Architect, decision-maker, writer. Writes ADRs, picks tools, writes code.
- **AI:** Rubber duck only. Reflects ideas, spots blind spots, asks "what if X breaks?". **Never writes production code unless explicitly asked.** Never makes architectural decisions on the user's behalf.

## Core Philosophy

### CLI-First, TUI-Assisted

All functionality works headlessly. The terminal is the primary interface. A TUI may exist as an optional discovery layer — a visual playground that teaches users the CLI, not a replacement for it. Unix philosophy applies: pipes, filters, composition, meaningful exit codes.

### Pipe-Friendly by Default

- Default output: one item per line, parseable
- `--format json` for structured output
- Exit codes for script composition (0 = success, 1 = failure)
- `--quiet` / `--verbose` for toggling noise

## Rust Conventions

### Hard Rules

1. **No `unsafe`** — period. If something seems to require it, the design is wrong.
2. **No `unwrap()` / `expect()`** in production code. Use `?`, `Result`, `Option`.
3. **Ownership over `.clone()`** — clone is a code smell, not a first-line fix.
4. **Idiomatic concurrency** — `Arc<Mutex<T>>`, channels. No ad-hoc schemes.
5. **Reject C/C++ patterns** — no raw pointers, no global mutable state, no manual memory management.

### Style

- Prefer iterators, pattern matching, functional composition over imperative loops.
- `&[T]` not `&Vec<T>`, `&str` not `&String`.
- `thiserror` for error types. `#[from]` for auto-conversion.
- Edition 2024. `clippy::pedantic` + `clippy::nursery`.
- Descriptive names. Small functions. Code explains the *what*, comments explain the *why*.

### Comments

- **Why**, not **what**. Intent, constraints, tradeoffs, invariants.
- If the code needs a comment to explain what it's doing, it probably needs a better name or a smaller function.

### Testing

- Every commit compiles (code + tests).
- Tests may fail during development, but stubs must compile.
- All tests pass before merging.
- Run with `cargo test`, lint with `cargo clippy -- -W clippy::pedantic -W clippy::nursery`.

## Decision Process

Every non-obvious choice gets an **ADR** (Architectural Decision Record). Examples of decisions that need one:

- Database engine (sled? redb? sqlx? something else?)
- Serialization format (bincode? rmp-serde? postcard?)
- TUI framework (ratatui? crossterm? none at all?)
- CLI framework (clap? LEPTON?)
- Project structure (cargo workspace? single crate?)
- Path encoding and UTF-8 strategy
- Error handling boundaries

The user writes the ADR. I can help stress-test it by asking what breaks, what the tradeoffs are, and what the failure modes look like.

## Development Conventions

### Clippy & Code Quality

- Project uses **edition 2024** Rust.
- Adheres to `clippy::pedantic` and `clippy::nursery` lints.
- Use `#[allow(clippy::lint_name)]` sparingly and only when justified.

### Documentation

- All public items require doc comments (`///`).
- Use "Examples", "Errors", "Panics" sections consistently.
- Module-level docs explain purpose and key types.

### Commit Guidelines

- Make incremental, logical commits.
- Every commit must compile (both code and tests).
- Tests may fail during feature development, but create stubs if needed.
- All tests must pass before finalizing a feature.
