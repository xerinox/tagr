# tagr

A fast, interactive command-line tool for organizing files with tags using fuzzy finding and persistent storage.

## Features

- 🏷️ **Tag-based file organization** - Organize files using flexible tags instead of rigid folder structures
- 🔍 **Interactive fuzzy finding** - Browse and select files using an intuitive fuzzy finder interface
- ⚡ **Fast queries** - O(1) tag lookups using reverse indexing with sled database
- 🎯 **Multi-select** - Select multiple tags and files at once
- 🧹 **Database cleanup** - Maintain database integrity by removing missing files and untagged entries
- 💾 **Persistent storage** - Reliable embedded database with automatic flushing
- 📊 **Multiple databases** - Manage separate databases for different projects

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

### Basic Usage

```bash
# Tag some files
tagr tag README.md documentation markdown
tagr tag src/main.rs rust code source
tagr tag src/lib.rs rust code library

# Browse files interactively (default command)
tagr

# Or explicitly
tagr browse

# Search for files by tag (non-interactive)
tagr search rust

# List all tags
tagr list-tags

# Remove tags from a file
tagr untag README.md markdown

# Clean up missing files
tagr cleanup
```

## Interactive Browse Mode

The browse command opens a two-stage fuzzy finder:

### Stage 1: Tag Selection
- Displays all available tags in the database
- **Multi-select enabled** via TAB key
- Fuzzy matching for quick filtering
- Press Enter to proceed to file selection

### Stage 2: File Selection
- Shows all files matching ANY of the selected tags
- Files displayed with their tags inline: `file.txt [tag1, tag2, tag3]`
- **Multi-select enabled** via TAB key
- Fuzzy matching for filtering
- Press Enter to confirm final selection

### Keyboard Controls

| Key | Action |
|-----|--------|
| ↑↓ or Ctrl+J/K | Navigate |
| TAB | Select/deselect (multi-select) |
| Enter | Confirm and proceed |
| ESC / Ctrl+C | Cancel |
| Type | Filter via fuzzy matching |

### Example

```bash
# Launch browse mode
tagr

# Stage 1: Select tags (e.g., "rust" and "programming")
# Stage 2: Select files from results
# Files shown with inline tags: src/main.rs [rust, code, source]
```

## Commands

### File Operations

```bash
# Tag a file
tagr tag <file> <tags...>

# Add tags to existing file (no duplicates)
tagr add-tags <file> <tags...>

# Remove specific tags
tagr untag <file> <tags...>

# Show tags for a file
tagr show <file>
```

### Search & Browse

```bash
# Interactive browse (default)
tagr
tagr browse
tagr b

# Search by single tag (non-interactive)
tagr search <tag>
tagr s <tag>

# List all tags
tagr list-tags
tagr lt

# List all files
tagr list-files
tagr lf
```

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

### Performance Benefits

| Operation | Before (Single Tree) | After (Multi-Tree) | Speedup |
|-----------|---------------------|-------------------|---------|
| `find_by_tag("rust")` | O(n) - scan all files | O(1) - direct lookup | **100-1000x** |
| `list_all_tags()` | O(n) - scan all files | O(k) - iterate tags | **100x** |
| `find_by_all_tags(...)` | O(n) - scan all files | O(k) - set intersection | **100x** |

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
use tagr::{db::Database, search};
use std::path::PathBuf;

// Open or create a database
let db = Database::open("my_db").unwrap();

// Tag a file
db.insert("file.txt", vec!["tag1".into(), "tag2".into()]).unwrap();

// Get tags for a file
let tags = db.get_tags("file.txt").unwrap();

// Find files by tag
let files = db.find_by_tag("tag1").unwrap();

// Find files with ALL specified tags (AND)
let files = db.find_by_all_tags(&["tag1".into(), "tag2".into()]).unwrap();

// Find files with ANY of the specified tags (OR)
let files = db.find_by_any_tag(&["tag1".into(), "tag2".into()]).unwrap();

// Interactive browse
match search::browse(&db) {
    Ok(Some(result)) => {
        println!("Selected {} tags", result.selected_tags.len());
        println!("Selected {} files", result.selected_files.len());
    }
    Ok(None) => println!("Cancelled"),
    Err(e) => eprintln!("Error: {}", e),
}
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

## Examples

### Try the Demo

```bash
cargo run --example browse_demo
```

This creates a test database with 10 files and 13+ tags, then launches browse mode.

## Testing

```bash
# Run tests
cargo test

# Run with test data
./test_browse.sh
```

## Dependencies

- **sled** - Embedded database for persistent storage
- **skim** - Fuzzy finder for interactive browsing
- **bincode** - Efficient binary serialization
- **clap** - Command-line argument parsing
- **thiserror** - Error handling

## Performance Notes

- Tag lookups are O(1) with reverse indexing
- Storage overhead is ~50% (files tree + tags tree)
- Auto-flush on drop ensures data durability
- Efficient for 10,000+ files with 100+ tags

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the MIT License.

## Future Enhancements

Potential improvements:

- [ ] Preview pane - Show file content in skim preview
- [ ] Tag statistics - Show file count per tag
- [ ] Recent selections - Remember last used tags
- [ ] Custom search queries - Complex tag expressions
- [ ] Export results - Save selections to file
- [ ] Actions on selection - Open, copy, delete files directly
- [ ] Tag counts - Store tag→count mapping for statistics
- [ ] Prefix search - Use key prefixes for tag autocomplete
- [ ] LRU cache - In-memory cache for hot tags