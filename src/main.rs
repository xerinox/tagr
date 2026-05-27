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

use clap::CommandFactory;
use tagr::{
    TagrError,
    cli::{Cli, Commands, ConfigCommands, DbCommands},
    commands, config,
    db::Database,
};

type Result<T> = std::result::Result<T, TagrError>;

fn required_arg(name: &'static str) -> TagrError {
    TagrError::InvalidInput(format!("Missing required argument '{name}'"))
}

/// Handle the db command - manage multiple databases
fn handle_db_command(
    config: config::TagrConfig,
    command: &DbCommands,
    quiet: bool,
) -> Result<()> {
    match command {
        DbCommands::Add { name, path } => handle_db_add(config, name, path, quiet),
        DbCommands::List => handle_db_list(&config, quiet),
        DbCommands::Remove { name, delete_files } => handle_db_remove(config, name, *delete_files, quiet),
        DbCommands::SetDefault { name } => handle_db_set_default(config, name, quiet),
    }
}

fn handle_db_add(
    mut config: config::TagrConfig,
    name: &str,
    path: &std::path::Path,
    quiet: bool,
) -> Result<()> {
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
        path.to_path_buf()
    };

    config.add_database(name.to_string(), resolved_path.clone())?;

    if !resolved_path.exists() {
        std::fs::create_dir_all(&resolved_path)?;
    }

    if !quiet {
        println!("Database '{name}' added at {}", resolved_path.display());
    }

    if config.databases.len() == 1 {
        config.set_default_database(name.to_string())?;
        if !quiet {
            println!("Set '{name}' as default database");
        }
    }

    #[cfg(feature = "dynamic-completions")]
    tagr::completions::invalidate_database_cache();
    Ok(())
}

fn handle_db_list(config: &config::TagrConfig, quiet: bool) -> Result<()> {
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
    Ok(())
}

fn handle_db_remove(
    mut config: config::TagrConfig,
    name: &str,
    delete_files: bool,
    quiet: bool,
) -> Result<()> {
    if config.get_database(name).is_none() {
        if !quiet {
            eprintln!("Error: Database '{name}' does not exist");
        }
        return Err(TagrError::InvalidInput(format!(
            "Database '{name}' does not exist"
        )));
    }

    let is_default = config.get_default_database().map(String::as_str) == Some(name);
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

        if delete_files {
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

    #[cfg(feature = "dynamic-completions")]
    tagr::completions::invalidate_database_cache();
    Ok(())
}

fn handle_db_set_default(
    mut config: config::TagrConfig,
    name: &str,
    quiet: bool,
) -> Result<()> {
    if config.get_database(name).is_none() {
        if !quiet {
            eprintln!("Error: Database '{name}' does not exist");
        }
        return Err(TagrError::InvalidInput(format!(
            "Database '{name}' does not exist"
        )));
    }

    config.set_default_database(name.to_string())?;

    if !quiet {
        println!("Set '{name}' as default database");
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
#[allow(clippy::too_many_lines)]
fn main() -> Result<()> {
    // Handle shell completion before anything else (when feature is enabled)
    #[cfg(feature = "dynamic-completions")]
    tagr::completions::init_dynamic_completions(Cli::command);

    let config = config::TagrConfig::load_or_setup()?;

    let cli = Cli::parse_args();

    let quiet = cli.quiet || config.quiet;

    let command = cli.get_command();

    // Handle completions generation command (no database needed)
    if let Commands::Completions { shell } = &command {
        let mut cmd = Cli::command();
        tagr::completions::generate_static(*shell, &mut cmd, &mut std::io::stdout());
        return Ok(());
    }

    if let Commands::Db { command } = &command {
        handle_db_command(config, command, quiet)?;
    } else if let Commands::Config { command } = &command {
        handle_config_command(config, command, quiet)?;
    } else if let Commands::Watch { command: watch_cmd } = &command {
        use commands::watch::{WatchCommands, WatchStartArgs};

        // Internal daemon bootstrap: `tagr watch start --daemon [--daemonize]`
        if let WatchCommands::Start(WatchStartArgs { daemon: true, daemonize }) = watch_cmd {
            #[cfg(unix)]
            if *daemonize {
                tagr::daemon::fallback::daemonize_self()
                    .map_err(|e| TagrError::InvalidInput(e.to_string()))?;
            }

            let db_name = command.get_db()
                .or_else(|| config.get_default_database().cloned())
                .ok_or_else(|| TagrError::InvalidInput(
                    "Daemon requires a default database. Set one with: tagr db add <name> <path>".into(),
                ))?;
            let db_path = config.get_database(&db_name).ok_or_else(|| {
                TagrError::InvalidInput(format!("Database '{db_name}' not found in configuration"))
            })?;
            let db = Database::open(db_path)?;
            tagr::daemon::core::run(&db)
                .map_err(|e| TagrError::InvalidInput(e.to_string()))?;
        } else {
            let mut stdout = std::io::stdout();
            match watch_cmd {
                WatchCommands::Add(add_args) => {
                    commands::watch::watch_add(add_args, &config, quiet, &mut stdout)
                        .map_err(|e| TagrError::InvalidInput(e.to_string()))?;
                }
                WatchCommands::Remove { index } => {
                    commands::watch::watch_remove(*index, quiet, &mut stdout)
                        .map_err(|e| TagrError::InvalidInput(e.to_string()))?;
                }
                WatchCommands::List => {
                    commands::watch::watch_list(&mut stdout)
                        .map_err(|e| TagrError::InvalidInput(e.to_string()))?;
                }
                WatchCommands::Status => {
                    commands::watch::watch_status(&mut stdout)
                        .map_err(|e| TagrError::InvalidInput(e.to_string()))?;
                }
                WatchCommands::Start(_) => {
                    commands::watch::watch_start(&config, quiet, &mut stdout)
                        .map_err(|e| TagrError::InvalidInput(e.to_string()))?;
                }
                WatchCommands::Stop => {
                    commands::watch::watch_stop(quiet, &mut stdout)
                        .map_err(|e| TagrError::InvalidInput(e.to_string()))?;
                }
            }
        }
    } else {
        let db_name = command
            .get_db()
            .or_else(|| config.get_default_database().map(ToOwned::to_owned))
            .ok_or_else(|| TagrError::InvalidInput(
            "No default database set. Use 'tagr db add <name> <path>' to create one, or specify --db <name>.".into()
        ))?;

        let db_path = config.get_database(&db_name).ok_or_else(|| {
            TagrError::InvalidInput(format!("Database '{db_name}' not found in configuration"))
        })?;

        // Determine path format: CLI override > config default
        let path_format = if let Some(cli_format) = cli.get_path_format() {
            match cli_format {
                tagr::cli::PathFormat::Absolute => config::PathFormat::Absolute,
                tagr::cli::PathFormat::Relative => config::PathFormat::Relative,
            }
        } else {
            config.path_format
        };

        // Try to open the DB directly first — this is the fast path and works
        // even when the daemon is running (sled allows a second reader only if
        // the lock is available).  If the DB lock is held (daemon has it open),
        // the open will fail with an EWOULDBLOCK-style error; in that case we
        // fall back to forwarding the command over IPC.
        match Database::open(db_path) {
            Ok(db) => {
                let mut stdout = std::io::stdout();
                commands::dispatch_command(&command, &db, &config, path_format, quiet, &mut stdout)?;
            }
            Err(lock_err) if is_db_lock_error(&lock_err) => {
                // DB is locked — forward to daemon if it is reachable.
                let rt = tokio::runtime::Runtime::new().map_err(TagrError::IoError)?;
                let daemon_running = rt.block_on(async {
                    use tagr::daemon::DaemonManager;
                    tagr::daemon::PlatformDaemonManager.is_running().await
                }).unwrap_or(false);

                if !daemon_running {
                    return Err(TagrError::InvalidInput(
                        "Database is locked and the daemon is not responding. \
                         Try `tagr watch --stop` then retry.".into(),
                    ));
                }

                // Forward command to daemon via typed IPC and render locally.
                dispatch_via_ipc(&rt, &command, path_format, quiet)?;
            }
            Err(other) => return Err(other.into()),
        }
    }

    Ok(())
}

/// Returns `true` when a [`DbError`] is caused by the sled advisory lock being
/// held by another process (i.e. the daemon has the database open).
fn is_db_lock_error(err: &tagr::db::DbError) -> bool {
    err.to_string().contains("could not acquire lock")
}

/// Forward a command to the daemon via typed IPC and render the response locally.
fn dispatch_via_ipc(
    rt: &tokio::runtime::Runtime,
    command: &tagr::cli::Commands,
    path_format: config::PathFormat,
    quiet: bool,
) -> Result<()> {
    use std::path::PathBuf;
    use tagr::cli::Commands;
    use tagr::daemon::client::send_request;
    use tagr::ipc::wire::{Request, Response, WireSearchParams};
    use tagr::output;

    let req = match command {
        Commands::Search { .. } => {
            let params = command.get_search_params().unwrap_or_default();
            Request::SearchFiles { params: WireSearchParams::from(params) }
        }
        Commands::List { variant, .. } => {
            match variant {
                tagr::cli::ListVariant::Tags => Request::ListTags,
                tagr::cli::ListVariant::Files => Request::ListFiles,
            }
        }
        Commands::Tag { .. } => {
            let ctx = command.get_tag_context().ok_or_else(|| {
                TagrError::InvalidInput("Failed to extract tag context from command".into())
            })?;
            let file = ctx.file.ok_or_else(|| {
                TagrError::InvalidInput("No file specified".into())
            })?;
            Request::AddTags {
                file: file.to_string_lossy().into_owned(),
                tags: ctx.tags,
            }
        }
        Commands::Untag { .. } => {
            let ctx = command.get_untag_context().ok_or_else(|| {
                TagrError::InvalidInput("Failed to extract untag context from command".into())
            })?;
            let file = ctx.file.ok_or_else(|| {
                TagrError::InvalidInput("No file specified".into())
            })?;
            let tags: Vec<String> = ctx.tags.to_vec();
            Request::RemoveTags {
                file: file.to_string_lossy().into_owned(),
                tags,
                all: ctx.all,
            }
        }
        Commands::Cleanup { .. } => Request::Cleanup,
        Commands::Browse { filter_args, .. } => {
            let ctx = command.get_browse_context().ok_or_else(|| {
                TagrError::InvalidInput("Failed to extract browse context from command".into())
            })?;

            let (ds, event_rx) = tagr::datasource::DataSource::remote()
                .map_err(|e| TagrError::InvalidInput(format!("Failed to create remote DataSource: {e}")))?;

            let save_filter = filter_args
                .save_filter
                .as_ref()
                .map(|name| (name.as_str(), filter_args.filter_desc.as_deref()));

            return commands::browse::execute(
                ds,
                Some(event_rx),
                ctx.search_params,
                filter_args.filter.as_deref(),
                save_filter,
                ctx.execute_cmd,
                Some(&ctx.preview_overrides),
                path_format,
                quiet,
            );
        }
        _ => {
            return Err(TagrError::InvalidInput(
                "This command is not supported while the daemon is running".into(),
            ));
        }
    };

    let resp = rt
        .block_on(send_request(req))
        .map_err(|e| TagrError::IoError(std::io::Error::other(e.to_string())))?;

    match resp {
        Response::Pong => {
            if !quiet { println!("pong"); }
        }
        Response::Ok => {
            // Mutation succeeded, nothing to print
        }
        Response::Tags(tags) => {
            for tag in &tags {
                if quiet {
                    println!("{}", tag.name);
                } else {
                    println!("{} ({})", tag.name, tag.file_count);
                }
            }
        }
        Response::Files(pairs) => {
            for pair in &pairs {
                let path = PathBuf::from(&pair.file);
                let formatted = output::format_path(&path, path_format);
                if quiet {
                    println!("{formatted}");
                } else {
                    println!("{formatted}\t[{}]", pair.tags.join(", "));
                }
            }
        }
        Response::FileTags(tags) => {
            for tag in &tags {
                println!("{tag}");
            }
        }
        Response::FilePaths(paths) => {
            for path_str in &paths {
                let path = PathBuf::from(path_str);
                let formatted = output::format_path(&path, path_format);
                println!("{formatted}");
            }
        }
        Response::Notes(entries) => {
            for entry in &entries {
                let path = PathBuf::from(&entry.path);
                let formatted = output::format_path(&path, path_format);
                if quiet {
                    println!("{formatted}");
                } else {
                    println!("{formatted}\t{}", entry.content.lines().next().unwrap_or(""));
                }
            }
        }
        Response::Note(note) => {
            if let Some(entry) = note {
                let path = PathBuf::from(&entry.path);
                let formatted = output::format_path(&path, path_format);
                if quiet {
                    println!("{}", entry.content);
                } else {
                    println!("{formatted}\t{}", entry.content);
                }
            }
        }
        Response::CleanupResult { removed } => {
            if !quiet {
                println!("Removed {removed} stale entries");
            }
        }
        Response::Error(err) => {
            return Err(TagrError::InvalidInput(err));
        }
    }

    Ok(())
}

