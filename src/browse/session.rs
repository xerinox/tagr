//! Unified browser session management
//!
//! This module implements the core session logic for Tagr's browse functionality,
//! using a unified browser pattern where tag selection and file selection are the
//! same interactive loop with different parameters.
//!
//! # Architecture
//!
//! - **`BrowseSession`**: Manages state transitions between browser phases
//! - **`BrowserPhase`**: Current phase state (tag or file selection) with config
//! - **Entry Flexibility**: Can start at tag or file phase based on CLI params
//! - **Unified Pattern**: Both phases use same browser loop in UI controller
//!
//! # Workflow
//!
//! ```text
//! Session Created
//!     ↓
//! Check initial_search?
//!     ├─ None → Tag Browser Phase
//!     └─ Some → File Browser Phase (pre-filtered)
//!         ↓
//! ┌─→ Browser Loop (UI controller)
//! │       ↓
//! │   User Action?
//! │   ├─ Enter → handle_accept() → Transition or Complete
//! │   ├─ ESC → Cancel
//! │   └─ Keybind → execute_action() → Refresh → Loop
//! ```

use crate::browse::models::{ActionOutcome, SearchMode, TagrItem};
use crate::browse::{actions, query};
use crate::cli::SearchParams;
use crate::config::PreviewConfig;
use crate::db::Database;
use crate::keybinds::actions::BrowseAction;
use crate::keybinds::config::KeybindConfig;
use std::path::PathBuf;

/// Browse session error type
pub type Result<T> = std::result::Result<T, BrowseError>;

/// Errors that can occur during browse session
#[derive(Debug, thiserror::Error)]
pub enum BrowseError {
    #[error("Database error: {0}")]
    Database(#[from] crate::db::DbError),

    #[error("Action not available in current phase")]
    ActionNotAvailable,

    #[error("Action failed: {0}")]
    ActionFailed(String),

    #[error("Invalid state: {0}")]
    InvalidState(String),

    #[error("Unexpected state: {0}")]
    UnexpectedState(String),
}

/// Browse session - manages unified browser state transitions
pub struct BrowseSession<'a> {
    db: &'a Database,
    config: BrowseConfig,
    current_phase: BrowserPhase,
}

/// Configuration for browse session
#[derive(Clone)]
pub struct BrowseConfig {
    /// Initial search parameters (if provided via CLI)
    pub initial_search: Option<SearchParams>,

    /// Path display format
    pub path_format: PathFormat,

    /// Tag selection phase settings
    pub tag_phase_settings: PhaseSettings,

    /// File selection phase settings
    pub file_phase_settings: PhaseSettings,
}

/// Path display format options
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathFormat {
    /// Full absolute path
    Absolute,

    /// Relative to current directory
    Relative,

    /// Just the filename
    Basename,
}

/// Configuration for a specific browser phase
#[derive(Clone)]
pub struct PhaseSettings {
    /// Whether preview is enabled
    pub preview_enabled: bool,

    /// Preview configuration (if enabled)
    pub preview_config: Option<PreviewConfig>,

    /// Keybind configuration for this phase
    pub keybind_config: KeybindConfig,

    /// Help text for F1 key
    pub help_text: HelpText,
}

/// Phase-specific help text for F1
#[derive(Clone)]
pub enum HelpText {
    /// Tag browser help
    TagBrowser(Vec<(String, String)>), // (keybind, description)

    /// File browser help
    FileBrowser(Vec<(String, String)>),
}

/// Current browser phase state
pub struct BrowserPhase {
    /// Type of phase (tag or file selection)
    pub phase_type: PhaseType,

    /// Items to display
    pub items: Vec<TagrItem>,

    /// Phase-specific settings
    pub settings: PhaseSettings,
}

/// Type of browser phase
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhaseType {
    /// Selecting tags
    TagSelection,

    /// Selecting files (with tags that were selected)
    FileSelection {
        /// Tags selected in previous phase (or from CLI)
        selected_tags: Vec<String>,
    },
}

impl<'a> BrowseSession<'a> {
    /// Create new browse session
    ///
    /// Determines starting phase based on `config.initial_search`:
    /// - If `None`: Start with tag selection phase
    /// - If `Some`: Skip to file selection with pre-filtered files
    ///
    /// # Errors
    ///
    /// Returns error if database queries fail
    pub fn new(db: &'a Database, config: BrowseConfig) -> Result<Self> {
        let current_phase = if let Some(ref search_params) = config.initial_search {
            // Skip tag selection, go directly to file browser
            let items = query::get_matching_files(db, search_params)?;

            BrowserPhase {
                phase_type: PhaseType::FileSelection {
                    selected_tags: search_params.tags.clone(),
                },
                items,
                settings: config.file_phase_settings.clone(),
            }
        } else {
            // Start with tag selection
            let items = query::get_available_tags(db)?;

            BrowserPhase {
                phase_type: PhaseType::TagSelection,
                items,
                settings: config.tag_phase_settings.clone(),
            }
        };

        Ok(Self {
            db,
            config,
            current_phase,
        })
    }

    /// Get current browser phase for UI to render
    #[must_use]
    pub const fn current_phase(&self) -> &BrowserPhase {
        &self.current_phase
    }

    /// Handle "Accept" action (Enter key) in current phase
    ///
    /// # Behavior
    ///
    /// - **Tag Phase**: Query files matching selected tags, transition to file phase
    /// - **File Phase**: Complete session with selected files
    ///
    /// # Errors
    ///
    /// Returns error if database queries fail
    pub fn handle_accept(&mut self, selected_ids: Vec<String>) -> Result<AcceptResult> {
        match &self.current_phase.phase_type {
            PhaseType::TagSelection => {
                if selected_ids.is_empty() {
                    return Ok(AcceptResult::Cancelled);
                }

                // Query files with selected tags
                let items = query::get_files_by_tags(self.db, &selected_ids, SearchMode::Any)?;

                if items.is_empty() {
                    return Ok(AcceptResult::NoData);
                }

                // Transition to file browser
                self.current_phase = BrowserPhase {
                    phase_type: PhaseType::FileSelection {
                        selected_tags: selected_ids,
                    },
                    items,
                    settings: self.config.file_phase_settings.clone(),
                };

                Ok(AcceptResult::PhaseTransition)
            }

            PhaseType::FileSelection { selected_tags } => {
                if selected_ids.is_empty() {
                    return Ok(AcceptResult::Cancelled);
                }

                // Extract paths from item IDs
                let selected_files: Vec<PathBuf> = self
                    .current_phase
                    .items
                    .iter()
                    .filter(|item| selected_ids.contains(&item.id))
                    .filter_map(|item| {
                        if let crate::browse::models::ItemMetadata::File(file_meta) = &item.metadata
                        {
                            Some(file_meta.path.clone())
                        } else {
                            None
                        }
                    })
                    .collect();

                Ok(AcceptResult::Complete(BrowseResult {
                    selected_tags: selected_tags.clone(),
                    selected_files,
                }))
            }
        }
    }

    /// Execute keybind action on current selection
    ///
    /// Actions are only available in the file selection phase. In tag selection,
    /// this returns an error.
    ///
    /// # Arguments
    ///
    /// * `action` - The action to execute
    /// * `selected_ids` - IDs of currently selected items
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Action is called in tag phase
    /// - Database operations fail
    /// - Action execution fails
    pub fn execute_action(
        &self,
        action: &BrowseAction,
        selected_ids: &[String],
    ) -> Result<ActionOutcome> {
        // Actions only available in file phase
        if !matches!(
            self.current_phase.phase_type,
            PhaseType::FileSelection { .. }
        ) {
            return Err(BrowseError::ActionNotAvailable);
        }

        let selected_files: Vec<PathBuf> = self
            .current_phase
            .items
            .iter()
            .filter(|item| selected_ids.contains(&item.id))
            .filter_map(|item| {
                if let crate::browse::models::ItemMetadata::File(file_meta) = &item.metadata {
                    Some(file_meta.path.clone())
                } else {
                    None
                }
            })
            .collect();

        match action {
            BrowseAction::AddTag => Ok(ActionOutcome::NeedsInput {
                prompt: "Enter tags to add (space-separated): ".into(),
                action_id: "add_tag".into(),
                context: crate::browse::models::ActionContext {
                    files: selected_files,
                    data: crate::browse::models::ActionData::None,
                },
            }),
            BrowseAction::RemoveTag => Ok(ActionOutcome::NeedsInput {
                prompt: "Enter tags to remove (space-separated): ".into(),
                action_id: "remove_tag".into(),
                context: crate::browse::models::ActionContext {
                    files: selected_files,
                    data: crate::browse::models::ActionData::None,
                },
            }),
            BrowseAction::DeleteFromDb => Ok(ActionOutcome::NeedsConfirmation {
                message: format!("Delete {} file(s) from database?", selected_files.len()),
                action_id: "delete_from_db".into(),
                context: crate::browse::models::ActionContext {
                    files: selected_files,
                    data: crate::browse::models::ActionData::None,
                },
            }),
            BrowseAction::OpenInDefault => Ok(actions::execute_open_in_default(&selected_files)),
            BrowseAction::OpenInEditor => {
                let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());
                Ok(actions::execute_open_in_editor(&selected_files, &editor))
            }
            BrowseAction::CopyPath => {
                actions::execute_copy_path(&selected_files).map_err(BrowseError::ActionFailed)
            }
            BrowseAction::CopyFiles => Ok(ActionOutcome::NeedsInput {
                prompt: "Enter destination directory: ".into(),
                action_id: "copy_files".into(),
                context: crate::browse::models::ActionContext {
                    files: selected_files,
                    data: crate::browse::models::ActionData::None,
                },
            }),
            BrowseAction::RefineSearch => {
                // Get current search criteria from config
                let criteria = self.get_current_search_criteria();
                Ok(ActionOutcome::NeedsInput {
                    prompt: "Refine search criteria".into(),
                    action_id: "refine_search".into(),
                    context: crate::browse::models::ActionContext {
                        files: vec![],
                        data: crate::browse::models::ActionData::SearchCriteria(criteria),
                    },
                })
            }
            // Other actions not yet implemented in session layer
            _ => Err(BrowseError::ActionNotAvailable),
        }
    }

    /// Get current search criteria data
    fn get_current_search_criteria(&self) -> crate::browse::models::SearchCriteriaData {
        if let Some(ref params) = self.config.initial_search {
            crate::browse::models::SearchCriteriaData {
                tags: params.tags.clone(),
                exclude_tags: params.exclude_tags.clone(),
                file_patterns: params.file_patterns.clone(),
                virtual_tags: params.virtual_tags.clone(),
            }
        } else if let PhaseType::FileSelection { selected_tags } = &self.current_phase.phase_type {
            crate::browse::models::SearchCriteriaData {
                tags: selected_tags.clone(),
                exclude_tags: vec![],
                file_patterns: vec![],
                virtual_tags: vec![],
            }
        } else {
            crate::browse::models::SearchCriteriaData {
                tags: vec![],
                exclude_tags: vec![],
                file_patterns: vec![],
                virtual_tags: vec![],
            }
        }
    }

    /// Update search parameters and refresh the file list
    ///
    /// # Arguments
    ///
    /// * `new_params` - New search parameters to apply
    ///
    /// # Errors
    ///
    /// Returns error if database queries fail
    pub fn update_search_params(&mut self, new_params: SearchParams) -> Result<()> {
        // Update config with new search params
        self.config.initial_search = Some(new_params.clone());

        // Re-query files with updated criteria
        let items = query::get_matching_files(self.db, &new_params)?;

        // Update current phase
        self.current_phase = BrowserPhase {
            phase_type: PhaseType::FileSelection {
                selected_tags: new_params.tags.clone(),
            },
            items,
            settings: self.config.file_phase_settings.clone(),
        };

        Ok(())
    }

    /// Refresh current phase data
    ///
    /// Reloads items for the current phase from the database. Used after
    /// actions that mutate data (add/remove tags, delete files, etc.)
    ///
    /// # Errors
    ///
    /// Returns error if database queries fail
    pub fn refresh_current_phase(&mut self) -> Result<()> {
        match &self.current_phase.phase_type {
            PhaseType::TagSelection => {
                self.current_phase.items = query::get_available_tags(self.db)?;
            }
            PhaseType::FileSelection { selected_tags } => {
                self.current_phase.items =
                    query::get_files_by_tags(self.db, selected_tags, SearchMode::Any)?;
            }
        }
        Ok(())
    }

    /// Get reference to database
    #[must_use]
    pub const fn db(&self) -> &Database {
        self.db
    }

    /// Get reference to config
    #[must_use]
    pub const fn config(&self) -> &BrowseConfig {
        &self.config
    }

    /// Get current search criteria for the refine search UI
    #[must_use]
    pub fn search_criteria(&self) -> crate::browse::models::SearchCriteriaData {
        self.get_current_search_criteria()
    }

    /// Get all available tags from the database for the refine search UI
    ///
    /// # Errors
    ///
    /// Returns error if database query fails
    pub fn available_tags(&self) -> Result<Vec<String>> {
        self.db.list_all_tags().map_err(Into::into)
    }
}

/// Result of accepting selection in a phase
#[derive(Debug)]
pub enum AcceptResult {
    /// User cancelled (ESC or empty selection)
    Cancelled,

    /// Transitioned to next phase (tags → files)
    PhaseTransition,

    /// No data available in next phase
    NoData,

    /// Session complete with final result
    Complete(BrowseResult),
}

/// Final result from browse session
#[derive(Debug)]
pub struct BrowseResult {
    /// Tags that were selected
    pub selected_tags: Vec<String>,

    /// Files that were selected
    pub selected_files: Vec<PathBuf>,
}

impl Default for BrowseConfig {
    fn default() -> Self {
        Self {
            initial_search: None,
            path_format: PathFormat::Absolute,
            tag_phase_settings: PhaseSettings::default_for_tags(),
            file_phase_settings: PhaseSettings::default_for_files(),
        }
    }
}

impl PhaseSettings {
    /// Default settings for tag selection phase
    #[must_use]
    pub fn default_for_tags() -> Self {
        Self {
            preview_enabled: false,
            preview_config: None,
            keybind_config: KeybindConfig::default(),
            help_text: HelpText::TagBrowser(vec![
                ("Enter".into(), "Select tags and continue".into()),
                ("Tab".into(), "Toggle multi-select mode".into()),
                ("Esc".into(), "Cancel and exit".into()),
                ("F1".into(), "Show this help".into()),
            ]),
        }
    }

    /// Default settings for file selection phase
    #[must_use]
    pub fn default_for_files() -> Self {
        Self {
            preview_enabled: true,
            preview_config: Some(PreviewConfig::default()),
            keybind_config: KeybindConfig::default(),
            help_text: HelpText::FileBrowser(vec![
                ("Enter".into(), "Complete selection".into()),
                ("Tab".into(), "Toggle multi-select mode".into()),
                ("Ctrl+t".into(), "Add tags to selection".into()),
                ("Ctrl+d".into(), "Delete from database".into()),
                ("Ctrl+o".into(), "Open in default app".into()),
                ("Ctrl+e".into(), "Open in editor".into()),
                ("Ctrl+Y".into(), "Copy paths to clipboard".into()),
                ("Ctrl+f".into(), "Copy files to directory".into()),
                ("F2".into(), "Refine search criteria".into()),
                ("Esc".into(), "Cancel and exit".into()),
                ("F1".into(), "Show this help".into()),
            ]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestDb;

    #[test]
    fn test_session_starts_at_tag_phase_when_no_initial_search() {
        let db = TestDb::new("test_session_tag_phase");
        let config = BrowseConfig::default();

        let session = BrowseSession::new(db.db(), config).unwrap();

        assert!(matches!(
            session.current_phase().phase_type,
            PhaseType::TagSelection
        ));
    }

    #[test]
    fn test_session_starts_at_file_phase_with_initial_search() {
        let db = TestDb::new("test_session_file_phase");
        let mut config = BrowseConfig::default();
        config.initial_search = Some(SearchParams {
            query: None,
            tags: vec!["rust".to_string()],
            tag_mode: crate::cli::SearchMode::Any,
            file_patterns: vec![],
            file_mode: crate::cli::SearchMode::All,
            exclude_tags: vec![],
            regex_tag: false,
            regex_file: false,
            glob_files: false,
            virtual_tags: vec![],
            virtual_mode: crate::cli::SearchMode::All,
        });

        let session = BrowseSession::new(db.db(), config).unwrap();

        assert!(matches!(
            session.current_phase().phase_type,
            PhaseType::FileSelection { .. }
        ));
    }

    #[test]
    fn test_handle_accept_empty_selection_cancels() {
        let db = TestDb::new("test_accept_empty");
        let config = BrowseConfig::default();
        let mut session = BrowseSession::new(db.db(), config).unwrap();

        let result = session.handle_accept(vec![]).unwrap();

        assert!(matches!(result, AcceptResult::Cancelled));
    }

    #[test]
    fn test_action_not_available_in_tag_phase() {
        let db = TestDb::new("test_action_tag_phase");
        let config = BrowseConfig::default();
        let session = BrowseSession::new(db.db(), config).unwrap();

        let result = session.execute_action(&BrowseAction::AddTag, &[]);

        assert!(matches!(result, Err(BrowseError::ActionNotAvailable)));
    }

    #[test]
    fn test_update_search_params() {
        use crate::testing::TempFile;
        use crate::Pair;

        let db = TestDb::new("test_update_search_params");
        db.db().clear().unwrap();

        // Create test files
        let file1 = TempFile::create("file1.txt").unwrap();
        let file2 = TempFile::create("file2.txt").unwrap();
        let file3 = TempFile::create("file3.txt").unwrap();

        // Insert test data
        db.db()
            .insert_pair(&Pair::new(
                file1.path().to_path_buf(),
                vec!["rust".into(), "code".into()],
            ))
            .unwrap();
        db.db()
            .insert_pair(&Pair::new(
                file2.path().to_path_buf(),
                vec!["rust".into(), "docs".into()],
            ))
            .unwrap();
        db.db()
            .insert_pair(&Pair::new(
                file3.path().to_path_buf(),
                vec!["python".into()],
            ))
            .unwrap();

        // Create session with initial search for "rust"
        let mut config = BrowseConfig::default();
        config.initial_search = Some(SearchParams {
            query: None,
            tags: vec!["rust".to_string()],
            tag_mode: crate::cli::SearchMode::Any,
            file_patterns: vec![],
            file_mode: crate::cli::SearchMode::All,
            exclude_tags: vec![],
            regex_tag: false,
            regex_file: false,
            glob_files: false,
            virtual_tags: vec![],
            virtual_mode: crate::cli::SearchMode::All,
        });

        let mut session = BrowseSession::new(db.db(), config).unwrap();

        // Should start with 2 files (file1 and file2 have "rust" tag)
        assert_eq!(session.current_phase().items.len(), 2);

        // Update search to exclude "docs" tag
        let new_params = SearchParams {
            query: None,
            tags: vec!["rust".to_string()],
            tag_mode: crate::cli::SearchMode::Any,
            file_patterns: vec![],
            file_mode: crate::cli::SearchMode::All,
            exclude_tags: vec!["docs".to_string()],
            regex_tag: false,
            regex_file: false,
            glob_files: false,
            virtual_tags: vec![],
            virtual_mode: crate::cli::SearchMode::All,
        };

        session.update_search_params(new_params).unwrap();

        // Should now have 1 file (file1 has "rust" but not "docs")
        assert_eq!(session.current_phase().items.len(), 1);
    }

    #[test]
    fn test_refine_search_action_returns_needs_input() {
        use crate::testing::TempFile;
        use crate::Pair;

        let db = TestDb::new("test_refine_search_action");
        db.db().clear().unwrap();

        let file1 = TempFile::create("file1.txt").unwrap();
        db.db()
            .insert_pair(&Pair::new(
                file1.path().to_path_buf(),
                vec!["test".into()],
            ))
            .unwrap();

        // Create session in file phase
        let mut config = BrowseConfig::default();
        config.initial_search = Some(SearchParams {
            query: None,
            tags: vec!["test".to_string()],
            tag_mode: crate::cli::SearchMode::Any,
            file_patterns: vec![],
            file_mode: crate::cli::SearchMode::All,
            exclude_tags: vec![],
            regex_tag: false,
            regex_file: false,
            glob_files: false,
            virtual_tags: vec![],
            virtual_mode: crate::cli::SearchMode::All,
        });

        let session = BrowseSession::new(db.db(), config).unwrap();

        // Execute refine search action
        let result = session.execute_action(&BrowseAction::RefineSearch, &[]).unwrap();

        // Should return NeedsInput with search criteria
        match result {
            crate::browse::models::ActionOutcome::NeedsInput {
                action_id,
                context,
                ..
            } => {
                assert_eq!(action_id, "refine_search");
                match context.data {
                    crate::browse::models::ActionData::SearchCriteria(criteria) => {
                        assert_eq!(criteria.tags, vec!["test".to_string()]);
                    }
                    _ => panic!("Expected SearchCriteria data"),
                }
            }
            _ => panic!("Expected NeedsInput outcome"),
        }
    }
}
