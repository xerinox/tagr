//! User prompt utilities for keybind actions.

use std::io::{self, Write};

/// Prompt the user for input.
///
/// # Errors
///
/// Returns error if reading from stdin fails.
pub fn prompt_for_input(prompt: &str) -> Result<String, PromptError> {
    print!("{}", prompt);
    io::stdout().flush()?;
    
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    
    Ok(input.trim().to_string())
}

/// Prompt the user for confirmation (yes/no).
///
/// # Errors
///
/// Returns error if reading from stdin fails.
pub fn prompt_for_confirmation(prompt: &str) -> Result<bool, PromptError> {
    let response = prompt_for_input(&format!("{} (y/N): ", prompt))?;
    Ok(matches!(response.to_lowercase().as_str(), "y" | "yes"))
}

/// Errors that can occur during prompting.
#[derive(Debug, thiserror::Error)]
pub enum PromptError {
    /// IO error during prompt
    #[error("Prompt IO error: {0}")]
    Io(#[from] io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_error_conversion() {
        let io_err = io::Error::new(io::ErrorKind::Other, "test");
        let prompt_err: PromptError = io_err.into();
        assert!(matches!(prompt_err, PromptError::Io(_)));
    }
}
