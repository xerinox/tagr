//! Example: Browse Mode Demo
//!
//! This example demonstrates the interactive browse functionality using
//! the default skim-based fuzzy finder.
//!
//! Run with:
//! ```bash
//! cargo run --example browse_demo
//! ```

use std::fs;
use std::path::PathBuf;
use tagr::Pair;
use tagr::browse::{BrowseConfig, BrowseController, BrowseSession};
use tagr::db::Database;
use tagr::ui::skim_adapter::SkimFinder;

/// Create sample files in a temporary directory
fn create_sample_files(dir: &PathBuf) -> Result<Vec<PathBuf>, std::io::Error> {
    fs::create_dir_all(dir)?;

    let files = vec![
        (
            "main.rs",
            "fn main() {\n    println!(\"Hello, world!\");\n}\n",
        ),
        (
            "lib.rs",
            "pub mod utils;\npub mod models;\n\npub use utils::*;\n",
        ),
        (
            "utils.rs",
            "pub fn helper() -> String {\n    String::from(\"helper\")\n}\n",
        ),
        ("models.rs", "pub struct User {\n    pub name: String,\n}\n"),
        ("README.md", "# My Project\n\nThis is a sample project.\n"),
        (
            "Cargo.toml",
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
        ),
        (".gitignore", "target/\n*.log\n"),
        (
            "tests/integration_test.rs",
            "#[test]\nfn test_example() {\n    assert!(true);\n}\n",
        ),
    ];

    let mut created_files = Vec::new();
    for (name, content) in files {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, content)?;
        created_files.push(path);
    }

    Ok(created_files)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Tagr Browse Mode Demo ===\n");

    // Create temporary directory for sample files
    let temp_dir = PathBuf::from("example_browse_demo_files");
    let files = create_sample_files(&temp_dir)?;
    println!("Created {} sample files in {:?}", files.len(), temp_dir);

    // Create and populate database
    let db = Database::open("example_browse_demo_db")?;
    db.clear()?;

    // Add files with tags
    let tags_map = vec![
        ("main.rs", vec!["rust", "code", "entry"]),
        ("lib.rs", vec!["rust", "library", "code"]),
        ("utils.rs", vec!["rust", "utility", "code"]),
        ("models.rs", vec!["rust", "data", "code"]),
        ("README.md", vec!["docs", "markdown"]),
        ("Cargo.toml", vec!["config", "rust", "toml"]),
        (".gitignore", vec!["config", "git"]),
        ("tests/integration_test.rs", vec!["rust", "test", "code"]),
    ];

    for (filename, tags) in tags_map {
        let file_path = temp_dir.join(filename);
        db.insert_pair(&Pair {
            file: file_path,
            tags: tags.iter().map(|s| (*s).to_string()).collect(),
        })?;
    }

    println!("Tagged {} files in database", files.len());
    println!("\nAvailable tags:");
    for tag in ["rust", "code", "docs", "config", "test", "markdown", "git"] {
        let count = db.find_by_tag(tag)?.len();
        println!("  - {tag} ({count} files)");
    }

    println!("\n=== Starting Interactive Browse Mode ===");
    println!("Instructions:");
    println!("  1. First, select tags (TAB for multi-select, Enter to continue)");
    println!("  2. Then, select files matching those tags");
    println!("  3. Press ESC at any time to cancel");
    println!("\nAvailable keybinds in file selection:");
    println!("  - TAB: Toggle selection");
    println!("  - ctrl+t: Add tags to selected files");
    println!("  - ctrl+d: Remove from database");
    println!("  - ctrl+o: Open in default application");
    println!("  - ctrl+e: Open in editor");
    println!("  - ctrl+y: Copy path to clipboard");
    println!("  - ctrl+f: Copy files to destination");
    println!("\nPress Enter to start...");

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;

    // Create browse session with default configuration
    let config = BrowseConfig::default();
    let session = BrowseSession::new(&db, config)?;

    // Create skim finder and controller
    let finder = SkimFinder::new();
    let controller = BrowseController::new(session, finder);

    // Run the interactive browse
    match controller.run()? {
        Some(result) => {
            println!("\n=== Browse Results ===");
            println!("Selected tags: {:?}", result.selected_tags);
            println!("\nSelected {} files:", result.selected_files.len());

            let has_files = !result.selected_files.is_empty();
            for file in result.selected_files {
                let exists = file.exists();
                let status = if exists { "✓" } else { "✗" };
                println!("  {} {}", status, file.display());
            }

            if has_files {
                println!("\nYou can now use these file paths in your application!");
            }
        }
        None => {
            println!("\nBrowse cancelled by user (ESC pressed)");
        }
    }

    // Cleanup
    println!("\nCleaning up temporary files...");
    std::fs::remove_dir_all(&temp_dir)?;
    std::fs::remove_dir_all("example_browse_demo_db")?;

    println!("Demo complete!");

    Ok(())
}
