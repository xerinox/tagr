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

use std::sync::Arc;

use crate::browse::models::{ActionOutcome, TagrItem};
use crate::browse::{actions, query};
use crate::config::PreviewConfig;
use crate::keybinds::actions::BrowseAction;
use crate::keybinds::config::KeybindConfig;
use crate::schema::{self, TagSchema};
use crate::store::TagStore;
use crate::types::{MatchMode, QueryCriteria, TagName, TagrPath};

/// Threshold for switching between in-memory and DB filtering
///
/// When the current result set is below this size, we use in-memory filtering
/// for interactive refinement. Above this size, we re-query the database.
const HYBRID_FILTER_THRESHOLD: usize = 5_000;

/// Browse session error type
pub type Result<T> = std::result::Result<T, BrowseError>;

/// Errors that can occur during browse session
#[derive(Debug, thiserror::Error)]
pub enum BrowseError {
    #[error("Store error: {0}")]
    Store(#[from] crate::store::StoreError),

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
pub struct BrowseSession {
    ds: Arc<dyn TagStore>,
    config: BrowseConfig,
    current_phase: BrowserPhase,
    schema: Option<TagSchema>,
    /// Base items for in-memory filtering (when applicable)
    /// This caches the initial DB query result for fast re-filtering
    base_items: Option<Vec<TagrItem>>,
}

/// Configuration for browse session
#[derive(Clone)]
pub struct BrowseConfig {
    /// Initial search criteria (if provided via CLI)
    pub initial_search: Option<QueryCriteria>,

    /// Path display format
    pub path_format: crate::config::PathFormat,

    /// Tag selection phase settings
    pub tag_phase_settings: PhaseSettings,

    /// File selection phase settings
    pub file_phase_settings: PhaseSettings,
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
        selected_tags: Vec<TagName>,
    },
}

impl BrowseSession {
    /// Create new browse session
    ///
    /// Determines starting phase based on `config.initial_search`:
    /// - If `None`: Start with tag selection phase
    /// - If `Some`: Skip to file selection with pre-filtered files
    ///
    /// # Errors
    ///
    /// Returns error if database queries fail
    pub fn new(ds: Arc<dyn TagStore>, config: BrowseConfig) -> Result<Self> {
        let current_phase = if let Some(ref criteria) = config.initial_search {
            let items = query::get_matching_files(&*ds, criteria)?;

            let selected_tags: Vec<TagName> = criteria
                .flat_include_tags()
                .map(|set| set.into_iter().cloned().collect())
                .unwrap_or_default();

            BrowserPhase {
                phase_type: PhaseType::FileSelection {
                    selected_tags,
                },
                items,
                settings: config.file_phase_settings.clone(),
            }
        } else {
            let items = query::get_available_tags(&*ds)?;

            BrowserPhase {
                phase_type: PhaseType::TagSelection,
                items,
                settings: config.tag_phase_settings.clone(),
            }
        };

        Ok(Self {
            ds,
            config,
            current_phase,
            schema: schema::load_default_schema().ok(),
            base_items: None,
        })
    }

    /// Get current browser phase for UI to render
    #[must_use]
    pub const fn current_phase(&self) -> &BrowserPhase {
        &self.current_phase
    }

    /// Get the tag schema (if loaded)
    #[must_use]
    pub const fn schema(&self) -> Option<&TagSchema> {
        self.schema.as_ref()
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
    pub fn handle_accept(&mut self, selected_ids: &[String]) -> Result<AcceptResult> {
        match &self.current_phase.phase_type {
            PhaseType::TagSelection => {
                if selected_ids.is_empty() {
                    return Ok(AcceptResult::Cancelled);
                }

                // NOTES_ONLY_TAG is a special virtual tag for browsing files with notes
                let has_notes_only = selected_ids
                    .iter()
                    .any(|id| id == crate::browse::models::NOTES_ONLY_TAG);

                let items = if has_notes_only && selected_ids.len() == 1 {
                    query::get_notes_only_files(&*self.ds)?
                } else if has_notes_only {
                    let regular_tags: Vec<String> = selected_ids
                        .iter()
                        .filter(|id| *id != crate::browse::models::NOTES_ONLY_TAG)
                        .cloned()
                        .collect();
                    let mut regular_files =
                        query::get_files_by_tags(&*self.ds, &regular_tags, MatchMode::Any)?;
                    let mut notes_files = query::get_notes_only_files(&*self.ds)?;
                    regular_files.append(&mut notes_files);
                    regular_files
                } else {
                    query::get_files_by_tags(&*self.ds, selected_ids, MatchMode::Any)?
                };

                if items.is_empty() {
                    return Ok(AcceptResult::NoData);
                }

                let selected_tags: Vec<TagName> = selected_ids
                    .iter()
                    .filter_map(|id| TagName::new(id).ok())
                    .collect();

                self.current_phase = BrowserPhase {
                    phase_type: PhaseType::FileSelection {
                        selected_tags,
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

                let selected_files: Vec<TagrPath> = self
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
        // In 3-pane mode, phases are virtual - the UI layer manages visibility.
        // This method trusts the UI only calls it with valid actions per current context.

        let selected_files: Vec<TagrPath> = selected_ids
            .iter()
            .filter_map(|id| TagrPath::new(id).ok())
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
            _ => Err(BrowseError::ActionNotAvailable),
        }
    }

    /// Get current search criteria data
    fn get_current_search_criteria(&self) -> crate::browse::models::SearchCriteriaData {
        self.config.initial_search.as_ref().map_or_else(
            || {
                if let PhaseType::FileSelection { selected_tags } = &self.current_phase.phase_type {
                    crate::browse::models::SearchCriteriaData {
                        tags: selected_tags.iter().map(ToString::to_string).collect(),
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
            },
            |criteria| {
                let tags = criteria
                    .flat_include_tags()
                    .map(|set| set.into_iter().map(ToString::to_string).collect())
                    .unwrap_or_default();
                let exclude_tags = criteria
                    .flat_exclude_tags()
                    .map(|set| set.into_iter().map(ToString::to_string).collect())
                    .unwrap_or_default();
                crate::browse::models::SearchCriteriaData {
                    tags,
                    exclude_tags,
                    file_patterns: criteria.file_patterns.clone(),
                    virtual_tags: criteria.virtual_tags.clone(),
                }
            },
        )
    }

    /// Update search parameters with hybrid filtering strategy
    ///
    /// Intelligently decides whether to:
    /// - Use in-memory filtering (fast, when result set < threshold)
    /// - Re-query database (slower, when result set > threshold or filters are relaxed)
    ///
    /// # Strategy
    ///
    /// 1. **Small result sets (< 5,000 items)**: Use in-memory filtering
    ///    - Cache the initial DB query as `base_items`
    ///    - Apply new filters in-memory via `filter_items_in_memory()`
    ///    - No DB queries for refinements
    ///
    /// 2. **Large result sets (≥ 5,000 items)**: Always re-query DB
    ///    - Discard `base_items` cache
    ///    - Every filter change triggers DB query
    ///    - Avoids memory overhead for large datasets
    ///
    /// 3. **Filter relaxation**: Re-query when filters become less restrictive
    ///    - E.g., removing exclude tags or reducing tag count
    ///    - Prevents missing results that were filtered out by DB initially
    ///
    /// # Arguments
    ///
    /// * `new_criteria` - New query criteria to apply
    ///
    /// # Errors
    ///
    /// Returns error if database queries fail
    pub fn update_search_params(&mut self, new_criteria: &QueryCriteria) -> Result<()> {
        let old_criteria = self.config.initial_search.as_ref();

        let filters_relaxed = old_criteria.is_some_and(|old| is_filter_relaxation(old, new_criteria));

        self.config.initial_search = Some(new_criteria.clone());

        let tag_names: Vec<TagName> = new_criteria
            .flat_include_tags()
            .map(|set| set.into_iter().cloned().collect())
            .unwrap_or_default();

        // Use hybrid filtering: DB queries for relaxations, in-memory for restrictions
        if filters_relaxed || self.base_items.is_none() {
            let items = query::get_matching_files(&*self.ds, new_criteria)?;

            if items.len() < HYBRID_FILTER_THRESHOLD {
                self.base_items = Some(items.clone());
            } else {
                self.base_items = None;
            }

            self.current_phase = BrowserPhase {
                phase_type: PhaseType::FileSelection {
                    selected_tags: tag_names,
                },
                items,
                settings: self.config.file_phase_settings.clone(),
            };
        } else if let Some(ref base) = self.base_items {
            let filtered_refs = query::filter_items_in_memory(base, new_criteria);
            let items: Vec<TagrItem> = filtered_refs.into_iter().cloned().collect();

            self.current_phase = BrowserPhase {
                phase_type: PhaseType::FileSelection {
                    selected_tags: tag_names,
                },
                items,
                settings: self.config.file_phase_settings.clone(),
            };
        } else {
            let items = query::get_matching_files(&*self.ds, new_criteria)?;

            self.current_phase = BrowserPhase {
                phase_type: PhaseType::FileSelection {
                    selected_tags: tag_names,
                },
                items,
                settings: self.config.file_phase_settings.clone(),
            };
        }

        Ok(())
    }

    /// Refresh current phase data
    ///
    /// Reloads items for the current phase from the database. Used after
    /// actions that mutate data (add/remove tags, delete files, etc.)
    ///
    /// **Important**: This invalidates the `base_items` cache, forcing the next
    /// filter update to re-query the database. This ensures data consistency
    /// after mutations.
    ///
    /// # Errors
    ///
    /// Returns error if database queries fail
    pub fn refresh_current_phase(&mut self) -> Result<()> {
        // Invalidate cache - data has changed
        self.base_items = None;

        match &self.current_phase.phase_type {
            PhaseType::TagSelection => {
                self.current_phase.items = query::get_available_tags(&*self.ds)?;
            }
            PhaseType::FileSelection { selected_tags } => {
                let tag_strings: Vec<String> =
                    selected_tags.iter().map(ToString::to_string).collect();
                self.current_phase.items =
                    query::get_files_by_tags(&*self.ds, &tag_strings, MatchMode::Any)?;
            }
        }
        Ok(())
    }

    /// Get reference to data source (as Arc for sharing with TUI)
    #[must_use]
    pub fn data_source(&self) -> &Arc<dyn TagStore> {
        &self.ds
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
    pub fn available_tags(&self) -> Result<Vec<TagName>> {
        self.ds
            .list_all_tags()
            .map_err(Into::into)
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
    pub selected_tags: Vec<TagName>,

    /// Files that were selected
    pub selected_files: Vec<TagrPath>,
}

impl Default for BrowseConfig {
    fn default() -> Self {
        Self {
            initial_search: None,
            path_format: crate::config::PathFormat::Absolute,
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

/// Check if new query criteria are less restrictive than old criteria
///
/// Returns `true` if filters are being relaxed, which requires a DB re-query
/// to ensure we don't miss results that were filtered out initially.
///
/// # Filter Relaxation Examples
///
/// - Removing exclude tags (e.g., `-x python` → no excludes)
/// - Reducing number of include tags in ALL mode
/// - Removing file patterns
const fn is_filter_relaxation(old: &QueryCriteria, new: &QueryCriteria) -> bool {
    if new.file_patterns.len() < old.file_patterns.len() {
        return true;
    }

    if new.virtual_tags.len() < old.virtual_tags.len() {
        return true;
    }

    // Comparing tag expressions is complex; conservatively re-query
    // when tag expressions differ
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestDb;

    fn store(db: &TestDb) -> Arc<dyn TagStore> {
        Arc::new(crate::store::DirectStore::new(db.db().clone()))
    }

    #[test]
    fn test_session_starts_at_tag_phase_when_no_initial_search() {
        let db = TestDb::new("test_session_tag_phase");
        let config = BrowseConfig::default();

        let session = BrowseSession::new(store(&db), config).unwrap();

        assert!(matches!(
            session.current_phase().phase_type,
            PhaseType::TagSelection
        ));
    }

    #[test]
    fn test_session_starts_at_file_phase_with_initial_search() {
        let db = TestDb::new("test_session_file_phase");
        let config = BrowseConfig {
            initial_search: Some(QueryCriteria {
                tag_expr: Some(crate::types::TagExpr::Tag(TagName::new("rust").unwrap())),
                ..QueryCriteria::default()
            }),
            ..Default::default()
        };

        let session = BrowseSession::new(store(&db), config).unwrap();

        assert!(matches!(
            session.current_phase().phase_type,
            PhaseType::FileSelection { .. }
        ));
    }

    #[test]
    fn test_handle_accept_empty_selection_cancels() {
        let db = TestDb::new("test_accept_empty");
        let config = BrowseConfig::default();
        let mut session = BrowseSession::new(store(&db), config).unwrap();

        let result = session.handle_accept(&[]).unwrap();

        assert!(matches!(result, AcceptResult::Cancelled));
    }

    // NOTE: test_action_not_available_in_tag_phase removed - phases don't exist in 3-pane view
    // Pane-focused filtering happens at UI layer, session layer trusts the UI

    #[test]
    fn test_update_search_params() {
        use crate::Pair;
        use crate::testing::TempFile;

        let db = TestDb::new("test_update_search_params");
        db.db().clear().unwrap();

        let file1 = TempFile::create("file1.txt").unwrap();
        let file2 = TempFile::create("file2.txt").unwrap();
        let file3 = TempFile::create("file3.txt").unwrap();

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

        let config = BrowseConfig {
            initial_search: Some(QueryCriteria {
                tag_expr: Some(crate::types::TagExpr::Tag(TagName::new("rust").unwrap())),
                ..QueryCriteria::default()
            }),
            ..Default::default()
        };

        let mut session = BrowseSession::new(store(&db), config).unwrap();

        assert_eq!(session.current_phase().items.len(), 2);

        let new_criteria = QueryCriteria {
            tag_expr: Some(crate::types::TagExpr::And(vec![
                crate::types::TagExpr::Tag(TagName::new("rust").unwrap()),
                crate::types::TagExpr::Not(Box::new(crate::types::TagExpr::Tag(TagName::new("docs").unwrap()))),
            ])),
            ..QueryCriteria::default()
        };

        session.update_search_params(&new_criteria).unwrap();

        assert_eq!(session.current_phase().items.len(), 1);
    }

    #[test]
    fn test_refine_search_action_returns_needs_input() {
        use crate::Pair;
        use crate::testing::TempFile;

        let db = TestDb::new("test_refine_search_action");
        db.db().clear().unwrap();

        let file1 = TempFile::create("file1.txt").unwrap();
        db.db()
            .insert_pair(&Pair::new(file1.path().to_path_buf(), vec!["test".into()]))
            .unwrap();

        let config = BrowseConfig {
            initial_search: Some(QueryCriteria {
                tag_expr: Some(crate::types::TagExpr::Tag(TagName::new("test").unwrap())),
                ..QueryCriteria::default()
            }),
            ..Default::default()
        };

        let session = BrowseSession::new(store(&db), config).unwrap();

        let result = session
            .execute_action(&BrowseAction::RefineSearch, &[])
            .unwrap();

        // Should return NeedsInput with search criteria
        match result {
            crate::browse::models::ActionOutcome::NeedsInput {
                action_id, context, ..
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

    #[test]
    fn test_is_filter_relaxation_file_patterns() {
        let old = QueryCriteria {
            file_patterns: vec!["*.rs".into(), "*.toml".into()],
            ..QueryCriteria::default()
        };

        let new = QueryCriteria {
            file_patterns: vec!["*.rs".into()],
            ..QueryCriteria::default()
        };

        assert!(is_filter_relaxation(&old, &new));
    }

    #[test]
    fn test_is_filter_relaxation_virtual_tags() {
        let old = QueryCriteria {
            virtual_tags: vec!["size:>1MB".into(), "modified:today".into()],
            ..QueryCriteria::default()
        };

        let new = QueryCriteria {
            virtual_tags: vec!["size:>1MB".into()],
            ..QueryCriteria::default()
        };

        assert!(is_filter_relaxation(&old, &new));
    }

    #[test]
    fn test_is_filter_relaxation_no_relaxation() {
        let old = QueryCriteria {
            tag_expr: Some(crate::types::TagExpr::Tag(TagName::new("rust").unwrap())),
            ..QueryCriteria::default()
        };

        let new = QueryCriteria {
            tag_expr: Some(crate::types::TagExpr::And(vec![
                crate::types::TagExpr::Tag(TagName::new("rust").unwrap()),
                crate::types::TagExpr::Tag(TagName::new("web").unwrap()),
            ])),
            ..QueryCriteria::default()
        };

        assert!(!is_filter_relaxation(&old, &new));
    }

    #[test]
    fn test_hybrid_filtering_small_result_set() {
        use crate::Pair;
        use crate::testing::TempFile;

        let db = TestDb::new("test_hybrid_small");
        db.db().clear().unwrap();

        // Create 10 files (well below threshold)
        for i in 0..10 {
            let file = TempFile::create(format!("file{i}.txt")).unwrap();
            db.db()
                .insert_pair(&Pair::new(
                    file.path().to_path_buf(),
                    vec!["rust".into(), format!("tag{i}")],
                ))
                .unwrap();
        }

        let config = BrowseConfig {
            initial_search: Some(QueryCriteria {
                tag_expr: Some(crate::types::TagExpr::Tag(TagName::new("rust").unwrap())),
                ..QueryCriteria::default()
            }),
            ..Default::default()
        };

        let mut session = BrowseSession::new(store(&db), config).unwrap();
        assert_eq!(session.current_phase().items.len(), 10);

        // First refinement should cache base_items
        assert!(session.base_items.is_none()); // Not cached yet (created in constructor)

        // Add exclude tag (more restrictive, should use in-memory filtering after first update)
        let new_criteria = QueryCriteria {
            tag_expr: Some(crate::types::TagExpr::And(vec![
                crate::types::TagExpr::Tag(TagName::new("rust").unwrap()),
                crate::types::TagExpr::Not(Box::new(crate::types::TagExpr::Tag(TagName::new("tag1").unwrap()))),
            ])),
            ..QueryCriteria::default()
        };

        session.update_search_params(&new_criteria).unwrap();

        // Should have cached base_items (result set < threshold)
        assert!(session.base_items.is_some());
        assert_eq!(session.current_phase().items.len(), 9); // Excluded 1 file
    }

    #[test]
    fn test_hybrid_filtering_detects_relaxation() {
        use crate::Pair;
        use crate::testing::TempFile;

        let db = TestDb::new("test_hybrid_relaxation");
        db.db().clear().unwrap();

        let file1 = TempFile::create("file1.txt").unwrap();
        let file2 = TempFile::create("file2.txt").unwrap();

        db.db()
            .insert_pair(&Pair::new(
                file1.path().to_path_buf(),
                vec!["rust".into(), "web".into()],
            ))
            .unwrap();

        db.db()
            .insert_pair(&Pair::new(
                file2.path().to_path_buf(),
                vec!["rust".into(), "cli".into()],
            ))
            .unwrap();

        let config = BrowseConfig {
            initial_search: Some(QueryCriteria {
                tag_expr: Some(crate::types::TagExpr::And(vec![
                    crate::types::TagExpr::Tag(TagName::new("rust").unwrap()),
                    crate::types::TagExpr::Not(Box::new(crate::types::TagExpr::Tag(TagName::new("cli").unwrap()))),
                ])),
                ..QueryCriteria::default()
            }),
            ..Default::default()
        };

        let mut session = BrowseSession::new(store(&db), config).unwrap();
        assert_eq!(session.current_phase().items.len(), 1); // file1 only

        // Remove exclude tag (change criteria) — force re-query via DB
        let new_criteria = QueryCriteria {
            tag_expr: Some(crate::types::TagExpr::Tag(TagName::new("rust").unwrap())),
            ..QueryCriteria::default()
        };

        session.update_search_params(&new_criteria).unwrap();

        // Should have both files now (re-queried DB)
        assert_eq!(session.current_phase().items.len(), 2);
    }

    #[test]
    fn test_refresh_invalidates_cache() {
        use crate::Pair;
        use crate::testing::TempFile;

        let db = TestDb::new("test_refresh_cache");
        db.db().clear().unwrap();

        let file1 = TempFile::create("file1.txt").unwrap();
        db.db()
            .insert_pair(&Pair::new(file1.path().to_path_buf(), vec!["rust".into()]))
            .unwrap();

        let config = BrowseConfig {
            initial_search: Some(QueryCriteria {
                tag_expr: Some(crate::types::TagExpr::Tag(TagName::new("rust").unwrap())),
                ..QueryCriteria::default()
            }),
            ..Default::default()
        };

        let mut session = BrowseSession::new(store(&db), config).unwrap();

        // Simulate cache being set
        session.base_items = Some(session.current_phase().items.clone());
        assert!(session.base_items.is_some());

        // Refresh should invalidate cache
        session.refresh_current_phase().unwrap();
        assert!(session.base_items.is_none());
    }
}
