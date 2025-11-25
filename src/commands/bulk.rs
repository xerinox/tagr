//! Bulk tag operations for multiple files
//!
//! This module implements bulk operations on files matching patterns:
//! - `bulk_tag`: Add tags to multiple files
//! - `bulk_untag`: Remove tags from multiple files
//! - `rename_tag`: Rename a tag globally across all files
//!
//! All operations support dry-run mode and confirmation prompts for safety.

use crate::cli::SearchParams;
use crate::db::Database;
use crate::{Pair, TagrError};
use colored::Colorize;
use dialoguer::Confirm;
use std::collections::HashSet;
use std::path::PathBuf;

type Result<T> = std::result::Result<T, TagrError>;

/// Action type for bulk operations (used in preview and confirmation)
#[derive(Debug, Clone, Copy)]
enum BulkAction {
    /// Add tags to files
    Add,
    /// Remove specific tags from files
    Remove,
    /// Remove all tags from files
    RemoveAll,
}

impl BulkAction {
    /// Get the verb for this action (for display)
    const fn verb(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Remove => "remove",
            Self::RemoveAll => "remove all tags from",
        }
    }

    /// Get the preposition for this action (for display)
    const fn preposition(self) -> &'static str {
        match self {
            Self::Add => "to",
            Self::Remove => "from",
            Self::RemoveAll => "",
        }
    }

    /// Get the action name for confirmation prompt
    const fn prompt_name(self) -> &'static str {
        match self {
            Self::Add => "tag",
            Self::Remove => "untag",
            Self::RemoveAll => "remove ALL tags from",
        }
    }
}

/// Summary of bulk operation results
#[derive(Debug, Default)]
pub struct BulkOpSummary {
    /// Number of files successfully processed
    pub success: usize,
    /// Number of files skipped (already had tags, etc.)
    pub skipped: usize,
    /// Number of files that encountered errors
    pub errors: usize,
    /// Error messages
    pub error_messages: Vec<String>,
}

impl BulkOpSummary {
    fn new() -> Self {
        Self::default()
    }

    #[inline]
    const fn add_success(&mut self) {
        self.success += 1;
    }

    #[inline]
    const fn add_skip(&mut self) {
        self.skipped += 1;
    }

    fn add_error(&mut self, msg: String) {
        self.errors += 1;
        self.error_messages.push(msg);
    }

    fn print(&self, operation: &str) {
        println!("\n{}", format!("=== {operation} Summary ===").bold());
        println!("  {} {}", "✓ Success:".green(), self.success);
        if self.skipped > 0 {
            println!("  {} {}", "⊘ Skipped:".yellow(), self.skipped);
        }
        if self.errors > 0 {
            println!("  {} {}", "✗ Errors:".red(), self.errors);
            if !self.error_messages.is_empty() {
                println!("\n{}", "Error details:".red().bold());
                for msg in &self.error_messages {
                    println!("  - {msg}");
                }
            }
        }
    }
}

/// Add tags to multiple files matching search criteria
///
/// # Arguments
/// * `db` - Database instance
/// * `params` - Search parameters to filter files
/// * `tags` - Tags to add to matching files
/// * `dry_run` - If true, preview changes without applying
/// * `yes` - Skip confirmation prompts
/// * `quiet` - Minimal output
///
/// # Errors
/// Returns error if database operations fail or no files match
pub fn bulk_tag(
    db: &Database,
    params: &SearchParams,
    tags: &[String],
    dry_run: bool,
    yes: bool,
    quiet: bool,
) -> Result<()> {
    if tags.is_empty() {
        return Err(TagrError::InvalidInput("No tags provided".into()));
    }

    let files = crate::db::query::apply_search_params(db, params)?;

    if files.is_empty() {
        if !quiet {
            println!("No files match the specified criteria.");
        }
        return Ok(());
    }

    if dry_run {
        print_dry_run_preview(&files, tags, BulkAction::Add);
        return Ok(());
    }

    // Confirmation prompt
    if !yes && !confirm_bulk_operation(&files, tags, BulkAction::Add)? {
        println!("Operation cancelled.");
        return Ok(());
    }

    let mut summary = BulkOpSummary::new();

    for file in &files {
        match db.add_tags(file, tags.to_vec()) {
            Ok(()) => {
                summary.add_success();
                if !quiet {
                    println!("✓ Tagged: {}", file.display());
                }
            }
            Err(e) => {
                summary.add_error(format!("{}: {}", file.display(), e));
                if !quiet {
                    eprintln!("✗ Failed to tag {}: {}", file.display(), e);
                }
            }
        }
    }

    if !quiet {
        summary.print("Bulk Tag");
    }

    Ok(())
}

/// Remove tags from multiple files matching search criteria
///
/// # Arguments
/// * `db` - Database instance
/// * `params` - Search parameters to filter files
/// * `tags` - Tags to remove (empty to remove all tags)
/// * `remove_all` - If true, remove all tags from matching files
/// * `dry_run` - If true, preview changes without applying
/// * `yes` - Skip confirmation prompts
/// * `quiet` - Minimal output
///
/// # Errors
/// Returns error if database operations fail or no files match
#[allow(clippy::too_many_arguments)]
#[allow(clippy::fn_params_excessive_bools)]
pub fn bulk_untag(
    db: &Database,
    params: &SearchParams,
    tags: &[String],
    remove_all: bool,
    dry_run: bool,
    yes: bool,
    quiet: bool,
) -> Result<()> {
    if !remove_all && tags.is_empty() {
        return Err(TagrError::InvalidInput(
            "No tags provided. Use --all to remove all tags".into(),
        ));
    }

    let files = crate::db::query::apply_search_params(db, params)?;

    if files.is_empty() {
        if !quiet {
            println!("No files match the specified criteria.");
        }
        return Ok(());
    }

    if dry_run {
        if remove_all {
            print_dry_run_preview(&files, &[], BulkAction::RemoveAll);
        } else {
            print_dry_run_preview(&files, tags, BulkAction::Remove);
        }
        return Ok(());
    }

    // Confirmation prompt
    let action = if remove_all {
        BulkAction::RemoveAll
    } else {
        BulkAction::Remove
    };
    if !yes && !confirm_bulk_operation(&files, tags, action)? {
        println!("Operation cancelled.");
        return Ok(());
    }

    let mut summary = BulkOpSummary::new();

    for file in &files {
        let result = if remove_all {
            db.remove(file).map(|_| ())
        } else {
            db.remove_tags(file, tags)
        };

        match result {
            Ok(()) => {
                summary.add_success();
                if !quiet {
                    println!("✓ Untagged: {}", file.display());
                }
            }
            Err(e) => {
                summary.add_error(format!("{}: {}", file.display(), e));
                if !quiet {
                    eprintln!("✗ Failed to untag {}: {}", file.display(), e);
                }
            }
        }
    }

    if !quiet {
        summary.print("Bulk Untag");
    }

    Ok(())
}

/// Rename a tag globally across all files in the database
///
/// # Arguments
/// * `db` - Database instance
/// * `old_tag` - Current tag name
/// * `new_tag` - New tag name
/// * `dry_run` - If true, preview changes without applying
/// * `yes` - Skip confirmation prompts
/// * `quiet` - Minimal output
///
/// # Errors
/// Returns error if database operations fail or tag doesn't exist
pub fn rename_tag(
    db: &Database,
    old_tag: &str,
    new_tag: &str,
    dry_run: bool,
    yes: bool,
    quiet: bool,
) -> Result<()> {
    if old_tag == new_tag {
        return Err(TagrError::InvalidInput(
            "Old and new tag names are identical".into(),
        ));
    }

    let files = db.find_by_tag(old_tag)?;

    if files.is_empty() {
        if !quiet {
            println!("Tag '{old_tag}' not found in database.");
        }
        return Ok(());
    }

    if dry_run {
        println!("{}", "=== Dry Run Mode ===".yellow().bold());
        println!(
            "Would rename tag '{}' → '{}' in {} file(s)",
            old_tag.cyan(),
            new_tag.green(),
            files.len()
        );
        println!("\n{}", "Affected files:".bold());
        for (i, file) in files.iter().enumerate().take(10) {
            println!("  {}. {}", i + 1, file.display());
        }
        if files.len() > 10 {
            println!("  ... and {} more", files.len() - 10);
        }
        println!("\n{}", "Run without --dry-run to apply changes.".yellow());
        return Ok(());
    }

    // Confirmation prompt
    if !yes {
        let prompt = format!(
            "Rename tag '{}' to '{}' in {} file(s)?",
            old_tag.cyan(),
            new_tag.green(),
            files.len()
        );
        let confirmed = Confirm::new()
            .with_prompt(prompt)
            .interact()
            .map_err(|e| TagrError::InvalidInput(format!("Failed to get confirmation: {e}")))?;
        if !confirmed {
            println!("Operation cancelled.");
            return Ok(());
        }
    }

    let mut summary = BulkOpSummary::new();

    for file in &files {
        // Get current tags
        let Some(current_tags) = db.get_tags(file)? else {
            summary.add_skip();
            continue;
        };

        let new_tags: Vec<String> = current_tags
            .into_iter()
            .map(|t| if t == old_tag { new_tag.to_string() } else { t })
            .collect::<HashSet<_>>() // Remove duplicates if new_tag already exists
            .into_iter()
            .collect();

        let pair = Pair {
            file: file.clone(),
            tags: new_tags,
        };

        match db.insert_pair(&pair) {
            Ok(()) => {
                summary.add_success();
                if !quiet {
                    println!("✓ Renamed in: {}", file.display());
                }
            }
            Err(e) => {
                summary.add_error(format!("{}: {}", file.display(), e));
                if !quiet {
                    eprintln!("✗ Failed to rename in {}: {}", file.display(), e);
                }
            }
        }
    }

    if !quiet {
        println!(
            "\n{} Renamed '{}' → '{}' in {} file(s)",
            "✓".green(),
            old_tag,
            new_tag,
            summary.success
        );
        if summary.errors > 0 {
            summary.print("Rename Tag");
        }
    }

    Ok(())
}

/// Print dry-run preview of bulk operation
fn print_dry_run_preview(files: &[PathBuf], tags: &[String], action: BulkAction) {
    println!("{}", "=== Dry Run Mode ===".yellow().bold());
    println!(
        "Would {} tags {} {} {} file(s)",
        action.verb(),
        if tags.is_empty() {
            String::new()
        } else {
            format!("[{}]", tags.join(", ").cyan())
        },
        action.preposition(),
        files.len()
    );

    println!("\n{}", "Affected files:".bold());
    for (i, file) in files.iter().enumerate().take(10) {
        println!("  {}. {}", i + 1, file.display());
    }
    if files.len() > 10 {
        println!("  ... and {} more", files.len() - 10);
    }
    println!("\n{}", "Run without --dry-run to apply changes.".yellow());
}

/// Show confirmation prompt for bulk operation
fn confirm_bulk_operation(files: &[PathBuf], tags: &[String], action: BulkAction) -> Result<bool> {
    let prompt = if tags.is_empty() {
        format!("{} {} file(s)?", action.prompt_name().to_uppercase(), files.len())
    } else {
        format!(
            "{} {} file(s) with tags [{}]?",
            action.prompt_name().to_uppercase(),
            files.len(),
            tags.join(", ")
        )
    };

    Confirm::new()
        .with_prompt(prompt)
        .interact()
        .map_err(|e| TagrError::InvalidInput(format!("Failed to get confirmation: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::SearchMode;
    use crate::testing::{TempFile, TestDb};

    #[test]
    fn test_bulk_tag_basic() {
        let test_db = TestDb::new("test_bulk_tag");
        let db = test_db.db();
        db.clear().unwrap();

        // Create test files
        let file1 = TempFile::create("file1.txt").unwrap();
        let file2 = TempFile::create("file2.txt").unwrap();

        // Add to database with initial tags
        db.add_tags(file1.path(), vec!["initial".into()]).unwrap();
        db.add_tags(file2.path(), vec!["initial".into()]).unwrap();

        // Bulk tag with search params
        let params = SearchParams {
            query: None,
            tags: vec!["initial".to_string()],
            tag_mode: SearchMode::Any,
            file_patterns: vec![],
            file_mode: SearchMode::All,
            exclude_tags: vec![],
            regex_tag: false,
            regex_file: false,
            virtual_tags: vec![],
            virtual_mode: SearchMode::All,
        };

        bulk_tag(
            db,
            &params,
            &["bulk".to_string(), "added".to_string()],
            false,
            true, // Skip confirmation
            true, // Quiet mode
        )
        .unwrap();

        // Verify tags were added
        let tags1 = db.get_tags(file1.path()).unwrap().unwrap();
        assert!(tags1.contains(&"initial".to_string()));
        assert!(tags1.contains(&"bulk".to_string()));
        assert!(tags1.contains(&"added".to_string()));

        let tags2 = db.get_tags(file2.path()).unwrap().unwrap();
        assert!(tags2.contains(&"initial".to_string()));
        assert!(tags2.contains(&"bulk".to_string()));
    }

    #[test]
    fn test_bulk_untag_specific_tags() {
        let test_db = TestDb::new("test_bulk_untag");
        let db = test_db.db();
        db.clear().unwrap();

        let file1 = TempFile::create("file1.txt").unwrap();
        let file2 = TempFile::create("file2.txt").unwrap();

        db.add_tags(file1.path(), vec!["tag1".into(), "tag2".into(), "keep".into()])
            .unwrap();
        db.add_tags(file2.path(), vec!["tag1".into(), "tag2".into(), "keep".into()])
            .unwrap();

        let params = SearchParams {
            query: None,
            tags: vec!["tag1".to_string()],
            tag_mode: SearchMode::Any,
            file_patterns: vec![],
            file_mode: SearchMode::All,
            exclude_tags: vec![],
            regex_tag: false,
            regex_file: false,
            virtual_tags: vec![],
            virtual_mode: SearchMode::All,
        };

        bulk_untag(
            db,
            &params,
            &["tag1".to_string(), "tag2".to_string()],
            false,
            false,
            true,
            true,
        )
        .unwrap();

        // Verify specific tags were removed
        let tags1 = db.get_tags(file1.path()).unwrap().unwrap();
        assert!(!tags1.contains(&"tag1".to_string()));
        assert!(!tags1.contains(&"tag2".to_string()));
        assert!(tags1.contains(&"keep".to_string()));
    }

    #[test]
    fn test_rename_tag_basic() {
        let test_db = TestDb::new("test_rename_tag");
        let db = test_db.db();
        db.clear().unwrap();

        let file1 = TempFile::create("file1.txt").unwrap();
        let file2 = TempFile::create("file2.txt").unwrap();

        db.add_tags(file1.path(), vec!["oldname".into(), "other".into()])
            .unwrap();
        db.add_tags(file2.path(), vec!["oldname".into()]).unwrap();

        rename_tag(db, "oldname", "newname", false, true, true).unwrap();

        // Verify tag was renamed
        let tags1 = db.get_tags(file1.path()).unwrap().unwrap();
        assert!(!tags1.contains(&"oldname".to_string()));
        assert!(tags1.contains(&"newname".to_string()));
        assert!(tags1.contains(&"other".to_string()));

        let tags2 = db.get_tags(file2.path()).unwrap().unwrap();
        assert!(!tags2.contains(&"oldname".to_string()));
        assert!(tags2.contains(&"newname".to_string()));
    }

    #[test]
    fn test_bulk_tag_no_files() {
        let test_db = TestDb::new("test_bulk_no_files");
        let db = test_db.db();
        db.clear().unwrap();

        let params = SearchParams {
            query: None,
            tags: vec!["nonexistent".to_string()],
            tag_mode: SearchMode::Any,
            file_patterns: vec![],
            file_mode: SearchMode::All,
            exclude_tags: vec![],
            regex_tag: false,
            regex_file: false,
            virtual_tags: vec![],
            virtual_mode: SearchMode::All,
        };

        // Should succeed but do nothing
        bulk_tag(db, &params, &["test".to_string()], false, true, true).unwrap();
    }
}
