# tagr
<img width="1886" height="1114" alt="image" src="https://github.com/user-attachments/assets/42972348-825b-4a3e-9164-62dc8144ca9f" />

A fast, interactive command-line tool for organizing files with tags using fuzzy finding and persistent storage.

## Features

- **File notes with markdown** - Attach rich text documentation to files with append-style timestamps.
- **Tag hierarchies and aliases** - Organize tags with parent:child relationships and synonyms.
- **Interactive fuzzy finding** - Browse tags and select files inside a modern customizable terminal user interface.
- **Real-time action keybinds** - Modify, rename, or wipe tags directly within the fuzzy finder interface.
- **Saved filters** - Store multi-criteria queries for automatic recall.
- **Virtual tags** - Query files dynamically by metadata (size, edit time, git status, extension, permissions).
- **Background Watch Mode** - File system watcher daemon for automated rule-based file tagging.
- **Multi-tree Reverse Index** - 100-1000x faster reverse lookup search queries.

---

## Detailed Feature Documentation

To keep the overview concise, full capabilities are split into dedicated feature guides:

* **Saved Queries**: Read the [docs/saved-filters.md](docs/saved-filters.md) guide for creating and sharing search configurations.
* **File Notes**: Learn about append-style Markdown logs in the [docs/file-notes.md](docs/file-notes.md) guide.
* **Metadata Queries**: Learn how to filter by size, extension, dates, or git status in the [docs/virtual-tags.md](docs/virtual-tags.md) guide.
* **Batch Operations**: Read the [docs/bulk-operations.md](docs/bulk-operations.md) guide to tag, remap, or untag directories of files.
* **Command Reference**: Consult the full [docs/cli-reference.md](docs/cli-reference.md) for detailed flag information.

---

## Quick Start

### Installation

```bash
git clone https://github.com/xerinox/tagr.git
cd tagr
cargo build --release
```

### First-Time Setup

When you run `tagr` for the first time, it will guide you through an interactive setup:

```bash
./target/release/tagr
```

You'll be prompted for:
- **Database name** (default: "default")
- **Database location** (default: `~/.local/share/tagr/<database_name>`)

The configuration is saved to `~/.config/tagr/config.toml`.

For a complete list of commands and flags, see the CLI reference in `docs/cli-reference.md`.

### Shell Completions

Tagr provides shell completions for bash, zsh, fish, PowerShell, and elvish.

**Static completions** (always available):

```bash
# Generate completion script for your shell
tagr completions bash > ~/.local/share/bash-completion/completions/tagr
tagr completions zsh > ~/.zfunc/_tagr
tagr completions fish > ~/.config/fish/completions/tagr.fish
```

**Dynamic completions** (context-aware, behind feature flag):

Build with `--features dynamic-completions` for intelligent completions that suggest:
- **Tags** from your database when using `-t/--tag`
- **Virtual tags** with syntax hints when using `-v/--virtual-tag`
- **Filter names** when using `-F/--filter`
- **Database names** when using `--db`

```bash
# Build with dynamic completions
cargo build --release --features dynamic-completions

# Setup for bash (add to ~/.bashrc)
source <(COMPLETE=bash tagr)

# Setup for zsh (add to ~/.zshrc)
source <(COMPLETE=zsh tagr)

# Setup for fish
COMPLETE=fish tagr | source
```

The completion cache is automatically updated when tags, filters, or databases change.

### Basic Usage

```bash
# Tag some files
tagr tag README.md documentation markdown
tagr tag src/main.rs rust code source
tagr tag src/lib.rs rust code library

# Search for files by tag (non-interactive)
tagr search -t rust

# List all tags
tagr list tags

# Remove tags from a file
tagr untag README.md markdown

# Clean up missing files
tagr cleanup
```



```bash
# Launch browse mode
tagr
# or
tagr browse
```

**Stage 1: Tag Selection**
- Displays all available tags in the database
- **Multi-select enabled** via TAB key
- Fuzzy matching for quick filtering
- Press Enter to proceed to file selection

**Stage 2: File Selection**
- Shows all files matching ANY of the selected tags
- Files displayed with their tags inline: `file.txt [tag1, tag2, tag3]`
- **Multi-select enabled** via TAB key
- Fuzzy matching for filtering
- Press Enter to confirm final selection

### 2. Pre-Populated Browse with Query Arguments

You can now pre-populate the browse mode with search criteria, skipping the tag selection stage:

```bash
# Browse with a general query (searches both filenames and tags)
tagr browse documents

# Browse files with specific tags
tagr browse -t rust -t programming

# Browse with file patterns (glob syntax)
tagr browse -f "*.txt" -f "*.md"

# Exclude specific tags
tagr browse -t documents -e archived

# Combine multiple criteria
tagr browse -t rust -f "src/*.rs" -e test
```

This behaves exactly like `tagr search`, but instead of printing results directly, it opens the fuzzy finder pre-filtered with matching files. You can then:
- Further filter with fuzzy matching
- Multi-select files
- Execute commands on selections

### Execute Commands on Selections

```bash
# Open selected files in your editor
tagr browse documents -x "nvim {}"

# Copy selected files
tagr browse -t images -x "cp {} /backup/"

# Preview file content
tagr browse -t config -x "cat {}"
```

### Keyboard Controls

| Key | Action |
|-----|--------|
| ↑↓ or Ctrl+J/K | Navigate |
| TAB | Select/deselect (multi-select) |
| Space | Expand/collapse tree nodes (tag phase) |
| Ctrl+N | Edit note for selected file  |
| Alt+N | Toggle file/note preview |
| Ctrl+L | Show file details modal (metadata + tags + note) |
| Enter | Confirm and proceed |
| ESC | Cancel |
| Type | Filter via fuzzy matching |

## Tag Hierarchies and Aliases

**New in v0.9.0** - Tagr now supports hierarchical tag organization and tag aliases (synonyms).

### Tag Hierarchies

Organize tags using parent:child relationships with the `:` delimiter:

```bash
# Tag files with hierarchical tags
tagr tag src/main.rs lang:rust
tagr tag docs/tutorial.md lang:rust:beginner
tagr tag app.py lang:python

# Search automatically expands to parent tags
tagr search -t lang:rust     # finds both lang:rust and lang:rust:beginner
tagr search -t lang           # finds all lang:* tags

# Disable hierarchy expansion
tagr search -t lang:rust --no-hierarchy

# Browse with hierarchical tag tree
tagr browse  # See visual tree: lang → rust → beginner

# List tags in tree format
tagr tags list --tree
```

**How it works:**
- Tags with `:` create parent-child relationships (e.g., `lang:rust:async`)
- Searching for a child tag automatically includes parent tags
- TUI displays tags in a collapsible tree structure
- Use `--no-hierarchy` flag to disable expansion

### Tag Aliases

Create synonyms for tags to consolidate similar tags and simplify tagging:

```bash
# Create aliases
tagr alias add js javascript
tagr alias add py python
tagr alias add ts typescript

# Tag files using aliases (automatically canonicalized)
tagr tag app.js js                    # stores as "javascript"
tagr tag script.py py                  # stores as "python"

# Search using any alias
tagr search -t js                      # finds files tagged "javascript"
tagr search -t javascript              # same result

# Aliases work with hierarchical tags
tagr alias add rust lang:rust
tagr tag main.rs rust                  # stores as "lang:rust"

# List all aliases
tagr alias list

# Show aliases for a specific tag
tagr alias show javascript             # displays: js, es6, ecmascript

# Remove an alias
tagr alias remove js

# Opt out of canonicalization when tagging
tagr tag file.txt js --no-canonicalize
```

**How it works:**
- Aliases automatically canonicalize to the target tag
- Database stores only canonical tags (saves space)
- Search and browse expand to all synonyms
- TUI shows aliases inline: "javascript (js, es6) (42 files)"
- Circular references are prevented

### Tag Schema Storage

Tag aliases and hierarchies are stored in `~/.config/tagr/tag_schema.toml`:

```toml
# Tag aliases (synonyms)
[aliases]
js = "javascript"
py = "python"
ts = "lang:typescript"

# Schema automatically enforces:
# - Circular reference prevention
# - Reserved delimiter validation (:)
# - Case-insensitive matching
```

### Examples

```bash
# Traditional browse
tagr

# Browse documents matching pattern
tagr browse -f "*.txt"

# Browse Rust files with specific tag, then edit
tagr browse -t tutorial -f "*.rs" -x "nvim {}"

# Browse any doc format, excluding archived
tagr browse -t documentation -e archived

# Browse with experimental action menu (Phase 1)
tagr browse
```

### Action Menu (Experimental)

**New in v0.5.0** - Phase 1 of advanced keybinds feature

An interactive action menu can appear after file selection, depending on your
configuration and workflow. No special CLI flag is required.

After selecting files, you'll see an action menu with these options:

- **Continue (use selections)** - Exit with your selected files
- **Add tags to selected files** - Interactively add tags to all selected files
- **Remove tags from selected files** - Choose tags to remove from selected files
- **Delete from database** - Remove files from the database (with confirmation)
- **Cancel (re-select)** - Go back and select different files

**Why experimental?** This is Phase 1 of the keybinds feature, using a post-selection menu approach. Future phases will add real-time keybinds within the fuzzy finder, additional file operations (open, copy, edit), and full keybind customization.

### Real-Time Keybinds

**New in v0.5.0** - Real-time action keybinds are now enabled by default in browse mode!

Browse mode now features real-time action keybinds directly within the fuzzy finder:

```bash
tagr browse
```

Trigger actions immediately while browsing without exiting the finder:

| Keybind | Action | Description |
|---------|--------|-------------|
| **Ctrl+T** | Add Tag | Add tags to selected files and continue browsing |
| **Ctrl+R** | Remove Tag | Remove tags from selected files and continue browsing |
| **Ctrl+D** | Delete from DB | Remove files from database (with confirmation) |
| **Enter** | Confirm | Exit with selected files |
| **ESC** | Cancel | Abort and exit browse mode |

**Workflow Example:**
1. Browse and select files with TAB
2. Press **Ctrl+T** to add tags (e.g., "urgent")
3. Continue browsing the same file list
4. Press **Ctrl+R** to remove unwanted tags
5. Press **Enter** to confirm final selection

**Keybind Customization:**
Configure keybinds in `~/.config/tagr/keybinds.toml`:

```toml
[keybinds]
add_tag = "ctrl-t"
remove_tag = "ctrl-r"
delete_from_db = "ctrl-d"
# Set to "none" to disable an action
# edit_tags = "none"
```

Future enhancements will add more actions (edit tags, open files, copy paths), better visual feedback, and help overlay.

## Preview Pane

The preview pane displays file content when browsing files in interactive mode, helping you make informed selections without leaving the fuzzy finder.

### Features

- **Syntax highlighting** - Automatically highlights code files using `bat` (if installed) or built-in `syntect`
- **Smart fallbacks** - Plain text preview if syntax highlighting unavailable or disabled
- **Binary file metadata** - Shows file size, modification time, permissions for non-text files
- **ANSI color support** - Preserves syntax highlighting colors in the preview
- **Configurable** - Control preview size, position, and features

### Usage

Preview is enabled by default when browsing:

```bash
# Browse with preview (default)
tagr browse

# Disable preview
tagr browse --no-preview

# Customize preview lines
tagr browse --preview-lines 100

# Change preview position (right/bottom/top)
tagr browse --preview-position bottom

# Adjust preview width (percentage)
tagr browse --preview-width 60
```

### Configuration

Add preview settings to `~/.config/tagr/config.toml`:

```toml
[preview]
enabled = true
max_file_size = 5242880  # 5MB
max_lines = 50
syntax_highlighting = true
show_line_numbers = true
preview_position = "right"  # right, bottom, or top
preview_width_percent = 50  # 0-100
```

### Syntax Highlighting

Preview uses a hybrid approach for best results:

1. **First choice**: Uses `bat` command (if installed) - respects your bat theme and config
2. **Fallback**: Uses built-in `syntect` library with default theme
3. **Final fallback**: Plain text if syntax highlighting disabled

To install `bat` for better syntax highlighting:

```bash
# macOS
brew install bat

# Ubuntu/Debian
apt install bat

# Arch Linux
pacman -S bat

# Cargo
cargo install bat
```

Syntax highlighting can be disabled via:
- Configuration: `syntax_highlighting = false` in config.toml
- CLI flag: `--no-preview` when browsing
- Compile-time: `cargo build --no-default-features` (removes syntect dependency)

## Commands (Quick Overview)

This section gives a short overview of common commands. The full, detailed
reference with all flags and advanced examples lives in `docs/cli-reference.md`.

### File Operations

```bash
# Tag a file (adds tags; no duplicates)
tagr tag <file> <tags...>

# Remove specific tags from a file
tagr untag <file> <tags...>

# List tags and files
tagr list tags
tagr list files
```

### Search & Browse

```bash
# Interactive browse (default)
tagr           # same as: tagr browse

# Search non-interactively
tagr search -t rust

# Browse with query/tags/patterns
tagr browse documents
tagr browse -t rust -t tutorial
tagr browse -f "*.txt" -f "*.md"
```
```

## Advanced Search

`tagr search` supports flexible multi-criteria queries with independent AND/OR
logic for tags, file patterns, and virtual tags. This is the core mechanism
behind most Tagr workflows.

High-level behavior:

- Combine multiple tags with AND/OR semantics.
- Combine multiple file patterns (glob or regex) with independent AND/OR.
- Exclude tags from the result set.
- Mix regular tags with virtual tags (size/time/path/git/etc.).

For a full set of examples and the complete option reference, see the
"Search Command" section in `docs/cli-reference.md`.

### Performance

**Regular tag operations are highly efficient:**
- **Tag lookups**: O(1) via reverse index  
- **Complex queries**: < 20ms for 10,000 files
- **Pattern filtering**: Only on result set, not entire database

**Virtual tag operations:**
- Evaluation is O(n) on candidate files (parallel with rayon)
- Best performance when combined with regular tags first
- Metadata caching reduces filesystem calls
- Example: `tagr search -t rust -v modified:today` evaluates only rust-tagged files

### Database Management

```bash
# List databases
tagr db list

# Add new database
tagr db add <name> <path>

# Set default database
tagr db set-default <name>

# Remove database
tagr db remove <name>
```

## See Feature Guides for Details

Detailed guides for these advanced features can be found in the `docs/` directory:
- [docs/saved-filters.md](docs/saved-filters.md) - Saved Filters
- [docs/file-notes.md](docs/file-notes.md) - File Notes
- [docs/virtual-tags.md](docs/virtual-tags.md) - Virtual Tags & Metadata Queries
- [docs/bulk-operations.md](docs/bulk-operations.md) - Bulk Tagging, Remapping, and Deletion

## Watch Mode (Daemon)

Tagr includes a background daemon that watches your filesystem and automatically tags files based on configurable rules.

### Quick Start

```bash
# Add a watch rule: tag all .rs files under ~/projects with "rust" and "code"
tagr watch add "~/projects/**/*.rs" -t rust -t code

# Start the daemon (foreground)
tagr watch start

# Start as a background daemon
tagr watch start --daemon

# Check daemon status
tagr watch status

# Stop the daemon
tagr watch stop
```

### Watch Rules

Rules are stored in `~/.config/tagr/watch.toml`. Each rule has:
- **patterns** — glob patterns to match files
- **tags** — tags to apply when files match
- **filter** — optional saved filter name for additional criteria
- **vtags** — optional virtual tags to apply
- **filter_by_tags** — only apply to files that already have these tags

```bash
# List all rules
tagr watch list

# Remove rule #2
tagr watch remove 2
```

### Logging

The daemon uses structured logging via `env_logger`. Control verbosity with the `RUST_LOG` environment variable:

```bash
# Default: info level (startup, shutdown, config changes, watch directories)
tagr watch start

# Debug: includes per-file auto-tag operations
RUST_LOG=debug tagr watch start

# Trace: maximum verbosity
RUST_LOG=trace tagr watch start

# Only errors
RUST_LOG=error tagr watch start

# Filter to tagr modules only
RUST_LOG=tagr=debug tagr watch start
```

**Log levels:**

| Level | What's logged |
|-------|--------------|
| `error` | Tag failures, spawn errors, config reload failures |
| `warn` | Missing watch roots, filter load issues, watcher errors |
| `info` | Startup/shutdown, config reload, watch directory registration |
| `debug` | Individual auto-tag operations (per-file) |

When running as a background daemon (`--daemon`), logs go to stdout/stderr of the detached process. Redirect to a file for persistent logging:

```bash
RUST_LOG=info tagr watch start --daemon 2>&1 | tee ~/.local/share/tagr/daemon.log
```

### TUI Integration

Press **F3** in browse mode to open the Watch Status modal, which shows:
- Daemon status (running/stopped), PID, store mode, socket path
- All configured watch rules with their patterns, tags, and filters

## Architecture

### Reverse Index with Sled Trees

tagr uses **multiple sled trees** for efficient bidirectional lookups:

#### Files Tree
```
Key: file_path (UTF-8 string as bytes)
Value: Vec<String> (bincode-encoded list of tags)

Example:
"file1.txt" → ["rust", "programming", "tutorial"]
"file2.txt" → ["rust", "advanced"]
```

#### Tags Tree (Reverse Index)
```
Key: tag (UTF-8 string as bytes)
Value: Vec<String> (bincode-encoded list of file paths)

Example:
"rust"        → ["file1.txt", "file2.txt", "file4.txt"]
"programming" → ["file1.txt", "file3.txt", "file4.txt"]
```

### Performance Benefits (Regular Tags)

| Operation | Before (Single Tree) | After (Multi-Tree) | Speedup |
|-----------|---------------------|-------------------|---------|  
| `find_by_tag("rust")` | O(n) - scan all files | O(1) - direct lookup | **100-1000x** |
| `list_all_tags()` | O(n) - scan all files | O(k) - iterate tags | **100x** |
| `find_by_all_tags(...)` | O(n) - scan all files | O(k) - set intersection | **100x** |

**Note:** Virtual tags use O(n) evaluation on candidate files with parallel processing for performance.

**Example**: For 10,000 files with 100 unique tags:
- Old: 10,000 iterations per query (~50ms)
- New: 1 iteration per query (~0.1ms) - **500x faster!**

### Module Structure

```
src/
├── types/          # Layer 0: Newtypes (TagName, TagrPath, FilterName, Pair, QueryCriteria)
├── store/          # Layer 1–2: TagStore trait + DirectStore (sled), DaemonStore (IPC), MockStore
├── query/          # Layer 3: Unified query pipeline (execute, hierarchy, patterns)
├── browse/         # Layer 4: BrowseSession (owns Arc<dyn TagStore>, QueryCriteria)
├── commands/       # Layer 5: CLI command implementations (tag, search, note, bulk, etc.)
├── ui/             # Layer 5: TUI abstraction (traits) + ratatui adapter
├── daemon/         # Layer 5: Watch daemon (tokio, IPC server, file watcher)
├── db/             # Embedded sled database (used by DirectStore)
├── ipc/            # Wire protocol (wincode framing, typed messages)
├── filters/        # Saved filter CRUD + persistence (filters.toml)
├── vtags/          # Virtual tags (parser, evaluator, cache — computed from filesystem metadata)
├── schema/         # Tag schema (aliases, hierarchy config)
├── config/         # Configuration management (platform paths, setup wizard)
├── keybinds/       # Keybind config + action executor
├── completions/    # Shell completion system (bash, zsh, fish, PowerShell, elvish)
├── patterns/       # Pattern builder (tag/file pattern validation)
├── preview/        # File preview generation (syntax highlighting)
├── watch/          # Watch rule config types
├── discovery/      # File discovery traits
├── output/         # Output formatting (StatusBar, StdoutWriter)
├── cli.rs          # clap structs + CLI argument parsing
├── lib.rs          # Library root, re-exports
└── main.rs         # Entry point + command dispatch
```

**Layer rule**: Lower layers never import higher layers. `types/` → `store/` → `query/` → `browse/` → `commands/`.

## Configuration

Configuration file location: `~/.config/tagr/config.toml`

```toml
default_database = "default"

[databases]
default = "~/.local/share/tagr/default"
```

### Default Locations

- **Linux**: `~/.local/share/tagr/`
- **macOS**: `~/Library/Application Support/tagr/`
- **Windows**: `C:\Users\<username>\AppData\Local\tagr\`

## Contributing

We welcome pull requests! To understand our strict safe Rust conventions and architecture policies, please consult our contribution standards outlined in the [CONTRIBUTING.md](CONTRIBUTING.md) guide.

## License

This project is licensed under the MIT License—see the [LICENSE](LICENSE) file for details.
