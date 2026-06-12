//! Action types for browse mode keybinds.

use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

/// Actions that can be triggered by keybinds in browse mode.
///
/// Split into two categories:
/// - **Configurable actions**: User-facing operations (tag management, file ops)
///   that can be remapped via keybind config.
/// - **Navigation actions**: Internal UI actions (cursor movement, search input)
///   that use fixed keybinds and are not user-configurable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowseAction {
    // === Configurable actions (user-facing, remappable) ===
    /// Add tags to selected file(s)
    AddTag,
    /// Remove tags from selected file(s)
    RemoveTag,
    /// Edit tags in external editor
    EditTags,
    /// Open file(s) in default application
    OpenInDefault,
    /// Open file(s) in configured editor
    OpenInEditor,
    /// Copy file path(s) to clipboard
    CopyPath,
    /// Copy file(s) to directory
    CopyFiles,
    /// Delete file(s) from database
    DeleteFromDb,
    /// Show detailed file information
    ShowDetails,
    /// Edit note for selected file
    EditNote,
    /// Toggle between file and note preview
    ToggleNotePreview,
    /// Refine search criteria
    RefineSearch,
    /// Show help screen
    ShowHelp,
    /// Show watch rules modal
    ShowWatchRules,
    /// Cancel current operation
    Cancel,

    // === Navigation actions (fixed keybinds, not user-configurable) ===
    /// Move cursor up in the active list
    MoveUp,
    /// Move cursor down in the active list
    MoveDown,
    /// Move cursor up by one page
    PageUp,
    /// Move cursor down by one page
    PageDown,
    /// Jump to the first item
    JumpStart,
    /// Jump to the last item
    JumpEnd,
    /// Scroll preview pane up
    ScrollPreviewUp,
    /// Scroll preview pane down
    ScrollPreviewDown,
    /// Toggle selection on current item (include tag / multi-select file)
    ToggleSelect,
    /// Toggle exclusion on current tag
    ToggleExclude,
    /// Toggle expand/collapse on a tree node
    ExpandToggle,
    /// Move focus to the left pane
    FocusLeft,
    /// Move focus to the right pane
    FocusRight,
    /// Enter search/filter input mode
    EnterSearch,
    /// Exit search mode (keep filter, stop typing)
    ExitSearch,
    /// Confirm current selection
    Confirm,
    /// Abort and exit
    Abort,
    /// Typed character during search
    CharInput(char),
    /// Backspace during search
    Backspace,
    /// Delete forward during search
    Delete,
    /// Move query cursor left
    QueryCursorLeft,
    /// Move query cursor right
    QueryCursorRight,
    /// Clear the entire query
    ClearQuery,
    /// Delete word backwards in query
    DeleteWord,
}

/// Error type for parsing action names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseActionError {
    action_name: String,
}

impl ParseActionError {
    fn new(action_name: impl Into<String>) -> Self {
        Self {
            action_name: action_name.into(),
        }
    }
}

impl fmt::Display for ParseActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Unknown action name: {}", self.action_name)
    }
}

impl std::error::Error for ParseActionError {}

impl FromStr for BrowseAction {
    type Err = ParseActionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "add_tag" => Ok(Self::AddTag),
            "remove_tag" => Ok(Self::RemoveTag),
            "edit_tags" => Ok(Self::EditTags),
            "open_default" => Ok(Self::OpenInDefault),
            "open_editor" => Ok(Self::OpenInEditor),
            "copy_path" => Ok(Self::CopyPath),
            "copy_files" => Ok(Self::CopyFiles),
            "delete_from_db" => Ok(Self::DeleteFromDb),
            "show_details" => Ok(Self::ShowDetails),
            "edit_note" => Ok(Self::EditNote),
            "toggle_note_preview" => Ok(Self::ToggleNotePreview),
            "refine_search" => Ok(Self::RefineSearch),
            "show_help" => Ok(Self::ShowHelp),
            "show_watch_rules" => Ok(Self::ShowWatchRules),
            "cancel" => Ok(Self::Cancel),
            "move_up" => Ok(Self::MoveUp),
            "move_down" => Ok(Self::MoveDown),
            "page_up" => Ok(Self::PageUp),
            "page_down" => Ok(Self::PageDown),
            "jump_start" => Ok(Self::JumpStart),
            "jump_end" => Ok(Self::JumpEnd),
            "scroll_preview_up" => Ok(Self::ScrollPreviewUp),
            "scroll_preview_down" => Ok(Self::ScrollPreviewDown),
            "toggle_select" => Ok(Self::ToggleSelect),
            "toggle_exclude" => Ok(Self::ToggleExclude),
            "expand_toggle" => Ok(Self::ExpandToggle),
            "focus_left" => Ok(Self::FocusLeft),
            "focus_right" => Ok(Self::FocusRight),
            "enter_search" => Ok(Self::EnterSearch),
            "exit_search" => Ok(Self::ExitSearch),
            "confirm" => Ok(Self::Confirm),
            "abort" => Ok(Self::Abort),
            "backspace" => Ok(Self::Backspace),
            "delete" => Ok(Self::Delete),
            "query_cursor_left" => Ok(Self::QueryCursorLeft),
            "query_cursor_right" => Ok(Self::QueryCursorRight),
            "clear_query" => Ok(Self::ClearQuery),
            "delete_word" => Ok(Self::DeleteWord),
            _ => Err(ParseActionError::new(s)),
        }
    }
}

/// Result of executing a browse action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionResult {
    /// Continue browsing with unchanged state
    Continue,
    /// Refresh the view and continue browsing
    Refresh,
    /// Exit browse mode with the given file selection
    Exit(Vec<PathBuf>),
    /// Show a message to the user and continue
    Message(String),
}

impl BrowseAction {
    /// Returns whether this action can be remapped by user keybind configuration.
    ///
    /// Navigation and text input actions use fixed keybinds and are not configurable.
    #[must_use]
    pub const fn is_configurable(&self) -> bool {
        matches!(
            self,
            Self::AddTag
                | Self::RemoveTag
                | Self::EditTags
                | Self::OpenInDefault
                | Self::OpenInEditor
                | Self::CopyPath
                | Self::CopyFiles
                | Self::DeleteFromDb
                | Self::ShowDetails
                | Self::EditNote
                | Self::ToggleNotePreview
                | Self::RefineSearch
                | Self::ShowHelp
                | Self::ShowWatchRules
                | Self::Cancel
        )
    }

    /// Returns whether this action requires file selection to work.
    #[must_use]
    pub const fn requires_selection(&self) -> bool {
        matches!(
            self,
            Self::RemoveTag
                | Self::OpenInDefault
                | Self::OpenInEditor
                | Self::CopyPath
                | Self::CopyFiles
                | Self::DeleteFromDb
                | Self::EditNote
        )
    }

    /// Returns whether this action is available in tag selection phase.
    ///
    /// Tag phase is for selecting which tags to filter by. Only navigation
    /// and universal actions (help, cancel, note editing, preview toggle, show details) are available.
    #[must_use]
    pub const fn available_in_tag_phase(&self) -> bool {
        matches!(
            self,
            Self::ShowHelp
                | Self::ShowWatchRules
                | Self::Cancel
                | Self::EditNote
                | Self::ToggleNotePreview
                | Self::ShowDetails
                | Self::RefineSearch
        )
    }

    /// Returns whether this action is available in file selection phase.
    ///
    /// File phase has full access to file operations, tag manipulation,
    /// and all other browse actions.
    #[must_use]
    pub const fn available_in_file_phase(&self) -> bool {
        true
    }

    /// Returns a human-readable description of the action.
    #[must_use]
    pub const fn description(&self) -> &'static str {
        match self {
            Self::AddTag => "Add tags to selected files",
            Self::RemoveTag => "Remove tags from selected files",
            Self::EditTags => "Edit tags in $EDITOR",
            Self::OpenInDefault => "Open in default application (xdg-open/open)",
            Self::OpenInEditor => "Open in $EDITOR",
            Self::CopyPath => "Copy file paths to clipboard",
            Self::CopyFiles => "Copy files to directory",
            Self::DeleteFromDb => "Delete from database",
            Self::ShowDetails => "Show file details",
            Self::EditNote => "Edit note for selected file",
            Self::ToggleNotePreview => "Toggle file/note preview",
            Self::RefineSearch => "Refine search criteria",
            Self::ShowHelp => "Show help",
            Self::ShowWatchRules => "Show watch rules",
            Self::Cancel => "Cancel",
            Self::MoveUp => "Move up",
            Self::MoveDown => "Move down",
            Self::PageUp => "Page up",
            Self::PageDown => "Page down",
            Self::JumpStart => "Jump to start",
            Self::JumpEnd => "Jump to end",
            Self::ScrollPreviewUp => "Scroll preview up",
            Self::ScrollPreviewDown => "Scroll preview down",
            Self::ToggleSelect => "Toggle selection",
            Self::ToggleExclude => "Toggle exclusion",
            Self::ExpandToggle => "Expand/collapse",
            Self::FocusLeft => "Focus left pane",
            Self::FocusRight => "Focus right pane",
            Self::EnterSearch => "Enter search mode",
            Self::ExitSearch => "Exit search mode",
            Self::Confirm => "Confirm selection",
            Self::Abort => "Abort",
            Self::CharInput(_) => "Type character",
            Self::Backspace => "Backspace",
            Self::Delete => "Delete forward",
            Self::QueryCursorLeft => "Move cursor left",
            Self::QueryCursorRight => "Move cursor right",
            Self::ClearQuery => "Clear query",
            Self::DeleteWord => "Delete word",
        }
    }

    /// Returns a dynamic description with resolved editor command.
    ///
    /// For editor-related actions, this will show the actual editor command
    /// from the environment or configuration instead of "$EDITOR".
    #[must_use]
    pub fn description_with_editor(&self, editor: &str) -> String {
        match self {
            Self::EditTags => format!("Edit tags in {editor}"),
            Self::OpenInEditor => format!("Open in {editor}"),
            _ => self.description().to_string(),
        }
    }

    /// Returns whether this action requires text input before executing.
    #[must_use]
    pub const fn requires_input(&self) -> bool {
        matches!(self, Self::AddTag | Self::RemoveTag)
    }

    /// Returns whether this action requires user confirmation before executing.
    #[must_use]
    pub const fn requires_confirmation(&self) -> bool {
        matches!(self, Self::DeleteFromDb)
    }

    /// Returns whether this action requires special handling (e.g., terminal suspend).
    #[must_use]
    pub const fn requires_special_handling(&self) -> bool {
        matches!(self, Self::EditNote | Self::RefineSearch)
    }

    /// Returns the prompt title and placeholder for input-requiring actions.
    #[must_use]
    pub fn input_prompt(&self) -> (String, String) {
        match self {
            Self::AddTag => (
                "Add Tags".to_string(),
                "Enter tags (space-separated)".to_string(),
            ),
            Self::RemoveTag => (
                "Remove Tags".to_string(),
                "Enter tags to remove".to_string(),
            ),
            _ => ("Input".to_string(), "Enter value".to_string()),
        }
    }

    /// Returns the confirmation prompt for confirmation-requiring actions.
    #[must_use]
    pub fn confirmation_prompt(&self) -> (String, String) {
        match self {
            Self::DeleteFromDb => (
                "Confirm Deletion".to_string(),
                "Are you sure you want to remove this file from the database?".to_string(),
            ),
            _ => ("Confirm Action".to_string(), "Are you sure?".to_string()),
        }
    }

    /// Returns the string identifier for this action.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::AddTag => "add_tag",
            Self::RemoveTag => "remove_tag",
            Self::EditTags => "edit_tags",
            Self::OpenInDefault => "open_default",
            Self::OpenInEditor => "open_editor",
            Self::CopyPath => "copy_path",
            Self::CopyFiles => "copy_files",
            Self::DeleteFromDb => "delete_from_db",
            Self::ShowDetails => "show_details",
            Self::EditNote => "edit_note",
            Self::ToggleNotePreview => "toggle_note_preview",
            Self::RefineSearch => "refine_search",
            Self::ShowHelp => "show_help",
            Self::ShowWatchRules => "show_watch_rules",
            Self::Cancel => "cancel",
            Self::MoveUp => "move_up",
            Self::MoveDown => "move_down",
            Self::PageUp => "page_up",
            Self::PageDown => "page_down",
            Self::JumpStart => "jump_start",
            Self::JumpEnd => "jump_end",
            Self::ScrollPreviewUp => "scroll_preview_up",
            Self::ScrollPreviewDown => "scroll_preview_down",
            Self::ToggleSelect => "toggle_select",
            Self::ToggleExclude => "toggle_exclude",
            Self::ExpandToggle => "expand_toggle",
            Self::FocusLeft => "focus_left",
            Self::FocusRight => "focus_right",
            Self::EnterSearch => "enter_search",
            Self::ExitSearch => "exit_search",
            Self::Confirm => "confirm",
            Self::Abort => "abort",
            Self::CharInput(_) => "char_input",
            Self::Backspace => "backspace",
            Self::Delete => "delete",
            Self::QueryCursorLeft => "query_cursor_left",
            Self::QueryCursorRight => "query_cursor_right",
            Self::ClearQuery => "clear_query",
            Self::DeleteWord => "delete_word",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_requires_selection() {
        assert!(!BrowseAction::AddTag.requires_selection());
        assert!(BrowseAction::RemoveTag.requires_selection());
        assert!(BrowseAction::CopyPath.requires_selection());
    }

    #[test]
    fn test_phase_availability() {
        // Tag phase: only help, cancel, note editing, preview toggle, show details
        assert!(BrowseAction::ShowHelp.available_in_tag_phase());
        assert!(BrowseAction::Cancel.available_in_tag_phase());
        assert!(BrowseAction::EditNote.available_in_tag_phase());
        assert!(BrowseAction::ToggleNotePreview.available_in_tag_phase());
        assert!(BrowseAction::ShowDetails.available_in_tag_phase());
        assert!(!BrowseAction::AddTag.available_in_tag_phase());
        assert!(!BrowseAction::DeleteFromDb.available_in_tag_phase());
        assert!(!BrowseAction::CopyPath.available_in_tag_phase());

        // File phase: all actions available
        assert!(BrowseAction::ShowHelp.available_in_file_phase());
        assert!(BrowseAction::AddTag.available_in_file_phase());
        assert!(BrowseAction::DeleteFromDb.available_in_file_phase());
        assert!(BrowseAction::CopyPath.available_in_file_phase());
    }

    #[test]
    fn test_description() {
        assert_eq!(
            BrowseAction::AddTag.description(),
            "Add tags to selected files"
        );
    }

    #[test]
    fn test_description_with_editor() {
        assert_eq!(
            BrowseAction::EditTags.description_with_editor("nvim"),
            "Edit tags in nvim"
        );
        assert_eq!(
            BrowseAction::OpenInEditor.description_with_editor("vim"),
            "Open in vim"
        );
        assert_eq!(
            BrowseAction::AddTag.description_with_editor("nvim"),
            "Add tags to selected files"
        );
    }

    #[test]
    fn test_requires_input() {
        assert!(BrowseAction::AddTag.requires_input());
        assert!(BrowseAction::RemoveTag.requires_input());
        assert!(!BrowseAction::DeleteFromDb.requires_input());
        assert!(!BrowseAction::ShowHelp.requires_input());
    }

    #[test]
    fn test_requires_confirmation() {
        assert!(BrowseAction::DeleteFromDb.requires_confirmation());
        assert!(!BrowseAction::AddTag.requires_confirmation());
        assert!(!BrowseAction::ShowHelp.requires_confirmation());
    }

    #[test]
    fn test_requires_special_handling() {
        assert!(BrowseAction::EditNote.requires_special_handling());
        assert!(BrowseAction::RefineSearch.requires_special_handling());
        assert!(!BrowseAction::AddTag.requires_special_handling());
    }

    #[test]
    fn test_as_str() {
        assert_eq!(BrowseAction::AddTag.as_str(), "add_tag");
        assert_eq!(BrowseAction::EditNote.as_str(), "edit_note");
        assert_eq!(BrowseAction::RefineSearch.as_str(), "refine_search");
    }
}
