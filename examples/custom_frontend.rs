//! Example: Custom `FuzzyFinder` Implementation
//!
//! This example demonstrates how to implement a custom frontend for tagr's
//! browse functionality by implementing the `FuzzyFinder` trait.
//!
//! Run with:
//! ```bash
//! cargo run --example custom_frontend
//! ```

use std::collections::HashSet;
use std::io::{self, Write};
use tagr::Pair;
use tagr::browse::{BrowseConfig, BrowseController, BrowseSession};
use tagr::db::Database;
use tagr::ui::{DisplayItem, FinderConfig, FinderResult, FuzzyFinder, Result as UiResult};

/// Simple terminal-based finder without fuzzy matching
///
/// This implementation displays items as a numbered list and accepts
/// user input as space-separated numbers.
struct SimpleFinder;

impl SimpleFinder {
    const fn new() -> Self {
        Self
    }
}

impl FuzzyFinder for SimpleFinder {
    fn run(&self, config: FinderConfig) -> UiResult<FinderResult> {
        println!("\n{}", config.prompt);
        println!("{}", "─".repeat(60));

        // Display all items with numbers
        for (idx, item) in config.items.iter().enumerate() {
            // Use display field for colored output (with ANSI codes)
            println!("{:3}. {}", idx + 1, item.display);
        }

        println!("{}", "─".repeat(60));

        // Show appropriate prompt based on multi-select mode
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

        // Handle quit/empty
        if input == "q" || input.is_empty() {
            return Ok(FinderResult {
                selected: vec![],
                aborted: true,
                final_key: Some("esc".to_string()),
                refine_search: None,
                input_action: None,
                direct_file_selection: false,
                selected_tags: vec![],
            });
        }

        // Parse selections
        let selected: Vec<String> = input
            .split_whitespace()
            .filter_map(|s| s.parse::<usize>().ok())
            .filter(|&n| n > 0 && n <= config.items.len())
            .map(|n| config.items[n - 1].key.clone())
            .collect();

        Ok(FinderResult {
            selected,
            aborted: false,
            final_key: Some("enter".to_string()),
            refine_search: None,
            input_action: None,
            direct_file_selection: false,
            selected_tags: vec![],
        })
    }
}

/// More sophisticated finder with filtering
///
/// This implementation adds basic text filtering to narrow down items.
struct FilteringFinder;

impl FilteringFinder {
    const fn new() -> Self {
        Self
    }

    fn filter_items(items: &[DisplayItem], query: &str) -> Vec<usize> {
        if query.is_empty() {
            return (0..items.len()).collect();
        }

        let query_lower = query.to_lowercase();
        items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.searchable.to_lowercase().contains(&query_lower))
            .map(|(idx, _)| idx)
            .collect()
    }
}

impl FuzzyFinder for FilteringFinder {
    fn run(&self, config: FinderConfig) -> UiResult<FinderResult> {
        let mut query = String::new();
        let mut selected_indices: HashSet<usize> = HashSet::new();

        loop {
            // Filter items based on current query
            let filtered = Self::filter_items(&config.items, &query);

            // Clear screen and display
            print!("\x1B[2J\x1B[1;1H"); // ANSI clear screen
            println!("{}", config.prompt);
            println!("Search: {query}_");
            println!("{}", "─".repeat(60));

            if filtered.is_empty() {
                println!("No matches found.");
            } else {
                for (display_idx, &item_idx) in filtered.iter().enumerate() {
                    let item = &config.items[item_idx];
                    let marker = if selected_indices.contains(&item_idx) {
                        "✓"
                    } else {
                        " "
                    };
                    println!("{} {:3}. {}", marker, display_idx + 1, item.display);
                }
            }

            println!("{}", "─".repeat(60));
            if config.multi_select {
                println!("Type to filter | Number to select | Enter to confirm | q to quit");
            } else {
                println!("Type to filter | Number to select | q to quit");
            }
            print!("> ");
            io::stdout().flush().unwrap();

            // Read single line of input
            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();
            let input = input.trim();

            // Handle commands
            if input == "q" {
                return Ok(FinderResult {
                    selected: vec![],
                    aborted: true,
                    final_key: Some("esc".to_string()),
                    refine_search: None,
                    input_action: None,
                    direct_file_selection: false,
                    selected_tags: vec![],
                });
            } else if input.is_empty() {
                // Enter pressed - finalize selection
                let selected: Vec<String> = if selected_indices.is_empty() && !config.multi_select {
                    // Single-select mode: if nothing selected, return first filtered item
                    filtered
                        .first()
                        .map(|&idx| vec![config.items[idx].key.clone()])
                        .unwrap_or_default()
                } else {
                    selected_indices
                        .iter()
                        .map(|&idx| config.items[idx].key.clone())
                        .collect()
                };

                return Ok(FinderResult {
                    selected,
                    aborted: false,
                    final_key: Some("enter".to_string()),
                    refine_search: None,
                    input_action: None,
                    direct_file_selection: false,
                    selected_tags: vec![],
                });
            } else if let Ok(num) = input.parse::<usize>() {
                // Number entered - toggle selection
                if let Some(&item_idx) = filtered.get(num.saturating_sub(1)) {
                    if config.multi_select {
                        if selected_indices.contains(&item_idx) {
                            selected_indices.remove(&item_idx);
                        } else {
                            selected_indices.insert(item_idx);
                        }
                    } else {
                        // Single-select mode - immediately return
                        return Ok(FinderResult {
                            selected: vec![config.items[item_idx].key.clone()],
                            aborted: false,
                            final_key: Some("enter".to_string()),
                            refine_search: None,
                            input_action: None,
                            direct_file_selection: false,
                            selected_tags: vec![],
                        });
                    }
                }
            } else {
                // Text entered - update query
                query = input.to_string();
            }
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Custom Frontend Example ===\n");

    // Create a test database with sample data
    let db = Database::open("example_custom_frontend_db")?;
    db.clear()?;

    // Add some sample files with tags
    db.insert_pair(&Pair {
        file: "src/main.rs".into(),
        tags: vec!["rust".into(), "code".into()],
    })?;
    db.insert_pair(&Pair {
        file: "src/lib.rs".into(),
        tags: vec!["rust".into(), "library".into()],
    })?;
    db.insert_pair(&Pair {
        file: "README.md".into(),
        tags: vec!["docs".into(), "markdown".into()],
    })?;
    db.insert_pair(&Pair {
        file: "Cargo.toml".into(),
        tags: vec!["config".into(), "rust".into()],
    })?;

    println!("Choose a finder implementation:");
    println!("1. SimpleFinder (basic numbered list)");
    println!("2. FilteringFinder (with text search)");
    print!("\nEnter choice (1 or 2): ");
    io::stdout().flush().unwrap();

    let mut choice = String::new();
    io::stdin().read_line(&mut choice)?;

    // Create browse session
    let config = BrowseConfig::default();
    let session = BrowseSession::new(&db, config)?;

    // Run with chosen finder
    let result = match choice.trim() {
        "1" => {
            println!("\n=== Using SimpleFinder ===");
            let finder = SimpleFinder::new();
            let controller = BrowseController::new(session, finder);
            controller.run()?
        }
        "2" => {
            println!("\n=== Using FilteringFinder ===");
            let finder = FilteringFinder::new();
            let controller = BrowseController::new(session, finder);
            controller.run()?
        }
        _ => {
            println!("Invalid choice, using SimpleFinder");
            let finder = SimpleFinder::new();
            let controller = BrowseController::new(session, finder);
            controller.run()?
        }
    };

    // Display results
    match result {
        Some(result) => {
            println!("\n=== Browse Results ===");
            println!("Selected tags: {:?}", result.selected_tags);
            println!("Selected {} files:", result.selected_files.len());
            for file in result.selected_files {
                println!("  - {}", file.display());
            }
        }
        None => {
            println!("\nBrowse cancelled by user");
        }
    }

    // Cleanup
    std::fs::remove_dir_all("example_custom_frontend_db")?;

    Ok(())
}
