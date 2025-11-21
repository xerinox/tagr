//! Action execution logic for keybinds.

use crate::db::Database;
use crate::keybinds::prompts::{prompt_for_confirmation, prompt_for_input, PromptError};
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
        if action.requires_selection() && context.selected_files.is_empty() && context.current_file.is_none() {
            return Err(ExecutorError::NoSelection);
        }

        match action {
            BrowseAction::AddTag => self.execute_add_tag(context),
            BrowseAction::RemoveTag => self.execute_remove_tag(context),
            BrowseAction::DeleteFromDb => self.execute_delete_from_db(context),
            BrowseAction::OpenInDefault => self.execute_open_in_default(context),
            BrowseAction::OpenInEditor => self.execute_open_in_editor(context),
            BrowseAction::CopyPath => self.execute_copy_path(context),
            BrowseAction::CopyFiles => self.execute_copy_files(context),
            BrowseAction::ToggleTagDisplay => self.execute_toggle_tag_display(context),
            BrowseAction::ShowDetails => self.execute_show_details(context),
            BrowseAction::FilterExtension => self.execute_filter_extension(context),
            BrowseAction::SelectAll => self.execute_select_all(context),
            BrowseAction::ClearSelection => self.execute_clear_selection(context),
            BrowseAction::ShowHelp => self.execute_show_help(context),
            BrowseAction::Cancel => Ok(ActionResult::Continue),
            _ => {
                Ok(ActionResult::Continue)
            }
        }
    }

    /// Execute the `AddTag` action.
    fn execute_add_tag(&self, context: &ActionContext) -> Result<ActionResult, ExecutorError> {
        let input = prompt_for_input("Add tags (space-separated): ")?;
        
        if input.trim().is_empty() {
            return Ok(ActionResult::Message("No tags entered".to_string()));
        }

        let new_tags: Vec<String> = input
            .split_whitespace()
            .map(ToString::to_string)
            .collect();

        let files_to_tag: Vec<&PathBuf> = if context.selected_files.is_empty() {
            context.current_file.into_iter().collect()
        } else {
            context.selected_files.iter().collect()
        };

        let mut tagged_count = 0;
        for file_path in &files_to_tag {
            let mut existing_tags = context.db.get_tags(file_path)?.unwrap_or_default();
            
            for tag in &new_tags {
                if !existing_tags.contains(tag) {
                    existing_tags.push(tag.clone());
                }
            }
            
            context.db.insert(file_path, existing_tags)?;
            tagged_count += 1;
        }

        let tag_list = new_tags.join(", ");
        let message = format!("✓ Added [{tag_list}] to {tagged_count} file(s)");
        Ok(ActionResult::Message(message))
    }

    /// Execute the `RemoveTag` action.
    fn execute_remove_tag(&self, context: &ActionContext) -> Result<ActionResult, ExecutorError> {
        let files_to_process: Vec<&PathBuf> = if context.selected_files.is_empty() {
            context.current_file.into_iter().collect()
        } else {
            context.selected_files.iter().collect()
        };

        if files_to_process.is_empty() {
            return Err(ExecutorError::NoSelection);
        }

        let mut all_tags = std::collections::HashSet::new();
        for file_path in &files_to_process {
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
            return Ok(ActionResult::Message("No tags selected for removal".to_string()));
        }

        let tags_to_remove: Vec<String> = input
            .split_whitespace()
            .filter_map(|s| {
                if let Ok(num) = s.parse::<usize>() {
                    tag_list.get(num.saturating_sub(1)).cloned()
                } else {
                    Some(s.to_string())
                }
            })
            .collect();

        if tags_to_remove.is_empty() {
            return Ok(ActionResult::Message("No valid tags selected".to_string()));
        }

        let mut updated_count = 0;
        for file_path in &files_to_process {
            if let Some(mut tags) = context.db.get_tags(file_path)? {
                let original_count = tags.len();
                
                tags.retain(|tag| !tags_to_remove.contains(tag));
                
                if tags.len() < original_count {
                    context.db.insert(file_path, tags)?;
                    updated_count += 1;
                }
            }
        }

        let removed_list = tags_to_remove.join(", ");
        let message = format!("✓ Removed [{removed_list}] from {updated_count} file(s)");
        Ok(ActionResult::Message(message))
    }

    /// Execute the `DeleteFromDb` action.
    fn execute_delete_from_db(&self, context: &ActionContext) -> Result<ActionResult, ExecutorError> {
        let files_to_delete: Vec<&PathBuf> = if context.selected_files.is_empty() {
            context.current_file.into_iter().collect()
        } else {
            context.selected_files.iter().collect()
        };

        if files_to_delete.is_empty() {
            return Err(ExecutorError::NoSelection);
        }

        let confirm = prompt_for_confirmation(&format!(
            "Delete {} file(s) from database?",
            files_to_delete.len()
        ))?;

        if !confirm {
            return Ok(ActionResult::Message("Deletion cancelled".to_string()));
        }

        let mut deleted_count = 0;
        for file_path in &files_to_delete {
            if context.db.remove(file_path)? {
                deleted_count += 1;
            }
        }

        let message = format!("✓ Deleted {deleted_count} file(s) from database");
        Ok(ActionResult::Message(message))
    }

    /// Execute the `OpenInDefault` action.
    fn execute_open_in_default(&self, context: &ActionContext) -> Result<ActionResult, ExecutorError> {
        let files_to_open: Vec<&PathBuf> = if context.selected_files.is_empty() {
            context.current_file.into_iter().collect()
        } else {
            context.selected_files.iter().collect()
        };

        if files_to_open.is_empty() {
            return Err(ExecutorError::NoSelection);
        }

        let mut opened_count = 0;
        let mut errors = Vec::new();

        for file_path in &files_to_open {
            match open::that(file_path) {
                Ok(()) => opened_count += 1,
                Err(e) => errors.push(format!("{}: {}", file_path.display(), e)),
            }
        }

        if !errors.is_empty() {
            let error_msg = errors.join("\n");
            return Err(ExecutorError::ExecutionFailed(format!(
                "Failed to open {} file(s):\n{}",
                errors.len(),
                error_msg
            )));
        }

        let message = format!("✓ Opened {opened_count} file(s) in default application");
        Ok(ActionResult::Message(message))
    }

    /// Execute the `OpenInEditor` action.
    fn execute_open_in_editor(&self, context: &ActionContext) -> Result<ActionResult, ExecutorError> {
        let files_to_open: Vec<&PathBuf> = if context.selected_files.is_empty() {
            context.current_file.into_iter().collect()
        } else {
            context.selected_files.iter().collect()
        };

        if files_to_open.is_empty() {
            return Err(ExecutorError::NoSelection);
        }

        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());
        
        let mut cmd = std::process::Command::new(&editor);
        for file_path in &files_to_open {
            cmd.arg(file_path);
        }

        let status = cmd.status()?;

        if !status.success() {
            return Err(ExecutorError::ExecutionFailed(format!(
                "Editor '{}' exited with status: {:?}",
                editor,
                status.code()
            )));
        }

        let message = format!("✓ Opened {} file(s) in {}", files_to_open.len(), editor);
        Ok(ActionResult::Message(message))
    }

    /// Execute the `CopyPath` action.
    fn execute_copy_path(&self, context: &ActionContext) -> Result<ActionResult, ExecutorError> {
        let files_to_copy: Vec<&PathBuf> = if context.selected_files.is_empty() {
            context.current_file.into_iter().collect()
        } else {
            context.selected_files.iter().collect()
        };

        if files_to_copy.is_empty() {
            return Err(ExecutorError::NoSelection);
        }

        let paths_text = files_to_copy
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join("\n");

        match arboard::Clipboard::new() {
            Ok(mut clipboard) => {
                clipboard.set_text(&paths_text).map_err(|e| {
                    ExecutorError::ExecutionFailed(format!("Failed to copy to clipboard: {e}"))
                })?;
                
                let message = format!("✓ Copied {} path(s) to clipboard", files_to_copy.len());
                Ok(ActionResult::Message(message))
            }
            Err(e) => {
                eprintln!("⚠️  Clipboard unavailable: {e}");
                println!("\nPath(s):\n{paths_text}");
                Ok(ActionResult::Message(format!(
                    "Clipboard unavailable - printed {} path(s) to stdout",
                    files_to_copy.len()
                )))
            }
        }
    }

    /// Execute the `CopyFiles` action.
    fn execute_copy_files(&self, context: &ActionContext) -> Result<ActionResult, ExecutorError> {
        let files_to_copy: Vec<&PathBuf> = if context.selected_files.is_empty() {
            context.current_file.into_iter().collect()
        } else {
            context.selected_files.iter().collect()
        };

        if files_to_copy.is_empty() {
            return Err(ExecutorError::NoSelection);
        }

        let dest_input = prompt_for_input("Copy files to directory: ")?;
        
        if dest_input.trim().is_empty() {
            return Ok(ActionResult::Message("Copy cancelled - no destination specified".to_string()));
        }

        let dest_dir = PathBuf::from(dest_input.trim());
        
        if !dest_dir.exists() {
            let create_confirm = prompt_for_confirmation(&format!(
                "Directory '{}' doesn't exist. Create it?",
                dest_dir.display()
            ))?;
            
            if create_confirm {
                std::fs::create_dir_all(&dest_dir)?;
            } else {
                return Ok(ActionResult::Message("Copy cancelled".to_string()));
            }
        }

        if !dest_dir.is_dir() {
            return Err(ExecutorError::ExecutionFailed(format!(
                "'{}' is not a directory",
                dest_dir.display()
            )));
        }

        let mut copied_count = 0;
        let mut errors = Vec::new();

        for file_path in &files_to_copy {
            if let Some(filename) = file_path.file_name() {
                let dest_path = dest_dir.join(filename);
                
                match std::fs::copy(file_path, &dest_path) {
                    Ok(_) => copied_count += 1,
                    Err(e) => errors.push(format!("{}: {}", file_path.display(), e)),
                }
            } else {
                errors.push(format!("{}: invalid filename", file_path.display()));
            }
        }

        if !errors.is_empty() {
            let error_msg = errors.join("\n");
            return Err(ExecutorError::ExecutionFailed(format!(
                "Failed to copy {} file(s):\n{}",
                errors.len(),
                error_msg
            )));
        }

        let message = format!(
            "✓ Copied {} file(s) to {}",
            copied_count,
            dest_dir.display()
        );
        Ok(ActionResult::Message(message))
    }

    /// Execute the `ToggleTagDisplay` action.
    ///
    /// **Note**: This is a stub implementation. The actual toggle functionality
    /// will be handled by the UI layer. Returns `Result` for API consistency
    /// with other action handlers.
    #[allow(clippy::unnecessary_wraps)]
    fn execute_toggle_tag_display(&self, _context: &ActionContext) -> Result<ActionResult, ExecutorError> {
        Ok(ActionResult::Message(
            "Tag display toggling will be implemented in UI layer".to_string()
        ))
    }

    /// Execute the `ShowDetails` action.
    fn execute_show_details(&self, context: &ActionContext) -> Result<ActionResult, ExecutorError> {
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
            format!("Tags: {}", if tags.is_empty() { 
                "(none)".to_string() 
            } else { 
                tags.join(", ") 
            }),
        ];

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            details.push(format!("Permissions: {:o}", metadata.permissions().mode() & 0o777));
        }

        details.push("─".repeat(60));
        
        let message = details.join("\n");
        println!("{message}");
        
        Ok(ActionResult::Continue)
    }

    /// Execute the `FilterExtension` action.
    fn execute_filter_extension(&self, _context: &ActionContext) -> Result<ActionResult, ExecutorError> {
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
    fn execute_select_all(&self, _context: &ActionContext) -> Result<ActionResult, ExecutorError> {
        Ok(ActionResult::Message(
            "Select all will be handled by skim UI layer".to_string()
        ))
    }

    /// Execute the `ClearSelection` action.
    ///
    /// **Note**: This is a stub implementation. Selection state is managed
    /// by the skim UI layer. Returns `Result` for API consistency.
    #[allow(clippy::unnecessary_wraps)]
    fn execute_clear_selection(&self, _context: &ActionContext) -> Result<ActionResult, ExecutorError> {
        Ok(ActionResult::Message(
            "Clear selection will be handled by skim UI layer".to_string()
        ))
    }

    /// Execute the `ShowHelp` action.
    fn execute_show_help(&self, _context: &ActionContext) -> Result<ActionResult, ExecutorError> {
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
    match metadata.modified() {
        Ok(time) => {
            match time.elapsed() {
                Ok(duration) => {
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
                }
                Err(_) => "unknown".to_string(),
            }
        }
        Err(_) => "unknown".to_string(),
    }
}

/// Display text in the minus pager with search support.
fn show_in_pager(text: &str) -> Result<(), std::io::Error> {
    use minus::{Pager, ExitStrategy};

    // Create pager with static output mode
    let pager = Pager::new();
    
    // CRITICAL: Set exit strategy to PagerQuit so pressing 'q' only quits the pager,
    // not the entire application. This ensures we return to browse mode after help.
    pager.set_exit_strategy(ExitStrategy::PagerQuit).map_err(|e| {
        std::io::Error::other(
            format!("Failed to set exit strategy: {e}"),
        )
    })?;
    
    // Write the help text to the pager using push_str (for mutable pager)
    pager.push_str(text).map_err(|e| {
        std::io::Error::other(
            format!("Failed to write to pager: {e}"),
        )
    })?;

    // Run the pager in blocking mode - this will handle all terminal state
    // This blocks until the user presses 'q' to quit the pager
    minus::page_all(pager).map_err(|e| {
        std::io::Error::other(
            format!("Pager error: {e}"),
        )
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{TestDb, TempFile};

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
        
        // These actions should fail without selection
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
        
        // Insert test file
        db.db().insert(temp_file.path(), vec!["test".to_string()]).unwrap();
        assert!(db.db().contains(temp_file.path()).unwrap());
        
        // Note: This test can't easily test the full delete flow because
        // it requires user input via prompt_for_confirmation
        // We would need to mock the prompt system for full integration testing
    }
}
