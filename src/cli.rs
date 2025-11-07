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
//! // Extract command-specific data
//! match command {
//!     Commands::Tag { .. } => {
//!         let file = command.get_file_from_tag();
//!         let tags = command.get_tags_from_tag();
//!     }
//!     _ => {}
//! }
//! ```

use clap::{Parser, Subcommand, ValueEnum};
use std::path::{Path, PathBuf};

/// List variant for the list command
#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListVariant {
    /// List all files in the database
    Files,
    /// List all tags in the database
    Tags,
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

/// Main CLI structure for parsing command-line arguments
#[derive(Parser, Debug)]
#[command(name = "tagr")]
#[command(about = "A file tagging system", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
    
    /// Suppress informational output (only print results)
    #[arg(short = 'q', long = "quiet", global = true)]
    pub quiet: bool,
}

/// Available CLI commands
#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Open interactive fuzzy finder (default)
    #[command(visible_alias = "b")]
    Browse {
        /// Execute command for each selected file (use {} as placeholder for file path)
        #[arg(short = 'x', long = "exec", value_name = "COMMAND")]
        execute: Option<String>,
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
    },

    /// Search files by tag
    #[command(visible_alias = "s")]
    Search {
        /// Tag to search for
        #[arg(short = 't', long = "tag", value_name = "TAG")]
        tag_flag: Option<String>,

        /// Tag to search for (positional)
        #[arg(value_name = "TAG", conflicts_with = "tag_flag")]
        tag_pos: Option<String>,
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
        #[arg(short = 'a', long = "all", conflicts_with = "tags_flag", conflicts_with = "tags_pos")]
        all: bool,

        /// File to untag (positional)
        #[arg(value_name = "FILE", conflicts_with = "file_flag")]
        file_pos: Option<PathBuf>,

        /// Tags to remove (positional)
        #[arg(value_name = "TAGS", conflicts_with = "tags_flag")]
        tags_pos: Vec<String>,
    },

    /// Manage tags globally
    Tags {
        #[command(subcommand)]
        command: TagsCommands,
    },

    /// Clean up database by removing missing files and files with no tags
    #[command(visible_alias = "c")]
    Cleanup,

    /// List files or tags in the database
    #[command(visible_alias = "l")]
    List {
        /// What to list (files or tags)
        variant: ListVariant,
    },
}

impl Commands {
    /// Helper method to get the file path from either flag or positional argument
    #[must_use] 
    pub fn get_file_from_tag(&self) -> Option<PathBuf> {
        match self {
            Self::Tag { file_flag, file_pos, .. } => {
                file_flag.clone().or_else(|| file_pos.clone())
            }
            _ => None,
        }
    }

    /// Helper method to get tags from either flag or positional arguments
    #[must_use] 
    pub fn get_tags_from_tag(&self) -> &[String] {
        match self {
            Self::Tag { tags_flag, tags_pos, .. } => {
                if tags_flag.is_empty() {
                    tags_pos
                } else {
                    tags_flag
                }
            }
            _ => &[],
        }
    }

    /// Helper method to get the tag from search command
    #[must_use] 
    pub fn get_tag_from_search(&self) -> Option<String> {
        match self {
            Self::Search { tag_flag, tag_pos } => {
                tag_flag.clone().or_else(|| tag_pos.clone())
            }
            _ => None,
        }
    }

    /// Helper method to get the execute command from browse
    #[must_use] 
    pub fn get_execute_from_browse(&self) -> Option<String> {
        match self {
            Self::Browse { execute } => execute.clone(),
            _ => None,
        }
    }

    /// Helper method to get the file path from untag command
    #[must_use] 
    pub fn get_file_from_untag(&self) -> Option<PathBuf> {
        match self {
            Self::Untag { file_flag, file_pos, .. } => {
                file_flag.clone().or_else(|| file_pos.clone())
            }
            _ => None,
        }
    }

    /// Helper method to get tags from untag command
    #[must_use] 
    pub fn get_tags_from_untag(&self) -> &[String] {
        match self {
            Self::Untag { tags_flag, tags_pos, .. } => {
                if tags_flag.is_empty() {
                    tags_pos
                } else {
                    tags_flag
                }
            }
            _ => &[],
        }
    }

    /// Helper method to check if untag should remove all tags
    #[must_use] 
    pub const fn get_all_from_untag(&self) -> bool {
        match self {
            Self::Untag { all, .. } => *all,
            _ => false,
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
        self.command.clone().unwrap_or(Commands::Browse { execute: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tag_with_flags() {
        let cli = Cli::parse_from(&["tagr", "tag", "-f", "test.txt", "-t", "tag1", "tag2"]);
        if let Some(Commands::Tag { .. }) = cli.command {
            let file = cli.command.as_ref().unwrap().get_file_from_tag();
            let tags = cli.command.as_ref().unwrap().get_tags_from_tag();
            assert_eq!(file, Some(PathBuf::from("test.txt")));
            assert_eq!(tags, vec!["tag1".to_string(), "tag2".to_string()]);
        } else {
            panic!("Expected Tag command");
        }
    }

    #[test]
    fn test_parse_tag_with_positional() {
        let cli = Cli::parse_from(&["tagr", "tag", "test.txt", "tag1", "tag2"]);
        if let Some(Commands::Tag { .. }) = cli.command {
            let file = cli.command.as_ref().unwrap().get_file_from_tag();
            let tags = cli.command.as_ref().unwrap().get_tags_from_tag();
            assert_eq!(file, Some(PathBuf::from("test.txt")));
            assert_eq!(tags, vec!["tag1".to_string(), "tag2".to_string()]);
        } else {
            panic!("Expected Tag command");
        }
    }

    #[test]
    fn test_parse_search_with_flag() {
        let cli = Cli::parse_from(&["tagr", "search", "-t", "mytag"]);
        if let Some(Commands::Search { .. }) = cli.command {
            let tag = cli.command.as_ref().unwrap().get_tag_from_search();
            assert_eq!(tag, Some("mytag".to_string()));
        } else {
            panic!("Expected Search command");
        }
    }

    #[test]
    fn test_parse_search_with_positional() {
        let cli = Cli::parse_from(&["tagr", "search", "mytag"]);
        if let Some(Commands::Search { .. }) = cli.command {
            let tag = cli.command.as_ref().unwrap().get_tag_from_search();
            assert_eq!(tag, Some("mytag".to_string()));
        } else {
            panic!("Expected Search command");
        }
    }

    #[test]
    fn test_default_browse() {
        let cli = Cli::parse_from(&["tagr"]);
        assert!(cli.command.is_none());
    }

    #[test]
    fn test_explicit_browse() {
        let cli = Cli::parse_from(&["tagr", "browse"]);
        assert!(matches!(cli.command, Some(Commands::Browse { .. })));
    }

    #[test]
    fn test_browse_with_exec() {
        let cli = Cli::parse_from(&["tagr", "browse", "-x", "cat {}"]);
        if let Some(Commands::Browse { .. }) = cli.command {
            let exec_cmd = cli.command.as_ref().unwrap().get_execute_from_browse();
            assert_eq!(exec_cmd, Some("cat {}".to_string()));
        } else {
            panic!("Expected Browse command");
        }
    }
}
