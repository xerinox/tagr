//! Query bridge — adapts legacy `SearchParams` to the new query engine.
//!
//! This module provides `apply_search_params()` as a backward-compatible
//! wrapper that converts `SearchParams` → `QueryCriteria` and delegates
//! to `query::execute()` via `DirectStore`.
//!
//! **Temporary bridge** — will be deleted in Phase 5/6 when all callers
//! adopt `QueryCriteria` and `TagStore` directly.

use crate::cli::{SearchMode, SearchParams};
use crate::db::{Database, DbError};
use crate::store::DirectStore;
use crate::types::{MatchMode, QueryCriteria, TagExpr, TagName, TagrPath};
use std::path::PathBuf;

/// Convert `SearchParams` to `QueryCriteria`.
///
/// **Temporary bridge** — deleted in Phase 5 when `SearchParams` is removed.
///
/// Mapping:
/// - `tags` + `tag_mode` → `tag_expr` (And/Or of Tag nodes)
/// - `exclude_tags` → `Not(Tag(...))` nodes merged into `tag_expr`
/// - `no_hierarchy` → `expand_hierarchy = !no_hierarchy`
/// - `regex_tag` → `regex_tags`
/// - All other fields map directly.
#[must_use]
pub fn search_params_to_criteria(params: &SearchParams) -> QueryCriteria {
    let mode = match params.tag_mode {
        SearchMode::All => MatchMode::All,
        SearchMode::Any => MatchMode::Any,
    };

    // Build include tag expressions
    let include_exprs: Vec<TagExpr> = params
        .tags
        .iter()
        .filter_map(|t| TagName::new(t).ok().map(TagExpr::Tag))
        .collect();

    // Build exclude tag expressions
    let exclude_exprs: Vec<TagExpr> = params
        .exclude_tags
        .iter()
        .filter_map(|t| TagName::new(t).ok().map(|tn| TagExpr::Not(Box::new(TagExpr::Tag(tn)))))
        .collect();

    // Combine into a single expression
    let mut all_exprs = include_exprs;
    all_exprs.extend(exclude_exprs);

    let tag_expr = match all_exprs.len() {
        0 => None,
        1 => Some(all_exprs.into_iter().next().unwrap_or_else(|| unreachable!())),
        _ => match mode {
            MatchMode::All => Some(TagExpr::And(all_exprs)),
            MatchMode::Any => {
                // In ANY mode with excludes, wrap includes in Or, then And with excludes
                let (includes, excludes): (Vec<_>, Vec<_>) = all_exprs
                    .into_iter()
                    .partition(|e| !matches!(e, TagExpr::Not(_)));
                if excludes.is_empty() {
                    Some(TagExpr::Or(includes))
                } else if includes.is_empty() {
                    Some(TagExpr::And(excludes))
                } else {
                    let include_expr = if includes.len() == 1 {
                        includes.into_iter().next().unwrap_or_else(|| unreachable!())
                    } else {
                        TagExpr::Or(includes)
                    };
                    let mut combined = vec![include_expr];
                    combined.extend(excludes);
                    Some(TagExpr::And(combined))
                }
            }
        },
    };

    QueryCriteria {
        tag_expr,
        regex_tags: params.regex_tag,
        expand_hierarchy: !params.no_hierarchy,
        file_patterns: params.file_patterns.clone(),
        file_mode: match params.file_mode {
            SearchMode::All => MatchMode::All,
            SearchMode::Any => MatchMode::Any,
        },
        regex_files: params.regex_file,
        virtual_tags: params.virtual_tags.clone(),
        virtual_mode: match params.virtual_mode {
            SearchMode::All => MatchMode::All,
            SearchMode::Any => MatchMode::Any,
        },
        query: params.query.clone(),
    }
}

/// Apply search parameters to build a filtered file list.
///
/// **Backward-compatible bridge** — converts `SearchParams` → `QueryCriteria`,
/// wraps `Database` in `DirectStore`, and calls `query::execute()`.
///
/// # Errors
/// Returns `DbError` if database operations fail or pattern validation fails.
pub fn apply_search_params(db: &Database, params: &SearchParams) -> Result<Vec<PathBuf>, DbError> {
    let criteria = search_params_to_criteria(params);

    let store = DirectStore::new(db.clone());

    let schema = crate::schema::load_default_schema().unwrap_or_default();

    let results = crate::query::execute(&store, &criteria, &schema)
        .map_err(|e| DbError::InvalidInput(format!("query engine error: {e}")))?;

    Ok(results
        .into_iter()
        .map(TagrPath::into_path_buf)
        .collect())
}
