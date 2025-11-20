//! UI controller for unified browser workflow
//!
//! This module provides the UI controller layer that bridges between the
//! business logic (`BrowseSession`) and the UI adapters (skim, ratatui).
//!
//! # Architecture
//!
//! The `BrowseController` implements a unified browser loop that works for
//! both tag selection and file selection phases. It:
//!
//! - Runs a single browser loop for the current phase
//! - Converts domain models (`TagrItem`) to UI display items
//! - Handles phase transitions (tags → files)
//! - Executes actions and refreshes data
//! - Manages the complete browse workflow
//!
//! # Workflow
//!
//! ```text
//! ┌─→ Get current phase from session
//! │   ↓
//! │   Run browser for phase (unified loop)
//! │   ↓
//! │   User Input?
//! │   ├─ Accept (Enter) → handle_accept() → Transition or Complete
//! │   ├─ Action (ctrl+t) → execute_action() → Refresh → Loop
//! │   └─ Cancel (ESC) → Exit
//! ```

use crate::browse::models::{ActionOutcome, ItemMetadata, TagrItem};
use crate::browse::session::{AcceptResult, BrowseResult, BrowseSession, PathFormat, PhaseType};
use crate::keybinds::actions::BrowseAction;
use crate::ui::{DisplayItem, FinderConfig, FuzzyFinder};
use colored::Colorize;
use std::path::Path;

/// UI controller - unified browser loop for tags and files
pub struct BrowseController<'a, F: FuzzyFinder> {
    session: BrowseSession<'a>,
    finder: F,
}

impl<'a, F: FuzzyFinder> BrowseController<'a, F> {
    /// Create new browser controller
    ///
    /// # Arguments
    ///
    /// * `session` - Browse session with state management
    /// * `finder` - UI adapter implementing FuzzyFinder trait
    #[must_use]
    pub fn new(session: BrowseSession<'a>, finder: F) -> Self {
        Self { session, finder }
    }

    /// Run unified browser workflow
    ///
    /// Runs a loop that:
    /// 1. Gets current phase from session
    /// 2. Runs browser for that phase
    /// 3. Handles user input (Accept, Action, Cancel)
    /// 4. Transitions between phases or completes
    ///
    /// # Returns
    ///
    /// - `Ok(Some(result))` - User completed browse with selections
    /// - `Ok(None)` - User cancelled or no data available
    /// - `Err(_)` - Error occurred during browse
    ///
    /// # Errors
    ///
    /// Returns error if database operations or action execution fails
    pub fn run(mut self) -> Result<Option<BrowseResult>, BrowseError> {
        loop {
            let phase = self.session.current_phase();

            // Check if phase has data
            if phase.items.is_empty() {
                match &phase.phase_type {
                    PhaseType::TagSelection => {
                        eprintln!("No tags in database");
                        return Ok(None);
                    }
                    PhaseType::FileSelection { .. } => {
                        eprintln!("No matching files");
                        return Ok(None);
                    }
                }
            }

            // Run unified browser loop for current phase
            let browser_result = self.run_browser_phase()?;

            match browser_result {
                BrowserResult::Accept(selected_ids) => {
                    // User pressed Enter - handle phase transition or completion
                    match self.session.handle_accept(selected_ids)? {
                        AcceptResult::PhaseTransition => {
                            // Transitioned to file phase, loop continues
                            continue;
                        }
                        AcceptResult::Complete(result) => {
                            // Session complete
                            return Ok(Some(result));
                        }
                        AcceptResult::Cancelled | AcceptResult::NoData => {
                            return Ok(None);
                        }
                    }
                }
                BrowserResult::Action { action, selected_ids } => {
                    // User pressed action keybind (ctrl+t, ctrl+d, etc.)
                    
                    // Handle UI-only actions that don't need session
                    match action {
                        BrowseAction::ShowHelp => {
                            self.show_help(&phase.settings.help_text);
                            continue;
                        }
                        BrowseAction::SelectAll | BrowseAction::ClearSelection => {
                            // These should be handled by skim directly via bindings
                            // If we get here, it's a configuration issue
                            eprintln!("Warning: {} should be handled by UI bindings", 
                                action.description());
                            continue;
                        }
                        _ => {}
                    }
                    
                    // Execute session-level action
                    let outcome = self.session.execute_action(action, &selected_ids)?;

                    // Handle action outcome (may need prompts from keybinds layer)
                    self.handle_action_outcome(outcome)?;

                    // Refresh data and continue browsing
                    self.session.refresh_current_phase()?;
                    continue;
                }
                BrowserResult::Cancel => {
                    // User pressed ESC
                    return Ok(None);
                }
            }
        }
    }

    /// Run unified browser for current phase (tag or file selection)
    ///
    /// Converts domain models to UI display items and invokes the finder.
    /// Returns user action (Accept, Action, Cancel).
    ///
    /// # Errors
    ///
    /// Returns error if finder invocation fails
    fn run_browser_phase(&self) -> Result<BrowserResult, BrowseError> {
        let phase = self.session.current_phase();

        // Convert domain models to UI display items with indices
        let display_items: Vec<DisplayItem> = phase
            .items
            .iter()
            .enumerate()
            .map(|(idx, item)| self.format_for_display(item, &phase.phase_type, idx))
            .collect();

        // Build phase-specific finder config
        let prompt = match &phase.phase_type {
            PhaseType::TagSelection => "Select tags (TAB for multi-select, Enter to continue)",
            PhaseType::FileSelection { .. } => {
                "Select files (TAB for multi-select, keybinds: ctrl+t/d/o/e/c/f)"
            }
        };

        // Convert keybind config to skim bindings
        let keybinds = phase.settings.keybind_config.to_skim_bindings();

        let config = FinderConfig::new(display_items, prompt.to_string())
            .with_multi_select(true)
            .with_ansi(true)
            .with_binds(keybinds);

        // Run finder - returns selection or action trigger
        let result = self.finder.run(config)?;

        if result.aborted {
            return Ok(BrowserResult::Cancel);
        }

        // Check if action was triggered via keybind
        if let Some(key) = &result.final_key {
            if key != "enter" {
                // Look up action for this key
                if let Some(action_name) = phase.settings.keybind_config.action_for_key(key) {
                    // Try to convert action name to BrowseAction
                    if let Ok(action) = action_name.parse::<BrowseAction>() {
                        return Ok(BrowserResult::Action {
                            action,
                            selected_ids: result.selected,
                        });
                    }
                }
            }
        }

        Ok(BrowserResult::Accept(result.selected))
    }

    /// Format TagrItem for display (phase-aware presentation logic)
    ///
    /// Converts domain models to display items with:
    /// - Colors and styling
    /// - Phase-specific formatting
    /// - Metadata annotations
    fn format_for_display(&self, item: &TagrItem, phase_type: &PhaseType, index: usize) -> DisplayItem {
        match &item.metadata {
            ItemMetadata::Tag(tag_meta) => {
                // Tag display: "tag_name (N files)"
                let display = format!(
                    "{} {}",
                    item.name.blue().bold(),
                    format!("({} files)", tag_meta.file_count).dimmed()
                );

                let mut metadata = crate::ui::ItemMetadata::default();
                metadata.index = Some(index);
                metadata.tags = vec![];
                metadata.exists = true;

                DisplayItem::with_metadata(item.id.clone(), display, item.name.clone(), metadata)
            }
            ItemMetadata::File(file_meta) => {
                // File display: path [tags]
                let path_str = self.format_path(&file_meta.path, phase_type);

                // Color based on existence
                let path_display = if file_meta.cached.exists {
                    path_str.green()
                } else {
                    path_str.red().strikethrough()
                };

                // Add tags in brackets
                let tags_display = if file_meta.tags.is_empty() {
                    String::new()
                } else {
                    format!(" {}", format!("[{}]", file_meta.tags.join(", ")).dimmed())
                };

                let display = format!("{}{}", path_display, tags_display);

                let mut metadata = crate::ui::ItemMetadata::default();
                metadata.index = Some(index);
                metadata.tags = file_meta.tags.clone();
                metadata.exists = file_meta.cached.exists;

                DisplayItem::with_metadata(item.id.clone(), display, path_str, metadata)
            }
        }
    }

    /// Format path based on session configuration
    ///
    /// Applies PathFormat settings from session config
    fn format_path(&self, path: &Path, phase_type: &PhaseType) -> String {
        // Only use configured path format in file phase
        let path_format = match phase_type {
            PhaseType::FileSelection { .. } => &self.session.config().path_format,
            _ => &PathFormat::Absolute, // Default for other phases
        };

        match path_format {
            PathFormat::Absolute => path.display().to_string(),
            PathFormat::Relative => {
                // Try to make path relative to current directory
                std::env::current_dir()
                    .ok()
                    .and_then(|cwd| path.strip_prefix(&cwd).ok())
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| path.display().to_string())
            }
            PathFormat::Basename => path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string(),
        }
    }

    /// Handle action outcome from session
    ///
    /// Displays results to user with formatting and emoji symbols.
    /// Note: Actions that need input (NeedsInput, NeedsConfirmation) should
    /// be handled by the keybinds layer, not here.
    ///
    /// # Errors
    ///
    /// Returns error if action failed or needs input (unexpected state)
    fn handle_action_outcome(&self, outcome: ActionOutcome) -> Result<(), BrowseError> {
        match outcome {
            ActionOutcome::Success {
                affected_count,
                details,
            } => {
                println!("✓ {} ({} files)", details, affected_count);
                Ok(())
            }
            ActionOutcome::Partial {
                succeeded,
                failed,
                errors,
            } => {
                println!(
                    "⚠️  Partial success: {} succeeded, {} failed",
                    succeeded, failed
                );
                for error in errors.iter().take(5) {
                    eprintln!("  - {}", error);
                }
                if errors.len() > 5 {
                    eprintln!("  ... and {} more errors", errors.len() - 5);
                }
                Ok(())
            }
            ActionOutcome::Failed(msg) => {
                eprintln!("❌ {}", msg);
                Err(BrowseError::ActionFailed(msg))
            }
            ActionOutcome::NeedsInput { prompt, .. } => {
                // This shouldn't happen - inputs should be handled by keybinds layer
                Err(BrowseError::UnexpectedState(format!(
                    "Action requires input but controller doesn't handle prompts: {}",
                    prompt
                )))
            }
            ActionOutcome::NeedsConfirmation { message, .. } => {
                // This shouldn't happen - confirmations should be handled by keybinds layer
                Err(BrowseError::UnexpectedState(format!(
                    "Action requires confirmation but controller doesn't handle prompts: {}",
                    message
                )))
            }
            ActionOutcome::Cancelled => Ok(()),
        }
    }

    /// Display help text to user
    fn show_help(&self, help_text: &crate::browse::session::HelpText) {
        use crate::browse::session::HelpText;
        
        println!("\n{}", "━".repeat(60).bright_blue());
        
        match help_text {
            HelpText::TagBrowser(_) => {
                println!("{}", "  TAG BROWSER HELP".bright_cyan().bold());
            }
            HelpText::FileBrowser(_) => {
                println!("{}", "  FILE BROWSER HELP".bright_cyan().bold());
            }
        }
        
        println!("{}", "━".repeat(60).bright_blue());
        
        let keybinds = match help_text {
            HelpText::TagBrowser(k) | HelpText::FileBrowser(k) => k,
        };
        
        for (key, desc) in keybinds {
            println!("  {:<15} {}", key.bright_yellow(), desc);
        }
        
        println!("{}", "━".repeat(60).bright_blue());
        println!("\nPress {} to continue (or {} to exit browse mode)...", 
            "Enter".bright_green(), "ESC".bright_yellow());
        
        let mut input = String::new();
        let _ = std::io::stdin().read_line(&mut input);
    }
}

/// Result from running browser phase
#[derive(Debug)]
enum BrowserResult {
    /// User accepted selection (Enter)
    Accept(Vec<String>),

    /// User triggered action (ctrl+t, etc.) with current selection
    Action {
        action: BrowseAction,
        selected_ids: Vec<String>,
    },

    /// User cancelled (ESC)
    Cancel,
}

/// Errors that can occur in UI controller
#[derive(Debug, thiserror::Error)]
pub enum BrowseError {
    #[error("Session error: {0}")]
    Session(#[from] crate::browse::session::BrowseError),

    #[error("UI error: {0}")]
    Ui(#[from] crate::ui::UiError),

    #[error("Action failed: {0}")]
    ActionFailed(String),

    #[error("Unexpected state: {0}")]
    UnexpectedState(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browse::session::BrowseConfig;
    use crate::testing::TestDb;
    use crate::ui::FinderResult;

    /// Mock finder for testing
    struct MockFinder {
        results: Vec<FinderResult>,
        call_count: std::cell::RefCell<usize>,
    }

    impl MockFinder {
        fn new(results: Vec<FinderResult>) -> Self {
            Self {
                results,
                call_count: std::cell::RefCell::new(0),
            }
        }
    }

    impl FuzzyFinder for MockFinder {
        fn run(&self, _config: FinderConfig) -> crate::ui::Result<FinderResult> {
            let mut count = self.call_count.borrow_mut();
            let result = self
                .results
                .get(*count)
                .ok_or_else(|| crate::ui::UiError::BuildError("No more mock results".into()))?;
            *count += 1;
            
            // Clone manually since FinderResult doesn't derive Clone
            Ok(FinderResult {
                selected: result.selected.clone(),
                aborted: result.aborted,
                final_key: result.final_key.clone(),
            })
        }
    }

    #[test]
    fn test_controller_cancels_on_empty_tag_selection() {
        let db = TestDb::new("test_controller_cancel");
        let config = BrowseConfig::default();
        let session = BrowseSession::new(db.db(), config).unwrap();

        let mock_finder = MockFinder::new(vec![FinderResult {
            selected: vec![],
            aborted: true,
            final_key: None,
        }]);

        let controller = BrowseController::new(session, mock_finder);
        let result = controller.run().unwrap();

        assert!(result.is_none());
    }
}
