use std::collections::HashSet;
use std::io::Write;
use std::path::Path;

use colored::Colorize;

use crate::cli::ConditionalArgs;
use crate::patterns::{PatternBuilder, PatternContext};
use crate::store::TagStore;
use crate::types::{MatchMode, QueryCriteria, TagExpr, TagName, TagrPath};
use crate::TagrError;

use super::core::{
    BulkAction, BulkOpSummary, SkipReason, confirm_bulk_operation, print_dry_run_preview,
};

type Result<T> = std::result::Result<T, TagrError>;

/// Run `QueryCriteria` through the query engine against a `TagStore`.
fn query_files(store: &dyn TagStore, criteria: &QueryCriteria) -> Result<Vec<TagrPath>> {
    let schema = crate::schema::load_default_schema().unwrap_or_default();
    let results = crate::query::execute(store, criteria, &schema)?;
    Ok(results)
}

/// Validate bulk search criteria using the pattern system.
///
/// Prevents glob-like tokens being supplied as tags without regex flag.
fn validate_bulk_criteria(criteria: &QueryCriteria) -> Result<()> {
    let tags: Vec<String> = criteria
        .flat_include_tags()
        .map(|set| set.iter().map(|t| t.as_str().to_string()).collect())
        .unwrap_or_default();

    let mut builder = PatternBuilder::new(PatternContext::BulkFiles)
        .regex_tags(criteria.regex_tags)
        .regex_files(criteria.regex_files)
        .glob_files_flag(!criteria.regex_files);

    for t in &tags {
        builder.add_tag_token(t);
    }
    for f in &criteria.file_patterns {
        builder.add_file_token(f);
    }

    let to_search_mode = |m: MatchMode| match m {
        MatchMode::All => crate::cli::SearchMode::All,
        MatchMode::Any => crate::cli::SearchMode::Any,
    };

    let tag_mode = if criteria.tag_expr.as_ref().is_some_and(|e| matches!(e, TagExpr::And(_))) {
        crate::cli::SearchMode::All
    } else {
        crate::cli::SearchMode::Any
    };

    let _ = builder.build(tag_mode, to_search_mode(criteria.file_mode))?;
    Ok(())
}

/// Check if a file meets conditional requirements
fn check_conditions(
    file: &TagrPath,
    store: &dyn TagStore,
    conditions: &ConditionalArgs,
    tags_to_add: &[String],
) -> Result<bool> {
    let file_tags: Vec<String> = store
        .get_tags(file)?
        .unwrap_or_default()
        .into_iter()
        .map(TagName::into_inner)
        .collect();
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
    store: &dyn TagStore,
    criteria: &QueryCriteria,
    tags: &[String],
    conditions: &ConditionalArgs,
    dry_run: bool,
    yes: bool,
    quiet: bool,
    writer: &mut impl Write,
) -> Result<()> {
    if tags.is_empty() {
        return Err(TagrError::InvalidInput("No tags provided".into()));
    }
    validate_bulk_criteria(criteria)?;
    let files = query_files(store, criteria)?;
    let path_bufs: Vec<std::path::PathBuf> = files.iter().map(|f| f.as_path().to_path_buf()).collect();
    if files.is_empty() {
        if !quiet {
            writeln!(writer, "No files match the specified criteria.")?;
        }
        return Ok(());
    }
    if dry_run {
        print_dry_run_preview(&path_bufs, tags, BulkAction::Add, writer)?;
        return Ok(());
    }
    if !yes && !confirm_bulk_operation(&path_bufs, tags, BulkAction::Add)? {
        writeln!(writer, "Operation cancelled.")?;
        return Ok(());
    }
    let tag_names: Vec<TagName> = tags.iter().map(TagName::new).collect::<std::result::Result<Vec<_>, _>>()?;
    let mut summary = BulkOpSummary::new();
    for file in &files {
        match check_conditions(file, store, conditions, tags) {
            Ok(true) => match store.add_tags(file, tag_names.clone()) {
                Ok(()) => {
                    summary.add_success();
                    if !quiet {
                        writeln!(writer, "✓ Tagged: {file}")?;
                    }
                }
                Err(e) => {
                    summary.add_error(format!("{file}: {e}"));
                    if !quiet {
                        eprintln!("✗ Failed to tag {file}: {e}");
                    }
                }
            },
            Ok(false) => {
                let _ = SkipReason::ConditionNotMet;
                summary.add_skip_condition();
                if !quiet {
                    writeln!(writer, "⊘ Skipped (condition): {file}")?;
                }
            }
            Err(e) => {
                summary.add_error(format!("{file}: {e}"));
                if !quiet {
                    eprintln!("✗ Failed to check conditions for {file}: {e}");
                }
            }
        }
    }

    // Invalidate completion cache (bulk ops may introduce new tags)
    #[cfg(feature = "dynamic-completions")]
    crate::completions::invalidate_database_cache();

    if !quiet {
        summary.print("Bulk Tag", writer)?;
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
    store: &dyn TagStore,
    criteria: &QueryCriteria,
    tags: &[String],
    remove_all: bool,
    conditions: &ConditionalArgs,
    dry_run: bool,
    yes: bool,
    quiet: bool,
    writer: &mut impl Write,
) -> Result<()> {
    if !remove_all && tags.is_empty() {
        return Err(TagrError::InvalidInput(
            "No tags provided. Use --all to remove all tags".into(),
        ));
    }
    validate_bulk_criteria(criteria)?;
    let files = query_files(store, criteria)?;
    let path_bufs: Vec<std::path::PathBuf> = files.iter().map(|f| f.as_path().to_path_buf()).collect();
    if files.is_empty() {
        if !quiet {
            writeln!(writer, "No files match the specified criteria.")?;
        }
        return Ok(());
    }
    if dry_run {
        print_dry_run_preview(
            &path_bufs,
            if remove_all { &[] } else { tags },
            if remove_all {
                BulkAction::RemoveAll
            } else {
                BulkAction::Remove
            },
            writer,
        )?;
        return Ok(());
    }
    let action = if remove_all {
        BulkAction::RemoveAll
    } else {
        BulkAction::Remove
    };
    if !yes && !confirm_bulk_operation(&path_bufs, tags, action)? {
        writeln!(writer, "Operation cancelled.")?;
        return Ok(());
    }
    let tag_names: Vec<TagName> = tags.iter().map(TagName::new).collect::<std::result::Result<Vec<_>, _>>()?;
    let mut summary = BulkOpSummary::new();
    for file in &files {
        match check_conditions(file, store, conditions, tags) {
            Ok(true) => {
                let result = if remove_all {
                    store.remove_file(file).map(|_| ())
                } else {
                    store.remove_tags(file, &tag_names)
                };
                match result {
                    Ok(()) => {
                        summary.add_success();
                        if !quiet {
                            writeln!(writer, "✓ Untagged: {file}")?;
                        }
                    }
                    Err(e) => {
                        summary.add_error(format!("{file}: {e}"));
                        if !quiet {
                            eprintln!("✗ Failed to untag {file}: {e}");
                        }
                    }
                }
            }
            Ok(false) => {
                let _ = SkipReason::ConditionNotMet;
                summary.add_skip_condition();
                if !quiet {
                    writeln!(writer, "⊘ Skipped (condition): {file}")?;
                }
            }
            Err(e) => {
                summary.add_error(format!("{file}: {e}"));
                if !quiet {
                    eprintln!("✗ Failed to check conditions for {file}: {e}");
                }
            }
        }
    }

    // Invalidate completion cache (bulk ops may orphan tags)
    #[cfg(feature = "dynamic-completions")]
    crate::completions::invalidate_database_cache();

    if !quiet {
        summary.print("Bulk Untag", writer)?;
    }
    Ok(())
}

/// Rename a tag across all files where it appears.
///
/// # Errors
/// Returns database errors during lookups and updates, and `TagrError::InvalidInput`
/// for invalid arguments (e.g., identical old/new names).
pub fn rename_tag(
    store: &dyn TagStore,
    old_tag: &str,
    new_tag: &str,
    dry_run: bool,
    yes: bool,
    quiet: bool,
    writer: &mut impl Write,
) -> Result<()> {
    if old_tag == new_tag {
        return Err(TagrError::InvalidInput(
            "Old and new tag names are identical".into(),
        ));
    }
    let old_tag_name = TagName::new(old_tag)?;
    let new_tag_name = TagName::new(new_tag)?;
    let files = store.find_by_tag(&old_tag_name)?;
    if files.is_empty() {
        if !quiet {
            writeln!(writer, "Tag '{old_tag}' not found in database.")?;
        }
        return Ok(());
    }
    if dry_run {
        writeln!(writer, "{}", "=== Dry Run Mode ===".yellow().bold())?;
        writeln!(
            writer,
            "Would rename tag '{}' → '{}' in {} file(s)",
            old_tag.cyan(),
            new_tag.green(),
            files.len()
        )?;
        writeln!(writer, "\n{}", "Affected files:".bold())?;
        for (i, file) in files.iter().enumerate().take(10) {
            writeln!(writer, "  {}. {file}", i + 1)?;
        }
        if files.len() > 10 {
            writeln!(writer, "  ... and {} more", files.len() - 10)?;
        }
        writeln!(writer, "\n{}", "Run without --dry-run to apply changes.".yellow())?;
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
            writeln!(writer, "Operation cancelled.")?;
            return Ok(());
        }
    }
    let mut summary = BulkOpSummary::new();
    for file in &files {
        let Some(current_tags) = store.get_tags(file)? else {
            summary.add_skip();
            continue;
        };
        let new_tags: Vec<TagName> = current_tags
            .into_iter()
            .map(|t| if t == old_tag_name { new_tag_name.clone() } else { t })
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        match store.insert(file, new_tags) {
            Ok(()) => {
                summary.add_success();
                if !quiet {
                    writeln!(writer, "✓ Renamed in: {file}")?;
                }
            }
            Err(e) => {
                summary.add_error(format!("{file}: {e}"));
                if !quiet {
                    eprintln!("✗ Failed to rename in {file}: {e}");
                }
            }
        }
    }
    if !quiet {
        writeln!(
            writer,
            "\n{} Renamed '{}' → '{}' in {} file(s)",
            "✓".green(),
            old_tag,
            new_tag,
            summary.success
        )?;
        if summary.errors > 0 {
            summary.print("Rename Tag", writer)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    // Validation of glob-like tag tokens is now handled by TagName::new()
    // at construction time — invalid characters like *, ?, / are rejected
    // before reaching validate_bulk_criteria.
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
    store: &dyn TagStore,
    source_file: &Path,
    criteria: &QueryCriteria,
    config: CopyTagsConfig,
    writer: &mut impl Write,
) -> Result<()> {
    let source_path = TagrPath::new(source_file)?;
    let source_tags = store.get_tags(&source_path)?.ok_or_else(|| {
        TagrError::InvalidInput(format!(
            "Source file not in database: {}",
            source_file.display()
        ))
    })?;
    let tags_to_copy: Vec<TagName> = source_tags
        .into_iter()
        .filter(|tag| {
            let tag_str = tag.as_str();
            if let Some(specific) = config.specific_tags
                && !specific.iter().any(|s| s == tag_str)
            {
                return false;
            }
            !config.exclude_tags.iter().any(|s| s == tag_str)
        })
        .collect();
    if tags_to_copy.is_empty() {
        if !config.quiet {
            writeln!(writer, "No tags to copy after filtering.")?;
        }
        return Ok(());
    }
    validate_bulk_criteria(criteria)?;
    let target_files = query_files(store, criteria)?;
    if target_files.is_empty() {
        if !config.quiet {
            writeln!(writer, "No target files match the specified criteria.")?;
        }
        return Ok(());
    }
    let target_files: Vec<TagrPath> = target_files
        .into_iter()
        .filter(|f| f != &source_path)
        .collect();
    if target_files.is_empty() {
        if !config.quiet {
            writeln!(writer, "No target files to copy tags to (excluding source file).")?;
        }
        return Ok(());
    }
    let tag_strs: Vec<String> = tags_to_copy.iter().map(|t| t.as_str().to_string()).collect();
    if config.dry_run {
        writeln!(writer, "{}", "=== Dry Run Mode ===".yellow().bold())?;
        writeln!(
            writer,
            "Would copy tags [{}] from '{}' to {} file(s)",
            tag_strs.join(", ").cyan(),
            source_file.display(),
            target_files.len()
        )?;
        writeln!(writer, "\n{}", "Target files:".bold())?;
        for (i, file) in target_files.iter().enumerate().take(10) {
            writeln!(writer, "  {}. {file}", i + 1)?;
        }
        if target_files.len() > 10 {
            writeln!(writer, "  ... and {} more", target_files.len() - 10)?;
        }
        writeln!(writer, "\n{}", "Run without --dry-run to apply changes.".yellow())?;
        return Ok(());
    }
    if !config.yes {
        let prompt = format!(
            "Copy tags [{}] from '{}' to {} file(s)?",
            tag_strs.join(", ").cyan(),
            source_file.display(),
            target_files.len()
        );
        let confirmed = dialoguer::Confirm::new()
            .with_prompt(prompt)
            .interact()
            .map_err(|e| TagrError::InvalidInput(format!("Failed to get confirmation: {e}")))?;
        if !confirmed {
            writeln!(writer, "Operation cancelled.")?;
            return Ok(());
        }
    }
    let mut summary = BulkOpSummary::new();
    for file in &target_files {
        match store.add_tags(file, tags_to_copy.clone()) {
            Ok(()) => {
                summary.add_success();
                if !config.quiet {
                    writeln!(writer, "✓ Copied tags to: {file}")?;
                }
            }
            Err(e) => {
                summary.add_error(format!("{file}: {e}"));
                if !config.quiet {
                    eprintln!("✗ Failed to copy tags to {file}: {e}");
                }
            }
        }
    }
    if !config.quiet {
        summary.print("Copy Tags", writer)?;
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
    store: &dyn TagStore,
    source_tags: &[String],
    target_tag: &str,
    dry_run: bool,
    yes: bool,
    quiet: bool,
    writer: &mut impl Write,
) -> Result<()> {
    if source_tags.is_empty() {
        return Err(TagrError::InvalidInput("No source tags provided".into()));
    }
    if source_tags.contains(&target_tag.to_string()) {
        return Err(TagrError::InvalidInput(
            "Target tag cannot be one of the source tags".into(),
        ));
    }
    let source_tag_names: Vec<TagName> = source_tags.iter().map(TagName::new).collect::<std::result::Result<Vec<_>, _>>()?;
    let target_tag_name = TagName::new(target_tag)?;
    let mut files_set = HashSet::new();
    for tag_name in &source_tag_names {
        let tag_files = store.find_by_tag(tag_name)?;
        files_set.extend(tag_files);
    }
    let files: Vec<TagrPath> = files_set.into_iter().collect();
    if files.is_empty() {
        if !quiet {
            writeln!(
                writer,
                "No files found with source tags: [{}]",
                source_tags.join(", ")
            )?;
        }
        return Ok(());
    }
    if dry_run {
        writeln!(writer, "{}", "=== Dry Run Mode ===".yellow().bold())?;
        writeln!(
            writer,
            "Would merge tags [{}] → '{}' in {} file(s)",
            source_tags.join(", ").cyan(),
            target_tag.green(),
            files.len()
        )?;
        writeln!(writer, "\n{}", "Affected files:".bold())?;
        for (i, file) in files.iter().enumerate().take(10) {
            writeln!(writer, "  {}. {file}", i + 1)?;
        }
        if files.len() > 10 {
            writeln!(writer, "  ... and {} more", files.len() - 10)?;
        }
        writeln!(writer, "\n{}", "Run without --dry-run to apply changes.".yellow())?;
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
            writeln!(writer, "Operation cancelled.")?;
            return Ok(());
        }
    }
    let mut summary = BulkOpSummary::new();
    for file in &files {
        let Some(current_tags) = store.get_tags(file)? else {
            summary.add_skip();
            continue;
        };
        let new_tags: Vec<TagName> = current_tags
            .into_iter()
            .map(|t| {
                if source_tag_names.contains(&t) {
                    target_tag_name.clone()
                } else {
                    t
                }
            })
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        match store.insert(file, new_tags) {
            Ok(()) => {
                summary.add_success();
                if !quiet {
                    writeln!(writer, "✓ Merged in: {file}")?;
                }
            }
            Err(e) => {
                summary.add_error(format!("{file}: {e}"));
                if !quiet {
                    eprintln!("✗ Failed to merge in {file}: {e}");
                }
            }
        }
    }
    if !quiet {
        writeln!(
            writer,
            "\n{} Merged [{}] → '{}' in {} file(s)",
            "✓".green(),
            source_tags.join(", "),
            target_tag,
            summary.success
        )?;
        if summary.errors > 0 {
            summary.print("Merge Tags", writer)?;
        }
    }
    Ok(())
}
