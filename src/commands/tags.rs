//! Tags command - global tag management

use crate::store::TagStore;
use crate::types::TagName;
use crate::{TagrError, cli::TagsCommands, output};
use dialoguer::Confirm;
use std::collections::{HashMap, HashSet};
use std::io::Write;

type Result<T> = std::result::Result<T, TagrError>;

/// Execute the tags management command
///
/// # Errors
/// Returns an error if database operations fail or user interaction fails
pub fn execute(
    store: &dyn TagStore,
    command: &TagsCommands,
    quiet: bool,
    writer: &mut impl Write,
) -> Result<()> {
    match command {
        TagsCommands::List { tree } => list_all_tags(store, *tree, quiet, writer),
        TagsCommands::Remove { tag } => remove_tag_globally(store, tag, quiet, writer),
    }
}

fn list_all_tags(
    store: &dyn TagStore,
    tree: bool,
    quiet: bool,
    writer: &mut impl Write,
) -> Result<()> {
    let tags = store.list_all_tags()?;

    if tags.is_empty() {
        if !quiet {
            writeln!(writer, "No tags found in database.")?;
        }
        return Ok(());
    }

    if tree {
        display_tree_view(store, &tags, quiet, writer)
    } else {
        display_flat_list(store, &tags, quiet, writer)
    }
}

fn display_flat_list(
    store: &dyn TagStore,
    tags: &[TagName],
    quiet: bool,
    writer: &mut impl Write,
) -> Result<()> {
    if !quiet {
        writeln!(writer, "Tags in database:")?;
    }
    for tag in tags {
        let count = store.find_by_tag(tag)?.len();
        writeln!(
            writer,
            "{}",
            output::tag_with_count(tag.as_str(), count, quiet)
        )?;
    }
    Ok(())
}

fn display_tree_view(
    store: &dyn TagStore,
    tags: &[TagName],
    quiet: bool,
    writer: &mut impl Write,
) -> Result<()> {
    use crate::schema::HIERARCHY_DELIMITER;

    let mut hierarchy: HashMap<String, Vec<TagName>> = HashMap::new();
    let mut root_tags: HashSet<String> = HashSet::new();

    for tag in tags {
        let tag_str = tag.as_str();
        if tag_str.contains(HIERARCHY_DELIMITER) {
            if let Some(parent) = tag_str
                .rsplit_once(HIERARCHY_DELIMITER)
                .map(|(p, _)| p.to_string())
            {
                hierarchy
                    .entry(parent.clone())
                    .or_default()
                    .push(tag.clone());
                root_tags.insert(extract_root(tag_str));
            }
        } else {
            root_tags.insert(tag_str.to_string());
        }
    }

    if !quiet {
        writeln!(writer, "Tags in database (tree view):")?;
    }

    let mut sorted_roots: Vec<_> = root_tags.into_iter().collect();
    sorted_roots.sort();

    for root in &sorted_roots {
        // Root tags may or may not exist as actual tags in the database
        let count = TagName::new(root)
            .ok()
            .map(|tn| store.find_by_tag(&tn).map(|f| f.len()))
            .transpose()?
            .unwrap_or(0);
        writeln!(writer, "{}", output::tag_with_count(root, count, quiet))?;
        print_children(store, root, &hierarchy, quiet, writer)?;
    }

    Ok(())
}

fn print_children(
    store: &dyn TagStore,
    parent: &str,
    hierarchy: &HashMap<String, Vec<TagName>>,
    quiet: bool,
    writer: &mut impl Write,
) -> Result<()> {
    use crate::schema::HIERARCHY_DELIMITER;

    if let Some(children) = hierarchy.get(parent) {
        let mut sorted_children = children.clone();
        sorted_children.sort();

        for (idx, child) in sorted_children.iter().enumerate() {
            let is_last = idx == sorted_children.len() - 1;
            let count = store.find_by_tag(child)?.len();
            let child_str = child.as_str();

            let depth = child_str.matches(HIERARCHY_DELIMITER).count();

            let prefix_str = if is_last { "└── " } else { "├── " };
            let indent = "    ".repeat(depth.saturating_sub(1));

            if quiet {
                writeln!(writer, "{indent}{prefix_str}{child_str}")?;
            } else {
                writeln!(
                    writer,
                    "  {indent}{prefix_str}{child_str}  ({count} file(s))"
                )?;
            }

            print_children(store, child_str, hierarchy, quiet, writer)?;
        }
    }

    Ok(())
}

fn extract_root(tag: &str) -> String {
    use crate::schema::HIERARCHY_DELIMITER;
    tag.split(HIERARCHY_DELIMITER)
        .next()
        .expect("split always returns at least one element")
        .to_string()
}

fn remove_tag_globally(
    store: &dyn TagStore,
    tag: &str,
    quiet: bool,
    writer: &mut impl Write,
) -> Result<()> {
    let tag_name = TagName::new(tag).map_err(|e| TagrError::InvalidInput(e.to_string()))?;
    let files_before = store.find_by_tag(&tag_name)?;

    if files_before.is_empty() {
        if !quiet {
            writeln!(writer, "Tag '{tag}' not found in database.")?;
        }
        return Ok(());
    }

    if !quiet {
        writeln!(
            writer,
            "Found tag '{tag}' in {} file(s):",
            files_before.len()
        )?;
        for file in &files_before {
            writeln!(writer, "  - {file}")?;
        }
        writeln!(writer)?;
    }

    if !confirm("Remove tag from all files?", quiet)? {
        if !quiet {
            writeln!(writer, "Cancelled.")?;
        }
        return Ok(());
    }

    let files_removed = store.remove_tag_globally(&tag_name)?;

    #[cfg(feature = "dynamic-completions")]
    crate::completions::invalidate_cache(store);

    if !quiet {
        writeln!(
            writer,
            "Removed tag '{tag}' from {} file(s).",
            files_before.len()
        )?;
        if files_removed > 0 {
            writeln!(
                writer,
                "Cleaned up {files_removed} file(s) with no remaining tags."
            )?;
        }
    }
    Ok(())
}

/// Prompt user for yes/no confirmation using dialoguer
fn confirm(prompt: &str, quiet: bool) -> Result<bool> {
    if quiet {
        return Ok(true);
    }

    Confirm::new()
        .with_prompt(prompt)
        .interact()
        .map_err(|e| TagrError::InvalidInput(format!("Confirmation failed: {e}")))
}
