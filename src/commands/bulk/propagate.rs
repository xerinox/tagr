use std::collections::HashMap;
use std::path::{Path, PathBuf};

use colored::Colorize;
use dialoguer::Confirm;

use super::core::BulkOpSummary;
use crate::TagrError;
use crate::db::Database;

type Result<T> = std::result::Result<T, TagrError>;

/// Default extension to tag mappings
static DEFAULT_EXT_MAPPINGS: &[(&str, &[&str])] = &[
    ("rs", &["rust"]),
    ("py", &["python"]),
    ("js", &["javascript"]),
    ("ts", &["typescript"]),
    ("jsx", &["javascript", "react"]),
    ("tsx", &["typescript", "react"]),
    ("md", &["markdown"]),
    ("toml", &["toml"]),
    ("json", &["json"]),
    ("yml", &["yaml"]),
    ("yaml", &["yaml"]),
    ("txt", &["text"]),
    ("sh", &["shell"]),
    ("bash", &["shell"]),
    ("zsh", &["shell"]),
    ("fish", &["shell"]),
    ("c", &["c"]),
    ("h", &["c"]),
    ("cpp", &["cpp"]),
    ("cc", &["cpp"]),
    ("cxx", &["cpp"]),
    ("hpp", &["cpp"]),
    ("go", &["go"]),
    ("java", &["java"]),
    ("kt", &["kotlin"]),
    ("swift", &["swift"]),
    ("rb", &["ruby"]),
    ("php", &["php"]),
    ("html", &["html"]),
    ("css", &["css"]),
    ("scss", &["css", "scss"]),
    ("sass", &["css", "sass"]),
    ("sql", &["sql"]),
    ("xml", &["xml"]),
    ("pdf", &["pdf"]),
    ("png", &["image", "png"]),
    ("jpg", &["image", "jpg"]),
    ("jpeg", &["image", "jpeg"]),
    ("gif", &["image", "gif"]),
    ("svg", &["image", "svg"]),
    ("webp", &["image", "webp"]),
];

/// Parse a directory-to-tag mapping string in "dir:tag" format
fn parse_dir_mapping(s: &str) -> Result<(String, String)> {
    let (dir, tag) = s.split_once(':').ok_or_else(|| {
        TagrError::InvalidInput(format!("Invalid mapping format '{s}'. Expected 'dir:tag'"))
    })?;
    Ok((dir.to_string(), tag.to_string()))
}

/// Parse an extension-to-tags mapping string in "ext:tag1,tag2" format
fn parse_ext_mapping(s: &str) -> Result<(String, Vec<String>)> {
    let (ext, tags_str) = s.split_once(':').ok_or_else(|| {
        TagrError::InvalidInput(format!(
            "Invalid mapping format '{s}'. Expected 'ext:tag1,tag2'"
        ))
    })?;
    let tags: Vec<String> = tags_str.split(',').map(|t| t.trim().to_string()).collect();
    if tags.is_empty() {
        return Err(TagrError::InvalidInput(format!(
            "No tags provided for extension '{ext}'"
        )));
    }
    Ok((ext.to_string(), tags))
}

/// Auto-tag files based on their directory structure.
///
/// # Arguments
/// * `db` - Database instance
/// * `root` - Optional root directory to filter files (None = all files)
/// * `custom_mappings` - Custom directory to tag mappings in "dir:tag" format
/// * `hierarchy` - Add tags from all parent directories
/// * `dry_run` - Preview changes without applying
/// * `yes` - Skip confirmation prompt
/// * `quiet` - Suppress output
///
/// # Errors
/// Returns database errors during file queries and updates, and `TagrError::InvalidInput`
/// for invalid mapping formats.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_lines)]
#[allow(clippy::fn_params_excessive_bools)]
pub fn propagate_by_directory(
    db: &Database,
    root: Option<&Path>,
    custom_mappings: &[String],
    hierarchy: bool,
    dry_run: bool,
    yes: bool,
    quiet: bool,
) -> Result<()> {
    // Parse custom mappings
    let custom_map: HashMap<String, String> = custom_mappings
        .iter()
        .map(|s| parse_dir_mapping(s))
        .collect::<Result<HashMap<_, _>>>()?;

    // Get all files from database
    let all_files: Vec<PathBuf> = db.list_all()?.into_iter().map(|p| p.file).collect();

    // Filter by root if specified
    let files: Vec<PathBuf> = if let Some(root_path) = root {
        all_files
            .into_iter()
            .filter(|f| f.starts_with(root_path))
            .collect()
    } else {
        all_files
    };

    if files.is_empty() {
        if !quiet {
            println!("No files found in database.");
        }
        return Ok(());
    }

    // Build file -> tags mapping
    let mut file_tags: HashMap<PathBuf, Vec<String>> = HashMap::new();

    for file in &files {
        let mut tags_to_add = Vec::new();

        if hierarchy {
            // Add tags from all parent directories
            let mut current = file.parent();
            while let Some(dir) = current {
                if let Some(dir_name) = dir.file_name().and_then(|n| n.to_str()) {
                    // Check custom mappings first
                    if let Some(tag) = custom_map.get(dir_name) {
                        tags_to_add.push(tag.clone());
                    } else {
                        // Use directory name as tag
                        tags_to_add.push(dir_name.to_string());
                    }
                }
                current = dir.parent();
            }
        } else {
            // Only add tag from immediate parent directory
            if let Some(parent) = file.parent()
                && let Some(dir_name) = parent.file_name().and_then(|n| n.to_str())
            {
                // Check custom mappings first
                if let Some(tag) = custom_map.get(dir_name) {
                    tags_to_add.push(tag.clone());
                } else {
                    // Use directory name as tag
                    tags_to_add.push(dir_name.to_string());
                }
            }
        }

        if !tags_to_add.is_empty() {
            file_tags.insert(file.clone(), tags_to_add);
        }
    }

    if file_tags.is_empty() {
        if !quiet {
            println!("No tags to apply.");
        }
        return Ok(());
    }

    if dry_run {
        println!("{}", "=== Dry Run Mode ===".yellow().bold());
        println!(
            "Would apply directory-based tags to {} file(s)",
            file_tags.len()
        );
        println!("\n{}", "Sample changes (up to 10):".bold());
        for (i, (file, tags)) in file_tags.iter().enumerate().take(10) {
            println!(
                "  {}. {} → [{}]",
                i + 1,
                file.display(),
                tags.join(", ").cyan()
            );
        }
        if file_tags.len() > 10 {
            println!("  ... and {} more", file_tags.len() - 10);
        }
        println!("\n{}", "Run without --dry-run to apply changes.".yellow());
        return Ok(());
    }

    if !yes {
        let prompt = format!("Apply directory-based tags to {} file(s)?", file_tags.len());
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

    for (file, tags) in &file_tags {
        match db.add_tags(file, tags.clone()) {
            Ok(()) => {
                summary.add_success();
                if !quiet {
                    println!("✓ Tagged {}: [{}]", file.display(), tags.join(", "));
                }
            }
            Err(e) => {
                summary.add_error(format!("{}: {}", file.display(), e));
                if !quiet {
                    eprintln!("✗ Failed to tag {}: {}", file.display(), e);
                }
            }
        }
    }

    if !quiet {
        summary.print("Propagate by Directory");
    }

    Ok(())
}

/// Auto-tag files based on their file extension.
///
/// # Arguments
/// * `db` - Database instance
/// * `custom_mappings` - Custom extension to tags mappings in "ext:tag1,tag2" format
/// * `no_defaults` - Use only custom mappings, ignore defaults
/// * `dry_run` - Preview changes without applying
/// * `yes` - Skip confirmation prompt
/// * `quiet` - Suppress output
///
/// # Errors
/// Returns database errors during file queries and updates, and `TagrError::InvalidInput`
/// for invalid mapping formats.
#[allow(clippy::fn_params_excessive_bools)]
pub fn propagate_by_extension(
    db: &Database,
    custom_mappings: &[String],
    no_defaults: bool,
    dry_run: bool,
    yes: bool,
    quiet: bool,
) -> Result<()> {
    // Build extension map
    let mut ext_map: HashMap<String, Vec<String>> = HashMap::new();

    // Add default mappings unless disabled
    if !no_defaults {
        for (ext, tags) in DEFAULT_EXT_MAPPINGS {
            ext_map.insert(
                ext.to_string(),
                tags.iter().map(std::string::ToString::to_string).collect(),
            );
        }
    }

    // Parse and add custom mappings (these override defaults)
    for mapping in custom_mappings {
        let (ext, tags) = parse_ext_mapping(mapping)?;
        ext_map.insert(ext, tags);
    }

    if ext_map.is_empty() {
        return Err(TagrError::InvalidInput(
            "No extension mappings available. Either use default mappings or provide custom ones."
                .into(),
        ));
    }

    // Get all files from database
    let all_files: Vec<PathBuf> = db.list_all()?.into_iter().map(|p| p.file).collect();

    // Build file -> tags mapping
    let mut file_tags: HashMap<PathBuf, Vec<String>> = HashMap::new();

    for file in &all_files {
        if let Some(ext_os) = file.extension()
            && let Some(ext_str) = ext_os.to_str()
        {
            let ext_lower = ext_str.to_lowercase();
            if let Some(tags) = ext_map.get(&ext_lower) {
                file_tags.insert(file.clone(), tags.clone());
            }
        }
    }

    if file_tags.is_empty() {
        if !quiet {
            println!("No files match any extension mappings.");
        }
        return Ok(());
    }

    if dry_run {
        println!("{}", "=== Dry Run Mode ===".yellow().bold());
        println!(
            "Would apply extension-based tags to {} file(s)",
            file_tags.len()
        );
        println!("\n{}", "Sample changes (up to 10):".bold());
        for (i, (file, tags)) in file_tags.iter().enumerate().take(10) {
            println!(
                "  {}. {} → [{}]",
                i + 1,
                file.display(),
                tags.join(", ").cyan()
            );
        }
        if file_tags.len() > 10 {
            println!("  ... and {} more", file_tags.len() - 10);
        }
        println!("\n{}", "Run without --dry-run to apply changes.".yellow());
        return Ok(());
    }

    if !yes {
        let prompt = format!("Apply extension-based tags to {} file(s)?", file_tags.len());
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

    for (file, tags) in &file_tags {
        match db.add_tags(file, tags.clone()) {
            Ok(()) => {
                summary.add_success();
                if !quiet {
                    println!("✓ Tagged {}: [{}]", file.display(), tags.join(", "));
                }
            }
            Err(e) => {
                summary.add_error(format!("{}: {}", file.display(), e));
                if !quiet {
                    eprintln!("✗ Failed to tag {}: {}", file.display(), e);
                }
            }
        }
    }

    if !quiet {
        summary.print("Propagate by Extension");
    }

    Ok(())
}
