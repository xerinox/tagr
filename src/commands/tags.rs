//! Tags command - global tag management

use crate::{TagrError, cli::TagsCommands, db::Database, output};
use dialoguer::Confirm;
use std::collections::{HashMap, HashSet};

type Result<T> = std::result::Result<T, TagrError>;

/// Execute the tags management command
///
/// # Errors
/// Returns an error if database operations fail or user interaction fails
pub fn execute(db: &Database, command: &TagsCommands, quiet: bool) -> Result<()> {
    match command {
        TagsCommands::List { tree } => list_all_tags(db, *tree, quiet),
        TagsCommands::Remove { tag } => remove_tag_globally(db, tag, quiet),
    }
}

fn list_all_tags(db: &Database, tree: bool, quiet: bool) -> Result<()> {
    let tags = db.list_all_tags()?;

    if tags.is_empty() {
        if !quiet {
            println!("No tags found in database.");
        }
        return Ok(());
    }

    if tree {
        display_tree_view(db, &tags, quiet)
    } else {
        display_flat_list(db, &tags, quiet)
    }
}

fn display_flat_list(db: &Database, tags: &[String], quiet: bool) -> Result<()> {
    if !quiet {
        println!("Tags in database:");
    }
    for tag in tags {
        let count = db.find_by_tag(tag)?.len();
        println!("{}", output::tag_with_count(tag, count, quiet));
    }
    Ok(())
}

fn display_tree_view(db: &Database, tags: &[String], quiet: bool) -> Result<()> {
    use crate::schema::HIERARCHY_DELIMITER;

    // Separate hierarchical tags from flat tags
    let mut hierarchy: HashMap<String, Vec<String>> = HashMap::new();
    let mut root_tags: HashSet<String> = HashSet::new();

    for tag in tags {
        if tag.contains(HIERARCHY_DELIMITER) {
            // Extract parent from hierarchical tag (e.g., "lang:rust" -> "lang")
            if let Some(parent) = tag
                .rsplit_once(HIERARCHY_DELIMITER)
                .map(|(p, _)| p.to_string())
            {
                hierarchy
                    .entry(parent.clone())
                    .or_default()
                    .push(tag.clone());
                root_tags.insert(extract_root(tag));
            }
        } else {
            // Flat tag
            root_tags.insert(tag.clone());
        }
    }

    if !quiet {
        println!("Tags in database (tree view):");
    }

    // Sort root tags for consistent output
    let mut sorted_roots: Vec<_> = root_tags.into_iter().collect();
    sorted_roots.sort();

    for root in sorted_roots {
        let count = db.find_by_tag(&root)?.len();
        println!("{}", output::tag_with_count(&root, count, quiet));
        print_children(db, &root, &hierarchy, quiet)?;
    }

    Ok(())
}

fn print_children(
    db: &Database,
    parent: &str,
    hierarchy: &HashMap<String, Vec<String>>,
    quiet: bool,
) -> Result<()> {
    use crate::schema::HIERARCHY_DELIMITER;

    // Find all direct children of this parent using the pre-computed hierarchy map
    // This is O(1) lookup effectively, avoiding O(N) scan of all tags
    if let Some(children) = hierarchy.get(parent) {
        let mut sorted_children = children.clone();
        sorted_children.sort();

        for (idx, child) in sorted_children.iter().enumerate() {
            let is_last = idx == sorted_children.len() - 1;
            let count = db.find_by_tag(child)?.len();

            // Calculate depth by counting delimiters
            // root (0 delimiters) -> depth 0 (but printed at top level)
            // root:child (1 delimiter) -> depth 1
            let depth = child.matches(HIERARCHY_DELIMITER).count();

            // Box drawing characters for tree visualization
            let prefix_str = if is_last { "└── " } else { "├── " };
            let indent = "    ".repeat(depth.saturating_sub(1));

            if quiet {
                println!("{}{}{}", indent, prefix_str, child);
            } else {
                println!("  {}{}{}  ({} file(s))", indent, prefix_str, child, count);
            }

            // Recursively print children of this child
            print_children(db, child, hierarchy, quiet)?;
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

fn remove_tag_globally(db: &Database, tag: &str, quiet: bool) -> Result<()> {
    let files_before = db.find_by_tag(tag)?;

    if files_before.is_empty() {
        if !quiet {
            println!("Tag '{tag}' not found in database.");
        }
        return Ok(());
    }

    if !quiet {
        println!("Found tag '{tag}' in {} file(s):", files_before.len());
        for file in &files_before {
            println!("  - {}", file.display());
        }
        println!();
    }

    if !confirm("Remove tag from all files?", quiet)? {
        if !quiet {
            println!("Cancelled.");
        }
        return Ok(());
    }

    let files_removed = db.remove_tag_globally(tag)?;

    if !quiet {
        println!("Removed tag '{tag}' from {} file(s).", files_before.len());
        if files_removed > 0 {
            println!("Cleaned up {files_removed} file(s) with no remaining tags.");
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
