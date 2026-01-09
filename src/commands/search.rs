//! Search command - find files by tags and patterns

use crate::{
    TagrError,
    cli::{SearchMode, SearchParams},
    config,
    db::{Database, query},
    filters::{FilterCriteria, FilterManager},
    output,
    patterns::{PatternBuilder, PatternContext},
};
use std::path::PathBuf;

type Result<T> = std::result::Result<T, TagrError>;

/// Execute the search command
///
/// # Arguments
/// * `has_explicit_tag_mode` - True if user provided --any-tag or --all-tags flags
/// * `has_explicit_file_mode` - True if user provided --any-file or --all-files flags
/// * `has_explicit_virtual_mode` - True if user provided --any-virtual or --all-virtual flags
///
/// # Errors
/// Returns an error if database operations fail or search parameters are invalid
pub fn execute(
    db: &Database,
    mut params: SearchParams,
    filter_name: Option<&str>,
    save_filter: Option<(&str, Option<&str>)>,
    has_explicit_tag_mode: bool,
    has_explicit_file_mode: bool,
    has_explicit_virtual_mode: bool,
    path_format: config::PathFormat,
    quiet: bool,
) -> Result<()> {
    if let Some(name) = filter_name {
        let filter_path = crate::filters::get_filter_path()?;
        let manager = FilterManager::new(filter_path);
        let filter = manager.get(name)?;

        // Start with filter params as base, then merge CLI overrides
        let mut filter_params = SearchParams::from(&filter.criteria);
        let cli_tag_mode = params.tag_mode;
        let cli_file_mode = params.file_mode;
        let cli_virtual_mode = params.virtual_mode;

        filter_params.merge(&params);

        // If user didn't explicitly provide mode flags, keep filter's modes
        if has_explicit_tag_mode {
            filter_params.tag_mode = cli_tag_mode;
        } else {
            filter_params.tag_mode = filter.criteria.tag_mode.into();
        }

        if has_explicit_file_mode {
            filter_params.file_mode = cli_file_mode;
        } else {
            filter_params.file_mode = filter.criteria.file_mode.into();
        }

        if has_explicit_virtual_mode {
            filter_params.virtual_mode = cli_virtual_mode;
        } else {
            filter_params.virtual_mode = filter.criteria.virtual_mode.into();
        }

        params = filter_params;

        manager.record_use(name)?;

        if !quiet {
            println!("Using filter '{name}'");
        }
    }

    if params.query.is_some() && (!params.tags.is_empty() || !params.file_patterns.is_empty()) {
        return Err(TagrError::InvalidInput(
            "Cannot use general query with -t or -f flags. Use either 'tagr search <query>' or 'tagr search -t <tag> -f <pattern>'.".into()
        ));
    }

    if params.query.is_none()
        && params.tags.is_empty()
        && params.file_patterns.is_empty()
        && params.virtual_tags.is_empty()
    {
        return Err(TagrError::InvalidInput("No search criteria provided. Use -t for tags, -f for file patterns, or -v for virtual tags.".into()));
    }

    // Strict mode: require explicit --glob-files or --regex-file for non-bulk search
    if !params.file_patterns.is_empty() {
        let has_glob_like = params
            .file_patterns
            .iter()
            .any(|p| p.contains('*') || p.contains('?') || p.contains('['));
        if has_glob_like && !params.glob_files && !params.regex_file {
            return Err(TagrError::InvalidInput(
                "Glob-like file pattern detected without --glob-files. Use --glob-files for globs or --regex-file for regex patterns.".into(),
            ));
        }
    }

    // Validate tag/file separation using PatternBuilder in SearchFiles context.
    // This does not alter params; it ensures glob-like tags are rejected and
    // patterns are consistent with flags.
    let mut builder = PatternBuilder::new(PatternContext::SearchFiles)
        .regex_tags(params.regex_tag)
        .regex_files(params.regex_file)
        .glob_files_flag(params.glob_files);
    for t in &params.tags {
        builder.add_tag_token(t);
    }
    for f in &params.file_patterns {
        builder.add_file_token(f);
    }
    let _ = builder.build(params.tag_mode, params.file_mode)?;

    let files = query::apply_search_params(db, &params)?;

    if let Some(query) = &params.query {
        print_results(db, &files, query, path_format, quiet);
    } else if files.is_empty() {
        if !quiet {
            let criteria = build_criteria_description(&params);
            println!("No files found matching {criteria}");
        }
    } else {
        if !quiet {
            let description = build_search_description(&params);
            println!("Found {} file(s) matching {}:", files.len(), description);
        }

        for file in files {
            print_file_with_tags(db, &file, path_format, quiet);
        }
    }

    if let Some((name, desc)) = save_filter {
        let filter_path = crate::filters::get_filter_path()?;
        let manager = FilterManager::new(filter_path);
        let criteria = FilterCriteria::from(params);
        let description = desc.unwrap_or("Saved search filter");

        manager.create(name, description.to_string(), criteria)?;

        if !quiet {
            println!("\nSaved filter '{name}'");
        }
    }

    Ok(())
}

fn print_results(
    db: &Database,
    files: &[PathBuf],
    query: &str,
    path_format: config::PathFormat,
    quiet: bool,
) {
    if files.is_empty() {
        if !quiet {
            println!("No files found matching query '{query}' (searched tags and filenames)");
        }
    } else {
        if !quiet {
            println!(
                "Found {} file(s) matching query '{}' (tags or filenames):",
                files.len(),
                query
            );
        }

        for file in files {
            print_file_with_tags(db, file, path_format, quiet);
        }
    }
}

fn print_file_with_tags(
    db: &Database,
    file: &PathBuf,
    path_format: config::PathFormat,
    quiet: bool,
) {
    if let Ok(Some(tags)) = db.get_tags(file) {
        let formatted = output::file_with_tags(file, &tags, path_format, quiet);
        println!("{formatted}");
    } else {
        let formatted = output::format_path(file, path_format);
        if quiet {
            println!("{formatted}");
        } else {
            println!("  {formatted}");
        }
    }
}

fn build_criteria_description(params: &SearchParams) -> String {
    if params.tags.is_empty() {
        format!("file patterns: {}", params.file_patterns.join(", "))
    } else {
        format!("tags: {}", params.tags.join(", "))
    }
}

fn build_search_description(params: &SearchParams) -> String {
    let tag_desc = if params.tags.is_empty() {
        String::new()
    } else if params.tag_mode == SearchMode::All {
        format!("ALL tags [{}]", params.tags.join(", "))
    } else {
        format!("ANY tag [{}]", params.tags.join(", "))
    };

    let file_desc = if params.file_patterns.is_empty() {
        String::new()
    } else if params.file_mode == SearchMode::All {
        format!("ALL patterns [{}]", params.file_patterns.join(", "))
    } else {
        format!("ANY pattern [{}]", params.file_patterns.join(", "))
    };

    let mut parts = Vec::new();
    if !tag_desc.is_empty() {
        parts.push(tag_desc);
    }
    if !file_desc.is_empty() {
        parts.push(file_desc);
    }

    parts.join(" and ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestDb;

    #[test]
    fn test_execute_errors_on_glob_without_flag() {
        let test_db = TestDb::new("search_exec_glob_no_flag");
        let db = test_db.db();
        let params = SearchParams {
            query: None,
            tags: vec![],
            tag_mode: SearchMode::All,
            file_patterns: vec!["*.rs".to_string()],
            file_mode: SearchMode::All,
            exclude_tags: vec![],
            regex_tag: false,
            regex_file: false,
            glob_files: false,
            virtual_tags: vec![],
            virtual_mode: SearchMode::All,
            no_hierarchy: false,
        };
        let err = execute(
            db,
            params,
            None,
            None,
            false,
            false,
            false,
            config::PathFormat::Absolute,
            true,
        )
        .err()
        .expect("should error");
        match err {
            TagrError::InvalidInput(msg) => {
                assert!(msg.contains("Glob-like file pattern"));
            }
            _ => panic!("Expected InvalidInput for glob-like pattern without flag"),
        }
    }

    #[test]
    fn test_execute_ok_with_explicit_glob_flag() {
        let test_db = TestDb::new("search_exec_glob_with_flag");
        let db = test_db.db();
        let params = SearchParams {
            query: None,
            tags: vec![],
            tag_mode: SearchMode::All,
            file_patterns: vec!["*.md".to_string()],
            file_mode: SearchMode::All,
            exclude_tags: vec![],
            regex_tag: false,
            regex_file: false,
            glob_files: true,
            virtual_tags: vec![],
            virtual_mode: SearchMode::All,
            no_hierarchy: false,
        };
        let res = execute(
            db,
            params,
            None,
            None,
            false,
            false,
            false,
            config::PathFormat::Absolute,
            true,
        );
        assert!(res.is_ok());
    }

    #[test]
    fn test_execute_errors_on_glob_like_tag() {
        let test_db = TestDb::new("search_exec_glob_like_tag");
        let db = test_db.db();
        let params = SearchParams {
            query: None,
            tags: vec!["feature/*".to_string()],
            tag_mode: SearchMode::All,
            file_patterns: vec![],
            file_mode: SearchMode::All,
            exclude_tags: vec![],
            regex_tag: false,
            regex_file: false,
            glob_files: false,
            virtual_tags: vec![],
            virtual_mode: SearchMode::All,
            no_hierarchy: false,
        };
        let err = execute(
            db,
            params,
            None,
            None,
            false,
            false,
            false,
            config::PathFormat::Absolute,
            true,
        )
        .err()
        .expect("should error");
        match err {
            TagrError::PatternError(_) => {}
            _ => panic!("Expected PatternError for glob-like tag token"),
        }
    }
}
