//! List command - list files or tags in the database

use crate::store::TagStore;
use crate::{TagrError, cli::ListVariant, config, output};
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
    json: bool,
    writer: &mut impl Write,
) -> Result<()> {
    if json {
        match variant {
            ListVariant::Files => list_files_json(store, path_format, writer),
            ListVariant::Tags => list_tags_json(store, writer),
        }
    } else {
        match variant {
            ListVariant::Files => list_files(store, path_format, quiet, writer),
            ListVariant::Tags => list_tags(store, quiet, writer),
        }
    }
}

#[derive(serde::Serialize)]
struct JsonPair {
    file: String,
    tags: Vec<String>,
}

#[derive(serde::Serialize)]
struct JsonTag {
    name: String,
    file_count: usize,
}

fn list_files_json(
    store: &dyn TagStore,
    path_format: config::PathFormat,
    writer: &mut impl Write,
) -> Result<()> {
    let all_pairs = store.list_all()?;
    let json_pairs: Vec<JsonPair> = all_pairs
        .iter()
        .map(|pair| JsonPair {
            file: output::format_path(&pair.file, path_format),
            tags: pair.tags.iter().map(|t| t.as_str().to_string()).collect(),
        })
        .collect();

    let json_str = serde_json::to_string_pretty(&json_pairs)
        .map_err(|e| TagrError::InvalidInput(format!("Failed to serialize JSON: {e}")))?;
    writeln!(writer, "{json_str}")?;
    Ok(())
}

fn list_tags_json(store: &dyn TagStore, writer: &mut impl Write) -> Result<()> {
    let tags = store.list_all_tags()?;
    let mut json_tags = Vec::new();
    for tag in &tags {
        let count = store.find_by_tag(tag)?.len();
        json_tags.push(JsonTag {
            name: tag.as_str().to_string(),
            file_count: count,
        });
    }

    let json_str = serde_json::to_string_pretty(&json_tags)
        .map_err(|e| TagrError::InvalidInput(format!("Failed to serialize JSON: {e}")))?;
    writeln!(writer, "{json_str}")?;
    Ok(())
}

fn list_files(
    store: &dyn TagStore,
    path_format: config::PathFormat,
    quiet: bool,
    writer: &mut impl Write,
) -> Result<()> {
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
            writeln!(
                writer,
                "{}",
                output::tag_with_count(tag.as_str(), count, quiet)
            )?;
        }
    }
    Ok(())
}
