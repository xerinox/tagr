//! Tagr CLI application entry point
//!
//! This is the main executable for the tagr file tagging system. It provides a command-line
//! interface for managing file tags and performing interactive searches.
//!
//! # Features
//!
//! - **Browse Mode**: Interactive fuzzy finder for selecting tags and files
//! - **Tag Management**: Add and manage tags for files
//! - **Search**: Find files by tag with efficient reverse index lookups
//! - **Database Management**: Configure and manage multiple tag databases
//! - **Quiet Mode**: Suppress informational output for scripting
//!
//! # Usage
//!
//! ```bash
//! # Browse files interactively (default command)
//! tagr
//! tagr browse
//!
//! # Tag a file
//! tagr tag file.txt tag1 tag2
//! tagr tag -f file.txt -t tag1 tag2
//!
//! # Search for files by tag
//! tagr search tag1
//! tagr search -t tag1
//!
//! # Execute a command on selected files
//! tagr browse -x "cat {}"
//!
//! # Clean up database (remove missing files and files with no tags)
//! tagr cleanup
//! tagr c
//!
//! # Quiet mode (only output results)
//! tagr -q search tag1
//! ```
//!
//! # Configuration
//!
//! On first run, tagr will prompt for initial setup. Configuration is stored in
//! the user's config directory (`~/.config/tagr/config.toml` on Linux).

use tagr::{
    TagrError,
    cli::{Cli, Commands, ConfigCommands, DbCommands, SearchParams},
    commands, config,
    db::Database,
};

type Result<T> = std::result::Result<T, TagrError>;

/// Handle the db command - manage multiple databases
#[allow(clippy::too_many_lines)]
fn handle_db_command(
    mut config: config::TagrConfig,
    command: &DbCommands,
    quiet: bool,
) -> Result<()> {
    match command {
        DbCommands::Add { name, path } => {
            if config.get_database(name).is_some() {
                if !quiet {
                    eprintln!("Error: Database '{name}' already exists");
                }
                return Err(TagrError::InvalidInput(format!(
                    "Database '{name}' already exists"
                )));
            }

            let resolved_path = if path.components().count() == 1 {
                let data_dir = dirs::data_local_dir().ok_or_else(|| {
                    TagrError::InvalidInput("Could not determine data directory".into())
                })?;
                data_dir.join("tagr").join(path)
            } else {
                path.clone()
            };

            config.add_database(name.clone(), resolved_path.clone())?;

            if !resolved_path.exists() {
                std::fs::create_dir_all(&resolved_path)?;
            }

            if !quiet {
                println!("Database '{name}' added at {}", resolved_path.display());
            }

            if config.databases.len() == 1 {
                config.set_default_database(name.clone())?;
                if !quiet {
                    println!("Set '{name}' as default database");
                }
            }
        }
        DbCommands::List => {
            if config.databases.is_empty() {
                if !quiet {
                    println!("No databases configured.");
                    println!("Add one with: tagr db add <name> <path>");
                }
                return Ok(());
            }

            if !quiet {
                println!("Configured databases:");
            }

            let default_db = config.get_default_database();
            let mut db_names: Vec<_> = config.list_databases();
            db_names.sort();

            for name in db_names {
                if let Some(path) = config.get_database(name) {
                    let is_default = default_db == Some(name);
                    let marker = if is_default { " (default)" } else { "" };

                    if quiet {
                        println!("{name}");
                    } else {
                        println!("  {} -> {}{}", name, path.display(), marker);
                    }
                }
            }
        }
        DbCommands::Remove { name, delete_files } => {
            if config.get_database(name).is_none() {
                if !quiet {
                    eprintln!("Error: Database '{name}' does not exist");
                }
                return Err(TagrError::InvalidInput(format!(
                    "Database '{name}' does not exist"
                )));
            }

            let is_default = config.get_default_database() == Some(name);
            if is_default && !quiet {
                println!(
                    "Warning: Removing the default database. You'll need to set a new default."
                );
            }

            let removed_path = config.remove_database(name)?;

            if let Some(path) = removed_path {
                if !quiet {
                    println!("Database '{name}' removed from configuration");
                }

                if *delete_files {
                    if path.exists() {
                        match std::fs::remove_dir_all(&path) {
                            Ok(()) => {
                                if !quiet {
                                    println!("Database files deleted from {}", path.display());
                                }
                            }
                            Err(e) => {
                                if !quiet {
                                    eprintln!("Warning: Failed to delete database files: {e}");
                                }
                            }
                        }
                    } else if !quiet {
                        println!(
                            "Database files at {} do not exist (already deleted)",
                            path.display()
                        );
                    }
                } else if !quiet {
                    println!(
                        "Note: Database files at {} were NOT deleted",
                        path.display()
                    );
                }
            }

            if is_default {
                config.default_database = None;
                config.save()?;
            }
        }
        DbCommands::SetDefault { name } => {
            if config.get_database(name).is_none() {
                if !quiet {
                    eprintln!("Error: Database '{name}' does not exist");
                }
                return Err(TagrError::InvalidInput(format!(
                    "Database '{name}' does not exist"
                )));
            }

            config.set_default_database(name.clone())?;

            if !quiet {
                println!("Set '{name}' as default database");
            }
        }
    }
    Ok(())
}

/// Handle the config command - manage application settings
///
/// Performs configuration operations including setting and getting config values.
///
/// # Arguments
/// * `config` - Application configuration
/// * `command` - Specific config subcommand to execute
/// * `quiet` - If true, suppress informational output
///
/// # Errors
///
/// Returns `TagrError` if the configuration key is invalid, value parsing fails,
/// or configuration save fails.
fn handle_config_command(
    mut config: config::TagrConfig,
    command: &ConfigCommands,
    quiet: bool,
) -> Result<()> {
    match command {
        ConfigCommands::Set { setting } => {
            let parts: Vec<&str> = setting.splitn(2, '=').collect();
            if parts.len() != 2 {
                return Err(TagrError::InvalidInput(
                    "Invalid format. Use: tagr config set key=value".into(),
                ));
            }

            let key = parts[0].trim();
            let value = parts[1].trim();

            match key {
                "quiet" => {
                    let new_value = value.parse::<bool>().map_err(|_| {
                        TagrError::InvalidInput(format!(
                            "Invalid value for quiet: '{value}'. Use 'true' or 'false'"
                        ))
                    })?;
                    config.quiet = new_value;
                    config.save()?;
                    if !quiet {
                        println!("Set quiet = {new_value}");
                    }
                }
                "path_format" | "path-format" => {
                    let new_value = match value.to_lowercase().as_str() {
                        "absolute" | "abs" => config::PathFormat::Absolute,
                        "relative" | "rel" => config::PathFormat::Relative,
                        _ => {
                            return Err(TagrError::InvalidInput(format!(
                                "Invalid value for path_format: '{value}'. Use 'absolute' or 'relative'"
                            )));
                        }
                    };
                    config.path_format = new_value;
                    config.save()?;
                    if !quiet {
                        println!("Set path_format = {new_value:?}");
                    }
                }
                _ => {
                    return Err(TagrError::InvalidInput(format!(
                        "Unknown configuration key: '{key}'. Available keys: quiet, path_format"
                    )));
                }
            }
        }
        ConfigCommands::Get { key } => match key.as_str() {
            "quiet" => {
                println!("{}", config.quiet);
            }
            "path_format" | "path-format" => {
                let value = match config.path_format {
                    config::PathFormat::Absolute => "absolute",
                    config::PathFormat::Relative => "relative",
                };
                println!("{value}");
            }
            _ => {
                return Err(TagrError::InvalidInput(format!(
                    "Unknown configuration key: '{key}'. Available keys: quiet, path_format"
                )));
            }
        },
    }
    Ok(())
}

/// Main entry point for the tagr application
///
/// Loads configuration, parses command-line arguments, and dispatches to the
/// appropriate command handler.
///
/// # Errors
///
/// Returns `TagrError` if configuration loading fails, database initialization fails,
/// or any command handler returns an error.
fn main() -> Result<()> {
    let config = config::TagrConfig::load_or_setup()?;

    let cli = Cli::parse_args();

    let quiet = cli.quiet || config.quiet;

    let command = cli.get_command();

    if let Commands::Db { command } = &command {
        handle_db_command(config, command, quiet)?;
    } else if let Commands::Config { command } = &command {
        handle_config_command(config, command, quiet)?;
    } else {
        let db_name = command.get_db().or_else(|| {
            config.get_default_database().cloned()
        }).ok_or_else(|| TagrError::InvalidInput(
            "No default database set. Use 'tagr db add <name> <path>' to create one, or specify --db <name>.".into()
        ))?;

        let db_path = config.get_database(&db_name).ok_or_else(|| {
            TagrError::InvalidInput(format!("Database '{db_name}' not found in configuration"))
        })?;

        let db = Database::open(db_path)?;

        // Determine path format: CLI override > config default
        let path_format = if let Some(cli_format) = cli.get_path_format() {
            match cli_format {
                tagr::cli::PathFormat::Absolute => config::PathFormat::Absolute,
                tagr::cli::PathFormat::Relative => config::PathFormat::Relative,
            }
        } else {
            config.path_format
        };

        match &command {
            Commands::Browse { filter_args, .. } => {
                let ctx = command.get_browse_context().unwrap();

                let save_filter = filter_args
                    .save_filter
                    .as_ref()
                    .map(|name| (name.as_str(), filter_args.filter_desc.as_deref()));

                commands::browse(
                    &db,
                    ctx.search_params,
                    filter_args.filter.as_deref(),
                    save_filter,
                    ctx.execute_cmd,
                    Some(&ctx.preview_overrides),
                    path_format,
                    quiet,
                )?;
            }
            Commands::Tag { .. } => {
                let ctx = command.get_tag_context().unwrap();
                commands::tag(&db, ctx.file, &ctx.tags, quiet)?;
            }
            Commands::Search { filter_args, .. } => {
                let params = command.get_search_params().ok_or_else(|| {
                    TagrError::InvalidInput("Failed to parse search parameters".into())
                })?;

                let save_filter = filter_args
                    .save_filter
                    .as_ref()
                    .map(|name| (name.as_str(), filter_args.filter_desc.as_deref()));

                commands::search(
                    &db,
                    params,
                    filter_args.filter.as_deref(),
                    save_filter,
                    path_format,
                    quiet,
                )?;
            }
            Commands::Untag { .. } => {
                let ctx = command.get_untag_context().unwrap();
                commands::tag::untag(&db, ctx.file, &ctx.tags, ctx.all, quiet)?;
            }
            Commands::Tags { command, .. } => {
                commands::tags(&db, command, quiet)?;
            }
            Commands::Bulk { command, .. } => {
                use tagr::cli::BulkCommands;

                match command {
                    BulkCommands::Tag {
                        criteria,
                        add_tags,
                        dry_run,
                        yes,
                    } => {
                        let params = SearchParams::from(criteria);
                        commands::bulk::bulk_tag(&db, &params, add_tags, *dry_run, *yes, quiet)?;
                    }
                    BulkCommands::Untag {
                        criteria,
                        remove_tags,
                        all,
                        dry_run,
                        yes,
                    } => {
                        let params = SearchParams::from(criteria);
                        commands::bulk::bulk_untag(
                            &db,
                            &params,
                            remove_tags,
                            *all,
                            *dry_run,
                            *yes,
                            quiet,
                        )?;
                    }
                    BulkCommands::RenameTag {
                        old_tag,
                        new_tag,
                        dry_run,
                        yes,
                    } => {
                        commands::bulk::rename_tag(&db, old_tag, new_tag, *dry_run, *yes, quiet)?;
                    }
                    BulkCommands::MergeTags {
                        source_tags,
                        target_tag,
                        dry_run,
                        yes,
                    } => {
                        commands::bulk::merge_tags(
                            &db,
                            source_tags,
                            target_tag,
                            *dry_run,
                            *yes,
                            quiet,
                        )?;
                    }
                    BulkCommands::CopyTags {
                        source,
                        criteria,
                        specific_tags,
                        exclude,
                        dry_run,
                        yes,
                    } => {
                        let params = SearchParams::from(criteria);
                        let specific = if specific_tags.is_empty() {
                            None
                        } else {
                            Some(specific_tags.as_slice())
                        };

                        commands::bulk::copy_tags(
                            &db, source, &params, specific, exclude, *dry_run, *yes, quiet,
                        )?;
                    }
                }
            }
            Commands::Cleanup { .. } => {
                commands::cleanup(&db, path_format, quiet)?;
            }
            Commands::List { variant, .. } => {
                commands::list(&db, *variant, path_format, quiet)?;
            }
            Commands::Filter { command } => {
                // Filter management doesn't need database access
                commands::filter(command, quiet)?;
            }
            Commands::Db { .. } | Commands::Config { .. } => unreachable!(),
        }
    }

    Ok(())
}
