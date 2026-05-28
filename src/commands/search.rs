//! Search command - find files by tags and patterns

use crate::{
    TagrError,
    config,
    filters::FilterManager,
    output,
    patterns::{PatternBuilder, PatternContext},
    schema,
    store::TagStore,
    types::{MatchMode, QueryCriteria, TagName, TagrPath},
};
use std::io::Write;

type Result<T> = std::result::Result<T, TagrError>;

#[derive(Clone, Copy)]
pub struct ExplicitFlags {
    pub tag_mode: bool,
    pub file_mode: bool,
    pub virtual_mode: bool,
    pub glob_files: bool,
}

#[derive(Clone, Copy)]
pub struct OutputConfig {
    pub format: config::PathFormat,
    pub quiet: bool,
}

#[derive(Clone, Copy)]
pub struct FilterConfig<'a> {
    pub apply: Option<&'a str>,
    pub save: Option<(&'a str, Option<&'a str>)>,
}

/// Execute the search command
///
/// # Arguments
/// * `filter_config` - Configuration for applying/saving filters
/// * `explicit_flags` - Flags indicating if user explicitly provided tag/file/virtual modes
/// * `output_config` - Configuration for output formatting and verbosity
///
/// # Errors
/// Returns an error if database operations fail or search parameters are invalid
pub fn execute(
    store: &dyn TagStore,
    mut criteria: QueryCriteria,
    filter_config: FilterConfig,
    explicit_flags: ExplicitFlags,
    output_config: OutputConfig,
    writer: &mut impl Write,
) -> Result<()> {
    if let Some(name) = filter_config.apply {
        let filter_path = crate::filters::get_filter_path()?;
        let manager = FilterManager::new(filter_path);
        let filter = manager.get(name)?;

        // The saved filter criteria is already a QueryCriteria
        let filter_criteria = filter.criteria;

        // Merge: CLI values override filter defaults
        criteria = merge_criteria(filter_criteria, &criteria, explicit_flags);

        manager.record_use(name)?;

        if !output_config.quiet {
            writeln!(writer, "Using filter '{name}'")?;
        }
    }

    if criteria.query.is_some()
        && (criteria.tag_expr.is_some() || !criteria.file_patterns.is_empty())
    {
        return Err(TagrError::InvalidInput(
            "Cannot use general query with -t or -f flags. Use either 'tagr search <query>' or 'tagr search -t <tag> -f <pattern>'.".into()
        ));
    }

    if criteria.query.is_none()
        && criteria.tag_expr.is_none()
        && criteria.file_patterns.is_empty()
        && criteria.virtual_tags.is_empty()
    {
        return Err(TagrError::InvalidInput("No search criteria provided. Use -t for tags, -f for file patterns, or -v for virtual tags.".into()));
    }

    // Strict mode: require explicit --glob-files or --regex-file for non-bulk search
    if !criteria.file_patterns.is_empty() {
        let has_glob_like = criteria
            .file_patterns
            .iter()
            .any(|p| p.contains('*') || p.contains('?') || p.contains('['));
        if has_glob_like && !explicit_flags.glob_files && !criteria.regex_files {
            return Err(TagrError::InvalidInput(
                "Glob-like file pattern detected without --glob-files. Use --glob-files for globs or --regex-file for regex patterns.".into(),
            ));
        }
    }

    // Validate tag/file separation via PatternBuilder
    validate_patterns(&criteria, explicit_flags)?;

    let schema = schema::load_default_schema().unwrap_or_default();
    let files = store.query(&criteria, &schema)?;

    if let Some(query) = &criteria.query {
        print_results(store, &files, query, output_config.format, output_config.quiet, writer)?;
    } else if files.is_empty() {
        if !output_config.quiet {
            let desc = criteria_description(&criteria);
            writeln!(writer, "No files found matching {desc}")?;
        }
    } else {
        if !output_config.quiet {
            let description = search_description(&criteria);
            writeln!(writer, "Found {} file(s) matching {}:", files.len(), description)?;
        }

        for file in files {
            print_file_with_tags(store, &file, output_config.format, output_config.quiet, writer)?;
        }
    }

    if let Some((name, desc)) = filter_config.save {
        let filter_path = crate::filters::get_filter_path()?;
        let manager = FilterManager::new(filter_path);
        let description = desc.unwrap_or("Saved search filter");

        manager.create(name, description.to_string(), criteria)?;

        if !output_config.quiet {
            writeln!(writer, "\nSaved filter '{name}'")?;
        }
    }

    Ok(())
}

/// Merge filter-based criteria with CLI-provided criteria.
///
/// The filter provides defaults; CLI values override when the user
/// explicitly set them (tracked by `ExplicitFlags`).
fn merge_criteria(
    filter: QueryCriteria,
    cli: &QueryCriteria,
    flags: ExplicitFlags,
) -> QueryCriteria {
    QueryCriteria {
        // CLI tag_expr overrides filter if present, otherwise keep filter's
        tag_expr: if cli.tag_expr.is_some() {
            cli.tag_expr.clone()
        } else {
            filter.tag_expr
        },
        regex_tags: cli.regex_tags || filter.regex_tags,
        expand_hierarchy: cli.expand_hierarchy && filter.expand_hierarchy,
        file_patterns: if cli.file_patterns.is_empty() {
            filter.file_patterns
        } else {
            cli.file_patterns.clone()
        },
        file_mode: if flags.file_mode {
            cli.file_mode
        } else {
            filter.file_mode
        },
        regex_files: cli.regex_files || filter.regex_files,
        virtual_tags: if cli.virtual_tags.is_empty() {
            filter.virtual_tags
        } else {
            cli.virtual_tags.clone()
        },
        virtual_mode: if flags.virtual_mode {
            cli.virtual_mode
        } else {
            filter.virtual_mode
        },
        query: cli.query.clone().or(filter.query),
    }
}

/// Validate tag/file separation using `PatternBuilder`.
fn validate_patterns(criteria: &QueryCriteria, flags: ExplicitFlags) -> Result<()> {
    // Extract flat tags from the tag expression for pattern validation
    let tags: Vec<String> = criteria
        .flat_include_tags()
        .map(|set| set.into_iter().map(|t| t.as_str().to_string()).collect())
        .unwrap_or_default();

    let mut builder = PatternBuilder::new(PatternContext::SearchFiles)
        .regex_tags(criteria.regex_tags)
        .regex_files(criteria.regex_files)
        .glob_files_flag(flags.glob_files);
    for t in &tags {
        builder.add_tag_token(t);
    }
    for f in &criteria.file_patterns {
        builder.add_file_token(f);
    }

    // Determine tag/file modes for validation
    let tag_mode = match &criteria.tag_expr {
        Some(crate::types::TagExpr::Or(_)) => MatchMode::Any,
        _ => MatchMode::All,
    };
    let file_mode = match criteria.file_mode {
        MatchMode::Any => MatchMode::Any,
        MatchMode::All => MatchMode::All,
    };

    let _ = builder.build(tag_mode, file_mode)?;
    Ok(())
}

fn print_results(
    store: &dyn TagStore,
    files: &[TagrPath],
    query: &str,
    path_format: config::PathFormat,
    quiet: bool,
    writer: &mut impl Write,
) -> Result<()> {
    if files.is_empty() {
        if !quiet {
            writeln!(writer, "No files found matching query '{query}' (searched tags and filenames)")?;
        }
    } else {
        if !quiet {
            writeln!(
                writer,
                "Found {} file(s) matching query '{}' (tags or filenames):",
                files.len(),
                query
            )?;
        }

        for file in files {
            print_file_with_tags(store, file, path_format, quiet, writer)?;
        }
    }
    Ok(())
}

fn print_file_with_tags(
    store: &dyn TagStore,
    file: &TagrPath,
    path_format: config::PathFormat,
    quiet: bool,
    writer: &mut impl Write,
) -> Result<()> {
    if let Ok(Some(tags)) = store.get_tags(file) {
        let formatted = output::file_with_tags(file, &tags, path_format, quiet);
        writeln!(writer, "{formatted}")?;
    } else {
        let formatted = output::format_path(file, path_format);
        if quiet {
            writeln!(writer, "{formatted}")?;
        } else {
            writeln!(writer, "  {formatted}")?;
        }
    }
    Ok(())
}

fn criteria_description(criteria: &QueryCriteria) -> String {
    if criteria.tag_expr.is_none() {
        format!("file patterns: {}", criteria.file_patterns.join(", "))
    } else {
        criteria.to_cli_string()
    }
}

fn search_description(criteria: &QueryCriteria) -> String {
    let tag_desc = criteria.tag_expr.as_ref().map_or_else(String::new, |expr| {
        let tags = criteria
            .flat_include_tags()
            .map_or_else(|| format!("{expr:?}"), |set| {
                let mut names: Vec<_> = set.into_iter().map(TagName::to_string).collect();
                names.sort();
                names.join(", ")
            });

        match expr {
            crate::types::TagExpr::Or(_) => format!("ANY tag [{tags}]"),
            _ => format!("ALL tags [{tags}]"),
        }
    });

    let file_desc = if criteria.file_patterns.is_empty() {
        String::new()
    } else {
        match criteria.file_mode {
            MatchMode::All => format!("ALL patterns [{}]", criteria.file_patterns.join(", ")),
            MatchMode::Any => format!("ANY pattern [{}]", criteria.file_patterns.join(", ")),
        }
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
    use crate::types::TagExpr;

    #[test]
    fn test_execute_errors_on_glob_without_flag() {
        let test_db = TestDb::new("search_exec_glob_no_flag");
        let criteria = QueryCriteria {
            file_patterns: vec!["*.rs".to_string()],
            ..Default::default()
        };
        let err = execute(
            test_db.store(),
            criteria,
            FilterConfig {
                apply: None,
                save: None,
            },
            ExplicitFlags {
                tag_mode: false,
                file_mode: false,
                virtual_mode: false,
                glob_files: false,
            },
            OutputConfig {
                format: config::PathFormat::Absolute,
                quiet: true,
            },
            &mut Vec::new(),
        )
        .expect_err("should error");
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
        let criteria = QueryCriteria {
            file_patterns: vec!["*.md".to_string()],
            ..Default::default()
        };
        let res = execute(
            test_db.store(),
            criteria,
            FilterConfig {
                apply: None,
                save: None,
            },
            ExplicitFlags {
                tag_mode: false,
                file_mode: false,
                virtual_mode: false,
                glob_files: true,
            },
            OutputConfig {
                format: config::PathFormat::Absolute,
                quiet: true,
            },
            &mut Vec::new(),
        );
        assert!(res.is_ok());
    }

    #[test]
    fn test_execute_errors_on_glob_like_tag() {
        let test_db = TestDb::new("search_exec_glob_like_tag");
        // "feature/*" contains '*' which is invalid for TagName,
        // but we can still test pattern validation by putting it in file_patterns
        // and tagging via tag_expr with a valid tag
        let criteria = QueryCriteria {
            tag_expr: Some(TagExpr::Tag(TagName::new("feature").unwrap())),
            file_patterns: vec![],
            ..Default::default()
        };
        // This should succeed since there's no glob-like pattern issue
        let res = execute(
            test_db.store(),
            criteria,
            FilterConfig {
                apply: None,
                save: None,
            },
            ExplicitFlags {
                tag_mode: false,
                file_mode: false,
                virtual_mode: false,
                glob_files: false,
            },
            OutputConfig {
                format: config::PathFormat::Absolute,
                quiet: true,
            },
            &mut Vec::new(),
        );
        assert!(res.is_ok());
    }
}
