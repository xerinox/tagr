# Tagr AI Coding Agent Instructions

## Project Overview

Tagr is a **fast, tag-based file organizer** for the command line built in Rust. It uses an embedded sled database with **reverse indexing** for O(1) tag lookups, fuzzy finding (skim), and interactive browsing. Think of it as a tag-based alternative to traditional folder hierarchies.

**Core value proposition**: 100-1000x faster tag queries via multi-tree architecture (files tree + tags reverse index).

## Architecture

### Multi-Tree Database Design

The database uses **two sled trees** for bidirectional lookups:

```rust
Database {
    db: Db,           // Database handle
    files: Tree,      // file_path -> Vec<tag>
    tags: Tree        // tag -> Vec<file_path> (reverse index)
}
```

**Critical**: Both trees must stay synchronized. When updating tags:
1. Update `files` tree with new tags
2. Remove old file associations from `tags` tree via `remove_from_tag_index()`
3. Add new file associations to `tags` tree via `add_to_tag_index()`

See `src/db/mod.rs::insert_pair()` for the canonical pattern.

### Module Structure

- **`src/db/`**: Database wrapper (types, query, error handling)
  - Uses bincode for serialization (not serde_json)
  - `PathKey` and `PathString` wrappers ensure UTF-8 safety
  - All database operations return `Result<T, DbError>`
- **`src/filters/`**: Saved filter management (CRUD, export/import)
  - Stores filters in `~/.config/tagr/filters.toml`
  - `FilterCriteria` represents search params
  - Builder pattern via `FilterCriteria::builder()` for tests
- **`src/vtags/`**: Virtual tags (dynamic file metadata queries)
  - Parser, evaluator, cache, config modules
  - Zero database storage - computed from filesystem metadata
  - Uses rayon for parallel evaluation
- **`src/search/`**: Interactive browse mode using skim fuzzy finder
  - Two-stage selection: tags → files
  - Multi-select enabled via TAB key
- **`src/commands/`**: CLI command implementations
  - Each command in separate file: `browse.rs`, `tag.rs`, `search.rs`, etc.
  - Command logic decoupled from CLI parsing (`src/cli.rs`)
- **`src/config/`**: Configuration management
  - Platform-specific paths (Linux/macOS/Windows)
  - First-run setup wizard in `setup.rs`

## Development Conventions

### Code Comments

**Avoid redundant "what" comments** - code should be self-explanatory through clear naming and structure. Comments should explain **WHY**, not **WHAT**.

❌ **Bad - Redundant "what" comments:**
```rust
// Get file metadata
let metadata = fs::metadata(path)?;

// Check if file exists
if path.exists() {
    // Create new item
    let item = Item::new();
}
```

✅ **Good - Comments explain WHY:**
```rust
let metadata = fs::metadata(path)?;

// Skip preview if file exceeds size limit to avoid memory issues
if metadata.len() > self.config.max_file_size {
    return Err(PreviewError::FileTooLarge);
}

// Use InvalidData error to distinguish encoding issues from I/O errors
let content = fs::read_to_string(path).map_err(|e| {
    if e.kind() == std::io::ErrorKind::InvalidData {
        PreviewError::InvalidUtf8(path.display().to_string())
    } else {
        PreviewError::IoError(e)
    }
})?;
```

When the code is clear, no comment is needed. Focus on meaningful function names, clear variable names, and logical structure.

### Error Handling

- Use `thiserror` for error types: `#[error("message: {0}")]`
- Propagate errors with `?` operator, don't unwrap in library code
- Return `Result<T, DbError>` for database ops, `Result<T, TagrError>` for top-level
- Use `#[must_use]` on functions returning Results or important values

### Path Handling

- **Always** use `PathString::new()` to validate UTF-8 when storing paths as strings
- **Always** use `PathKey::new()` when creating database keys from paths
- Use `PathBuf` internally, but validate before database insertion
- Example pattern:
  ```rust
  let file_path = PathString::new(&pair.file)?;
  let key: Vec<u8> = PathKey::new(&pair.file).try_into()?;
  ```

### Testing

- Use `TestDb` wrapper from `src/testing.rs` for database tests
  - Automatically cleans up on drop
  - Always `clear()` before testing
- Use `TempFile` for test file fixtures (auto-cleanup)
  - Creates unique temp dirs to avoid parallel test collisions
- Integration tests in `tests/integration_test.rs`
- Unit tests inline with `#[cfg(test)]` modules
- Run with: `cargo test`

**Commit Guidelines:**
- Make incremental, logical commits while working on features
- Every commit must compile (both code and tests)
- Tests may fail during feature development, but create stubs if needed to keep them compiling
- All tests must pass before finalizing/merging a feature
- Use `cargo test --no-run` to verify tests compile without running them

### Clippy & Code Quality

- Project uses **edition 2024** Rust
- Adheres to `clippy::pedantic` and `clippy::nursery` lints
- Use `#[allow(clippy::lint_name)]` sparingly and only when justified
- Common acceptable exceptions:
  - `#[allow(clippy::too_many_lines)]` for long but cohesive functions (e.g., CLI handlers)
  - `#[allow(clippy::too_many_arguments)]` for builder-like patterns
  - `#[allow(clippy::unnecessary_wraps)]` for API consistency

### Documentation

- All public items require doc comments (`///`)
- Use "Examples", "Errors", "Panics" sections consistently
- Module-level docs explain purpose and key types
- See `src/db/mod.rs` for canonical documentation style

## Key Workflows

### Building & Running

```bash
# Debug build
cargo build

# Release build (much faster for large databases)
cargo build --release

# Run (uses default database)
cargo run -- browse

# Run with specific database
cargo run -- --db mydb browse
```

### Testing

```bash
# All tests
cargo test

# Specific test
cargo test test_insert_and_retrieve

# Integration tests only
cargo test --test integration_test

# Linting
cargo clippy -- -W clippy::pedantic -W clippy::nursery
```

### Adding New Commands

1. Create command module in `src/commands/`
2. Add command variant to `Commands` enum in `src/cli.rs`
3. Implement argument parsing (use `clap` derives)
4. Add handler in `main.rs` match statement
5. Wire up helper methods (`get_*_from_*` pattern)

Example: See `src/commands/filter.rs` for full command implementation.

### Working with Filters

When implementing search/filter features:

1. Use `FilterCriteria` for search parameters
2. Implement bidirectional conversion:
   - `impl From<SearchParams> for FilterCriteria`
   - `impl From<&FilterCriteria> for SearchParams`
3. Use `SearchParams::merge()` to combine filter + CLI args
4. Store filters via `FilterManager` at `~/.config/tagr/filters.toml`

Pattern from `src/commands/search.rs`:
```rust
let mut params = cli_params;
if let Some(filter_name) = filter_name {
    let filter = filter_manager.get(filter_name)?;
    params = params.merge(&filter.criteria);
}
```

### Implementing Virtual Tags

Virtual tags evaluate file metadata dynamically:

1. Add variant to `VirtualTag` enum in `src/vtags/types.rs`
2. Implement parsing in `src/vtags/parser.rs`
3. Add evaluation logic in `src/vtags/evaluator.rs::evaluate()`
4. Update documentation with examples
5. Consider caching in `MetadataCache` for performance

## Performance Considerations

- **Tag lookups**: O(1) via reverse index - use `find_by_tag()` not iteration
- **Large file sets**: Use rayon's `par_iter()` for parallel processing (see vtags)
- **Database flushes**: Automatic on drop, but explicit `flush()` for durability
- **Serialization**: bincode is faster than serde_json for internal storage
- **Metadata caching**: Use `MetadataCache` with TTL for vtag evaluations (default 300s)

## Common Pitfalls

❌ **Don't** iterate files to find tags - use reverse index:
```rust
// BAD
for pair in db.list_all()? {
    if pair.tags.contains(&tag) { /* ... */ }
}

// GOOD
let files = db.find_by_tag(&tag)?;
```

❌ **Don't** forget to update reverse index when modifying tags:
```rust
// Must remove old associations before adding new ones
self.remove_from_tag_index(file_path.as_str(), &old_tags)?;
self.add_to_tag_index(file_path.as_str(), &pair.tags)?;
```

❌ **Don't** use `PathBuf::to_str()` without checking for None:
```rust
// BAD
let path_str = path.to_str().unwrap();

// GOOD
let path_str = PathString::new(path)?;
```

❌ **Don't** create temporary files without cleanup:
```rust
// Use TempFile wrapper for automatic cleanup
let temp = TempFile::create("test.txt")?;
// File auto-deleted on drop
```

## Library Usage

Tagr can be used as a library. Public API exports:

- `tagr::db::Database` - Core database operations
- `tagr::search::browse()` - Interactive fuzzy finder
- `tagr::filters::FilterManager` - Filter management
- `tagr::Pair` - File-tag data structure
- `tagr::cli::execute_command_on_files()` - Execute shell commands on file selections

See `README.md` "Library Usage" section for examples.

## Configuration

- Config file: `~/.config/tagr/config.toml` (Linux)
- Filters file: `~/.config/tagr/filters.toml`
- Database default: `~/.local/share/tagr/` (Linux)
- Paths are platform-specific - use `dirs` crate functions

## Project State

**Current version**: 0.4.0 (edition 2024)

**Recently completed** (see CHANGELOG.md):
- ✅ Virtual tags (12 types: time, size, extension, permissions, git, etc.)
- ✅ Saved filters with export/import
- ✅ Multi-tree reverse indexing (100-1000x faster queries)
- ✅ Interactive browse mode with fuzzy finding
- ✅ Database cleanup command

**Future enhancements** (from CHANGELOG.md):
- Preview pane in browse mode
- Tag statistics and autocomplete
- Transaction support for batch operations
- File watching for auto-cleanup

When implementing new features, follow patterns from recently completed work (virtual tags, filters) as reference implementations.
