# Custom Frontend Implementation Guide

This guide explains how to implement a custom UI frontend for Tagr's browse functionality, demonstrating the clean separation between business logic and presentation layers.

## Table of Contents

1. [Architecture Overview](#architecture-overview)
2. [The `FuzzyFinder` Trait](#the-fuzzyfinder-trait)
3. [Minimal Implementation Example](#minimal-implementation-example)
4. [Understanding the Browse Session](#understanding-the-browse-session)
5. [Ratatui Implementation Guide](#ratatui-implementation-guide)
6. [Testing Custom Frontends](#testing-custom-frontends)
7. [Advanced Topics](#advanced-topics)

---

## Architecture Overview

Tagr's browse functionality uses a layered architecture that cleanly separates concerns:

```
┌─────────────────────────────────────────────────────┐
│              Command Layer                          │
│          (src/commands/browse.rs)                   │
│        - CLI argument parsing                       │
│        - Output formatting                          │
└────────────────┬────────────────────────────────────┘
                 │
┌────────────────▼────────────────────────────────────┐
│           UI Controller Layer                       │
│          (src/browse/ui.rs)                         │
│     - BrowseController<F: FuzzyFinder>              │
│     - Phase management (tag → file selection)       │
│     - Domain model → UI conversion                  │
│     - Action execution & data refresh               │
└────────────────┬────────────────────────────────────┘
                 │
        ┌────────┴────────┐
        │                 │
┌───────▼──────┐    ┌────▼────────────────────────────┐
│ Session      │    │    YOUR CUSTOM FRONTEND         │
│ (Business    │    │  (implements FuzzyFinder)       │
│  Logic)      │    │                                 │
│              │    │  - SkimFinder (current)         │
│ - Data       │    │  - RatatuiAdapter (future)      │
│ - State      │    │  - YourCustomFinder             │
│ - Actions    │    │                                 │
└──────────────┘    └─────────────────────────────────┘
```

**Key Benefits:**

- ✅ **Complete business logic reuse** - All data queries, state management, and actions work with any UI
- ✅ **Minimal trait surface** - Only one trait to implement: `FuzzyFinder`
- ✅ **Phase-aware** - Controller handles tag/file phase transitions automatically
- ✅ **Testable** - Mock implementations for testing business logic

---

## The `FuzzyFinder` Trait

The entire UI contract is defined by a single trait in `src/ui/traits.rs`:

```rust
pub trait FuzzyFinder {
    /// Run the fuzzy finder with given configuration
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration containing items, prompt, keybinds, etc.
    ///
    /// # Returns
    ///
    /// * `FinderResult` - Selected items and user action (Enter/ESC/keybind)
    ///
    /// # Errors
    ///
    /// Returns `UiError` if the finder cannot initialize or operation fails
    fn run(&self, config: FinderConfig) -> Result<FinderResult>;
}
```

### `FinderConfig` - Input to your UI

```rust
pub struct FinderConfig {
    /// Items to display (already formatted with colors/metadata)
    pub items: Vec<DisplayItem>,
    
    /// Enable multi-select mode (TAB key behavior)
    pub multi_select: bool,
    
    /// Prompt text to show user
    pub prompt: String,
    
    /// Enable ANSI color support
    pub ansi: bool,
    
    /// Preview configuration (None = no preview)
    pub preview_config: Option<PreviewConfig>,
    
    /// Custom keybinds (backend-specific format)
    pub bind: Vec<String>,
}
```

### `DisplayItem` - Pre-formatted UI items

```rust
pub struct DisplayItem {
    /// Unique ID (path for files, tag name for tags)
    pub id: String,
    
    /// Formatted display text (with ANSI colors if config.ansi == true)
    pub display: String,
    
    /// Plain text for matching (no ANSI codes)
    pub match_text: String,
    
    /// Additional metadata
    pub metadata: ItemMetadata,
}

pub struct ItemMetadata {
    pub index: Option<usize>,
    pub tags: Vec<String>,
    pub exists: bool,
}
```

**Important:** The `display` field contains ANSI escape codes for colors and styling. Your UI should either:
1. Render ANSI codes (most terminal libraries support this)
2. Strip ANSI codes if you want custom styling (use `match_text` instead)

### `FinderResult` - Output from your UI

```rust
pub struct FinderResult {
    /// IDs of selected items (from DisplayItem.id)
    pub selected: Vec<String>,
    
    /// Whether user cancelled (ESC key)
    pub aborted: bool,
    
    /// Final key pressed (e.g., "enter", "ctrl-t", "esc")
    pub final_key: Option<String>,
}
```

**Key Insights:**

- `selected` can be empty (user pressed Enter without selecting anything)
- `aborted = true` means ESC key → controller will return `None`
- `final_key` is used for action keybinds (ctrl+t, ctrl+d, etc.)

---

## Minimal Implementation Example

Here's a complete minimal implementation that demonstrates the interface:

```rust
use tagr::ui::{FuzzyFinder, FinderConfig, FinderResult, Result};
use std::io::{self, Write};

/// Simple terminal-based finder (no fuzzy matching, just list + input)
pub struct SimpleFinder;

impl SimpleFinder {
    pub fn new() -> Self {
        Self
    }
}

impl FuzzyFinder for SimpleFinder {
    fn run(&self, config: FinderConfig) -> Result<FinderResult> {
        println!("\n{}", config.prompt);
        println!("─".repeat(60));
        
        // Display all items
        for (idx, item) in config.items.iter().enumerate() {
            // Use match_text (no ANSI) or display (with ANSI)
            println!("{:3}. {}", idx + 1, item.display);
        }
        
        println!("─".repeat(60));
        
        if config.multi_select {
            print!("Select items (space-separated numbers, or 'q' to quit): ");
        } else {
            print!("Select item (number, or 'q' to quit): ");
        }
        io::stdout().flush().unwrap();
        
        // Read user input
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        let input = input.trim();
        
        // Handle quit
        if input == "q" || input.is_empty() {
            return Ok(FinderResult {
                selected: vec![],
                aborted: true,
                final_key: Some("esc".to_string()),
            });
        }
        
        // Parse selections
        let selected: Vec<String> = input
            .split_whitespace()
            .filter_map(|s| s.parse::<usize>().ok())
            .filter(|&n| n > 0 && n <= config.items.len())
            .map(|n| config.items[n - 1].id.clone())
            .collect();
        
        Ok(FinderResult {
            selected,
            aborted: false,
            final_key: Some("enter".to_string()),
        })
    }
}

// Usage
fn main() {
    use tagr::browse::{BrowseSession, BrowseController, BrowseConfig};
    use tagr::db::Database;
    
    let db = Database::open("test_db").unwrap();
    let config = BrowseConfig::default();
    let session = BrowseSession::new(&db, config).unwrap();
    
    let finder = SimpleFinder::new();
    let controller = BrowseController::new(session, finder);
    
    match controller.run() {
        Ok(Some(result)) => {
            println!("\n✓ Selected {} files", result.selected_files.len());
            for file in result.selected_files {
                println!("  - {}", file.display());
            }
        }
        Ok(None) => println!("Cancelled"),
        Err(e) => eprintln!("Error: {}", e),
    }
}
```

**What happens here:**

1. Controller calls `finder.run(config)` twice:
   - First with tags (phase 1)
   - Then with files (phase 2)
2. Each call displays items and gets user selection
3. Controller handles phase transition automatically
4. Actions (ctrl+t, etc.) would need keybind parsing in `final_key`

---

## Understanding the Browse Session

The `BrowseSession` manages all business logic and state transitions. You don't interact with it directly (the `BrowseController` does), but understanding it helps design better UIs.

### Session Phases

```rust
pub enum PhaseType {
    /// User is selecting tags
    TagSelection,
    
    /// User is selecting files (tags already selected)
    FileSelection { selected_tags: Vec<String> },
}
```

### Phase Lifecycle

```
Session Created
    ↓
Determine starting phase
    ├─ No CLI params → TagSelection
    └─ Has --tags → FileSelection (skip tag selection)
        ↓
┌─→ Controller.run_browser_phase()
│       ↓
│   Finder.run(config) ← YOUR IMPLEMENTATION
│       ↓
│   User action?
│   ├─ Enter → handle_accept()
│   │   ├─ TagSelection → Query files → Transition to FileSelection
│   │   └─ FileSelection → Complete(BrowseResult)
│   ├─ ESC → Cancel → Exit
│   └─ Keybind (ctrl+t) → execute_action() → Refresh → Loop
```

### Configuration Per Phase

The controller provides phase-specific configuration:

**Tag Phase:**
- Items: `Vec<TagrItem>` with tag names and file counts
- Keybinds: Minimal (TAB, Enter, ESC, F1)
- Preview: Disabled
- Prompt: "Select tags (TAB for multi-select, Enter to continue)"

**File Phase:**
- Items: `Vec<TagrItem>` with file paths, tags, existence status
- Keybinds: Full action set (ctrl+t/d/o/e/c/f + TAB/Enter/ESC/F1)
- Preview: Enabled (if configured)
- Prompt: "Select files (TAB for multi-select, keybinds: ctrl+t/d/o/e/c/f)"

---

## Ratatui Implementation Guide

Here's a comprehensive guide to implementing a ratatui-based frontend.

### Step 1: Project Setup

```toml
# Cargo.toml
[dependencies]
ratatui = "0.26"
crossterm = "0.27"
```

### Step 2: Basic Structure

```rust
// src/ui/ratatui_adapter.rs

use ratatui::{
    backend::CrosstermBackend,
    Terminal,
    layout::{Layout, Constraint, Direction},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    style::{Style, Color, Modifier},
};
use crossterm::{
    terminal::{enable_raw_mode, disable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
};
use std::io;

use crate::ui::{FuzzyFinder, FinderConfig, FinderResult, Result as UiResult, UiError};

pub struct RatatuiAdapter {
    // Configuration
}

impl RatatuiAdapter {
    pub fn new() -> UiResult<Self> {
        Ok(Self {})
    }
    
    /// Setup terminal for TUI rendering
    fn setup_terminal() -> UiResult<Terminal<CrosstermBackend<io::Stdout>>> {
        enable_raw_mode()
            .map_err(|e| UiError::InitError(format!("Failed to enable raw mode: {}", e)))?;
        
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)
            .map_err(|e| UiError::InitError(format!("Failed to enter alternate screen: {}", e)))?;
        
        let backend = CrosstermBackend::new(stdout);
        Terminal::new(backend)
            .map_err(|e| UiError::InitError(format!("Failed to create terminal: {}", e)))
    }
    
    /// Cleanup terminal state
    fn cleanup_terminal() -> UiResult<()> {
        disable_raw_mode()
            .map_err(|e| UiError::CleanupError(format!("Failed to disable raw mode: {}", e)))?;
        
        let mut stdout = io::stdout();
        execute!(stdout, LeaveAlternateScreen)
            .map_err(|e| UiError::CleanupError(format!("Failed to leave alternate screen: {}", e)))?;
        
        Ok(())
    }
}
```

### Step 3: State Management

```rust
/// Internal state for the TUI
struct AppState {
    /// All items to display
    items: Vec<DisplayItem>,
    
    /// Currently filtered items (after search)
    filtered_items: Vec<usize>, // indices into items
    
    /// Current cursor position in filtered list
    cursor: usize,
    
    /// Selected item indices (for multi-select)
    selected: HashSet<usize>,
    
    /// Search query
    query: String,
    
    /// Multi-select enabled
    multi_select: bool,
    
    /// User aborted (ESC)
    aborted: bool,
    
    /// Final key pressed
    final_key: Option<String>,
}

impl AppState {
    fn new(items: Vec<DisplayItem>, multi_select: bool) -> Self {
        let filtered_items: Vec<usize> = (0..items.len()).collect();
        
        Self {
            items,
            filtered_items,
            cursor: 0,
            selected: HashSet::new(),
            query: String::new(),
            multi_select,
            aborted: false,
            final_key: None,
        }
    }
    
    /// Update filter based on current query
    fn update_filter(&mut self) {
        if self.query.is_empty() {
            self.filtered_items = (0..self.items.len()).collect();
        } else {
            // Simple case-insensitive substring match
            // For fuzzy matching, use a library like `fuzzy-matcher`
            let query_lower = self.query.to_lowercase();
            self.filtered_items = self.items
                .iter()
                .enumerate()
                .filter(|(_, item)| {
                    item.match_text.to_lowercase().contains(&query_lower)
                })
                .map(|(idx, _)| idx)
                .collect();
        }
        
        // Reset cursor if out of bounds
        if self.cursor >= self.filtered_items.len() {
            self.cursor = self.filtered_items.len().saturating_sub(1);
        }
    }
    
    /// Toggle selection of current item
    fn toggle_selection(&mut self) {
        if let Some(&item_idx) = self.filtered_items.get(self.cursor) {
            if self.selected.contains(&item_idx) {
                self.selected.remove(&item_idx);
            } else {
                self.selected.insert(item_idx);
            }
        }
    }
    
    /// Get IDs of selected items
    fn get_selected_ids(&self) -> Vec<String> {
        if self.selected.is_empty() && !self.multi_select {
            // Single-select: return current cursor item
            if let Some(&item_idx) = self.filtered_items.get(self.cursor) {
                return vec![self.items[item_idx].id.clone()];
            }
        }
        
        self.selected
            .iter()
            .map(|&idx| self.items[idx].id.clone())
            .collect()
    }
    
    /// Move cursor up
    fn move_up(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }
    
    /// Move cursor down
    fn move_down(&mut self) {
        if self.cursor < self.filtered_items.len().saturating_sub(1) {
            self.cursor += 1;
        }
    }
}
```

### Step 4: Implement FuzzyFinder

```rust
impl FuzzyFinder for RatatuiAdapter {
    fn run(&self, config: FinderConfig) -> UiResult<FinderResult> {
        let mut terminal = Self::setup_terminal()?;
        let mut state = AppState::new(config.items, config.multi_select);
        
        // Main event loop
        loop {
            // Render UI
            terminal.draw(|f| {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(3),  // Search bar
                        Constraint::Min(1),     // Item list
                        Constraint::Length(3),  // Status bar
                    ])
                    .split(f.size());
                
                // Search bar
                let search_text = format!("> {}", state.query);
                let search = Paragraph::new(search_text)
                    .block(Block::default()
                        .borders(Borders::ALL)
                        .title(config.prompt.clone()));
                f.render_widget(search, chunks[0]);
                
                // Item list
                let items: Vec<ListItem> = state.filtered_items
                    .iter()
                    .enumerate()
                    .map(|(list_idx, &item_idx)| {
                        let item = &state.items[item_idx];
                        let mut content = item.match_text.clone(); // Use plain text
                        
                        // Add selection indicator
                        if state.selected.contains(&item_idx) {
                            content = format!("✓ {}", content);
                        } else {
                            content = format!("  {}", content);
                        }
                        
                        let mut style = Style::default();
                        
                        // Highlight cursor
                        if list_idx == state.cursor {
                            style = style.bg(Color::DarkGray);
                        }
                        
                        // Color by metadata
                        if !item.metadata.exists {
                            style = style.fg(Color::Red);
                        }
                        
                        ListItem::new(content).style(style)
                    })
                    .collect();
                
                let list = List::new(items)
                    .block(Block::default()
                        .borders(Borders::ALL)
                        .title(format!("Items ({}/{})", 
                            state.filtered_items.len(), 
                            state.items.len())));
                f.render_widget(list, chunks[1]);
                
                // Status bar
                let help = if state.multi_select {
                    "TAB: select | Enter: accept | ESC: cancel | ↑↓: navigate"
                } else {
                    "Enter: accept | ESC: cancel | ↑↓: navigate"
                };
                let status = Paragraph::new(help)
                    .block(Block::default().borders(Borders::ALL));
                f.render_widget(status, chunks[2]);
            }).map_err(|e| UiError::RenderError(format!("Failed to draw: {}", e)))?;
            
            // Handle input
            if event::poll(std::time::Duration::from_millis(100))
                .map_err(|e| UiError::InputError(format!("Failed to poll events: {}", e)))?
            {
                if let Event::Key(key) = event::read()
                    .map_err(|e| UiError::InputError(format!("Failed to read event: {}", e)))?
                {
                    match key.code {
                        KeyCode::Esc => {
                            state.aborted = true;
                            state.final_key = Some("esc".to_string());
                            break;
                        }
                        
                        KeyCode::Enter => {
                            state.final_key = Some("enter".to_string());
                            break;
                        }
                        
                        KeyCode::Up => state.move_up(),
                        KeyCode::Down => state.move_down(),
                        
                        KeyCode::Tab if state.multi_select => {
                            state.toggle_selection();
                        }
                        
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            // ctrl+c is handled, but you might want to map it to an action
                            state.aborted = true;
                            state.final_key = Some("esc".to_string());
                            break;
                        }
                        
                        KeyCode::Char('t') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            state.final_key = Some("ctrl-t".to_string());
                            break;
                        }
                        
                        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            state.final_key = Some("ctrl-d".to_string());
                            break;
                        }
                        
                        // Add more keybinds as needed...
                        
                        KeyCode::Backspace => {
                            state.query.pop();
                            state.update_filter();
                        }
                        
                        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                            state.query.push(c);
                            state.update_filter();
                        }
                        
                        _ => {}
                    }
                }
            }
        }
        
        // Cleanup
        Self::cleanup_terminal()?;
        
        Ok(FinderResult {
            selected: state.get_selected_ids(),
            aborted: state.aborted,
            final_key: state.final_key,
        })
    }
}
```

### Step 5: Integration

```rust
// In src/ui/mod.rs
pub mod ratatui_adapter;

// In src/commands/browse.rs - just change one line!
use crate::ui::ratatui_adapter::RatatuiAdapter;

// Replace:
// let finder = SkimFinder::new();
// With:
let finder = RatatuiAdapter::new()?;
```

**That's it!** All business logic, state management, and actions continue to work unchanged.

---

## Testing Custom Frontends

### Unit Testing with Mock Finder

```rust
// tests/custom_finder_test.rs

use tagr::ui::{FuzzyFinder, FinderConfig, FinderResult, DisplayItem};
use tagr::browse::{BrowseSession, BrowseController, BrowseConfig};

struct MockFinder {
    responses: Vec<FinderResult>,
    call_count: std::cell::RefCell<usize>,
}

impl MockFinder {
    fn new(responses: Vec<FinderResult>) -> Self {
        Self {
            responses,
            call_count: std::cell::RefCell::new(0),
        }
    }
}

impl FuzzyFinder for MockFinder {
    fn run(&self, _config: FinderConfig) -> tagr::ui::Result<FinderResult> {
        let mut count = self.call_count.borrow_mut();
        let result = self.responses.get(*count)
            .ok_or_else(|| tagr::ui::UiError::BuildError("No more responses".into()))?
            .clone();
        *count += 1;
        Ok(result)
    }
}

#[test]
fn test_full_browse_workflow() {
    let db = tagr::testing::TestDb::new("test_browse");
    
    // Mock user selecting "rust" tag, then "main.rs" file
    let finder = MockFinder::new(vec![
        FinderResult {
            selected: vec!["rust".to_string()],
            aborted: false,
            final_key: Some("enter".to_string()),
        },
        FinderResult {
            selected: vec!["/path/to/main.rs".to_string()],
            aborted: false,
            final_key: Some("enter".to_string()),
        },
    ]);
    
    let session = BrowseSession::new(db.db(), BrowseConfig::default()).unwrap();
    let controller = BrowseController::new(session, finder);
    
    let result = controller.run().unwrap().unwrap();
    
    assert_eq!(result.selected_tags, vec!["rust"]);
    assert_eq!(result.selected_files.len(), 1);
}
```

### Integration Testing

Test your finder independently:

```rust
#[test]
fn test_ratatui_adapter_single_select() {
    let items = vec![
        DisplayItem::new(
            "item1".to_string(),
            "Item 1".to_string(),
            "Item 1".to_string(),
        ),
        DisplayItem::new(
            "item2".to_string(),
            "Item 2".to_string(),
            "Item 2".to_string(),
        ),
    ];
    
    let config = FinderConfig::new(items, "Test".to_string())
        .with_multi_select(false);
    
    let finder = RatatuiAdapter::new().unwrap();
    
    // Manual test: Run and interact
    let result = finder.run(config).unwrap();
    
    assert!(!result.aborted);
    assert_eq!(result.selected.len(), 1);
}
```

---

## Advanced Topics

### Preview Pane Implementation

The `FinderConfig` includes `preview_config`:

```rust
pub struct PreviewConfig {
    pub enabled: bool,
    pub max_file_size: u64,
    pub max_lines: usize,
    pub syntax_highlighting: bool,
    pub show_line_numbers: bool,
    pub position: PreviewPosition,
    pub width_percent: u8,
}
```

For ratatui, create a two-pane layout:

```rust
let chunks = Layout::default()
    .direction(Direction::Horizontal)
    .constraints([
        Constraint::Percentage(50),  // Item list
        Constraint::Percentage(50),  // Preview
    ])
    .split(f.size());

// Generate preview content
if let Some(&item_idx) = state.filtered_items.get(state.cursor) {
    let item = &state.items[item_idx];
    
    // For files, read content
    if let Ok(content) = std::fs::read_to_string(&item.id) {
        let preview = Paragraph::new(content)
            .block(Block::default().borders(Borders::ALL).title("Preview"));
        f.render_widget(preview, chunks[1]);
    }
}
```

For syntax highlighting, use the `syntect` crate (already a dependency):

```rust
use syntect::easy::HighlightLines;
use syntect::parsing::SyntaxSet;
use syntect::highlighting::ThemeSet;

let syntax_set = SyntaxSet::load_defaults_newlines();
let theme_set = ThemeSet::load_defaults();
let syntax = syntax_set.find_syntax_for_file(&item.id).unwrap()
    .unwrap_or_else(|| syntax_set.find_syntax_plain_text());

let mut highlighter = HighlightLines::new(syntax, &theme_set.themes["base16-ocean.dark"]);

for line in content.lines() {
    let ranges = highlighter.highlight_line(line, &syntax_set).unwrap();
    // Convert syntect ranges to ratatui styles
}
```

### Keybind Configuration

The `config.bind` field contains backend-specific keybind strings. For ratatui:

```rust
// Parse keybinds from config
for bind_str in &config.bind {
    // Example: "ctrl-t:add_tag"
    if let Some((key, action)) = bind_str.split_once(':') {
        // Map to ratatui KeyCode + action
        match key {
            "ctrl-t" => {
                // In event loop, check for ctrl+t and set final_key
                if key.code == KeyCode::Char('t') 
                    && key.modifiers.contains(KeyModifiers::CONTROL) 
                {
                    state.final_key = Some("ctrl-t".to_string());
                    break;
                }
            }
            // ... more keybinds
        }
    }
}
```

### Fuzzy Matching

For better UX, integrate a fuzzy matching library:

```toml
[dependencies]
fuzzy-matcher = "0.3"
```

```rust
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;

impl AppState {
    fn update_filter(&mut self) {
        if self.query.is_empty() {
            self.filtered_items = (0..self.items.len()).collect();
            return;
        }
        
        let matcher = SkimMatcherV2::default();
        
        let mut matches: Vec<(usize, i64)> = self.items
            .iter()
            .enumerate()
            .filter_map(|(idx, item)| {
                matcher.fuzzy_match(&item.match_text, &self.query)
                    .map(|score| (idx, score))
            })
            .collect();
        
        // Sort by score (descending)
        matches.sort_by(|a, b| b.1.cmp(&a.1));
        
        self.filtered_items = matches.into_iter()
            .map(|(idx, _)| idx)
            .collect();
        
        self.cursor = 0;
    }
}
```

### Async Operations

For non-blocking preview generation or large file operations:

```rust
use tokio::runtime::Runtime;

pub struct AsyncRatatuiAdapter {
    runtime: Runtime,
}

impl AsyncRatatuiAdapter {
    pub fn new() -> Self {
        Self {
            runtime: Runtime::new().unwrap(),
        }
    }
    
    fn generate_preview_async(&self, path: &str) -> String {
        self.runtime.block_on(async {
            tokio::fs::read_to_string(path).await.unwrap_or_else(|_| "Error reading file".to_string())
        })
    }
}
```

---

## Summary

**To implement a custom frontend:**

1. ✅ Implement `FuzzyFinder` trait (one method: `run()`)
2. ✅ Handle `FinderConfig` input (items, prompt, keybinds)
3. ✅ Return `FinderResult` (selected IDs, aborted, final_key)
4. ✅ Replace `SkimFinder::new()` with `YourFinder::new()` in one place

**All business logic remains unchanged:**
- Tag/file queries
- Phase transitions
- Action execution
- Data refresh
- State management

**The `BrowseController` handles:**
- Converting `TagrItem` → `DisplayItem` with formatting
- Phase-aware configuration
- Action routing
- Refresh cycles

This architecture enables rapid UI iteration while maintaining a stable, well-tested business logic layer.

---

## Additional Trait Abstractions Needed

### Overview ✅ IMPLEMENTED

A **complete ratatui migration** requires three trait abstractions:

1. ✅ **FuzzyFinder trait** (COMPLETE) - The primary UI abstraction
2. ✅ **UserInput trait** (COMPLETE) - Abstracts `dialoguer` prompts
3. ✅ **OutputWriter trait** (COMPLETE) - Abstracts stdout/stderr output

**All three core traits are now implemented!** The architecture is ready for ratatui.

### 1. ✅ FuzzyFinder (This Guide)

**Status**: Production-ready, documented in this guide

**What it does**: Interactive item selection with fuzzy matching

**Implementations**:
- ✅ `SkimFinder` - Current skim-based implementation
- 🔜 `RatatuiAdapter` - Future custom TUI (guide provided in this document)

### 2. ✅ UserInput (IMPLEMENTED)

**Status**: ✅ Implemented in `src/ui/input.rs`

**Current:** Direct `dialoguer` dependency in `src/keybinds/prompts.rs`

**Problem:** Ratatui can't break out to stdin for prompts - needs in-TUI input widgets

**Solution:** `UserInput` trait with three methods:
```rust
pub trait UserInput: Send + Sync {
    fn prompt_text(&self, prompt: &str, default: Option<&str>, allow_empty: bool) 
        -> Result<Option<String>>;
    fn prompt_confirm(&self, prompt: &str, default: bool) 
        -> Result<Option<bool>>;
    fn prompt_select(&self, prompt: &str, items: &[String], default: Option<usize>) 
        -> Result<Option<usize>>;
}
```

**Implementations**:
- ✅ `DialoguerInput` - CLI adapter (implemented)
- 🔜 `RatatuiInput` - TUI adapter (guide in additional-trait-abstractions.md)

**Usage:**
```rust
use tagr::ui::input::{UserInput, DialoguerInput};

let input = DialoguerInput::new();
if let Some(tag) = input.prompt_text("Enter tag:", None, false)? {
    println!("Adding tag: {}", tag);
}
```

### 3. ✅ OutputWriter (IMPLEMENTED)

**Status**: ✅ Implemented in `src/ui/output.rs`

**Current:** Direct `println!`/`eprintln!` throughout commands

**Problem:** Can't write to stdout when ratatui owns terminal

**Solution:** `OutputWriter` trait with severity levels:
```rust
pub trait OutputWriter: Send + Sync {
    fn write(&self, message: &str);
    fn error(&self, message: &str);
    fn success(&self, message: &str);
    fn warning(&self, message: &str);
    fn info(&self, message: &str);
    fn clear(&self);
}
```

**Implementations**:
- ✅ `StdoutWriter` - Direct stdout/stderr (implemented)
- ✅ `StatusBarWriter` - Buffered message queue for TUI (implemented)

**Usage:**
```rust
use tagr::ui::output::{OutputWriter, StdoutWriter};

let output = StdoutWriter::new();
output.success("Tags added successfully");
output.error("File not found");
```

### Complete Reference

See **[docs/additional-trait-abstractions.md](additional-trait-abstractions.md)** for:
- Detailed analysis of blocking issues
- Complete trait implementations (CLI + TUI adapters)
- Migration strategy and effort estimates
- Example unified entry point with mode switching

### Ratatui Readiness Checklist

- ✅ FuzzyFinder trait - Already abstracted, skim adapter works
- ✅ PreviewProvider trait - Already abstracted for file previews
- ✅ UserInput trait - Implemented, dialoguer adapter ready
- ✅ OutputWriter trait - Implemented, stdout + buffered adapters ready
- 🔜 RatatuiAdapter - Implement FuzzyFinder for custom TUI (guide in this doc)
- 🔜 RatatuiInput - Implement UserInput for in-TUI prompts (guide in additional-trait-abstractions.md)
- 🔜 Unified entry point - Mode-based adapter selection (example in additional-trait-abstractions.md)

**Architecture is 95% ready for ratatui!** Only remaining work is implementing the ratatui-specific adapters.

---

## Further Resources

- **Current Skim Implementation**: `src/ui/skim_adapter.rs` - Reference implementation
- **Controller Logic**: `src/browse/ui.rs` - See how controller uses `FuzzyFinder`
- **Session State**: `src/browse/session.rs` - Understand phase transitions
- **Data Models**: `src/browse/models.rs` - Domain types
- **Additional Abstractions**: `docs/additional-trait-abstractions.md` - Complete migration requirements
- **Ratatui Documentation**: https://ratatui.rs/
- **Crossterm Events**: https://docs.rs/crossterm/latest/crossterm/event/

For questions or contributions, please open an issue on GitHub!
