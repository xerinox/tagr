# tagr

A fast, interactive command-line tool for organizing files with tags using fuzzy finding and persistent storage.

## Features

- **Tag hierarchies and aliases** - Organize tags with parent:child relationships and create tag synonyms
- **Tag-based file organization** - Organize files using flexible tags instead of rigid folder structures
- **Interactive fuzzy finding** - Browse and select files using an intuitive fuzzy finder interface
- **Hierarchical tag tree** - Visual tree widget in TUI with collapsible nodes and multi-select
- **Preview pane** - See file content with syntax highlighting before selecting (uses bat/syntect)
- **Real-time action keybinds** - Perform tag operations directly within the fuzzy finder
- **File notes** - Attach markdown notes to files for documentation and context
- **Saved filters** - Save complex search criteria for quick recall
- **Virtual tags** - Query files by metadata (size, date, extension, permissions, git status)
- **Bulk operations** - Manage tags across many files at once
- **Database cleanup** - Maintain database integrity by removing missing files and untagged entries
- **Persistent storage** - Reliable embedded database with automatic flushing
- **Multiple databases** - Manage separate databases for different projects
- **Typed pattern validation** - Explicit flags for regex/glob with strict search semantics

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
| Ctrl+N | Edit note for selected file |
| Alt+N | Toggle file/note preview |
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

## Saved Filters

Save complex search criteria as named filters for quick recall, eliminating the need to repeatedly type complex queries.

### Why Use Filters?

Filters are perfect for searches you run frequently:
- Finding all Rust tutorial files: `tagr search -t rust -t tutorial -f "*.rs"`
- Reviewing production code: `tagr search -t rust -t production -e deprecated -e test`
- Checking documentation: `tagr search -t documentation -f "*.md" -f "*.txt" --any-file`

Instead of retyping these, save them once and recall instantly!

### Creating Filters

```bash
# Create a filter with tags
tagr filter create rust-tutorials \
  -d "Find Rust tutorial files" \
  -t rust -t tutorial \
  -f "*.rs"

# Create a filter with all criteria
tagr filter create prod-rust \
  -d "Production Rust code (no tests/deprecated)" \
  -t rust -t production --all-tags \
  -f "src/*.rs" -f "lib/*.rs" --any-file \
  -e test -e deprecated

# Create with regex
tagr filter create config-files \
  -d "All configuration files" \
  -t config \
  -f ".*\\.(toml|yaml|json)$" --regex-file
```

### Managing Filters

```bash
# List all saved filters
tagr filter list
tagr filter ls

# Show detailed filter information
tagr filter show rust-tutorials

# Rename a filter
tagr filter rename rust-tutorials rust-beginner-tutorials
tagr filter mv rust-tutorials rust-beginner-tutorials

# Delete a filter
tagr filter delete rust-tutorials
tagr filter rm rust-tutorials

# Delete without confirmation
tagr filter delete rust-tutorials --force
tagr filter rm rust-tutorials -f
```

### Using Filters with Search & Browse

Filters work seamlessly with `tagr search` and `tagr browse` commands:

```bash
# Use a saved filter
tagr search --filter rust-tutorials
tagr search -F rust-tutorials

# Load in browse mode
tagr browse --filter prod-rust
tagr browse -F prod-rust

# Combine filter with additional criteria
tagr search -F rust-tutorials -e beginner
tagr browse -F config-files -f "*.toml"

# Save current search as filter
tagr search -t rust -t tutorial -f "*.rs" --save-filter "my-rust-search"

# Save with description
tagr search -t rust -f "*.rs" --save-filter "rust-src" --filter-desc "All Rust source files"
```

### Export & Import Filters

Share filters with your team or back them up:

```bash
# Export all filters to file
tagr filter export --output team-filters.toml

# Export specific filters
tagr filter export rust-tutorials prod-rust --output rust-filters.toml

# Export to stdout
tagr filter export rust-tutorials

# Import filters
tagr filter import team-filters.toml

# Import with conflict resolution
tagr filter import team-filters.toml --overwrite      # Replace existing
tagr filter import team-filters.toml --skip-existing  # Keep existing
```

### Filter Storage

Filters are stored in TOML format at `~/.config/tagr/filters.toml`:

```toml
[[filter]]
name = "rust-tutorials"
description = "Find Rust tutorial files"
created = "2025-11-10T14:30:00Z"
last_used = "2025-11-10T15:45:00Z"
use_count = 12

[filter.criteria]
tags = ["rust", "tutorial"]
tag_mode = "all"
file_patterns = ["*.rs"]
file_mode = "any"
excludes = []
regex_tag = false
regex_file = false
virtual_tags = []
virtual_mode = "all"
```

## File Notes

Attach markdown notes to files for documentation, context, and chronological updates - without cluttering your tags.

### Why Use Notes?

- **Document file context** - Record why a file exists, what it does, or design decisions
- **Track changes over time** - Append timestamped entries as files evolve
- **Separate concerns** - Tags for organization, notes for documentation
- **Markdown support** - Write rich content with **bold**, *italic*, `code`, headers, lists, etc.
- **TUI integration** - View and edit notes without leaving the browser

### Quick Start

```bash
# Edit note in $EDITOR
tagr note edit config.toml

# Append timestamped entry (quick workflow)
tagr note add config.toml "Updated timeout settings"

# Show note content
tagr note show config.toml

# Search across all notes
tagr note search "TODO"

# List all files with notes
tagr note list
```

### Append-Style Workflow

Quickly add timestamped entries without opening an editor:

```bash
# First entry creates note with timestamp heading
tagr note add refactor.rs "Started breaking into smaller modules"
# Output:
# ### 2026-01-14 10:30
# 
# Started breaking into smaller modules

# Subsequent entries append with separator
tagr note add refactor.rs "Completed refactor, all tests passing"
# Output:
# ### 2026-01-14 10:30
# 
# Started breaking into smaller modules
# 
# ---
# ### 2026-01-14 15:45
# 
# Completed refactor, all tests passing
```

Timestamp format uses markdown H3 headings (`### YYYY-MM-DD HH:MM`) with horizontal rule separators (`---`) for clean chronological organization.

### Full Editor Control

For complex edits, reorganization, or formatting:

```bash
# Open full note in $EDITOR
tagr note edit refactor.rs

# Override editor for this command
tagr note edit README.md --editor nvim
```

You have complete control - reorganize entries, delete outdated content, or use any markdown format you prefer.

### Searching Notes

```bash
# Find files with notes containing "TODO"
tagr note search "TODO"

# Show matching content snippets
tagr note search "refactor" --show-content

# Pipe-friendly output for scripts
tagr note search "bug" --format quiet | xargs -I {} echo "File: {}"

# JSON output for structured parsing
tagr note search "urgent" --format json
```

### Managing Notes

```bash
# List all files with notes
tagr note list

# Show metadata (created/updated timestamps)
tagr note list --verbose

# Delete notes
tagr note delete old-file.txt

# Batch delete with confirmation
tagr note delete file1.txt file2.txt --yes

# Preview deletion without applying
tagr note delete *.tmp --dry-run
```

### TUI Integration

In browse mode (`tagr browse`):

- **Ctrl+N** - Edit note for selected file (suspends TUI, launches $EDITOR)
- **Alt+N** - Toggle between file preview and note preview
- **📝 icon** - Indicates files that have notes
- **" Notes Only"** - Special tag category for files with notes but no tags
- **Markdown highlighting** - Notes rendered with full syntax highlighting

Files with notes but no tags appear in a special **" Notes Only"** category at the top of the tag tree, making them easily discoverable.

### Notes Philosophy

**Notes are for documentation, not task management:**
- Simple timestamped markdown entries
- No priority, status, or due dates
- Chronicle file evolution without losing context
- Full user control via $EDITOR
- Composable with Unix tools (`grep`, `awk`, etc.)

**Storage:**
- Integrated database storage (default)
- Reliable, atomic operations alongside tags
- Future: Optional file-backed mode for git-tracked notes

### Configuration

Configure note behavior in `~/.config/tagr/config.toml`:

```toml
[notes]
storage = "integrated"        # Database storage (default)
editor = "nvim"               # Override $EDITOR
max_note_size_kb = 100       # Size limit warning
default_template = ""        # Pre-populate new notes
```

## Virtual Tags

Virtual tags are dynamically computed filters based on file metadata, requiring zero database storage. Query files by size, modification time, extension type, and more!

### Why Virtual Tags?

- **No database overhead** - Virtual tags are computed on-the-fly from filesystem metadata
- **Always accurate** - Reflects current file state without manual updates
- **Powerful queries** - Filter by properties that would be cumbersome to tag manually
- **Combine with regular tags** - Mix virtual tags with your tagged files seamlessly

### Time-based Virtual Tags

Query files by their timestamps:

```bash
# Files modified today
tagr search -v modified:today

# Files modified in the last 7 days
tagr search -v modified:last-7-days

# Files created this week
tagr search -v created:this-week

# Files accessed in the last 24 hours
tagr search -v accessed:last-24-hours

# Files modified after a specific date
tagr search -v modified:after-2025-11-01

# Files modified before a date
tagr search -v modified:before-2025-10-01

# Files modified between dates
tagr search -v modified:2025-11-01..2025-11-10
```

### Size-based Virtual Tags

Filter by file size:

```bash
# Empty files
tagr search -v size:empty

# Size categories
tagr search -v size:tiny     # < 1KB
tagr search -v size:small    # 1KB - 100KB
tagr search -v size:medium   # 100KB - 1MB
tagr search -v size:large    # 1MB - 10MB
tagr search -v size:huge     # > 10MB

# Specific sizes
tagr search -v "size:>1MB"
tagr search -v "size:<100KB"
tagr search -v "size:1MB-10MB"

# Exact size
tagr search -v size:1024
```

### Extension Virtual Tags

Filter by file extension or type:

```bash
# Specific extension
tagr search -v ext:.rs
tagr search -v ext:.md

# Extension types
tagr search -v ext-type:source    # .rs, .py, .js, .go, .cpp, .c, .java, .ts
tagr search -v ext-type:document  # .md, .txt, .pdf, .doc, .docx, .org
tagr search -v ext-type:config    # .toml, .yaml, .json, .ini, .conf
tagr search -v ext-type:image     # .png, .jpg, .jpeg, .gif, .svg, .webp
tagr search -v ext-type:archive   # .zip, .tar, .gz, .7z, .rar, .bz2
```

### Location Virtual Tags

Query by file location:

```bash
# Files in a specific directory
tagr search -v dir:src

# Files matching a path pattern
tagr search -v "path:src/**/*.rs"
tagr search -v "path:tests/*.rs"

# Files at a specific depth
tagr search -v depth:3
tagr search -v "depth:<5"
```

### Permission Virtual Tags

Filter by file permissions:

```bash
# Executable files
tagr search -v perm:executable

# Read-only files
tagr search -v perm:readonly

# Writable files
tagr search -v perm:writable
```

### Content Virtual Tags

Query by file content properties:

```bash
# Files with specific line count
tagr search -v "lines:>100"
tagr search -v "lines:<50"
tagr search -v lines:10-50
```

### Git Virtual Tags

Query by Git status (when in a repository):

```bash
# Tracked files
tagr search -v git:tracked

# Modified files
tagr search -v git:modified

# Staged files
tagr search -v git:staged

# Untracked files
tagr search -v git:untracked

# Stale files (not modified in 6+ months)
tagr search -v git:stale
```

### Combining Virtual Tags

Use multiple virtual tags together with AND/OR logic:

```bash
# Find large Rust files (AND logic - default)
tagr search -v ext:.rs -v "size:>100KB"

# Find files that are either empty OR tiny (OR logic)
tagr search -v size:empty -v size:tiny --any-virtual

# Combine regular tags with virtual tags
tagr search -t rust -v "modified:this-week"
tagr search -t config -v ext:.toml -v "size:<10KB"

# Complex queries
tagr search -t documentation -v ext-type:document -v "modified:last-7-days"
```

### Saving Virtual Tags in Filters

Virtual tags can be saved in filters for quick recall:

```bash
# Save a filter with virtual tags
tagr search -t rust -v ext:.rs -v "size:>1KB" \\
  --save-filter "rust-source" \\
  --filter-desc "Non-empty Rust source files"

# Use the saved filter
tagr search -F rust-source

# View the filter (shows virtual tags)
tagr filter show rust-source

# Combine saved filter with additional virtual tags
tagr search -F rust-source -v "modified:today"
```

### Virtual Tag Configuration

Customize virtual tag behavior in `~/.config/tagr/config.toml`:

```toml
[virtual_tags]
enabled = true
cache_metadata = true
cache_ttl_seconds = 300

[virtual_tags.size_categories]
tiny = "1KB"
small = "100KB"
medium = "1MB"
large = "10MB"
huge = "100MB"

[virtual_tags.extension_types]
source = [".rs", ".py", ".js", ".go"]
document = [".md", ".txt", ".pdf"]
config = [".toml", ".yaml", ".json"]

[virtual_tags.time]
recent = 7      # days
stale = 180     # days

[virtual_tags.git]
enabled = true
detect_repo = true
```

### Maintenance

```bash
# Clean up missing files and untagged entries
tagr cleanup
tagr c

# Quiet mode (suppress informational output)
tagr -q <command>
```

## Cleanup Feature

The cleanup command helps maintain database integrity by identifying and removing:

1. **Missing Files** - Files in the database that no longer exist on the filesystem
2. **Untagged Files** - Files with no tags assigned

### Interactive Cleanup

```bash
tagr cleanup
```

For each problematic file, you can respond with:
- `y` or `yes` - Delete this file from the database
- `n` or `no` - Skip this file
- `a` or `yes-to-all` - Delete this file and all remaining in this category
- `q` or `no-to-all` - Skip this file and all remaining in this category

### Automated Cleanup

```bash
# Delete all missing files and all untagged files
echo -e "a\na" | tagr cleanup

# Delete all missing files but skip untagged files
echo -e "a\nq" | tagr cleanup
```

## Bulk Operations

Tagr provides powerful bulk operations for managing tags across many files at once.

### Bulk Tag vs Copy Tags

Tagr offers two related bulk operations that serve different intents:

- `bulk tag`: Adds the literal tags you specify to the matched files, regardless of any source file. Use this when you want to apply explicit tags you provide.
- `bulk copy-tags`: Copies tags from a specific source file to matched target files. Optional `--copy-tags` acts as an allowlist and copies only the intersection of that list and the tags currently present on the source. `--exclude-tags` removes specific tags from being copied.

Why both exist side by side:

- Provenance: `copy-tags` enforces that tags originate from the source file’s current state, preventing drift when templates evolve.
- Safety: With `--copy-tags`, only the intersection with the source is applied; typos or stale names are ignored instead of being introduced.
- Workflows: Teams using a “template” file can propagate its current approved tags to others, while `bulk tag` remains the tool to add arbitrary tags directly.

Rule of thumb:

- Use `bulk tag` to add explicit tags you type.
- Use `bulk copy-tags` to propagate tags from a source-of-truth file, optionally narrowed via `--copy-tags` and/or `--exclude-tags`.

Examples:

```bash
# Add explicit tags to files that match tag "initial"
tagr bulk tag -t initial --any-tag review approved --yes

# Copy only tags that the source currently has, limited to an allowlist
tagr bulk copy-tags /path/template.md -t initial --any-tag \
    --copy-tags review --copy-tags approved --yes

# Copy all tags from source except a specific one
tagr bulk copy-tags /path/template.md -t initial --any-tag \
    --exclude-tags deprecated --yes
```

### Bulk Untag

Remove tags from multiple files matching search criteria.

```bash
# Remove "temp" tag from all files
tagr bulk untag temp --yes

# Remove "wip" tag from files matching "*.rs"
tagr bulk untag -f "*.rs" wip --yes

# Remove ALL tags from files matching "*.tmp"
tagr bulk untag -f "*.tmp" --all --yes
```

### Batch Tagging From File

Apply tags to many files by supplying a structured batch file. Supported formats: plain text, CSV, JSON. Select with `--format` and (for CSV) an optional `--delimiter`.

### Plain Text (`--format text`)
Each non-empty, non-comment line: `<file> <tag1> <tag2> ...` (whitespace-separated). Lines starting with `#` are ignored.

```
/proj/app/README.md docs markdown
/proj/app/src/main.rs rust backend service
# A comment line
/proj/app/src/lib/util.rs rust helper
```

Usage:
```bash
tagr bulk from-file batch.txt --format text --yes
```

### CSV (`--format csv`)
First column is the file path; remaining columns are tags. Default delimiter is `,`; override with `--delimiter ';'` etc. A quoted single field may contain an inner comma list of tags (e.g. `"tag3,tag4"`).

```
/proj/app/README.md,docs,markdown
/proj/app/src/main.rs,rust,backend,service
/proj/app/src/lib/util.rs,"rust,helper"
```

Custom delimiter example (`;`):
```
/proj/app/README.md;docs;markdown
/proj/app/src/main.rs;rust;backend;service
```

Usage:
```bash
tagr bulk from-file tags.csv --format csv --yes
tagr bulk from-file tags-semicolon.csv --format csv --delimiter ';' --dry-run
```

### JSON (`--format json`)
Array of objects each with `file` and `tags` keys:
```json
[
    {"file": "/proj/app/README.md", "tags": ["docs", "markdown"]},
    {"file": "/proj/app/src/main.rs", "tags": ["rust", "backend", "service"]},
    {"file": "/proj/app/src/lib/util.rs", "tags": ["rust", "helper"]}
]
```

Usage:
```bash
tagr bulk from-file tags.json --format json --dry-run
```

### Format Mismatch Hints
If parsing fails, Tagr attempts the other parsers and emits a hint instead of guessing:
```
Invalid JSON at line 1 column 2
Hint: The file appears to be CSV. Use '--format csv' (with '--delimiter' if needed).
```
```
Invalid CSV record 1
Hint: The file may be JSON. Use '--format json'.
```
No automatic fallback occurs—adjust your flags and re-run.

- Individual line errors do not abort the whole batch unless global parse fails

### Dry Run
Preview impact without applying changes:
```bash
tagr bulk from-file tags.csv --format csv --dry-run
```

### Choosing a Format
| Scenario | Format |
|----------|--------|
| Hand editing | Plain text |
| Spreadsheet export | CSV |
| Programmatic generation | JSON |

### Delimiter (CSV)
`--delimiter` applies only to CSV. Internally stored as `Csv(char)` for clarity.


### Bulk Tag Mapping

Rename (map) multiple tag names across all files using a structured mapping file. Each mapping replaces one tag (`from`) with another (`to`). If the target tag already exists on a file, the source tag is removed (merging semantics, no duplicates created).

### When To Use
- Consolidating synonymous tags (e.g. `todo` → `pending`)
- Normalizing inconsistent capitalization (`Bug` → `bug`)
- Migrating deprecated taxonomy (`legacy-api` → `deprecated`)

### Supported Formats

#### Plain Text (`--format text`)
Each non-empty, non-comment line: `old_tag new_tag` (exactly two whitespace-separated tokens). Lines starting with `#` are ignored.

```
old pending
Bug bug
# comment line
legacy-api deprecated
```

Usage:
```bash
tagr bulk map-tags mappings.txt --format text --yes
tagr bulk map-tags mappings.txt --format text --dry-run
```

#### CSV (`--format csv`)
Exactly 2 columns per record: `old,new`. Provide a custom delimiter with `--delimiter` if needed.

```
old,pending
Bug,bug
legacy-api,deprecated
```

Custom delimiter example (`;`):
```
old;pending
Bug;bug
```

Usage:
```bash
tagr bulk map-tags mappings.csv --format csv --yes
tagr bulk map-tags mappings-semicolon.csv --format csv --delimiter ';' --dry-run
```

#### JSON (`--format json`)
Array of objects with `from` and `to` keys:
```json
[
    {"from": "old", "to": "pending"},
    {"from": "Bug", "to": "bug"},
    {"from": "legacy-api", "to": "deprecated"}
]
```

Usage:
```bash
tagr bulk map-tags mappings.json --format json --dry-run
```

### Behavior & Semantics
- Skips mappings where `from == to` (reported as skipped)
- Skips mappings whose source tag does not exist in any file
- Merges automatically if `to` already present on a file (removes only the `from` tag)
- Deduplicates resulting tag lists per file
- Order matters for cascading renames (process appears sequentially). For A→B and B→C in one file list, apply them in intended sequence explicitly in your mapping file.
- Reverse index kept consistent via internal `insert_pair()` pattern

### Dry Run Preview
```bash
tagr bulk map-tags mappings.txt --format text --dry-run
```
Shows planned mappings and counts without modifying the database.

### Error Handling & Hints
Malformed lines/records abort parsing with a hint (e.g., CSV given when JSON expected). Parser attempts generate format mismatch hints; Tagr never silently falls back.

### Examples
```bash
# Normalize capitalization & consolidate synonyms
tagr bulk map-tags normalize.csv --format csv --yes

# Preview large migration
tagr bulk map-tags taxonomy.json --format json --dry-run
```

### Bulk Delete Files

Remove many file entries from the database using an input list. This does NOT delete files from the filesystem—only database records are removed.

### Use Cases
- Prune outdated or deprecated file entries
- Clean up after moving/renaming files externally
- Prepare for a fresh retagging effort

### Supported Formats

#### Plain Text (`--format text`)
Each non-empty, non-comment line begins with the file path. Additional tokens on the line are ignored (allowing copy/paste from other lists).
```
/proj/app/src/old.rs
/proj/app/docs/legacy.md extra tokens ignored
# comment
/proj/app/tmp/scratch.txt
```
Usage:
```bash
tagr bulk delete-files delete.txt --format text --yes
tagr bulk delete-files delete.txt --format text --dry-run
```

#### CSV (`--format csv`)
First column is the path; remaining columns ignored (can hold notes).
```
/proj/app/src/old.rs,unused,legacy
/proj/app/docs/legacy.md,deprecated
```
Usage:
```bash
tagr bulk delete-files delete.csv --format csv --yes
tagr bulk delete-files delete-semicolon.csv --format csv --delimiter ';' --dry-run
```

#### JSON (`--format json`)
Array of objects with a `file` field:
```json
[
    {"file": "/proj/app/src/old.rs"},
    {"file": "/proj/app/docs/legacy.md"}
]
```
Usage:
```bash
tagr bulk delete-files delete.json --format json --dry-run
```

### Behavior & Semantics
- Input paths are de-duplicated before processing
- Missing paths (not present in the database) are reported as skipped
- No filesystem deletion—safe to run without data loss
- Confirmation prompt skipped with `--yes`; use `--dry-run` to preview

### Dry Run
```bash
tagr bulk delete-files delete.txt --format text --dry-run
```
Displays the unique set of database entries that would be removed.

### Errors & Hints
Malformed records produce an immediate error. Format mismatch hints guide you to the correct `--format`/`--delimiter`.

### Examples
```bash
# Remove a curated set of obsolete entries
tagr bulk delete-files obsolete.csv --format csv --yes

# Preview a massive cleanup first
tagr bulk delete-files stale.json --format json --dry-run
```


### Bulk Rename Tag

Rename a single tag globally across all files in the database. This is useful for fixing typos or restructuring your tag taxonomy.

```bash
# Rename "todo" to "pending"
tagr bulk rename-tag todo pending --yes

# Preview rename
tagr bulk rename-tag legacy-api deprecated --dry-run
```

### Bulk Merge Tags

Merge multiple source tags into a single target tag. This is useful for consolidating synonymous tags.

```bash
# Merge "bug", "defect", "issue" into "bug-report"
tagr bulk merge-tags bug defect issue --into bug-report --yes

# Preview merge
tagr bulk merge-tags wip draft --into pending --dry-run
```
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
├── lib.rs          # Library root, exports all modules
├── main.rs         # CLI application entry point
├── cli.rs          # Command line interface
├── config.rs       # Configuration management
├── db/             # Database wrapper
│   ├── mod.rs      # Database operations
│   ├── types.rs    # Data types
│   └── error.rs    # Error types
└── search/         # Interactive fuzzy finding
    ├── mod.rs      # Browse functionality
    ├── browse.rs   # Browse implementation
    └── error.rs    # Error types
```

## Library Usage

tagr can be used as a library in your Rust projects:

```rust
use tagr::{db::Database, browse, filters::FilterManager};
use tagr::browse::{BrowseSession, BrowseController, BrowseConfig};
use tagr::ui::RatatuiFinder;
use std::path::PathBuf;

// Database operations
let db = Database::open("my_db").unwrap();
db.insert("file.txt", vec!["tag1".into(), "tag2".into()]).unwrap();
let files = db.find_by_tag("tag1").unwrap();

// Filter management
let filter_manager = FilterManager::new(PathBuf::from("~/.config/tagr/filters.toml"));
let filters = filter_manager.list().unwrap();

// Interactive browse with ratatui
let config = BrowseConfig::default();
let session = BrowseSession::new(&db, config).unwrap();
let finder = RatatuiFinder::new();
let controller = BrowseController::new(session, finder);

match controller.run() {
    Ok(Some(result)) => {
        println!("Selected {} files from tags: {:?}", 
            result.selected_files.len(),
            result.selected_tags);
        
        for file in result.selected_files {
            println!("  - {}", file.display());
        }
    }
    Ok(None) => println!("Browse cancelled"),
    Err(e) => eprintln!("Error: {}", e),
}
```

### Interactive Browse API

The browse functionality is cleanly separated into business logic and UI layers:

```rust
use tagr::browse::{BrowseSession, BrowseController, BrowseConfig, PathFormat};
use tagr::ui::RatatuiFinder;

// Configure browse session
let config = BrowseConfig {
    initial_search: None,  // Start with tag selection
    path_format: PathFormat::Relative,  // Show relative paths
    tag_phase_settings: Default::default(),
    file_phase_settings: Default::default(),
};

let session = BrowseSession::new(&db, config).unwrap();
let controller = BrowseController::new(session, RatatuiFinder::new());

if let Ok(Some(result)) = controller.run() {
    // Process selected files
    for file in result.selected_files {
        println!("{}", file.display());
    }
}
```

### Direct Action Execution (without browse)

You can execute actions directly without the interactive browser:

```rust
use tagr::browse::actions;
use std::path::PathBuf;

let files = vec![
    PathBuf::from("src/main.rs"),
    PathBuf::from("src/lib.rs"),
];
let tags = vec!["rust".to_string(), "important".to_string()];

// Add tags to files
match actions::execute_add_tag(&db, &files, &tags) {
    Ok(outcome) => match outcome {
        tagr::browse::models::ActionOutcome::Success { affected_count, details } => {
            println!("✓ {} ({} files)", details, affected_count);
        }
        tagr::browse::models::ActionOutcome::Partial { succeeded, failed, errors } => {
            println!("⚠️  {} succeeded, {} failed", succeeded, failed);
            for error in &errors {
                eprintln!("  - {}", error);
            }
        }
        _ => {}
    }
    Err(e) => eprintln!("Error: {}", e),
}

// Other available actions
actions::execute_remove_tag(&db, &files, &["old_tag".to_string()]).unwrap();
actions::execute_delete_from_db(&db, &files).unwrap();
actions::execute_open_in_default(&files);
actions::execute_open_in_editor(&files, "vim");
actions::execute_copy_path(&files).unwrap();
actions::execute_copy_files(&files, &PathBuf::from("/dest"), false);
```

### Custom Frontend Implementation

To implement a custom UI backend (e.g., ratatui), implement the `FuzzyFinder` trait:

```rust
use tagr::ui::{FuzzyFinder, FinderConfig, FinderResult};

struct MyCustomFinder {
    // Your UI state
}

impl FuzzyFinder for MyCustomFinder {
    fn run(&self, config: FinderConfig) -> tagr::ui::Result<FinderResult> {
        // Your custom UI implementation
        // - Render config.items (Vec<DisplayItem>)
        // - Handle user input
        // - Return selected item IDs
        
        Ok(FinderResult {
            selected: vec!["item1".to_string(), "item2".to_string()],
            aborted: false,
            final_key: Some("enter".to_string()),
        })
    }
}

// Use your custom finder
let session = BrowseSession::new(&db, BrowseConfig::default()).unwrap();
let controller = BrowseController::new(session, MyCustomFinder::new());
controller.run().unwrap();
```

For a complete guide on implementing custom frontends, see [`docs/custom-frontend-guide.md`](docs/custom-frontend-guide.md).

### Query Business Logic

Access pure query functions without UI:

```rust
use tagr::browse::query;
use tagr::browse::models::TagrItem;

// Get all tags with file counts
let tags: Vec<TagrItem> = query::get_available_tags(&db).unwrap();
for tag in tags {
    println!("{}: {} files", tag.name, /* extract file_count from metadata */);
}

// Get files by search parameters
use tagr::cli::SearchParams;
let search = SearchParams {
    tags: vec!["rust".to_string()],
    tag_mode: tagr::cli::SearchMode::Any,
    ..Default::default()
};

let files = query::get_matching_files(&db, &search).unwrap();
```

### Filter Management API

```rust
use tagr::filters::{FilterManager, FilterCriteria, TagMode};
use std::path::PathBuf;

let manager = FilterManager::new(PathBuf::from("~/.config/tagr/filters.toml"));

// Create a filter
let criteria = FilterCriteria {
    tags: vec!["rust".to_string(), "tutorial".to_string()],
    tag_mode: TagMode::All,
    file_patterns: vec!["*.rs".to_string()],
    ..Default::default()
};

manager.create(
    "rust-tutorials".to_string(),
    "Find Rust tutorial files".to_string(),
    criteria,
).unwrap();

// Use a filter
let filter = manager.get("rust-tutorials").unwrap();
println!("Filter: {} - {}", filter.name, filter.description);

// List all filters
let filters = manager.list().unwrap();
for filter in filters {
    println!("{}: {}", filter.name, filter.description);
}

// Export/import filters
manager.export(&PathBuf::from("my-filters.toml"), &[]).unwrap();
```

### Database API

```rust
// Insert/Update
db.insert("file.txt", vec!["tag1".into()]).unwrap();
db.insert_pair(pair).unwrap();

// Retrieve
db.get_tags("file.txt").unwrap();      // Option<Vec<String>>
db.get_pair("file.txt").unwrap();      // Option<Pair>

// Add/Remove Tags
db.add_tags("file.txt", vec!["tag3".into()]).unwrap();
db.remove_tags("file.txt", &["tag1".into()]).unwrap();

// Delete
db.remove("file.txt").unwrap();        // bool (existed?)

// Query
db.find_by_tag("tag1").unwrap();       // Vec<PathBuf>
db.find_by_all_tags(&[...]).unwrap();  // Vec<PathBuf>
db.find_by_any_tag(&[...]).unwrap();   // Vec<PathBuf>

// List
db.list_all().unwrap();                // Vec<Pair>
db.list_all_tags().unwrap();           // Vec<String>

// Utility
db.contains("file.txt").unwrap();      // bool
db.count();                            // usize
db.flush().unwrap();
db.clear().unwrap();
```

## Configuration

Configuration file location: `~/.config/tagr/config.toml`

```toml
default_database = "myfiles"

[databases]
myfiles = "/home/user/tags_db"
default = "/home/user/.local/share/tagr/default"
```

### Default Locations

- **Linux**: `~/.local/share/tagr/`
- **macOS**: `~/Library/Application Support/tagr/`
- **Windows**: `C:\Users\<username>\AppData\Local\tagr\`

## Testing

```bash
# Run tests
cargo test

# Run with test data
./test_browse.sh
```

## Dependencies

- **sled** - Embedded database for persistent storage
- **nucleo** - Fast fuzzy matching engine
- **ratatui** - Modern terminal user interface framework
- **crossterm** - Cross-platform terminal manipulation
- **bincode** - Efficient binary serialization
- **clap** - Command-line argument parsing
- **chrono** - Date/time handling for filter timestamps
- **thiserror** - Error handling

## Performance Notes

**Regular Tags (database-stored):**
- Tag lookups are O(1) via reverse index
- Complex queries (AND/OR) use set operations on O(1) lookups
- Storage overhead is ~50% (files tree + tags tree)
- Efficient for 10,000+ files with 100+ tags

**Virtual Tags (filesystem metadata):**
- O(n) evaluation where n = candidate files (from database or prior filters)
- Parallel evaluation using rayon for multi-core performance
- Metadata caching (300s TTL) reduces repeated filesystem calls
- Best performance when combined with regular tags to reduce candidate set
- Example: `-t rust -v modified:today` evaluates only rust-tagged files

**General:**
- Auto-flush on drop ensures data durability
- Pattern filtering (glob/regex) applied after tag lookups

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the MIT License.

## Future Enhancements

Potential improvements for future releases:

### Advanced Bulk Operations
- Deduplicate tags using fuzzy matching (detect similar/redundant tags)
- Git integration for auto-tagging based on commit history
- Content-based auto-tagging suggestions using pattern matching or ML

### Browse Mode Enhancements
- Tag statistics - Show file count per tag in browse mode
- Recent selections - Remember last used tags
- Export results - Save selections to file
- LRU cache - In-memory cache for hot tags

### Infrastructure & Safety
- Backup and restore system for database
- Progress indicators for long-running bulk operations
- Parallel processing for large file sets
- JSON output format for scripting integration
- Transaction support for atomic batch operations