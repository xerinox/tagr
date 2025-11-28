//! Bulk tag operations for multiple files
//!
//! This module implements bulk operations on files matching patterns:
//! - `bulk_tag`: Add tags to multiple files
//! - `bulk_untag`: Remove tags from multiple files
//! - `rename_tag`: Rename a tag globally across all files
//! - `merge_tags`: Merge multiple tags into a single tag
//! - `copy_tags`: Copy tags from a source file to multiple target files
//!
//! All operations support dry-run mode and confirmation prompts for safety.

use crate::cli::{ConditionalArgs, SearchParams};
use crate::db::Database;
use crate::{Pair, TagrError};
use colored::Colorize;
use dialoguer::Confirm;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, TagrError>;

/// Reason a file was skipped during bulk operation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SkipReason {
    /// File already had the tag(s) (condition check)
    AlreadyExists,
    /// File didn't meet conditional requirements
    ConditionNotMet,
    /// Other reason (e.g., database error handled gracefully)
    Other,
}

/// Check if a file meets the conditional requirements
///
/// # Arguments
/// * `file` - File path to check
/// * `db` - Database instance
/// * `conditions` - Conditional flags to evaluate
/// * `tags_to_add` - Tags that will be added (for if-not-exists check)
///
/// # Returns
/// `Ok(true)` if conditions are met, `Ok(false)` if not, `Err` on database error
fn check_conditions(
    file: &Path,
    db: &Database,
    conditions: &ConditionalArgs,
    tags_to_add: &[String],
) -> Result<bool> {
    let file_tags = db.get_tags(file)?.unwrap_or_default();
    
    // Check --if-not-exists: only add tags if they don't already exist
    if conditions.if_not_exists {
        let has_any = tags_to_add.iter().any(|tag| file_tags.contains(tag));
        if has_any {
            return Ok(false);
        }
    }
    
    // Check --if-has-tag: only process if file has ALL specified tags
    if !conditions.if_has_tag.is_empty() {
        let has_all = conditions.if_has_tag.iter().all(|tag| file_tags.contains(tag));
        if !has_all {
            return Ok(false);
        }
    }
    
    // Check --if-missing-tag: only process if file is missing ANY specified tags
    if !conditions.if_missing_tag.is_empty() {
        let missing_any = conditions.if_missing_tag.iter().any(|tag| !file_tags.contains(tag));
        if !missing_any {
            return Ok(false);
        }
    }
    
    Ok(true)
}

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
    /// Number of files skipped due to conditional checks
    pub skipped_condition: usize,
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

    #[inline]
    const fn add_skip_condition(&mut self) {
        self.skipped_condition += 1;
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
        if self.skipped_condition > 0 {
            println!("  {} {}", "⊘ Skipped (condition):".yellow(), self.skipped_condition);
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
/// * `conditions` - Conditional flags for selective processing
/// * `dry_run` - If true, preview changes without applying
/// * `yes` - Skip confirmation prompts
/// * `quiet` - Minimal output
///
/// # Errors
/// Returns error if database operations fail or no files match
#[allow(clippy::too_many_arguments)]
pub fn bulk_tag(
    db: &Database,
    params: &SearchParams,
    tags: &[String],
    conditions: &ConditionalArgs,
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
        // Check conditions
        match check_conditions(file, db, conditions, tags) {
            Ok(true) => {
                // Conditions met, proceed with tagging
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
            Ok(false) => {
                // Conditions not met, skip
                summary.add_skip_condition();
                if !quiet {
                    println!("⊘ Skipped (condition): {}", file.display());
                }
            }
            Err(e) => {
                summary.add_error(format!("{}: {}", file.display(), e));
                if !quiet {
                    eprintln!("✗ Failed to check conditions for {}: {}", file.display(), e);
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
/// * `conditions` - Conditional flags for selective processing
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
    conditions: &ConditionalArgs,
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
        // Check conditions
        match check_conditions(file, db, conditions, tags) {
            Ok(true) => {
                // Conditions met, proceed with untagging
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
            Ok(false) => {
                // Conditions not met, skip
                summary.add_skip_condition();
                if !quiet {
                    println!("⊘ Skipped (condition): {}", file.display());
                }
            }
            Err(e) => {
                summary.add_error(format!("{}: {}", file.display(), e));
                if !quiet {
                    eprintln!("✗ Failed to check conditions for {}: {}", file.display(), e);
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

/// Copy tags from a source file to multiple target files
///
/// # Arguments
/// * `db` - Database instance
/// * `source_file` - Source file to copy tags from
/// * `params` - Search parameters to filter target files
/// * `specific_tags` - Only copy these specific tags (None = copy all)
/// * `exclude_tags` - Exclude these tags from copying
/// * `dry_run` - If true, preview changes without applying
/// * `yes` - Skip confirmation prompts
/// * `quiet` - Minimal output
///
/// # Errors
/// Returns error if source file not found, database operations fail, or no target files match
#[allow(clippy::too_many_arguments)]
pub fn copy_tags(
    db: &Database,
    source_file: &Path,
    params: &SearchParams,
    specific_tags: Option<&[String]>,
    exclude_tags: &[String],
    dry_run: bool,
    yes: bool,
    quiet: bool,
) -> Result<()> {
    // Get tags from source file
    let source_tags = db.get_tags(source_file)?.ok_or_else(|| {
        TagrError::InvalidInput(format!(
            "Source file not in database: {}",
            source_file.display()
        ))
    })?;

    // Filter tags based on specific_tags and exclude_tags
    let tags_to_copy: Vec<String> = source_tags
        .into_iter()
        .filter(|tag| {
            // If specific_tags is provided, only include those
            if let Some(specific) = specific_tags
                && !specific.contains(tag)
            {
                return false;
            }
            // Exclude any tags in exclude_tags
            !exclude_tags.contains(tag)
        })
        .collect();

    if tags_to_copy.is_empty() {
        if !quiet {
            println!("No tags to copy after filtering.");
        }
        return Ok(());
    }

    // Find target files via search params
    let target_files = crate::db::query::apply_search_params(db, params)?;

    if target_files.is_empty() {
        if !quiet {
            println!("No target files match the specified criteria.");
        }
        return Ok(());
    }

    // Remove source file from target list if present
    let target_files: Vec<PathBuf> = target_files
        .into_iter()
        .filter(|f| f != source_file)
        .collect();

    if target_files.is_empty() {
        if !quiet {
            println!("No target files to copy tags to (excluding source file).");
        }
        return Ok(());
    }

    if dry_run {
        println!("{}", "=== Dry Run Mode ===".yellow().bold());
        println!(
            "Would copy tags [{}] from '{}' to {} file(s)",
            tags_to_copy.join(", ").cyan(),
            source_file.display(),
            target_files.len()
        );
        println!("\n{}", "Target files:".bold());
        for (i, file) in target_files.iter().enumerate().take(10) {
            println!("  {}. {}", i + 1, file.display());
        }
        if target_files.len() > 10 {
            println!("  ... and {} more", target_files.len() - 10);
        }
        println!("\n{}", "Run without --dry-run to apply changes.".yellow());
        return Ok(());
    }

    // Confirmation prompt
    if !yes {
        let prompt = format!(
            "Copy tags [{}] from '{}' to {} file(s)?",
            tags_to_copy.join(", ").cyan(),
            source_file.display(),
            target_files.len()
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

    for file in &target_files {
        match db.add_tags(file, tags_to_copy.clone()) {
            Ok(()) => {
                summary.add_success();
                if !quiet {
                    println!("✓ Copied tags to: {}", file.display());
                }
            }
            Err(e) => {
                summary.add_error(format!("{}: {}", file.display(), e));
                if !quiet {
                    eprintln!("✗ Failed to copy tags to {}: {}", file.display(), e);
                }
            }
        }
    }

    if !quiet {
        summary.print("Copy Tags");
    }

    Ok(())
}

/// Merge multiple tags into a single tag across all files
///
/// Finds all files with any of the source tags and replaces them with the target tag.
/// Automatically deduplicates if target tag already exists.
///
/// # Arguments
/// * `db` - Database instance
/// * `source_tags` - Tags to be merged (will be removed)
/// * `target_tag` - Tag to merge into (will be added)
/// * `dry_run` - If true, preview changes without applying
/// * `yes` - Skip confirmation prompts
/// * `quiet` - Minimal output
///
/// # Errors
/// Returns error if database operations fail or no files have the source tags
pub fn merge_tags(
    db: &Database,
    source_tags: &[String],
    target_tag: &str,
    dry_run: bool,
    yes: bool,
    quiet: bool,
) -> Result<()> {
    if source_tags.is_empty() {
        return Err(TagrError::InvalidInput("No source tags provided".into()));
    }

    if source_tags.contains(&target_tag.to_string()) {
        return Err(TagrError::InvalidInput(
            "Target tag cannot be one of the source tags".into(),
        ));
    }

    // Collect all files that have any of the source tags
    let mut files_set = HashSet::new();
    for tag in source_tags {
        let tag_files = db.find_by_tag(tag)?;
        files_set.extend(tag_files);
    }

    let files: Vec<PathBuf> = files_set.into_iter().collect();

    if files.is_empty() {
        if !quiet {
            println!(
                "No files found with source tags: [{}]",
                source_tags.join(", ")
            );
        }
        return Ok(());
    }

    if dry_run {
        println!("{}", "=== Dry Run Mode ===".yellow().bold());
        println!(
            "Would merge tags [{}] → '{}' in {} file(s)",
            source_tags.join(", ").cyan(),
            target_tag.green(),
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
            "Merge tags [{}] into '{}' in {} file(s)?",
            source_tags.join(", ").cyan(),
            target_tag.green(),
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

        // Replace source tags with target tag
        let new_tags: Vec<String> = current_tags
            .into_iter()
            .map(|t| {
                if source_tags.contains(&t) {
                    target_tag.to_string()
                } else {
                    t
                }
            })
            .collect::<HashSet<_>>() // Remove duplicates
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
                    println!("✓ Merged in: {}", file.display());
                }
            }
            Err(e) => {
                summary.add_error(format!("{}: {}", file.display(), e));
                if !quiet {
                    eprintln!("✗ Failed to merge in {}: {}", file.display(), e);
                }
            }
        }
    }

    if !quiet {
        println!(
            "\n{} Merged [{}] → '{}' in {} file(s)",
            "✓".green(),
            source_tags.join(", "),
            target_tag,
            summary.success
        );
        if summary.errors > 0 {
            summary.print("Merge Tags");
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
        format!(
            "{} {} file(s)?",
            action.prompt_name().to_uppercase(),
            files.len()
        )
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
            &ConditionalArgs::default(),
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

        db.add_tags(
            file1.path(),
            vec!["tag1".into(), "tag2".into(), "keep".into()],
        )
        .unwrap();
        db.add_tags(
            file2.path(),
            vec!["tag1".into(), "tag2".into(), "keep".into()],
        )
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
            &ConditionalArgs::default(),
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
        bulk_tag(db, &params, &["test".to_string()], &ConditionalArgs::default(), false, true, true).unwrap();
    }

    #[test]
    fn test_merge_tags_basic() {
        let test_db = TestDb::new("test_merge_tags");
        let db = test_db.db();
        db.clear().unwrap();

        let file1 = TempFile::create("file1.txt").unwrap();
        let file2 = TempFile::create("file2.txt").unwrap();
        let file3 = TempFile::create("file3.txt").unwrap();

        db.add_tags(file1.path(), vec!["javascript".into(), "frontend".into()])
            .unwrap();
        db.add_tags(file2.path(), vec!["js".into(), "frontend".into()])
            .unwrap();
        db.add_tags(file3.path(), vec!["JS".into(), "backend".into()])
            .unwrap();

        // Merge javascript, js, JS into js
        merge_tags(
            db,
            &["javascript".to_string(), "JS".to_string()],
            "js",
            false,
            true,
            true,
        )
        .unwrap();

        // Verify all source tags were replaced with target
        let tags1 = db.get_tags(file1.path()).unwrap().unwrap();
        assert!(!tags1.contains(&"javascript".to_string()));
        assert!(tags1.contains(&"js".to_string()));
        assert!(tags1.contains(&"frontend".to_string()));

        let tags2 = db.get_tags(file2.path()).unwrap().unwrap();
        assert!(tags2.contains(&"js".to_string()));
        assert!(tags2.contains(&"frontend".to_string()));

        let tags3 = db.get_tags(file3.path()).unwrap().unwrap();
        assert!(!tags3.contains(&"JS".to_string()));
        assert!(tags3.contains(&"js".to_string()));
        assert!(tags3.contains(&"backend".to_string()));
    }

    #[test]
    fn test_merge_tags_with_duplicates() {
        let test_db = TestDb::new("test_merge_dupes");
        let db = test_db.db();
        db.clear().unwrap();

        let file1 = TempFile::create("file1.txt").unwrap();

        // File already has target tag
        db.add_tags(file1.path(), vec!["old".into(), "new".into()])
            .unwrap();

        merge_tags(db, &["old".to_string()], "new", false, true, true).unwrap();

        // Should not have duplicate 'new' tags
        let tags = db.get_tags(file1.path()).unwrap().unwrap();
        assert_eq!(tags.len(), 1);
        assert!(tags.contains(&"new".to_string()));
    }

    #[test]
    fn test_copy_tags_all() {
        let test_db = TestDb::new("test_copy_tags_all");
        let db = test_db.db();
        db.clear().unwrap();

        // Create source file with tags
        let source = TempFile::create("source.txt").unwrap();
        db.add_tags(
            source.path(),
            vec!["tag1".into(), "tag2".into(), "tag3".into()],
        )
        .unwrap();

        // Create target files with initial tags
        let target1 = TempFile::create("target1.txt").unwrap();
        let target2 = TempFile::create("target2.txt").unwrap();
        db.add_tags(target1.path(), vec!["initial".into()]).unwrap();
        db.add_tags(target2.path(), vec!["initial".into()]).unwrap();

        // Search params to match target files
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

        // Copy all tags from source
        copy_tags(db, source.path(), &params, None, &[], false, true, true).unwrap();

        // Verify all tags were copied
        let tags1 = db.get_tags(target1.path()).unwrap().unwrap();
        assert!(tags1.contains(&"initial".to_string()));
        assert!(tags1.contains(&"tag1".to_string()));
        assert!(tags1.contains(&"tag2".to_string()));
        assert!(tags1.contains(&"tag3".to_string()));

        let tags2 = db.get_tags(target2.path()).unwrap().unwrap();
        assert!(tags2.contains(&"initial".to_string()));
        assert!(tags2.contains(&"tag1".to_string()));
    }

    #[test]
    fn test_copy_tags_specific() {
        let test_db = TestDb::new("test_copy_tags_specific");
        let db = test_db.db();
        db.clear().unwrap();

        let source = TempFile::create("source.txt").unwrap();
        db.add_tags(
            source.path(),
            vec!["tag1".into(), "tag2".into(), "tag3".into()],
        )
        .unwrap();

        let target = TempFile::create("target.txt").unwrap();
        db.add_tags(target.path(), vec!["initial".into()]).unwrap();

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

        // Copy only specific tags
        copy_tags(
            db,
            source.path(),
            &params,
            Some(&["tag1".to_string(), "tag2".to_string()]),
            &[],
            false,
            true,
            true,
        )
        .unwrap();

        // Verify only specified tags were copied
        let tags = db.get_tags(target.path()).unwrap().unwrap();
        assert!(tags.contains(&"initial".to_string()));
        assert!(tags.contains(&"tag1".to_string()));
        assert!(tags.contains(&"tag2".to_string()));
        assert!(!tags.contains(&"tag3".to_string()));
    }

    #[test]
    fn test_copy_tags_with_exclusions() {
        let test_db = TestDb::new("test_copy_tags_exclude");
        let db = test_db.db();
        db.clear().unwrap();

        let source = TempFile::create("source.txt").unwrap();
        db.add_tags(
            source.path(),
            vec!["tag1".into(), "tag2".into(), "tag3".into()],
        )
        .unwrap();

        let target = TempFile::create("target.txt").unwrap();
        db.add_tags(target.path(), vec!["initial".into()]).unwrap();

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

        // Copy all except excluded tags
        copy_tags(
            db,
            source.path(),
            &params,
            None,
            &["tag2".to_string()],
            false,
            true,
            true,
        )
        .unwrap();

        // Verify excluded tag was not copied
        let tags = db.get_tags(target.path()).unwrap().unwrap();
        assert!(tags.contains(&"initial".to_string()));
        assert!(tags.contains(&"tag1".to_string()));
        assert!(!tags.contains(&"tag2".to_string()));
        assert!(tags.contains(&"tag3".to_string()));
    }

    #[test]
    fn test_copy_tags_source_not_found() {
        let test_db = TestDb::new("test_copy_no_source");
        let db = test_db.db();
        db.clear().unwrap();

        let source = TempFile::create("source.txt").unwrap();
        // Don't add source to database

        let target = TempFile::create("target.txt").unwrap();
        db.add_tags(target.path(), vec!["initial".into()]).unwrap();

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

        // Should return error
        let result = copy_tags(db, source.path(), &params, None, &[], false, true, true);
        assert!(result.is_err());
    }

    #[test]
    fn test_copy_tags_no_targets() {
        let test_db = TestDb::new("test_copy_no_targets");
        let db = test_db.db();
        db.clear().unwrap();

        let source = TempFile::create("source.txt").unwrap();
        db.add_tags(source.path(), vec!["tag1".into()]).unwrap();

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

        // Should succeed but do nothing (no targets match)
        copy_tags(db, source.path(), &params, None, &[], false, true, true).unwrap();
    }

    #[test]
    fn test_copy_tags_excludes_source() {
        let test_db = TestDb::new("test_copy_exclude_source");
        let db = test_db.db();
        db.clear().unwrap();

        // Source file has matching tag
        let source = TempFile::create("source.txt").unwrap();
        db.add_tags(source.path(), vec!["shared".into(), "unique".into()])
            .unwrap();

        let target = TempFile::create("target.txt").unwrap();
        db.add_tags(target.path(), vec!["shared".into()]).unwrap();

        // Search for files with "shared" tag (includes both source and target)
        let params = SearchParams {
            query: None,
            tags: vec!["shared".to_string()],
            tag_mode: SearchMode::Any,
            file_patterns: vec![],
            file_mode: SearchMode::All,
            exclude_tags: vec![],
            regex_tag: false,
            regex_file: false,
            virtual_tags: vec![],
            virtual_mode: SearchMode::All,
        };

        copy_tags(db, source.path(), &params, None, &[], false, true, true).unwrap();

        // Source should not have copied to itself
        let source_tags = db.get_tags(source.path()).unwrap().unwrap();
        assert_eq!(source_tags.len(), 2); // Only original tags

        // Target should have received the unique tag
        let target_tags = db.get_tags(target.path()).unwrap().unwrap();
        assert!(target_tags.contains(&"shared".to_string()));
        assert!(target_tags.contains(&"unique".to_string()));
    }

    #[test]
    fn test_copy_tags_specific_and_exclude() {
        let test_db = TestDb::new("test_copy_specific_exclude");
        let db = test_db.db();
        db.clear().unwrap();

        let source = TempFile::create("source.txt").unwrap();
        db.add_tags(
            source.path(),
            vec!["tag1".into(), "tag2".into(), "tag3".into(), "tag4".into()],
        )
        .unwrap();

        let target = TempFile::create("target.txt").unwrap();
        db.add_tags(target.path(), vec!["initial".into()]).unwrap();

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

        // Specify tags 1, 2, 3 but exclude tag2
        copy_tags(
            db,
            source.path(),
            &params,
            Some(&["tag1".to_string(), "tag2".to_string(), "tag3".to_string()]),
            &["tag2".to_string()],
            false,
            true,
            true,
        )
        .unwrap();

        // Should only have tag1 and tag3 (tag2 excluded, tag4 not specified)
        let tags = db.get_tags(target.path()).unwrap().unwrap();
        assert!(tags.contains(&"initial".to_string()));
        assert!(tags.contains(&"tag1".to_string()));
        assert!(!tags.contains(&"tag2".to_string()));
        assert!(tags.contains(&"tag3".to_string()));
        assert!(!tags.contains(&"tag4".to_string()));
    }
}
