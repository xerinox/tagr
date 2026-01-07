use colored::Colorize;

use crate::cli::AliasCommands;
use crate::schema::{SchemaError, load_default_schema};

/// Execute alias management commands
///
/// # Errors
/// Returns error if schema operations fail (I/O, validation, circular references)
pub fn execute_alias_command(command: &AliasCommands) -> Result<(), SchemaError> {
    match command {
        AliasCommands::Add { alias, canonical } => add_alias(alias, canonical),
        AliasCommands::Remove { alias } => remove_alias(alias),
        AliasCommands::List => list_aliases(),
        AliasCommands::Show { tag } => show_aliases(tag),
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
        let mut sorted_synonyms = synonyms.clone();
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
