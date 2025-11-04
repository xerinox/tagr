//! Demo showing interactive browse mode
//! 
//! This example populates the database with test data and then
//! launches the interactive browse mode to demonstrate fuzzy finding

use tagr::db::Database;
use tagr::search;

fn main() {
    println!("=== Tagr Browse Mode Demo ===\n");
    
    let db = Database::open("my_db").expect("Failed to open database");
    
    db.clear().expect("Failed to clear database");
    
    println!("Populating database with test data...");
    
    db.insert("docs/rust_tutorial.md", vec!["rust".into(), "tutorial".into(), "programming".into()]).unwrap();
    db.insert("docs/rust_advanced.md", vec!["rust".into(), "advanced".into(), "programming".into()]).unwrap();
    db.insert("docs/python_basics.md", vec!["python".into(), "tutorial".into(), "programming".into()]).unwrap();
    db.insert("docs/web_development.md", vec!["webdev".into(), "javascript".into(), "tutorial".into()]).unwrap();
    db.insert("src/main.rs", vec!["rust".into(), "code".into(), "source".into()]).unwrap();
    db.insert("src/lib.rs", vec!["rust".into(), "code".into(), "source".into()]).unwrap();
    db.insert("scripts/deploy.sh", vec!["bash".into(), "deployment".into(), "automation".into()]).unwrap();
    db.insert("scripts/test.py", vec!["python".into(), "testing".into(), "automation".into()]).unwrap();
    db.insert("config/app.toml", vec!["config".into(), "toml".into()]).unwrap();
    db.insert("README.md", vec!["documentation".into(), "markdown".into()]).unwrap();
    
    println!("Inserted {} files with {} unique tags\n", db.count(), db.list_all_tags().unwrap().len());
    
    println!("Available tags:");
    for tag in db.list_all_tags().unwrap() {
        let count = db.find_by_tag(&tag).unwrap().len();
        println!("  - {} ({} files)", tag, count);
    }
    
    println!("\n=== Starting Interactive Browse Mode ===");
    println!("Instructions:");
    println!("  - Use arrow keys to navigate");
    println!("  - Press TAB to select/deselect items (multi-select enabled)");
    println!("  - Press Enter to confirm selection");
    println!("  - Press ESC or Ctrl+C to cancel\n");
    
    match search::browse(&db) {
        Ok(Some(result)) => {
            println!("\n=== Browse Results ===");
            println!("\nSelected Tags ({}):", result.selected_tags.len());
            for tag in &result.selected_tags {
                println!("  ✓ {}", tag);
            }
            
            println!("\nSelected Files ({}):", result.selected_files.len());
            for file in &result.selected_files {
                if let Some(tags) = db.get_tags(file).unwrap() {
                    println!("  ✓ {} [{}]", file.display(), tags.join(", "));
                }
            }
        }
        Ok(None) => {
            println!("\nBrowse cancelled by user.");
        }
        Err(e) => {
            eprintln!("\nError during browse: {}", e);
            std::process::exit(1);
        }
    }
}
