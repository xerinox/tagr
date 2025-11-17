//! Action execution logic for keybinds.

use crate::db::Database;
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
        _action: &BrowseAction,
        _context: &ActionContext,
    ) -> Result<ActionResult, ExecutorError> {
        // Placeholder - will be implemented in subsequent commits
        Ok(ActionResult::Continue)
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
    
    /// Action execution failed
    #[error("Failed to execute action: {0}")]
    ExecutionFailed(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestDb;

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
}
