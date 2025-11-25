//! Action execution logic for keybinds.
//!
//! This module handles UI concerns (prompting, formatting, emoji symbols)
//! and delegates business logic to `browse::actions`.

use crate::browse::{actions, models::ActionOutcome};
use crate::db::Database;
use crate::keybinds::prompts::{PromptError, prompt_for_confirmation, prompt_for_input};
use crate::keybinds::{ActionResult, BrowseAction};
use std::path::PathBuf;

/// Context provided to action executors.
pub struct ActionContext<'a> {
    /// Currently selected files
    pub selected_files: &'a [PathBuf],
    /// The file under cursor (if any)
    pub current_file: Option<&'a PathBuf>,
    /// Database reference
    pub db: &'a Database,
}

/// Executes actions triggered by keybinds.
pub struct ActionExecutor {
    // Will be expanded with state in future commits
}

impl ActionExecutor {
    /// Create a new action executor.
    #[must_use]
    pub const fn new() -> Self {
        Self {}
    }

    /// Execute an action with the given context.
    ///
    /// # Errors
    ///
    /// Returns error if the action execution fails.
    pub fn execute(
        &self,
        action: &BrowseAction,
        context: &ActionContext,
    ) -> Result<ActionResult, ExecutorError> {
        if action.requires_selection()
            && context.selected_files.is_empty()
            && context.current_file.is_none()
        {
            return Err(ExecutorError::NoSelection);
        }

        match action {
            BrowseAction::AddTag => Self::execute_add_tag(context),
            BrowseAction::RemoveTag => Self::execute_remove_tag(context),
            BrowseAction::DeleteFromDb => Self::execute_delete_from_db(context),
            BrowseAction::OpenInDefault => Self::execute_open_in_default(context),
            BrowseAction::OpenInEditor => Self::execute_open_in_editor(context),
            BrowseAction::CopyPath => Self::execute_copy_path(context),
            BrowseAction::CopyFiles => Self::execute_copy_files(context),
            BrowseAction::ToggleTagDisplay => Self::execute_toggle_tag_display(context),
            BrowseAction::ShowDetails => Self::execute_show_details(context),
            BrowseAction::FilterExtension => Self::execute_filter_extension(context),
            BrowseAction::SelectAll => Self::execute_select_all(context),
            BrowseAction::ClearSelection => Self::execute_clear_selection(context),
            BrowseAction::ShowHelp => Self::execute_show_help(context),
            BrowseAction::Cancel | _ => Ok(ActionResult::Continue),
        }
    }

    /// Execute the `AddTag` action.
    fn execute_add_tag(context: &ActionContext) -> Result<ActionResult, ExecutorError> {
        let input = prompt_for_input("Add tags (space-separated): ")?;

        if input.trim().is_empty() {
            return Ok(ActionResult::Message("No tags entered".to_string()));
        }

        let new_tags: Vec<String> = input.split_whitespace().map(ToString::to_string).collect();

        let files: Vec<PathBuf> = if context.selected_files.is_empty() {
            context.current_file.iter().map(|p| (*p).clone()).collect()
        } else {
            context.selected_files.to_vec()
        };

        let outcome = actions::execute_add_tag(context.db, &files, &new_tags)?;

        Ok(outcome.into())
    }

    /// Execute the `RemoveTag` action.
    fn execute_remove_tag(context: &ActionContext) -> Result<ActionResult, ExecutorError> {
        let files: Vec<PathBuf> = if context.selected_files.is_empty() {
            context.current_file.iter().map(|p| (*p).clone()).collect()
        } else {
            context.selected_files.to_vec()
        };

        if files.is_empty() {
            return Err(ExecutorError::NoSelection);
        }

        let mut all_tags = std::collections::HashSet::new();
        for file_path in &files {
            if let Some(tags) = context.db.get_tags(file_path)? {
                all_tags.extend(tags);
            }
        }

        if all_tags.is_empty() {
            return Ok(ActionResult::Message("No tags to remove".to_string()));
        }

        let tag_list: Vec<String> = all_tags.into_iter().collect();
        println!("\nAvailable tags:");
        for (i, tag) in tag_list.iter().enumerate() {
            println!("  {}. {}", i + 1, tag);
        }

        let input = prompt_for_input("\nEnter tag numbers or names to remove (space-separated): ")?;

        if input.trim().is_empty() {
            return Ok(ActionResult::Message(
                "No tags selected for removal".to_string(),
            ));
        }

        let tags_to_remove: Vec<String> = input
            .split_whitespace()
            .filter_map(|s| {
                s.parse::<usize>().map_or_else(
                    |_| Some(s.to_string()),
                    |num| tag_list.get(num.saturating_sub(1)).cloned(),
                )
            })
            .collect();

        if tags_to_remove.is_empty() {
            return Ok(ActionResult::Message("No valid tags selected".to_string()));
        }

        let outcome = actions::execute_remove_tag(context.db, &files, &tags_to_remove)?;

        Ok(outcome.into())
    }

    /// Execute the `DeleteFromDb` action.
    fn execute_delete_from_db(
        context: &ActionContext,
    ) -> Result<ActionResult, ExecutorError> {
        let files: Vec<PathBuf> = if context.selected_files.is_empty() {
            context.current_file.iter().map(|p| (*p).clone()).collect()
        } else {
            context.selected_files.to_vec()
        };

        if files.is_empty() {
            return Err(ExecutorError::NoSelection);
        }

        let confirm =
            prompt_for_confirmation(&format!("Delete {} file(s) from database?", files.len()))?;

        if !confirm {
            return Ok(ActionResult::Message("Deletion cancelled".to_string()));
        }

        let outcome = actions::execute_delete_from_db(context.db, &files)?;

        Ok(outcome.into())
    }

    /// Execute the `OpenInDefault` action.
    fn execute_open_in_default(
        context: &ActionContext,
    ) -> Result<ActionResult, ExecutorError> {
        let files: Vec<PathBuf> = if context.selected_files.is_empty() {
            context.current_file.iter().map(|p| (*p).clone()).collect()
        } else {
            context.selected_files.to_vec()
        };

        if files.is_empty() {
            return Err(ExecutorError::NoSelection);
        }

        let outcome = actions::execute_open_in_default(&files);

        Ok(outcome.into())
    }

    /// Execute the `OpenInEditor` action.
    fn execute_open_in_editor(
        context: &ActionContext,
    ) -> Result<ActionResult, ExecutorError> {
        let files: Vec<PathBuf> = if context.selected_files.is_empty() {
            context.current_file.iter().map(|p| (*p).clone()).collect()
        } else {
            context.selected_files.to_vec()
        };

        if files.is_empty() {
            return Err(ExecutorError::NoSelection);
        }

        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());

        let outcome = actions::execute_open_in_editor(&files, &editor);

        Ok(outcome.into())
    }

    /// Execute the `CopyPath` action.
    fn execute_copy_path(context: &ActionContext) -> Result<ActionResult, ExecutorError> {
        let files: Vec<PathBuf> = if context.selected_files.is_empty() {
            context.current_file.iter().map(|p| (*p).clone()).collect()
        } else {
            context.selected_files.to_vec()
        };

        if files.is_empty() {
            return Err(ExecutorError::NoSelection);
        }

        match actions::execute_copy_path(&files) {
            Ok(outcome) => Ok(outcome.into()),
            Err(e) => {
                let paths_text = files
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join("\n");
                eprintln!("⚠️  {e}");
                println!("Path(s):\n{paths_text}");
                Ok(ActionResult::Message(
                    "⚠️  Clipboard unavailable, paths printed to stdout".to_string(),
                ))
            }
        }
    }

    /// Execute the `CopyFiles` action.
    fn execute_copy_files(context: &ActionContext) -> Result<ActionResult, ExecutorError> {
        let files: Vec<PathBuf> = if context.selected_files.is_empty() {
            context.current_file.iter().map(|p| (*p).clone()).collect()
        } else {
            context.selected_files.to_vec()
        };

        if files.is_empty() {
            return Err(ExecutorError::NoSelection);
        }

        let dest_input = prompt_for_input("Enter destination directory: ")?;
        let dest_dir = std::path::PathBuf::from(dest_input.trim());

        if dest_dir.as_os_str().is_empty() {
            return Ok(ActionResult::Message("No destination provided".to_string()));
        }

        let outcome = actions::execute_copy_files(&files, &dest_dir, true);

        Ok(outcome.into())
    }

    /// Execute the `ToggleTagDisplay` action.
    ///
    /// **Note**: This is a stub implementation. The actual toggle functionality
    /// will be handled by the UI layer. Returns `Result` for API consistency
    /// with other action handlers.
    #[allow(clippy::unnecessary_wraps)]
    fn execute_toggle_tag_display(
        _context: &ActionContext,
    ) -> Result<ActionResult, ExecutorError> {
        Ok(ActionResult::Message(
            "Tag display toggling will be implemented in UI layer".to_string(),
        ))
    }

    /// Execute the `ShowDetails` action.
    fn execute_show_details(context: &ActionContext) -> Result<ActionResult, ExecutorError> {
        let file_to_show = if let Some(file) = context.current_file {
            file
        } else if let Some(file) = context.selected_files.first() {
            file
        } else {
            return Err(ExecutorError::NoSelection);
        };

        let metadata = std::fs::metadata(file_to_show)?;
        let tags = context.db.get_tags(file_to_show)?.unwrap_or_default();

        let mut details = vec![
            format!("\n📄 File Details: {}", file_to_show.display()),
            "─".repeat(60),
            format!("Size: {}", format_file_size(metadata.len())),
            format!("Modified: {}", format_modified_time(&metadata)),
            format!(
                "Tags: {}",
                if tags.is_empty() {
                    "(none)".to_string()
                } else {
                    tags.join(", ")
                }
            ),
        ];

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            details.push(format!(
                "Permissions: {:o}",
                metadata.permissions().mode() & 0o777
            ));
        }

        details.push("─".repeat(60));

        let message = details.join("\n");
        println!("{message}");

        Ok(ActionResult::Continue)
    }

    /// Execute the `FilterExtension` action.
    fn execute_filter_extension(
        _context: &ActionContext,
    ) -> Result<ActionResult, ExecutorError> {
        let extension = prompt_for_input("Filter by extension (e.g., 'txt', '.rs'): ")?;

        if extension.trim().is_empty() {
            return Ok(ActionResult::Message("Filter cancelled".to_string()));
        }

        let ext = extension.trim().trim_start_matches('.');
        Ok(ActionResult::Message(format!(
            "Extension filtering ({ext}) will be handled by browse UI layer"
        )))
    }

    /// Execute the `SelectAll` action.
    ///
    /// **Note**: This is a stub implementation. Selection state is managed
    /// by the skim UI layer. Returns `Result` for API consistency.
    #[allow(clippy::unnecessary_wraps)]
    fn execute_select_all(_context: &ActionContext) -> Result<ActionResult, ExecutorError> {
        Ok(ActionResult::Message(
            "Select all will be handled by skim UI layer".to_string(),
        ))
    }

    /// Execute the `ClearSelection` action.
    ///
    /// **Note**: This is a stub implementation. Selection state is managed
    /// by the skim UI layer. Returns `Result` for API consistency.
    #[allow(clippy::unnecessary_wraps)]
    fn execute_clear_selection(
        _context: &ActionContext,
    ) -> Result<ActionResult, ExecutorError> {
        Ok(ActionResult::Message(
            "Clear selection will be handled by skim UI layer".to_string(),
        ))
    }

    /// Execute the `ShowHelp` action.
    fn execute_show_help(_context: &ActionContext) -> Result<ActionResult, ExecutorError> {
        let help_text = r"
╔═══════════════════════════════════════════════════════════╗
║                  Tagr Browse Mode Keybinds                ║
╚═══════════════════════════════════════════════════════════╝

TAG MANAGEMENT:
  Ctrl+T    Add tags to selected files
  Ctrl+R    Remove tags from selected files
  Ctrl+E    Edit tags in $EDITOR

FILE OPERATIONS:
  Ctrl+O    Open in default application
  Ctrl+V    Open in $EDITOR
  Ctrl+Y    Copy file paths to clipboard
  Ctrl+P    Copy files to directory
  Ctrl+D    Delete from database

VIEW & NAVIGATION:
  Ctrl+I    Toggle tag display mode
  Ctrl+L    Show file details
  Ctrl+F    Filter by extension
  Ctrl+A    Select all files
  Ctrl+X    Clear selection

SEARCH & FILTER:
  Ctrl+S    Quick tag search
  Ctrl+G    Go to file

HISTORY & SESSIONS:
  Ctrl+H    Show recent selections
  Ctrl+B    Bookmark selection

SYSTEM:
  F1        Show this help (press 'q' to return)
  Enter     Exit with selection
  ESC       Cancel and abort

BASIC NAVIGATION:
  TAB       Toggle file selection
  Up/Down   Navigate files
  /         Start search query

Press 'q' to return to browse mode
        ";

        match show_in_pager(help_text) {
            Ok(()) => {
                // Give terminal a moment to stabilize after pager exits
                std::thread::sleep(std::time::Duration::from_millis(50));
                Ok(ActionResult::Continue)
            }
            Err(e) => {
                eprintln!("⚠️  Pager unavailable: {e}");
                println!("{help_text}");
                prompt_for_input("Press Enter to continue...")?;
                Ok(ActionResult::Continue)
            }
        }
    }
}

impl Default for ActionExecutor {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert `ActionOutcome` from business logic to `ActionResult` for UI
impl From<ActionOutcome> for ActionResult {
    fn from(outcome: ActionOutcome) -> Self {
        match outcome {
            ActionOutcome::Success {
                affected_count,
                details,
            } => Self::Message(format!("✓ {details} ({affected_count} files)")),
            ActionOutcome::Partial {
                succeeded,
                failed,
                errors,
            } => {
                let error_summary = if errors.len() > 3 {
                    format!(
                        "{} errors (showing first 3):\n  {}",
                        errors.len(),
                        errors[..3].join("\n  ")
                    )
                } else {
                    errors.join("\n  ")
                };
                Self::Message(format!(
                    "⚠️  {succeeded} succeeded, {failed} failed:\n  {error_summary}"
                ))
            }
            ActionOutcome::Failed(msg) => Self::Message(format!("❌ {msg}")),
            ActionOutcome::Cancelled => Self::Continue,
            ActionOutcome::NeedsInput { .. } | ActionOutcome::NeedsConfirmation { .. } => {
                // This shouldn't happen in executor context (prompting done before calling actions)
                Self::Message("❌ Unexpected state: action needs input".to_string())
            }
        }
    }
}

/// Errors that can occur during action execution.
#[derive(Debug, thiserror::Error)]
pub enum ExecutorError {
    /// Action requires selection but none provided
    #[error("Action requires file selection")]
    NoSelection,

    /// Database operation failed
    #[error("Database error: {0}")]
    Database(#[from] crate::db::DbError),

    /// IO operation failed
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Prompt operation failed
    #[error("Prompt error: {0}")]
    Prompt(#[from] PromptError),

    /// Action execution failed
    #[error("Failed to execute action: {0}")]
    ExecutionFailed(String),
}

/// Format file size in human-readable format.
fn format_file_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} bytes")
    }
}

/// Format modification time in human-readable format.
fn format_modified_time(metadata: &std::fs::Metadata) -> String {
    metadata.modified().map_or_else(
        |_| "unknown".to_string(),
        |time| {
            time.elapsed().map_or_else(
                |_| "unknown".to_string(),
                |duration| {
                    let secs = duration.as_secs();
                    if secs < 60 {
                        format!("{secs} seconds ago")
                    } else if secs < 3600 {
                        format!("{} minutes ago", secs / 60)
                    } else if secs < 86400 {
                        format!("{} hours ago", secs / 3600)
                    } else {
                        format!("{} days ago", secs / 86400)
                    }
                },
            )
        },
    )
}

/// Display text in the minus pager with search support.
fn show_in_pager(text: &str) -> Result<(), std::io::Error> {
    use minus::{ExitStrategy, Pager};

    let pager = Pager::new();

    // CRITICAL: Set exit strategy to PagerQuit so pressing 'q' only quits the pager,
    // not the entire application. This ensures we return to browse mode after help.
    pager
        .set_exit_strategy(ExitStrategy::PagerQuit)
        .map_err(|e| std::io::Error::other(format!("Failed to set exit strategy: {e}")))?;

    pager
        .push_str(text)
        .map_err(|e| std::io::Error::other(format!("Failed to write to pager: {e}")))?;

    minus::page_all(pager).map_err(|e| std::io::Error::other(format!("Pager error: {e}")))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{TempFile, TestDb};

    #[test]
    fn test_executor_creation() {
        let executor = ActionExecutor::new();
        let db = TestDb::new("test_executor_creation");

        let context = ActionContext {
            selected_files: &[],
            current_file: None,
            db: db.db(),
        };

        let result = executor.execute(&BrowseAction::Cancel, &context);
        assert!(result.is_ok());
    }

    #[test]
    fn test_action_requires_selection() {
        let executor = ActionExecutor::new();
        let db = TestDb::new("test_action_requires_selection");

        let context = ActionContext {
            selected_files: &[],
            current_file: None,
            db: db.db(),
        };

        let result = executor.execute(&BrowseAction::RemoveTag, &context);
        assert!(matches!(result, Err(ExecutorError::NoSelection)));

        let result = executor.execute(&BrowseAction::CopyPath, &context);
        assert!(matches!(result, Err(ExecutorError::NoSelection)));
    }

    #[test]
    fn test_delete_from_db() {
        let _executor = ActionExecutor::new();
        let db = TestDb::new("test_delete_from_db");
        let temp_file = TempFile::create("test_delete.txt").unwrap();

        db.db()
            .insert(temp_file.path(), vec!["test".to_string()])
            .unwrap();
        assert!(db.db().contains(temp_file.path()).unwrap());

        // This test can't easily test the full delete flow without mocking the prompt system
    }
}
