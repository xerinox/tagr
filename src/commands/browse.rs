//! Browse command - interactive fuzzy finder for tags and files

use crate::{
    TagrError,
    cli::{PreviewOverrides, SearchParams},
    config,
    db::Database,
    filters::{FilterCriteria, FilterManager},
    output, search,
};

type Result<T> = std::result::Result<T, TagrError>;

/// Execute the browse command
///
/// # Errors
/// Returns an error if database operations fail or if the browse operation encounters issues
#[allow(clippy::too_many_arguments)]
pub fn execute(
    db: &Database,
    mut search_params: Option<SearchParams>,
    filter_name: Option<&str>,
    save_filter: Option<(&str, Option<&str>)>,
    execute_cmd: Option<String>,
    preview_overrides: Option<PreviewOverrides>,
    path_format: config::PathFormat,
    quiet: bool,
    with_actions: bool,
) -> Result<()> {
    if let Some(name) = filter_name {
        let filter_path = crate::filters::get_filter_path()?;
        let manager = FilterManager::new(filter_path);
        let filter = manager.get(name)?;

        let filter_params = SearchParams::from(&filter.criteria);

        if let Some(ref mut params) = search_params {
            params.merge(&filter_params);
        } else {
            search_params = Some(filter_params);
        }

        manager.record_use(name)?;

        if !quiet {
            println!("Using filter '{name}'");
        }
    }

    // Load keybind configuration and use real-time keybinds
    let keybind_config = crate::keybinds::KeybindConfig::load_or_default()
        .map_err(|e| TagrError::InvalidInput(format!("Failed to load keybind config: {e}")))?;
    
    match search::browse_with_realtime_keybinds(
        db,
        search_params.clone(),
        preview_overrides,
        path_format,
        &keybind_config,
    ) {
        Ok(Some(result)) => {
            if with_actions {
                if !quiet {
                    println!("=== Selected Files ===");
                    for file in &result.selected_files {
                        println!("  - {}", output::format_path(file, path_format));
                    }
                }

                match search::show_actions_for_files(db, result.selected_files) {
                    Ok(()) => {}
                    Err(e) => eprintln!("Action error: {e}"),
                }
            } else {
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

            if let Some((name, desc)) = save_filter {
                if let Some(params) = search_params {
                    let filter_path = crate::filters::get_filter_path()?;
                    let manager = FilterManager::new(filter_path);
                    let criteria = FilterCriteria::from(params);
                    let description = desc.unwrap_or("Saved browse filter");

                    manager.create(name, description.to_string(), criteria)?;

                    if !quiet {
                        println!("\nSaved filter '{name}'");
                    }
                } else if !quiet {
                    println!("\nWarning: Cannot save filter with no search criteria");
                }
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
