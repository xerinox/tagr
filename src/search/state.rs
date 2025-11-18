//! Browse state and builder pattern
//!
//! Provides a clean, stateful API for browse operations:
//! ```no_run
//! use tagr::search::{BrowseState, BrowseVariant};
//! # use tagr::db::Database;
//! # use tagr::config::PathFormat;
//! # fn example(db: &Database) -> Result<(), Box<dyn std::error::Error>> {
//! let mut browse = BrowseState::builder()
//!     .db(db)
//!     .path_format(PathFormat::Relative)
//!     .build()?;
//!
//! let result = browse.run(BrowseVariant::Standard)?;
//! # Ok(())
//! # }
//! ```

use super::error::SearchError;
use crate::cli::{PreviewOverrides, SearchParams};
use crate::config::PathFormat;
use crate::db::Database;
use crate::keybinds::KeybindConfig;
use std::path::PathBuf;

/// Result of an interactive browse session
#[derive(Debug)]
pub struct BrowseResult {
    pub selected_tags: Vec<String>,
    pub selected_files: Vec<PathBuf>,
}

/// Different modes of browse operation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowseVariant {
    /// Standard browse: select tags → select files
    Standard,
    /// Advanced browse with AND/OR logic
    Advanced,
    /// Browse with post-selection action menu
    WithActions,
    /// Browse with real-time keybind support
    WithRealtimeKeybinds,
}

/// Stateful browse session
///
/// Encapsulates all configuration and state needed for an interactive
/// browse session. Use `BrowseState::builder()` to construct.
pub struct BrowseState<'a> {
    db: &'a Database,
    search_params: Option<SearchParams>,
    preview_overrides: Option<PreviewOverrides>,
    path_format: PathFormat,
    keybind_config: KeybindConfig,
    quiet: bool,
}

impl<'a> BrowseState<'a> {
    /// Create a new builder for constructing a `BrowseState`
    #[must_use]
    pub fn builder() -> BrowseStateBuilder<'a> {
        BrowseStateBuilder::new()
    }

    /// Run the browse session with the specified variant
    ///
    /// # Arguments
    /// * `variant` - The type of browse operation to perform
    ///
    /// # Returns
    /// * `Ok(Some(BrowseResult))` - User made selections and confirmed
    /// * `Ok(None)` - User cancelled the operation
    /// * `Err(SearchError)` - An error occurred during the operation
    ///
    /// # Errors
    ///
    /// Returns `SearchError` if database operations fail, UI interactions fail,
    /// or if skim selection is interrupted.
    pub fn run(&mut self, variant: BrowseVariant) -> Result<Option<BrowseResult>, SearchError> {
        match variant {
            BrowseVariant::Standard => self.run_standard(),
            BrowseVariant::Advanced => self.run_advanced(),
            BrowseVariant::WithActions => self.run_with_actions(),
            BrowseVariant::WithRealtimeKeybinds => self.run_with_realtime_keybinds(),
        }
    }

    /// Get database reference
    #[must_use]
    pub const fn db(&self) -> &Database {
        self.db
    }

    /// Get path format
    #[must_use]
    pub const fn path_format(&self) -> PathFormat {
        self.path_format
    }

    /// Get keybind config reference
    #[must_use]
    pub const fn keybind_config(&self) -> &KeybindConfig {
        &self.keybind_config
    }

    /// Get search params reference
    #[must_use]
    pub const fn search_params(&self) -> Option<&SearchParams> {
        self.search_params.as_ref()
    }

    /// Update search parameters
    pub fn set_search_params(&mut self, params: Option<SearchParams>) {
        self.search_params = params;
    }

    /// Update preview overrides
    pub fn set_preview_overrides(&mut self, overrides: Option<PreviewOverrides>) {
        self.preview_overrides = overrides;
    }

    /// Set quiet mode
    pub const fn set_quiet(&mut self, quiet: bool) {
        self.quiet = quiet;
    }

    // Private implementation methods
    fn run_standard(&self) -> Result<Option<BrowseResult>, SearchError> {
        crate::search::browse::browse_with_params(
            self.db,
            self.search_params.clone(),
            self.preview_overrides.clone(),
            self.path_format,
            Some(&self.keybind_config),
        )
    }

    fn run_advanced(&self) -> Result<Option<BrowseResult>, SearchError> {
        crate::search::browse::browse_advanced(self.db, self.path_format, Some(&self.keybind_config))
    }

    fn run_with_actions(&self) -> Result<Option<BrowseResult>, SearchError> {
        crate::search::browse::browse_with_actions(
            self.db,
            self.search_params.clone(),
            self.preview_overrides.clone(),
            self.path_format,
            Some(&self.keybind_config),
        )
    }

    fn run_with_realtime_keybinds(&self) -> Result<Option<BrowseResult>, SearchError> {
        crate::search::browse::browse_with_realtime_keybinds(
            self.db,
            self.search_params.clone(),
            self.preview_overrides.clone(),
            self.path_format,
            &self.keybind_config,
        )
    }
}

/// Builder for `BrowseState`
///
/// Provides a fluent API for configuring browse sessions:
/// ```no_run
/// # use tagr::search::BrowseState;
/// # use tagr::db::Database;
/// # use tagr::config::PathFormat;
/// # fn example(db: &Database) -> Result<(), Box<dyn std::error::Error>> {
/// let browse = BrowseState::builder()
///     .db(db)
///     .path_format(PathFormat::Absolute)
///     .quiet(true)
///     .build()?;
/// # Ok(())
/// # }
/// ```
pub struct BrowseStateBuilder<'a> {
    db: Option<&'a Database>,
    search_params: Option<SearchParams>,
    preview_overrides: Option<PreviewOverrides>,
    path_format: PathFormat,
    keybind_config: Option<KeybindConfig>,
    quiet: bool,
}

impl<'a> BrowseStateBuilder<'a> {
    /// Create a new builder with default settings
    #[must_use]
    pub const fn new() -> Self {
        Self {
            db: None,
            search_params: None,
            preview_overrides: None,
            path_format: PathFormat::Absolute,
            keybind_config: None,
            quiet: false,
        }
    }

    /// Set the database reference (required)
    #[must_use]
    pub const fn db(mut self, db: &'a Database) -> Self {
        self.db = Some(db);
        self
    }

    /// Set search parameters for pre-filtering
    #[must_use]
    pub fn search_params(mut self, params: SearchParams) -> Self {
        self.search_params = Some(params);
        self
    }

    /// Set preview configuration overrides
    #[must_use]
    pub fn preview_overrides(mut self, overrides: PreviewOverrides) -> Self {
        self.preview_overrides = Some(overrides);
        self
    }

    /// Set path display format
    #[must_use]
    pub const fn path_format(mut self, format: PathFormat) -> Self {
        self.path_format = format;
        self
    }

    /// Set keybind configuration
    #[must_use]
    pub fn keybind_config(mut self, config: KeybindConfig) -> Self {
        self.keybind_config = Some(config);
        self
    }

    /// Enable quiet mode (minimal output)
    #[must_use]
    pub const fn quiet(mut self, quiet: bool) -> Self {
        self.quiet = quiet;
        self
    }

    /// Build the `BrowseState`
    ///
    /// # Errors
    ///
    /// Returns `SearchError::BuildError` if:
    /// - Database reference is not provided
    /// - Keybind configuration cannot be loaded
    pub fn build(self) -> Result<BrowseState<'a>, SearchError> {
        let db = self.db.ok_or_else(|| {
            SearchError::BuildError("Database reference is required".to_string())
        })?;

        let keybind_config = if let Some(config) = self.keybind_config {
            config
        } else {
            KeybindConfig::load_or_default().map_err(|e| {
                SearchError::BuildError(format!("Failed to load keybind config: {e}"))
            })?
        };

        Ok(BrowseState {
            db,
            search_params: self.search_params,
            preview_overrides: self.preview_overrides,
            path_format: self.path_format,
            keybind_config,
            quiet: self.quiet,
        })
    }
}

impl Default for BrowseStateBuilder<'_> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestDb;

    #[test]
    fn test_browse_state_builder_requires_db() {
        let result = BrowseState::builder().build();
        assert!(result.is_err());
        assert!(matches!(result, Err(SearchError::BuildError(_))));
    }

    #[test]
    fn test_browse_state_builder_with_db() {
        let test_db = TestDb::new("test_browse_state");
        let db = test_db.db();

        let result = BrowseState::builder().db(db).build();
        assert!(result.is_ok());
    }

    #[test]
    fn test_browse_state_builder_fluent_api() {
        let test_db = TestDb::new("test_browse_state_fluent");
        let db = test_db.db();

        let state = BrowseState::builder()
            .db(db)
            .path_format(PathFormat::Relative)
            .quiet(true)
            .build()
            .unwrap();

        assert_eq!(state.path_format(), PathFormat::Relative);
        assert!(state.quiet);
    }

    #[test]
    fn test_browse_variant_equality() {
        assert_eq!(BrowseVariant::Standard, BrowseVariant::Standard);
        assert_ne!(BrowseVariant::Standard, BrowseVariant::Advanced);
    }

    #[test]
    fn test_browse_result_creation() {
        let result = BrowseResult {
            selected_tags: vec!["rust".to_string(), "programming".to_string()],
            selected_files: vec![PathBuf::from("file1.txt"), PathBuf::from("file2.txt")],
        };

        assert_eq!(result.selected_tags.len(), 2);
        assert_eq!(result.selected_files.len(), 2);
    }
}
