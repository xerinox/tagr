use crate::cli::{Commands, AliasCommands};
use crate::config::TagrConfig;
use crate::db::Database;
use crate::TagrError;
use crate::commands;
use std::io::Write;

pub fn dispatch_command(
    command: &Commands,
    db: &Database,
    config: &TagrConfig,
    path_format: crate::config::PathFormat,
    quiet: bool,
    writer: &mut impl Write,
) -> Result<(), TagrError> {
    match command {
        Commands::Search {
            filter_args,
            criteria,
            ..
        } => {
            use crate::commands::search::{ExplicitFlags, FilterConfig, OutputConfig};

            let params = command.get_search_params().ok_or_else(|| {
                TagrError::InvalidInput("Failed to parse search parameters".into())
            })?;

            let save_filter = filter_args
                .save_filter
                .as_ref()
                .map(|name| (name.as_str(), filter_args.filter_desc.as_deref()));

            let has_explicit_tag_mode = criteria.any_tag || criteria.all_tags;
            let has_explicit_file_mode = criteria.any_file || criteria.all_files;
            let has_explicit_virtual_mode = criteria.any_virtual || criteria.all_virtual;

            commands::search::execute(
                db,
                params,
                FilterConfig {
                    apply: filter_args.filter.as_deref(),
                    save: save_filter,
                },
                ExplicitFlags {
                    tag_mode: has_explicit_tag_mode,
                    file_mode: has_explicit_file_mode,
                    virtual_mode: has_explicit_virtual_mode,
                },
                OutputConfig {
                    format: path_format,
                    quiet,
                },
                writer,
            )?;
        }
        Commands::List { variant, .. } => {
            commands::list::execute(db, *variant, path_format, quiet, writer)?;
        }
        Commands::Tag { .. } => {
            let ctx = command.get_tag_context().ok_or_else(|| {
                TagrError::InvalidInput("Failed to extract tag context from command".into())
            })?;
            commands::tag::execute(db, ctx.file, &ctx.tags, ctx.no_canonicalize, quiet, writer)?;
        }
        Commands::Untag { .. } => {
            let ctx = command.get_untag_context().ok_or_else(|| {
                TagrError::InvalidInput("Failed to extract untag context from command".into())
            })?;
            commands::tag::untag(db, ctx.file, &ctx.tags, ctx.all, quiet, writer)?;
        }
        Commands::Cleanup { .. } => {
            commands::cleanup(db, path_format, quiet, writer)?;
        }
        Commands::Note { command, .. } => {
            command.execute(db, config, path_format, writer)?;
        }
        Commands::Tags { command, .. } => {
            commands::tags(db, command, quiet, writer)?;
        }
        Commands::Bulk { command: _, .. } => {
            return Err(TagrError::InvalidInput("Bulk commands not yet supported in daemon mode".into()));
        }
        Commands::Alias { command } => {
            let db_ref = match command {
                AliasCommands::SetCanonical { .. } => Some(db),
                _ => None,
            };
            commands::alias(command, db_ref, writer)
                .map_err(|e| TagrError::InvalidInput(e.to_string()))?;
        }
        Commands::Filter { command } => {
             commands::filter::execute(command, quiet)?;
        }
        Commands::Browse { .. } => {
            return Err(TagrError::InvalidInput("Interactive browse mode not supported in daemon mode".into()));
        }
        // Config/Db commands handled locally usually
        Commands::Config { .. } | Commands::Db { .. } | Commands::Completions { .. } | Commands::Watch(_) => {
             // Should not happen or handled locally
        }
    }
    Ok(())
}
