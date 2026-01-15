//! Action metadata registry - single source of truth for keybind information

use crate::keybinds::actions::BrowseAction;
use crate::keybinds::config::KeybindConfig;

/// Metadata for a browse action - single source of truth
#[derive(Debug, Clone)]
pub struct ActionMetadata {
    /// Action enum variant
    pub action: BrowseAction,

    /// Internal action identifier (e.g., "add_tag")
    pub id: &'static str,

    /// Default keybind(s) in internal format (e.g., "ctrl-t")
    pub default_keys: &'static [&'static str],

    /// Short human-readable name (e.g., "Add Tags")
    pub short_name: &'static str,

    /// Full description (e.g., "Add tags to selected files")
    pub description: &'static str,

    /// Category for grouping in help
    pub category: ActionCategory,

    /// Whether available in tag selection phase
    pub available_in_tag_phase: bool,

    /// Whether available in file selection phase
    pub available_in_file_phase: bool,
}

/// Category for organizing actions in help displays
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionCategory {
    /// Navigation and search actions
    Search,
    /// Tag management actions
    TagManagement,
    /// File operation actions
    FileOperations,
    /// Notes and preview actions
    NotesAndPreview,
    /// System actions (help, etc.)
    System,
}

impl ActionMetadata {
    /// Convert internal key format to human-readable (e.g., "ctrl-t" -> "Ctrl+T")
    #[must_use]
    pub fn format_key(key: &str) -> String {
        key.split('-')
            .map(|part| match part {
                "ctrl" => "Ctrl".to_string(),
                "alt" => "Alt".to_string(),
                "shift" => "Shift".to_string(),
                "pgup" => "PgUp".to_string(),
                "pgdn" => "PgDn".to_string(),
                "bspace" => "Backspace".to_string(),
                "btab" => "Shift+Tab".to_string(),
                "f1" => "F1".to_string(),
                "f2" => "F2".to_string(),
                other if other.len() == 1 => other.to_uppercase(),
                other => {
                    // Capitalize first letter
                    let mut chars = other.chars();
                    match chars.next() {
                        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                        None => other.to_string(),
                    }
                }
            })
            .collect::<Vec<_>>()
            .join("+")
    }

    /// Get configured keybind(s) from config, falling back to defaults
    #[must_use]
    pub fn get_keys(&self, config: &KeybindConfig) -> Vec<String> {
        let configured = config.get(self.id);
        if configured.is_empty() {
            self.default_keys.iter().map(|s| (*s).to_string()).collect()
        } else {
            configured
        }
    }

    /// Get human-readable keybind(s)
    #[must_use]
    pub fn get_keys_human(&self, config: &KeybindConfig) -> Vec<String> {
        self.get_keys(config)
            .iter()
            .map(|k| Self::format_key(k))
            .collect()
    }

    /// Get primary keybind (first one) in human format
    #[must_use]
    pub fn primary_key_human(&self, config: &KeybindConfig) -> String {
        self.get_keys_human(config)
            .first()
            .cloned()
            .unwrap_or_else(|| "None".to_string())
    }
}

/// Global registry of all action metadata
pub struct ActionRegistry;

impl ActionRegistry {
    /// Get all registered actions
    #[must_use]
    pub const fn all() -> &'static [ActionMetadata] {
        ALL_ACTIONS
    }

    /// Get metadata for a specific action
    #[must_use]
    pub fn get(action: &BrowseAction) -> Option<&'static ActionMetadata> {
        ALL_ACTIONS.iter().find(|m| &m.action == action)
    }

    /// Get metadata by action ID
    #[must_use]
    pub fn get_by_id(id: &str) -> Option<&'static ActionMetadata> {
        ALL_ACTIONS.iter().find(|m| m.id == id)
    }

    /// Get actions by category
    #[must_use]
    pub fn by_category(category: ActionCategory) -> Vec<&'static ActionMetadata> {
        ALL_ACTIONS
            .iter()
            .filter(|m| m.category == category)
            .collect()
    }
}

/// Static registry - compile-time constant with all action metadata
static ALL_ACTIONS: &[ActionMetadata] = &[
    // Tag Management
    ActionMetadata {
        action: BrowseAction::AddTag,
        id: "add_tag",
        default_keys: &["ctrl-t"],
        short_name: "Add Tags",
        description: "Add tags to selected files",
        category: ActionCategory::TagManagement,
        available_in_tag_phase: false,
        available_in_file_phase: true,
    },
    ActionMetadata {
        action: BrowseAction::RemoveTag,
        id: "remove_tag",
        default_keys: &["ctrl-r"],
        short_name: "Remove Tags",
        description: "Remove tags from selected files",
        category: ActionCategory::TagManagement,
        available_in_tag_phase: false,
        available_in_file_phase: true,
    },
    ActionMetadata {
        action: BrowseAction::EditTags,
        id: "edit_tags",
        default_keys: &["ctrl-e"],
        short_name: "Edit Tags",
        description: "Edit tags in $EDITOR",
        category: ActionCategory::TagManagement,
        available_in_tag_phase: false,
        available_in_file_phase: true,
    },
    // File Operations
    ActionMetadata {
        action: BrowseAction::OpenInDefault,
        id: "open_default",
        default_keys: &["ctrl-o"],
        short_name: "Open File",
        description: "Open file in default application",
        category: ActionCategory::FileOperations,
        available_in_tag_phase: false,
        available_in_file_phase: true,
    },
    ActionMetadata {
        action: BrowseAction::OpenInEditor,
        id: "open_editor",
        default_keys: &["ctrl-v"],
        short_name: "Edit File",
        description: "Open file in $EDITOR",
        category: ActionCategory::FileOperations,
        available_in_tag_phase: false,
        available_in_file_phase: true,
    },
    ActionMetadata {
        action: BrowseAction::CopyPath,
        id: "copy_path",
        default_keys: &["ctrl-y"],
        short_name: "Copy Paths",
        description: "Copy file paths to clipboard",
        category: ActionCategory::FileOperations,
        available_in_tag_phase: false,
        available_in_file_phase: true,
    },
    ActionMetadata {
        action: BrowseAction::CopyFiles,
        id: "copy_files",
        default_keys: &["ctrl-p"],
        short_name: "Copy Files",
        description: "Copy files to directory",
        category: ActionCategory::FileOperations,
        available_in_tag_phase: false,
        available_in_file_phase: true,
    },
    ActionMetadata {
        action: BrowseAction::DeleteFromDb,
        id: "delete_from_db",
        default_keys: &["ctrl-d"],
        short_name: "Delete from DB",
        description: "Delete file from database",
        category: ActionCategory::FileOperations,
        available_in_tag_phase: false,
        available_in_file_phase: true,
    },
    // Notes & Preview
    ActionMetadata {
        action: BrowseAction::EditNote,
        id: "edit_note",
        default_keys: &["ctrl-n"],
        short_name: "Edit Note",
        description: "Edit note for selected file",
        category: ActionCategory::NotesAndPreview,
        available_in_tag_phase: true,
        available_in_file_phase: true,
    },
    ActionMetadata {
        action: BrowseAction::ToggleNotePreview,
        id: "toggle_note_preview",
        default_keys: &["alt-n"],
        short_name: "Toggle Preview",
        description: "Toggle between file and note preview",
        category: ActionCategory::NotesAndPreview,
        available_in_tag_phase: true,
        available_in_file_phase: true,
    },
    ActionMetadata {
        action: BrowseAction::ShowDetails,
        id: "show_details",
        default_keys: &["ctrl-l"],
        short_name: "Show Details",
        description: "Show detailed file information",
        category: ActionCategory::NotesAndPreview,
        available_in_tag_phase: true,
        available_in_file_phase: true,
    },
    // Search & Filter
    ActionMetadata {
        action: BrowseAction::RefineSearch,
        id: "refine_search",
        default_keys: &["f2"],
        short_name: "Refine Search",
        description: "Refine search criteria (tags/files/vtags/excludes)",
        category: ActionCategory::Search,
        available_in_tag_phase: true,
        available_in_file_phase: true,
    },
    // System
    ActionMetadata {
        action: BrowseAction::ShowHelp,
        id: "show_help",
        default_keys: &["f1"],
        short_name: "Help",
        description: "Show this help",
        category: ActionCategory::System,
        available_in_tag_phase: true,
        available_in_file_phase: true,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_key() {
        assert_eq!(ActionMetadata::format_key("ctrl-t"), "Ctrl+T");
        assert_eq!(ActionMetadata::format_key("alt-n"), "Alt+N");
        assert_eq!(ActionMetadata::format_key("f2"), "F2");
        assert_eq!(ActionMetadata::format_key("f1"), "F1");
        assert_eq!(ActionMetadata::format_key("pgup"), "PgUp");
        assert_eq!(ActionMetadata::format_key("ctrl-shift-t"), "Ctrl+Shift+T");
    }

    #[test]
    fn test_registry_get() {
        let meta = ActionRegistry::get(&BrowseAction::AddTag);
        assert!(meta.is_some());
        assert_eq!(meta.unwrap().id, "add_tag");
    }

    #[test]
    fn test_registry_get_by_id() {
        let meta = ActionRegistry::get_by_id("add_tag");
        assert!(meta.is_some());
        assert_eq!(meta.unwrap().action, BrowseAction::AddTag);
    }

    #[test]
    fn test_registry_by_category() {
        let tag_actions = ActionRegistry::by_category(ActionCategory::TagManagement);
        assert!(tag_actions.len() >= 3); // AddTag, RemoveTag, EditTags
        assert!(tag_actions.iter().any(|m| m.action == BrowseAction::AddTag));
    }

    #[test]
    fn test_no_duplicate_ids() {
        let ids: Vec<_> = ALL_ACTIONS.iter().map(|m| m.id).collect();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(
            ids.len(),
            unique.len(),
            "Duplicate action IDs found in registry"
        );
    }

    #[test]
    fn test_get_keys_with_default_config() {
        let config = KeybindConfig::default();
        let meta = ActionRegistry::get(&BrowseAction::AddTag).unwrap();
        let keys = meta.get_keys(&config);
        assert!(!keys.is_empty());
    }

    #[test]
    fn test_primary_key_human() {
        let config = KeybindConfig::default();
        let meta = ActionRegistry::get(&BrowseAction::AddTag).unwrap();
        let key = meta.primary_key_human(&config);
        assert_eq!(key, "Ctrl+T");
    }
}
