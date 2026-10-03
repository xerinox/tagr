//! Shell completion support for tagr
//!
//! Provides context-aware completions for:
//! - Tags from database
//! - File paths
//! - Filter names
//! - Database names
//! - Virtual tag types and values
//! - Config keys
//!
//! # Architecture
//!
//! This module uses a hybrid approach:
//! - **Static completions** (always available): subcommands, flags, paths, vtag types, config keys
//! - **Dynamic completions** (behind `dynamic-completions` feature): tags, filters, databases
//!
//! # Feature Flags
//!
//! - Default: Static completions only (no extra dependencies)
//! - `dynamic-completions`: Enables database/config lookups for smart completions

mod candidates;
mod traits;

#[cfg(feature = "dynamic-completions")]
mod cache;
#[cfg(feature = "dynamic-completions")]
mod completers;

pub use candidates::*;
pub use traits::*;

#[cfg(feature = "dynamic-completions")]
pub use cache::*;
#[cfg(feature = "dynamic-completions")]
pub use completers::*;

use clap::Command;
use clap_complete::Shell;
use std::io::Write;

/// Generate static shell completions to stdout
///
/// This generates traditional shell completion scripts that provide
/// static completions for commands, flags, and value hints.
///
/// # Arguments
/// * `shell` - Target shell (bash, zsh, fish, powershell, elvish)
/// * `cmd` - The clap Command to generate completions for
pub fn generate_static<W: Write>(shell: Shell, cmd: &mut Command, buf: &mut W) {
    clap_complete::generate(shell, cmd, cmd.get_name().to_string(), buf);
}

/// Initialize dynamic completion system
///
/// Call this at the start of `main()` before argument parsing when
/// the `dynamic-completions` feature is enabled.
///
/// This checks for the `COMPLETE` environment variable and handles
/// completion requests before normal command execution.
#[cfg(feature = "dynamic-completions")]
pub fn init_dynamic_completions<F: Fn() -> Command>(factory: F) {
    clap_complete::CompleteEnv::with_factory(factory).complete();
}

// =============================================================================
// Functions for ArgValueCompleter (used by cli.rs)
// =============================================================================

/// Complete tags for `-t/--tag` argument
///
/// This is the entry point for `ArgValueCompleter`. It returns candidates
/// for database tags based on what the user has typed.
///
/// Uses hierarchical completion with smart sorting:
/// - Exact matches first
/// - Prefix matches next
/// - Fuzzy matches ranked by Levenshtein distance
/// - Hierarchy-aware (shows `lang:` roots, filters children)
#[cfg(feature = "dynamic-completions")]
#[must_use]
pub fn complete_tags(current: &std::ffi::OsStr) -> Vec<clap_complete::engine::CompletionCandidate> {
    use clap_complete::engine::CompletionCandidate;
    use traits::DynamicCompleter;

    completers::HierarchicalTagCompleter
        .complete(current)
        .into_iter()
        .map(|c| {
            let mut candidate = CompletionCandidate::new(c.value);
            if let Some(help) = c.help {
                candidate = candidate.help(Some(help.into()));
            }
            candidate
        })
        .collect()
}

/// Complete virtual tags for `-v/--virtual-tag` argument
///
/// This is the entry point for `ArgValueCompleter`. It returns candidates
/// for virtual tags (modified:, size:, etc.) based on what the user has typed.
#[cfg(feature = "dynamic-completions")]
#[must_use]
pub fn complete_vtags(
    current: &std::ffi::OsStr,
) -> Vec<clap_complete::engine::CompletionCandidate> {
    use clap_complete::engine::CompletionCandidate;

    let current_str = current.to_string_lossy();

    complete_vtag(&current_str)
        .into_iter()
        .map(|c| {
            let mut candidate = CompletionCandidate::new(c.value);
            if let Some(help) = c.help {
                candidate = candidate.help(Some(help.into()));
            }
            candidate
        })
        .collect()
}

/// Complete filter names for `-F/--filter` argument
#[cfg(feature = "dynamic-completions")]
#[must_use]
pub fn complete_filters(
    current: &std::ffi::OsStr,
) -> Vec<clap_complete::engine::CompletionCandidate> {
    use crate::filters::{FilterManager, default_filter_path};
    use clap_complete::engine::CompletionCandidate;

    let current_str = current.to_string_lossy();
    let current_lower = current_str.to_lowercase();

    if let Ok(path) = default_filter_path() {
        let manager = FilterManager::new(path);
        if let Ok(filters) = manager.list() {
            return filters
                .into_iter()
                .filter(|f| f.name.to_lowercase().starts_with(&current_lower))
                .take(50)
                .map(|f| {
                    let mut candidate = CompletionCandidate::new(f.name);
                    if !f.description.is_empty() {
                        candidate = candidate.help(Some(f.description.into()));
                    }
                    candidate
                })
                .collect();
        }
    }

    Vec::new()
}

/// Complete database names for `--db` argument
#[cfg(feature = "dynamic-completions")]
#[must_use]
pub fn complete_databases(
    current: &std::ffi::OsStr,
) -> Vec<clap_complete::engine::CompletionCandidate> {
    use crate::config::TagrConfig;
    use clap_complete::engine::CompletionCandidate;

    let current_str = current.to_string_lossy();
    let current_lower = current_str.to_lowercase();

    if let Ok(config) = TagrConfig::load() {
        let default_db = config.get_default_database().map(ToOwned::to_owned);
        return config
            .list_databases()
            .into_iter()
            .filter(|name| name.to_lowercase().starts_with(&current_lower))
            .map(|name| {
                let mut candidate = CompletionCandidate::new(name);
                if let Some(ref def) = default_db
                    && name == def
                {
                    candidate = candidate.help(Some("default".into()));
                }
                candidate
            })
            .collect();
    }

    Vec::new()
}

/// Complete aliases
#[cfg(feature = "dynamic-completions")]
#[must_use]
pub fn complete_aliases(
    current: &std::ffi::OsStr,
) -> Vec<clap_complete::engine::CompletionCandidate> {
    use crate::schema::load_default_schema;
    use clap_complete::engine::CompletionCandidate;

    let current_str = current.to_string_lossy();
    let current_lower = current_str.to_lowercase();

    if let Ok(schema) = load_default_schema() {
        return schema
            .list_aliases()
            .into_iter()
            .filter(|(alias, _)| alias.to_lowercase().starts_with(&current_lower))
            .take(50)
            .map(|(alias, target)| {
                CompletionCandidate::new(alias).help(Some(format!("-> {target}").into()))
            })
            .collect();
    }

    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vtag_types_not_empty() {
        let types = vtag_types();
        assert!(!types.is_empty());
    }

    #[test]
    fn test_config_keys_not_empty() {
        let keys = config_keys();
        assert!(!keys.is_empty());
    }
}
