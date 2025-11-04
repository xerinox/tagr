//! Configuration module for tagr
//! 
//! Manages application configuration including database paths.
//! Configuration is stored in the user's config directory.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use config::{Config, ConfigError, File, FileFormat};
use dialoguer::{Input, theme::ColorfulTheme};
use serde::{Deserialize, Serialize};

/// Application configuration structure
#[derive(Debug, Serialize, Deserialize, Clone)]
#[derive(Default)]
pub struct TagrConfig {
    /// Map of database names to their filesystem paths
    #[serde(default)]
    pub databases: HashMap<String, PathBuf>,
    
    /// The default database to use when none is specified
    #[serde(default)]
    pub default_database: Option<String>,
    
    /// Suppress informational output by default
    #[serde(default)]
    pub quiet: bool,
}


impl TagrConfig {
    /// Get the path to the config file
    /// 
    /// # Errors
    /// 
    /// Returns `ConfigError` if the system config directory cannot be determined.
    pub fn config_path() -> Result<PathBuf, ConfigError> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| ConfigError::Message("Could not determine config directory".to_string()))?;
        
        let tagr_config_dir = config_dir.join("tagr");
        Ok(tagr_config_dir.join("config.toml"))
    }

    /// Load configuration from file, creating default if it doesn't exist
    /// 
    /// # Errors
    /// 
    /// Returns `ConfigError` if the config file cannot be read, parsed, or created.
    pub fn load() -> Result<Self, ConfigError> {
        let config_path = Self::config_path()?;
        
        if !config_path.exists() {
            let default_config = Self::default();
            default_config.save()?;
            return Ok(default_config);
        }
        
        let settings = Config::builder()
            .add_source(File::from(config_path).format(FileFormat::Toml))
            .build()?;
        
        settings.try_deserialize()
    }

    /// Save configuration to file
    /// 
    /// # Errors
    /// 
    /// Returns `ConfigError` if the config directory cannot be created, the configuration
    /// cannot be serialized to TOML, or the file cannot be written.
    pub fn save(&self) -> Result<(), ConfigError> {
        let config_path = Self::config_path()?;
        
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| ConfigError::Message(format!("Failed to create config directory: {e}")))?;
        }
        
        let toml_string = toml::to_string_pretty(self)
            .map_err(|e| ConfigError::Message(format!("Failed to serialize config: {e}")))?;
        
        fs::write(&config_path, toml_string)
            .map_err(|e| ConfigError::Message(format!("Failed to write config file: {e}")))?;
        
        Ok(())
    }

    /// Add a database to the configuration
    /// 
    /// # Errors
    /// 
    /// Returns `ConfigError` if saving the configuration fails.
    pub fn add_database(&mut self, name: String, path: PathBuf) -> Result<(), ConfigError> {
        self.databases.insert(name, path);
        self.save()
    }

    /// Remove a database from the configuration
    /// 
    /// # Errors
    /// 
    /// Returns `ConfigError` if saving the configuration fails.
    pub fn remove_database(&mut self, name: &str) -> Result<Option<PathBuf>, ConfigError> {
        let removed = self.databases.remove(name);
        self.save()?;
        Ok(removed)
    }

    /// Get a database path by name
    #[must_use] 
    pub fn get_database(&self, name: &str) -> Option<&PathBuf> {
        self.databases.get(name)
    }

    /// List all database names
    #[must_use] 
    pub fn list_databases(&self) -> Vec<&String> {
        self.databases.keys().collect()
    }

    /// Set the default database
    /// 
    /// # Errors
    /// 
    /// Returns `ConfigError` if the database name doesn't exist in the configuration
    /// or if saving the configuration fails.
    pub fn set_default_database(&mut self, name: String) -> Result<(), ConfigError> {
        if !self.databases.contains_key(&name) {
            return Err(ConfigError::Message(
                format!("Database '{name}' does not exist in configuration")
            ));
        }
        self.default_database = Some(name);
        self.save()
    }

    /// Get the default database name
    #[must_use] 
    pub const fn get_default_database(&self) -> Option<&String> {
        self.default_database.as_ref()
    }

    /// Interactive first-time setup - prompts for database name and location
    /// 
    /// # Errors
    /// 
    /// Returns `ConfigError` if user input cannot be read or if saving the configuration fails.
    pub fn first_time_setup() -> Result<Self, ConfigError> {
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
        
        let mut config = Self::default();
        config.databases.insert(db_name.clone(), db_path);
        config.default_database = Some(db_name);
        config.quiet = false;
        
        config.save()?;
        
        println!("\nConfiguration saved successfully!");
        Ok(config)
    }

    /// Load configuration, running first-time setup if config doesn't exist
    /// 
    /// # Errors
    /// 
    /// Returns `ConfigError` if loading or creating the configuration fails.
    pub fn load_or_setup() -> Result<Self, ConfigError> {
        let config_path = Self::config_path()?;
        
        if config_path.exists() {
            Self::load()
        } else {
            Self::first_time_setup()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = TagrConfig::default();
        assert!(config.databases.is_empty());
        assert!(config.default_database.is_none());
    }

    #[test]
    fn test_add_database() {
        let mut config = TagrConfig::default();
        config.databases.insert("test_db".to_string(), PathBuf::from("/tmp/test_db"));
        
        assert_eq!(config.databases.len(), 1);
        assert_eq!(config.get_database("test_db"), Some(&PathBuf::from("/tmp/test_db")));
    }

    #[test]
    fn test_create_and_add_database() {
        let db_path = PathBuf::from("/tmp/new_test_db");
        
        let mut config = TagrConfig::default();
        
        config.databases.insert("new_test_db".to_string(), db_path.clone());
        
        assert_eq!(config.databases.len(), 1);
        assert_eq!(config.get_database("new_test_db"), Some(&db_path));
        assert!(config.databases.contains_key("new_test_db"));
    }

    #[test]
    fn test_remove_database_from_config() {
        let mut config = TagrConfig::default();
        let db_path = PathBuf::from("/tmp/test_remove_db");
        
        config.databases.insert("remove_me".to_string(), db_path.clone());
        assert_eq!(config.databases.len(), 1);
        
        let removed = config.databases.remove("remove_me");
        assert_eq!(removed, Some(db_path));
        assert_eq!(config.databases.len(), 0);
        assert!(!config.databases.contains_key("remove_me"));
    }

    #[test]
    fn test_remove_nonexistent_database() {
        let mut config = TagrConfig::default();
        
        let removed = config.databases.remove("nonexistent");
        assert_eq!(removed, None);
    }

    #[test]
    fn test_create_multiple_databases() {
        let mut config = TagrConfig::default();
        
        for i in 1..=5 {
            let db_name = format!("db_{}", i);
            let db_path = PathBuf::from(format!("/tmp/db_{}", i));
            config.databases.insert(db_name, db_path);
        }
        
        assert_eq!(config.databases.len(), 5);
        assert!(config.get_database("db_1").is_some());
        assert!(config.get_database("db_5").is_some());
    }

    #[test]
    fn test_remove_one_of_multiple_databases() {
        let mut config = TagrConfig::default();
        
        config.databases.insert("db1".to_string(), PathBuf::from("/tmp/db1"));
        config.databases.insert("db2".to_string(), PathBuf::from("/tmp/db2"));
        config.databases.insert("db3".to_string(), PathBuf::from("/tmp/db3"));
        
        assert_eq!(config.databases.len(), 3);
        
        let removed = config.databases.remove("db2");
        assert!(removed.is_some());
        assert_eq!(config.databases.len(), 2);
        assert!(config.get_database("db1").is_some());
        assert!(config.get_database("db2").is_none());
        assert!(config.get_database("db3").is_some());
    }

    #[test]
    fn test_list_databases() {
        let mut config = TagrConfig::default();
        
        config.databases.insert("alpha".to_string(), PathBuf::from("/tmp/alpha"));
        config.databases.insert("beta".to_string(), PathBuf::from("/tmp/beta"));
        config.databases.insert("gamma".to_string(), PathBuf::from("/tmp/gamma"));
        
        let db_list = config.list_databases();
        assert_eq!(db_list.len(), 3);
        assert!(db_list.contains(&&"alpha".to_string()));
        assert!(db_list.contains(&&"beta".to_string()));
        assert!(db_list.contains(&&"gamma".to_string()));
    }

    #[test]
    fn test_set_default_database() {
        let mut config = TagrConfig::default();
        
        config.databases.insert("db1".to_string(), PathBuf::from("/tmp/db1"));
        config.databases.insert("db2".to_string(), PathBuf::from("/tmp/db2"));
        
        config.default_database = Some("db1".to_string());
        
        assert_eq!(config.get_default_database(), Some(&"db1".to_string()));
    }

    #[test]
    fn test_remove_default_database() {
        let mut config = TagrConfig::default();
        
        config.databases.insert("default_db".to_string(), PathBuf::from("/tmp/default_db"));
        config.default_database = Some("default_db".to_string());
        
        config.databases.remove("default_db");
        
        assert!(config.get_database("default_db").is_none());
        assert_eq!(config.get_default_database(), Some(&"default_db".to_string()));
    }
}
