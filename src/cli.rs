//! Command-line interface definitions and parsing
//!
//! This module defines the complete CLI structure for tagr using the `clap` crate.
//! It provides command parsing, argument validation, and helper methods for extracting
//! command-specific data.
//!
//! # Commands
//!
//! - **browse**: Interactive fuzzy finder for tags and files (default)
//! - **tag**: Add tags to files
//! - **search**: Find files by tag
//! - **db**: Manage multiple databases (add, remove, list, set-default)
//!
//! # Design Features
//!
//! - Supports both flag-based (`-f file -t tag1`) and positional (`file tag1`) syntax
//! - Global `--quiet` flag for scripting-friendly output
//! - Command aliases (e.g., `b` for `browse`, `t` for `tag`)
//! - Multi-select support in browse mode
//! - Command execution on selected files with `{}` placeholder
//!
//! # Examples
//!
//! ```
//! use tagr::cli::{Cli, Commands};
//!
//! // Parse command line arguments
//! let cli = Cli::parse_args();
//! let command = cli.get_command();
//!
//! // Extract command-specific data using context
//! match command {
//!     Commands::Tag { .. } => {
//!         if let Some(ctx) = command.get_tag_context() {
//!             let file = ctx.file;
//!             let tags = ctx.tags;
//!         }
//!     }
//!     _ => {}
//! }
//! ```

use clap::{Parser, Subcommand, ValueEnum};
use std::path::{Path, PathBuf};

/// Path display format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathFormat {
    /// Display absolute paths
    Absolute,
    /// Display relative paths (relative to current directory)
    Relative,
}

/// List variant for the list command
#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListVariant {
    /// List all files in the database
    Files,
    /// List all tags in the database
    Tags,
}

/// Search mode for combining multiple criteria
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    /// Match ANY of the criteria (OR logic)
    Any,
    /// Match ALL of the criteria (AND logic)
    All,
}

/// Parameters for search command
#[derive(Debug, Clone)]
pub struct SearchParams {
    /// General query (for combined filename and tag search)
    pub query: Option<String>,
    /// Tags to search for
    pub tags: Vec<String>,
    /// How to combine multiple tags (AND/OR)
    pub tag_mode: SearchMode,
    /// File patterns to filter by
    pub file_patterns: Vec<String>,
    /// How to combine multiple file patterns (AND/OR)
    pub file_mode: SearchMode,
    /// Tags to exclude
    pub exclude_tags: Vec<String>,
    /// Use regex for tag matching
    pub regex_tag: bool,
    /// Use regex for file pattern matching
    pub regex_file: bool,
    /// Virtual tags to filter by
    pub virtual_tags: Vec<String>,
    /// How to combine multiple virtual tags (AND/OR)
    pub virtual_mode: SearchMode,
}

/// Preview configuration overrides from CLI
#[derive(Debug, Clone)]
pub struct PreviewOverrides {
    /// Disable preview
    pub no_preview: bool,
    /// Maximum preview lines
    pub preview_lines: Option<usize>,
    /// Preview position
    pub preview_position: Option<String>,
    /// Preview width percentage
    pub preview_width: Option<u8>,
}

// ============================================================================
// Command Context Types
// ============================================================================

/// Context for tag command execution
#[derive(Debug, Clone)]
pub struct TagContext {
    /// File to tag
    pub file: Option<PathBuf>,
    /// Tags to add
    pub tags: Vec<String>,
}

/// Context for untag command execution
#[derive(Debug, Clone)]
pub struct UntagContext {
    /// File to untag
    pub file: Option<PathBuf>,
    /// Tags to remove (empty if removing all)
    pub tags: Vec<String>,
    /// Remove all tags from file
    pub all: bool,
}

/// Context for browse command execution
#[derive(Debug, Clone)]
pub struct BrowseContext {
    /// Initial search parameters
    pub search_params: Option<SearchParams>,
    /// Command to execute on selected files
    pub execute_cmd: Option<String>,
    /// Preview configuration overrides
    pub preview_overrides: PreviewOverrides,
}

impl SearchParams {
    /// Merge with another `SearchParams` (typically from a loaded filter)
    ///
    /// This extends the current params with additional criteria:
    /// - Tags and file patterns are combined
    /// - Exclusions are merged
    /// - Regex flags are OR'd
    /// - Modes are preserved from self (CLI takes precedence)
    pub fn merge(&mut self, other: &Self) {
        for tag in &other.tags {
            if !self.tags.contains(tag) {
                self.tags.push(tag.clone());
            }
        }

        for pattern in &other.file_patterns {
            if !self.file_patterns.contains(pattern) {
                self.file_patterns.push(pattern.clone());
            }
        }

        for exclude in &other.exclude_tags {
            if !self.exclude_tags.contains(exclude) {
                self.exclude_tags.push(exclude.clone());
            }
        }

        self.regex_tag = self.regex_tag || other.regex_tag;
        self.regex_file = self.regex_file || other.regex_file;
    }
}

impl From<SearchParams> for crate::filters::FilterCriteria {
    /// Convert `SearchParams` to `FilterCriteria` for saving as a filter
    ///
    /// Note: The general query is not preserved in `FilterCriteria` since
    /// filters use explicit tags and file patterns only.
    fn from(params: SearchParams) -> Self {
        Self {
            tags: params.tags,
            tag_mode: params.tag_mode.into(),
            file_patterns: params.file_patterns,
            file_mode: params.file_mode.into(),
            excludes: params.exclude_tags,
            regex_tag: params.regex_tag,
            regex_file: params.regex_file,
            virtual_tags: params.virtual_tags,
            virtual_mode: params.virtual_mode.into(),
        }
    }
}

impl From<&SearchParams> for crate::filters::FilterCriteria {
    fn from(params: &SearchParams) -> Self {
        Self {
            tags: params.tags.clone(),
            tag_mode: params.tag_mode.into(),
            file_patterns: params.file_patterns.clone(),
            file_mode: params.file_mode.into(),
            excludes: params.exclude_tags.clone(),
            regex_tag: params.regex_tag,
            regex_file: params.regex_file,
            virtual_tags: params.virtual_tags.clone(),
            virtual_mode: params.virtual_mode.into(),
        }
    }
}

impl From<&crate::filters::FilterCriteria> for SearchParams {
    fn from(criteria: &crate::filters::FilterCriteria) -> Self {
        Self {
            query: None,
            tags: criteria.tags.clone(),
            tag_mode: criteria.tag_mode.into(),
            file_patterns: criteria.file_patterns.clone(),
            file_mode: criteria.file_mode.into(),
            exclude_tags: criteria.excludes.clone(),
            regex_tag: criteria.regex_tag,
            regex_file: criteria.regex_file,
            virtual_tags: criteria.virtual_tags.clone(),
            virtual_mode: criteria.virtual_mode.into(),
        }
    }
}

/// Execute a command template for each file in the list
///
/// Runs a shell command for each file, replacing the `{}` placeholder in the
/// command template with the file path.
///
/// # Arguments
/// * `files` - List of files to process
/// * `cmd_template` - Command template with `{}` as placeholder for file path
/// * `quiet` - If true, suppress "Running:" messages
///
/// # Returns
/// Number of successful executions
///
/// # Panics
///
/// This function does not panic. Command execution failures are logged to stderr
/// and the function continues processing remaining files.
///
/// # Examples
///
/// ```no_run
/// use tagr::cli::execute_command_on_files;
/// use std::path::PathBuf;
///
/// let files = vec![PathBuf::from("file1.txt"), PathBuf::from("file2.txt")];
/// let count = execute_command_on_files(&files, "cat {}", false);
/// println!("Successfully executed command on {} files", count);
/// ```
pub fn execute_command_on_files<P: AsRef<Path>>(
    files: &[P],
    cmd_template: &str,
    quiet: bool,
) -> usize {
    let mut success_count = 0;

    for file in files {
        let file_str = file.as_ref().to_string_lossy();
        let cmd = cmd_template.replace("{}", &file_str);

        if !quiet {
            println!("Running: {cmd}");
        }
        match std::process::Command::new("sh")
            .arg("-c")
            .arg(&cmd)
            .status()
        {
            Ok(exit_status) => {
                if exit_status.success() {
                    success_count += 1;
                } else {
                    eprintln!("Command failed with status: {exit_status}");
                }
            }
            Err(e) => {
                eprintln!("Failed to execute command: {e}");
            }
        }
    }

    success_count
}

/// Configuration management subcommands
#[derive(Subcommand, Debug, Clone)]
pub enum ConfigCommands {
    /// Set a configuration value
    Set {
        /// Configuration key=value (e.g., quiet=true)
        #[arg(value_name = "KEY=VALUE")]
        setting: String,
    },

    /// Get a configuration value
    Get {
        /// Configuration key to retrieve (e.g., quiet)
        #[arg(value_name = "KEY")]
        key: String,
    },
}

/// Tag management subcommands
#[derive(Subcommand, Debug, Clone)]
pub enum TagsCommands {
    /// List all tags in the database
    List,

    /// Remove a tag from all files (cleans up files with no remaining tags)
    #[command(visible_alias = "rm")]
    Remove {
        /// Tag to remove from all files
        tag: String,
    },
}

/// Database management subcommands
#[derive(Subcommand, Debug, Clone)]
pub enum DbCommands {
    /// Add a new database
    Add {
        /// Name of the database
        name: String,

        /// Path to the database directory
        path: PathBuf,
    },

    /// List all databases
    List,

    /// Remove a database from configuration
    #[command(visible_alias = "rm")]
    Remove {
        /// Name of the database to remove
        name: String,

        /// Also delete database files from disk
        #[arg(short = 'd', long = "delete-files")]
        delete_files: bool,
    },

    /// Set the default database
    #[command(name = "set-default")]
    SetDefault {
        /// Name of the database to set as default
        name: String,
    },
}

/// Bulk operation subcommands
#[derive(Subcommand, Debug, Clone)]
pub enum BulkCommands {
    /// Add tags to multiple files matching search criteria
    Tag {
        #[command(flatten)]
        criteria: SearchCriteriaArgs,

        /// Tags to add to matching files
        #[arg(value_name = "TAG", required = true)]
        add_tags: Vec<String>,

        /// Preview changes without applying them
        #[arg(short = 'n', long = "dry-run")]
        dry_run: bool,

        /// Skip confirmation prompt
        #[arg(short = 'y', long = "yes")]
        yes: bool,
    },

    /// Remove tags from multiple files matching search criteria
    Untag {
        #[command(flatten)]
        criteria: SearchCriteriaArgs,

        /// Tags to remove (omit with --all to remove all tags)
        #[arg(value_name = "TAG")]
        remove_tags: Vec<String>,

        /// Remove all tags from matching files
        #[arg(short = 'a', long = "all")]
        all: bool,

        /// Preview changes without applying them
        #[arg(short = 'n', long = "dry-run")]
        dry_run: bool,

        /// Skip confirmation prompt
        #[arg(short = 'y', long = "yes")]
        yes: bool,
    },

    /// Rename a tag globally across all files in the database
    #[command(visible_alias = "rename")]
    RenameTag {
        /// Current tag name
        old_tag: String,

        /// New tag name
        new_tag: String,

        /// Preview changes without applying them
        #[arg(short = 'n', long = "dry-run")]
        dry_run: bool,

        /// Skip confirmation prompt
        #[arg(short = 'y', long = "yes")]
        yes: bool,
    },
}

/// Filter management subcommands
#[derive(Subcommand, Debug, Clone)]
pub enum FilterCommands {
    /// List all saved filters
    #[command(visible_alias = "ls")]
    List,

    /// Show detailed information about a filter
    Show {
        /// Name of the filter to show
        name: String,
    },

    /// Create a new filter
    Create {
        /// Name of the filter
        name: String,

        /// Description of the filter
        #[arg(short = 'd', long = "description")]
        description: Option<String>,

        #[command(flatten)]
        criteria: SearchCriteriaArgs,
    },
    /// Delete a filter
    #[command(visible_alias = "rm")]
    Delete {
        /// Name of the filter to delete
        name: String,

        /// Skip confirmation prompt
        #[arg(short = 'f', long = "force")]
        force: bool,
    },

    /// Rename a filter
    #[command(visible_alias = "mv")]
    Rename {
        /// Current name of the filter
        old_name: String,

        /// New name for the filter
        new_name: String,
    },

    /// Export filters to a file
    Export {
        /// Names of specific filters to export (exports all if not specified)
        #[arg(value_name = "FILTER")]
        filters: Vec<String>,

        /// Output file path (prints to stdout if not specified)
        #[arg(short = 'o', long = "output")]
        output: Option<PathBuf>,
    },

    /// Import filters from a file
    Import {
        /// Path to the file to import from
        path: PathBuf,

        /// Overwrite existing filters with the same name
        #[arg(long = "overwrite", conflicts_with = "skip_existing")]
        overwrite: bool,

        /// Skip filters that already exist
        #[arg(long = "skip-existing", conflicts_with = "overwrite")]
        skip_existing: bool,
    },

    /// Show filter usage statistics
    Stats,
}

/// Shared arguments for commands that work with a database
#[derive(Parser, Debug, Clone)]
pub struct DbArgs {
    /// Database name to use (overrides default)
    #[arg(long = "db", value_name = "NAME")]
    pub db: Option<String>,
}

/// Shared search criteria arguments (tags, file patterns, virtual tags)
#[derive(Parser, Debug, Clone)]
pub struct SearchCriteriaArgs {
    /// Tags to search for
    #[arg(short = 't', long = "tag", value_name = "TAG", num_args = 0..)]
    pub tags: Vec<String>,

    /// Match files with ANY of the specified tags (OR logic, default is AND)
    #[arg(long = "any-tag", conflicts_with = "all_tags")]
    pub any_tag: bool,

    /// Match files with ALL of the specified tags (AND logic, explicit)
    #[arg(long = "all-tags", conflicts_with = "any_tag")]
    pub all_tags: bool,

    /// File path patterns to filter results (glob syntax: *.rs, src/**/*)  
    #[arg(short = 'f', long = "file", value_name = "PATTERN", num_args = 0..)]
    pub file_patterns: Vec<String>,

    /// Match files with ANY of the file patterns (OR logic, default is AND)
    #[arg(long = "any-file", conflicts_with = "all_files")]
    pub any_file: bool,

    /// Match files with ALL of the file patterns (AND logic, explicit)
    #[arg(long = "all-files", conflicts_with = "any_file")]
    pub all_files: bool,

    /// Exclude files with these tags
    #[arg(short = 'e', long = "exclude", value_name = "TAG", num_args = 0..)]
    pub excludes: Vec<String>,

    /// Use regex matching for tags
    #[arg(short = 'r', long = "regex-tag")]
    pub regex_tag: bool,

    /// Use regex matching for file patterns
    #[arg(long = "regex-file")]
    pub regex_file: bool,

    /// Virtual tags to filter by (e.g., "size:>1MB", "modified:today")
    #[arg(short = 'v', long = "virtual-tag", value_name = "VTAG", num_args = 0..)]
    pub virtual_tags: Vec<String>,

    /// Match files with ANY of the virtual tags (OR logic, default is AND)
    #[arg(long = "any-virtual", conflicts_with = "all_virtual")]
    pub any_virtual: bool,

    /// Match files with ALL of the virtual tags (AND logic, explicit)
    #[arg(long = "all-virtual", conflicts_with = "any_virtual")]
    pub all_virtual: bool,
}

/// Shared arguments for filter operations
#[derive(Parser, Debug, Clone)]
pub struct FilterArgs {
    /// Load a saved filter
    #[arg(short = 'F', long = "filter", value_name = "NAME")]
    pub filter: Option<String>,

    /// Save current search as a filter
    #[arg(long = "save-filter", value_name = "NAME")]
    pub save_filter: Option<String>,

    /// Description for saved filter
    #[arg(long = "filter-desc", value_name = "DESC", requires = "save_filter")]
    pub filter_desc: Option<String>,
}

/// Main CLI structure for parsing command-line arguments
#[derive(Parser, Debug)]
#[command(name = "tagr")]
#[command(about = "A file tagging system", long_about = None)]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Suppress informational output (only print results)
    #[arg(short = 'q', long = "quiet", global = true)]
    pub quiet: bool,

    /// Display absolute paths (overrides config)
    #[arg(long = "absolute", global = true, conflicts_with = "relative")]
    pub absolute: bool,

    /// Display relative paths (overrides config)
    #[arg(long = "relative", global = true, conflicts_with = "absolute")]
    pub relative: bool,
}

/// Available CLI commands
#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Open interactive fuzzy finder (default)
    #[command(visible_alias = "b")]
    Browse {
        /// General query (searches both filenames and tags when -t/-f not specified)
        #[arg(value_name = "QUERY")]
        query: Option<String>,

        #[command(flatten)]
        criteria: SearchCriteriaArgs,

        /// Execute command for each selected file (use {} as placeholder for file path)
        #[arg(short = 'x', long = "exec", value_name = "COMMAND")]
        execute: Option<String>,

        /// Disable preview pane
        #[arg(long = "no-preview")]
        no_preview: bool,

        /// Maximum number of preview lines
        #[arg(long = "preview-lines", value_name = "LINES")]
        preview_lines: Option<usize>,

        /// Preview position (right, bottom, top)
        #[arg(long = "preview-position", value_name = "POSITION")]
        preview_position: Option<String>,

        /// Preview width percentage (0-100)
        #[arg(long = "preview-width", value_name = "PERCENT")]
        preview_width: Option<u8>,

        #[command(flatten)]
        db_args: DbArgs,

        #[command(flatten)]
        filter_args: FilterArgs,
    },

    /// Manage configuration settings
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },

    /// Manage databases
    Db {
        #[command(subcommand)]
        command: DbCommands,
    },

    /// Manage saved filters
    Filter {
        #[command(subcommand)]
        command: FilterCommands,
    },

    /// Tag a file with one or more tags
    #[command(visible_alias = "t")]
    Tag {
        /// File to tag
        #[arg(short = 'f', long = "file", value_name = "FILE")]
        file_flag: Option<PathBuf>,

        /// Tags to apply
        #[arg(short = 't', long = "tags", value_name = "TAG", num_args = 0..)]
        tags_flag: Vec<String>,

        /// File to tag (positional)
        #[arg(value_name = "FILE", conflicts_with = "file_flag")]
        file_pos: Option<PathBuf>,

        /// Tags to apply (positional)
        #[arg(value_name = "TAGS", conflicts_with = "tags_flag")]
        tags_pos: Vec<String>,

        #[command(flatten)]
        db_args: DbArgs,
    },

    /// Search files by tag
    #[command(visible_alias = "s")]
    Search {
        /// General query (searches both filenames and tags when -t/-f not specified)
        #[arg(value_name = "QUERY")]
        query: Option<String>,

        #[command(flatten)]
        criteria: SearchCriteriaArgs,

        #[command(flatten)]
        db_args: DbArgs,

        #[command(flatten)]
        filter_args: FilterArgs,
    },
    /// Remove tags from a file
    #[command(visible_alias = "u")]
    Untag {
        /// File to untag
        #[arg(short = 'f', long = "file", value_name = "FILE")]
        file_flag: Option<PathBuf>,

        /// Tags to remove (omit to remove all tags)
        #[arg(short = 't', long = "tags", value_name = "TAG", num_args = 0..)]
        tags_flag: Vec<String>,

        /// Remove all tags from the file
        #[arg(
            short = 'a',
            long = "all",
            conflicts_with = "tags_flag",
            conflicts_with = "tags_pos"
        )]
        all: bool,

        /// File to untag (positional)
        #[arg(value_name = "FILE", conflicts_with = "file_flag")]
        file_pos: Option<PathBuf>,

        /// Tags to remove (positional)
        #[arg(value_name = "TAGS", conflicts_with = "tags_flag")]
        tags_pos: Vec<String>,

        #[command(flatten)]
        db_args: DbArgs,
    },

    /// Manage tags globally
    Tags {
        #[command(subcommand)]
        command: TagsCommands,
    },

    /// Perform bulk operations on multiple files
    Bulk {
        #[command(subcommand)]
        command: BulkCommands,

        #[command(flatten)]
        db_args: DbArgs,
    },

    /// Clean up database by removing missing files and files with no tags
    #[command(visible_alias = "c")]
    Cleanup {
        #[command(flatten)]
        db_args: DbArgs,
    },

    /// List files or tags in the database
    #[command(visible_alias = "l")]
    List {
        /// What to list (files or tags)
        variant: ListVariant,

        #[command(flatten)]
        db_args: DbArgs,
    },
}

impl Commands {
    /// Get tag command context
    #[must_use]
    pub fn get_tag_context(&self) -> Option<TagContext> {
        match self {
            Self::Tag {
                file_flag,
                file_pos,
                tags_flag,
                tags_pos,
                ..
            } => {
                let file = file_flag.clone().or_else(|| file_pos.clone());
                let tags = if tags_flag.is_empty() {
                    tags_pos.clone()
                } else {
                    tags_flag.clone()
                };
                Some(TagContext { file, tags })
            }
            _ => None,
        }
    }

    /// Helper method to get search parameters from search command
    #[must_use]
    pub fn get_search_params(&self) -> Option<SearchParams> {
        match self {
            Self::Search {
                query, criteria, ..
            } => Some(SearchParams {
                query: query.clone(),
                tags: criteria.tags.clone(),
                tag_mode: if criteria.any_tag {
                    SearchMode::Any
                } else {
                    SearchMode::All
                },
                file_patterns: criteria.file_patterns.clone(),
                file_mode: if criteria.any_file {
                    SearchMode::Any
                } else {
                    SearchMode::All
                },
                exclude_tags: criteria.excludes.clone(),
                regex_tag: criteria.regex_tag,
                regex_file: criteria.regex_file,
                virtual_tags: criteria.virtual_tags.clone(),
                virtual_mode: if criteria.any_virtual {
                    SearchMode::Any
                } else {
                    SearchMode::All
                },
            }),
            _ => None,
        }
    }

    /// Get browse command context
    #[must_use]
    pub fn get_browse_context(&self) -> Option<BrowseContext> {
        match self {
            Self::Browse {
                query,
                criteria,
                execute,
                no_preview,
                preview_lines,
                preview_position,
                preview_width,
                ..
            } => {
                let search_params = if query.is_some()
                    || !criteria.tags.is_empty()
                    || !criteria.file_patterns.is_empty()
                    || !criteria.excludes.is_empty()
                    || !criteria.virtual_tags.is_empty()
                {
                    Some(SearchParams {
                        query: query.clone(),
                        tags: criteria.tags.clone(),
                        tag_mode: SearchMode::Any,
                        file_patterns: criteria.file_patterns.clone(),
                        file_mode: SearchMode::Any,
                        exclude_tags: criteria.excludes.clone(),
                        regex_tag: false,
                        regex_file: false,
                        virtual_tags: criteria.virtual_tags.clone(),
                        virtual_mode: SearchMode::Any,
                    })
                } else {
                    None
                };

                Some(BrowseContext {
                    search_params,
                    execute_cmd: execute.clone(),
                    preview_overrides: PreviewOverrides {
                        no_preview: *no_preview,
                        preview_lines: *preview_lines,
                        preview_position: preview_position.clone(),
                        preview_width: *preview_width,
                    },
                })
            }
            _ => None,
        }
    }

    /// Get untag command context
    #[must_use]
    pub fn get_untag_context(&self) -> Option<UntagContext> {
        match self {
            Self::Untag {
                file_flag,
                file_pos,
                tags_flag,
                tags_pos,
                all,
                ..
            } => {
                let file = file_flag.clone().or_else(|| file_pos.clone());
                let tags = if tags_flag.is_empty() {
                    tags_pos.clone()
                } else {
                    tags_flag.clone()
                };
                Some(UntagContext {
                    file,
                    tags,
                    all: *all,
                })
            }
            _ => None,
        }
    }

    /// Helper method to get the database name override from commands
    #[must_use]
    pub fn get_db(&self) -> Option<String> {
        match self {
            Self::Browse { db_args, .. }
            | Self::Tag { db_args, .. }
            | Self::Search { db_args, .. }
            | Self::Untag { db_args, .. }
            | Self::Bulk { db_args, .. }
            | Self::Cleanup { db_args }
            | Self::List { db_args, .. } => db_args.db.clone(),
            _ => None,
        }
    }

    /// Helper method to convert SearchCriteriaArgs to SearchParams
    fn search_criteria_to_params(criteria: &SearchCriteriaArgs) -> SearchParams {
        SearchParams {
            query: None,
            tags: criteria.tags.clone(),
            tag_mode: if criteria.any_tag {
                SearchMode::Any
            } else {
                SearchMode::All
            },
            file_patterns: criteria.file_patterns.clone(),
            file_mode: if criteria.any_file {
                SearchMode::Any
            } else {
                SearchMode::All
            },
            exclude_tags: criteria.excludes.clone(),
            regex_tag: criteria.regex_tag,
            regex_file: criteria.regex_file,
            virtual_tags: criteria.virtual_tags.clone(),
            virtual_mode: if criteria.any_virtual {
                SearchMode::Any
            } else {
                SearchMode::All
            },
        }
    }

    /// Get bulk command details
    #[must_use]
    pub fn get_bulk_context(&self) -> Option<(&BulkCommands, bool, bool)> {
        if let Self::Bulk { command, .. } = self {
            let (dry_run, yes) = match command {
                BulkCommands::Tag { dry_run, yes, .. } => (*dry_run, *yes),
                BulkCommands::Untag { dry_run, yes, .. } => (*dry_run, *yes),
                BulkCommands::RenameTag { dry_run, yes, .. } => (*dry_run, *yes),
            };
            Some((command, dry_run, yes))
        } else {
            None
        }
    }
}

impl Cli {
    /// Parse command line arguments
    #[must_use]
    pub fn parse_args() -> Self {
        Self::parse()
    }

    /// Get the command, defaulting to Browse if none specified
    #[must_use]
    pub fn get_command(&self) -> Commands {
        self.command.clone().unwrap_or(Commands::Browse {
            query: None,
            criteria: SearchCriteriaArgs {
                tags: Vec::new(),
                any_tag: false,
                all_tags: false,
                file_patterns: Vec::new(),
                any_file: false,
                all_files: false,
                excludes: Vec::new(),
                regex_tag: false,
                regex_file: false,
                virtual_tags: Vec::new(),
                any_virtual: false,
                all_virtual: false,
            },
            execute: None,
            no_preview: false,
            preview_lines: None,
            preview_position: None,
            preview_width: None,
            db_args: DbArgs { db: None },
            filter_args: FilterArgs {
                filter: None,
                save_filter: None,
                filter_desc: None,
            },
        })
    }

    /// Helper method to get the path format override from global flags
    #[must_use]
    pub const fn get_path_format(&self) -> Option<PathFormat> {
        if self.absolute {
            Some(PathFormat::Absolute)
        } else if self.relative {
            Some(PathFormat::Relative)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tag_with_flags() {
        let cli = Cli::parse_from(["tagr", "tag", "-f", "test.txt", "-t", "tag1", "tag2"]);
        if let Some(Commands::Tag { .. }) = cli.command {
            let ctx = cli.command.as_ref().unwrap().get_tag_context().unwrap();
            assert_eq!(ctx.file, Some(PathBuf::from("test.txt")));
            assert_eq!(ctx.tags, vec!["tag1".to_string(), "tag2".to_string()]);
        } else {
            panic!("Expected Tag command");
        }
    }

    #[test]
    fn test_parse_tag_with_positional() {
        let cli = Cli::parse_from(["tagr", "tag", "test.txt", "tag1", "tag2"]);
        if let Some(Commands::Tag { .. }) = cli.command {
            let ctx = cli.command.as_ref().unwrap().get_tag_context().unwrap();
            assert_eq!(ctx.file, Some(PathBuf::from("test.txt")));
            assert_eq!(ctx.tags, vec!["tag1".to_string(), "tag2".to_string()]);
        } else {
            panic!("Expected Tag command");
        }
    }

    #[test]
    fn test_parse_search_with_single_tag() {
        let cli = Cli::parse_from(["tagr", "search", "-t", "mytag"]);
        if let Some(Commands::Search { .. }) = cli.command {
            let params = cli.command.as_ref().unwrap().get_search_params().unwrap();
            assert_eq!(params.tags, vec!["mytag".to_string()]);
            assert_eq!(params.tag_mode, SearchMode::All);
        } else {
            panic!("Expected Search command");
        }
    }

    #[test]
    fn test_parse_search_with_multiple_tags() {
        let cli = Cli::parse_from(["tagr", "search", "-t", "tag1", "-t", "tag2", "--any-tag"]);
        if let Some(Commands::Search { .. }) = cli.command {
            let params = cli.command.as_ref().unwrap().get_search_params().unwrap();
            assert_eq!(params.tags, vec!["tag1".to_string(), "tag2".to_string()]);
            assert_eq!(params.tag_mode, SearchMode::Any);
        } else {
            panic!("Expected Search command");
        }
    }

    #[test]
    fn test_parse_search_with_file_patterns() {
        let cli = Cli::parse_from([
            "tagr",
            "search",
            "-t",
            "rust",
            "-f",
            "*.rs",
            "-f",
            "main.*",
            "--any-file",
        ]);
        if let Some(Commands::Search { .. }) = cli.command {
            let params = cli.command.as_ref().unwrap().get_search_params().unwrap();
            assert_eq!(params.tags, vec!["rust".to_string()]);
            assert_eq!(
                params.file_patterns,
                vec!["*.rs".to_string(), "main.*".to_string()]
            );
            assert_eq!(params.file_mode, SearchMode::Any);
        } else {
            panic!("Expected Search command");
        }
    }

    #[test]
    fn test_parse_search_with_exclusions() {
        let cli = Cli::parse_from([
            "tagr",
            "search",
            "-t",
            "rust",
            "-e",
            "deprecated",
            "-e",
            "old",
        ]);
        if let Some(Commands::Search { .. }) = cli.command {
            let params = cli.command.as_ref().unwrap().get_search_params().unwrap();
            assert_eq!(
                params.exclude_tags,
                vec!["deprecated".to_string(), "old".to_string()]
            );
        } else {
            panic!("Expected Search command");
        }
    }

    #[test]
    fn test_default_browse() {
        let cli = Cli::parse_from(["tagr"]);
        assert!(cli.command.is_none());
        let cmd = cli.get_command();
        assert!(matches!(cmd, Commands::Browse { .. }));
    }

    #[test]
    fn test_explicit_browse() {
        let cli = Cli::parse_from(["tagr", "browse"]);
        assert!(matches!(cli.command, Some(Commands::Browse { .. })));
    }

    #[test]
    fn test_browse_with_exec() {
        let cli = Cli::parse_from(["tagr", "browse", "-x", "cat {}"]);
        if let Some(Commands::Browse { .. }) = cli.command {
            let ctx = cli.command.as_ref().unwrap().get_browse_context().unwrap();
            assert_eq!(ctx.execute_cmd, Some("cat {}".to_string()));
        } else {
            panic!("Expected Browse command");
        }
    }

    #[test]
    fn test_browse_with_query() {
        let cli = Cli::parse_from(["tagr", "browse", "documents"]);
        if let Some(Commands::Browse { .. }) = cli.command {
            let ctx = cli.command.as_ref().unwrap().get_browse_context().unwrap();
            assert!(ctx.search_params.is_some());
            let params = ctx.search_params.unwrap();
            assert_eq!(params.query, Some("documents".to_string()));
        } else {
            panic!("Expected Browse command");
        }
    }

    #[test]
    fn test_browse_with_tags_and_patterns() {
        let cli = Cli::parse_from([
            "tagr",
            "browse",
            "-t",
            "documents",
            "-f",
            "*.txt",
            "-e",
            "*.md",
        ]);
        if let Some(Commands::Browse { .. }) = cli.command {
            let ctx = cli.command.as_ref().unwrap().get_browse_context().unwrap();
            assert!(ctx.search_params.is_some());
            let params = ctx.search_params.unwrap();
            assert_eq!(params.tags, vec!["documents".to_string()]);
            assert_eq!(params.file_patterns, vec!["*.txt".to_string()]);
            assert_eq!(params.exclude_tags, vec!["*.md".to_string()]);
        } else {
            panic!("Expected Browse command");
        }
    }

    #[test]
    fn test_parse_search_with_general_query() {
        let cli = Cli::parse_from(["tagr", "search", "document"]);
        if let Some(Commands::Search { .. }) = cli.command {
            let params = cli.command.as_ref().unwrap().get_search_params().unwrap();
            assert_eq!(params.query, Some("document".to_string()));
            assert!(params.tags.is_empty());
            assert!(params.file_patterns.is_empty());
        } else {
            panic!("Expected Search command");
        }
    }
}
