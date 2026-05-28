//! `QueryCriteria`, `TagExpr`, and `MatchMode` — unified query types.

use std::collections::HashSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::pair::Pair;
use super::tag_name::TagName;

/// AND/OR matching semantics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MatchMode {
    /// All items must match.
    #[default]
    All,
    /// Any item may match.
    Any,
}

impl fmt::Display for MatchMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::All => write!(f, "all"),
            Self::Any => write!(f, "any"),
        }
    }
}

/// Boolean expression tree for tag matching.
///
/// Supports arbitrary nesting: `(A & (B | C) & !D)`.
/// Simple cases map trivially: a flat tag list with AND mode becomes
/// `And(vec![Tag(a), Tag(b)])`.
///
/// # Examples
///
/// ```
/// use tagr::types::{TagExpr, TagName};
///
/// // Simple AND: files tagged with both "rust" AND "cli"
/// let expr = TagExpr::And(vec![
///     TagExpr::Tag(TagName::new("rust").unwrap()),
///     TagExpr::Tag(TagName::new("cli").unwrap()),
/// ]);
///
/// // With exclusion: "rust" AND NOT "deprecated"
/// let expr = TagExpr::And(vec![
///     TagExpr::Tag(TagName::new("rust").unwrap()),
///     TagExpr::Not(Box::new(TagExpr::Tag(TagName::new("deprecated").unwrap()))),
/// ]);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TagExpr {
    /// Matches a single tag (leaf node).
    Tag(TagName),
    /// Negation — matches files that do NOT match the inner expression.
    Not(Box<Self>),
    /// Conjunction — all sub-expressions must match.
    And(Vec<Self>),
    /// Disjunction — any sub-expression must match.
    Or(Vec<Self>),
}

impl TagExpr {
    /// Evaluate this expression against a set of tags.
    ///
    /// Uses prefix matching for hierarchy: `Tag("language")` matches a file
    /// that has `"language:rust"` in its tag set.
    #[must_use]
    pub fn matches(&self, file_tags: &[TagName]) -> bool {
        match self {
            Self::Tag(pattern) => file_tags.iter().any(|t| t.matches_pattern(pattern)),
            Self::Not(inner) => !inner.matches(file_tags),
            Self::And(exprs) => exprs.iter().all(|e| e.matches(file_tags)),
            Self::Or(exprs) => exprs.iter().any(|e| e.matches(file_tags)),
        }
    }
}

/// Unified search parameters.
///
/// All query consumers use this single type.
///
/// An empty `QueryCriteria` (all fields at defaults) matches every file
/// in the database.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryCriteria {
    /// Tag filter expression. `None` = no tag filtering (matches all files).
    pub tag_expr: Option<TagExpr>,

    /// Whether tag names in `tag_expr` should be treated as regex patterns.
    pub regex_tags: bool,

    /// Whether to expand tags through hierarchy (parent matches children).
    /// Defaults to `true` via `Default`.
    pub expand_hierarchy: bool,

    /// File path patterns (glob or regex depending on `regex_files`).
    pub file_patterns: Vec<String>,

    /// Matching mode for file patterns.
    pub file_mode: MatchMode,

    /// Whether `file_patterns` are regex (true) or glob (false).
    pub regex_files: bool,

    /// Virtual tag expressions (e.g. `"size:>1MB"`, `"modified:today"`).
    pub virtual_tags: Vec<String>,

    /// Matching mode for virtual tags.
    pub virtual_mode: MatchMode,

    /// Free-text query string for fuzzy matching.
    pub query: Option<String>,
}

impl QueryCriteria {
    /// Returns `true` if no filtering criteria are set (matches everything).
    #[must_use]
    #[allow(clippy::missing_const_for_fn)] // Vec::is_empty() is not const-stable
    pub fn is_empty(&self) -> bool {
        self.tag_expr.is_none()
            && !self.regex_tags
            && self.file_patterns.is_empty()
            && self.virtual_tags.is_empty()
            && self.query.is_none()
    }

    /// Toggle a tag in the include set (flat AND/OR mode).
    ///
    /// If the tag is already in a flat include list, removes it.
    /// If not, adds it. Returns `true` if the tag is now included.
    ///
    /// For complex (non-flat) expressions, adds the tag as an additional
    /// AND clause.
    /// # Panics
    ///
    /// Cannot panic — `take()` is called only when `self.tag_expr` is `Some`.
    pub fn toggle_include_tag(&mut self, tag: TagName) -> bool {
        match &mut self.tag_expr {
            None => {
                self.tag_expr = Some(TagExpr::Tag(tag));
                true
            }
            Some(TagExpr::Tag(existing)) if *existing == tag => {
                self.tag_expr = None;
                false
            }
            // Single tag (different) or Not — wrap in And with new tag
            Some(TagExpr::Tag(_) | TagExpr::Not(_)) => {
                let existing = self.tag_expr.take().unwrap();
                self.tag_expr = Some(TagExpr::And(vec![existing, TagExpr::Tag(tag)]));
                true
            }
            Some(TagExpr::And(exprs) | TagExpr::Or(exprs)) => {
                if let Some(pos) =
                    exprs
                        .iter()
                        .position(|e| matches!(e, TagExpr::Tag(t) if *t == tag))
                {
                    exprs.remove(pos);
                    collapse_tag_expr(&mut self.tag_expr);
                    false
                } else {
                    exprs.push(TagExpr::Tag(tag));
                    true
                }
            }
        }
    }

    /// Toggle a tag in the exclude set (wraps in `Not`).
    ///
    /// If the tag is already excluded (as `Not(Tag(tag))`), removes the exclusion.
    /// Otherwise adds `Not(Tag(tag))` to the expression. Returns `true` if
    /// the tag is now excluded.
    /// # Panics
    ///
    /// Cannot panic — `take()` is called only when `self.tag_expr` is `Some`.
    pub fn toggle_exclude_tag(&mut self, tag: &TagName) -> bool {
        let not_tag = TagExpr::Not(Box::new(TagExpr::Tag(tag.clone())));

        match &mut self.tag_expr {
            None => {
                self.tag_expr = Some(not_tag);
                true
            }
            Some(TagExpr::Not(inner))
                if matches!(inner.as_ref(), TagExpr::Tag(t) if *t == *tag) =>
            {
                self.tag_expr = None;
                false
            }
            Some(TagExpr::And(exprs)) => {
                if let Some(pos) = exprs.iter().position(|e| is_not_tag(e, tag)) {
                    exprs.remove(pos);
                    collapse_tag_expr(&mut self.tag_expr);
                    false
                } else {
                    exprs.push(not_tag);
                    true
                }
            }
            Some(_) => {
                let existing = self.tag_expr.take().unwrap();
                self.tag_expr = Some(TagExpr::And(vec![existing, not_tag]));
                true
            }
        }
    }

    /// Switch between AND/OR for the top-level tag expression.
    ///
    /// Converts `And(exprs)` ↔ `Or(exprs)`. Has no effect on single tags
    /// or complex nested expressions.
    pub fn toggle_tag_mode(&mut self) {
        match self.tag_expr.take() {
            Some(TagExpr::And(exprs)) => self.tag_expr = Some(TagExpr::Or(exprs)),
            Some(TagExpr::Or(exprs)) => self.tag_expr = Some(TagExpr::And(exprs)),
            other => self.tag_expr = other,
        }
    }

    /// Extract flat include tag list for TUI display.
    ///
    /// Returns `Some` with the set of directly included tags if the expression
    /// is a flat structure (single tag, or `And`/`Or` of tags with optional `Not` children).
    /// Returns `None` if the expression is too complex to flatten.
    ///
    /// **Note:** Returns `HashSet<&TagName>`. Lookups must use `&TagName`, not `&str`.
    #[must_use]
    pub fn flat_include_tags(&self) -> Option<HashSet<&TagName>> {
        match &self.tag_expr {
            None | Some(TagExpr::Not(_)) => Some(HashSet::new()),
            Some(TagExpr::Tag(t)) => Some(HashSet::from([t])),
            Some(TagExpr::And(exprs) | TagExpr::Or(exprs)) => {
                let mut set = HashSet::new();
                for expr in exprs {
                    match expr {
                        TagExpr::Tag(t) => {
                            set.insert(t);
                        }
                        TagExpr::Not(_) => {}
                        TagExpr::And(_) | TagExpr::Or(_) => return None,
                    }
                }
                Some(set)
            }
        }
    }

    /// Extract flat exclude tag list for TUI display.
    ///
    /// Returns `Some` with the set of directly excluded tags if the expression
    /// is a flat structure. Returns `None` if the expression is too complex.
    ///
    /// **Note:** Returns `HashSet<&TagName>`. Same lookup constraints as
    /// [`flat_include_tags`](Self::flat_include_tags).
    #[must_use]
    pub fn flat_exclude_tags(&self) -> Option<HashSet<&TagName>> {
        match &self.tag_expr {
            None | Some(TagExpr::Tag(_)) => Some(HashSet::new()),
            Some(TagExpr::Not(inner)) => match inner.as_ref() {
                TagExpr::Tag(t) => Some(HashSet::from([t])),
                _ => None,
            },
            Some(TagExpr::And(exprs) | TagExpr::Or(exprs)) => {
                let mut set = HashSet::new();
                for expr in exprs {
                    match expr {
                        TagExpr::Not(inner) => match inner.as_ref() {
                            TagExpr::Tag(t) => {
                                set.insert(t);
                            }
                            _ => return None,
                        },
                        TagExpr::Tag(_) => {}
                        TagExpr::And(_) | TagExpr::Or(_) => return None,
                    }
                }
                Some(set)
            }
        }
    }

    /// Evaluate whether a [`Pair`] matches this criteria.
    ///
    /// Used by `DaemonStore` for local cache filtering. Only evaluates
    /// tag expressions and file patterns — virtual tags and free-text query
    /// are not evaluated locally (they require filesystem access or fuzzy
    /// matching that the daemon already resolved).
    #[must_use]
    pub fn matches_pair(&self, pair: &Pair) -> bool {
        if let Some(ref expr) = self.tag_expr
            && !expr.matches(&pair.tags)
        {
            return false;
        }

        if !self.file_patterns.is_empty() && !self.regex_files {
            let path_str = pair.file.as_str();
            let matches_pattern = |pattern: &str| {
                glob::Pattern::new(pattern).is_ok_and(|p| p.matches(path_str))
            };

            match self.file_mode {
                MatchMode::All => {
                    if !self.file_patterns.iter().all(|p| matches_pattern(p)) {
                        return false;
                    }
                }
                MatchMode::Any => {
                    if !self.file_patterns.iter().any(|p| matches_pattern(p)) {
                        return false;
                    }
                }
            }
        }

        true
    }

    /// Generate a CLI-equivalent string for this criteria.
    ///
    /// Used by TUI status bar to show users the headless command equivalent.
    #[must_use]
    pub fn to_cli_string(&self) -> String {
        let mut parts = Vec::new();

        if let Some(ref expr) = self.tag_expr {
            collect_cli_tags(expr, &mut parts);
        }

        for pattern in &self.file_patterns {
            parts.push(format!("--file-pattern {pattern}"));
        }

        for vtag in &self.virtual_tags {
            parts.push(format!("--vtag {vtag}"));
        }

        if let Some(ref q) = self.query {
            parts.push(format!("--query {q}"));
        }

        if parts.is_empty() {
            "tagr search".to_string()
        } else {
            format!("tagr search {}", parts.join(" "))
        }
    }
}

/// After removing an element from an `And`/`Or` vec, collapse to single expr or `None`.
///
/// Called after `exprs.remove(pos)` while the mutable borrow on `exprs` is still
/// held indirectly through `tag_expr`. Works by re-matching to avoid double borrows.
fn collapse_tag_expr(tag_expr: &mut Option<TagExpr>) {
    let should_collapse = matches!(tag_expr, Some(TagExpr::And(exprs) | TagExpr::Or(exprs)) if exprs.len() <= 1);
    if should_collapse {
        match tag_expr.take() {
            Some(TagExpr::And(mut exprs) | TagExpr::Or(mut exprs)) => {
                *tag_expr = if exprs.len() == 1 {
                    Some(exprs.remove(0))
                } else {
                    None
                };
            }
            other => *tag_expr = other,
        }
    }
}

/// Check if a `TagExpr` is `Not(Tag(target))`.
fn is_not_tag(expr: &TagExpr, target: &TagName) -> bool {
    matches!(expr, TagExpr::Not(inner) if matches!(inner.as_ref(), TagExpr::Tag(t) if *t == *target))
}

/// Collect CLI flag representations from a tag expression.
fn collect_cli_tags(expr: &TagExpr, parts: &mut Vec<String>) {
    match expr {
        TagExpr::Tag(t) => parts.push(format!("-t {t}")),
        TagExpr::Not(inner) => match inner.as_ref() {
            TagExpr::Tag(t) => parts.push(format!("-x {t}")),
            _ => parts.push(format!("-x \"({inner:?})\"")),
        },
        TagExpr::And(exprs) | TagExpr::Or(exprs) => {
            for e in exprs {
                collect_cli_tags(e, parts);
            }
        }
    }
}
