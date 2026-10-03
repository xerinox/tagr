//! Cleanup command - remove missing files and files with no tags

use crate::store::TagStore;
use crate::types::TagrPath;
use crate::{TagrError, config, output};
use dialoguer::Select;
use std::io::Write;

type Result<T> = std::result::Result<T, TagrError>;

/// Execute the cleanup command
///
/// # Errors
/// Returns an error if database operations fail or if user interaction fails
#[allow(clippy::too_many_lines)]
pub fn execute(
    store: &dyn TagStore,
    path_format: config::PathFormat,
    quiet: bool,
    writer: &mut impl Write,
) -> Result<()> {
    if !quiet {
        writeln!(writer, "Scanning database for issues...")?;
    }

    let all_pairs = store.list_all()?;
    let mut missing_files = Vec::new();
    let mut untagged_no_notes = Vec::new();
    let mut notes_only_files = Vec::new();

    for pair in all_pairs {
        if !pair.file.as_path().exists() {
            missing_files.push(pair.file);
        } else if pair.tags.is_empty() {
            let has_note = store.get_note(&pair.file)?.is_some();
            if has_note {
                notes_only_files.push(pair.file);
            } else {
                untagged_no_notes.push(pair.file);
            }
        }
    }

    let total_issues = missing_files.len() + untagged_no_notes.len();

    if total_issues == 0 && notes_only_files.is_empty() {
        if !quiet {
            writeln!(writer, "No issues found. Database is clean.")?;
        }
        return Ok(());
    }

    let mut deleted_count = 0;
    let mut skipped_count = 0;

    if !missing_files.is_empty() {
        if !quiet {
            writeln!(writer, "\n=== Missing Files ===")?;
            writeln!(writer, "Found {} missing file(s):", missing_files.len())?;
            for file in &missing_files {
                writeln!(writer, "  - {}", output::format_path(file, path_format))?;
            }
            writeln!(writer)?;
        }

        let (deleted, skipped) = process_cleanup_files(
            store,
            &missing_files,
            "File not found",
            path_format,
            quiet,
            writer,
        )?;
        deleted_count += deleted;
        skipped_count += skipped;
    }

    if !untagged_no_notes.is_empty() {
        if !quiet {
            writeln!(writer, "\n=== Files with No Tags or Notes ===")?;
            writeln!(
                writer,
                "Found {} orphaned file(s):",
                untagged_no_notes.len()
            )?;
            for file in &untagged_no_notes {
                writeln!(writer, "  - {}", output::format_path(file, path_format))?;
            }
            writeln!(writer)?;
        }

        let (deleted, skipped) = process_cleanup_files(
            store,
            &untagged_no_notes,
            "File has no tags or notes",
            path_format,
            quiet,
            writer,
        )?;
        deleted_count += deleted;
        skipped_count += skipped;
    }

    if !quiet {
        writeln!(writer, "\n=== Cleanup Summary ===")?;
        writeln!(writer, "Total issues found: {total_issues}")?;
        writeln!(writer, "  Missing files: {}", missing_files.len())?;
        writeln!(
            writer,
            "  Files with no tags or notes: {}",
            untagged_no_notes.len()
        )?;

        if !notes_only_files.is_empty() {
            writeln!(
                writer,
                "\n ℹ Known files (notes only, no tags): {}",
                notes_only_files.len()
            )?;
            for file in &notes_only_files {
                writeln!(writer, "  - {}", output::format_path(file, path_format))?;
            }
        }

        writeln!(writer, "\nDeleted: {deleted_count}")?;
        writeln!(writer, "Skipped: {skipped_count}")?;
    }

    // Clean up orphaned notes from deleted missing files
    let mut orphaned_notes = 0;
    for file in &missing_files {
        if store.delete_note(file)? {
            orphaned_notes += 1;
        }
    }
    if !quiet && orphaned_notes > 0 {
        writeln!(
            writer,
            "Cleaned up {orphaned_notes} orphaned note(s) from deleted files"
        )?;
    }

    Ok(())
}

/// Process a list of files for cleanup, prompting for each file
fn process_cleanup_files(
    store: &dyn TagStore,
    files: &[TagrPath],
    description: &str,
    path_format: config::PathFormat,
    quiet: bool,
    writer: &mut impl Write,
) -> Result<(usize, usize)> {
    let mut deleted_count = 0;
    let mut skipped_count = 0;
    let mut delete_all = quiet;
    let mut skip_all = false;

    for file in files {
        if delete_all {
            store.remove_file(file)?;
            deleted_count += 1;
            if !quiet {
                writeln!(
                    writer,
                    "Deleted: {}",
                    output::format_path(file, path_format)
                )?;
            }
            continue;
        }

        if skip_all {
            skipped_count += 1;
            continue;
        }

        if !quiet {
            writeln!(
                writer,
                "\n{description}: {}",
                output::format_path(file, path_format)
            )?;

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
                    store.remove_file(file)?;
                    deleted_count += 1;
                    writeln!(
                        writer,
                        "✓ Deleted: {}",
                        output::format_path(file, path_format)
                    )?;
                }
                1 => {
                    delete_all = true;
                    store.remove_file(file)?;
                    deleted_count += 1;
                    writeln!(
                        writer,
                        "✓ Deleted: {}",
                        output::format_path(file, path_format)
                    )?;
                }
                2 => {
                    skipped_count += 1;
                    writeln!(
                        writer,
                        "⊘ Skipped: {}",
                        output::format_path(file, path_format)
                    )?;
                }
                3 => {
                    skip_all = true;
                    skipped_count += 1;
                    writeln!(
                        writer,
                        "⊘ Skipped: {}",
                        output::format_path(file, path_format)
                    )?;
                }
                _ => unreachable!(),
            }
        }
    }

    Ok((deleted_count, skipped_count))
}
