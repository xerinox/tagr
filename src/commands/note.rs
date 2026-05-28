//! Note management commands

use crate::config::TagrConfig;
use crate::types::NoteRecord;
use crate::store::TagStore;
use crate::types::TagrPath;
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
    /// Add timestamped entry to note (append mode)
    Add(AddArgs),
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

/// Arguments for the add subcommand
#[derive(Debug, Clone, Args)]
pub struct AddArgs {
    /// File to add note entry to
    pub file: PathBuf,

    /// Note content to append
    pub content: String,
}

/// Output format for note display
#[derive(Default, Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum OutputFormat {
    /// Plain text output (default)
    #[default]
    Text,
    /// JSON output for scripting
    Json,
    /// Minimal output (paths only)
    Quiet,
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
    pub fn execute(
        &self,
        store: &dyn TagStore,
        config: &TagrConfig,
        path_format: config::PathFormat,
        writer: &mut impl Write,
    ) -> Result<(), NoteError> {
        match self {
            Self::Edit(args) => execute_edit(args, store, config, writer),
            Self::Add(args) => execute_add(args, store, path_format, writer),
            Self::Show(args) => execute_show(args, store, path_format, writer),
            Self::Delete(args) => execute_delete(args, store, path_format, writer),
            Self::List(args) => execute_list(args, store, path_format, writer),
            Self::Search(args) => execute_search(args, store, path_format, writer),
        }
    }
}

/// Edit notes for files
fn execute_edit(args: &EditArgs, store: &dyn TagStore, config: &TagrConfig, writer: &mut impl Write) -> Result<(), NoteError> {
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

        let tagr_path = TagrPath::new(&canonical_path)
            .map_err(|e| NoteError::PathError(e.to_string()))?;

        let existing_note = store.get_note(&tagr_path)?;
        let initial_content = existing_note.as_ref().map_or_else(
            || config.notes.default_template.clone(),
            |n| n.content.clone(),
        );

        let temp_path = create_temp_note_file(&initial_content)?;

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

        let updated_content = std::fs::read_to_string(&temp_path)?;
        std::fs::remove_file(&temp_path)?;

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

        let note = if let Some(mut existing) = existing_note {
            existing.update_content(updated_content);
            existing
        } else {
            NoteRecord::new(updated_content)
        };

        store.set_note(&tagr_path, &note)?;
        writeln!(writer, "✓ Updated note for {}", file.display())?;
    }

    Ok(())
}

/// Add a timestamped entry to a note (append mode)
fn execute_add(
    args: &AddArgs,
    store: &dyn TagStore,
    path_format: config::PathFormat,
    writer: &mut impl Write,
) -> Result<(), NoteError> {
    let canonical_path = args.file.canonicalize().map_err(|e| {
        NoteError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Cannot access path '{}': {}", args.file.display(), e),
        ))
    })?;

    let tagr_path = TagrPath::new(&canonical_path)
        .map_err(|e| NoteError::PathError(e.to_string()))?;

    let existing_content = store
        .get_note(&tagr_path)?
        .map(|n| n.content)
        .unwrap_or_default();

    let updated_content = append_note_entry(&existing_content, &args.content);

    let note = if let Some(mut existing) = store.get_note(&tagr_path)? {
        existing.update_content(updated_content);
        existing
    } else {
        NoteRecord::new(updated_content)
    };

    store.set_note(&tagr_path, &note)?;
    writeln!(
        writer,
        "✓ Added note entry to {}",
        output::format_path(&canonical_path, path_format)
    )?;

    Ok(())
}

/// Show notes for files
fn execute_show(
    args: &ShowArgs,
    store: &dyn TagStore,
    path_format: config::PathFormat,
    writer: &mut impl Write,
) -> Result<(), NoteError> {
    for file in &args.files {
        let canonical_path = file.canonicalize().map_err(|e| {
            NoteError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Cannot access path '{}': {}", file.display(), e),
            ))
        })?;

        let tagr_path = TagrPath::new(&canonical_path)
            .map_err(|e| NoteError::PathError(e.to_string()))?;

        let note = store.get_note(&tagr_path)?;

        if let Some(note) = note {
            match args.format {
                OutputFormat::Text => {
                    if args.verbose {
                        writeln!(
                            writer,
                            "File: {}",
                            output::format_path(&canonical_path, path_format)
                        )?;
                        writeln!(writer, "Created: {}", format_timestamp(note.metadata.created_at))?;
                        writeln!(writer, "Updated: {}", format_timestamp(note.metadata.updated_at))?;
                        writeln!(writer, "\n{}", note.content)?;
                    } else {
                        writeln!(writer, "{}", note.content)?;
                    }
                }
                OutputFormat::Json => {
                    let json = serde_json::json!({
                        "file": output::format_path(&canonical_path, path_format),
                        "content": note.content,
                        "metadata": {
                            "created_at": note.metadata.created_at,
                            "updated_at": note.metadata.updated_at,
                        },
                    });
                    writeln!(writer, "{}", serde_json::to_string_pretty(&json)?)?;
                }
                OutputFormat::Quiet => {
                    writeln!(writer, "{}", output::format_path(&canonical_path, path_format))?;
                }
            }
        } else {
            if args.format != OutputFormat::Quiet {
                eprintln!("No note for {}", file.display());
            }
            return Err(NoteError::NotFound(file.display().to_string()));
        }
    }

    Ok(())
}

/// Delete notes from files
fn execute_delete(
    args: &DeleteArgs,
    store: &dyn TagStore,
    path_format: config::PathFormat,
    writer: &mut impl Write,
) -> Result<(), NoteError> {
    let mut files_to_delete = Vec::new();

    for file in &args.files {
        let canonical_path = file.canonicalize().map_err(|e| {
            NoteError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Cannot access path '{}': {}", file.display(), e),
            ))
        })?;

        let tagr_path = TagrPath::new(&canonical_path)
            .map_err(|e| NoteError::PathError(e.to_string()))?;

        if store.get_note(&tagr_path)?.is_some() {
            files_to_delete.push(tagr_path);
        }
    }

    if files_to_delete.is_empty() {
        writeln!(writer, "No notes to delete")?;
        return Ok(());
    }

    if args.dry_run {
        writeln!(writer, "Would delete notes for {} file(s):", files_to_delete.len())?;
        for file in &files_to_delete {
            writeln!(writer, "  - {}", output::format_path(file, path_format))?;
        }
        return Ok(());
    }

    if !args.yes {
        print!("Delete notes for {} file(s)? [y/N] ", files_to_delete.len());
        std::io::stdout().flush()?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;

        if !input.trim().eq_ignore_ascii_case("y") {
            writeln!(writer, "Cancelled")?;
            return Ok(());
        }
    }

    let mut deleted = 0;
    for file in &files_to_delete {
        if store.delete_note(file)? {
            deleted += 1;
            writeln!(
                writer,
                "✓ Deleted note for {}",
                output::format_path(file, path_format)
            )?;
        }
    }

    writeln!(writer, "Deleted {deleted} note(s)")?;
    Ok(())
}

/// List all files with notes
fn execute_list(
    args: &ListArgs,
    store: &dyn TagStore,
    path_format: config::PathFormat,
    writer: &mut impl Write,
) -> Result<(), NoteError> {
    let all_notes = store.list_all_notes()?;

    if all_notes.is_empty() {
        if args.format != OutputFormat::Quiet {
            writeln!(writer, "No notes found")?;
        }
        return Ok(());
    }

    match args.format {
        OutputFormat::Text => {
            if args.verbose {
                writeln!(writer, "Files with notes ({}):", all_notes.len())?;
                for (path, note) in &all_notes {
                    writeln!(
                        writer,
                        "  {} [updated: {}]",
                        output::format_path(path, path_format),
                        format_timestamp(note.metadata.updated_at)
                    )?;
                }
            } else {
                for (path, _) in &all_notes {
                    writeln!(writer, "{}", output::format_path(path, path_format))?;
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
                    })
                })
                .collect();
            writeln!(writer, "{}", serde_json::to_string_pretty(&json)?)?;
        }
        OutputFormat::Quiet => {
            for (path, _) in &all_notes {
                writeln!(writer, "{}", output::format_path(path, path_format))?;
            }
        }
    }

    Ok(())
}

/// Search notes by content
fn execute_search(
    args: &SearchArgs,
    store: &dyn TagStore,
    path_format: config::PathFormat,
    writer: &mut impl Write,
) -> Result<(), NoteError> {
    let results = store.search_notes(&args.query)?;

    if results.is_empty() {
        if args.format != OutputFormat::Quiet {
            eprintln!("No notes found matching '{}'", args.query);
        }
        std::process::exit(1);
    }

    match args.format {
        OutputFormat::Text => {
            for (path, note) in &results {
                writeln!(writer, "{}", output::format_path(path, path_format))?;
                if args.show_content {
                    let snippet = create_snippet(&note.content, &args.query, 100);
                    writeln!(writer, "  {snippet}")?;
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
                        },
                    });

                    if args.show_content {
                        obj["content"] = serde_json::json!(note.content);
                    }

                    obj
                })
                .collect();
            writeln!(writer, "{}", serde_json::to_string_pretty(&json)?)?;
        }
        OutputFormat::Quiet => {
            for (path, _) in &results {
                writeln!(writer, "{}", output::format_path(path, path_format))?;
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

    Local.timestamp_opt(timestamp, 0).single().map_or_else(
        || "unknown".to_string(),
        |dt: DateTime<Local>| dt.format("%Y-%m-%d %H:%M:%S").to_string(),
    )
}

/// Format a timestamp for note entry headers (shorter format without seconds)
fn format_note_timestamp(timestamp: i64) -> String {
    use chrono::{DateTime, Local, TimeZone};
    Local.timestamp_opt(timestamp, 0).single().map_or_else(
        || "unknown".to_string(),
        |dt: DateTime<Local>| dt.format("%Y-%m-%d %H:%M").to_string(),
    )
}

/// Append a new timestamped entry to existing note content
fn append_note_entry(existing: &str, new_content: &str) -> String {
    let timestamp = chrono::Utc::now().timestamp();
    let formatted_time = format_note_timestamp(timestamp);

    if existing.trim().is_empty() {
        // First entry - no leading separator
        format!("### {formatted_time}\n\n{new_content}")
    } else {
        // Append to existing - add horizontal rule and heading
        format!("{existing}\n\n---\n### {formatted_time}\n\n{new_content}")
    }
}

/// Create a snippet from content around the query match
fn create_snippet(content: &str, query: &str, max_length: usize) -> String {
    let query_lower = query.to_lowercase();
    let content_lower = content.to_lowercase();

    content_lower.find(&query_lower).map_or_else(
        || {
            content
                .chars()
                .take(max_length)
                .collect::<String>()
                .replace('\n', " ")
        },
        |pos| {
            let start = pos.saturating_sub(max_length / 2);
            let end = (pos + query.len() + max_length / 2).min(content.len());

            let mut snippet = content[start..end].to_string();

            if start > 0 {
                snippet = format!("...{snippet}");
            }
            if end < content.len() {
                snippet = format!("{snippet}...");
            }

            snippet.replace('\n', " ")
        },
    )
}

// ==================== Error Types ====================

#[derive(Debug, thiserror::Error)]
pub enum NoteError {
    #[error("Store error: {0}")]
    Store(#[from] crate::store::StoreError),

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

    #[error("Invalid path: {0}")]
    PathError(String),
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
        let timestamp = 1_234_567_890_i64;
        let formatted = format_timestamp(timestamp);

        assert!(!formatted.is_empty());
        assert_ne!(formatted, "unknown");
    }

    #[test]
    fn test_append_note_entry_first() {
        let result = append_note_entry("", "First note");
        assert!(result.starts_with("### "));
        assert!(result.contains("\n\nFirst note"));
        assert!(!result.contains("---")); // No separator for first entry
    }

    #[test]
    fn test_append_note_entry_existing() {
        let existing = "### 2026-01-14 10:30\n\nFirst note";
        let result = append_note_entry(existing, "Second note");

        // Should contain both entries
        assert!(result.contains("First note"));
        assert!(result.contains("Second note"));
        // Should have horizontal rule separator between entries
        assert!(result.contains("\n---\n### "));
    }

    #[test]
    fn test_append_note_entry_whitespace() {
        let result = append_note_entry("   \n  ", "First note");
        // Empty/whitespace content treated as first entry
        assert!(result.starts_with("### "));
        assert!(!result.contains("---")); // No separator for first entry
    }

    #[test]
    fn test_format_note_timestamp() {
        let timestamp = 1_705_243_800_i64; // 2024-01-14 10:30:00
        let formatted = format_note_timestamp(timestamp);
        // Should not contain seconds
        assert!(!formatted.contains(":00"));
        // Should contain date
        assert!(formatted.contains("2024-01-14"));
    }
}
