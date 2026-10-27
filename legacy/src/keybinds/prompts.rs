//! User prompt utilities for keybind actions.
//!
//! This module provides convenience wrappers around the `UserInput` trait
//! for backward compatibility and ease of use.

use crate::ui::input::{DialoguerInput, InputError, UserInput};

/// Prompt the user for input using the default CLI input handler.
///
/// # Errors
///
/// Returns error if reading from stdin fails or user cancels.
pub fn prompt_for_input(prompt: &str) -> Result<String, PromptError> {
    let input = DialoguerInput::new();
    input
        .prompt_text(prompt, None, true)
        .map_err(PromptError::from)?
        .ok_or(PromptError::Cancelled)
}

/// Prompt the user for confirmation (yes/no) using the default CLI input handler.
///
/// # Errors
///
/// Returns error if reading from stdin fails or user cancels.
pub fn prompt_for_confirmation(prompt: &str) -> Result<bool, PromptError> {
    let input = DialoguerInput::new();
    input
        .prompt_confirm(prompt, false)
        .map_err(PromptError::from)?
        .ok_or(PromptError::Cancelled)
}

/// Errors that can occur during prompting.
#[derive(Debug, thiserror::Error)]
pub enum PromptError {
    /// IO error during prompt
    #[error("Prompt IO error: {0}")]
    Io(#[from] std::io::Error),

    /// User cancelled the prompt
    #[error("Prompt cancelled by user")]
    Cancelled,

    /// Invalid input provided
    #[error("Invalid input: {0}")]
    Invalid(String),
}

impl From<InputError> for PromptError {
    fn from(err: InputError) -> Self {
        match err {
            InputError::Io(e) => Self::Io(e),
            InputError::Cancelled => Self::Cancelled,
            InputError::Invalid(s) => Self::Invalid(s),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_error_conversion() {
        let io_err = std::io::Error::other("test");
        let prompt_err: PromptError = io_err.into();
        assert!(matches!(prompt_err, PromptError::Io(_)));
    }

    #[test]
    fn test_prompt_error_from_input_error() {
        let input_err = InputError::Cancelled;
        let prompt_err: PromptError = input_err.into();
        assert!(matches!(prompt_err, PromptError::Cancelled));
    }
}
