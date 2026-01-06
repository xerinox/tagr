//! UI controller for unified browser workflow
//!
//! This module provides the UI controller layer that bridges between the
//! business logic (`BrowseSession`) and the TUI adapter (ratatui).
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

use crate::browse::actions;
use crate::browse::models::{ActionOutcome, ItemMetadata, TagrItem};
use crate::browse::session::{AcceptResult, BrowseResult, BrowseSession, PathFormat, PhaseType};
use crate::keybinds::actions::BrowseAction;
use crate::keybinds::prompts::{prompt_for_confirmation, prompt_for_input};
use crate::ui::{DisplayItem, FinderConfig, FuzzyFinder};
use colored::Colorize;
use std::path::{Path, PathBuf};

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
    /// * `finder` - UI adapter implementing `FuzzyFinder` trait
    #[must_use]
    pub const fn new(session: BrowseSession<'a>, finder: F) -> Self {
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
    #[allow(clippy::too_many_lines)]
    pub fn run(mut self) -> Result<Option<BrowseResult>, BrowseError> {
        loop {
            let phase = self.session.current_phase();

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
                    match self.session.handle_accept(selected_ids)? {
                        AcceptResult::PhaseTransition => {
                            // Transitioned to file phase, loop continues
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
                BrowserResult::Action {
                    action,
                    selected_ids,
                } => {
                    match action {
                        BrowseAction::ShowHelp => {
                            // Help is handled internally by the TUI overlay (F1/?)
                            // This branch shouldn't be reached with ratatui
                            continue;
                        }
                        BrowseAction::SelectAll | BrowseAction::ClearSelection => {
                            // These should be handled by skim directly via bindings
                            // If we get here, it's a configuration issue
                            eprintln!(
                                "Warning: {} should be handled by UI bindings",
                                action.description()
                            );
                            continue;
                        }
                        BrowseAction::RefineSearch => {
                            // Refine search is handled via BrowserResult::RefineSearch
                            // The TUI overlay handles the user interaction
                            continue;
                        }
                        _ => {}
                    }

                    // Execute session-level action
                    let outcome = self.session.execute_action(&action, &selected_ids)?;

                    self.handle_action_outcome(outcome)?;

                    self.session.refresh_current_phase()?;
                }
                BrowserResult::RefineSearch {
                    include_tags,
                    exclude_tags,
                    file_patterns,
                    virtual_tags,
                } => {
                    // User completed refine search overlay - apply the new criteria
                    use crate::cli::SearchParams;

                    let current =
                        self.session
                            .config()
                            .initial_search
                            .clone()
                            .unwrap_or_else(|| {
                                if let PhaseType::FileSelection { selected_tags } =
                                    &self.session.current_phase().phase_type
                                {
                                    SearchParams {
                                        query: None,
                                        tags: selected_tags.clone(),
                                        tag_mode: crate::cli::SearchMode::Any,
                                        file_patterns: vec![],
                                        file_mode: crate::cli::SearchMode::All,
                                        exclude_tags: vec![],
                                        regex_tag: false,
                                        regex_file: false,
                                        glob_files: false,
                                        virtual_tags: vec![],
                                        virtual_mode: crate::cli::SearchMode::All,
                                    }
                                } else {
                                    SearchParams {
                                        query: None,
                                        tags: vec![],
                                        tag_mode: crate::cli::SearchMode::Any,
                                        file_patterns: vec![],
                                        file_mode: crate::cli::SearchMode::All,
                                        exclude_tags: vec![],
                                        regex_tag: false,
                                        regex_file: false,
                                        glob_files: false,
                                        virtual_tags: vec![],
                                        virtual_mode: crate::cli::SearchMode::All,
                                    }
                                }
                            });

                    let new_params = SearchParams {
                        query: current.query.clone(),
                        tags: include_tags,
                        tag_mode: current.tag_mode,
                        file_patterns,
                        file_mode: current.file_mode,
                        exclude_tags,
                        regex_tag: current.regex_tag,
                        regex_file: current.regex_file,
                        glob_files: current.glob_files,
                        virtual_tags,
                        virtual_mode: current.virtual_mode,
                    };

                    self.session.update_search_params(new_params)?;
                    // Continue browsing with updated criteria
                }
                BrowserResult::InputAction {
                    action_id,
                    selected_ids,
                    values,
                } => {
                    // Execute action directly - confirmation already done in TUI modal
                    // Convert selected_ids to file paths
                    let files: Vec<PathBuf> = selected_ids.iter().map(PathBuf::from).collect();

                    // Execute the action based on action_id
                    let outcome = if values.is_empty() {
                        // Confirmation-only action (e.g., delete_from_db)
                        self.execute_confirmed_action(&action_id, &files)?
                    } else {
                        // Input action (e.g., add_tag, remove_tag)
                        let input = values.join(" ");
                        self.execute_action_with_input(&action_id, &files, &input)?
                    };

                    self.handle_action_outcome(outcome)?;
                    self.session.refresh_current_phase()?;
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

        // Convert PhaseType to BrowsePhase for keybind filtering
        let browse_phase = match &phase.phase_type {
            PhaseType::TagSelection => crate::ui::BrowsePhase::TagSelection,
            PhaseType::FileSelection { .. } => crate::ui::BrowsePhase::FileSelection,
        };

        // Get keybinds filtered by phase
        let keybinds = phase
            .settings
            .keybind_config
            .bindings_for_phase(browse_phase);

        // Get search criteria and available tags for refine search
        let search_criteria = self.session.search_criteria();
        let available_tags = self.session.available_tags().unwrap_or_default();

        let config = FinderConfig::new(display_items, prompt.to_string())
            .with_multi_select(true)
            .with_ansi(true)
            .with_binds(keybinds)
            .with_phase(browse_phase)
            .with_available_tags(available_tags)
            .with_search_criteria(crate::ui::RefineSearchCriteria::new(
                search_criteria.tags,
                search_criteria.exclude_tags,
                search_criteria.file_patterns,
                search_criteria.virtual_tags,
            ));

        // Add preview config if enabled for this phase
        let config = if let Some(preview_cfg) = phase.settings.preview_config.clone() {
            config.with_preview(preview_cfg.into())
        } else {
            config
        };

        // Run finder - returns selection or action trigger
        let result = self.finder.run(config)?;

        if result.aborted {
            return Ok(BrowserResult::Cancel);
        }

        // Check if this was a refine search result (F2 overlay completed)
        if let Some(ref criteria) = result.refine_search {
            return Ok(BrowserResult::RefineSearch {
                include_tags: criteria.include_tags.clone(),
                exclude_tags: criteria.exclude_tags.clone(),
                file_patterns: criteria.file_patterns.clone(),
                virtual_tags: criteria.virtual_tags.clone(),
            });
        }

        // Check if this was an input/confirmation action from TUI modal
        if let Some(ref input_action) = result.input_action {
            return Ok(BrowserResult::InputAction {
                action_id: input_action.action_id.clone(),
                selected_ids: result.selected.clone(),
                values: input_action.values.clone(),
            });
        }

        // Check if this was a custom action (not Enter/accept)
        if let Some(action_name) = &result.final_key
            && action_name != "enter"
        {
            // final_key contains the action name directly
            let resolved_action = action_name.clone();

            // Try to convert action name to BrowseAction
            if let Ok(action) = resolved_action.parse::<BrowseAction>() {
                return Ok(BrowserResult::Action {
                    action,
                    selected_ids: result.selected,
                });
            }
        }

        Ok(BrowserResult::Accept(result.selected))
    }

    /// Format `TagrItem` for display (phase-aware presentation logic)
    ///
    /// Converts domain models to display items with:
    /// - Colors and styling
    /// - Phase-specific formatting
    /// - Metadata annotations
    fn format_for_display(
        &self,
        item: &TagrItem,
        phase_type: &PhaseType,
        index: usize,
    ) -> DisplayItem {
        match &item.metadata {
            ItemMetadata::Tag(tag_meta) => {
                // Tag display: "tag_name (N files)"
                let display = format!(
                    "{} {}",
                    item.name.blue().bold(),
                    format!("({} files)", tag_meta.file_count).dimmed()
                );

                let metadata = crate::ui::ItemMetadata {
                    index: Some(index),
                    tags: vec![],
                    exists: true,
                };

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

                let display = format!("{path_display}{tags_display}");

                let metadata = crate::ui::ItemMetadata {
                    index: Some(index),
                    tags: file_meta.tags.clone(),
                    exists: file_meta.cached.exists,
                };

                DisplayItem::with_metadata(item.id.clone(), display, path_str, metadata)
            }
        }
    }

    /// Format path based on session configuration
    ///
    /// Applies `PathFormat` settings from session config
    fn format_path(&self, path: &Path, phase_type: &PhaseType) -> String {
        // Only use configured path format in file phase
        let path_format = match phase_type {
            PhaseType::FileSelection { .. } => &self.session.config().path_format,
            PhaseType::TagSelection => &PathFormat::Absolute, // Default for tag phase
        };

        match path_format {
            PathFormat::Absolute => path.display().to_string(),
            PathFormat::Relative => {
                // Try to make path relative to current directory
                std::env::current_dir()
                    .ok()
                    .and_then(|cwd| path.strip_prefix(&cwd).ok())
                    .map_or_else(|| path.display().to_string(), |p| p.display().to_string())
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
    /// For actions that need input or confirmation, prompts the user and
    /// executes the action with the provided input.
    ///
    /// # Errors
    ///
    /// Returns error if action failed or prompting failed
    fn handle_action_outcome(&self, outcome: ActionOutcome) -> Result<(), BrowseError> {
        match outcome {
            ActionOutcome::Success {
                affected_count,
                details,
            } => {
                println!("✓ {details} ({affected_count} files)");
                Ok(())
            }
            ActionOutcome::Partial {
                succeeded,
                failed,
                errors,
            } => {
                println!("⚠️  Partial success: {succeeded} succeeded, {failed} failed");
                for error in errors.iter().take(5) {
                    eprintln!("  - {error}");
                }
                if errors.len() > 5 {
                    eprintln!("  ... and {} more errors", errors.len() - 5);
                }
                Ok(())
            }
            ActionOutcome::Failed(msg) => {
                eprintln!("❌ {msg}");
                Err(BrowseError::ActionFailed(msg))
            }
            ActionOutcome::NeedsInput {
                prompt,
                action_id,
                context,
            } => {
                // Prompt user for input
                let input = prompt_for_input(&prompt)
                    .map_err(|e| BrowseError::UnexpectedState(format!("Prompt failed: {e}")))?;

                if input.trim().is_empty() {
                    println!("Cancelled - no input provided");
                    return Ok(());
                }

                // Execute the action with the input
                let result = self.execute_action_with_input(&action_id, &context.files, &input)?;

                // Recursively handle the result (which should now be Success/Failed/Partial)
                self.handle_action_outcome(result)
            }
            ActionOutcome::NeedsConfirmation {
                message,
                action_id,
                context,
            } => {
                // Prompt user for confirmation
                let confirmed = prompt_for_confirmation(&message)
                    .map_err(|e| BrowseError::UnexpectedState(format!("Prompt failed: {e}")))?;

                if !confirmed {
                    println!("Cancelled by user");
                    return Ok(());
                }

                // Execute the action with confirmation
                let result = self.execute_confirmed_action(&action_id, &context.files)?;

                // Recursively handle the result
                self.handle_action_outcome(result)
            }
            ActionOutcome::Cancelled => Ok(()),
        }
    }

    /// Execute action that required user input
    fn execute_action_with_input(
        &self,
        action_id: &str,
        files: &[PathBuf],
        input: &str,
    ) -> Result<ActionOutcome, BrowseError> {
        match action_id {
            "add_tag" => {
                let tags: Vec<String> = input.split_whitespace().map(ToString::to_string).collect();

                if tags.is_empty() {
                    return Ok(ActionOutcome::Failed("No tags specified".to_string()));
                }

                actions::execute_add_tag(self.session.db(), files, &tags)
                    .map_err(|e| BrowseError::ActionFailed(e.to_string()))
            }
            "remove_tag" => {
                let tags: Vec<String> = input.split_whitespace().map(ToString::to_string).collect();

                if tags.is_empty() {
                    return Ok(ActionOutcome::Failed("No tags specified".to_string()));
                }

                actions::execute_remove_tag(self.session.db(), files, &tags)
                    .map_err(|e| BrowseError::ActionFailed(e.to_string()))
            }
            "copy_files" => {
                let dest_dir = PathBuf::from(input.trim());

                let create_dest = if dest_dir.exists() {
                    false
                } else {
                    prompt_for_confirmation(&format!(
                        "Directory '{}' doesn't exist. Create it?",
                        dest_dir.display()
                    ))
                    .map_err(|e| BrowseError::UnexpectedState(format!("Prompt failed: {e}")))?
                };

                Ok(actions::execute_copy_files(files, &dest_dir, create_dest))
            }
            _ => Err(BrowseError::UnexpectedState(format!(
                "Unknown action_id: {action_id}"
            ))),
        }
    }

    /// Execute action that required confirmation
    fn execute_confirmed_action(
        &self,
        action_id: &str,
        files: &[PathBuf],
    ) -> Result<ActionOutcome, BrowseError> {
        match action_id {
            "delete_from_db" => actions::execute_delete_from_db(self.session.db(), files)
                .map_err(|e| BrowseError::ActionFailed(e.to_string())),
            _ => Err(BrowseError::UnexpectedState(format!(
                "Unknown action_id: {action_id}"
            ))),
        }
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

    /// User submitted an input/confirmation action via TUI modal
    /// This bypasses the executor's confirmation prompts since TUI already confirmed
    InputAction {
        action_id: String,
        selected_ids: Vec<String>,
        /// Values from text input (empty for confirmation-only actions)
        values: Vec<String>,
    },

    /// User refined search criteria (F2 overlay completed)
    RefineSearch {
        include_tags: Vec<String>,
        exclude_tags: Vec<String>,
        file_patterns: Vec<String>,
        virtual_tags: Vec<String>,
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

    #[error("Database error: {0}")]
    Database(#[from] crate::db::DbError),
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
                refine_search: result.refine_search.clone(),
                input_action: result.input_action.clone(),
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
            refine_search: None,
            input_action: None,
        }]);

        let controller = BrowseController::new(session, mock_finder);
        let result = controller.run().unwrap();

        assert!(result.is_none());
    }
}
