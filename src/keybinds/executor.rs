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
pub struct ActionExecutor {}

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
            BrowseAction::Cancel => Ok(ActionResult::Continue),
            _ => Ok(ActionResult::Continue),
        }
    }

    /// Execute the AddTag action.
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
        let message = format!("✓ Added [{}] to {} file(s)", tag_list, tagged_count);
        Ok(ActionResult::Message(message))
    }

    /// Execute the RemoveTag action.
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
        let message = format!("✓ Removed [{}] from {} file(s)", removed_list, updated_count);
        Ok(ActionResult::Message(message))
    }

    /// Execute the DeleteFromDb action.
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

        let message = format!("✓ Deleted {} file(s) from database", deleted_count);
        Ok(ActionResult::Message(message))
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
        
        db.db().insert(temp_file.path(), vec!["test".to_string()]).unwrap();
        assert!(db.db().contains(temp_file.path()).unwrap());
        
        // TODO: Full integration test requires mocking prompt_for_confirmation
    }
}
