//! Action types for browse mode keybinds.

use std::path::PathBuf;

/// Actions that can be triggered by keybinds in browse mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowseAction {
    /// Add tags to selected file(s) - Ctrl+T
    AddTag,
    /// Remove tags from selected file(s) - Ctrl+R
    RemoveTag,
    /// Edit tags in external editor - Ctrl+E
    EditTags,
    
    /// Open file(s) in default application - Ctrl+O
    OpenInDefault,
    /// Open file(s) in configured editor - Ctrl+V
    OpenInEditor,
    /// Copy file path(s) to clipboard - Ctrl+Y
    CopyPath,
    /// Copy file(s) to directory - Ctrl+P
    CopyFiles,
    /// Delete file(s) from database - Ctrl+D
    DeleteFromDb,
    
    /// Toggle tag display mode - Ctrl+I
    ToggleTagDisplay,
    /// Show detailed file information - Ctrl+L
    ShowDetails,
    /// Filter by file extension - Ctrl+F
    FilterExtension,
    
    /// Select all visible files - Ctrl+A
    SelectAll,
    /// Clear current selection - Ctrl+X
    ClearSelection,
    
    /// Quick tag search - Ctrl+S
    QuickTagSearch,
    /// Go to specific file - Ctrl+G
    GoToFile,
    
    /// Show recent selections - Ctrl+H
    ShowHistory,
    /// Bookmark current selection - Ctrl+B
    BookmarkSelection,
    
    /// Show help screen - Ctrl+? or F1
    ShowHelp,
    /// Cancel current operation
    Cancel,
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
        )
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
            Self::ToggleTagDisplay => "Toggle tag display mode",
            Self::ShowDetails => "Show file details",
            Self::FilterExtension => "Filter by extension",
            Self::SelectAll => "Select all files",
            Self::ClearSelection => "Clear selection",
            Self::QuickTagSearch => "Quick tag search",
            Self::GoToFile => "Go to file",
            Self::ShowHistory => "Show recent selections",
            Self::BookmarkSelection => "Bookmark selection",
            Self::ShowHelp => "Show help",
            Self::Cancel => "Cancel",
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
}
