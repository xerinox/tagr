//! Interactive browse functionality
//!
//! Provides a two-stage fuzzy finder interface:
//! 1. Select tags using fuzzy finder (multi-select supported)
//! 2. View and select files matching those tags (multi-select supported)
//!
//! Uses abstracted UI layer for easy backend swapping.
//!
//! # Note
//!
//! This module contains the legacy function-based API. For new code,
//! prefer using `BrowseState` with the builder pattern from `super::state`.

use super::error::SearchError;
pub use super::state::BrowseResult;
use crate::cli::SearchParams;
use crate::config::PathFormat;
use crate::db::{Database, query};
use crate::keybinds::{ActionExecutor, ActionContext, ActionResult, BrowseAction, KeybindConfig};
use crate::preview::FilePreviewProvider;
use crate::ui::{
    DisplayItem, FinderConfig, FuzzyFinder, ItemMetadata, PreviewConfig,
    skim_adapter::SkimFinder,
};
use colored::Colorize;
use std::path::{Path, PathBuf};

/// Result from file selection with keybind detection
struct FileSelectionResult {
    selected_files: Vec<PathBuf>,
    final_key: Option<String>,
}

fn format_path_for_display(path: &Path, format: PathFormat) -> String {
    match format {
        PathFormat::Absolute => path.display().to_string(),
        PathFormat::Relative => {
            if let Ok(cwd) = std::env::current_dir()
                && let Ok(rel_path) = path.strip_prefix(&cwd)
            {
                return rel_path.display().to_string();
            }
            path.display().to_string()
        }
    }
}

/// Run interactive browse mode
///
/// This function provides a two-stage fuzzy finder interface:
/// 1. First, user selects one or more tags from all available tags
/// 2. Then, user selects one or more files from files matching those tags
///
/// # Arguments
/// * `db` - The database to query
/// * `path_format` - Format to use for displaying file paths
/// * `keybind_config` - Optional keybind configuration
///
/// # Returns
/// * `Ok(Some(BrowseResult))` - User made selections and confirmed
/// * `Ok(None)` - User cancelled the operation
/// * `Err(SearchError)` - An error occurred during the operation
///
/// # Errors
///
/// Returns `SearchError` if database operations fail, UI interactions fail,
/// or if skim selection is interrupted.
pub fn browse(db: &Database, path_format: PathFormat, keybind_config: Option<&KeybindConfig>) -> Result<Option<BrowseResult>, SearchError> {
    browse_with_params(db, None, None, path_format, keybind_config)
}

/// Run interactive browse mode with optional pre-populated search parameters
///
/// This function provides a two-stage fuzzy finder interface with optional
/// pre-filtering based on search parameters:
/// 1. If search params are provided, skip tag selection and use those filters
/// 2. Otherwise, user selects one or more tags from all available tags
/// 3. Then, user selects one or more files from files matching the criteria
///
/// # Arguments
/// * `db` - The database to query
/// * `search_params` - Optional search parameters to pre-populate the browse
/// * `preview_overrides` - Optional preview configuration overrides
/// * `path_format` - Format to use for displaying file paths
/// * `keybind_config` - Optional keybind configuration
///
/// # Returns
/// * `Ok(Some(BrowseResult))` - User made selections and confirmed
/// * `Ok(None)` - User cancelled the operation
/// * `Err(SearchError)` - An error occurred during the operation
///
/// # Errors
///
/// Returns `SearchError` if database operations fail, UI interactions fail,
/// or if skim selection is interrupted.
pub fn browse_with_params(
    db: &Database,
    search_params: Option<SearchParams>,
    preview_overrides: Option<crate::cli::PreviewOverrides>,
    path_format: PathFormat,
    keybind_config: Option<&KeybindConfig>,
) -> Result<Option<BrowseResult>, SearchError> {
    browse_with_params_and_actions(db, search_params, preview_overrides, path_format, false, keybind_config)
}

/// Run interactive browse mode with keybind actions enabled
///
/// Experimental feature that adds an action menu after file selection.
/// This is Phase 1 of keybind integration.
pub fn browse_with_actions(
    db: &Database,
    search_params: Option<SearchParams>,
    preview_overrides: Option<crate::cli::PreviewOverrides>,
    path_format: PathFormat,
    keybind_config: Option<&KeybindConfig>,
) -> Result<Option<BrowseResult>, SearchError> {
    browse_with_params_and_actions(db, search_params, preview_overrides, path_format, true, keybind_config)
}

/// Run action menu on already-selected files
///
/// This is a simpler wrapper for when files have already been selected
/// and you just want to show the action menu.
pub fn show_actions_for_files(db: &Database, files: Vec<PathBuf>, keybind_config: Option<&KeybindConfig>) -> Result<(), SearchError> {
    loop {
        if !show_action_menu(db, &files, keybind_config)? {
            return Ok(());
        }
    }
}

/// Run interactive browse mode with optional actions enabled
///
/// Internal function that adds action menu support.
fn browse_with_params_and_actions(
    db: &Database,
    search_params: Option<SearchParams>,
    preview_overrides: Option<crate::cli::PreviewOverrides>,
    path_format: PathFormat,
    enable_actions: bool,
    keybind_config: Option<&KeybindConfig>,
) -> Result<Option<BrowseResult>, SearchError> {
    let (selected_tags, matching_files) = if let Some(params) = search_params {
        let files = query::apply_search_params(db, &params).map_err(SearchError::DatabaseError)?;
        (params.tags, files)
    } else {
        let selected_tags = select_tags(db, keybind_config)?;

        if selected_tags.is_empty() {
            return Ok(None);
        }

        let files = db.find_by_any_tag(&selected_tags)?;
        (selected_tags, files)
    };

    if matching_files.is_empty() {
        eprintln!("No files found matching the criteria");
        return Ok(None);
    }

    loop {
        let selected_files = select_files_from_list(db, &matching_files, preview_overrides.clone(), path_format)?;

        if selected_files.is_empty() {
            return Ok(None);
        }

        if enable_actions {
            let should_retry = show_action_menu(db, &selected_files, keybind_config)?;
            if should_retry {
                continue;
            }
        }

        return Ok(Some(BrowseResult {
            selected_tags,
            selected_files,
        }));
    }
}

/// Run interactive browse mode with real-time keybind support
///
/// Phase 1B implementation: Uses skim's native keybind support via --bind
/// and detects which action was triggered via final_key in SkimOutput.
///
/// # Arguments
/// * `db` - The database to query
/// * `search_params` - Optional search parameters to pre-populate the browse
/// * `preview_overrides` - Optional preview configuration overrides
/// * `path_format` - Format to use for displaying file paths
/// * `keybind_config` - Keybind configuration with action mappings
///
/// # Returns
/// * `Ok(Some(BrowseResult))` - User made selections and confirmed
/// * `Ok(None)` - User cancelled the operation
/// * `Err(SearchError)` - An error occurred during the operation
///
/// # Errors
///
/// Returns `SearchError` if database operations fail, UI interactions fail,
/// or if skim selection is interrupted.
pub fn browse_with_realtime_keybinds(
    db: &Database,
    search_params: Option<SearchParams>,
    preview_overrides: Option<crate::cli::PreviewOverrides>,
    path_format: PathFormat,
    keybind_config: &KeybindConfig,
) -> Result<Option<BrowseResult>, SearchError> {
    // Get the files to browse
    let (selected_tags, matching_files) = if let Some(params) = search_params {
        let files = query::apply_search_params(db, &params).map_err(SearchError::DatabaseError)?;
        (params.tags, files)
    } else {
        let selected_tags = select_tags(db, Some(keybind_config))?;

        if selected_tags.is_empty() {
            return Ok(None);
        }

        let files = db.find_by_any_tag(&selected_tags)?;
        (selected_tags, files)
    };

    if matching_files.is_empty() {
        eprintln!("No files found matching the criteria");
        return Ok(None);
    }

    // Convert keybinds to skim format
    let skim_bindings = keybind_config.to_skim_bindings();
    
    // Main loop: select files → detect action → execute → loop back or exit
    loop {
        // Select files with keybinds enabled
        let result = select_files_from_list_with_keybinds(
            db,
            &matching_files,
            preview_overrides.clone(),
            path_format,
            &skim_bindings,
        )?;

        // Check which key was pressed
        if let Some(ref key_str) = result.final_key {
            // If Enter was pressed, return the selections (even if empty = abort)
            if key_str == "enter" {
                if result.selected_files.is_empty() {
                    return Ok(None);
                }
                return Ok(Some(BrowseResult {
                    selected_tags: selected_tags.clone(),
                    selected_files: result.selected_files,
                }));
            }

            // Map key to action
            if let Some(action_name) = keybind_config.action_for_key(key_str)
                && let Some(action) = action_name_to_enum(&action_name)
            {
                    // Execute the action
                    let context = ActionContext {
                        db,
                        selected_files: &result.selected_files,
                        current_file: result.selected_files.first(),
                    };

                    let executor = ActionExecutor::new();
                    let action_result = executor.execute(&action, &context)
                        .map_err(|e| SearchError::UiError(crate::ui::UiError::BuildError(e.to_string())))?;

                    match action_result {
                        ActionResult::Continue => {
                            // Continue browsing - loop back to file selection
                            continue;
                        }
                        ActionResult::Refresh => {
                            // Loop back to file selection
                            continue;
                        }
                        ActionResult::Exit(files) => {
                            return Ok(Some(BrowseResult {
                                selected_tags,
                                selected_files: files,
                            }));
                        }
                        ActionResult::Message(msg) => {
                            eprintln!("{msg}");
                            continue;
                        }
                    }
                } else {
                    // Unknown key - if no files selected, treat as abort
                    if result.selected_files.is_empty() {
                        return Ok(None);
                    }
                }
        } else {
            // No final_key detected - user aborted (ESC)
            return Ok(None);
        }

        // Fallback: if no action detected but Enter pressed with selection, return it
        if !result.selected_files.is_empty() {
            return Ok(Some(BrowseResult {
                selected_tags,
                selected_files: result.selected_files,
            }));
        }
        
        // Empty selection with unknown key - abort
        return Ok(None);
    }
}

/// Apply search parameters to filter files from the database
///
/// Handles the complex logic of combining general query, tags, file patterns,
/// and exclusions to produce a final list of matching files.
///
/// # Arguments
/// * `db` - Database to query
/// * `params` - Search parameters with tags, patterns, and exclusions
///
/// # Returns
/// * Vector of file paths matching all criteria
///
/// # Errors
///
/// Show fuzzy finder for tag selection (multi-select enabled)
///
/// Displays all available tags in an interactive fuzzy finder, allowing
/// the user to select multiple tags using TAB.
///
/// # Arguments
/// * `db` - Database to query for available tags
/// * `keybind_config` - Optional keybind configuration
///
/// # Returns
/// * Empty vector if no tags exist or user cancelled
/// * Vector of selected tag strings if user confirmed selection
///
/// # Errors
///
/// Returns `SearchError::DatabaseError` if tag listing fails or
/// `SearchError::InterruptedError` if the fuzzy finder is interrupted.
fn select_tags(db: &Database, keybind_config: Option<&KeybindConfig>) -> Result<Vec<String>, SearchError> {
    let all_tags = db.list_all_tags()?;

    if all_tags.is_empty() {
        eprintln!("No tags found in database");
        return Ok(Vec::new());
    }

    // Loop to handle keybind actions in tag selection
    loop {
        let items: Vec<DisplayItem> = all_tags
            .iter()
            .enumerate()
            .map(|(idx, tag)| {
                DisplayItem::with_metadata(
                    tag.clone(),
                    tag.clone(),
                    tag.clone(),
                    ItemMetadata {
                        tags: vec![],
                        exists: true,
                        index: Some(idx),
                    },
                )
            })
            .collect();

        let mut config = FinderConfig::new(
            items,
            "Select tags (TAB to select multiple, Enter to confirm): ".to_string(),
        )
        .with_multi_select(true);

        // Add keybinds if provided
        if let Some(keybind_cfg) = keybind_config {
            config = config.with_binds(keybind_cfg.to_skim_bindings());
        }

        let finder = SkimFinder::new();
        let result = finder.run(config).map_err(SearchError::UiError)?;

        if result.aborted {
            return Ok(Vec::new());
        }

        // Check if a keybind action was triggered
        if let Some(keybind_cfg) = keybind_config
            && let Some(ref key_str) = result.final_key
            && key_str != "enter"
        {
            // Map key to action
            if let Some(action_name) = keybind_cfg.action_for_key(key_str)
                && let Some(action) = action_name_to_enum(&action_name)
            {
                // Execute the action (with empty file context for tag selection)
                let context = ActionContext {
                    db,
                    selected_files: &[],
                    current_file: None,
                };

                let executor = ActionExecutor::new();
                match executor.execute(&action, &context) {
                    Ok(ActionResult::Continue) | Ok(ActionResult::Refresh) => {
                        // Loop back to tag selection
                        continue;
                    }
                    Ok(ActionResult::Message(msg)) => {
                        eprintln!("{msg}");
                        continue;
                    }
                    Ok(ActionResult::Exit(_)) => {
                        // Exit tag selection
                        return Ok(Vec::new());
                    }
                    Err(e) => {
                        eprintln!("❌ Action failed: {e}");
                        continue;
                    }
                }
            }
        }

        // Normal selection (Enter key or no keybind matched)
        return Ok(result.selected);
    }
}

/// Select files from a pre-filtered list (consolidated file selection logic)
///
/// Displays files with their associated tags in a fuzzy finder. Files are
/// color-coded (green for existing, red for missing).
///
/// # Arguments
/// * `db` - Database to query for file tags
/// * `files` - Pre-filtered list of files to display
/// * `path_format` - Format to use for displaying file paths
///
/// # Returns
/// * Empty vector if no files provided or user cancelled
/// * Vector of selected file paths if user confirmed selection
///
/// # Errors
///
/// Returns `SearchError::DatabaseError` if tag lookup fails or
/// `SearchError::InterruptedError` if the fuzzy finder is interrupted.
/// Select files from list with keybind detection enabled
fn select_files_from_list_with_keybinds(
    db: &Database,
    files: &[PathBuf],
    preview_overrides: Option<crate::cli::PreviewOverrides>,
    path_format: PathFormat,
    skim_bindings: &[String],
) -> Result<FileSelectionResult, SearchError> {
    if files.is_empty() {
        eprintln!("No files found with the selected tags");
        return Ok(FileSelectionResult {
            selected_files: Vec::new(),
            final_key: None,
        });
    }

    // Start with default preview config
    let mut preview_config = PreviewConfig::default();
    
    // Apply preview overrides from CLI
    if let Some(overrides) = preview_overrides {
        if overrides.no_preview {
            preview_config.enabled = false;
        }
        if let Some(lines) = overrides.preview_lines {
            preview_config.max_lines = lines;
        }
        if let Some(position) = overrides.preview_position {
            preview_config.position = match position.to_lowercase().as_str() {
                "right" => crate::ui::PreviewPosition::Right,
                "bottom" => crate::ui::PreviewPosition::Bottom,
                "top" => crate::ui::PreviewPosition::Top,
                _ => preview_config.position,
            };
        }
        if let Some(width) = overrides.preview_width {
            preview_config.width_percent = width.min(100);
        }
    }

    let items: Vec<DisplayItem> = files
        .iter()
        .enumerate()
        .filter_map(|(idx, file)| {
            let tags = db.get_tags(file).ok()??;
            let display_path = format_path_for_display(file, path_format);
            let exists = file.exists();
            
            let file_colored = if exists {
                display_path.green()
            } else {
                display_path.red()
            };
            let display = format!("{} [{}]", file_colored, tags.join(", "));

            Some(DisplayItem::with_metadata(
                file.to_string_lossy().to_string(),
                display,
                display_path,
                ItemMetadata {
                    tags,
                    exists,
                    index: Some(idx),
                },
            ))
        })
        .collect();

    let config = FinderConfig::new(
        items,
        "Select files (TAB to select multiple, Enter to confirm): ".to_string(),
    )
    .with_multi_select(true)
    .with_ansi(true)
    .with_preview(preview_config.clone())
    .with_binds(skim_bindings.to_vec());

    // Create preview provider for file previews
    let preview_provider = FilePreviewProvider::new(preview_config);
    let finder = SkimFinder::with_preview_provider(preview_provider);
    let result = finder.run(config).map_err(SearchError::UiError)?;

    if result.aborted {
        return Ok(FileSelectionResult {
            selected_files: Vec::new(),
            final_key: result.final_key,
        });
    }

    let selected_files: Vec<PathBuf> = result
        .selected
        .iter()
        .map(PathBuf::from)
        .collect();

    Ok(FileSelectionResult {
        selected_files,
        final_key: result.final_key,
    })
}

fn select_files_from_list(
    db: &Database,
    files: &[PathBuf],
    preview_overrides: Option<crate::cli::PreviewOverrides>,
    path_format: PathFormat,
) -> Result<Vec<PathBuf>, SearchError> {
    if files.is_empty() {
        eprintln!("No files found with the selected tags");
        return Ok(Vec::new());
    }

    // Start with default preview config
    let mut preview_config = PreviewConfig::default();
    
    // Apply preview overrides from CLI
    if let Some(overrides) = preview_overrides {
        if overrides.no_preview {
            preview_config.enabled = false;
        }
        if let Some(lines) = overrides.preview_lines {
            preview_config.max_lines = lines;
        }
        if let Some(position) = overrides.preview_position {
            preview_config.position = match position.to_lowercase().as_str() {
                "right" => crate::ui::PreviewPosition::Right,
                "bottom" => crate::ui::PreviewPosition::Bottom,
                "top" => crate::ui::PreviewPosition::Top,
                _ => preview_config.position,
            };
        }
        if let Some(width) = overrides.preview_width {
            preview_config.width_percent = width.min(100);
        }
    }

    let items: Vec<DisplayItem> = files
        .iter()
        .enumerate()
        .filter_map(|(idx, file)| {
            let tags = db.get_tags(file).ok()??;
            let display_path = format_path_for_display(file, path_format);
            let exists = file.exists();
            
            let file_colored = if exists {
                display_path.green()
            } else {
                display_path.red()
            };
            let display = format!("{} [{}]", file_colored, tags.join(", "));

            Some(DisplayItem::with_metadata(
                file.to_string_lossy().to_string(),
                display,
                display_path,
                ItemMetadata {
                    tags,
                    exists,
                    index: Some(idx),
                },
            ))
        })
        .collect();

    let config = FinderConfig::new(
        items,
        "Select files (TAB to select multiple, Enter to confirm): ".to_string(),
    )
    .with_multi_select(true)
    .with_ansi(true)
    .with_preview(preview_config.clone());

    // Create preview provider for file previews
    let preview_provider = FilePreviewProvider::new(preview_config);
    let finder = SkimFinder::with_preview_provider(preview_provider);
    let result = finder.run(config).map_err(SearchError::UiError)?;

    if result.aborted {
        return Ok(Vec::new());
    }

    let selected_files: Vec<PathBuf> = result
        .selected
        .iter()
        .map(PathBuf::from)
        .collect();

    Ok(selected_files)
}

/// Show action menu for selected files
///
/// After files are selected, present an optional action menu for quick operations.
/// This is a simplified Phase 1 approach - full keybind integration will come later.
///
/// Returns true to continue browsing, false to exit with selections.
fn show_action_menu(
    db: &Database,
    selected_files: &[PathBuf],
    keybind_config: Option<&KeybindConfig>,
) -> Result<bool, SearchError> {
    let actions = [
        "Continue (use selections)",
        "Add tags to selected files",
        "Remove tags from selected files",
        "Delete from database",
        "Cancel (re-select)",
    ];
    
    let items: Vec<DisplayItem> = actions
        .iter()
        .enumerate()
        .map(|(idx, action)| {
            DisplayItem::with_metadata(
                (*action).to_string(),
                (*action).to_string(),
                (*action).to_string(),
                ItemMetadata {
                    tags: vec![],
                    exists: true,
                    index: Some(idx),
                },
            )
        })
        .collect();

    let mut config = FinderConfig::new(items, format!("Action for {} file(s): ", selected_files.len()));

    // Add keybinds if provided
    if let Some(keybind_cfg) = keybind_config {
        config = config.with_binds(keybind_cfg.to_skim_bindings());
    }

    let finder = SkimFinder::new();
    let result = finder.run(config).map_err(SearchError::UiError)?;

    if result.aborted || result.selected.is_empty() {
        return Ok(false);
    }

    let selection = &result.selected[0];
    let executor = ActionExecutor::new();
    
    let action_result = match selection.as_str() {
        s if s.starts_with("Add tags") => {
            let context = ActionContext {
                selected_files,
                current_file: None,
                db,
            };
            executor.execute(&BrowseAction::AddTag, &context)
        }
        s if s.starts_with("Remove tags") => {
            let context = ActionContext {
                selected_files,
                current_file: None,
                db,
            };
            executor.execute(&BrowseAction::RemoveTag, &context)
        }
        s if s.starts_with("Delete from") => {
            let context = ActionContext {
                selected_files,
                current_file: None,
                db,
            };
            executor.execute(&BrowseAction::DeleteFromDb, &context)
        }
        s if s.starts_with("Cancel") => return Ok(true),
        _ => return Ok(false),
    };

    match action_result {
        Ok(ActionResult::Message(msg)) => {
            println!("\n{msg}");
            std::thread::sleep(std::time::Duration::from_secs(1));
            Ok(true)
        }
        Ok(ActionResult::Continue) => Ok(false),
        Ok(ActionResult::Refresh) => Ok(true),
        Ok(ActionResult::Exit(_)) => Ok(false),
        Err(e) => {
            eprintln!("\n❌ Action failed: {e}");
            std::thread::sleep(std::time::Duration::from_secs(2));
            Ok(true)
        }
    }
}

/// Advanced search with AND/OR logic for tag filtering
///
/// Provides an enhanced browse experience with two search modes:
/// - ANY: Files matching any of the selected tags (OR operation)
/// - ALL: Files matching all of the selected tags (AND operation)
///
/// # Arguments
/// * `db` - The database to query
/// * `path_format` - Format to use for displaying file paths
/// * `keybind_config` - Optional keybind configuration
///
/// # Returns
/// * `Ok(Some(BrowseResult))` - User made selections and confirmed
/// * `Ok(None)` - User cancelled or no files matched
/// * `Err(SearchError)` - An error occurred
///
/// # Errors
///
/// Returns `SearchError` if database operations fail, UI interactions fail,
/// or if skim selection is interrupted.
pub fn browse_advanced(
    db: &Database,
    path_format: PathFormat,
    keybind_config: Option<&KeybindConfig>,
) -> Result<Option<BrowseResult>, SearchError> {
    let selected_tags = select_tags(db, keybind_config)?;

    if selected_tags.is_empty() {
        return Ok(None);
    }

    let use_and_logic = if selected_tags.len() > 1 {
        select_search_mode()?
    } else {
        false
    };

    let matching_files = if use_and_logic {
        db.find_by_all_tags(&selected_tags)?
    } else {
        db.find_by_any_tag(&selected_tags)?
    };

    if matching_files.is_empty() {
        eprintln!("No files found with the selected tags");
        return Ok(None);
    }

    let selected_files = select_files_from_list(db, &matching_files, None, path_format)?;

    if selected_files.is_empty() {
        return Ok(None);
    }

    Ok(Some(BrowseResult {
        selected_tags,
        selected_files,
    }))
}

/// Let user choose between AND/OR search mode
///
/// Presents a simple choice between:
/// - ANY: Files with any of the selected tags (OR operation)
/// - ALL: Files with all of the selected tags (AND operation)
///
/// # Returns
/// * `true` if user selected ALL (AND logic)
/// * `false` if user selected ANY (OR logic) or cancelled
///
/// # Errors
///
/// Returns `SearchError::BuildError` if skim options cannot be built or
/// `SearchError::InterruptedError` if the fuzzy finder is interrupted.
fn select_search_mode() -> Result<bool, SearchError> {
    let options = ["ANY (files with any of these tags)", "ALL (files with all of these tags)"];
    
    let items: Vec<DisplayItem> = options
        .iter()
        .enumerate()
        .map(|(idx, opt)| {
            DisplayItem::with_metadata(
                (*opt).to_string(),
                (*opt).to_string(),
                (*opt).to_string(),
                ItemMetadata {
                    tags: vec![],
                    exists: true,
                    index: Some(idx),
                },
            )
        })
        .collect();

    let config = FinderConfig::new(items, "Search mode: ".to_string());

    let finder = SkimFinder::new();
    let result = finder.run(config).map_err(SearchError::UiError)?;

    if result.aborted || result.selected.is_empty() {
        return Ok(false);
    }

    let selection = &result.selected[0];
    Ok(selection.starts_with("ALL"))
}

/// Map action name to BrowseAction enum
fn action_name_to_enum(action: &str) -> Option<BrowseAction> {
    match action {
        "add_tag" => Some(BrowseAction::AddTag),
        "remove_tag" => Some(BrowseAction::RemoveTag),
        "edit_tags" => Some(BrowseAction::EditTags),
        "delete_from_db" => Some(BrowseAction::DeleteFromDb),
        "open_default" => Some(BrowseAction::OpenInDefault),
        "open_editor" => Some(BrowseAction::OpenInEditor),
        "copy_path" => Some(BrowseAction::CopyPath),
        "copy_files" => Some(BrowseAction::CopyFiles),
        "toggle_tag_display" => Some(BrowseAction::ToggleTagDisplay),
        "show_details" => Some(BrowseAction::ShowDetails),
        "filter_extension" => Some(BrowseAction::FilterExtension),
        "select_all" => Some(BrowseAction::SelectAll),
        "clear_selection" => Some(BrowseAction::ClearSelection),
        "quick_search" => Some(BrowseAction::QuickTagSearch),
        "goto_file" => Some(BrowseAction::GoToFile),
        "show_history" => Some(BrowseAction::ShowHistory),
        "bookmark_selection" => Some(BrowseAction::BookmarkSelection),
        "show_help" => Some(BrowseAction::ShowHelp),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{TempFile, TestDb};

    #[test]
    fn test_browse_result_creation() {
        let result = BrowseResult {
            selected_tags: vec!["rust".to_string(), "programming".to_string()],
            selected_files: vec![PathBuf::from("file1.txt"), PathBuf::from("file2.txt")],
        };

        assert_eq!(result.selected_tags.len(), 2);
        assert_eq!(result.selected_files.len(), 2);
    }

    #[test]
    fn test_with_empty_database() {
        let test_db = TestDb::new("test_db_search_empty");
        let db = test_db.db();

        let tags = db.list_all_tags().unwrap();
        assert!(tags.is_empty());
        // TestDb automatically cleaned up
    }

    #[test]
    fn test_with_populated_database() {
        let test_db = TestDb::new("test_db_search_populated");
        let db = test_db.db();

        let file1 = TempFile::create("file1.txt").unwrap();
        let file2 = TempFile::create("file2.txt").unwrap();
        let file3 = TempFile::create("file3.txt").unwrap();

        db.insert(file1.path(), vec!["rust".into(), "programming".into()])
            .unwrap();
        db.insert(file2.path(), vec!["rust".into(), "tutorial".into()])
            .unwrap();
        db.insert(file3.path(), vec!["python".into()]).unwrap();

        let tags = db.list_all_tags().unwrap();
        assert_eq!(tags.len(), 4); // rust, programming, tutorial, python
        assert!(tags.contains(&"rust".to_string()));

        let rust_files = db.find_by_tag("rust").unwrap();
        assert_eq!(rust_files.len(), 2);
        // TempFiles and TestDb automatically cleaned up
    }
}
