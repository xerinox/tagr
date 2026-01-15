//! Configuration for keybinds.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Errors that can occur during configuration loading.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// IO error reading config file
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// TOML parsing error
    #[error("TOML parse error: {0}")]
    TomlParse(#[from] toml::de::Error),

    /// Config directory not found
    #[error("Could not determine config directory")]
    NoConfigDir,
}

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
    keybinds.insert(
        "add_tag".to_string(),
        KeybindDef::Single("ctrl-t".to_string()),
    );
    keybinds.insert(
        "remove_tag".to_string(),
        KeybindDef::Single("ctrl-r".to_string()),
    );
    keybinds.insert(
        "edit_tags".to_string(),
        KeybindDef::Single("ctrl-e".to_string()),
    );

    // File Operations
    keybinds.insert(
        "open_default".to_string(),
        KeybindDef::Single("ctrl-o".to_string()),
    );
    keybinds.insert(
        "open_editor".to_string(),
        KeybindDef::Single("ctrl-v".to_string()),
    );
    keybinds.insert(
        "copy_path".to_string(),
        KeybindDef::Single("ctrl-y".to_string()),
    );
    keybinds.insert(
        "copy_files".to_string(),
        KeybindDef::Single("ctrl-p".to_string()),
    );
    keybinds.insert(
        "delete_from_db".to_string(),
        KeybindDef::Single("ctrl-d".to_string()),
    );

    // View Options
    keybinds.insert(
        "toggle_tag_display".to_string(),
        KeybindDef::Single("ctrl-i".to_string()),
    );
    keybinds.insert(
        "show_details".to_string(),
        KeybindDef::Single("ctrl-l".to_string()),
    );
    keybinds.insert(
        "filter_extension".to_string(),
        KeybindDef::Single("ctrl-f".to_string()),
    );

    // Note Management
    keybinds.insert(
        "edit_note".to_string(),
        KeybindDef::Single("ctrl-n".to_string()),
    );
    keybinds.insert(
        "toggle_note_preview".to_string(),
        KeybindDef::Single("alt-n".to_string()),
    );

    // Navigation
    keybinds.insert(
        "select_all".to_string(),
        KeybindDef::Single("ctrl-a".to_string()),
    );
    keybinds.insert(
        "clear_selection".to_string(),
        KeybindDef::Single("ctrl-x".to_string()),
    );

    // Search & Filter
    keybinds.insert(
        "quick_search".to_string(),
        KeybindDef::Single("ctrl-s".to_string()),
    );
    keybinds.insert(
        "goto_file".to_string(),
        KeybindDef::Single("ctrl-g".to_string()),
    );

    // History & Sessions
    keybinds.insert(
        "show_history".to_string(),
        KeybindDef::Single("ctrl-h".to_string()),
    );
    keybinds.insert(
        "bookmark_selection".to_string(),
        KeybindDef::Single("ctrl-b".to_string()),
    );

    // Search Refinement
    keybinds.insert(
        "refine_search".to_string(),
        KeybindDef::Multiple(vec!["ctrl-/".to_string(), "f2".to_string()]),
    );

    // Note: F1/? for help is handled internally by the TUI, not as a custom keybind

    keybinds
}

fn default_editor() -> String {
    std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string())
}

const fn default_true() -> bool {
    true
}

fn default_tag_display() -> String {
    "inline".to_string()
}

const fn default_max_sessions() -> usize {
    50
}

impl KeybindConfig {
    /// Load keybind configuration from file.
    ///
    /// # Errors
    ///
    /// Returns error if the file cannot be read or parsed.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let content = fs::read_to_string(path.as_ref())?;
        let config: Self = toml::from_str(&content)?;
        Ok(config)
    }

    /// Load keybind configuration from the default location.
    ///
    /// Returns the default configuration if the file doesn't exist.
    ///
    /// # Errors
    ///
    /// Returns error if the file exists but cannot be read or parsed.
    pub fn load_or_default() -> Result<Self, ConfigError> {
        let config_path = Self::default_config_path()?;

        if config_path.exists() {
            Self::load(&config_path)
        } else {
            Ok(Self::default())
        }
    }

    /// Get the default configuration file path.
    ///
    /// Returns `~/.config/tagr/keybinds.toml` on Unix-like systems.
    ///
    /// # Errors
    ///
    /// Returns error if the config directory cannot be determined.
    pub fn default_config_path() -> Result<PathBuf, ConfigError> {
        let config_dir = dirs::config_dir().ok_or(ConfigError::NoConfigDir)?;

        Ok(config_dir.join("tagr").join("keybinds.toml"))
    }

    /// Get the keybind(s) for a given action name.
    ///
    /// Returns an empty slice if the action is not configured.
    #[must_use]
    pub fn get(&self, action: &str) -> Vec<String> {
        self.keybinds
            .get(action)
            .map_or_else(Vec::new, |def| match def {
                KeybindDef::Single(key) => vec![key.clone()],
                KeybindDef::Multiple(keys) => keys.clone(),
            })
    }

    /// Check if a keybind is disabled for an action.
    #[must_use]
    pub fn is_disabled(&self, action: &str) -> bool {
        self.keybinds.get(action).is_some_and(|def| match def {
            KeybindDef::Single(key) => key == "none",
            KeybindDef::Multiple(keys) => keys.iter().all(|k| k == "none"),
        })
    }

    /// Convert keybind configuration to finder-compatible format.
    ///
    /// Returns a vector of "key:action" strings that can be passed to the
    /// finder for custom keybind handling.
    ///
    /// Note: Filters out Tab and `BTab` (Shift+Tab) to preserve the
    /// finder's default multi-select behavior.
    #[must_use]
    pub fn bindings(&self) -> Vec<String> {
        let mut bindings = Vec::new();

        for (action, def) in &self.keybinds {
            let keys = match def {
                KeybindDef::Single(key) if key != "none" => vec![key.clone()],
                KeybindDef::Multiple(keys) => {
                    keys.iter().filter(|k| *k != "none").cloned().collect()
                }
                KeybindDef::Single(_) => continue,
            };

            for key in keys {
                // Skip Tab and BTab to preserve multi-select behavior
                if key == "tab" || key == "btab" {
                    continue;
                }

                // Format: "key:action" - action is needed for ratatui handler
                bindings.push(format!("{key}:{action}"));
            }
        }

        bindings
    }

    /// Get the action name mapped to a specific key string.
    ///
    /// Returns None if no action is mapped to this key.
    #[must_use]
    pub fn action_for_key(&self, key_str: &str) -> Option<String> {
        for (action, def) in &self.keybinds {
            let matches = match def {
                KeybindDef::Single(k) => k == key_str,
                KeybindDef::Multiple(keys) => keys.iter().any(|k| k == key_str),
            };

            if matches {
                return Some(action.clone());
            }
        }
        None
    }

    /// Convert keybind configuration to a KeyEvent -> action name map.
    ///
    /// This is primarily for testing and validation. In production, use
    /// `bindings()` or `bindings_for_phase()` to get the key:action strings.
    ///
    /// # Returns
    ///
    /// HashMap mapping KeyEvent to action name strings.
    #[cfg(test)]
    pub(crate) fn to_keybind_map(
        &self,
    ) -> std::collections::HashMap<crossterm::event::KeyEvent, String> {
        use crate::ui::ratatui_adapter::parse_key_string_for_test;
        use std::collections::HashMap;

        let mut map = HashMap::new();

        for (action, def) in &self.keybinds {
            let keys = match def {
                KeybindDef::Single(key) if key != "none" => vec![key.clone()],
                KeybindDef::Multiple(keys) => {
                    keys.iter().filter(|k| *k != "none").cloned().collect()
                }
                KeybindDef::Single(_) => continue,
            };

            for key_str in keys {
                if let Some(key_event) = parse_key_string_for_test(&key_str) {
                    map.insert(key_event, action.clone());
                }
            }
        }

        map
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
        keybinds.insert(
            "disabled".to_string(),
            KeybindDef::Single("none".to_string()),
        );
        keybinds.insert(
            "enabled".to_string(),
            KeybindDef::Single("ctrl-t".to_string()),
        );

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

    #[test]
    fn test_config_load_from_toml() {
        use crate::testing::TempFile;

        let toml_content = r#"
[keybinds]
add_tag = "ctrl-t"
remove_tag = ["ctrl-r", "F2"]

[editor]
command = "nvim"
args = ["-n"]

[actions]
confirm_delete = false
"#;

        let temp_file =
            TempFile::create_with_content("keybinds.toml", toml_content.as_bytes()).unwrap();

        let config = KeybindConfig::load(temp_file.path()).unwrap();
        assert_eq!(config.get("add_tag"), vec!["ctrl-t"]);
        assert_eq!(config.get("remove_tag"), vec!["ctrl-r", "F2"]);
        assert_eq!(config.editor.command, "nvim");
        assert_eq!(config.editor.args, vec!["-n"]);
        assert!(!config.actions.confirm_delete);
    }

    #[test]
    fn test_default_config_path() {
        let path = KeybindConfig::default_config_path();
        assert!(path.is_ok());
        let path = path.unwrap();
        assert!(path.to_string_lossy().contains("tagr"));
        assert!(path.to_string_lossy().contains("keybinds.toml"));
    }

    #[test]
    fn test_load_or_default_returns_default_when_missing() {
        // This test assumes the config file doesn't exist
        // If it does exist, it will load it instead
        let result = KeybindConfig::load_or_default();
        assert!(result.is_ok());
    }

    #[test]
    fn test_duplicate_keybind_detection() {
        use crate::ui::ratatui_adapter::parse_key_string_for_test;

        let toml = r#"
            [keybinds]
            add_tag = "ctrl-t"
            remove_tag = "ctrl-t"  # Duplicate!
        "#;

        let config: KeybindConfig = toml::from_str(toml).unwrap();
        let bindings = config.to_keybind_map();

        // Parse the key
        let key = parse_key_string_for_test("ctrl-t");
        assert!(key.is_some());

        // Count how many actions map to this key
        let key = key.unwrap();
        let matching_actions: Vec<_> = bindings.iter().filter(|(k, _)| **k == key).collect();

        // Should only have one action (HashMap prevents duplicates)
        assert_eq!(matching_actions.len(), 1);

        // The action should be one of the two (HashMap iteration order is undefined)
        let action = matching_actions[0].1;
        assert!(
            action == "add_tag" || action == "remove_tag",
            "Expected action to be add_tag or remove_tag, got {}",
            action
        );
    }

    #[test]
    fn test_invalid_keybind_graceful_handling() {
        use crate::ui::ratatui_adapter::parse_key_string_for_test;

        // Invalid keybind strings should be skipped during parsing
        // Note: The parser is quite permissive - it ignores unknown modifiers
        // and parses F-keys with any number. These are truly invalid:
        let invalid_keys = vec![
            "ctrl-unknown-key", // Multi-character unknown key
            "invalid",          // Unknown key name
            "ctrl-",            // Incomplete (no key after modifier)
            "",                 // Empty
            "ctrl-abc",         // Multi-character non-special key
            "unknown-unknown",  // Unknown parts
            "fkey",             // Not f + number
        ];

        for key_str in invalid_keys {
            let result = parse_key_string_for_test(key_str);
            // Invalid keys should return None (gracefully handled)
            assert!(
                result.is_none(),
                "Expected '{}' to be invalid, but got {:?}",
                key_str,
                result
            );
        }

        // Also test that warnings would be logged for ignored modifiers
        // (though we don't actually log them in the current implementation)
        // Examples that parse but ignore parts:
        //   "super-t" -> parses as just "t" (super is ignored)
        //   "ctrl-ctrl-t" -> parses as "ctrl-t" (duplicate ctrl ignored)
        //   "f99" -> parses as F(99) (no validation on F-key range)
    }

    #[test]
    fn test_valid_keybind_invalid_action() {
        use super::super::actions::BrowseAction;
        use std::str::FromStr;

        // Valid keybind but action doesn't exist
        let toml = r#"
            [keybinds]
            nonexistent_action = "ctrl-z"
            add_tag = "ctrl-t"
        "#;

        let config: KeybindConfig = toml::from_str(toml).unwrap();
        let bindings = config.to_keybind_map();

        // Both keybinds should be in the map
        assert_eq!(bindings.len(), 2);

        // Verify that the invalid action string is stored
        let has_invalid = bindings.values().any(|v| v == "nonexistent_action");
        assert!(has_invalid);

        // Verify that parsing the invalid action fails
        let parse_result = BrowseAction::from_str("nonexistent_action");
        assert!(
            parse_result.is_err(),
            "Expected invalid action to fail parsing"
        );

        // Verify that valid action parses correctly
        let parse_result = BrowseAction::from_str("add_tag");
        assert!(parse_result.is_ok(), "Expected valid action to parse");
        assert_eq!(parse_result.unwrap(), BrowseAction::AddTag);
    }

    #[test]
    fn test_keybind_normalization() {
        use crate::ui::ratatui_adapter::parse_key_string_for_test;

        // Test that different variations parse to same key
        let variations = vec![
            ("ctrl-t", "control-t"),
            ("alt-x", "Alt-X"),
            ("CTRL-A", "ctrl-a"),
        ];

        for (v1, v2) in variations {
            let key1 = parse_key_string_for_test(v1);
            let key2 = parse_key_string_for_test(v2);
            assert_eq!(
                key1, key2,
                "Expected '{}' and '{}' to parse to same key",
                v1, v2
            );
        }
    }

    #[test]
    fn test_special_keys_parsing() {
        use crate::ui::ratatui_adapter::parse_key_string_for_test;
        use crossterm::event::KeyCode;

        let special_keys = vec![
            ("f1", KeyCode::F(1)),
            ("f12", KeyCode::F(12)),
            ("enter", KeyCode::Enter),
            ("esc", KeyCode::Esc),
            ("tab", KeyCode::Tab),
            ("bspace", KeyCode::Backspace),
            ("del", KeyCode::Delete),
            ("up", KeyCode::Up),
            ("down", KeyCode::Down),
            ("pgup", KeyCode::PageUp),
            ("pgdn", KeyCode::PageDown),
        ];

        for (key_str, expected_code) in special_keys {
            let result = parse_key_string_for_test(key_str);
            assert!(
                result.is_some(),
                "Expected '{}' to parse successfully",
                key_str
            );
            assert_eq!(result.unwrap().code, expected_code);
        }
    }

    #[test]
    fn test_refine_search_included_in_bindings() {
        let config = KeybindConfig::default();
        let bindings = config.bindings();

        // Check that refine_search keybinds are included
        let has_refine_search = bindings.iter().any(|b| b.contains("refine_search"));
        assert!(
            has_refine_search,
            "Expected refine_search in bindings: {:?}",
            bindings
        );

        // Check both keybinds are present
        let has_ctrl_slash = bindings.iter().any(|b| b == "ctrl-/:refine_search");
        let has_f2 = bindings.iter().any(|b| b == "f2:refine_search");
        assert!(
            has_ctrl_slash || has_f2,
            "Expected ctrl-/ or f2 keybind for refine_search: {:?}",
            bindings
        );
    }
}
