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
/// Call this at the start of main() before argument parsing when
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
/// This is the entry point for ArgValueCompleter. It returns candidates
/// for database tags based on what the user has typed.
#[cfg(feature = "dynamic-completions")]
pub fn complete_tags(current: &std::ffi::OsStr) -> Vec<clap_complete::engine::CompletionCandidate> {
    use clap_complete::engine::CompletionCandidate;

    let current_str = current.to_string_lossy();
    let current_lower = current_str.to_lowercase();

    let tags = cache::load_cached_tags();

    tags.into_iter()
        .filter(|tag| {
            let tag_lower = tag.to_lowercase();
            tag_lower.starts_with(&current_lower) || tag_lower.contains(&current_lower)
        })
        .take(50)
        .map(CompletionCandidate::new)
        .collect()
}

/// Complete virtual tags for `-v/--virtual-tag` argument
///
/// This is the entry point for ArgValueCompleter. It returns candidates
/// for virtual tags (modified:, size:, etc.) based on what the user has typed.
#[cfg(feature = "dynamic-completions")]
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
pub fn complete_filters(
    current: &std::ffi::OsStr,
) -> Vec<clap_complete::engine::CompletionCandidate> {
    use clap_complete::engine::CompletionCandidate;

    let current_str = current.to_string_lossy();
    let current_lower = current_str.to_lowercase();

    let filters = cache::load_cached_filters();

    filters
        .into_iter()
        .filter(|(name, _)| name.to_lowercase().starts_with(&current_lower))
        .take(50)
        .map(|(name, desc)| {
            let mut candidate = CompletionCandidate::new(name);
            if let Some(d) = desc {
                candidate = candidate.help(Some(d.into()));
            }
            candidate
        })
        .collect()
}

/// Complete database names for `--db` argument
#[cfg(feature = "dynamic-completions")]
pub fn complete_databases(
    current: &std::ffi::OsStr,
) -> Vec<clap_complete::engine::CompletionCandidate> {
    use clap_complete::engine::CompletionCandidate;

    let current_str = current.to_string_lossy();
    let current_lower = current_str.to_lowercase();

    let databases = cache::load_cached_databases();

    databases
        .into_iter()
        .filter(|(name, _)| name.to_lowercase().starts_with(&current_lower))
        .map(|(name, is_default)| {
            let mut candidate = CompletionCandidate::new(&name);
            if is_default {
                candidate = candidate.help(Some("default".into()));
            }
            candidate
        })
        .collect()
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
