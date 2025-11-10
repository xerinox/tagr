//! List command - list files or tags in the database

use crate::{
    db::Database,
    cli::ListVariant,
    config,
    output,
    TagrError,
};

type Result<T> = std::result::Result<T, TagrError>;

/// Execute the list command
pub fn execute(
    db: &Database,
    variant: ListVariant,
    path_format: config::PathFormat,
    quiet: bool,
) -> Result<()> {
    match variant {
        ListVariant::Files => list_files(db, path_format, quiet),
        ListVariant::Tags => list_tags(db, quiet),
    }
}

fn list_files(db: &Database, path_format: config::PathFormat, quiet: bool) -> Result<()> {
    let all_pairs = db.list_all()?;
    
    if all_pairs.is_empty() {
        if !quiet {
            println!("No files found in database.");
        }
    } else {
        if !quiet {
            println!("Files in database:");
        }
        for pair in all_pairs {
            println!("{}", output::file_with_tags(&pair.file, &pair.tags, path_format, quiet));
        }
    }
    Ok(())
}

fn list_tags(db: &Database, quiet: bool) -> Result<()> {
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
