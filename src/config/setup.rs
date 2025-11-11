//! Interactive setup wizard for first-time configuration
//!
//! This module handles the interactive prompts for creating an initial
//! configuration when tagr is run for the first time.

use super::{PathFormat, TagrConfig};
use config::ConfigError;
use dialoguer::{Input, theme::ColorfulTheme};
use std::path::PathBuf;

/// Interactive first-time setup - prompts for database name and location
///
/// Guides the user through creating their first database configuration:
/// 1. Prompts for a database name (default: "default")
/// 2. Prompts for database location (default: system data directory)
/// 3. Creates and saves the configuration
///
/// # Errors
///
/// Returns `ConfigError` if:
/// - The system data directory cannot be determined
/// - User input cannot be read
/// - The configuration cannot be saved
///
/// # Examples
/// ```ignore
/// use tagr::config::first_time_setup;
///
/// let config = first_time_setup()?;
/// println!("Configuration created with {} database(s)", config.databases.len());
/// ```
pub fn first_time_setup() -> Result<TagrConfig, ConfigError> {
    println!("Welcome to tagr! Let's set up your first database.\n");

    let default_data_dir = dirs::data_local_dir()
        .ok_or_else(|| ConfigError::Message("Could not determine data directory".to_string()))?
        .join("tagr");

    let db_name: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Database name")
        .default("default".to_string())
        .interact_text()
        .map_err(|e| ConfigError::Message(format!("Failed to read input: {e}")))?;

    let default_path = default_data_dir.join(&db_name);
    let db_path_str: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Database location")
        .default(default_path.to_string_lossy().to_string())
        .interact_text()
        .map_err(|e| ConfigError::Message(format!("Failed to read input: {e}")))?;

    let db_path = PathBuf::from(db_path_str);

    let mut config = TagrConfig::default();
    config.databases.insert(db_name.clone(), db_path);
    config.default_database = Some(db_name);
    config.quiet = false;
    config.path_format = PathFormat::Absolute;

    config.save()?;

    println!("\nConfiguration saved successfully!");
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_setup_module_compiles() {
        // Ensures the module compiles and the function signature is correct
        let _: fn() -> Result<TagrConfig, ConfigError> = first_time_setup;
    }
}
