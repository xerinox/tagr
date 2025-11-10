//! Browse command - interactive fuzzy finder for tags and files

use crate::{
    db::Database,
    cli::SearchParams,
    config,
    output,
    search,
    TagrError,
};

type Result<T> = std::result::Result<T, TagrError>;

/// Execute the browse command
///
/// # Errors
/// Returns an error if database operations fail or if the browse operation encounters issues
pub fn execute(
    db: &Database,
    search_params: Option<SearchParams>,
    execute_cmd: Option<String>,
    path_format: config::PathFormat,
    quiet: bool,
) -> Result<()> {
    match search::browse_with_params(db, search_params, path_format) {
        Ok(Some(result)) => {
            if !quiet {
                println!("=== Selected Tags ===");
                for tag in &result.selected_tags {
                    println!("  - {tag}");
                }
                
                println!("\n=== Selected Files ===");
            }
            for file in &result.selected_files {
                let formatted_path = output::format_path(file, path_format);
                if quiet {
                    println!("{formatted_path}");
                } else {
                    println!("  - {formatted_path}");
                }
            }
            
            if let Some(cmd_template) = execute_cmd {
                if !quiet {
                    println!("\n=== Executing Command ===");
                }
                crate::cli::execute_command_on_files(&result.selected_files, &cmd_template, quiet);
            }
        }
        Ok(None) => {
            if !quiet {
                println!("Browse cancelled.");
            }
        }
        Err(e) => return Err(e.into()),
    }
    Ok(())
}
