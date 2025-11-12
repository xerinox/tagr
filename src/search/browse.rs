//! Interactive browse functionality
//!
//! Provides a two-stage fuzzy finder interface:
//! 1. Select tags using fuzzy finder (multi-select supported)
//! 2. View and select files matching those tags (multi-select supported)
//!
//! Uses abstracted UI layer for easy backend swapping.

use super::error::SearchError;
use crate::cli::SearchParams;
use crate::config::PathFormat;
use crate::db::{Database, query};
use crate::preview::FilePreviewProvider;
use crate::ui::{
    DisplayItem, FinderConfig, FuzzyFinder, ItemMetadata, PreviewConfig,
    skim_adapter::SkimFinder,
};
use colored::Colorize;
use std::path::{Path, PathBuf};


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

/// Result of an interactive browse session
#[derive(Debug)]
pub struct BrowseResult {
    pub selected_tags: Vec<String>,
    pub selected_files: Vec<PathBuf>,
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
pub fn browse(db: &Database, path_format: PathFormat) -> Result<Option<BrowseResult>, SearchError> {
    browse_with_params(db, None, None, path_format)
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
/// * `path_format` - Format to use for displaying file paths
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
) -> Result<Option<BrowseResult>, SearchError> {
    let (selected_tags, matching_files) = if let Some(params) = search_params {
        let files = query::apply_search_params(db, &params).map_err(SearchError::DatabaseError)?;
        (params.tags, files)
    } else {
        let selected_tags = select_tags(db)?;

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

    let selected_files = select_files_from_list(db, &matching_files, preview_overrides, path_format)?;

    if selected_files.is_empty() {
        return Ok(None);
    }

    Ok(Some(BrowseResult {
        selected_tags,
        selected_files,
    }))
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
///
/// # Returns
/// * Empty vector if no tags exist or user cancelled
/// * Vector of selected tag strings if user confirmed selection
///
/// # Errors
///
/// Returns `SearchError::DatabaseError` if tag listing fails or
/// `SearchError::InterruptedError` if the fuzzy finder is interrupted.
fn select_tags(db: &Database) -> Result<Vec<String>, SearchError> {
    let all_tags = db.list_all_tags()?;

    if all_tags.is_empty() {
        eprintln!("No tags found in database");
        return Ok(Vec::new());
    }

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

    let config = FinderConfig::new(
        items,
        "Select tags (TAB to select multiple, Enter to confirm): ".to_string(),
    )
    .with_multi_select(true);

    let finder = SkimFinder::new();
    let result = finder.run(config).map_err(SearchError::UiError)?;

    if result.aborted {
        return Ok(Vec::new());
    }

    Ok(result.selected)
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

/// Advanced search with AND/OR logic for tag filtering
///
/// Provides an enhanced browse experience with two search modes:
/// - ANY: Files matching any of the selected tags (OR operation)
/// - ALL: Files matching all of the selected tags (AND operation)
///
/// # Arguments
/// * `db` - The database to query
/// * `path_format` - Format to use for displaying file paths
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
) -> Result<Option<BrowseResult>, SearchError> {
    let selected_tags = select_tags(db)?;

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
    let options = vec!["ANY (files with any of these tags)", "ALL (files with all of these tags)"];
    
    let items: Vec<DisplayItem> = options
        .iter()
        .enumerate()
        .map(|(idx, opt)| {
            DisplayItem::with_metadata(
                opt.to_string(),
                opt.to_string(),
                opt.to_string(),
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
