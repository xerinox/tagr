# Contributing to Tagr

Thank you for your interest in contributing to Tagr! This document outlines the development guidelines and architectural philosophy that keep are codebase fast, elegant, and secure.

## Codebase and Architectural Philosophy

Tagr is a high-performance system tool, and we have strict conventions in place to maintain code quality. Please familiarize yourself with these guidelines before submitting a pull request.

### 1. CLI-First, TUI-Assisted Design
All features **must** work fully headlessly. The TUI (browse mode) is a visual convenience and learning layer, but never a replacement for a robust CLI command.
- Focus on pipes, filters, standard input/output formatting, and Unix-philosophy composition.
- Quiet mode (`--quiet`) must output clean, raw, pipe-friendly data without decorations or header elements (ideal for scripting).
- Implement full CLI capability *before* wiring up TUI visuals.

### 2. Strict Safe Rust Rule (No `unsafe`)
We maintain a zero-compromise safe Rust policy.
- **Do not use the `unsafe` keyword under any circumstances.**
- Avoid mutable static variables (`static mut`) or raw pointers.
- Work with the borrow checker to design clean, provably safe resource lifetimes.

### 3. Minimize Cloning (`.clone()`)
We treat `.clone()` as a design smell to be avoided whenever possible.
- Prioritize using borrows, lifetimes, and standard traits (`AsRef<str>`, `Borrow<str>`, etc.).
- Convert standard types to dedicated domain-driven validated newtypes like `TagName` and `TagrPath`.
- Only use `.clone()` when making a deep copy of the underlying data is a logical, semantic necessity.

### 4. Correctness & Robust Error Handling
- **No `unwrap()` or `expect()`**: Production code must handle every error explicitly using `Result<T, E>` and `Option<T>`.
- Use the `thiserror` crate to specify structured, context-rich error types. Use `#[from]` for automatic conversion, and context-rich `.map_err()` when wrapping external errors.
- Never let failures panic. Return human-friendly error messages that direct the user to the correct path of action.

### 5. Idiomatic Rust Patterns
- Use iterator chains (`.map()`, `.filter()`, `.fold()`, `.collect()`) over imperative `for` loops.
- Use pattern matching (`match`, `if let`, `while let`) to guarantee correctness and handle all states exhaustively.
- Leverage the type system to make invalid states unrepresentable.

### 6. Strict Clippy Compliance
Tagr complies with both `clippy::pedantic` and `clippy::nursery` lint groups. Before submitting any changes, make sure your code passes with no clippy warnings:
```bash
cargo clippy --all-targets -- -W clippy::pedantic -W clippy::nursery
```

## Development and Setup

To start developing on Tagr, make sure you have the Rust toolchain installed (Edition 2024).

### Building and Running the Code

```bash
# Compile a debug build
cargo build

# Compile a release build (much faster database performance)
cargo build --release

# Run interactive browse mode using debug binary
cargo run -- browse
```

### Running Tests

We expect complete test coverage for all features.

```bash
# Run all unit and integration tests
cargo test

# Run a specific unit test
cargo test test_name

# Run only integration tests
cargo test --test integration_test

# Validate that code compiles without actually running the full test suite
cargo test --no-run
```

## Submission Checklist

Before submitting a Pull Request, please ensure:
1. [ ] Your code compiles cleanly on both debug and release profiles.
2. [ ] All existing and new tests pass successfully (`cargo test`).
3. [ ] No Clippy warnings are produced (`cargo clippy -- -W clippy::pedantic -W clippy::nursery`).
4. [ ] Standard formatting has been applied (`cargo fmt`).
5. [ ] Relevant changes are documented in `README.md` and `CHANGELOG.md` under `[Unreleased]`.
