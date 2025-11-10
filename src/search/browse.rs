//! Interactive browse functionality using skim fuzzy finder
//! 
//! Provides a two-stage fuzzy finder interface:
//! 1. Select tags using fuzzy finder (multi-select supported)
//! 2. View and select files matching those tags (multi-select supported)

use crate::db::Database;
use crate::cli::SearchParams;
use crate::config::PathFormat;
use super::error::SearchError;
use skim::prelude::*;
use std::io::Cursor;
use std::path::{PathBuf, Path};
use std::sync::Arc;
use std::borrow::Cow;
use colored::Colorize;

/// Build skim options with common defaults
///
/// Creates a configured `SkimOptions` instance with standardized settings
/// for the fuzzy finder UI. Uses full-screen mode with alternate buffer
/// to avoid leaving UI artifacts in the terminal.
///
/// # Arguments
/// * `multi` - Whether multi-select is enabled
/// * `prompt` - Prompt text to display
/// * `ansi` - Whether to enable ANSI color support
///
/// # Errors
///
/// Returns `SearchError::BuildError` if the skim options cannot be constructed
/// (this is rare and usually indicates an internal skim configuration issue).
fn build_skim_options(
    multi: bool,
    prompt: &str,
    ansi: bool,
) -> Result<SkimOptions, SearchError> {
    let mut builder = SkimOptionsBuilder::default();
    builder.multi(multi)
        .prompt(prompt.to_string())
        .reverse(true);
    
    if ansi {
        builder.ansi(true).color(Some("dark".to_string()));
    }
    
    builder.build()
        .map_err(|e| SearchError::BuildError(format!("Failed to build skim options: {e}")))
}

/// Format a path according to the specified format for display
///
/// # Arguments
/// * `path` - The path to format
/// * `format` - Whether to display as absolute or relative
///
/// # Returns
/// A string representation of the path
fn format_path_for_display(path: &Path, format: PathFormat) -> String {
    match format {
        PathFormat::Absolute => path.display().to_string(),
        PathFormat::Relative => {
            // Try to get relative path from current directory
            if let Ok(cwd) = std::env::current_dir() {
                if let Ok(rel_path) = path.strip_prefix(&cwd) {
                    return rel_path.display().to_string();
                }
            }
            // Fallback to absolute if relative path cannot be computed
            path.display().to_string()
        }
    }
}

// File selection rendering & multi-select:
// We use a custom `FileItem` so skim handles ANSI via `AnsiString::parse`.
// Earlier issues with multi-select arose when the output string included tags
// (making the logical selection key differ per file+tags). We now keep `output()`
// strictly to the raw path while exposing tags only in `display()` and in the
// searchable `text()` payload. This preserves multi-select correctness and ANSI colors.

#[derive(Debug, Clone)]
struct FileItem {
    path: String,
    display_path: String,
    tags: Vec<String>,
    exists: bool,
    index: usize,
}

impl SkimItem for FileItem {
    fn text(&self) -> Cow<'_, str> {
        // Limit searchable text to just the display path to avoid unintended bulk selection side-effects.
        Cow::Borrowed(&self.display_path)
    }

    fn display<'a>(&'a self, _context: DisplayContext<'a>) -> AnsiString<'a> {
        let file_colored = if self.exists { self.display_path.green() } else { self.display_path.red() };
        let line = format!("{} [{}]", file_colored, self.tags.join(", "));
        AnsiString::parse(&line)
    }

    fn output(&self) -> Cow<'_, str> {
        // Raw absolute path only for stable multi-select key.
        Cow::Borrowed(&self.path)
    }

    fn get_index(&self) -> usize {
        self.index
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
    browse_with_params(db, None, path_format)
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
    path_format: PathFormat,
) -> Result<Option<BrowseResult>, SearchError> {
    let (selected_tags, matching_files) = if let Some(params) = search_params {
        let files = apply_search_params(db, &params)?;
        (params.tags.clone(), files)
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
    
    let selected_files = select_files_from_list(db, &matching_files, path_format)?;
    
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
/// Returns `SearchError::DatabaseError` if database operations fail.
fn apply_search_params(db: &Database, params: &SearchParams) -> Result<Vec<PathBuf>, SearchError> {
    let mut files = if let Some(query) = &params.query {
        // General query searches both tags and filenames
        let mut by_tag = db.find_by_tag(query)?;
        let all_files = db.list_all_files()?;
        let by_name: Vec<PathBuf> = all_files
            .into_iter()
            .filter(|f| {
                f.to_string_lossy()
                    .to_lowercase()
                    .contains(&query.to_lowercase())
            })
            .collect();
        
        // Combine and deduplicate
        by_tag.extend(by_name);
        by_tag.sort();
        by_tag.dedup();
        by_tag
    } else if !params.tags.is_empty() {
        use crate::cli::SearchMode;
        match params.tag_mode {
            SearchMode::Any => db.find_by_any_tag(&params.tags)?,
            SearchMode::All => db.find_by_all_tags(&params.tags)?,
        }
    } else {
        db.list_all_files()?
    };
    
    if !params.file_patterns.is_empty() {
        files = filter_by_patterns(&files, &params.file_patterns, params.regex_file);
    }
    
    if !params.exclude_tags.is_empty() {
        files = filter_out_tags(db, &files, &params.exclude_tags)?;
    }
    
    Ok(files)
}

/// Filter files by glob or regex patterns
///
/// # Arguments
/// * `files` - Files to filter
/// * `patterns` - Patterns to match against
/// * `use_regex` - Whether to use regex matching
///
/// # Returns
/// * Filtered vector of files matching at least one pattern
fn filter_by_patterns(files: &[PathBuf], patterns: &[String], use_regex: bool) -> Vec<PathBuf> {
    if use_regex {
        files
            .iter()
            .filter(|f| {
                let path_str = f.to_string_lossy();
                patterns.iter().any(|p| {
                    regex::Regex::new(p)
                        .map(|re| re.is_match(&path_str))
                        .unwrap_or(false)
                })
            })
            .cloned()
            .collect()
    } else {
        files
            .iter()
            .filter(|f| {
                let path_str = f.to_string_lossy();
                patterns.iter().any(|p| {
                    glob::Pattern::new(p)
                        .map(|pattern| pattern.matches(&path_str))
                        .unwrap_or(false)
                })
            })
            .cloned()
            .collect()
    }
}

/// Filter out files that have any of the excluded tags
///
/// # Arguments
/// * `db` - Database to query for file tags
/// * `files` - Files to filter
/// * `exclude_tags` - Tags to exclude
///
/// # Returns
/// * Filtered vector of files without any excluded tags
///
/// # Errors
///
/// Returns `SearchError::DatabaseError` if tag lookup fails.
fn filter_out_tags(
    db: &Database,
    files: &[PathBuf],
    exclude_tags: &[String],
) -> Result<Vec<PathBuf>, SearchError> {
    let mut result = Vec::new();
    for file in files {
        if let Some(file_tags) = db.get_tags(file)? {
            let has_excluded = file_tags
                .iter()
                .any(|tag| exclude_tags.contains(tag));
            if !has_excluded {
                result.push(file.clone());
            }
        } else {
            result.push(file.clone());
        }
    }
    Ok(result)
}

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
    
    let tag_list = all_tags.join("\n");
    let item_reader = SkimItemReader::default();
    let items = item_reader.of_bufread(Cursor::new(tag_list));
    
    let options = build_skim_options(
        true,
        "Select tags (TAB to select multiple, Enter to confirm): ",
        false,
    )?;
    
    let output = Skim::run_with(&options, Some(items))
        .ok_or(SearchError::InterruptedError)?;
    
    if output.is_abort {
        return Ok(Vec::new());
    }
    
    let selected_tags: Vec<String> = output
        .selected_items
        .iter()
        .map(|item| item.output().to_string())
        .collect();
    
    Ok(selected_tags)
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
fn select_files_from_list(db: &Database, files: &[PathBuf], path_format: PathFormat) -> Result<Vec<PathBuf>, SearchError> {
    if files.is_empty() {
        eprintln!("No files found with the selected tags");
        return Ok(Vec::new());
    }

    let (tx, rx): (SkimItemSender, SkimItemReceiver) = unbounded();
    for (idx, file) in files.iter().enumerate() {
        if let Some(tags) = db.get_tags(file)? {
            let item = Arc::new(FileItem {
                path: file.to_string_lossy().to_string(),
                display_path: format_path_for_display(file, path_format),
                tags,
                exists: Path::new(file).exists(),
                index: idx,
            });
            let _ = tx.send(item);
        }
    }
    drop(tx);

    let options = build_skim_options(
        true,
        "Select files (TAB to select multiple, Enter to confirm): ",
        true,
    )?;

    let output = Skim::run_with(&options, Some(rx))
        .ok_or(SearchError::InterruptedError)?;

    if output.is_abort {
        return Ok(Vec::new());
    }

    let selected_files: Vec<PathBuf> = output
        .selected_items
        .iter()
        .map(|it| PathBuf::from(it.output().to_string()))
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
pub fn browse_advanced(db: &Database, path_format: PathFormat) -> Result<Option<BrowseResult>, SearchError> {
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
    
    let selected_files = select_files_from_list(db, &matching_files, path_format)?;
    
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
    let options_text = "ANY (files with any of these tags)\nALL (files with all of these tags)";
    let item_reader = SkimItemReader::default();
    let items = item_reader.of_bufread(Cursor::new(options_text));
    
    let options = build_skim_options(
        false,
        "Search mode: ",
        false,
    )?;
    
    let output = Skim::run_with(&options, Some(items))
        .ok_or(SearchError::InterruptedError)?;
    
    if output.is_abort || output.selected_items.is_empty() {
        return Ok(false);
    }
    
    let selection = output.selected_items[0].output().to_string();
    Ok(selection.starts_with("ALL"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    
    // Helper function to create a test file
    fn create_test_file(path: &str) -> std::io::Result<()> {
        let mut file = fs::File::create(path)?;
        file.write_all(b"test content")?;
        Ok(())
    }
    
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
        let test_db_path = "test_db_search_empty";
        let db = Database::open(test_db_path).unwrap();
        db.clear().unwrap();
        
        let tags = db.list_all_tags().unwrap();
        assert!(tags.is_empty());
        
        drop(db);
        let _ = fs::remove_dir_all(test_db_path);
    }
    
    #[test]
    fn test_with_populated_database() {
        let test_db_path = "test_db_search_populated";
        let db = Database::open(test_db_path).unwrap();
        db.clear().unwrap();
        
        create_test_file("file1.txt").unwrap();
        create_test_file("file2.txt").unwrap();
        create_test_file("file3.txt").unwrap();
        
        db.insert("file1.txt", vec!["rust".into(), "programming".into()]).unwrap();
        db.insert("file2.txt", vec!["rust".into(), "tutorial".into()]).unwrap();
        db.insert("file3.txt", vec!["python".into()]).unwrap();
        
        let tags = db.list_all_tags().unwrap();
        assert_eq!(tags.len(), 4); // rust, programming, tutorial, python
        assert!(tags.contains(&"rust".to_string()));
        
        let rust_files = db.find_by_tag("rust").unwrap();
        assert_eq!(rust_files.len(), 2);
        
        db.clear().unwrap();
        drop(db);
        let _ = fs::remove_dir_all(test_db_path);
        let _ = fs::remove_file("file1.txt");
        let _ = fs::remove_file("file2.txt");
        let _ = fs::remove_file("file3.txt");
    }
}
