//! Cleanup command - remove missing files and files with no tags

use crate::{
    db::Database,
    config,
    output,
    TagrError,
};
use dialoguer::Select;
use std::path::PathBuf;

type Result<T> = std::result::Result<T, TagrError>;

/// Execute the cleanup command
///
/// # Errors
/// Returns an error if database operations fail or if user interaction fails
pub fn execute(
    db: &Database,
    path_format: config::PathFormat,
    quiet: bool,
) -> Result<()> {
    if !quiet {
        println!("Scanning database for issues...");
    }
    
    let all_pairs = db.list_all()?;
    let mut missing_files = Vec::new();
    let mut untagged_files = Vec::new();
    
    for pair in all_pairs {
        if !pair.file.exists() {
            missing_files.push(pair.file);
        } else if pair.tags.is_empty() {
            untagged_files.push(pair.file);
        }
    }
    
    let total_issues = missing_files.len() + untagged_files.len();
    
    if total_issues == 0 {
        if !quiet {
            println!("No issues found. Database is clean.");
        }
        return Ok(());
    }
    
    let mut deleted_count = 0;
    let mut skipped_count = 0;
    
    if !missing_files.is_empty() {
        if !quiet {
            println!("\n=== Missing Files ===");
            println!("Found {} missing file(s):", missing_files.len());
            for file in &missing_files {
                println!("  - {}", output::format_path(file, path_format));
            }
            println!();
        }
        
        let (deleted, skipped) = process_cleanup_files(
            db,
            &missing_files,
            "File not found",
            path_format,
            quiet,
        )?;
        deleted_count += deleted;
        skipped_count += skipped;
    }
    
    if !untagged_files.is_empty() {
        if !quiet {
            println!("\n=== Files with No Tags ===");
            println!("Found {} file(s) with no tags:", untagged_files.len());
            for file in &untagged_files {
                println!("  - {}", output::format_path(file, path_format));
            }
            println!();
        }
        
        let (deleted, skipped) = process_cleanup_files(
            db,
            &untagged_files,
            "File has no tags",
            path_format,
            quiet,
        )?;
        deleted_count += deleted;
        skipped_count += skipped;
    }
    
    if !quiet {
        println!("\n=== Cleanup Summary ===");
        println!("Total issues found: {total_issues}");
        println!("  Missing files: {}", missing_files.len());
        println!("  Files with no tags: {}", untagged_files.len());
        println!("Deleted: {deleted_count}");
        println!("Skipped: {skipped_count}");
    }
    
    Ok(())
}

/// Process a list of files for cleanup, prompting for each file
fn process_cleanup_files(
    db: &Database,
    files: &[PathBuf],
    description: &str,
    path_format: config::PathFormat,
    quiet: bool,
) -> Result<(usize, usize)> {
    let mut deleted_count = 0;
    let mut skipped_count = 0;
    let mut delete_all = quiet;
    let mut skip_all = false;
    
    for file in files {
        if delete_all {
            db.remove(file)?;
            deleted_count += 1;
            if !quiet {
                println!("Deleted: {}", output::format_path(file, path_format));
            }
            continue;
        }
        
        if skip_all {
            skipped_count += 1;
            continue;
        }
        
        if !quiet {
            println!("\n{description}: {}", output::format_path(file, path_format));
            
            let options = vec![
                "Delete this file",
                "Delete all remaining",
                "Skip this file",
                "Skip all remaining",
            ];
            
            let selection = Select::new()
                .with_prompt("Action")
                .items(&options)
                .default(0)
                .interact()
                .map_err(|e| TagrError::InvalidInput(format!("Selection failed: {e}")))?;
            
            match selection {
                0 => {
                    db.remove(file)?;
                    deleted_count += 1;
                    println!("✓ Deleted: {}", output::format_path(file, path_format));
                }
                1 => {
                    delete_all = true;
                    db.remove(file)?;
                    deleted_count += 1;
                    println!("✓ Deleted: {}", output::format_path(file, path_format));
                }
                2 => {
                    skipped_count += 1;
                    println!("⊘ Skipped: {}", output::format_path(file, path_format));
                }
                3 => {
                    skip_all = true;
                    skipped_count += 1;
                    println!("⊘ Skipped: {}", output::format_path(file, path_format));
                }
                _ => unreachable!(),
            }
        }
    }
    
    Ok((deleted_count, skipped_count))
}
