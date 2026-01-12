use colored::Colorize;
use std::io::Write;

use crate::cli::AliasCommands;
use crate::db::Database;
use crate::schema::{SchemaError, load_default_schema};

/// Execute alias management commands
///
/// # Errors
/// Returns error if schema operations fail (I/O, validation, circular references)
pub fn execute_alias_command(
    command: &AliasCommands,
    db: Option<&Database>,
) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        AliasCommands::Add { alias, canonical } => {
            add_alias(alias, canonical)?;
            Ok(())
        }
        AliasCommands::Remove { alias } => {
            remove_alias(alias)?;
            Ok(())
        }
        AliasCommands::List => {
            list_aliases()?;
            Ok(())
        }
        AliasCommands::Show { tag } => {
            show_aliases(tag)?;
            Ok(())
        }
        AliasCommands::SetCanonical {
            alias,
            canonical,
            dry_run,
            yes,
        } => set_canonical(alias, canonical, *dry_run, *yes, db),
    }
}

/// Add a new alias
fn add_alias(alias: &str, canonical: &str) -> Result<(), SchemaError> {
    let mut schema = load_default_schema()?;

    schema.add_alias(alias, canonical)?;
    schema.save()?;

    println!(
        "{} Added alias: {} {} {}",
        "✓".green().bold(),
        alias.cyan(),
        "→".dimmed(),
        canonical.yellow()
    );

    Ok(())
}

/// Remove an alias
fn remove_alias(alias: &str) -> Result<(), SchemaError> {
    let mut schema = load_default_schema()?;

    // Get the canonical before removing (for display)
    let canonical = schema.canonicalize(alias);

    schema.remove_alias(alias)?;
    schema.save()?;

    println!(
        "{} Removed alias: {} {} {}",
        "✓".green().bold(),
        alias.cyan(),
        "→".dimmed(),
        canonical.yellow()
    );

    Ok(())
}

/// List all aliases
fn list_aliases() -> Result<(), SchemaError> {
    let schema = load_default_schema()?;
    let aliases = schema.list_aliases();

    if aliases.is_empty() {
        println!("{}", "No aliases defined".dimmed());
        return Ok(());
    }

    println!("{}", "Aliases:".bold());
    println!();

    // Find max alias length for alignment
    let max_alias_len = aliases
        .iter()
        .map(|(alias, _)| alias.len())
        .max()
        .unwrap_or(0);

    for (alias, canonical) in &aliases {
        println!(
            "  {:<width$} {} {}",
            alias.cyan(),
            "→".dimmed(),
            canonical.yellow(),
            width = max_alias_len
        );
    }

    println!();
    println!("{} aliases total", aliases.len().to_string().bold());

    Ok(())
}

/// Show aliases for a specific tag
fn show_aliases(tag: &str) -> Result<(), SchemaError> {
    let schema = load_default_schema()?;

    // Check if tag is an alias
    let canonical = schema.canonicalize(tag);
    let is_alias = canonical != tag;

    if is_alias {
        println!(
            "{} {} is an alias for {}",
            "ℹ".blue().bold(),
            tag.cyan(),
            canonical.yellow()
        );
    }

    // Get all aliases for the canonical tag
    let aliases = schema.get_aliases(&canonical);

    if aliases.is_empty() {
        if !is_alias {
            println!(
                "{} No aliases defined for {}",
                "ℹ".blue().bold(),
                tag.yellow()
            );
        }
        return Ok(());
    }

    println!();
    println!("{} for {}:", "Aliases".bold(), canonical.yellow());
    for alias in &aliases {
        println!("  • {}", alias.cyan());
    }

    // Show full synonym expansion
    let synonyms = schema.expand_synonyms(tag);
    if synonyms.len() > 1 {
        println!();
        println!("{}", "All synonyms:".bold());
        let mut sorted_synonyms = synonyms;
        sorted_synonyms.sort();
        for synonym in sorted_synonyms {
            if synonym == canonical {
                println!("  • {} {}", synonym.yellow(), "(canonical)".dimmed());
            } else {
                println!("  • {}", synonym.cyan());
            }
        }
    }

    Ok(())
}

/// Set canonical tag (swap alias and canonical, updating database)
#[allow(clippy::too_many_lines)]
fn set_canonical(
    alias: &str,
    canonical: &str,
    dry_run: bool,
    yes: bool,
    db: Option<&Database>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Load schema
    let schema = load_default_schema()?;

    // Validate: alias must exist and point to canonical
    let current_canonical = schema.canonicalize(alias);
    if current_canonical == alias {
        return Err(format!(
            "\"{alias}\" is not an alias. Use 'tagr alias add' to create an alias first."
        )
        .into());
    }

    if current_canonical != canonical {
        return Err(format!(
            "Alias \"{}\" points to \"{}\" not \"{}\". \
             Current mapping: {} → {}",
            alias,
            current_canonical,
            canonical,
            alias.cyan(),
            current_canonical.yellow()
        )
        .into());
    }

    // Get database
    let db = db.ok_or("Database required for set-canonical operation")?;

    // Check how many files would be affected
    let affected_files = db.find_by_tag(canonical)?;
    let file_count = affected_files.len();

    // Show what will happen
    println!("{}", "Swap canonical tag:".bold());
    println!();
    println!(
        "  Current:  {} {} {} (alias)",
        alias.cyan(),
        "→".dimmed(),
        canonical.yellow()
    );
    println!(
        "  New:      {} {} {} (alias)",
        canonical.cyan(),
        "→".dimmed(),
        alias.yellow()
    );
    println!();
    println!("{}", "Changes:".bold());
    println!(
        "  1. Remove alias: {} → {}",
        alias.cyan(),
        canonical.yellow()
    );
    println!(
        "  2. Rename all tags in database: {} → {}",
        canonical.yellow(),
        alias.cyan()
    );
    println!(
        "  3. Add new alias: {} → {}",
        canonical.cyan(),
        alias.yellow()
    );
    println!();
    println!(
        "Files affected: {}",
        if file_count == 0 {
            "none".dimmed().to_string()
        } else {
            file_count.to_string().yellow().to_string()
        }
    );

    if dry_run {
        println!();
        println!(
            "{} {}",
            "ℹ".blue().bold(),
            "Dry run - no changes made".dimmed()
        );
        return Ok(());
    }

    // Confirm unless --yes
    if !yes {
        println!();
        print!("{} ", "Proceed? [y/N]".bold());
        std::io::stdout().flush()?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let input = input.trim().to_lowercase();

        if input != "y" && input != "yes" {
            println!("{}", "Cancelled".dimmed());
            return Ok(());
        }
    }

    println!();

    // Step 1: Remove old alias
    let mut schema = load_default_schema()?;
    schema.remove_alias(alias)?;
    schema.save()?;
    println!(
        "{} Removed alias: {} → {}",
        "1/3".dimmed(),
        alias.cyan(),
        canonical.yellow()
    );

    // Step 2: Rename tags in database
    for file in &affected_files {
        // Convert PathBuf to str safely
        let Some(file_path) = file.to_str() else {
            eprintln!(
                "Warning: Skipping file with invalid UTF-8 path: {}",
                file.display()
            );
            continue;
        };

        if let Ok(Some(mut tags)) = db.get_tags(file_path) {
            // Replace canonical with alias
            if let Some(pos) = tags.iter().position(|t| t == canonical) {
                tags[pos] = alias.to_string();
                db.insert(file_path, tags)?;
            }
        }
    }
    println!(
        "{} Renamed tags: {} → {} ({} files)",
        "2/3".dimmed(),
        canonical.yellow(),
        alias.cyan(),
        file_count
    );

    // Step 3: Add new alias
    let mut schema = load_default_schema()?;
    schema.add_alias(canonical, alias)?;
    schema.save()?;
    println!(
        "{} Added alias: {} → {}",
        "3/3".dimmed(),
        canonical.cyan(),
        alias.yellow()
    );

    println!();
    println!("{} Canonical tag swapped successfully", "✓".green().bold());

    Ok(())
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;
    use crate::schema::TagSchema;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn create_test_schema() -> (TagSchema, PathBuf, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let schema_path = temp_dir.path().join("test_schema.toml");
        let schema = TagSchema::load(&schema_path).unwrap();
        (schema, schema_path, temp_dir)
    }

    #[test]
    fn test_add_and_remove_alias() {
        let (mut schema, _path, _dir) = create_test_schema();

        // Add alias
        schema.add_alias("js", "javascript").unwrap();
        assert_eq!(schema.canonicalize("js"), "javascript");

        // Remove alias
        schema.remove_alias("js").unwrap();
        assert_eq!(schema.canonicalize("js"), "js");
    }

    #[test]
    fn test_list_empty_aliases() {
        let (schema, _path, _dir) = create_test_schema();
        let aliases = schema.list_aliases();
        assert!(aliases.is_empty());
    }

    #[test]
    fn test_list_multiple_aliases() {
        let (mut schema, _path, _dir) = create_test_schema();

        schema.add_alias("js", "javascript").unwrap();
        schema.add_alias("py", "python").unwrap();
        schema.add_alias("es", "javascript").unwrap();

        let aliases = schema.list_aliases();
        assert_eq!(aliases.len(), 3);
    }
}
