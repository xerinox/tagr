//! Tags command - global tag management

use crate::{
    db::Database,
    cli::TagsCommands,
    output,
    TagrError,
};
use dialoguer::Confirm;

type Result<T> = std::result::Result<T, TagrError>;

/// Execute the tags management command
pub fn execute(db: &Database, command: &TagsCommands, quiet: bool) -> Result<()> {
    match command {
        TagsCommands::List => list_all_tags(db, quiet),
        TagsCommands::Remove { tag } => remove_tag_globally(db, tag, quiet),
    }
}

fn list_all_tags(db: &Database, quiet: bool) -> Result<()> {
    let tags = db.list_all_tags()?;
    
    if tags.is_empty() {
        if !quiet {
            println!("No tags found in database.");
        }
    } else {
        if !quiet {
            println!("Tags in database:");
        }
        for tag in tags {
            let count = db.find_by_tag(&tag)?.len();
            println!("{}", output::tag_with_count(&tag, count, quiet));
        }
    }
    Ok(())
}

fn remove_tag_globally(db: &Database, tag: &str, quiet: bool) -> Result<()> {
    let files_before = db.find_by_tag(tag)?;
    
    if files_before.is_empty() {
        if !quiet {
            println!("Tag '{tag}' not found in database.");
        }
        return Ok(());
    }
    
    if !quiet {
        println!("Found tag '{tag}' in {} file(s):", files_before.len());
        for file in &files_before {
            println!("  - {}", file.display());
        }
        println!();
    }
    
    if !confirm("Remove tag from all files?", quiet)? {
        if !quiet {
            println!("Cancelled.");
        }
        return Ok(());
    }
    
    let files_removed = db.remove_tag_globally(tag)?;
    
    if !quiet {
        println!("Removed tag '{tag}' from {} file(s).", files_before.len());
        if files_removed > 0 {
            println!("Cleaned up {files_removed} file(s) with no remaining tags.");
        }
    }
    Ok(())
}

/// Prompt user for yes/no confirmation using dialoguer
fn confirm(prompt: &str, quiet: bool) -> Result<bool> {
    if quiet {
        return Ok(true);
    }
    
    Confirm::new()
        .with_prompt(prompt)
        .interact()
        .map_err(|e| TagrError::InvalidInput(format!("Confirmation failed: {}", e)))
}
