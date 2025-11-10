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
    db::Database,
    cli::{Cli, Commands, ConfigCommands, DbCommands, ListVariant, SearchParams, execute_command_on_files},
    config,
    search,
    TagrError,
};
use std::io::{self, Write};
use std::path::PathBuf;

type Result<T> = std::result::Result<T, TagrError>;

/// Prompt user for yes/no confirmation
///
/// # Arguments
/// * `prompt` - Question to ask the user
/// * `quiet` - If true, auto-confirms without prompting
///
/// # Returns
/// * `Ok(true)` if user confirmed (y/yes) or quiet mode
/// * `Ok(false)` if user declined (n/no)
///
/// # Errors
/// Returns `TagrError` if I/O operations fail.
fn confirm(prompt: &str, quiet: bool) -> Result<bool> {
    if quiet {
        return Ok(true);
    }
    
    print!("{prompt} [y/n]: ");
    io::stdout().flush()?;
    
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let response = input.trim().to_lowercase();
    
    Ok(matches!(response.as_str(), "y" | "yes"))
}

/// Action to take when processing files during cleanup
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CleanupAction {
    Delete,
    DeleteAll,
    Skip,
    SkipAll,
}

/// Parse user response for cleanup file processing
///
/// # Arguments
/// * `response` - User's input string
///
/// # Returns
/// The corresponding `CleanupAction`
fn parse_cleanup_response(response: &str) -> CleanupAction {
    match response {
        "y" | "yes" => CleanupAction::Delete,
        "a" | "yes-to-all" => CleanupAction::DeleteAll,
        "q" | "no-to-all" => CleanupAction::SkipAll,
        _ => CleanupAction::Skip,
    }
}

/// Process a list of files for cleanup, prompting for each file
///
/// # Arguments
/// * `db` - Database instance
/// * `files` - List of files to process
/// * `description` - Description of why these files are being cleaned (e.g., "File not found")
/// * `quiet` - If true, suppress prompts and auto-delete
///
/// # Returns
/// Tuple of (`deleted_count`, `skipped_count`)
///
/// # Errors
/// Returns `TagrError` if database operations or I/O operations fail.
fn process_cleanup_files(
    db: &Database,
    files: &[PathBuf],
    description: &str,
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
                println!("Deleted: {}", file.display());
            }
            continue;
        }
        
        if skip_all {
            skipped_count += 1;
            continue;
        }
        
        if !quiet {
            println!("{description}: {}", file.display());
            print!("Delete from database? [y/n/a/q] (yes/no/yes-to-all/no-to-all): ");
            io::stdout().flush()?;
            
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            let response = input.trim().to_lowercase();
            
            match parse_cleanup_response(&response) {
                CleanupAction::Delete => {
                    db.remove(file)?;
                    deleted_count += 1;
                    println!("Deleted: {}", file.display());
                }
                CleanupAction::DeleteAll => {
                    delete_all = true;
                    db.remove(file)?;
                    deleted_count += 1;
                    println!("Deleted: {}", file.display());
                }
                CleanupAction::SkipAll => {
                    skip_all = true;
                    skipped_count += 1;
                    println!("Skipped: {}", file.display());
                }
                CleanupAction::Skip => {
                    skipped_count += 1;
                    println!("Skipped: {}", file.display());
                }
            }
        }
    }
    
    Ok((deleted_count, skipped_count))
}

/// Handle the browse command - interactive fuzzy finder for tags and files
///
/// Presents an interactive UI for selecting tags and files, optionally executing
/// a command on the selected files. Can be pre-populated with search parameters.
///
/// # Arguments
/// * `db` - Database instance to query
/// * `search_params` - Optional search parameters to pre-populate the browse
/// * `execute_cmd` - Optional command template to execute on selected files
/// * `quiet` - If true, suppress informational output
///
/// # Errors
///
/// Returns `TagrError` if the browse operation fails or command execution fails.
fn handle_browse_command(
    db: &Database,
    search_params: Option<SearchParams>,
    execute_cmd: Option<String>,
    quiet: bool,
) -> Result<()> {
    match search::browse_with_params(db, search_params) {
        Ok(Some(result)) => {
            if !quiet {
                println!("=== Selected Tags ===");
                for tag in &result.selected_tags {
                    println!("  - {tag}");
                }
                
                println!("\n=== Selected Files ===");
            }
            for file in &result.selected_files {
                if quiet {
                    println!("{}", file.display());
                } else {
                    println!("  - {}", file.display());
                }
            }
            
            if let Some(cmd_template) = execute_cmd {
                if !quiet {
                    println!("\n=== Executing Command ===");
                }
                execute_command_on_files(&result.selected_files, &cmd_template, quiet);
            }
        }
        Ok(None) => {
            if !quiet {
                println!("Browse cancelled.");
            }
        }
        Err(e) => return Err(e.into()),
    }
    Ok(())
}

/// Handle the tag command - add tags to a file
///
/// Associates one or more tags with a file in the database.
///
/// # Arguments
/// * `db` - Database instance to update
/// * `file` - File path to tag
/// * `tags` - Tag strings to add to the file
/// * `quiet` - If true, suppress informational output
///
/// # Errors
///
/// Returns `TagrError` if no file is provided, no tags are provided, or database operations fail.
fn handle_tag_command(db: &Database, file: Option<PathBuf>, tags: &[String], quiet: bool) -> Result<()> {
    if let Some(file_path) = file {
        if tags.is_empty() {
            return Err(TagrError::InvalidInput("No tags provided".into()));
        }
        let fullpath = file_path.canonicalize()
            .map_err(|e| TagrError::InvalidInput(format!("Cannot access path '{}': {}", file_path.display(), e)))?;
        db.add_tags(&fullpath, tags.to_vec())?;
        if !quiet {
            println!("Tagged {} with: {}", file_path.display(), tags.join(", "));
        }
    } else {
        return Err(TagrError::InvalidInput("No file provided".into()));
    }
    Ok(())
}

/// Handle the search command - find files by tag
///
/// Searches the database for files matching the specified criteria including:
/// - Multiple tags with AND/OR logic
/// - Multiple file patterns with AND/OR logic  
/// - Tag exclusions
/// - Regex matching for tags and file patterns
///
/// # Arguments
/// * `db` - Database instance to query
/// * `params` - Search parameters from CLI
/// * `quiet` - If true, suppress informational output
///
/// # Errors
///
/// Returns `TagrError` if no search criteria provided or database query fails.
fn handle_search_command(db: &Database, params: &tagr::cli::SearchParams, quiet: bool) -> Result<()> {
    use tagr::cli::SearchMode;
    
    // Handle general query mode (no -t or -f flags)
    if let Some(query) = &params.query {
        if !params.tags.is_empty() || !params.file_patterns.is_empty() {
            return Err(TagrError::InvalidInput(
                "Cannot use general query with -t or -f flags. Use either 'tagr search <query>' or 'tagr search -t <tag> -f <pattern>'.".into()
            ));
        }
        
        let files_by_tag = db.find_by_tag_regex(query)?;
        
        // Search for files by filename pattern (using glob with wildcards for substring matching)
        let all_files = db.list_all_files()?;
        let filename_pattern = format!("*{query}*");
        let files_by_name = Database::filter_by_patterns_any(all_files, &[filename_pattern])?;
        
        let mut files: std::collections::HashSet<_> = files_by_tag.into_iter().collect();
        files.extend(files_by_name);
        let mut files: Vec<_> = files.into_iter().collect();
        files.sort();
        
        if files.is_empty() {
            if !quiet {
                println!("No files found matching query '{query}' (searched tags and filenames)");
            }
        } else {
            if !quiet {
                println!("Found {} file(s) matching query '{}' (tags or filenames):", files.len(), query);
            }
            
            for file in files {
                if quiet {
                    println!("{}", file.display());
                } else if let Ok(Some(tags)) = db.get_tags(&file) {
                    if tags.is_empty() {
                        println!("  {} (no tags)", file.display());
                    } else {
                        println!("  {} [{}]", file.display(), tags.join(", "));
                    }
                } else {
                    println!("  {}", file.display());
                }
            }
        }
        
        return Ok(());
    }
    
    if params.tags.is_empty() && params.file_patterns.is_empty() {
        return Err(TagrError::InvalidInput("No search criteria provided. Use -t for tags or -f for file patterns.".into()));
    }
    
    let mut files = if params.tags.is_empty() {
        db.list_all_files()?
    } else if params.regex_tag && params.tags.len() == 1 {
        db.find_by_tag_regex(&params.tags[0])?
    } else if params.tag_mode == SearchMode::All {
        db.find_by_all_tags(&params.tags)?
    } else {
        db.find_by_any_tag(&params.tags)?
    };

    if !params.exclude_tags.is_empty() {
        let excluded: std::collections::HashSet<_> = db.find_by_any_tag(&params.exclude_tags)?
            .into_iter()
            .collect();
        files.retain(|f| !excluded.contains(f));
    }

    if !params.file_patterns.is_empty() {
        files = if params.regex_file {
            if params.file_mode == SearchMode::All {
                Database::filter_by_regex_all(files, &params.file_patterns)?
            } else {
                Database::filter_by_regex_any(files, &params.file_patterns)?
            }
        } else if params.file_mode == SearchMode::All {
            Database::filter_by_patterns_all(files, &params.file_patterns)?
        } else {
            Database::filter_by_patterns_any(files, &params.file_patterns)?
        };
    }

    if files.is_empty() {
        if !quiet {
            let criteria = if params.tags.is_empty() {
                format!("file patterns: {}", params.file_patterns.join(", "))
            } else {
                format!("tags: {}", params.tags.join(", "))
            };
            println!("No files found matching {criteria}");
        }
    } else {
        if !quiet {
            let tag_desc = if params.tags.is_empty() {
                String::new()
            } else if params.tag_mode == SearchMode::All {
                format!("ALL tags [{}]", params.tags.join(", "))
            } else {
                format!("ANY tag [{}]", params.tags.join(", "))
            };
            
            let file_desc = if params.file_patterns.is_empty() {
                String::new()
            } else if params.file_mode == SearchMode::All {
                format!("ALL patterns [{}]", params.file_patterns.join(", "))
            } else {
                format!("ANY pattern [{}]", params.file_patterns.join(", "))
            };
            
            let mut parts = Vec::new();
            if !tag_desc.is_empty() {
                parts.push(tag_desc);
            }
            if !file_desc.is_empty() {
                parts.push(file_desc);
            }
            
            println!("Found {} file(s) matching {}:", files.len(), parts.join(" and "));
        }
        
        for file in files {
            if quiet {
                println!("{}", file.display());
            } else if let Ok(Some(tags)) = db.get_tags(&file) {
                if tags.is_empty() {
                    println!("  {} (no tags)", file.display());
                } else {
                    println!("  {} [{}]", file.display(), tags.join(", "));
                }
            } else {
                println!("  {}", file.display());
            }
        }
    }
    
    Ok(())
}

/// Handle the untag command - remove tags from a file
///
/// Removes specific tags from a file or all tags if the `all` flag is set.
///
/// # Arguments
/// * `db` - Database instance to update
/// * `file` - File path to untag
/// * `tags` - Tags to remove
/// * `all` - If true, remove all tags from the file
/// * `quiet` - If true, suppress informational output
///
/// # Errors
///
/// Returns `TagrError` if no file is provided, no tags specified when required,
/// or database operations fail.
fn handle_untag_command(db: &Database, file: Option<PathBuf>, tags: &[String], all: bool, quiet: bool) -> Result<()> {
    if let Some(file_path) = file {
        let fullpath = file_path.canonicalize()
            .map_err(|e| TagrError::InvalidInput(format!("Cannot access path '{}': {}", file_path.display(), e)))?;
        if all {
            db.remove(&fullpath)?;
            if !quiet {
                println!("Removed all tags from {}", file_path.display());
            }
        } else if !tags.is_empty() {
            db.remove_tags(&fullpath, tags)?;
            if !quiet {
                println!("Removed tags {} from {}", tags.join(", "), file_path.display());
            }
        } else {
            return Err(TagrError::InvalidInput("No tags provided. Use -t to specify tags or --all to remove all tags".into()));
        }
    } else {
        return Err(TagrError::InvalidInput("No file provided".into()));
    }
    Ok(())
}

/// Handle the tags command - manage tags globally
///
/// Performs global tag management operations such as listing all tags
/// or removing a tag from all files.
///
/// # Arguments
/// * `db` - Database instance to query/update
/// * `command` - Specific tags subcommand to execute
/// * `quiet` - If true, suppress informational output
///
/// # Errors
///
/// Returns `TagrError` if database operations fail or I/O errors occur during
/// interactive prompts.
fn handle_tags_command(db: &Database, command: &tagr::cli::TagsCommands, quiet: bool) -> Result<()> {
    match command {
        tagr::cli::TagsCommands::List => {
            let tags = db.list_all_tags()?;
            
            if tags.is_empty() {
                if !quiet {
                    println!("No tags found in database.");
                }
            } else {
                if !quiet {
                    println!("Tags in database:");
                }
                for tag in tags {
                    if quiet {
                        println!("{tag}");
                    } else {
                        let files = db.find_by_tag(&tag)?;
                        println!("  {} (used by {} file(s))", tag, files.len());
                    }
                }
            }
        }
        tagr::cli::TagsCommands::Remove { tag } => {
            let files_before = db.find_by_tag(tag)?;
            
            if files_before.is_empty() {
                if !quiet {
                    println!("Tag '{tag}' not found in database.");
                }
                return Ok(());
            }
            
            if !quiet {
                println!("Found tag '{tag}' in {} file(s):", files_before.len());
                for file in &files_before {
                    println!("  - {}", file.display());
                }
                println!();
            }
            
            if !confirm("Remove tag from all files?", quiet)? {
                if !quiet {
                    println!("Cancelled.");
                }
                return Ok(());
            }
            
            let files_removed = db.remove_tag_globally(tag)?;
            
            if !quiet {
                println!("Removed tag '{tag}' from {} file(s).", files_before.len());
                if files_removed > 0 {
                    println!("Cleaned up {files_removed} file(s) with no remaining tags.");
                }
            }
        }
    }
    Ok(())
}

/// Handle the list command - list files or tags
///
/// Lists all files or all tags in the database based on the variant.
///
/// # Arguments
/// * `db` - Database instance to query
/// * `variant` - Whether to list files or tags
/// * `quiet` - If true, suppress informational output
///
/// # Errors
///
/// Returns `TagrError` if database operations fail.
fn handle_list_command(db: &Database, variant: ListVariant, quiet: bool) -> Result<()> {
    match variant {
        ListVariant::Files => {
            let all_pairs = db.list_all()?;
            
            if all_pairs.is_empty() {
                if !quiet {
                    println!("No files found in database.");
                }
            } else {
                if !quiet {
                    println!("Files in database:");
                }
                for pair in all_pairs {
                    if quiet {
                        println!("{}", pair.file.display());
                    } else if pair.tags.is_empty() {
                        println!("  {} (no tags)", pair.file.display());
                    } else {
                        println!("  {} [{}]", pair.file.display(), pair.tags.join(", "));
                    }
                }
            }
        }
        ListVariant::Tags => {
            let tags = db.list_all_tags()?;
            
            if tags.is_empty() {
                if !quiet {
                    println!("No tags found in database.");
                }
            } else {
                if !quiet {
                    println!("Tags in database:");
                }
                for tag in tags {
                    if quiet {
                        println!("{tag}");
                    } else {
                        let files = db.find_by_tag(&tag)?;
                        println!("  {} (used by {} file(s))", tag, files.len());
                    }
                }
            }
        }
    }
    Ok(())
}

/// Handle the cleanup command - remove missing files and untagged files
///
/// Scans the database for files that no longer exist on the filesystem or have
/// no tags, and optionally removes them from the database.
///
/// # Arguments
/// * `db` - Database instance to clean up
/// * `quiet` - If true, suppress informational output and prompts
///
/// # Errors
///
/// Returns `TagrError` if database operations fail or I/O errors occur during
/// interactive prompts.
fn handle_cleanup_command(db: &Database, quiet: bool) -> Result<()> {
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
                println!("  - {}", file.display());
            }
            println!();
        }
        
        let (deleted, skipped) = process_cleanup_files(
            db,
            &missing_files,
            "File not found",
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
                println!("  - {}", file.display());
            }
            println!();
        }
        
        let (deleted, skipped) = process_cleanup_files(
            db,
            &untagged_files,
            "File has no tags",
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

/// Handle the db command - manage multiple databases
///
/// Performs database management operations including adding, removing, listing,
/// and setting the default database.
///
/// # Arguments
/// * `config` - Application configuration
/// * `command` - Specific database subcommand to execute
/// * `quiet` - If true, suppress informational output
///
/// # Errors
///
/// Returns `TagrError` if the database operation is invalid (e.g., duplicate name,
/// non-existent database), configuration save fails, or filesystem operations fail.
#[allow(clippy::too_many_lines)]
fn handle_db_command(mut config: config::TagrConfig, command: &DbCommands, quiet: bool) -> Result<()> {
    match command {
        DbCommands::Add { name, path } => {
            if config.get_database(name).is_some() {
                if !quiet {
                    eprintln!("Error: Database '{name}' already exists");
                }
                return Err(TagrError::InvalidInput(format!("Database '{name}' already exists")));
            }
            
            let resolved_path = if path.components().count() == 1 {
                let data_dir = dirs::data_local_dir()
                    .ok_or_else(|| TagrError::InvalidInput(
                        "Could not determine data directory".into()
                    ))?;
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
                return Err(TagrError::InvalidInput(format!("Database '{name}' does not exist")));
            }
            
            let is_default = config.get_default_database() == Some(name);
            if is_default && !quiet {
                println!("Warning: Removing the default database. You'll need to set a new default.");
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
                        println!("Database files at {} do not exist (already deleted)", path.display());
                    }
                } else if !quiet {
                    println!("Note: Database files at {} were NOT deleted", path.display());
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
                return Err(TagrError::InvalidInput(format!("Database '{name}' does not exist")));
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
fn handle_config_command(mut config: config::TagrConfig, command: &ConfigCommands, quiet: bool) -> Result<()> {
    match command {
        ConfigCommands::Set { setting } => {
            let parts: Vec<&str> = setting.splitn(2, '=').collect();
            if parts.len() != 2 {
                return Err(TagrError::InvalidInput(
                    "Invalid format. Use: tagr config set key=value".into()
                ));
            }
            
            let key = parts[0].trim();
            let value = parts[1].trim();
            
            match key {
                "quiet" => {
                    let new_value = value.parse::<bool>().map_err(|_| {
                        TagrError::InvalidInput(
                            format!("Invalid value for quiet: '{value}'. Use 'true' or 'false'")
                        )
                    })?;
                    config.quiet = new_value;
                    config.save()?;
                    if !quiet {
                        println!("Set quiet = {new_value}");
                    }
                }
                _ => {
                    return Err(TagrError::InvalidInput(
                        format!("Unknown configuration key: '{key}'. Available keys: quiet")
                    ));
                }
            }
        }
        ConfigCommands::Get { key } => {
            match key.as_str() {
                "quiet" => {
                    println!("{}", config.quiet);
                }
                _ => {
                    return Err(TagrError::InvalidInput(
                        format!("Unknown configuration key: '{key}'. Available keys: quiet")
                    ));
                }
            }
        }
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
        
        let db_path = config.get_database(&db_name)
            .ok_or_else(|| TagrError::InvalidInput(
                format!("Database '{db_name}' not found in configuration")
            ))?;
        
        let db = Database::open(db_path)?;
        
        match &command {
            Commands::Browse { .. } => {
                let search_params = command.get_search_params_from_browse();
                let execute_cmd = command.get_execute_from_browse();
                handle_browse_command(&db, search_params, execute_cmd, quiet)?;
            }
            Commands::Tag { .. } => {
                let file = command.get_file_from_tag();
                let tags = command.get_tags_from_tag();
                handle_tag_command(&db, file, tags, quiet)?;
            }
            Commands::Search { .. } => {
                let params = command.get_search_params()
                    .ok_or_else(|| TagrError::InvalidInput("Failed to parse search parameters".into()))?;
                handle_search_command(&db, &params, quiet)?;
            }
            Commands::Untag { .. } => {
                let file = command.get_file_from_untag();
                let tags = command.get_tags_from_untag();
                let all = command.get_all_from_untag();
                handle_untag_command(&db, file, tags, all, quiet)?;
            }
            Commands::Tags { command } => {
                handle_tags_command(&db, command, quiet)?;
            }
            Commands::Cleanup { .. } => {
                handle_cleanup_command(&db, quiet)?;
            }
            Commands::List { variant, .. } => {
                handle_list_command(&db, *variant, quiet)?;
            }
            Commands::Db { .. } | Commands::Config { .. } => unreachable!(),
        }
    }
    
    Ok(())
}
