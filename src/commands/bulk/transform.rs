use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use colored::Colorize;
use dialoguer::Confirm;
use heck::{ToKebabCase, ToLowerCamelCase, ToPascalCase, ToSnakeCase};
use regex::Regex;

use super::core::BulkOpSummary;
use crate::db::Database;
use crate::{Pair, TagrError};

type Result<T> = std::result::Result<T, TagrError>;

/// Tag transformation type
#[derive(Debug, Clone)]
pub enum TagTransformation {
    Lowercase,
    Uppercase,
    KebabCase,
    SnakeCase,
    CamelCase,
    PascalCase,
    AddPrefix(String),
    AddSuffix(String),
    RemovePrefix(String),
    RemoveSuffix(String),
    RegexReplace { pattern: String, replacement: String },
}

impl TagTransformation {
    /// Apply transformation to a tag
    fn apply(&self, tag: &str) -> Result<String> {
        Ok(match self {
            Self::Lowercase => tag.to_lowercase(),
            Self::Uppercase => tag.to_uppercase(),
            Self::KebabCase => tag.to_kebab_case(),
            Self::SnakeCase => tag.to_snake_case(),
            Self::CamelCase => tag.to_lower_camel_case(),
            Self::PascalCase => tag.to_pascal_case(),
            Self::AddPrefix(prefix) => format!("{prefix}{tag}"),
            Self::AddSuffix(suffix) => format!("{tag}{suffix}"),
            Self::RemovePrefix(prefix) => {
                tag.strip_prefix(prefix).unwrap_or(tag).to_string()
            }
            Self::RemoveSuffix(suffix) => {
                tag.strip_suffix(suffix).unwrap_or(tag).to_string()
            }
            Self::RegexReplace { pattern, replacement } => {
                let re = Regex::new(pattern).map_err(|e| {
                    TagrError::InvalidInput(format!("Invalid regex pattern '{pattern}': {e}"))
                })?;
                re.replace_all(tag, replacement.as_str()).to_string()
            }
        })
    }
}

/// Transform tags across all files in the database.
///
/// # Arguments
/// * `db` - Database instance
/// * `transformation` - Transformation to apply
/// * `filter_tags` - Only transform specific tags (None = all tags)
/// * `dry_run` - Preview changes without applying
/// * `yes` - Skip confirmation prompt
/// * `quiet` - Suppress output
///
/// # Errors
/// Returns database errors during file queries and updates, and `TagrError::InvalidInput`
/// for invalid regex patterns.
pub fn transform_tags(
    db: &Database,
    transformation: &TagTransformation,
    filter_tags: Option<&[String]>,
    dry_run: bool,
    yes: bool,
    quiet: bool,
) -> Result<()> {
    // Collect all unique tags from database
    let all_pairs = db.list_all()?;
    let mut all_tags: HashSet<String> = HashSet::new();
    for pair in &all_pairs {
        all_tags.extend(pair.tags.iter().cloned());
    }

    // Filter tags if specified
    let tags_to_transform: Vec<String> = if let Some(filter) = filter_tags {
        all_tags
            .into_iter()
            .filter(|t| filter.contains(t))
            .collect()
    } else {
        all_tags.into_iter().collect()
    };

    if tags_to_transform.is_empty() {
        if !quiet {
            println!("No tags found to transform.");
        }
        return Ok(());
    }

    // Build transformation mapping
    let mut tag_mapping: HashMap<String, String> = HashMap::new();
    let mut conflicts: HashMap<String, Vec<String>> = HashMap::new();

    for old_tag in &tags_to_transform {
        let new_tag = transformation.apply(old_tag)?;
        
        // Check for collisions
        if new_tag != *old_tag {
            if let Some(existing_old) = tag_mapping
                .iter()
                .find(|(_, v)| *v == &new_tag)
                .map(|(k, _)| k.clone())
            {
                conflicts
                    .entry(new_tag.clone())
                    .or_default()
                    .push(old_tag.clone());
                if &existing_old != old_tag {
                    conflicts.get_mut(&new_tag).unwrap().push(existing_old);
                }
            } else {
                tag_mapping.insert(old_tag.clone(), new_tag);
            }
        }
    }

    if tag_mapping.is_empty() {
        if !quiet {
            println!("No transformations to apply (all tags unchanged).");
        }
        return Ok(());
    }

    // Show conflicts if any
    if !conflicts.is_empty() && !quiet {
        println!("{}", "Warning: Tag collisions detected:".yellow().bold());
        for (new_tag, old_tags) in &conflicts {
            println!("  {} ← {}", new_tag.cyan(), old_tags.join(", "));
        }
        println!();
    }

    // Find affected files
    let mut affected_files: HashSet<PathBuf> = HashSet::new();
    for pair in &all_pairs {
        if pair.tags.iter().any(|t| tag_mapping.contains_key(t)) {
            affected_files.insert(pair.file.clone());
        }
    }

    if dry_run {
        println!("{}", "=== Dry Run Mode ===".yellow().bold());
        println!(
            "Would transform {} tag(s) affecting {} file(s)",
            tag_mapping.len(),
            affected_files.len()
        );
        println!("\n{}", "Tag transformations:".bold());
        let mut mappings: Vec<_> = tag_mapping.iter().collect();
        mappings.sort_by_key(|(old, _)| old.as_str());
        for (i, (old_tag, new_tag)) in mappings.iter().enumerate().take(20) {
            println!("  {}. {} → {}", i + 1, old_tag, new_tag.cyan());
        }
        if tag_mapping.len() > 20 {
            println!("  ... and {} more", tag_mapping.len() - 20);
        }
        println!("\n{}", "Run without --dry-run to apply changes.".yellow());
        return Ok(());
    }

    if !yes {
        let prompt = format!(
            "Transform {} tag(s) in {} file(s)?",
            tag_mapping.len(),
            affected_files.len()
        );
        let confirmed = Confirm::new()
            .with_prompt(prompt)
            .interact()
            .map_err(|e| TagrError::InvalidInput(format!("Failed to get confirmation: {e}")))?;
        if !confirmed {
            println!("Operation cancelled.");
            return Ok(());
        }
    }

    let mut summary = BulkOpSummary::new();

    for pair in all_pairs {
        let has_affected_tags = pair.tags.iter().any(|t| tag_mapping.contains_key(t));
        if !has_affected_tags {
            continue;
        }

        let new_tags: Vec<String> = pair
            .tags
            .iter()
            .map(|t| tag_mapping.get(t).cloned().unwrap_or_else(|| t.clone()))
            .collect::<HashSet<_>>() // Deduplicate in case of merges
            .into_iter()
            .collect();

        let new_pair = Pair {
            file: pair.file.clone(),
            tags: new_tags,
        };

        match db.insert_pair(&new_pair) {
            Ok(()) => {
                summary.add_success();
                if !quiet {
                    println!("✓ Transformed tags in: {}", pair.file.display());
                }
            }
            Err(e) => {
                summary.add_error(format!("{}: {}", pair.file.display(), e));
                if !quiet {
                    eprintln!("✗ Failed to transform {}: {}", pair.file.display(), e);
                }
            }
        }
    }

    if !quiet {
        summary.print("Transform Tags");
    }

    Ok(())
}
