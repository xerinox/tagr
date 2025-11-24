//! User input abstraction layer
//!
//! This module provides a backend-agnostic interface for user input operations,
//! allowing different implementations for CLI (dialoguer) and TUI (ratatui).

use std::io;

/// Trait for user input operations
///
/// This trait abstracts away the specific input mechanism, allowing
/// different implementations for CLI (dialoguer) and TUI (ratatui).
///
/// # Examples
///
/// ```no_run
/// use tagr::ui::input::{UserInput, DialoguerInput};
///
/// let input = DialoguerInput::new();
/// 
/// // Prompt for text
/// if let Some(name) = input.prompt_text("Enter name:", None, false).unwrap() {
///     println!("Hello, {}!", name);
/// }
/// 
/// // Prompt for confirmation
/// if let Some(true) = input.prompt_confirm("Delete files?", false).unwrap() {
///     println!("Deleting...");
/// }
/// ```
pub trait UserInput: Send + Sync {
    /// Prompt user for text input
    ///
    /// # Arguments
    ///
    /// * `prompt` - The prompt message to display
    /// * `default` - Optional default value
    /// * `allow_empty` - Whether empty input is allowed
    ///
    /// # Returns
    ///
    /// * `Ok(Some(String))` - User entered text
    /// * `Ok(None)` - User cancelled (ESC)
    /// * `Err(_)` - Input operation failed
    fn prompt_text(
        &self,
        prompt: &str,
        default: Option<&str>,
        allow_empty: bool,
    ) -> Result<Option<String>>;

    /// Prompt user for confirmation (yes/no)
    ///
    /// # Arguments
    ///
    /// * `prompt` - The prompt message to display
    /// * `default` - Default selection (true = yes, false = no)
    ///
    /// # Returns
    ///
    /// * `Ok(Some(bool))` - User confirmed (true) or denied (false)
    /// * `Ok(None)` - User cancelled (ESC)
    /// * `Err(_)` - Input operation failed
    fn prompt_confirm(&self, prompt: &str, default: bool) -> Result<Option<bool>>;

    /// Prompt user to select from a list
    ///
    /// # Arguments
    ///
    /// * `prompt` - The prompt message to display
    /// * `items` - List of items to choose from
    /// * `default` - Optional default selection index
    ///
    /// # Returns
    ///
    /// * `Ok(Some(usize))` - Index of selected item
    /// * `Ok(None)` - User cancelled (ESC)
    /// * `Err(_)` - Input operation failed
    fn prompt_select(
        &self,
        prompt: &str,
        items: &[String],
        default: Option<usize>,
    ) -> Result<Option<usize>>;
}

/// Result type for user input operations
pub type Result<T> = std::result::Result<T, InputError>;

/// Errors that can occur during user input
#[derive(Debug, thiserror::Error)]
pub enum InputError {
    /// IO error during input
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    /// Input cancelled by user
    #[error("Input cancelled by user")]
    Cancelled,

    /// Invalid input provided
    #[error("Invalid input: {0}")]
    Invalid(String),
}

/// CLI-based user input using dialoguer
///
/// This implementation uses the `dialoguer` crate to provide interactive
/// prompts in a traditional command-line interface.
///
/// # Examples
///
/// ```no_run
/// use tagr::ui::input::{UserInput, DialoguerInput};
///
/// let input = DialoguerInput::new();
/// let name = input.prompt_text("Your name:", Some("Anonymous"), false).unwrap();
/// ```
pub struct DialoguerInput {
    theme: dialoguer::theme::ColorfulTheme,
}

impl DialoguerInput {
    /// Create a new dialoguer-based input handler
    #[must_use]
    pub fn new() -> Self {
        Self {
            theme: dialoguer::theme::ColorfulTheme::default(),
        }
    }
}

impl Default for DialoguerInput {
    fn default() -> Self {
        Self::new()
    }
}

impl UserInput for DialoguerInput {
    fn prompt_text(
        &self,
        prompt: &str,
        default: Option<&str>,
        allow_empty: bool,
    ) -> Result<Option<String>> {
        use dialoguer::Input;

        let mut input = Input::<String>::with_theme(&self.theme)
            .with_prompt(prompt)
            .allow_empty(allow_empty);

        if let Some(def) = default {
            input = input.default(def.to_string());
        }

        input
            .interact_text()
            .map(Some)
            .map_err(|e| InputError::Io(io::Error::other(e)))
    }

    fn prompt_confirm(&self, prompt: &str, default: bool) -> Result<Option<bool>> {
        use dialoguer::Confirm;

        Confirm::with_theme(&self.theme)
            .with_prompt(prompt)
            .default(default)
            .interact()
            .map(Some)
            .map_err(|e| InputError::Io(io::Error::other(e)))
    }

    fn prompt_select(
        &self,
        prompt: &str,
        items: &[String],
        default: Option<usize>,
    ) -> Result<Option<usize>> {
        use dialoguer::Select;

        let mut select = Select::with_theme(&self.theme)
            .with_prompt(prompt)
            .items(items);

        if let Some(def) = default {
            select = select.default(def);
        }

        select
            .interact()
            .map(Some)
            .map_err(|e| InputError::Io(io::Error::other(e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_error_from_io() {
        let io_err = io::Error::new(io::ErrorKind::Other, "test error");
        let input_err: InputError = io_err.into();
        assert!(matches!(input_err, InputError::Io(_)));
    }

    #[test]
    fn test_dialoguer_input_creation() {
        let _input = DialoguerInput::new();
        let _input2 = DialoguerInput::default();
    }
}
