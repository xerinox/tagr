//! Search command - find files by tags and patterns

use crate::{
    TagrError,
    cli::{SearchMode, SearchParams},
    config,
    db::{Database, query},
    filters::{FilterCriteria, FilterManager},
    output,
};
use std::path::PathBuf;

type Result<T> = std::result::Result<T, TagrError>;

/// Execute the search command
///
/// # Errors
/// Returns an error if database operations fail or search parameters are invalid
pub fn execute(
    db: &Database,
    mut params: SearchParams,
    filter_name: Option<&str>,
    save_filter: Option<(&str, Option<&str>)>,
    path_format: config::PathFormat,
    quiet: bool,
) -> Result<()> {
    if let Some(name) = filter_name {
        let filter_path = crate::filters::get_filter_path()?;
        let manager = FilterManager::new(filter_path);
        let filter = manager.get(name)?;

        let filter_params = SearchParams::from(&filter.criteria);
        params.merge(&filter_params);

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
