//! Tag and untag commands

use crate::TagrError;
use crate::schema::load_default_schema;
use crate::store::TagStore;
use crate::types::{TagName, TagrPath};
use std::io::Write;
use std::path::PathBuf;

type Result<T> = std::result::Result<T, TagrError>;

/// Invalidate completion cache if any tag is new (not yet in database)
#[cfg(feature = "dynamic-completions")]
fn invalidate_cache_if_new_tags(store: &dyn TagStore, tags: &[TagName]) {
    let has_new_tag = tags
        .iter()
        .any(|tag| store.tag_exists(tag).unwrap_or(false) == false);

    if has_new_tag {
        crate::completions::invalidate_cache(store);
    }
}

/// Invalidate completion cache if any tag became orphaned (no files have it)
#[cfg(feature = "dynamic-completions")]
fn invalidate_cache_if_orphaned_tags(store: &dyn TagStore, tags: &[TagName]) {
    let has_orphaned_tag = tags
        .iter()
        .any(|tag| store.tag_exists(tag).unwrap_or(true) == false);

    if has_orphaned_tag {
        crate::completions::invalidate_cache(store);
    }
}

/// Execute the tag command - add tags to a file
///
/// # Errors
/// Returns an error if the file cannot be accessed or database operations fail
pub fn execute(
    store: &dyn TagStore,
    file: Option<PathBuf>,
    tags: &[String],
    no_canonicalize: bool,
    quiet: bool,
    writer: &mut impl Write,
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

    let canonical_path = TagrPath::new(&fullpath)?;

    // Canonicalize tags unless disabled
    let final_tag_strings = if no_canonicalize {
        tags.to_vec()
    } else {
        match load_default_schema() {
            Ok(schema) => tags.iter().map(|t| schema.canonicalize(t)).collect(),
            Err(e) => {
                if !quiet {
                    writeln!(
                        writer,
                        "Warning: Could not load schema ({e}), using tags as-is"
                    )?;
                }
                tags.to_vec()
            }
        }
    };

    // Convert to TagName at the boundary
    let final_tags: Vec<TagName> = final_tag_strings
        .iter()
        .map(TagName::new)
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let success_msg = if quiet {
        None
    } else {
        Some(format!(
            "Tagged {} with: {}",
            file_path.display(),
            final_tag_strings.join(", ")
        ))
    };

    #[cfg(feature = "dynamic-completions")]
    invalidate_cache_if_new_tags(store, &final_tags);

    store.add_tags(&canonical_path, final_tags)?;

    if let Some(msg) = success_msg {
        writeln!(writer, "{msg}")?;
    }

    Ok(())
}

/// Execute the untag command - remove tags from a file
///
/// # Errors
/// Returns an error if the file cannot be accessed or database operations fail
pub fn untag(
    store: &dyn TagStore,
    file: Option<PathBuf>,
    tags: &[String],
    all: bool,
    quiet: bool,
    writer: &mut impl Write,
) -> Result<()> {
    let file_path = file.ok_or_else(|| TagrError::InvalidInput("No file provided".into()))?;

    let fullpath = file_path.canonicalize().map_err(|e| {
        TagrError::InvalidInput(format!(
            "Cannot access path '{}': {}",
            file_path.display(),
            e
        ))
    })?;

    let canonical_path = TagrPath::new(&fullpath)?;

    if all {
        #[cfg(feature = "dynamic-completions")]
        let old_tags = store
            .get_tags(&canonical_path)
            .ok()
            .flatten()
            .unwrap_or_default();

        store.remove_file(&canonical_path)?;

        #[cfg(feature = "dynamic-completions")]
        invalidate_cache_if_orphaned_tags(store, &old_tags);

        if !quiet {
            writeln!(writer, "Removed all tags from {}", file_path.display())?;
        }
        return Ok(());
    }

    if tags.is_empty() {
        return Err(TagrError::InvalidInput(
            "No tags provided. Use -t to specify tags or --all to remove all tags".into(),
        ));
    }

    // Convert to TagName at the boundary
    let tag_names: Vec<TagName> = tags
        .iter()
        .map(TagName::new)
        .collect::<std::result::Result<Vec<_>, _>>()?;

    // Warn about tags that don't exist on this file
    if let Ok(Some(existing_tags)) = store.get_tags(&canonical_path) {
        for tag in &tag_names {
            if !existing_tags.iter().any(|t| t == tag) {
                eprintln!(
                    "warning: file '{}' does not have tag '{}'",
                    file_path.display(),
                    tag.as_str()
                );
            }
        }
    }

    store.remove_tags(&canonical_path, &tag_names)?;

    #[cfg(feature = "dynamic-completions")]
    invalidate_cache_if_orphaned_tags(store, &tag_names);

    if !quiet {
        writeln!(
            writer,
            "Removed tags {} from {}",
            tags.join(", "),
            file_path.display()
        )?;
    }

    Ok(())
}
