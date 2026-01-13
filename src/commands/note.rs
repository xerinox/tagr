//! Note management commands

use crate::config::TagrConfig;
use crate::db::{Database, NoteRecord};
use crate::{config, output};
use clap::{Args, Subcommand, ValueEnum};
use std::io::Write;
use std::path::PathBuf;

/// Note management operations
#[derive(Debug, Args)]
pub struct NoteCommand {
    #[command(subcommand)]
    pub subcommand: NoteSubcommand,
}

/// Note subcommands
#[derive(Debug, Clone, Subcommand)]
pub enum NoteSubcommand {
    /// Edit note for one or more files in $EDITOR
    Edit(EditArgs),
    /// Show note content for files
    Show(ShowArgs),
    /// Delete notes from files
    Delete(DeleteArgs),
    /// List all files that have notes
    List(ListArgs),
    /// Search for notes containing text
    Search(SearchArgs),
}

/// Arguments for the edit subcommand
#[derive(Debug, Clone, Args)]
pub struct EditArgs {
    /// Files to edit notes for
    #[arg(required = true)]
    pub files: Vec<PathBuf>,

    /// Editor to use (overrides config and $EDITOR)
    #[arg(short = 'e', long = "editor")]
    pub editor: Option<String>,
}

/// Output format for note display
#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum OutputFormat {
    /// Plain text output (default)
    Text,
    /// JSON output for scripting
    Json,
    /// Minimal output (paths only)
    Quiet,
}

impl Default for OutputFormat {
    fn default() -> Self {
        Self::Text
    }
}

/// Arguments for the show subcommand
#[derive(Debug, Clone, Args)]
pub struct ShowArgs {
    /// Files to show notes for
    #[arg(required = true)]
    pub files: Vec<PathBuf>,

    /// Output format
    #[arg(short = 'f', long = "format", default_value = "text")]
    pub format: OutputFormat,

    /// Show additional metadata
    #[arg(short = 'v', long = "verbose")]
    pub verbose: bool,
}

/// Arguments for the delete subcommand
#[derive(Debug, Clone, Args)]
pub struct DeleteArgs {
    /// Files to delete notes from
    #[arg(required = true)]
    pub files: Vec<PathBuf>,

    /// Preview changes without applying them
    #[arg(short = 'n', long = "dry-run")]
    pub dry_run: bool,

    /// Skip confirmation prompt
    #[arg(short = 'y', long = "yes")]
    pub yes: bool,
}

/// Arguments for the list subcommand
#[derive(Debug, Clone, Args)]
pub struct ListArgs {
    /// Output format
    #[arg(short = 'f', long = "format", default_value = "text")]
    pub format: OutputFormat,

    /// Show additional metadata
    #[arg(short = 'v', long = "verbose")]
    pub verbose: bool,
}

/// Arguments for the search subcommand
#[derive(Debug, Clone, Args)]
pub struct SearchArgs {
    /// Search query
    #[arg(required = true)]
    pub query: String,

    /// Output format
    #[arg(short = 'f', long = "format", default_value = "text")]
    pub format: OutputFormat,

    /// Show note content snippets in results
    #[arg(short = 'c', long = "show-content")]
    pub show_content: bool,
}

// ==================== Implementation ====================

impl NoteSubcommand {
    /// Execute the note subcommand
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails
    pub fn execute(&self, db: &Database, config: &TagrConfig, path_format: config::PathFormat) -> Result<(), NoteError> {
        match self {
            Self::Edit(args) => execute_edit(args, db, config),
            Self::Show(args) => execute_show(args, db, path_format),
            Self::Delete(args) => execute_delete(args, db, path_format),
            Self::List(args) => execute_list(args, db, path_format),
            Self::Search(args) => execute_search(args, db, path_format),
        }
    }
}

/// Edit notes for files
fn execute_edit(args: &EditArgs, db: &Database, config: &TagrConfig) -> Result<(), NoteError> {
    let editor = args
        .editor
        .clone()
        .unwrap_or_else(|| config.notes.get_editor());

    for file in &args.files {
        let canonical_path = file.canonicalize().map_err(|e| {
            NoteError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Cannot access path '{}': {}", file.display(), e),
            ))
        })?;
        
        // Get existing note or create new one
        let existing_note = db.get_note(&canonical_path)?;
        let initial_content = existing_note
            .as_ref()
            .map(|n| n.content.clone())
            .unwrap_or_else(|| config.notes.default_template.clone());

        // Create temp file with initial content
        let temp_path = create_temp_note_file(&initial_content)?;

        // Open editor
        let status = std::process::Command::new(&editor)
            .arg(&temp_path)
            .status()
            .map_err(|e| NoteError::EditorFailed(format!("Failed to launch editor: {e}")))?;

        if !status.success() {
            std::fs::remove_file(&temp_path)?;
            return Err(NoteError::EditorFailed(format!(
                "Editor exited with status: {status}"
            )));
        }

        // Read updated content
        let updated_content = std::fs::read_to_string(&temp_path)?;
        std::fs::remove_file(&temp_path)?;

        // Check size limit
        if config
            .notes
            .exceeds_size_limit(updated_content.len() as u64)
        {
            eprintln!(
                "Warning: Note for {} exceeds size limit ({}KB)",
                file.display(),
                config.notes.max_note_size_kb
            );
        }

        // Save note
        let note = if let Some(mut existing) = existing_note {
            existing.update_content(updated_content);
            existing
        } else {
            NoteRecord::new(updated_content)
        };

        db.set_note(&canonical_path, note)?;
        println!("✓ Updated note for {}", file.display());
    }

    Ok(())
}

/// Show notes for files
fn execute_show(args: &ShowArgs, db: &Database, path_format: config::PathFormat) -> Result<(), NoteError> {
    for file in &args.files {
        let canonical_path = file.canonicalize().map_err(|e| {
            NoteError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Cannot access path '{}': {}", file.display(), e),
            ))
        })?;
        
        let note = db.get_note(&canonical_path)?;

        match note {
            Some(note) => match args.format {
                OutputFormat::Text => {
                    if args.verbose {
                        println!("File: {}", output::format_path(&canonical_path, path_format));
                        println!("Created: {}", format_timestamp(note.metadata.created_at));
                        println!("Updated: {}", format_timestamp(note.metadata.updated_at));
                        if let Some(author) = &note.metadata.author {
                            println!("Author: {author}");
                        }
                        if let Some(priority) = note.metadata.priority {
                            println!("Priority: {priority}");
                        }
                        println!("\n{}", note.content);
                    } else {
                        println!("{}", note.content);
                    }
                }
                OutputFormat::Json => {
                    let json = serde_json::json!({
                        "file": output::format_path(&canonical_path, path_format),
                        "content": note.content,
                        "metadata": {
                            "created_at": note.metadata.created_at,
                            "updated_at": note.metadata.updated_at,
                            "author": note.metadata.author,
                            "priority": note.metadata.priority,
                        },
                        "attachments": note.attachments,
                    });
                    println!("{}", serde_json::to_string_pretty(&json)?);
                }
                OutputFormat::Quiet => {
                    println!("{}", output::format_path(&canonical_path, path_format));
                }
            },
            None => {
                if args.format != OutputFormat::Quiet {
                    eprintln!("No note for {}", file.display());
                }
                return Err(NoteError::NotFound(file.display().to_string()));
            }
        }
    }

    Ok(())
}

/// Delete notes from files
fn execute_delete(args: &DeleteArgs, db: &Database, path_format: config::PathFormat) -> Result<(), NoteError> {
    let mut files_to_delete = Vec::new();

    // Check which files have notes
    for file in &args.files {
        let canonical_path = file.canonicalize().map_err(|e| {
            NoteError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Cannot access path '{}': {}", file.display(), e),
            ))
        })?;
        
        if db.get_note(&canonical_path)?.is_some() {
            files_to_delete.push(canonical_path);
        }
    }

    if files_to_delete.is_empty() {
        println!("No notes to delete");
        return Ok(());
    }

    if args.dry_run {
        println!("Would delete notes for {} file(s):", files_to_delete.len());
        for file in &files_to_delete {
            println!("  - {}", output::format_path(file, path_format));
        }
        return Ok(());
    }

    // Confirmation prompt
    if !args.yes {
        print!(
            "Delete notes for {} file(s)? [y/N] ",
            files_to_delete.len()
        );
        std::io::stdout().flush()?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;

        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Cancelled");
            return Ok(());
        }
    }

    // Delete notes
    let mut deleted = 0;
    for file in &files_to_delete {
        if db.delete_note(file)? {
            deleted += 1;
            println!("✓ Deleted note for {}", output::format_path(file, path_format));
        }
    }

    println!("Deleted {deleted} note(s)");
    Ok(())
}

/// List all files with notes
fn execute_list(args: &ListArgs, db: &Database, path_format: config::PathFormat) -> Result<(), NoteError> {
    let all_notes = db.list_all_notes()?;

    if all_notes.is_empty() {
        if args.format != OutputFormat::Quiet {
            println!("No notes found");
        }
        return Ok(());
    }

    match args.format {
        OutputFormat::Text => {
            if args.verbose {
                println!("Files with notes ({}):", all_notes.len());
                for (path, note) in &all_notes {
                    println!(
                        "  {} [updated: {}]",
                        output::format_path(path, path_format),
                        format_timestamp(note.metadata.updated_at)
                    );
                }
            } else {
                for (path, _) in &all_notes {
                    println!("{}", output::format_path(path, path_format));
                }
            }
        }
        OutputFormat::Json => {
            let json: Vec<_> = all_notes
                .iter()
                .map(|(path, note)| {
                    serde_json::json!({
                        "file": output::format_path(path, path_format),
                        "created_at": note.metadata.created_at,
                        "updated_at": note.metadata.updated_at,
                        "author": note.metadata.author,
                        "priority": note.metadata.priority,
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&json)?);
        }
        OutputFormat::Quiet => {
            for (path, _) in &all_notes {
                println!("{}", output::format_path(path, path_format));
            }
        }
    }

    Ok(())
}

/// Search notes by content
fn execute_search(args: &SearchArgs, db: &Database, path_format: config::PathFormat) -> Result<(), NoteError> {
    let results = db.search_notes(&args.query)?;

    if results.is_empty() {
        if args.format != OutputFormat::Quiet {
            eprintln!("No notes found matching '{}'", args.query);
        }
        std::process::exit(1);
    }

    match args.format {
        OutputFormat::Text => {
            for (path, note) in &results {
                println!("{}", output::format_path(path, path_format));
                if args.show_content {
                    let snippet = create_snippet(&note.content, &args.query, 100);
                    println!("  {snippet}");
                }
            }
        }
        OutputFormat::Json => {
            let json: Vec<_> = results
                .iter()
                .map(|(path, note)| {
                    let mut obj = serde_json::json!({
                        "file": output::format_path(path, path_format),
                        "metadata": {
                            "created_at": note.metadata.created_at,
                            "updated_at": note.metadata.updated_at,
                            "author": note.metadata.author,
                            "priority": note.metadata.priority,
                        },
                    });

                    if args.show_content {
                        obj["content"] = serde_json::json!(note.content);
                    }

                    obj
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&json)?);
        }
        OutputFormat::Quiet => {
            for (path, _) in &results {
                println!("{}", output::format_path(path, path_format));
            }
        }
    }

    Ok(())
}

// ==================== Helpers ====================

/// Create a temporary file for note editing
pub fn create_temp_note_file(content: &str) -> Result<PathBuf, NoteError> {
    let temp_dir = std::env::temp_dir();
    let temp_file = temp_dir.join(format!("tagr_note_{}.md", std::process::id()));

    std::fs::write(&temp_file, content)?;
    Ok(temp_file)
}

/// Format Unix timestamp as human-readable string
fn format_timestamp(timestamp: i64) -> String {
    use chrono::{DateTime, Local, TimeZone};

    Local
        .timestamp_opt(timestamp, 0)
        .single()
        .map(|dt: DateTime<Local>| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Create a snippet from content around the query match
fn create_snippet(content: &str, query: &str, max_length: usize) -> String {
    let query_lower = query.to_lowercase();
    let content_lower = content.to_lowercase();

    if let Some(pos) = content_lower.find(&query_lower) {
        let start = pos.saturating_sub(max_length / 2);
        let end = (pos + query.len() + max_length / 2).min(content.len());

        let mut snippet = content[start..end].to_string();

        if start > 0 {
            snippet = format!("...{snippet}");
        }
        if end < content.len() {
            snippet = format!("{snippet}...");
        }

        // Replace newlines with spaces for compact display
        snippet.replace('\n', " ")
    } else {
        // Fallback if query not found (shouldn't happen)
        content
            .chars()
            .take(max_length)
            .collect::<String>()
            .replace('\n', " ")
    }
}

// ==================== Error Types ====================

#[derive(Debug, thiserror::Error)]
pub enum NoteError {
    #[error("Database error: {0}")]
    Database(#[from] crate::db::DbError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Editor failed: {0}")]
    EditorFailed(String),

    #[error("Note not found: {0}")]
    NotFound(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_format_default() {
        assert_eq!(OutputFormat::default(), OutputFormat::Text);
    }

    #[test]
    fn test_create_snippet() {
        let content = "This is a long piece of content with the word rust in the middle";
        let snippet = create_snippet(content, "rust", 20);

        assert!(snippet.contains("rust"));
        assert!(snippet.len() < content.len());
    }

    #[test]
    fn test_create_snippet_with_newlines() {
        let content = "Line 1\nLine 2 with rust\nLine 3";
        let snippet = create_snippet(content, "rust", 20);

        assert!(snippet.contains("rust"));
        assert!(!snippet.contains('\n')); // Newlines should be replaced
    }

    #[test]
    fn test_format_timestamp() {
        let timestamp = 1234567890_i64;
        let formatted = format_timestamp(timestamp);

        assert!(!formatted.is_empty());
        assert_ne!(formatted, "unknown");
    }
}
