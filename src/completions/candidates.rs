//! Static completion candidates
//!
//! These candidates are always available (no feature flag required) because
//! they don't need database or file system access.

use super::Candidate;

/// Known configuration keys for `config set/get`
///
/// Returns all valid configuration keys with help text.
#[must_use]
pub fn config_keys() -> Vec<Candidate> {
    vec![
        Candidate::new("quiet").with_help("Suppress informational output"),
        Candidate::new("path_format").with_help("Display paths as 'absolute' or 'relative'"),
    ]
}

/// Virtual tag type prefixes for `-v/--virtual-tag`
///
/// Note: This completer is EXCLUSIVELY for virtual tags.
/// Database tags use `-t/--tag` with a separate completer.
#[must_use]
pub fn vtag_types() -> Vec<Candidate> {
    vec![
        // Time-based vtags
        Candidate::new("modified:").with_help("Filter by modification time"),
        Candidate::new("accessed:").with_help("Filter by last access time"),
        Candidate::new("created:").with_help("Filter by creation time"),
        // Size vtags
        Candidate::new("size:").with_help("Filter by file size"),
        // Type vtags
        Candidate::new("ext:").with_help("Filter by file extension"),
        Candidate::new("type:").with_help("Filter by file type category"),
        Candidate::new("mime:").with_help("Filter by MIME type"),
        // Permission vtags
        Candidate::new("perm:").with_help("Filter by permissions"),
        // Content vtags
        Candidate::new("empty:").with_help("Filter empty/non-empty files"),
        Candidate::new("hidden:").with_help("Filter hidden files"),
        // Git vtags
        Candidate::new("git:").with_help("Filter by git status"),
        // Depth vtags
        Candidate::new("depth:").with_help("Filter by directory depth"),
    ]
}

/// Time-based virtual tag values
#[must_use]
pub fn vtag_time_values() -> Vec<Candidate> {
    vec![
        Candidate::new("today"),
        Candidate::new("yesterday"),
        Candidate::new("this-week"),
        Candidate::new("last-week"),
        Candidate::new("this-month"),
        Candidate::new("last-month"),
        Candidate::new("this-year"),
        Candidate::new("last-7-days"),
        Candidate::new("last-30-days"),
        Candidate::new("last-90-days"),
        Candidate::new("older-than-1-year"),
    ]
}

/// Size-based virtual tag values
#[must_use]
pub fn vtag_size_values() -> Vec<Candidate> {
    vec![
        Candidate::new("empty").with_help("0 bytes"),
        Candidate::new("tiny").with_help("<1KB"),
        Candidate::new("small").with_help("1KB-100KB"),
        Candidate::new("medium").with_help("100KB-1MB"),
        Candidate::new("large").with_help("1MB-100MB"),
        Candidate::new("huge").with_help(">100MB"),
        Candidate::new(">1MB"),
        Candidate::new("<1KB"),
        Candidate::new(">10MB"),
        Candidate::new("<100KB"),
    ]
}

/// Permission-based virtual tag values
#[must_use]
pub fn vtag_permission_values() -> Vec<Candidate> {
    vec![
        Candidate::new("executable"),
        Candidate::new("readable"),
        Candidate::new("writable"),
        Candidate::new("readonly"),
    ]
}

/// Git status virtual tag values
#[must_use]
pub fn vtag_git_values() -> Vec<Candidate> {
    vec![
        Candidate::new("tracked"),
        Candidate::new("untracked"),
        Candidate::new("modified"),
        Candidate::new("staged"),
        Candidate::new("ignored"),
        Candidate::new("clean"),
    ]
}

/// File type category values
#[must_use]
pub fn vtag_type_values() -> Vec<Candidate> {
    vec![
        Candidate::new("source").with_help("Source code files"),
        Candidate::new("document").with_help("Documents (pdf, doc, txt)"),
        Candidate::new("image").with_help("Image files"),
        Candidate::new("video").with_help("Video files"),
        Candidate::new("audio").with_help("Audio files"),
        Candidate::new("archive").with_help("Archive files (zip, tar, etc)"),
        Candidate::new("config").with_help("Configuration files"),
        Candidate::new("data").with_help("Data files (json, csv, etc)"),
    ]
}

/// Boolean virtual tag values
#[must_use]
pub fn vtag_bool_values() -> Vec<Candidate> {
    vec![Candidate::new("true"), Candidate::new("false")]
}

/// Context-aware virtual tag completion
///
/// Provides smart completion based on what the user has typed:
/// - No prefix: suggest vtag types (modified:, size:, etc.)
/// - With prefix: suggest values for that type
///
/// # Arguments
/// * `current` - What the user has typed so far
#[must_use]
pub fn complete_vtag(current: &str) -> Vec<Candidate> {
    // Check if we have a vtag type prefix
    if let Some(colon_pos) = current.find(':') {
        let prefix = &current[..colon_pos];
        let value_so_far = &current[colon_pos + 1..];

        // Return value candidates for the specific type
        let values = match prefix {
            "modified" | "accessed" | "created" => vtag_time_values(),
            "size" => vtag_size_values(),
            "perm" => vtag_permission_values(),
            "git" => vtag_git_values(),
            "type" => vtag_type_values(),
            "empty" | "hidden" => vtag_bool_values(),
            _ => vec![],
        };

        // Filter by what user has typed and prepend the prefix
        values
            .into_iter()
            .filter(|c| c.value.starts_with(value_so_far))
            .map(|c| Candidate {
                value: format!("{}:{}", prefix, c.value),
                help: c.help,
            })
            .collect()
    } else {
        // No colon yet - suggest vtag types
        vtag_types()
            .into_iter()
            .filter(|c| c.value.starts_with(current))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_keys_contains_quiet() {
        let keys = config_keys();
        assert!(keys.iter().any(|c| c.value == "quiet"));
    }

    #[test]
    fn test_vtag_types_contains_modified() {
        let types = vtag_types();
        assert!(types.iter().any(|c| c.value == "modified:"));
    }

    #[test]
    fn test_vtag_types_contains_git() {
        let types = vtag_types();
        assert!(types.iter().any(|c| c.value == "git:"));
    }

    #[test]
    fn test_complete_vtag_no_prefix() {
        let candidates = complete_vtag("mod");
        assert!(candidates.iter().any(|c| c.value == "modified:"));
    }

    #[test]
    fn test_complete_vtag_with_prefix() {
        let candidates = complete_vtag("modified:tod");
        assert!(candidates.iter().any(|c| c.value == "modified:today"));
    }

    #[test]
    fn test_complete_vtag_size_values() {
        let candidates = complete_vtag("size:");
        assert!(candidates.iter().any(|c| c.value == "size:empty"));
        assert!(candidates.iter().any(|c| c.value == "size:large"));
    }
}
