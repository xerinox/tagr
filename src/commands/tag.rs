//! Tag and untag commands

use crate::schema::load_default_schema;
use crate::{TagrError, db::Database};
use std::path::PathBuf;

type Result<T> = std::result::Result<T, TagrError>;

/// Invalidate completion cache if any tag is new (not yet in database)
#[cfg(feature = "dynamic-completions")]
fn invalidate_cache_if_new_tags(db: &Database, tags: &[String]) {
    // Check if any tag is new (doesn't exist yet)
    let has_new_tag = tags
        .iter()
        .any(|tag| db.tag_exists(tag).unwrap_or(false) == false);

    if has_new_tag {
        crate::completions::invalidate_cache(db);
    }
}

/// Invalidate completion cache if any tag became orphaned (no files have it)
#[cfg(feature = "dynamic-completions")]
fn invalidate_cache_if_orphaned_tags(db: &Database, tags: &[String]) {
    // Check if any tag is now orphaned (no longer exists)
    let has_orphaned_tag = tags
        .iter()
        .any(|tag| db.tag_exists(tag).unwrap_or(true) == false);

    if has_orphaned_tag {
        crate::completions::invalidate_cache(db);
    }
}

/// Execute the tag command - add tags to a file
///
/// # Errors
/// Returns an error if the file cannot be accessed or database operations fail
pub fn execute(
    db: &Database,
    file: Option<PathBuf>,
    tags: &[String],
    no_canonicalize: bool,
    quiet: bool,
) -> Result<()> {
    let file_path = file.ok_or_else(|| TagrError::InvalidInput("No file provided".into()))?;

    if tags.is_empty() {
        return Err(TagrError::InvalidInput("No tags provided".into()));
    }

    let fullpath = file_path.canonicalize().map_err(|e| {
        TagrError::InvalidInput(format!(
            "Cannot access path '{}': {}",
            file_path.display(),
            e
        ))
    })?;

    // Canonicalize tags unless disabled
    let final_tags = if no_canonicalize {
        tags.to_vec()
    } else {
        // Load schema and canonicalize each tag
        match load_default_schema() {
            Ok(schema) => tags.iter().map(|t| schema.canonicalize(t)).collect(),
            Err(e) => {
                // If schema can't be loaded, warn but continue with original tags
                if !quiet {
                    eprintln!("Warning: Could not load schema ({e}), using tags as-is");
                }
                tags.to_vec()
            }
        }
    };

    let success_msg = if quiet {
        None
    } else {
        Some(format!(
            "Tagged {} with: {}",
            file_path.display(),
            final_tags.join(", ")
        ))
    };

    // Check for new tags before adding (for cache invalidation)
    #[cfg(feature = "dynamic-completions")]
    invalidate_cache_if_new_tags(db, &final_tags);

    db.add_tags(&fullpath, final_tags)?;

    if let Some(msg) = success_msg {
        println!("{msg}");
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
    let file_path = file.ok_or_else(|| TagrError::InvalidInput("No file provided".into()))?;

    let fullpath = file_path.canonicalize().map_err(|e| {
        TagrError::InvalidInput(format!(
            "Cannot access path '{}': {}",
            file_path.display(),
            e
        ))
    })?;

    if all {
        // Get current tags before removing (for cache invalidation)
        #[cfg(feature = "dynamic-completions")]
        let old_tags = db.get_tags(&fullpath).ok().flatten().unwrap_or_default();

        db.remove(&fullpath)?;

        // Check for orphaned tags after removal
        #[cfg(feature = "dynamic-completions")]
        invalidate_cache_if_orphaned_tags(db, &old_tags);

        if !quiet {
            println!("Removed all tags from {}", file_path.display());
        }
        return Ok(());
    }

    if tags.is_empty() {
        return Err(TagrError::InvalidInput(
            "No tags provided. Use -t to specify tags or --all to remove all tags".into(),
        ));
    }

    db.remove_tags(&fullpath, tags)?;

    // Check for orphaned tags after removal
    #[cfg(feature = "dynamic-completions")]
    invalidate_cache_if_orphaned_tags(db, tags);

    if !quiet {
        println!(
            "Removed tags {} from {}",
            tags.join(", "),
            file_path.display()
        );
    }

    Ok(())
}
