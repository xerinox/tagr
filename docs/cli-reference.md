# Tagr CLI Reference

This document provides a concise, structured reference for the Tagr command-line interface.
It complements the high-level overview in the main README.

You can always view the built-in help:

```bash
# Top-level help
tagr --help

# Per-command help
tagr <command> --help
```

---

## Global Flags

These flags are available to most commands:

```bash
-q, --quiet        Suppress informational output (only print results)
    --db <NAME>    Use a specific database (overrides default)
```

---

## Main Commands Overview

```bash
# Browse (default)
tagr               # same as: tagr browse

# Search files
tagr search

# Tag / untag files
tagr tag
tagr untag

# List files or tags
tagr list

# File notes
tagr note

# Cleanup database
tagr cleanup

# Bulk operations
tagr bulk

# Database management
tagr db

# Saved filters
tagr filter

# Tag aliases
tagr alias

# Global tag management
tagr tags

# Config management
tagr config

# Shell completions
tagr completions
```

Aliases:

- `browse` → `b`
- `search` → `s`
- `tag` → `t`
- `untag` → `u`
- `list` → `l`
- `cleanup` → `c`
- `bulk map-tags` → `bulk map`
- `bulk delete-files` → `bulk del-files`

---

## Browse Command

Interactive fuzzy finder for tags and files.

```bash
# Default browse (same as just `tagr`)
tagr browse

# Browse with an initial query (search tags and filenames)
tagr browse documents

# Browse with explicit criteria
tagr browse -t rust -t tutorial

# Browse with file patterns (glob)
tagr browse -f "*.rs" -f "src/*.rs"

# Exclude tags
tagr browse -t documentation -e archived

# Execute a command on each selected file
tagr browse -t images -x "cp {} /backup/"
```

Key options (shared with `search`):

```bash
-t, --tag <TAG>           Tags to filter by (multiple allowed, supports aliases)
-f, --file <PATTERN>      File patterns (glob or regex)
-e, --exclude <TAG>       Exclude files with these tags
-v, --virtual-tag <VTAG>  Virtual tags (e.g. size:>1MB, modified:today)
    --any-tag             Match ANY tag (OR logic)
    --all-tags            Match ALL tags (AND logic, default)
    --any-file            Match ANY file pattern
    --all-files           Match ALL file patterns (default)
    --any-virtual         Match ANY virtual tag
    --all-virtual         Match ALL virtual tags (default)
    --regex-tag           Treat tags as regex (alias: --regex-tags)
    --regex-file          Treat file patterns as regex (alias: --regex-files)
    --glob-files          Treat file patterns as globs (alias: --glob-file)
    --no-hierarchy        Skip hierarchy expansion (don't search parent tags)
```

Browse-specific options:

```bash
-x, --exec <CMD>          Execute a command per selected file ("{}" = file path)
    --no-preview          Disable preview pane
    --preview-lines N     Set max preview lines
    --preview-position P  right | bottom | top
    --preview-width N     Preview width (percent)
    --absolute            Show absolute paths
    --relative            Show relative paths
```

Examples:

```bash
# Browse and edit Rust tutorial files
tagr browse -t tutorial -f "*.rs" -x "nvim {}"

# Browse docs (any format) excluding archived
tagr browse -t documentation -f "*.md" -f "*.txt" --any-file -e archived

# Browse with virtual tags (recently modified Rust files)
tagr browse -t rust -v "modified:last-7-days"
```

---

## Search Command

Non-interactive search that prints results to stdout.

Basic usage:

```bash
# Single tag
tagr search -t rust

# Multiple tags (AND logic by default)
tagr search -t rust -t tutorial

# OR logic for tags
tagr search -t rust -t python --any-tag

# File patterns
tagr search -t tutorial -f "*.rs"
tagr search -t config -f "*.toml" -f "*.yaml" --any-file
```

Advanced examples:

```bash
# Tags AND, files OR
# Files must have BOTH "rust" AND "library" tags
# AND match EITHER "*.rs" OR "*.md"
tagr search -t rust -t library --all-tags \
  -f "*.rs" -f "*.md" --any-file

# Tags OR, files AND
# Files must have EITHER "rust" OR "python" tag
# AND match BOTH "src/*" AND "*test*" patterns
tagr search -t rust -t python --any-tag \
  -f "src/*" -f "*test*" --all-files

# Regex tags and regex file patterns
tagr search --regex-tag -t '^topic\..*' -e drop --any-tag

tagr search --regex-file -t source -f 'src/.*\\.rs$'

# Combine regular and virtual tags
tagr search -t rust -v "modified:this-week"
tagr search -t documentation -v ext-type:document -v "modified:last-7-days"
```

Output control:

```bash
-q, --quiet      Print only file paths (useful for piping)
    --absolute   Show absolute paths
    --relative   Show relative paths
```

Integration:

```bash
# Open all matching files in nvim
tagr search -q -t rust -t tutorial -f "*.rs" | xargs nvim

# Count matches
tagr search -q -t python -t test | wc -l
```

---

## Tag / Untag Commands

### tag

Add tags to a single file.

```bash
# Basic
tagr tag <file> <tags...>

# With explicit flags (equivalent)
tagr tag -f <file> -t tag1 -t tag2
```

Semantics:

- Adds the provided tags to the file.
- Existing tags are preserved; duplicates are avoided.

### untag

Remove tags from a single file.

```bash
# Remove specific tags
tagr untag <file> <tags...>

# Remove all tags from a file
tagr untag -f <file> --all
```

The command accepts both positional and flag-based forms; see `tagr untag --help` for details.

---

## List, Cleanup, Tags

### list

```bash
# List all tags
tagr list tags

# List all files
tagr list files
```

Options:

- `--absolute` / `--relative` – control path display.

### cleanup

```bash
# Interactive cleanup
tagr cleanup

# Scripted cleanup (example)
echo -e "a\na" | tagr cleanup
```

Cleans up:

- Missing files (entries whose paths no longer exist).
- Untagged files (have no tags).

### tags

Global tag management:

```bash
# List all tags
tagr tags list

# List tags in tree format (shows hierarchies)
tagr tags list --tree

# Remove a tag from all files
tagr tags remove <tag>
tagr tags rm <tag>
```

---

## Saved Filters (`tagr filter`)

Manage named filters that encapsulate search criteria.

Common subcommands:

```bash
# Create a filter from criteria
tagr filter create rust-tutorials \
  -d "Find Rust tutorial files" \
  -t rust -t tutorial -f "*.rs"

# List filters
tagr filter list

tagr filter show rust-tutorials

tagr filter rename rust-tutorials rust-beginner-tutorials

tagr filter delete rust-tutorials
```

Using filters with search/browse:

```bash
# Use a saved filter
tagr search --filter rust-tutorials

# Load in browse mode
tagr browse --filter prod-rust

# Combine filter with extra criteria
tagr search -F rust-tutorials -e beginner
```

Export / import:

```bash
# Export all filters
tagr filter export --output team-filters.toml

# Import filters
tagr filter import team-filters.toml --overwrite
```

---

## Note Command

Attach and manage markdown notes for files.

### note edit

Edit note in $EDITOR:

```bash
# Edit note for one file
tagr note edit config.toml

# Edit notes for multiple files
tagr note edit file1.txt file2.txt

# Use specific editor
tagr note edit README.md --editor nvim
```

### note add

Append timestamped entry (quick workflow):

```bash
# Add first entry
tagr note add refactor.rs "Started breaking into smaller modules"
# Creates:
# ### 2026-01-14 10:30
# 
# Started breaking into smaller modules

# Add subsequent entry
tagr note add refactor.rs "Completed refactor, tests passing"
# Appends:
# ---
# ### 2026-01-14 15:45
# 
# Completed refactor, tests passing
```

Timestamp format: `### YYYY-MM-DD HH:MM` (markdown H3 heading)  
Separator: `---` (horizontal rule)

### note show

Display note content:

```bash
# Show note (plain text)
tagr note show file.txt

# Show with metadata
tagr note show file.txt --verbose

# JSON output
tagr note show file.txt --format json

# Quiet mode (just file path if note exists)
tagr note show file.txt --format quiet
```

### note search

Search note content:

```bash
# Find notes containing "TODO"
tagr note search "TODO"

# Show content snippets
tagr note search "refactor" --show-content

# Quiet output for piping
tagr note search "bug" --format quiet | xargs -I {} echo "File: {}"

# JSON output
tagr note search "urgent" --format json
```

### note list

List all files with notes:

```bash
# List files (one per line)
tagr note list

# Show metadata
tagr note list --verbose

# JSON output
tagr note list --format json
```

### note delete

Remove notes from files:

```bash
# Delete note from one file
tagr note delete old-file.txt

# Delete from multiple files
tagr note delete file1.txt file2.txt

# Preview deletion
tagr note delete *.tmp --dry-run

# Skip confirmation
tagr note delete file.txt --yes
```

### Notes in TUI

Browse mode (`tagr browse`) integration:

- **Ctrl+N**: Edit note for selected file
- **Alt+N**: Toggle file/note preview
- **📝 icon**: Indicates files with notes
- **"📝 Notes Only"**: Special category for files with notes but no tags

### Output Formats

All note commands support `--format` flag:

```bash
--format text    # Human-readable (default)
--format json    # Structured JSON for scripting
--format quiet   # Minimal output (paths only)
```

### Configuration

Configure in `~/.config/tagr/config.toml`:

```toml
[notes]
storage = "integrated"        # Database storage
editor = "nvim"               # Override $EDITOR
max_note_size_kb = 100       # Size limit warning
default_template = ""        # Pre-populate new notes
```

---

## Tag Aliases (`tagr alias`)

Manage tag aliases (synonyms) for consolidating similar tags.

### Create Alias

```bash
# Map alias to canonical tag
tagr alias add js javascript
tagr alias add py python
tagr alias add ts lang:typescript  # aliases can point to hierarchical tags
```

### List Aliases

```bash
# Show all aliases
tagr alias list
tagr alias ls
```

Output:
```
js → javascript
py → python
ts → lang:typescript
```

### Show Tag Synonyms

```bash
# Display all aliases for a tag
tagr alias show javascript
```

Output:
```
Tag: javascript
Aliases: js, es6, ecmascript
```

### Remove Alias

```bash
# Delete an alias
tagr alias remove js
tagr alias rm js
```

### Usage in Tagging

```bash
# Aliases automatically canonicalize
tagr tag app.js js              # stores as "javascript"
tagr tag script.py py           # stores as "python"

# Opt out of canonicalization
tagr tag file.txt js --no-canonicalize  # stores as "js"
```

### Usage in Search

```bash
# Search using any alias (automatically expands)
tagr search -t js               # finds files tagged "javascript"
tagr search -t javascript       # same result

# Works with browse mode
tagr browse -t py               # finds files tagged "python"
```

### Hierarchical Aliases

Aliases can point to hierarchical tags:

```bash
# Create alias to hierarchical tag
tagr alias add rust lang:rust

# Tagging with alias
tagr tag main.rs rust           # stores as "lang:rust"

# Searching expands both alias AND hierarchy
tagr search -t rust             # finds lang:rust and all lang:rust:* children
```

### Validation

- Alias names cannot contain `:` (reserved for hierarchies)
- Circular references are prevented (e.g., A→B→A)
- Case-insensitive matching
- Self-references are allowed (harmless)

---

## Tag Hierarchies

Organize tags with parent:child relationships using `:` delimiter.

### Tagging with Hierarchies

```bash
# Create hierarchical tags
tagr tag src/main.rs lang:rust
tagr tag docs/tutorial.md lang:rust:beginner
tagr tag app.py lang:python
```

### Searching with Hierarchies

```bash
# Search automatically includes parent tags
tagr search -t lang:rust        # finds lang:rust AND lang:rust:beginner
tagr search -t lang             # finds ALL lang:* tags

# Disable hierarchy expansion
tagr search -t lang:rust --no-hierarchy
```

### Tree Visualization

```bash
# Display tags in tree format
tagr tags list --tree
```

Output:
```
lang (parent)
├── python (12 files)
└── rust (42 files)
    ├── async (8 files)
    └── beginner (15 files)
```

### TUI Tag Tree

Browse mode displays hierarchical tags in an interactive tree:

```bash
tagr browse
```

**Left pane** (Tag Tree):
- Collapsible hierarchy with Space key
- Multi-select with TAB
- Shows aliases: "javascript (js, es) (42 files)"
- Short names in tree (python not lang:python)

**Right pane** (Items List):
- Synchronized with tag tree selections
- Live filtering matches tree

---

## Virtual Tags (CLI View)

Virtual tags are passed via `-v/--virtual-tag` to `search` or `browse`.

Examples:

```bash
# Time-based
tagr search -v modified:today

# Size-based
tagr search -v size:empty

# Extension-based
tagr search -v ext:.rs

# Git-based
tagr search -v git:modified

# Combine with regular tags
tagr search -t rust -v "modified:last-7-days"
```

The main README explains the semantics and configuration of each virtual tag family.

---

## Bulk Operations (`tagr bulk`)

Bulk commands operate on many files at once.

Shared concepts:

- **Search criteria**: most bulk commands accept the same `-t/-f/-v` criteria as `search`.
- **Dry-run**: `-n/--dry-run` previews changes without applying them.
- **Confirmation**: `-y/--yes` skips the interactive confirmation prompt.

### bulk tag

Add tags to multiple files matching search criteria.

```bash
# Add a tag to all Rust files
tagr bulk tag -t rust review --yes

# Add tags to files matching a glob
tagr bulk tag -f "**/*.rs" reviewed --yes
```

### bulk untag

Remove tags from multiple files.

```bash
# Remove a tag everywhere
tagr bulk untag temp --yes

# Remove a tag from matching files only
tagr bulk untag -f "*.rs" wip --yes

# Remove all tags from files matching a pattern
tagr bulk untag -f "*.tmp" --all --yes
```

### bulk copy-tags

Copy tags from a source file to matching target files.

```bash
# Copy all tags from source to files tagged "initial"
tagr bulk copy-tags /path/template.md -t initial --any-tag --yes

# Copy only specific tags from the source
tagr bulk copy-tags /path/template.md -t initial --any-tag \
  --copy-tags review --copy-tags approved --yes

# Copy all tags except a specific one
tagr bulk copy-tags /path/template.md -t initial --any-tag \
  --exclude-tags deprecated --yes
```

### bulk from-file

Apply tags from a batch input file (text, CSV, JSON).

```bash
# Plain text
tagr bulk from-file batch.txt --format text --yes

# CSV
tagr bulk from-file tags.csv --format csv --yes

# JSON
tagr bulk from-file tags.json --format json --dry-run
```

### bulk map-tags

Rename (map) many tags using a mapping file.

```bash
# Text mapping file
tagr bulk map-tags mappings.txt --format text --yes

# CSV mapping file
tagr bulk map-tags mappings.csv --format csv --yes

# JSON mapping file
tagr bulk map-tags mappings.json --format json --dry-run
```

### bulk delete-files

Delete many file entries from the database using an input list.

```bash
# Text
tagr bulk delete-files delete.txt --format text --yes

# CSV
tagr bulk delete-files delete.csv --format csv --yes

# JSON
tagr bulk delete-files delete.json --format json --dry-run
```

### bulk rename-tag / merge-tags

```bash
# Rename a tag globally
tagr bulk rename-tag todo pending --yes

# Merge multiple tags into a single tag
tagr bulk merge-tags bug defect issue --into bug-report --yes
```

---

## Shell Completions (`tagr completions`)

Generate shell completion scripts for various shells.

### Static Completions

```bash
# Generate for bash
tagr completions bash > ~/.local/share/bash-completion/completions/tagr

# Generate for zsh
tagr completions zsh > ~/.zfunc/_tagr

# Generate for fish
tagr completions fish > ~/.config/fish/completions/tagr.fish

# Generate for PowerShell
tagr completions powershell > tagr.ps1

# Generate for elvish
tagr completions elvish > tagr.elv
```

### Dynamic Completions

When built with `--features dynamic-completions`, tagr provides context-aware completions:

| Argument | Completes |
|----------|----------|
| `-t/--tag` | Tags from your database |
| `-e/--exclude` | Tags from your database |
| `-v/--virtual-tag` | Virtual tag types with syntax hints |
| `-F/--filter` | Saved filter names with descriptions |
| `--db` | Configured database names |
| File arguments | File paths (via shell) |

**Setup:**

> **Note**: You only need to set up ONE completion method. If you want dynamic completions, use the instructions below **instead** of the "Static Completions" commands above. The dynamic method handles both static flags and dynamic values.

```bash
# Bash (add to ~/.bashrc)
source <(COMPLETE=bash tagr)

# Zsh (add to ~/.zshrc)
source <(COMPLETE=zsh tagr)

# Fish
COMPLETE=fish tagr | source
```

**Cache Behavior:**

- Cache stored at `~/.cache/tagr/completions.cache`
- Auto-invalidates when:
  - A **new tag** is created (first use of a tag)
  - A **tag becomes orphaned** (last file with that tag removed)
  - **Filters** are created or deleted
  - **Databases** are added or removed
- Falls back to database lookup on cache miss

---

## Database & Config

### db

```bash
# List databases
tagr db list

# Add a database
tagr db add <name> <path>

# Set default database
tagr db set-default <name>

# Remove from config (optionally delete files)
tagr db remove <name> --delete-files
```

### config

```bash
# Set a config value
tagr config set quiet=true

# Get a config value
tagr config get quiet
```

These map to keys in the Tagr config file (see the README for locations and structure).
