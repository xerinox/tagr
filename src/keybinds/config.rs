//! Configuration for keybinds.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for keybinds and related settings.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KeybindConfig {
    /// Keybind mappings
    #[serde(default = "default_keybinds")]
    pub keybinds: HashMap<String, KeybindDef>,
    
    /// Editor configuration
    #[serde(default)]
    pub editor: EditorConfig,
    
    /// Action-specific settings
    #[serde(default)]
    pub actions: ActionSettings,
    
    /// Display settings
    #[serde(default)]
    pub display: DisplaySettings,
    
    /// History settings
    #[serde(default)]
    pub history: HistorySettings,
}

/// Keybind definition - can be single key, multiple keys, or disabled.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum KeybindDef {
    /// Single keybind
    Single(String),
    /// Multiple alternative keybinds for the same action
    Multiple(Vec<String>),
}

/// Editor configuration for opening files.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EditorConfig {
    /// Editor command (e.g., "vim", "nvim", "code")
    #[serde(default = "default_editor")]
    pub command: String,
    /// Additional arguments to pass to the editor
    #[serde(default)]
    pub args: Vec<String>,
}

/// Settings for various actions.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ActionSettings {
    /// Custom clipboard command (optional)
    #[serde(default)]
    pub clipboard_command: Option<String>,
    /// Require confirmation for delete operations
    #[serde(default = "default_true")]
    pub confirm_delete: bool,
    /// Require confirmation for copy operations
    #[serde(default)]
    pub confirm_copy: bool,
}

/// Display-related settings.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DisplaySettings {
    /// Default tag display mode
    #[serde(default = "default_tag_display")]
    pub default_tag_display: String,
    /// Show keybind hints at bottom
    #[serde(default = "default_true")]
    pub show_hints: bool,
}

/// History-related settings.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HistorySettings {
    /// Maximum number of sessions to remember
    #[serde(default = "default_max_sessions")]
    pub max_sessions: usize,
    /// Custom history file path (optional)
    #[serde(default)]
    pub history_file: Option<String>,
}

impl Default for KeybindConfig {
    fn default() -> Self {
        Self {
            keybinds: default_keybinds(),
            editor: EditorConfig::default(),
            actions: ActionSettings::default(),
            display: DisplaySettings::default(),
            history: HistorySettings::default(),
        }
    }
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            command: default_editor(),
            args: vec![],
        }
    }
}

impl Default for ActionSettings {
    fn default() -> Self {
        Self {
            clipboard_command: None,
            confirm_delete: true,
            confirm_copy: false,
        }
    }
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            default_tag_display: default_tag_display(),
            show_hints: true,
        }
    }
}

impl Default for HistorySettings {
    fn default() -> Self {
        Self {
            max_sessions: default_max_sessions(),
            history_file: None,
        }
    }
}

fn default_keybinds() -> HashMap<String, KeybindDef> {
    let mut keybinds = HashMap::new();
    
    // Tag Management
    keybinds.insert("add_tag".to_string(), KeybindDef::Single("ctrl-t".to_string()));
    keybinds.insert("remove_tag".to_string(), KeybindDef::Single("ctrl-r".to_string()));
    keybinds.insert("edit_tags".to_string(), KeybindDef::Single("ctrl-e".to_string()));
    
    // File Operations
    keybinds.insert("open_default".to_string(), KeybindDef::Single("ctrl-o".to_string()));
    keybinds.insert("open_editor".to_string(), KeybindDef::Single("ctrl-v".to_string()));
    keybinds.insert("copy_path".to_string(), KeybindDef::Single("ctrl-y".to_string()));
    keybinds.insert("copy_files".to_string(), KeybindDef::Single("ctrl-p".to_string()));
    keybinds.insert("delete_from_db".to_string(), KeybindDef::Single("ctrl-d".to_string()));
    
    // View Options
    keybinds.insert("toggle_tag_display".to_string(), KeybindDef::Single("ctrl-i".to_string()));
    keybinds.insert("show_details".to_string(), KeybindDef::Single("ctrl-l".to_string()));
    keybinds.insert("filter_extension".to_string(), KeybindDef::Single("ctrl-f".to_string()));
    
    // Navigation
    keybinds.insert("select_all".to_string(), KeybindDef::Single("ctrl-a".to_string()));
    keybinds.insert("clear_selection".to_string(), KeybindDef::Single("ctrl-x".to_string()));
    
    // Search & Filter
    keybinds.insert("quick_search".to_string(), KeybindDef::Single("ctrl-s".to_string()));
    keybinds.insert("goto_file".to_string(), KeybindDef::Single("ctrl-g".to_string()));
    
    // History & Sessions
    keybinds.insert("show_history".to_string(), KeybindDef::Single("ctrl-h".to_string()));
    keybinds.insert("bookmark_selection".to_string(), KeybindDef::Single("ctrl-b".to_string()));
    
    // System
    keybinds.insert("show_help".to_string(), KeybindDef::Multiple(vec!["f1".to_string()]));
    
    keybinds
}

fn default_editor() -> String {
    std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string())
}

fn default_true() -> bool {
    true
}

fn default_tag_display() -> String {
    "inline".to_string()
}

fn default_max_sessions() -> usize {
    50
}

impl KeybindConfig {
    /// Get the keybind(s) for a given action name.
    ///
    /// Returns an empty slice if the action is not configured.
    #[must_use]
    pub fn get(&self, action: &str) -> Vec<String> {
        self.keybinds.get(action).map_or_else(Vec::new, |def| {
            match def {
                KeybindDef::Single(key) => vec![key.clone()],
                KeybindDef::Multiple(keys) => keys.clone(),
            }
        })
    }

    /// Check if a keybind is disabled for an action.
    #[must_use]
    pub fn is_disabled(&self, action: &str) -> bool {
        self.keybinds.get(action).map_or(false, |def| {
            match def {
                KeybindDef::Single(key) => key == "none",
                KeybindDef::Multiple(keys) => keys.iter().all(|k| k == "none"),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_keybinds() {
        let config = KeybindConfig::default();
        assert_eq!(config.get("add_tag"), vec!["ctrl-t"]);
        assert_eq!(config.get("remove_tag"), vec!["ctrl-r"]);
    }

    #[test]
    fn test_keybind_def_parsing() {
        let toml = r#"
            [keybinds]
            add_tag = "ctrl-t"
            remove_tag = ["ctrl-r", "F2"]
        "#;
        
        let config: KeybindConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.get("add_tag"), vec!["ctrl-t"]);
        assert_eq!(config.get("remove_tag"), vec!["ctrl-r", "F2"]);
    }

    #[test]
    fn test_is_disabled() {
        let mut keybinds = HashMap::new();
        keybinds.insert("disabled".to_string(), KeybindDef::Single("none".to_string()));
        keybinds.insert("enabled".to_string(), KeybindDef::Single("ctrl-t".to_string()));
        
        let config = KeybindConfig {
            keybinds,
            ..Default::default()
        };
        
        assert!(config.is_disabled("disabled"));
        assert!(!config.is_disabled("enabled"));
    }

    #[test]
    fn test_editor_default() {
        let config = EditorConfig::default();
        assert!(!config.command.is_empty());
    }
}
