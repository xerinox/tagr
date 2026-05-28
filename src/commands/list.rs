//! List command - list files or tags in the database

use crate::{TagrError, cli::ListVariant, config, output};
use crate::store::TagStore;
use std::io::Write;

type Result<T> = std::result::Result<T, TagrError>;

/// Execute the list command
///
/// # Errors
/// Returns an error if database operations fail
pub fn execute(
    store: &dyn TagStore,
    variant: ListVariant,
    path_format: config::PathFormat,
    quiet: bool,
    writer: &mut impl Write,
) -> Result<()> {
    match variant {
        ListVariant::Files => list_files(store, path_format, quiet, writer),
        ListVariant::Tags => list_tags(store, quiet, writer),
    }
}

fn list_files(store: &dyn TagStore, path_format: config::PathFormat, quiet: bool, writer: &mut impl Write) -> Result<()> {
    let all_pairs = store.list_all()?;

    if all_pairs.is_empty() {
        if !quiet {
            writeln!(writer, "No files found in database.")?;
        }
    } else {
        if !quiet {
            writeln!(writer, "Files in database:")?;
        }
        for pair in all_pairs {
            writeln!(
                writer,
                "{}",
                output::file_with_tags(&pair.file, &pair.tags, path_format, quiet)
            )?;
        }
    }
    Ok(())
}

fn list_tags(store: &dyn TagStore, quiet: bool, writer: &mut impl Write) -> Result<()> {
    let tags = store.list_all_tags()?;

    if tags.is_empty() {
        if !quiet {
            writeln!(writer, "No tags found in database.")?;
        }
    } else {
        if !quiet {
            writeln!(writer, "Tags in database:")?;
        }
        for tag in &tags {
            let count = store.find_by_tag(tag)?.len();
            writeln!(writer, "{}", output::tag_with_count(tag.as_str(), count, quiet))?;
        }
    }
    Ok(())
}
