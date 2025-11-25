//! Pure action business logic for browse workflows
//!
//! This module contains the core business logic for executing actions on files,
//! separated from UI concerns like prompting and message formatting. Functions
//! here return `ActionOutcome` with pure data about what happened, allowing the
//! UI layer to decide how to present results to the user.

use crate::browse::models::ActionOutcome;
use crate::db::{Database, DbError};
use std::path::{Path, PathBuf};

/// Execute tag addition on files (pure business logic)
///
/// Adds the specified tags to the given files. Tags are only added if they
/// don't already exist on the file.
///
/// # Arguments
/// * `db` - Database reference
/// * `files` - Files to add tags to
/// * `new_tags` - Tags to add
///
/// # Returns
/// `ActionOutcome` describing the result
///
/// # Errors
/// Returns `DbError` if database operations fail
pub fn execute_add_tag(
    db: &Database,
    files: &[PathBuf],
    new_tags: &[String],
) -> Result<ActionOutcome, DbError> {
    if files.is_empty() {
        return Ok(ActionOutcome::Failed("No files specified".to_string()));
    }

    if new_tags.is_empty() {
        return Ok(ActionOutcome::Failed("No tags specified".to_string()));
    }

    let mut affected = 0;
    let mut errors = Vec::new();

    for file in files {
        match add_tags_to_file(db, file, new_tags) {
            Ok(true) => affected += 1,
            Ok(false) => {} // No change needed
            Err(e) => errors.push(format!("{}: {}", file.display(), e)),
        }
    }

    if errors.is_empty() {
        Ok(ActionOutcome::Success {
            affected_count: affected,
            details: format!("Added tags: {}", new_tags.join(", ")),
        })
    } else if affected > 0 {
        Ok(ActionOutcome::Partial {
            succeeded: affected,
            failed: errors.len(),
            errors,
        })
    } else {
        Ok(ActionOutcome::Failed(format!(
            "Failed to add tags:\n{}",
            errors.join("\n")
        )))
    }
}

/// Helper: Add tags to a single file
fn add_tags_to_file(db: &Database, file: &Path, new_tags: &[String]) -> Result<bool, DbError> {
    let mut tags = db.get_tags(file)?.unwrap_or_default();
    let original_len = tags.len();

    for tag in new_tags {
        if !tags.contains(tag) {
            tags.push(tag.clone());
        }
    }

    if tags.len() > original_len {
        db.insert(file, tags)?;
        Ok(true) // Changed
    } else {
        Ok(false) // No change
    }
}

/// Execute tag removal from files (pure business logic)
///
/// Removes the specified tags from the given files. Only removes tags that
/// actually exist on each file.
///
/// # Arguments
/// * `db` - Database reference
/// * `files` - Files to remove tags from
/// * `tags_to_remove` - Tags to remove
///
/// # Returns
/// `ActionOutcome` describing the result
///
/// # Errors
/// Returns `DbError` if database operations fail
pub fn execute_remove_tag(
    db: &Database,
    files: &[PathBuf],
    tags_to_remove: &[String],
) -> Result<ActionOutcome, DbError> {
    if files.is_empty() {
        return Ok(ActionOutcome::Failed("No files specified".to_string()));
    }

    if tags_to_remove.is_empty() {
        return Ok(ActionOutcome::Failed("No tags specified".to_string()));
    }

    let mut affected = 0;
    let mut errors = Vec::new();

    for file in files {
        match remove_tags_from_file(db, file, tags_to_remove) {
            Ok(true) => affected += 1,
            Ok(false) => {} // No change needed
            Err(e) => errors.push(format!("{}: {}", file.display(), e)),
        }
    }

    if errors.is_empty() {
        if affected > 0 {
            Ok(ActionOutcome::Success {
                affected_count: affected,
                details: format!("Removed tags: {}", tags_to_remove.join(", ")),
            })
        } else {
            Ok(ActionOutcome::Failed(
                "No tags were removed (tags may not exist on selected files)".to_string(),
            ))
        }
    } else if affected > 0 {
        Ok(ActionOutcome::Partial {
            succeeded: affected,
            failed: errors.len(),
            errors,
        })
    } else {
        Ok(ActionOutcome::Failed(format!(
            "Failed to remove tags:\n{}",
            errors.join("\n")
        )))
    }
}

/// Helper: Remove tags from a single file
fn remove_tags_from_file(
    db: &Database,
    file: &Path,
    tags_to_remove: &[String],
) -> Result<bool, DbError> {
    let Some(mut tags) = db.get_tags(file)? else {
        return Ok(false); // File has no tags
    };

    let original_len = tags.len();
    tags.retain(|tag| !tags_to_remove.contains(tag));

    if tags.len() < original_len {
        db.insert(file, tags)?;
        Ok(true) // Changed
    } else {
        Ok(false) // No change
    }
}

/// Execute database deletion for files (pure business logic)
///
/// Removes file entries from the database. Does not delete the actual files
/// from the filesystem, only their database records.
///
/// # Arguments
/// * `db` - Database reference
/// * `files` - Files to remove from database
///
/// # Returns
/// `ActionOutcome` describing the result
///
/// # Errors
/// Returns `DbError` if database operations fail
pub fn execute_delete_from_db(db: &Database, files: &[PathBuf]) -> Result<ActionOutcome, DbError> {
    if files.is_empty() {
        return Ok(ActionOutcome::Failed("No files specified".to_string()));
    }

    let mut deleted = 0;
    let mut errors = Vec::new();

    for file in files {
        match db.remove(file) {
            Ok(true) => deleted += 1,
            Ok(false) => {} // File wasn't in database
            Err(e) => errors.push(format!("{}: {}", file.display(), e)),
        }
    }

    if errors.is_empty() {
        if deleted > 0 {
            Ok(ActionOutcome::Success {
                affected_count: deleted,
                details: "Deleted from database".to_string(),
            })
        } else {
            Ok(ActionOutcome::Failed(
                "No files were deleted (may not exist in database)".to_string(),
            ))
        }
    } else if deleted > 0 {
        Ok(ActionOutcome::Partial {
            succeeded: deleted,
            failed: errors.len(),
            errors,
        })
    } else {
        Ok(ActionOutcome::Failed(format!(
            "Failed to delete:\n{}",
            errors.join("\n")
        )))
    }
}

/// Execute file opening in default application (pure business logic)
///
/// Opens files using the system's default application handler.
///
/// # Arguments
/// * `files` - Files to open
///
/// # Returns
/// `ActionOutcome` describing the result
#[must_use]
pub fn execute_open_in_default(files: &[PathBuf]) -> ActionOutcome {
    if files.is_empty() {
        return ActionOutcome::Failed("No files specified".to_string());
    }

    let mut opened = 0;
    let mut errors = Vec::new();

    for file in files {
        match open::that(file) {
            Ok(()) => opened += 1,
            Err(e) => errors.push(format!("{}: {}", file.display(), e)),
        }
    }

    if errors.is_empty() {
        ActionOutcome::Success {
            affected_count: opened,
            details: "Opened in default application".to_string(),
        }
    } else if opened > 0 {
        ActionOutcome::Partial {
            succeeded: opened,
            failed: errors.len(),
            errors,
        }
    } else {
        ActionOutcome::Failed(format!("Failed to open files:\n{}", errors.join("\n")))
    }
}

/// Execute file opening in editor (pure business logic)
///
/// Opens files in the specified editor command.
///
/// # Arguments
/// * `files` - Files to open
/// * `editor` - Editor command to use
///
/// # Returns
/// `ActionOutcome` describing the result
#[must_use]
pub fn execute_open_in_editor(files: &[PathBuf], editor: &str) -> ActionOutcome {
    if files.is_empty() {
        return ActionOutcome::Failed("No files specified".to_string());
    }

    let mut cmd = std::process::Command::new(editor);
    for file in files {
        cmd.arg(file);
    }

    match cmd.status() {
        Ok(status) if status.success() => ActionOutcome::Success {
            affected_count: files.len(),
            details: format!("Opened in {editor}"),
        },
        Ok(status) => ActionOutcome::Failed(format!(
            "Editor '{editor}' exited with status: {:?}",
            status.code()
        )),
        Err(e) => ActionOutcome::Failed(format!("Failed to launch editor '{editor}': {e}")),
    }
}

/// Execute path copying to clipboard (pure business logic)
///
/// Copies file paths to the system clipboard. Returns the formatted paths
/// string for fallback display if clipboard is unavailable.
///
/// # Arguments
/// * `files` - Files whose paths to copy
///
/// # Returns
/// `ActionOutcome` with paths string, or error
///
/// # Errors
/// Returns error string if clipboard operations fail
pub fn execute_copy_path(files: &[PathBuf]) -> Result<ActionOutcome, String> {
    if files.is_empty() {
        return Ok(ActionOutcome::Failed("No files specified".to_string()));
    }

    let paths_text = files
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join("\n");

    match arboard::Clipboard::new() {
        Ok(mut clipboard) => match clipboard.set_text(&paths_text) {
            Ok(()) => Ok(ActionOutcome::Success {
                affected_count: files.len(),
                details: "Copied paths to clipboard".to_string(),
            }),
            Err(e) => Err(format!("Clipboard error: {e}")),
        },
        Err(e) => Err(format!("Clipboard unavailable: {e}")),
    }
}

/// Execute file copying to directory (pure business logic)
///
/// Copies files to the specified destination directory. Creates the directory
/// if `create_dest` is true.
///
/// # Arguments
/// * `files` - Files to copy
/// * `dest_dir` - Destination directory
/// * `create_dest` - Whether to create the destination if it doesn't exist
///
/// # Returns
/// `ActionOutcome` describing the result
#[must_use]
pub fn execute_copy_files(files: &[PathBuf], dest_dir: &Path, create_dest: bool) -> ActionOutcome {
    if files.is_empty() {
        return ActionOutcome::Failed("No files specified".to_string());
    }

    if !dest_dir.exists() {
        if create_dest {
            if let Err(e) = std::fs::create_dir_all(dest_dir) {
                return ActionOutcome::Failed(format!(
                    "Failed to create directory '{}': {}",
                    dest_dir.display(),
                    e
                ));
            }
        } else {
            return ActionOutcome::Failed(format!(
                "Destination directory '{}' does not exist",
                dest_dir.display()
            ));
        }
    }

    if !dest_dir.is_dir() {
        return ActionOutcome::Failed(format!("'{}' is not a directory", dest_dir.display()));
    }

    let mut copied = 0;
    let mut errors = Vec::new();

    for file in files {
        let Some(filename) = file.file_name() else {
            errors.push(format!("{}: invalid filename", file.display()));
            continue;
        };

        let dest_path = dest_dir.join(filename);

        match std::fs::copy(file, &dest_path) {
            Ok(_) => copied += 1,
            Err(e) => errors.push(format!("{}: {}", file.display(), e)),
        }
    }

    if errors.is_empty() {
        ActionOutcome::Success {
            affected_count: copied,
            details: format!("Copied to {}", dest_dir.display()),
        }
    } else if copied > 0 {
        ActionOutcome::Partial {
            succeeded: copied,
            failed: errors.len(),
            errors,
        }
    } else {
        ActionOutcome::Failed(format!("Failed to copy files:\n{}", errors.join("\n")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{TempFile, TestDb};

    #[test]
    fn test_execute_add_tag_success() {
        let db = TestDb::new("test_add_tag_success");
        let temp_file = TempFile::create("test.txt").unwrap();

        db.db()
            .insert(temp_file.path(), vec!["old".into()])
            .unwrap();

        let outcome = execute_add_tag(
            db.db(),
            &[temp_file.path().to_path_buf()],
            &["new".to_string(), "tags".to_string()],
        )
        .unwrap();

        assert!(matches!(outcome, ActionOutcome::Success { .. }));
        if let ActionOutcome::Success { affected_count, .. } = outcome {
            assert_eq!(affected_count, 1);
        }

        let tags = db.db().get_tags(temp_file.path()).unwrap().unwrap();
        assert!(tags.contains(&"new".to_string()));
        assert!(tags.contains(&"tags".to_string()));
        assert!(tags.contains(&"old".to_string()));
    }

    #[test]
    fn test_execute_add_tag_no_duplicates() {
        let db = TestDb::new("test_add_tag_no_dup");
        let temp_file = TempFile::create("test.txt").unwrap();

        db.db()
            .insert(temp_file.path(), vec!["existing".into()])
            .unwrap();

        let outcome = execute_add_tag(
            db.db(),
            &[temp_file.path().to_path_buf()],
            &["existing".to_string()],
        )
        .unwrap();

        // Should succeed but with 0 affected since tag already exists
        assert!(matches!(
            outcome,
            ActionOutcome::Success {
                affected_count: 0,
                ..
            }
        ));
    }

    #[test]
    fn test_execute_add_tag_empty_files() {
        let db = TestDb::new("test_add_tag_empty");

        let outcome = execute_add_tag(db.db(), &[], &["tag".to_string()]).unwrap();

        assert!(matches!(outcome, ActionOutcome::Failed(_)));
    }

    #[test]
    fn test_execute_remove_tag_success() {
        let db = TestDb::new("test_remove_tag_success");
        let temp_file = TempFile::create("test.txt").unwrap();

        db.db()
            .insert(
                temp_file.path(),
                vec!["keep".into(), "remove".into(), "also_remove".into()],
            )
            .unwrap();

        let outcome = execute_remove_tag(
            db.db(),
            &[temp_file.path().to_path_buf()],
            &["remove".to_string(), "also_remove".to_string()],
        )
        .unwrap();

        assert!(matches!(outcome, ActionOutcome::Success { .. }));
        if let ActionOutcome::Success { affected_count, .. } = outcome {
            assert_eq!(affected_count, 1);
        }

        let tags = db.db().get_tags(temp_file.path()).unwrap().unwrap();
        assert_eq!(tags.len(), 1);
        assert!(tags.contains(&"keep".to_string()));
    }

    #[test]
    fn test_execute_remove_tag_nonexistent() {
        let db = TestDb::new("test_remove_tag_nonexistent");
        let temp_file = TempFile::create("test.txt").unwrap();

        db.db()
            .insert(temp_file.path(), vec!["tag1".into()])
            .unwrap();

        let outcome = execute_remove_tag(
            db.db(),
            &[temp_file.path().to_path_buf()],
            &["nonexistent".to_string()],
        )
        .unwrap();

        assert!(matches!(outcome, ActionOutcome::Failed(_)));
    }

    #[test]
    fn test_execute_delete_from_db_success() {
        let db = TestDb::new("test_delete_success");
        let temp_file = TempFile::create("test.txt").unwrap();

        db.db()
            .insert(temp_file.path(), vec!["tag".into()])
            .unwrap();
        assert!(db.db().contains(temp_file.path()).unwrap());

        let outcome = execute_delete_from_db(db.db(), &[temp_file.path().to_path_buf()]).unwrap();

        assert!(matches!(outcome, ActionOutcome::Success { .. }));
        assert!(!db.db().contains(temp_file.path()).unwrap());
    }

    #[test]
    fn test_execute_delete_from_db_nonexistent() {
        let db = TestDb::new("test_delete_nonexistent");
        let fake_file = PathBuf::from("/nonexistent/file.txt");

        let outcome = execute_delete_from_db(db.db(), &[fake_file]).unwrap();

        assert!(matches!(outcome, ActionOutcome::Failed(_)));
    }

    #[test]
    fn test_execute_open_in_default_empty() {
        let outcome = execute_open_in_default(&[]);
        assert!(matches!(outcome, ActionOutcome::Failed(_)));
    }

    #[test]
    fn test_execute_open_in_editor_empty() {
        let outcome = execute_open_in_editor(&[], "vim");
        assert!(matches!(outcome, ActionOutcome::Failed(_)));
    }

    #[test]
    fn test_execute_copy_path_empty() {
        let outcome = execute_copy_path(&[]).unwrap();
        assert!(matches!(outcome, ActionOutcome::Failed(_)));
    }

    #[test]
    fn test_execute_copy_files_empty() {
        let dest = PathBuf::from("/tmp/dest");
        let outcome = execute_copy_files(&[], &dest, false);
        assert!(matches!(outcome, ActionOutcome::Failed(_)));
    }

    #[test]
    fn test_execute_copy_files_invalid_dest() {
        let temp_file = TempFile::create("test.txt").unwrap();
        let invalid_dest = temp_file.path(); // File, not directory

        let outcome = execute_copy_files(&[temp_file.path().to_path_buf()], invalid_dest, false);

        assert!(matches!(outcome, ActionOutcome::Failed(_)));
    }

    #[test]
    fn test_add_tags_to_file_no_change() {
        let db = TestDb::new("test_add_no_change");
        let temp_file = TempFile::create("test.txt").unwrap();

        db.db()
            .insert(temp_file.path(), vec!["tag1".into()])
            .unwrap();

        let changed = add_tags_to_file(db.db(), temp_file.path(), &["tag1".to_string()]).unwrap();

        assert!(!changed);
    }

    #[test]
    fn test_remove_tags_from_file_no_tags() {
        let db = TestDb::new("test_remove_no_tags");
        let temp_file = TempFile::create("test.txt").unwrap();

        db.db().insert(temp_file.path(), vec![]).unwrap();

        let changed =
            remove_tags_from_file(db.db(), temp_file.path(), &["tag1".to_string()]).unwrap();

        assert!(!changed);
    }

    #[test]
    fn test_multiple_files_partial_success() {
        let db = TestDb::new("test_multi_partial");
        let file1 = TempFile::create("file1.txt").unwrap();
        let file2 = TempFile::create("file2.txt").unwrap();
        let fake_file = PathBuf::from("/nonexistent/file.txt");

        db.db().insert(file1.path(), vec!["tag1".into()]).unwrap();
        db.db().insert(file2.path(), vec!["tag2".into()]).unwrap();

        let files = vec![
            file1.path().to_path_buf(),
            file2.path().to_path_buf(),
            fake_file,
        ];

        let outcome = execute_add_tag(db.db(), &files, &["new".to_string()]).unwrap();

        assert!(matches!(outcome, ActionOutcome::Partial { .. }));
        if let ActionOutcome::Partial {
            succeeded, failed, ..
        } = outcome
        {
            assert_eq!(succeeded, 2);
            assert_eq!(failed, 1);
        }
    }
}
