use std::collections::HashSet;
use std::path::{Path, PathBuf};

use colored::Colorize;

use crate::cli::{ConditionalArgs, SearchParams};
use crate::db::Database;
use crate::patterns::{PatternBuilder, PatternContext};
use crate::{Pair, TagrError};

use super::core::{
    BulkAction, BulkOpSummary, SkipReason, confirm_bulk_operation, print_dry_run_preview,
};

type Result<T> = std::result::Result<T, TagrError>;
/// Normalize and validate bulk search params using the pattern system.
///
/// - Implicitly enables glob handling for file patterns in bulk context
/// - Prevents glob-like tokens being supplied as tags without regex flag
fn normalize_bulk_params(params: &mut SearchParams) -> Result<()> {
    // Builder validates separation and will error on glob-like tags
    let mut builder = PatternBuilder::new(PatternContext::BulkFiles)
        .regex_tags(params.regex_tag)
        .regex_files(params.regex_file)
        .glob_files_flag(params.glob_files);

    for t in &params.tags {
        builder.add_tag_token(t);
    }
    for f in &params.file_patterns {
        builder.add_file_token(f);
    }

    // Build to run validation; we ignore the typed queries for now
    // as DB integration is out of scope. Errors propagate via TagrError::from.
    let _ = builder.build(params.tag_mode, params.file_mode)?;

    // Implicit glob enable: if any file token looks like a glob and regex_file is false
    if !params.regex_file
        && params
            .file_patterns
            .iter()
            .any(|p| p.contains('*') || p.contains('?') || p.contains('['))
    {
        params.glob_files = true;
    }
    Ok(())
}

/// Check if a file meets conditional requirements
fn check_conditions(
    file: &Path,
    db: &Database,
    conditions: &ConditionalArgs,
    tags_to_add: &[String],
) -> Result<bool> {
    let file_tags = db.get_tags(file)?.unwrap_or_default();
    if conditions.if_not_exists && tags_to_add.iter().any(|t| file_tags.contains(t)) {
        return Ok(false);
    }
    if !conditions.if_has_tag.is_empty()
        && !conditions.if_has_tag.iter().all(|t| file_tags.contains(t))
    {
        return Ok(false);
    }
    if !conditions.if_missing_tag.is_empty()
        && !conditions
            .if_missing_tag
            .iter()
            .any(|t| !file_tags.contains(t))
    {
        return Ok(false);
    }
    Ok(true)
}

/// Add tags in bulk to files matching the search parameters.
///
/// # Errors
/// Returns database errors from query and tag operations, and `TagrError::InvalidInput`
/// for invalid arguments (e.g., empty tag list).
#[allow(clippy::too_many_arguments)]
pub fn bulk_tag(
    db: &Database,
    mut params: SearchParams,
    tags: &[String],
    conditions: &ConditionalArgs,
    dry_run: bool,
    yes: bool,
    quiet: bool,
) -> Result<()> {
    if tags.is_empty() {
        return Err(TagrError::InvalidInput("No tags provided".into()));
    }
    normalize_bulk_params(&mut params)?;
    let files = crate::db::query::apply_search_params(db, &params)?;
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
    if !yes && !confirm_bulk_operation(&files, tags, BulkAction::Add)? {
        println!("Operation cancelled.");
        return Ok(());
    }
    let mut summary = BulkOpSummary::new();
    for file in &files {
        match check_conditions(file, db, conditions, tags) {
            Ok(true) => match db.add_tags(file, tags.to_vec()) {
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
            },
            Ok(false) => {
                let _ = SkipReason::ConditionNotMet;
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

/// Remove tags in bulk, optionally removing all tags from matched files.
///
/// # Errors
/// Returns database errors from query and tag operations, and `TagrError::InvalidInput`
/// for invalid arguments (e.g., missing tags without `--all`).
#[allow(clippy::too_many_arguments)]
#[allow(clippy::fn_params_excessive_bools)]
pub fn bulk_untag(
    db: &Database,
    mut params: SearchParams,
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
    normalize_bulk_params(&mut params)?;
    let files = crate::db::query::apply_search_params(db, &params)?;
    if files.is_empty() {
        if !quiet {
            println!("No files match the specified criteria.");
        }
        return Ok(());
    }
    if dry_run {
        print_dry_run_preview(
            &files,
            if remove_all { &[] } else { tags },
            if remove_all {
                BulkAction::RemoveAll
            } else {
                BulkAction::Remove
            },
        );
        return Ok(());
    }
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
        match check_conditions(file, db, conditions, tags) {
            Ok(true) => {
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
                let _ = SkipReason::ConditionNotMet;
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

/// Rename a tag across all files where it appears.
///
/// # Errors
/// Returns database errors during lookups and updates, and `TagrError::InvalidInput`
/// for invalid arguments (e.g., identical old/new names).
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
    if !yes {
        let prompt = format!(
            "Rename tag '{}' to '{}' in {} file(s)?",
            old_tag.cyan(),
            new_tag.green(),
            files.len()
        );
        let confirmed = dialoguer::Confirm::new()
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
        let Some(current_tags) = db.get_tags(file)? else {
            summary.add_skip();
            continue;
        };
        let new_tags: Vec<String> = current_tags
            .into_iter()
            .map(|t| if t == old_tag { new_tag.to_string() } else { t })
            .collect::<HashSet<_>>()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_sets_glob_files_on_wildcards() {
        let mut params = SearchParams {
            query: None,
            tags: vec![],
            tag_mode: crate::cli::SearchMode::All,
            file_patterns: vec!["**/*.rs".to_string(), "src/?ain.rs".to_string()],
            file_mode: crate::cli::SearchMode::All,
            exclude_tags: vec![],
            regex_tag: false,
            regex_file: false,
            glob_files: false,
            virtual_tags: vec![],
            virtual_mode: crate::cli::SearchMode::All,
            no_hierarchy: false,
        };

        normalize_bulk_params(&mut params).expect("normalize should succeed");
        assert!(
            params.glob_files,
            "glob_files should be enabled when patterns have wildcards"
        );
    }

    #[test]
    fn test_normalize_preserves_regex_file_flag() {
        let mut params = SearchParams {
            query: None,
            tags: vec![],
            tag_mode: crate::cli::SearchMode::All,
            file_patterns: vec![".*\\.md".to_string()],
            file_mode: crate::cli::SearchMode::All,
            exclude_tags: vec![],
            regex_tag: false,
            regex_file: true,
            glob_files: false,
            virtual_tags: vec![],
            virtual_mode: crate::cli::SearchMode::All,
            no_hierarchy: false,
        };

        normalize_bulk_params(&mut params).expect("normalize should succeed");
        assert!(params.regex_file, "regex_file should remain true");
        assert!(
            !params.glob_files,
            "glob_files should remain false when regex_file is true"
        );
    }

    #[test]
    fn test_normalize_errors_on_glob_like_tags() {
        let mut params = SearchParams {
            query: None,
            tags: vec!["feature/*".to_string()],
            tag_mode: crate::cli::SearchMode::All,
            file_patterns: vec!["src".to_string()],
            file_mode: crate::cli::SearchMode::All,
            exclude_tags: vec![],
            regex_tag: false,
            regex_file: false,
            glob_files: false,
            virtual_tags: vec![],
            virtual_mode: crate::cli::SearchMode::All,
            no_hierarchy: false,
        };

        let err = normalize_bulk_params(&mut params)
            .err()
            .expect("should error");
        match err {
            TagrError::PatternError(_) => {}
            _ => panic!("Expected PatternError for glob-like tag token"),
        }
    }
}

#[derive(Clone, Copy)]
pub struct CopyTagsConfig<'a> {
    pub specific_tags: Option<&'a [String]>,
    pub exclude_tags: &'a [String],
    pub dry_run: bool,
    pub yes: bool,
    pub quiet: bool,
}

/// Copy tags from a source file to a set of target files.
///
/// # Errors
/// Returns database errors during lookups and updates, and `TagrError::InvalidInput`
/// when the source file is missing or after filtering no tags are available.
pub fn copy_tags(
    db: &Database,
    source_file: &Path,
    mut params: SearchParams,
    config: CopyTagsConfig,
) -> Result<()> {
    let source_tags = db.get_tags(source_file)?.ok_or_else(|| {
        TagrError::InvalidInput(format!(
            "Source file not in database: {}",
            source_file.display()
        ))
    })?;
    let tags_to_copy: Vec<String> = source_tags
        .into_iter()
        .filter(|tag| {
            if let Some(specific) = config.specific_tags
                && !specific.contains(tag)
            {
                return false;
            }
            !config.exclude_tags.contains(tag)
        })
        .collect();
    if tags_to_copy.is_empty() {
        if !config.quiet {
            println!("No tags to copy after filtering.");
        }
        return Ok(());
    }
    normalize_bulk_params(&mut params)?;
    let target_files = crate::db::query::apply_search_params(db, &params)?;
    if target_files.is_empty() {
        if !config.quiet {
            println!("No target files match the specified criteria.");
        }
        return Ok(());
    }
    let target_files: Vec<PathBuf> = target_files
        .into_iter()
        .filter(|f| f != source_file)
        .collect();
    if target_files.is_empty() {
        if !config.quiet {
            println!("No target files to copy tags to (excluding source file).");
        }
        return Ok(());
    }
    if config.dry_run {
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
    if !config.yes {
        let prompt = format!(
            "Copy tags [{}] from '{}' to {} file(s)?",
            tags_to_copy.join(", ").cyan(),
            source_file.display(),
            target_files.len()
        );
        let confirmed = dialoguer::Confirm::new()
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
                if !config.quiet {
                    println!("✓ Copied tags to: {}", file.display());
                }
            }
            Err(e) => {
                summary.add_error(format!("{}: {}", file.display(), e));
                if !config.quiet {
                    eprintln!("✗ Failed to copy tags to {}: {}", file.display(), e);
                }
            }
        }
    }
    if !config.quiet {
        summary.print("Copy Tags");
    }
    Ok(())
}

/// Merge multiple source tags into a single target tag across matched files.
///
/// # Errors
/// Returns database errors during lookups and updates, and `TagrError::InvalidInput`
/// for invalid inputs (e.g., empty source tags, target among sources).
#[allow(clippy::too_many_lines)]
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
    if !yes {
        let prompt = format!(
            "Merge tags [{}] into '{}' in {} file(s)?",
            source_tags.join(", ").cyan(),
            target_tag.green(),
            files.len()
        );
        let confirmed = dialoguer::Confirm::new()
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
        let Some(current_tags) = db.get_tags(file)? else {
            summary.add_skip();
            continue;
        };
        let new_tags: Vec<String> = current_tags
            .into_iter()
            .map(|t| {
                if source_tags.contains(&t) {
                    target_tag.to_string()
                } else {
                    t
                }
            })
            .collect::<HashSet<_>>()
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
