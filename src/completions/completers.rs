//! Dynamic completion implementations
//!
//! These completers query the cache/database for context-aware suggestions.
//! Only available with the `dynamic-completions` feature.

use super::cache::{load_cached_databases, load_cached_filters, load_cached_tags};
use super::traits::{Candidate, DynamicCompleter};
use std::ffi::OsStr;

/// Complete tags from the completion cache
///
/// Provides:
/// - All tags from the cached database state
/// - Fuzzy prefix matching
///
/// Note: `-t/--tag` is EXCLUSIVELY for database tags.
/// Virtual tags use `-v/--virtual-tag` with a separate completer.
pub struct TagCompleter;

impl DynamicCompleter for TagCompleter {
    fn complete(&self, current: &OsStr) -> Vec<Candidate> {
        let current = current.to_string_lossy();
        let current_lower = current.to_lowercase();

        let tags = load_cached_tags();

        tags.into_iter()
            .filter(|tag| {
                let tag_lower = tag.to_lowercase();
                tag_lower.starts_with(&current_lower) || tag_lower.contains(&current_lower)
            })
            .take(50) // Limit results for performance
            .map(Candidate::new)
            .collect()
    }
}

/// Complete filter names from cache
pub struct FilterCompleter;

impl DynamicCompleter for FilterCompleter {
    fn complete(&self, current: &OsStr) -> Vec<Candidate> {
        let current = current.to_string_lossy();
        let current_lower = current.to_lowercase();

        let filters = load_cached_filters();

        filters
            .into_iter()
            .filter(|(name, _)| {
                let name_lower = name.to_lowercase();
                name_lower.starts_with(&current_lower)
            })
            .take(50)
            .map(|(name, desc)| {
                let mut candidate = Candidate::new(name);
                if let Some(d) = desc {
                    candidate = candidate.with_help(d);
                }
                candidate
            })
            .collect()
    }
}

/// Complete database names from cache
pub struct DatabaseCompleter;

impl DynamicCompleter for DatabaseCompleter {
    fn complete(&self, current: &OsStr) -> Vec<Candidate> {
        let current = current.to_string_lossy();
        let current_lower = current.to_lowercase();

        let databases = load_cached_databases();

        databases
            .into_iter()
            .filter(|(name, _)| {
                let name_lower = name.to_lowercase();
                name_lower.starts_with(&current_lower)
            })
            .map(|(name, is_default)| {
                let mut candidate = Candidate::new(&name);
                if is_default {
                    candidate = candidate.with_help("default");
                }
                candidate
            })
            .collect()
    }
}

/// Smart tag completer with hierarchy awareness
///
/// Features:
/// - Shows top-level tags first
/// - When user types `lang:`, shows only children under that prefix
/// - Suggests colons after hierarchy roots
///
/// Note: Uses cache, not live database lookup
pub struct HierarchicalTagCompleter;

impl DynamicCompleter for HierarchicalTagCompleter {
    fn complete(&self, current: &OsStr) -> Vec<Candidate> {
        let current = current.to_string_lossy();
        let tags = load_cached_tags();

        if current.is_empty() {
            // Show top-level tags and hierarchy roots
            let mut roots: Vec<String> = Vec::new();
            let mut seen_prefixes = std::collections::HashSet::new();

            for tag in &tags {
                if let Some(colon_pos) = tag.find(':') {
                    let prefix = &tag[..colon_pos + 1]; // Include colon
                    if seen_prefixes.insert(prefix.to_string()) {
                        roots.push(prefix.to_string());
                    }
                } else {
                    roots.push(tag.clone());
                }
            }

            roots.sort();
            roots.dedup();

            return roots
                .into_iter()
                .take(50)
                .map(|t| {
                    if t.ends_with(':') {
                        Candidate::new(&t).with_help("hierarchy")
                    } else {
                        Candidate::new(t)
                    }
                })
                .collect();
        }

        // Check if user is typing within a hierarchy
        if let Some(colon_pos) = current.rfind(':') {
            let prefix = &current[..=colon_pos];
            let suffix = &current[colon_pos + 1..];

            // Find all tags under this hierarchy
            let children: Vec<_> = tags
                .iter()
                .filter(|t| t.starts_with(prefix))
                .filter(|t| {
                    let child_part = &t[prefix.len()..];
                    child_part
                        .to_lowercase()
                        .starts_with(&suffix.to_lowercase())
                })
                .take(50)
                .map(|t| Candidate::new(t.clone()))
                .collect();

            return children;
        }

        // Regular prefix matching
        tags.into_iter()
            .filter(|t| t.to_lowercase().starts_with(&current.to_lowercase()))
            .take(50)
            .map(Candidate::new)
            .collect()
    }
}

/// Virtual tag completer with context awareness
///
/// This completer handles `-v/--virtual-tag` arguments.
/// It NEVER suggests database tags - that's for `-t/--tag`.
pub struct VirtualTagCompleter;

impl DynamicCompleter for VirtualTagCompleter {
    fn complete(&self, current: &OsStr) -> Vec<Candidate> {
        let current = current.to_string_lossy();
        super::candidates::complete_vtag(&current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tag_completer_empty_input() {
        let completer = TagCompleter;
        // With empty cache, should return empty
        let results = completer.complete(OsStr::new(""));
        // Can be empty if no cache - that's expected
        assert!(results.len() <= 50);
    }

    #[test]
    fn test_filter_completer_empty_input() {
        let completer = FilterCompleter;
        let results = completer.complete(OsStr::new(""));
        assert!(results.len() <= 50);
    }

    #[test]
    fn test_database_completer_empty_input() {
        let completer = DatabaseCompleter;
        let results = completer.complete(OsStr::new(""));
        assert!(results.len() <= 50);
    }

    #[test]
    fn test_virtual_tag_completer() {
        let completer = VirtualTagCompleter;
        let results = completer.complete(OsStr::new("mod"));
        assert!(results.iter().any(|c| c.value == "modified:"));
    }
}
