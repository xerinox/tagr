//! User prompt utilities for keybind actions.

use dialoguer::{theme::ColorfulTheme, Confirm, Input};

/// Prompt the user for input.
///
/// # Errors
///
/// Returns error if reading from stdin fails.
pub fn prompt_for_input(prompt: &str) -> Result<String, PromptError> {
    Input::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .allow_empty(true)
        .interact_text()
        .map_err(|e| PromptError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))
}

/// Prompt the user for confirmation (yes/no).
///
/// # Errors
///
/// Returns error if reading from stdin fails.
pub fn prompt_for_confirmation(prompt: &str) -> Result<bool, PromptError> {
    Confirm::new()
        .with_prompt(prompt)
        .default(false)
        .interact()
        .map_err(|e| PromptError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))
}

/// Errors that can occur during prompting.
#[derive(Debug, thiserror::Error)]
pub enum PromptError {
    /// IO error during prompt
    #[error("Prompt IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::Other, "test");
        let prompt_err: PromptError = io_err.into();
        assert!(matches!(prompt_err, PromptError::Io(_)));
    }
}
