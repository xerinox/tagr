//! Cleanup command - remove missing files and files with no tags

use crate::{TagrError, config, db::Database, output};
use dialoguer::Select;
use std::path::PathBuf;

type Result<T> = std::result::Result<T, TagrError>;

/// Execute the cleanup command
///
/// # Errors
/// Returns an error if database operations fail or if user interaction fails
pub fn execute(db: &Database, path_format: config::PathFormat, quiet: bool) -> Result<()> {
    if !quiet {
        println!("Scanning database for issues...");
    }

    let all_pairs = db.list_all()?;
    let mut missing_files = Vec::new();
    let mut untagged_no_notes = Vec::new();
    let mut notes_only_files = Vec::new();

    for pair in all_pairs {
        if !pair.file.exists() {
            missing_files.push(pair.file);
        } else if pair.tags.is_empty() {
            // File has no tags - check if it has a note
            let has_note = db.get_note(&pair.file)?.is_some();
            if has_note {
                notes_only_files.push(pair.file);
            } else {
                // No tags and no note - this shouldn't happen with equality model
                // but handle it gracefully
                untagged_no_notes.push(pair.file);
            }
        }
    }

    let total_issues = missing_files.len() + untagged_no_notes.len();

    if total_issues == 0 && notes_only_files.is_empty() {
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

        let (deleted, skipped) =
            process_cleanup_files(db, &missing_files, "File not found", path_format, quiet)?;
        deleted_count += deleted;
        skipped_count += skipped;
    }

    if !untagged_no_notes.is_empty() {
        if !quiet {
            println!("\n=== Files with No Tags or Notes ===");
            println!("Found {} orphaned file(s):", untagged_no_notes.len());
            for file in &untagged_no_notes {
                println!("  - {}", output::format_path(file, path_format));
            }
            println!();
        }

        let (deleted, skipped) = process_cleanup_files(
            db,
            &untagged_no_notes,
            "File has no tags or notes",
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
        println!("  Files with no tags or notes: {}", untagged_no_notes.len());
        
        if !notes_only_files.is_empty() {
            println!("\n ℹ Known files (notes only, no tags): {}", notes_only_files.len());
            for file in &notes_only_files {
                println!("  - {}", output::format_path(file, path_format));
            }
        }
        
        println!("\nDeleted: {deleted_count}");
        println!("Skipped: {skipped_count}");
    }

    // Clean up orphaned notes from deleted missing files
    let mut orphaned_notes = 0;
    for file in &missing_files {
        if db.delete_note(file)? {
            orphaned_notes += 1;
        }
    }
    if !quiet && orphaned_notes > 0 {
        println!("Cleaned up {orphaned_notes} orphaned note(s) from deleted files");
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
            println!(
                "\n{description}: {}",
                output::format_path(file, path_format)
            );

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
