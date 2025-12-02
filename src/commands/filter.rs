//! Filter management command implementation
//!
//! This module provides commands for managing saved search filters:
//! - List all filters
//! - Show detailed filter information
//! - Create new filters
//! - Delete existing filters
//! - Rename filters
//! - Export filters to file
//! - Import filters from file
//! - Show filter usage statistics

use crate::TagrError;
use crate::cli::FilterCommands;
use crate::filters::{FileMode, FilterCriteria, FilterManager, TagMode};
use std::io::Write;

type Result<T> = std::result::Result<T, TagrError>;

/// Execute a filter management command
///
/// Routes to the appropriate subcommand handler based on the command type.
///
/// # Arguments
/// * `command` - The filter subcommand to execute
/// * `quiet` - If true, suppress informational output
///
/// # Errors
///
/// Returns `TagrError` if:
/// - Filter storage cannot be accessed
/// - Filter validation fails
/// - Any filter operation fails
pub fn execute(command: &FilterCommands, quiet: bool) -> Result<()> {
    match command {
        FilterCommands::List => {
            list_filters(quiet)?;
        }
        FilterCommands::Show { name } => {
            show_filter(name, quiet)?;
        }
        FilterCommands::Create {
            name,
            description,
            criteria,
        } => {
            let tag_mode = if criteria.any_tag {
                TagMode::Any
            } else {
                TagMode::All
            };
            let file_mode = if criteria.any_file {
                FileMode::Any
            } else {
                FileMode::All
            };
            let virtual_mode = if criteria.any_virtual {
                TagMode::Any
            } else {
                TagMode::All
            };

            create_filter(
                name,
                description.as_deref(),
                &criteria.tags,
                tag_mode,
                &criteria.file_patterns,
                file_mode,
                &criteria.excludes,
                criteria.regex_tag,
                criteria.regex_file,
                &criteria.virtual_tags,
                virtual_mode,
                quiet,
            )?;
        }
        FilterCommands::Delete { name, force } => {
            delete_filter(name, *force, quiet)?;
        }
        FilterCommands::Rename { old_name, new_name } => {
            rename_filter(old_name, new_name, quiet)?;
        }
        FilterCommands::Export { filters, output } => {
            export_filters(filters, output.as_ref(), quiet)?;
        }
        FilterCommands::Import {
            path,
            overwrite,
            skip_existing,
        } => {
            import_filters(path, *overwrite, *skip_existing, quiet)?;
        }
        FilterCommands::Stats => {
            show_stats(quiet)?;
        }
    }
    Ok(())
}

/// List all saved filters
fn list_filters(quiet: bool) -> Result<()> {
    let filter_path = crate::filters::get_filter_path()?;
    let manager = FilterManager::new(filter_path);

    let filters = manager.list()?;

    if filters.is_empty() {
        if !quiet {
            println!("No saved filters.");
            println!("Create one with: tagr filter create <name> [options]");
        }
        return Ok(());
    }

    if !quiet {
        println!("Saved Filters:");
        println!();
    }

    let max_name_len = filters
        .iter()
        .map(|f| f.name.len())
        .max()
        .unwrap_or(0)
        .max(4);

    for filter in filters {
        let tags_count = filter.criteria.tags.len();
        let files_count = filter.criteria.file_patterns.len();

        if quiet {
            println!("{}", filter.name);
        } else {
            let desc = if filter.description.is_empty() {
                String::from("(no description)")
            } else {
                filter.description.clone()
            };

            println!("  {:<width$}  {}", filter.name, desc, width = max_name_len);

            if tags_count > 0 || files_count > 0 {
                let mut details = Vec::new();
                if tags_count > 0 {
                    details.push(format!(
                        "{} tag{}",
                        tags_count,
                        if tags_count == 1 { "" } else { "s" }
                    ));
                }
                if files_count > 0 {
                    details.push(format!(
                        "{} pattern{}",
                        files_count,
                        if files_count == 1 { "" } else { "s" }
                    ));
                }
                println!(
                    "  {:<width$}  ({})",
                    "",
                    details.join(", "),
                    width = max_name_len
                );
            }
        }
    }

    Ok(())
}

/// Show detailed information about a specific filter
fn show_filter(name: &str, quiet: bool) -> Result<()> {
    let filter_path = crate::filters::get_filter_path()?;
    let manager = FilterManager::new(filter_path);

    let filter = manager.get(name)?;

    if quiet {
        println!("{}", filter.name);
    } else {
        print!("{filter}");
    }

    Ok(())
}

/// Create a new filter with the specified criteria
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
fn create_filter(
    name: &str,
    description: Option<&str>,
    tags: &[String],
    tag_mode: TagMode,
    file_patterns: &[String],
    file_mode: FileMode,
    excludes: &[String],
    regex_tag: bool,
    regex_file: bool,
    virtual_tags: &[String],
    virtual_mode: TagMode,
    quiet: bool,
) -> Result<()> {
    let filter_path = crate::filters::get_filter_path()?;
    let manager = FilterManager::new(filter_path);

    let criteria = FilterCriteria {
        tags: tags.to_vec(),
        tag_mode,
        file_patterns: file_patterns.to_vec(),
        file_mode,
        excludes: excludes.to_vec(),
        regex_tag,
        regex_file,
        glob_files: false,
        virtual_tags: virtual_tags.to_vec(),
        virtual_mode,
    };

    let desc = description.unwrap_or("").to_string();

    manager.create(name, desc, criteria)?;

    if !quiet {
        println!("Filter '{name}' created successfully");
    }

    Ok(())
}

/// Delete a filter by name
fn delete_filter(name: &str, force: bool, quiet: bool) -> Result<()> {
    let filter_path = crate::filters::get_filter_path()?;
    let manager = FilterManager::new(filter_path);

    let _ = manager.get(name)?;

    if !force && !quiet {
        print!("Delete filter '{name}'? (y/N): ");
        std::io::stdout().flush()?;

        let mut response = String::new();
        std::io::stdin().read_line(&mut response)?;

        let response = response.trim().to_lowercase();
        if response != "y" && response != "yes" {
            println!("Cancelled");
            return Ok(());
        }
    }

    manager.delete(name)?;

    if !quiet {
        println!("Filter '{name}' deleted");
    }

    Ok(())
}

/// Rename a filter
fn rename_filter(old_name: &str, new_name: &str, quiet: bool) -> Result<()> {
    let filter_path = crate::filters::get_filter_path()?;
    let manager = FilterManager::new(filter_path);

    manager.rename(old_name, new_name.to_string())?;

    if !quiet {
        println!("Filter '{old_name}' renamed to '{new_name}'");
    }

    Ok(())
}

/// Export filters to a file or stdout
fn export_filters(
    filters: &[String],
    output: Option<&std::path::PathBuf>,
    quiet: bool,
) -> Result<()> {
    let filter_path = crate::filters::get_filter_path()?;
    let manager = FilterManager::new(filter_path);

    if let Some(output_path) = output {
        manager.export(output_path, filters)?;

        if !quiet {
            let count = if filters.is_empty() {
                manager.list()?.len()
            } else {
                filters.len()
            };
            println!(
                "Exported {} filter{} to {}",
                count,
                if count == 1 { "" } else { "s" },
                output_path.display()
            );
        }
    } else {
        let storage = if filters.is_empty() {
            let all_filters = manager.list()?;
            crate::filters::FilterStorage {
                filters: all_filters,
            }
        } else {
            let mut exported = Vec::new();
            for name in filters {
                let filter = manager.get(name)?;
                exported.push(filter);
            }
            crate::filters::FilterStorage { filters: exported }
        };

        let toml =
            toml::to_string_pretty(&storage).map_err(|e| TagrError::FilterError(e.into()))?;
        println!("{toml}");
    }

    Ok(())
}

/// Import filters from a file
fn import_filters(
    path: &std::path::PathBuf,
    overwrite: bool,
    skip_existing: bool,
    quiet: bool,
) -> Result<()> {
    let filter_path = crate::filters::get_filter_path()?;
    let manager = FilterManager::new(filter_path);

    let (imported, skipped) = manager.import(path, overwrite, skip_existing)?;

    if !quiet {
        println!(
            "Imported {} filter{}",
            imported,
            if imported == 1 { "" } else { "s" }
        );
        if skipped > 0 {
            println!(
                "Skipped {} existing filter{}",
                skipped,
                if skipped == 1 { "" } else { "s" }
            );
        }
    }

    Ok(())
}

/// Show filter usage statistics
///
/// NOTE: This is a stub for future implementation of usage statistics.
/// When implemented, it will show:
/// - Most used filters (top 5)
/// - Recently used filters (top 5)
/// - Total filter count
/// - Total usage count across all filters
#[allow(clippy::unnecessary_wraps)]
fn show_stats(_quiet: bool) -> Result<()> {
    println!("Filter statistics feature is not yet implemented.");
    println!("This will show usage statistics for your saved filters.");
    Ok(())
}
