//! Tag and untag commands

use crate::{
    db::Database,
    TagrError,
};
use std::path::PathBuf;

type Result<T> = std::result::Result<T, TagrError>;

/// Execute the tag command - add tags to a file
///
/// # Errors
/// Returns an error if the file cannot be accessed or database operations fail
pub fn execute(
    db: &Database,
    file: Option<PathBuf>,
    tags: &[String],
    quiet: bool,
) -> Result<()> {
    if let Some(file_path) = file {
        if tags.is_empty() {
            return Err(TagrError::InvalidInput("No tags provided".into()));
        }
        let fullpath = file_path.canonicalize()
            .map_err(|e| TagrError::InvalidInput(format!("Cannot access path '{}': {}", file_path.display(), e)))?;
        db.add_tags(&fullpath, tags.to_vec())?;
        if !quiet {
            println!("Tagged {} with: {}", file_path.display(), tags.join(", "));
        }
    } else {
        return Err(TagrError::InvalidInput("No file provided".into()));
    }
    Ok(())
}

/// Execute the untag command - remove tags from a file
///
/// # Errors
/// Returns an error if the file cannot be accessed or database operations fail
pub fn untag(
    db: &Database,
    file: Option<PathBuf>,
    tags: &[String],
    all: bool,
    quiet: bool,
) -> Result<()> {
    if let Some(file_path) = file {
        let fullpath = file_path.canonicalize()
            .map_err(|e| TagrError::InvalidInput(format!("Cannot access path '{}': {}", file_path.display(), e)))?;
        if all {
            db.remove(&fullpath)?;
            if !quiet {
                println!("Removed all tags from {}", file_path.display());
            }
        } else if !tags.is_empty() {
            db.remove_tags(&fullpath, tags)?;
            if !quiet {
                println!("Removed tags {} from {}", tags.join(", "), file_path.display());
            }
        } else {
            return Err(TagrError::InvalidInput("No tags provided. Use -t to specify tags or --all to remove all tags".into()));
        }
    } else {
        return Err(TagrError::InvalidInput("No file provided".into()));
    }
    Ok(())
}
