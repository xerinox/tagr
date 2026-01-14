//! Browse command - interactive fuzzy finder for tags and files

use crate::{
    TagrError,
    browse::{
        session::{BrowseConfig, BrowseSession, HelpText, PhaseSettings},
        ui::BrowseController,
    },
    cli::{PreviewOverrides, SearchParams},
    config::{self, PreviewConfig},
    db::Database,
    filters::{FilterCriteria, FilterManager},
    keybinds::config::KeybindConfig,
    output,
    ui::ratatui_adapter::RatatuiFinder,
};

type Result<T> = std::result::Result<T, TagrError>;

impl From<config::PathFormat> for crate::browse::session::PathFormat {
    fn from(format: config::PathFormat) -> Self {
        match format {
            config::PathFormat::Absolute => Self::Absolute,
            config::PathFormat::Relative => Self::Relative,
        }
    }
}

/// Execute the browse command
///
/// # Errors
/// Returns an error if database operations fail or if the browse operation encounters issues
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn execute(
    db: &Database,
    mut search_params: Option<SearchParams>,
    filter_name: Option<&str>,
    save_filter: Option<(&str, Option<&str>)>,
    execute_cmd: Option<String>,
    preview_overrides: Option<&PreviewOverrides>,
    path_format: config::PathFormat,
    quiet: bool,
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

    let preview_config = if preview_overrides.as_ref().is_some_and(|o| o.no_preview) {
        None
    } else {
        let mut config = PreviewConfig::default();
        if let Some(overrides) = &preview_overrides
            && let Some(lines) = overrides.preview_lines
        {
            config.max_lines = lines;
        }
        Some(config)
    };

    let keybind_config = KeybindConfig::load_or_default()
        .map_err(|e| TagrError::InvalidInput(format!("Failed to load keybinds: {e}")))?;

    let tag_phase_settings = PhaseSettings {
        preview_enabled: preview_config.is_some(),
        preview_config: preview_config.clone(),
        keybind_config: keybind_config.clone(),
        help_text: HelpText::TagBrowser(vec![
            ("TAB".to_string(), "Multi-select".to_string()),
            ("Enter".to_string(), "Confirm selection".to_string()),
            ("Alt+N".to_string(), "Toggle file/note preview".to_string()),
            ("ESC".to_string(), "Cancel".to_string()),
        ]),
    };

    let file_phase_settings = PhaseSettings {
        preview_enabled: preview_config.is_some(),
        preview_config,
        keybind_config,
        help_text: HelpText::FileBrowser(vec![
            ("TAB".to_string(), "Multi-select".to_string()),
            ("Enter".to_string(), "Confirm selection".to_string()),
            ("ctrl+t".to_string(), "Add tags".to_string()),
            ("ctrl+d".to_string(), "Delete from database".to_string()),
            ("ctrl+o".to_string(), "Open file".to_string()),
            ("ctrl+y".to_string(), "Copy path".to_string()),
            ("ESC".to_string(), "Cancel".to_string()),
        ]),
    };

    let config = BrowseConfig {
        initial_search: search_params.clone(),
        path_format: path_format.into(),
        tag_phase_settings,
        file_phase_settings,
    };

    let session =
        BrowseSession::new(db, config).map_err(|e| TagrError::BrowseError(e.to_string()))?;

    let finder = RatatuiFinder::with_styled_preview(100); // Max 100 lines of syntax-highlighted preview

    let controller = BrowseController::new(session, finder);

    match controller.run() {
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

            Ok(())
        }
        Ok(None) => {
            if !quiet {
                println!("Browse cancelled.");
            }
            Ok(())
        }
        Err(e) => Err(TagrError::BrowseError(e.to_string())),
    }
}
