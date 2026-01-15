//! Help text generation from keybind metadata

use crate::keybinds::config::KeybindConfig;
use crate::keybinds::metadata::{ActionCategory, ActionRegistry};

/// Generate formatted help text for F1 menu based on configured keybinds
#[must_use]
pub fn generate_help_text(config: &KeybindConfig) -> String {
    let mut output = String::new();

    output.push_str("╔═══════════════════════════════════════════════════════════╗\n");
    output.push_str("║              Tagr Browse Mode - Keybind Reference         ║\n");
    output.push_str("╚═══════════════════════════════════════════════════════════╝\n\n");

    // Built-in navigation (not configurable)
    output.push_str("NAVIGATION (vim-style):\n");
    output.push_str("  j/k or ↑/↓    Move cursor in active pane\n");
    output.push_str("  h/l or ←/→    Switch between panes (tag tree ↔ file list)\n");
    output.push_str("  PgUp/PgDn     Page up/down\n");
    output.push_str("  Home/End      Jump to start/end\n");
    output.push_str("  Tab           Toggle item selection (multi-select)\n");
    output.push_str("  Shift+↑/↓     Scroll preview pane\n");
    output.push_str("  Enter         Confirm selection\n");
    output.push_str("  ESC           Cancel and exit\n\n");

    // Search (partially configurable)
    output.push_str("SEARCH & FILTER:\n");
    output.push_str("  /             Start fuzzy search (type to filter)\n");
    output.push_str("  Ctrl+U        Clear search query\n");
    output.push_str("  Ctrl+W        Delete word in query\n");
    output.push_str("  ←/→           Move cursor in query (when searching)\n");

    // Refine search is configurable
    if let Some(meta) = ActionRegistry::get_by_id("refine_search")
        && !config.is_disabled("refine_search")
    {
        let keys = meta.primary_key_human(config);
        output.push_str(&format!("  {:<14}{}\n", keys, meta.description));
    }
    output.push('\n');

    // Generate sections for each category
    for category in [
        ActionCategory::TagManagement,
        ActionCategory::FileOperations,
        ActionCategory::NotesAndPreview,
    ] {
        let actions = ActionRegistry::by_category(category);
        let actions_enabled: Vec<_> = actions
            .iter()
            .filter(|m| !config.is_disabled(m.id))
            .collect();

        if actions_enabled.is_empty() {
            continue;
        }

        // Category header
        output.push_str(&format!("{}:\n", category_name(category)));

        for meta in actions_enabled {
            let keys = meta.primary_key_human(config);
            output.push_str(&format!("  {:<14}{}\n", keys, meta.description));
        }
        output.push('\n');
    }

    output.push_str("Press any key to close this help screen\n");

    output
}

/// Get category display name
const fn category_name(category: ActionCategory) -> &'static str {
    match category {
        ActionCategory::Search => "SEARCH & FILTER",
        ActionCategory::TagManagement => "TAG MANAGEMENT",
        ActionCategory::FileOperations => "FILE OPERATIONS",
        ActionCategory::NotesAndPreview => "NOTES & PREVIEW",
        ActionCategory::System => "SYSTEM",
    }
}

/// Generate keybind list for TUI help overlay
///
/// Returns a vector of (key, description) tuples that can be displayed
/// in the help overlay. Uses the configured keybinds from the user's config.
#[must_use]
pub fn generate_overlay_binds(config: &KeybindConfig) -> Vec<(String, String)> {
    let mut binds: Vec<(String, String)> = ActionRegistry::all()
        .iter()
        .filter(|m| !config.is_disabled(m.id))
        .flat_map(|meta| {
            let keys = meta.get_keys_human(config);
            keys.into_iter()
                .map(|k| (k, meta.short_name.to_string()))
                .collect::<Vec<_>>()
        })
        .collect();

    // Sort by key for consistent display
    binds.sort_by(|a, b| a.0.cmp(&b.0));

    // Add preview scroll hint (always available)
    binds.push(("Shift+↑/↓".to_string(), "scroll preview".to_string()));

    binds
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_help_includes_categories() {
        let config = KeybindConfig::default();
        let help = generate_help_text(&config);

        assert!(help.contains("TAG MANAGEMENT:"));
        assert!(help.contains("FILE OPERATIONS:"));
        assert!(help.contains("NOTES & PREVIEW:"));
        assert!(help.contains("NAVIGATION (vim-style):"));
    }

    #[test]
    fn test_generate_help_includes_vim_keys() {
        let config = KeybindConfig::default();
        let help = generate_help_text(&config);

        assert!(help.contains("j/k or ↑/↓"));
        assert!(help.contains("h/l or ←/→"));
    }

    #[test]
    fn test_generate_help_includes_slash_search() {
        let config = KeybindConfig::default();
        let help = generate_help_text(&config);

        assert!(help.contains("/             Start fuzzy search"));
    }

    #[test]
    fn test_generate_overlay_binds_not_empty() {
        let config = KeybindConfig::default();
        let binds = generate_overlay_binds(&config);

        assert!(!binds.is_empty());
        assert!(binds.iter().any(|(k, _)| k.contains("Ctrl")));
    }

    #[test]
    fn test_generate_overlay_includes_scroll() {
        let config = KeybindConfig::default();
        let binds = generate_overlay_binds(&config);

        assert!(
            binds
                .iter()
                .any(|(k, d)| k == "Shift+↑/↓" && d == "scroll preview")
        );
    }

    #[test]
    fn test_category_name() {
        assert_eq!(
            category_name(ActionCategory::TagManagement),
            "TAG MANAGEMENT"
        );
        assert_eq!(
            category_name(ActionCategory::FileOperations),
            "FILE OPERATIONS"
        );
        assert_eq!(
            category_name(ActionCategory::NotesAndPreview),
            "NOTES & PREVIEW"
        );
    }
}
