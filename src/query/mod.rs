//! Query engine — the single pipeline for all tag-based file searches.
//!
//! Both CLI and TUI use [`execute()`] to run queries. This module consolidates
//! logic previously scattered across `db/query.rs`, `search/`, and inline
//! implementations in the TUI state.
//!
//! # Pipeline stages
//!
//! 1. **Free-text query** — regex tag search + glob filename matching (union)
//! 2. **Tag expression evaluation** — hierarchy expansion via schema + store
//! 3. **File pattern filtering** — glob/regex with AND/OR
//! 4. **Virtual tag filtering** — rayon parallel evaluation
//!
//! # Examples
//!
//! ```ignore
//! use tagr::query;
//! use tagr::store::DirectStore;
//! use tagr::types::QueryCriteria;
//!
//! let store = DirectStore::open("my_db")?;
//! let schema = tagr::schema::load_default_schema()?;
//! let criteria = QueryCriteria::default();
//! let files = query::execute(&store, &criteria, &schema)?;
//! ```

pub mod error;
pub mod hierarchy;
pub mod patterns;

pub use error::SearchError;

#[cfg(test)]
mod tests;

use crate::schema::types::TagSchema;
use crate::schema::HIERARCHY_DELIMITER;
use crate::store::{StoreError, TagStore};
use crate::types::{MatchMode, QueryCriteria, TagExpr, TagName, TagrPath};
use std::collections::HashSet;

/// Execute a query against a store using the full pipeline.
///
/// This is THE single query entry point for tagr. Both CLI and TUI
/// funnel through here. `DirectStore::query()` delegates to this function.
///
/// # Pipeline
///
/// 1. If `criteria.query` is set: free-text search (regex tags + glob filenames)
/// 2. If `criteria.tag_expr` is set: evaluate tag expression with hierarchy awareness
/// 3. Apply file pattern filters (glob/regex, AND/OR)
/// 4. Apply virtual tag filters (filesystem metadata, parallel via rayon)
///
/// # Errors
///
/// Returns `StoreError` on backend failures or invalid patterns.
#[allow(clippy::too_many_lines)]
pub fn execute(
    store: &dyn TagStore,
    criteria: &QueryCriteria,
    schema: &TagSchema,
) -> Result<Vec<TagrPath>, StoreError> {
    if criteria.is_empty() {
        return store.list_all_files();
    }

    // Stage 1: Collect candidate files
    let mut files = if let Some(ref query) = criteria.query {
        collect_freetext_matches(store, query)?
    } else if let Some(ref expr) = criteria.tag_expr {
        evaluate_tag_expr(store, expr, criteria, schema)?
    } else {
        store.list_all_files()?
    };

    // Stage 2: File pattern filtering
    if !criteria.file_patterns.is_empty() {
        let match_all = criteria.file_mode == MatchMode::All;
        let path_strs: Vec<String> = files.iter().map(|p| p.as_str().to_string()).collect();
        let filtered = patterns::filter_by_patterns(
            &path_strs,
            &criteria.file_patterns,
            criteria.regex_files,
            match_all,
        )?;
        let filtered_set: HashSet<&str> = filtered.iter().map(String::as_str).collect();
        files.retain(|p| filtered_set.contains(p.as_str()));
    }

    // Stage 3: Virtual tag filtering
    if !criteria.virtual_tags.is_empty() {
        files = apply_virtual_tags(files, &criteria.virtual_tags, criteria.virtual_mode)?;
    }

    Ok(files)
}

/// Expand tags using schema (aliases, hierarchy) and store (prefix matching).
///
/// For each input tag:
/// - Expands to all synonyms (canonical + all aliases)
/// - If `include_hierarchy` is true, also expands hierarchical parents
/// - If tag doesn't exist but has children (e.g., "lang" → "lang:rust"), expands to children
///
/// # Errors
///
/// Returns `StoreError` if store operations fail.
pub fn expand_tags(
    tags: &[String],
    schema: &TagSchema,
    store: &dyn TagStore,
    include_hierarchy: bool,
) -> Result<Vec<String>, StoreError> {
    let mut expanded = HashSet::new();

    let all_tags = store.list_all_tags()?;
    let all_tags_strs: Vec<&str> = all_tags.iter().map(TagName::as_ref).collect();
    let all_tags_set: HashSet<&str> = all_tags_strs.iter().copied().collect();

    for tag in tags {
        if include_hierarchy {
            let hierarchy_expanded = schema.expand_with_hierarchy(tag);

            let has_real_tag = hierarchy_expanded
                .iter()
                .any(|t| all_tags_set.contains(t.as_str()));

            if has_real_tag {
                expanded.extend(hierarchy_expanded);
            } else {
                let canonical = schema.canonicalize(tag);
                let prefix = format!("{canonical}{HIERARCHY_DELIMITER}");

                let children: Vec<_> = all_tags
                    .iter()
                    .filter(|t| t.as_ref().starts_with(&prefix))
                    .map(|t| t.as_ref().to_string())
                    .collect();

                if children.is_empty() {
                    expanded.insert(tag.clone());
                } else {
                    expanded.extend(children);
                }
            }
        } else {
            for synonym in schema.expand_synonyms(tag) {
                expanded.insert(synonym);
            }
        }
    }

    Ok(expanded.into_iter().collect())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Free-text search: combine regex tag matches + glob filename matches.
fn collect_freetext_matches(
    store: &dyn TagStore,
    query: &str,
) -> Result<Vec<TagrPath>, StoreError> {
    let files_by_tag = store.find_by_tag_regex(query)?;

    let all_files = store.list_all_files()?;
    let filename_pattern = format!("*{query}*");
    let path_strs: Vec<String> = all_files.iter().map(|p| p.as_str().to_string()).collect();
    let files_by_name =
        patterns::filter_by_patterns(&path_strs, &[filename_pattern], false, false)?;

    let mut file_set: HashSet<String> = files_by_tag
        .into_iter()
        .map(|p| p.as_str().to_string())
        .collect();
    file_set.extend(files_by_name);

    let mut result: Vec<TagrPath> = file_set
        .into_iter()
        .map(TagrPath::from_string)
        .collect();
    result.sort();
    Ok(result)
}

/// Evaluate a tag expression against the store with hierarchy awareness.
fn evaluate_tag_expr(
    store: &dyn TagStore,
    expr: &TagExpr,
    criteria: &QueryCriteria,
    schema: &TagSchema,
) -> Result<Vec<TagrPath>, StoreError> {
    if criteria.regex_tags {
        return evaluate_regex_tags(store, expr, criteria);
    }

    if criteria.expand_hierarchy {
        return evaluate_with_hierarchy(store, expr, criteria, schema);
    }

    // Flat (no-hierarchy) evaluation: use store index lookups directly
    let files = match expr {
        TagExpr::Tag(tag) => store.find_by_tag(tag)?,
        TagExpr::And(exprs) => {
            let flat_tags: Vec<&TagName> = exprs
                .iter()
                .filter_map(|e| match e {
                    TagExpr::Tag(t) => Some(t),
                    _ => None,
                })
                .collect();
            if flat_tags.is_empty() {
                store.list_all_files()?
            } else {
                let owned: Vec<TagName> = flat_tags.into_iter().cloned().collect();
                store.find_by_all_tags(&owned)?
            }
        }
        TagExpr::Or(exprs) => {
            let flat_tags: Vec<&TagName> = exprs
                .iter()
                .filter_map(|e| match e {
                    TagExpr::Tag(t) => Some(t),
                    _ => None,
                })
                .collect();
            if flat_tags.is_empty() {
                store.list_all_files()?
            } else {
                let owned: Vec<TagName> = flat_tags.into_iter().cloned().collect();
                store.find_by_any_tag(&owned)?
            }
        }
        TagExpr::Not(_) => store.list_all_files()?,
    };

    // Post-filter for Not expressions
    if needs_post_filter(expr) {
        let pairs = store.list_all()?;
        Ok(pairs
            .into_iter()
            .filter(|p| criteria.matches_pair(p))
            .map(|p| p.file)
            .collect())
    } else {
        Ok(files)
    }
}

/// Evaluate tag expression using regex matching.
fn evaluate_regex_tags(
    store: &dyn TagStore,
    expr: &TagExpr,
    criteria: &QueryCriteria,
) -> Result<Vec<TagrPath>, StoreError> {
    let regex_patterns = collect_tag_patterns(expr);
    if regex_patterns.is_empty() {
        return store.list_all_files();
    }

    let is_and = matches!(expr, TagExpr::And(_));

    if is_and {
        // Intersect: files matching ALL regex patterns
        let mut file_sets: Vec<HashSet<String>> = Vec::new();
        for pattern in &regex_patterns {
            let matching = store.find_by_tag_regex(pattern)?;
            file_sets.push(
                matching
                    .into_iter()
                    .map(|p| p.as_str().to_string())
                    .collect(),
            );
        }
        if file_sets.is_empty() {
            return Ok(Vec::new());
        }
        let first = file_sets.remove(0);
        let result: Vec<TagrPath> = first
            .into_iter()
            .filter(|f| file_sets.iter().all(|set| set.contains(f)))
            .map(TagrPath::from_string)
            .collect();
        Ok(result)
    } else {
        // Union: files matching ANY regex pattern
        let mut file_set = HashSet::new();
        for pattern in &regex_patterns {
            let matching = store.find_by_tag_regex(pattern)?;
            file_set.extend(matching.into_iter().map(|p| p.as_str().to_string()));
        }
        let mut result: Vec<TagrPath> = file_set
            .into_iter()
            .map(TagrPath::from_string)
            .collect();
        result.sort();

        // Post-filter for Not expressions
        if needs_post_filter_in_criteria(criteria) {
            let pairs = store.list_all()?;
            Ok(pairs
                .into_iter()
                .filter(|p| criteria.matches_pair(p))
                .map(|p| p.file)
                .collect())
        } else {
            Ok(result)
        }
    }
}

/// Evaluate tag expression with hierarchy expansion.
fn evaluate_with_hierarchy(
    store: &dyn TagStore,
    expr: &TagExpr,
    _criteria: &QueryCriteria,
    schema: &TagSchema,
) -> Result<Vec<TagrPath>, StoreError> {
    // Extract include/exclude tag patterns from the expression
    let include_patterns = collect_include_patterns(expr);
    let exclude_patterns = collect_exclude_patterns(expr);

    // Expand includes via schema synonyms only (hierarchy matching uses prefix logic)
    let expanded_includes: Vec<String> = include_patterns
        .iter()
        .flat_map(|t| schema.expand_synonyms(t))
        .collect();

    // Load all pairs for hierarchy-based filtering
    let all_pairs = store.list_all()?;

    let is_and = matches!(expr, TagExpr::And(_));

    let filtered: Vec<TagrPath> = if is_and && !expanded_includes.is_empty() {
        // ALL mode: file must have tags matching ALL original include patterns
        // (each pattern may match via hierarchy prefix)
        all_pairs
            .into_iter()
            .filter(|pair| {
                let tags: Vec<&str> = pair.tags.iter().map(TagName::as_ref).collect();

                // Check ALL include patterns match (each via prefix matching)
                let all_match = expanded_includes.iter().all(|pattern| {
                    tags.iter()
                        .any(|tag| hierarchy::pattern_matches(pattern, tag))
                });
                if !all_match {
                    return false;
                }

                // Check exclude patterns via specificity rules
                if exclude_patterns.is_empty() {
                    true
                } else {
                    let tag_strings: Vec<String> =
                        tags.iter().map(|t| (*t).to_string()).collect();
                    hierarchy::should_include_file(&tag_strings, &expanded_includes, &exclude_patterns)
                }
            })
            .map(|p| p.file)
            .collect()
    } else if !expanded_includes.is_empty() {
        // ANY mode: file must have tags matching ANY include pattern
        let tag_str_pairs: Vec<(String, Vec<String>)> = all_pairs
            .into_iter()
            .map(|pair| {
                let file = pair.file.as_str().to_string();
                let tags: Vec<String> =
                    pair.tags.iter().map(|t| t.as_ref().to_string()).collect();
                (file, tags)
            })
            .collect();

        let filtered_paths = hierarchy::filter_by_hierarchy(
            tag_str_pairs
                .iter()
                .map(|(f, tags)| (f.as_str(), tags.as_slice())),
            &expanded_includes,
            &exclude_patterns,
        );

        filtered_paths
            .into_iter()
            .map(TagrPath::from_string)
            .collect()
    } else if !exclude_patterns.is_empty() {
        // Only excludes, no includes — return all files minus excluded
        all_pairs
            .into_iter()
            .filter(|pair| {
                let tag_strings: Vec<String> =
                    pair.tags.iter().map(|t| t.as_ref().to_string()).collect();
                let no_includes: &[String] = &[];
                hierarchy::should_include_file(&tag_strings, no_includes, &exclude_patterns)
            })
            .map(|p| p.file)
            .collect()
    } else {
        store.list_all_files()?
    };

    Ok(filtered)
}

/// Apply virtual tag filtering with rayon parallelism.
fn apply_virtual_tags(
    files: Vec<TagrPath>,
    virtual_tags: &[String],
    mode: MatchMode,
) -> Result<Vec<TagrPath>, StoreError> {
    use crate::vtags::{VirtualTag, VirtualTagConfig, VirtualTagEvaluator};
    use rayon::prelude::*;
    use std::time::Duration;

    let config = VirtualTagConfig::default();

    let parsed_tags: Vec<VirtualTag> = virtual_tags
        .iter()
        .map(|s| VirtualTag::parse_with_config(s, &config))
        .collect::<std::result::Result<_, _>>()
        .map_err(|e| StoreError::IoFailed {
            context: format!("invalid virtual tag: {e}"),
            source: std::io::Error::new(std::io::ErrorKind::InvalidInput, e.to_string()),
        })?;

    let cache_ttl = Duration::from_secs(config.cache_ttl_seconds);

    let filtered: Vec<TagrPath> = files
        .into_par_iter()
        .filter(|path| {
            let mut evaluator = VirtualTagEvaluator::new(cache_ttl, config.clone());
            let path_buf: std::path::PathBuf = path.as_path().to_path_buf();
            match mode {
                MatchMode::All => parsed_tags
                    .iter()
                    .all(|vtag| evaluator.matches(&path_buf, vtag).unwrap_or(false)),
                MatchMode::Any => parsed_tags
                    .iter()
                    .any(|vtag| evaluator.matches(&path_buf, vtag).unwrap_or(false)),
            }
        })
        .collect();

    Ok(filtered)
}

/// Check if a `TagExpr` contains `Not` nodes requiring post-filtering.
fn needs_post_filter(expr: &TagExpr) -> bool {
    match expr {
        TagExpr::Tag(_) => false,
        TagExpr::Not(_) => true,
        TagExpr::And(exprs) | TagExpr::Or(exprs) => exprs.iter().any(needs_post_filter),
    }
}

/// Check if criteria has Not in its tag expression.
fn needs_post_filter_in_criteria(criteria: &QueryCriteria) -> bool {
    criteria
        .tag_expr
        .as_ref()
        .is_some_and(needs_post_filter)
}

/// Extract flat tag pattern strings from a `TagExpr` (for regex evaluation).
fn collect_tag_patterns(expr: &TagExpr) -> Vec<String> {
    match expr {
        TagExpr::Tag(t) => vec![t.as_ref().to_string()],
        TagExpr::Not(inner) => collect_tag_patterns(inner),
        TagExpr::And(exprs) | TagExpr::Or(exprs) => {
            exprs.iter().flat_map(collect_tag_patterns).collect()
        }
    }
}

/// Extract include tag patterns (positive `Tag` nodes) from a `TagExpr`.
fn collect_include_patterns(expr: &TagExpr) -> Vec<String> {
    match expr {
        TagExpr::Tag(t) => vec![t.as_ref().to_string()],
        TagExpr::Not(_) => Vec::new(),
        TagExpr::And(exprs) | TagExpr::Or(exprs) => {
            exprs.iter().flat_map(collect_include_patterns).collect()
        }
    }
}

/// Extract exclude tag patterns (`Not(Tag(...))` nodes) from a `TagExpr`.
fn collect_exclude_patterns(expr: &TagExpr) -> Vec<String> {
    match expr {
        TagExpr::Tag(_) => Vec::new(),
        TagExpr::Not(inner) => match inner.as_ref() {
            TagExpr::Tag(t) => vec![t.as_ref().to_string()],
            other => collect_exclude_patterns(other),
        },
        TagExpr::And(exprs) | TagExpr::Or(exprs) => {
            exprs.iter().flat_map(collect_exclude_patterns).collect()
        }
    }
}
