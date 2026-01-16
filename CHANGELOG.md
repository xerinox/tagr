# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.10.0] - 2026-01-16

### Added

#### File Notes (Complete)
- **Markdown Notes for Files** - Attach rich text documentation to any file
  - Free-form markdown content with timestamps
  - Simple append-style workflow with chronological context
  - Full editor control via `$EDITOR` integration
  - Zero task management complexity (no priority, status, or due dates)
- **CLI Commands** - Complete headless interface
  - `tagr note add <file> <content>` - Quick append with timestamp
  - `tagr note edit <file>` - Full editor access (create/update)
  - `tagr note show <file>` - Display note content
  - `tagr note search <query>` - Full-text search across all notes
  - `tagr note list` - List all files with notes
  - `tagr note delete <file>` - Remove notes
  - Pipe-friendly output with `--format quiet|json|text`
  - Exit codes for script composition (0 = success, 1 = failure)
- **TUI Integration** - Visual convenience layer
  - **Ctrl+N** - Edit note for selected file (suspends TUI, launches `$EDITOR`)
  - **Alt+N** - Toggle between file preview and note preview
  - **" Notes Only" category** - Special tag tree entry for files with notes but no tags
  - **Markdown syntax highlighting** - Notes rendered with full syntect integration
  - **Ctrl+L details modal** - Inline file metadata with note preview
- **Database Integration** - Seamless storage alongside tags
  - New `notes` sled tree for O(1) lookups
  - Equality model: Files tracked if tags OR notes OR both exist
  - Atomic operations preserve data integrity
  - Cleanup command removes orphaned notes
- **Timestamp Format** - Markdown-native chronological entries
  - Format: `### YYYY-MM-DD HH:MM` (H3 heading)
  - Horizontal rules (`---`) between entries for clear separation
  - Fully compatible with markdown parsers and syntax highlighters
  - User-editable - reorganize/delete entries as needed
- **Configuration** - Customizable note behavior
  - `notes.editor` - Override `$EDITOR` environment variable
  - `notes.max_note_size_kb` - Size limit warnings
  - `notes.storage` - Storage mode (integrated database, future: file-backed)
  - Stored in `~/.config/tagr/config.toml`

### Changed
- **Simplified TUI Architecture** - Major refactor for clarity
  - Removed `BrowsePhase` enum entirely (TagSelection vs FileSelection)
  - Always show 3-pane layout (tags | files | preview)
  - Focus pane determines behavior instead of phase switching
  - Tag tree always built from database with full tag list
  - Simplified keybind management (no phase-specific filtering)
- **Preview System** - Extended for notes support
  - Added `PreviewContent::Note` variant
  - Toggle between file content and note content via Alt+N
  - Preview cache properly invalidates on mode changes
  - Note preview shows markdown with timestamp highlighting

### Fixed
- **Tag Tree Filtering** - Synchronized multi-pane filtering
  - Fuzzy search now filters both tags AND files simultaneously
  - Query filters tag tree by tag name substring matching
  - Query also filters file list via nucleo fuzzy matching
  - Real-time synchronized filtering across both panes
- **Selection Visibility** - Improved visual feedback
  - Checkmarks stay green, not affected by highlight color
  - Only filename text gets REVERSED style, not entire row
  - Tag tree: blue background + bold when both highlighted AND selected
- **Tag Mode Syncing** - Automatic mode switching for UX
  - Automatically sets Any (OR) for multiple tag selections
  - Automatically sets All (AND) for single tag selection
- **Preview Rendering** - Fixed blank preview issues
  - Preview now updates when navigating files with j/k/up/down
  - Fixed preview in 3-pane TUI during tag selection
  - Preview generation uses correct file path in tag selection mode
- **CLI Preview Accuracy** - Status bar shows correct tag mode
  - Displays `--any-tag` vs `--all-tags` based on actual mode
  - Accurate CLI command representation in status bar

### Testing
- 368 library tests passing (+14 from v0.9.0)
- 43 integration tests passing (+6 from v0.9.0)
- Comprehensive note functionality coverage:
  - Database CRUD operations (10 tests)
  - CLI command behavior (4 tests)
  - Integration workflows (4 tests)
  - Append logic and timestamp formatting (8 tests)

---

## [0.9.0] - 2026-01-07

### Added

#### Tag Hierarchies and Aliases (Complete)
- **Hierarchical Tags** - Organize tags with parent:child relationships using `:` delimiter
  - Example: `lang:rust:async` creates three-level hierarchy
  - Search expansion: searching `lang:rust` automatically includes `lang:rust:async`
  - `--no-hierarchy` flag to disable expansion in search/browse
  - Tree visualization with `tagr tags list --tree`
- **Tag Aliases (Synonyms)** - Create multiple names for the same tag
  - `tagr alias add js javascript` - map alias to canonical tag
  - Automatic canonicalization on tag insertion (stores only canonical form)
  - Search/browse expands to all synonyms (e.g., search "js" finds "javascript")
  - `--no-canonicalize` flag to opt out when tagging
  - Circular reference detection prevents infinite loops
  - Case-insensitive alias matching
- **Tag Schema Module** - Core type system for managing relationships
  - `TagSchema` struct with bidirectional indices (forward + reverse)
  - Persists to `~/.config/tagr/tag_schema.toml`
  - Level-by-level canonicalization for hierarchical tags
  - Comprehensive validation (reserved delimiters, circular refs)
  - TOML serialization/deserialization
- **CLI Commands** - Complete alias management interface
  - `tagr alias add <alias> <canonical>` - Create new alias
  - `tagr alias remove <alias>` - Delete alias
  - `tagr alias list` - Show all aliases
  - `tagr alias show <tag>` - Display synonyms for a tag
  - Colored, user-friendly output
- **Query Expansion** - Automatic tag expansion in searches
  - Centralized in `db/query.rs::apply_search_params()`
  - Expands to all synonyms (javascript → [js, es, javascript])
  - Expands hierarchical tags to parents (lang:rust → [lang, lang:rust])
  - Automatic ANY mode switching when single tag expands
  - Works in CLI search and interactive browse (shared logic)
- **TUI Enhancements** - Tag hierarchy visualization in browse mode
  - **Hierarchical Tag Tree Widget** - Visual tree with parent-child relationships
    - Automatic hierarchy inference from `:` delimiter
    - Collapsible/expandable nodes with Space key
    - Multi-select with visual checkmarks (green ✓)
    - File counts for tags, (parent) markers for inferred nodes
  - **Two-Pane Layout** - Split screen for tag selection phase
    - Left: Hierarchical tag tree with navigation
    - Right: Items list with synchronized selections
    - Bidirectional cursor synchronization
  - **Alias Display** - Shows synonyms inline in tree
    - Format: "javascript (js, es) (42 files)"
    - Tag consolidation by canonical form (deduplicates js/javascript)
    - Short tag names in hierarchy (python vs lang:python)
    - ANSI code stripping for clean rendering
  - **Smart Filtering** - Tree filters match items list
    - Both panes filter simultaneously
    - Bidirectional cursor sync across filters
    - Multi-select preserved during filter changes
    - "No matching tags" message for empty results
  - **Parent Multi-Select** - TAB on parent selects all children
    - Recursive descendant collection
    - Bulk selection for entire subtrees
  - **Live CLI Preview** - Status bar shows equivalent CLI command
    - Color-coded syntax highlighting
    - Real-time updates as tags selected
    - Educational tool for learning CLI syntax
- **Filter Integration** - Hierarchies and aliases work with saved filters
  - Filters store user-provided tags (not expanded)
  - Expansion happens at query time (always uses latest schema)
  - `--no-hierarchy` flag preserved in SearchParams
  - Filter loading respects hierarchy settings
- **Hierarchical Aliases** - Aliases can point to hierarchical tags
  - Example: `ts → lang:typescript` (allowed)
  - Aliases themselves cannot contain `:` (validation enforced)
  - Powerful patterns for namespace consolidation

### Changed

- **Tag Validation** - Reserved `:` delimiter for hierarchies
  - Canonical tags can contain `:` for hierarchies
  - Alias names cannot contain `:` (enforced by schema)
  - Proper error messages for invalid tag syntax
- **Query Logic** - Centralized tag expansion in database layer
  - Single source of truth in `db/query.rs`
  - Consistent behavior across CLI and TUI
  - Deduplication via HashSet

### Fixed

- **Filter Mode Preservation** - Critical bug fix for saved filters
  - Filters now correctly preserve ANY/ALL tag mode
  - CLI mode flags (`--any-tag`, `--all-tags`) properly override filter defaults
  - `SearchParams::merge()` tracks explicit mode flags
- **Circular Reference Prevention** - Comprehensive cycle detection
  - Direct cycles (A→B→A) blocked
  - Indirect cycles (A→B→C→A) blocked
  - Long chain cycles (5+ hops) blocked
  - Case-insensitive detection
  - Self-references allowed (harmless)

### Testing

- 294 library tests passing
- 37 integration tests passing
- Comprehensive circular reference testing (10 scenarios)
- Manual TUI testing:
  - Tag tree navigation and expansion
  - Alias display and consolidation
  - Filtering and cursor synchronization
  - Multi-select preservation
  - Parent node selection

---

## [0.8.0] - 2026-01-06

### Changed

#### Ratatui TUI Migration (Complete)
- **Replaced skim with ratatui + nucleo** - Complete TUI overhaul
  - Removed `skim` dependency entirely
  - Added `nucleo` for fast fuzzy matching (10-100x faster for large lists)
  - Added `ratatui` for modern terminal UI framework
  - Added `crossterm` for cross-platform terminal handling
- **Removed feature flags** - Simplified build configuration
  - Removed `skim-tui` and `ratatui-tui` feature flags
  - TUI dependencies are now always included (non-optional)
  - Only remaining feature: `syntax-highlighting` (default, uses syntect)
- **New TUI features**
  - Modal text input with fuzzy autocomplete for tag entry
  - Confirmation dialogs for destructive actions (delete from DB)
  - F1 help overlay showing all keybinds
  - F2 refine search overlay for editing search criteria in-TUI
  - Native ratatui syntax-highlighted preview (no ANSI parsing)
  - Shift+↑/↓ for preview pane scrolling
- **Code cleanup**
  - Deleted `src/ui/skim_adapter.rs` (~400 lines removed)
  - Removed all `#[cfg(feature = "...")]` conditional compilation
  - Renamed `ratatui_bindings()` → `bindings()` in keybind config
  - Applied `cargo fmt --all` formatting

### Removed

- `skim` dependency
- `skim-tui` feature flag
- `ratatui-tui` feature flag (now always included)
- `skim_bindings()` method from KeybindConfig

---

## [Unreleased]

### Added

#### Preview Pane Feature (Phase 4 Complete)
- **Interactive File Preview** - View file content in fuzzy finder before selecting
  - Real-time preview while navigating through files
  - Automatic text file detection with UTF-8 validation
  - Binary file metadata display (size, modified time, permissions, type)
  - Empty file detection and handling
- **Syntax Highlighting** - Hybrid approach for best highlighting quality
  - Primary: Uses `bat` command if installed (respects user's bat config/theme)
  - Fallback: Built-in `syntect` library with default theme
  - Final fallback: Plain text if syntax highlighting disabled
  - ANSI color codes properly rendered in preview pane
- **Feature Flag** - `syntax-highlighting` feature (enabled by default)
  - Compile without syntect: `cargo build --no-default-features`
  - Optional syntect 5.2 dependency
  - Zero impact on binary size when disabled
- **Configuration** - Fully configurable preview behavior
  - Enable/disable preview globally via config
  - Max file size limit (default: 5MB)
  - Max lines to display (default: 50)
  - Toggle syntax highlighting
  - Show/hide line numbers
  - Preview position: right, bottom, or top
  - Preview width percentage (0-100)
- **CLI Flags** - Override preview settings per command
  - `--no-preview` - Disable preview for this session
  - `--preview-lines <LINES>` - Override max lines
  - `--preview-position <POSITION>` - Override position (right/bottom/top)
  - `--preview-width <PERCENT>` - Override width percentage
- **Performance** - Efficient preview generation with caching
  - Moka cache with 300s TTL and 1000 item capacity
  - Cached previews for fast navigation
  - File size checks before reading
  - Lazy loading (preview generated on demand)
- **UI Abstraction** - Backend-agnostic preview system
  - `PreviewProvider` trait for custom preview implementations
  - `PreviewText` type with ANSI metadata tracking
  - Native ratatui styled preview with syntax highlighting
  - Easy to add new preview providers

#### Virtual Tags Feature (Complete)
- **Dynamic Metadata Queries** - Query files by filesystem metadata without database storage
  - Time-based queries: modified, created, accessed timestamps
  - Size-based queries: size categories (tiny/small/medium/large/huge), ranges, specific sizes
  - Extension queries: specific extensions (.rs, .md) and type categories (source, document, config, image, archive)
  - Location queries: directory path, glob patterns, depth levels
  - Permission queries: executable, readable, writable, read-only
  - Content queries: line count ranges
  - Git queries: tracked, modified, staged, untracked, stale files
- **Virtual Tag CLI** - Seamless integration with search and browse commands
  - `-v` / `--virtual-tag <VTAG>` - Add virtual tag filter
  - `--any-virtual` - Match ANY virtual tag (OR logic)
  - `--all-virtual` - Match ALL virtual tags (AND logic, default)
  - Combine virtual tags with regular tags seamlessly
- **Virtual Tag Parser** - Parse human-friendly virtual tag syntax
  - Time formats: `modified:today`, `created:this-week`, `accessed:last-7-days`
  - Size formats: `size:>1MB`, `size:<100KB`, `size:empty`, `size:large`
  - Extension formats: `ext:.rs`, `ext-type:source`
  - Path formats: `dir:src`, `path:src/**/*.rs`, `depth:3`
  - Permission formats: `perm:executable`, `perm:readonly`
  - Content formats: `lines:>100`, `lines:10-50`
  - Git formats: `git:tracked`, `git:modified`, `git:stale`
- **Virtual Tag Evaluator** - Efficient metadata evaluation with caching
  - Metadata cache with configurable TTL (default 300s)
  - Parallel evaluation using rayon for performance
  - Lazy evaluation (only checks files already in database)
  - Graceful error handling for missing files or unsupported metadata
- **Filter Integration** - Virtual tags fully integrated with saved filters
  - Save filters containing virtual tags
  - Load and apply virtual tag filters
  - Display virtual tags in `filter show` command
  - Combine saved filters with additional virtual tags
- **FilterCriteria Builder Pattern** - Clean, fluent API for filter construction
  - `FilterCriteria::builder()` - Create new builder
  - Chainable methods: `.tags()`, `.file_patterns()`, `.virtual_tags()`, etc.
  - Type-safe construction with compile-time guarantees
  - Simplified test code and improved maintainability
- **Configuration** - Customizable virtual tag behavior
  - Size category thresholds (tiny, small, medium, large, huge)
  - Extension type mappings (source, document, config, image, archive)

### Improved

- **Test Infrastructure** - Automatic cleanup for integration tests
  - `TestDb` and `TestFile` wrappers with Drop-based cleanup
  - Prevents leftover test files and directories
  - Panic-safe cleanup (works even when tests fail)
  - All integration tests migrated to new pattern

### Fixed

- Clippy warnings reduced with pedantic and nursery lints
  - Derived Default for UiBackend enum
  - Made config helper functions const
  - Fixed redundant closures and nested if statements
  - Improved doc comments with proper backticks
  - Better error handling patterns

  - Time thresholds (recent, stale)
  - Git integration toggle
  - Metadata cache TTL
- **12 Virtual Tag Types** - Comprehensive metadata coverage
  - Modified, Created, Accessed (time conditions)
  - Size (categories, ranges, comparisons)
  - Extension (specific extensions)
  - ExtensionType (type categories)
  - Directory (parent directory path)
  - Path (glob pattern matching)
  - Depth (directory depth)
  - Permission (file permissions)
  - Lines (line count ranges)
  - Git (Git status, tracked/untracked/modified/staged/stale)
  - Empty (zero-byte files)
- **Documentation** - Comprehensive virtual tags documentation
  - README.md section with examples for all virtual tag types
  - Configuration examples
  - Usage patterns and best practices
  - Integration with saved filters
  - Performance characteristics

#### Saved Filters Feature (Complete)
- **Filter Management CLI** - Complete command-line interface for filter operations
  - `tagr filter create <name>` - Create new filter with tags, patterns, exclusions
  - `tagr filter list` / `ls` - List all saved filters with descriptions
  - `tagr filter show <name>` - Show detailed filter information
  - `tagr filter delete <name>` / `rm` - Delete filter (with confirmation)
  - `tagr filter rename <old> <new>` / `mv` - Rename existing filter
  - `tagr filter export [names...]` - Export filters to TOML file
  - `tagr filter import <file>` - Import filters with conflict resolution
  - `tagr filter stats` - Show filter usage statistics (stub for future implementation)
- **Search & Browse Integration** - Full integration with search and browse commands
  - `--filter` / `-F <name>` - Load and apply saved filter
  - `--save-filter <name>` - Save current search/browse as filter
  - `--filter-desc <desc>` - Add description when saving filter
  - Automatic filter criteria merging with CLI arguments
  - Usage statistics tracking on filter load
- **SearchParams Conversions** - Idiomatic From trait implementations
  - `impl From<SearchParams> for FilterCriteria` - Convert CLI args to filter
  - `impl From<&FilterCriteria> for SearchParams` - Convert filter to CLI args
  - `SearchParams::merge()` - Merge filter criteria with additional CLI arguments
- **CLI Integration** - Filter subcommands integrated into main CLI
  - `Commands::Filter` variant added to main command enum
  - `FilterArgs` struct with flatten for search/browse commands
  - All filter commands properly routed through `commands/filter.rs`
  - Short aliases for common operations (`ls`, `rm`, `mv`)
- **Export/Import Features** - Share and backup filters
  - Export to file with `--output` flag or stdout
  - Import with conflict resolution: `--overwrite` or `--skip-existing`
  - Selective export by filter names
- **User Experience**
  - Force delete with `--force` / `-f` flag
  - Detailed output with creation dates and usage stats
  - Comprehensive descriptions for each filter
  - Usage tracking (last_used, use_count) in metadata
  - Warning when saving browse filter with no criteria

#### Saved Filters and Bookmarks (Foundation)
- **Filter Storage Infrastructure** - Core types and operations for saved search filters
- `FilterManager` API - Idiomatic Rust interface for filter management
- `FilterCriteria` - Stores search parameters (tags, patterns, modes, exclusions, regex flags)
- `Filter` - Complete filter with metadata (name, description, created, last_used, use_count)
- `FilterStorage` - TOML-based persistent storage at `~/.config/tagr/filters.toml`
- Filter CRUD operations: create, get, update, delete, rename, list
- Filter validation - Name rules (alphanumeric, hyphens, underscores, max 64 chars)
- Criteria validation - At least one tag or file pattern required
- Filter export/import - Share filters with conflict resolution (overwrite/skip-existing)
- Usage statistics tracking - Automatic use_count and last_used updates
- Auto-backup functionality - Backup before saves (configurable)
- Comprehensive error handling with `FilterError` type
- 10 unit tests covering all CRUD operations and edge cases

#### Interactive Browse Mode
- Two-stage fuzzy finder for tag and file selection
- Multi-select support via TAB key for both tags and files
- Inline tag display: files shown with their tags (`file.txt [tag1, tag2]`)
- Fuzzy matching for quick filtering
- Browse mode is now the default command when no command is specified
- Advanced browse mode with AND/OR search logic selection
- `search` module with `browse()` and `browse_advanced()` functions
- `BrowseResult` struct containing selected tags and files
- Demo example (`examples/browse_demo.rs`) with test data
- Keyboard-driven interface for efficient navigation

#### Cleanup Feature
- `cleanup` command to maintain database integrity
- Detection of missing files (in database but not on filesystem)
- Detection of untagged files (files with no tags)
- Interactive prompts for each problematic file
- Response options: yes/no/yes-to-all/no-to-all
- Automated cleanup via piped responses
- Quiet mode support with `-q` flag
- Summary report showing total issues found and actions taken

#### Database Management
- Multiple database support
- `db add <name> <path>` - Add new database
- `db list` - List all configured databases
- `db set-default <name>` - Set default database
- `db remove <name>` - Remove database from config
- Configuration file at `~/.config/tagr/config.toml`
- First-time setup wizard for interactive configuration
- Platform-specific default paths (Linux, macOS, Windows)

#### Library Interface
- `lib.rs` exposing all modules for use as a library
- Public API for database operations
- Public API for interactive search/browse
- Example usage in documentation

### Changed

#### Performance Improvements - Multi-Tree Architecture
- **100-1000x faster tag queries** using reverse indexing
- Migrated from single sled tree to multiple trees:
  - `files` tree: file → tags mapping
  - `tags` tree: tag → files reverse index
- `find_by_tag()`: O(n) → O(1) direct lookup
- `list_all_tags()`: O(n) → O(k) iteration (k = unique tags)
- `find_by_all_tags()`: O(n) → O(k) set intersection
- Automatic index maintenance on insert/update/delete
- Helper methods: `add_to_tag_index()`, `remove_from_tag_index()`

#### Database API Enhancements
- `insert_pair()` now maintains reverse index automatically
- `remove()` cleans up reverse index entries
- Added `find_by_all_tags()` - AND query (files with all specified tags)
- Added `find_by_any_tag()` - OR query (files with any of the specified tags)
- Added `add_tags()` - Add tags without removing existing ones
- Added `remove_tags()` - Remove specific tags while keeping others
- Added `contains()` - Check if file exists in database
- Added `count()` - Get total entry count
- Auto-flush on drop for data durability

#### CLI Improvements
- Browse mode is now the default command
- Short aliases for common commands (e.g., `b` for browse, `c` for cleanup)
- Better error messages and user feedback
- Quiet mode (`-q`) for suppressing informational output

### Fixed
- Proper UTF-8 path handling with error messages
- Automatic cleanup of reverse index when updating file tags
- Removal of empty tag entries from reverse index
- Prevention of duplicate tags when using `add_tags()`

## Architecture Changes

### Before: Single Tree Implementation

```
Database {
    db: Db  // Single default tree
}

Operations:
- find_by_tag(): O(n) - scan every file
- list_all_tags(): O(n) - scan every file
```

### After: Multi-Tree Implementation

```
Database {
    db: Db,      // Database handle
    files: Tree, // file → tags
    tags: Tree   // tag → files (reverse index)
}

Operations:
- find_by_tag(): O(1) - direct lookup in tags tree
- list_all_tags(): O(k) - iterate tags tree (k = unique tags)
```

### Performance Benchmark

**Scenario**: 10,000 files, 100 unique tags, find all files tagged "rust"

| Metric | Before | After | Improvement |
|--------|--------|-------|-------------|
| Iterations | 10,000 | 1 | 10,000x |
| Time | ~50ms | ~0.1ms | 500x |
| Complexity | O(n) | O(1) | Direct lookup |

### Trade-offs

**Advantages**:
- ✅ Much faster queries (O(1) vs O(n))
- ✅ Scalable (performance independent of total files)
- ✅ Tag listing is instant
- ✅ Efficient complex queries (AND/OR operations)

**Storage**:
- ~50% more storage (files tree + tags tree)
- Negligible for most use cases
- ~1.5 MB for 10,000 files vs ~1 MB

## Migration Guide

### From Single Tree to Multi-Tree

If you have an existing database using the old single-tree approach:

1. Backup your existing database
2. Create new database instance (auto-creates trees)
3. Read all pairs from old format
4. Insert into new format (index built automatically)

```rust
let old_db = sled::open("old_db")?;
let new_db = Database::open("new_db")?;

for item in old_db.iter() {
    let (key, value) = item?;
    let pair: Pair = (key.as_ref(), value).try_into()?;
    new_db.insert_pair(pair)?;  // Automatically builds index
}
```

## Dependencies

### Added
- `chrono = "0.4"` - Date/time handling for filter timestamps
- `nucleo = "0.5"` - Fast fuzzy matching engine
- `ratatui = "0.30"` - Terminal UI framework
- `crossterm = "0.28"` - Cross-platform terminal handling
- `bincode = "2.0.0-rc.3"` - Binary serialization
- `sled = "0.34"` - Embedded database
- `clap = "4.5"` - CLI parsing
- `thiserror = "1.0"` - Error handling

## Documentation

### New Documentation Files (Consolidated into README.md)
- Interactive browse mode usage
- Cleanup feature documentation
- Database wrapper API guide
- First-time setup instructions
- Architecture and performance comparisons
- Quick start guide
- Search module implementation details

All documentation has been consolidated into `README.md` and this `CHANGELOG.md`.

## Known Issues

None at this time.

## Future Enhancements

### Potential Improvements
- Tag statistics - Show file count per tag inline
- Recent selections - Remember last used tags
- Custom search queries - Complex tag expressions (e.g., `(rust AND web) OR python`)
- Export results - Save selections to file
- Actions on selection - Open, copy, delete files directly from browse mode
- Tag counts - Store tag→count mapping for O(1) statistics
- Prefix search - Use key prefixes for tag autocomplete
- Batch operations - Transaction support for bulk updates
- LRU cache - In-memory cache for frequently accessed tags
- Bloom filters - Quick existence checks before tree lookups
- File watching - Auto-detect when files are deleted/moved
- Tag aliases - Define shortcuts for common tag combinations
- Tag hierarchies - Support parent/child tag relationships

## Breaking Changes

None in this release. The multi-tree architecture is backward compatible at the API level, though the internal database format has changed.

---

## Development History

### Implementation Summary

This project evolved through several key phases:

1. **Initial Implementation** - Basic tag storage with single sled tree
2. **Performance Optimization** - Multi-tree architecture with reverse indexing
3. **Interactive Interface** - Fuzzy finding with skim integration
4. **Maintenance Features** - Cleanup command and database management
5. **Library Development** - Exposed public API for use as a library
6. **Documentation** - Comprehensive guides and examples
7. **Ratatui Migration** - Replaced skim with nucleo + ratatui for modern TUI

### Code Statistics

- **Core Modules**: 5 (lib, main, cli, config, db, search)
- **Database Trees**: 2 (files, tags)
- **CLI Commands**: 15+
- **Example Programs**: 2 (browse_demo, custom_frontend)
- **Lines of Code**: ~3000+ (excluding comments)

### Contributors

- xerinox - Initial implementation and development

## Support

For issues, questions, or contributions, please visit the repository:
https://github.com/xerinox/tagr
