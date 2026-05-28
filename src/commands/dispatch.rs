use crate::cli::{AliasCommands, BulkCommands, Commands, TransformationType};
use crate::cli::SearchParams;
use crate::config::TagrConfig;
use crate::store::TagStore;
use crate::db::Database;
use crate::commands;
use crate::commands::bulk::{BatchFormat, CopyTagsConfig, TagTransformation};
use crate::TagrError;
use std::io::Write;

fn required_arg(name: &str) -> TagrError {
    TagrError::InvalidInput(format!("Missing required argument '{name}'"))
}

/// Dispatch a CLI command through the writer for IPC capture.
///
/// Takes both `&Database` (for unmigrated commands) and `&dyn TagStore` (for
/// migrated commands). The `db` parameter will be removed once all commands
/// accept `&dyn TagStore`.
///
/// # Errors
/// Returns `TagrError` if the command handler fails.
pub fn dispatch_command(
    command: &Commands,
    db: &Database,
    store: &dyn TagStore,
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
        } => dispatch_search(command, filter_args, criteria, store, path_format, quiet, writer),
        Commands::List { variant, .. } => {
            commands::list::execute(store, *variant, path_format, quiet, writer)?;
            Ok(())
        }
        Commands::Tag { .. } => {
            let ctx = command.get_tag_context().ok_or_else(|| {
                TagrError::InvalidInput("Failed to extract tag context from command".into())
            })?;
            commands::tag::execute(store, ctx.file, &ctx.tags, ctx.no_canonicalize, quiet, writer)?;
            Ok(())
        }
        Commands::Untag { .. } => {
            let ctx = command.get_untag_context().ok_or_else(|| {
                TagrError::InvalidInput("Failed to extract untag context from command".into())
            })?;
            commands::tag::untag(store, ctx.file, &ctx.tags, ctx.all, quiet, writer)?;
            Ok(())
        }
        Commands::Cleanup { .. } => {
            commands::cleanup(store, path_format, quiet, writer)?;
            Ok(())
        }
        Commands::Note { command, .. } => {
            command.execute(store, config, path_format, writer)?;
            Ok(())
        }
        Commands::Tags { command, .. } => {
            commands::tags(store, command, quiet, writer)?;
            Ok(())
        }
        Commands::Bulk { command, .. } => dispatch_bulk(command, store, quiet, writer),
        Commands::Alias { command } => {
            let store_ref = match command {
                AliasCommands::SetCanonical { .. } => Some(store),
                _ => None,
            };
            commands::alias(command, store_ref, writer)
                .map_err(|e| TagrError::InvalidInput(e.to_string()))?;
            Ok(())
        }
        Commands::Filter { command } => {
             commands::filter::execute(command, quiet)?;
             Ok(())
        }
        Commands::Browse { filter_args, .. } => {
            // Browse needs Arc<dyn TagStore> — construct one from the Database.
            // Will be refactored when main.rs passes Arc<dyn TagStore> directly.
            let ctx = command.get_browse_context().ok_or_else(|| {
                TagrError::InvalidInput("Failed to extract browse context from command".into())
            })?;

            let save_filter = filter_args
                .save_filter
                .as_ref()
                .map(|name| (name.as_str(), filter_args.filter_desc.as_deref()));

            commands::browse::execute(
                std::sync::Arc::new(crate::store::DirectStore::new(db.clone())),
                None,
                ctx.search_params,
                filter_args.filter.as_deref(),
                save_filter,
                ctx.execute_cmd,
                Some(&ctx.preview_overrides),
                path_format,
                quiet,
            )?;
            Ok(())
        }
        Commands::Config { .. } | Commands::Db { .. } | Commands::Completions { .. } | Commands::Watch { .. } => {
             Ok(())
        }
    }
}

/// Handle the search command with filter and output configuration.
fn dispatch_search(
    command: &Commands,
    filter_args: &crate::cli::FilterArgs,
    criteria_args: &crate::cli::SearchCriteriaArgs,
    store: &dyn TagStore,
    path_format: crate::config::PathFormat,
    quiet: bool,
    writer: &mut impl Write,
) -> Result<(), TagrError> {
    use crate::commands::search::{ExplicitFlags, FilterConfig, OutputConfig};

    let criteria = command.get_search_criteria().ok_or_else(|| {
        TagrError::InvalidInput("Failed to parse search parameters".into())
    })?;

    let save_filter = filter_args
        .save_filter
        .as_ref()
        .map(|name| (name.as_str(), filter_args.filter_desc.as_deref()));

    commands::search::execute(
        store,
        criteria,
        FilterConfig {
            apply: filter_args.filter.as_deref(),
            save: save_filter,
        },
        ExplicitFlags {
            tag_mode: criteria_args.any_tag || criteria_args.all_tags,
            file_mode: criteria_args.any_file || criteria_args.all_files,
            virtual_mode: criteria_args.any_virtual || criteria_args.all_virtual,
            glob_files: criteria_args.glob_files,
        },
        OutputConfig {
            format: path_format,
            quiet,
        },
        writer,
    )?;
    Ok(())
}

/// Handle all bulk command variants.
fn dispatch_bulk(
    command: &BulkCommands,
    store: &dyn TagStore,
    quiet: bool,
    writer: &mut impl Write,
) -> Result<(), TagrError> {
    match command {
        BulkCommands::Tag {
            criteria,
            add_tags,
            conditions,
            dry_run,
            yes,
        } => {
            let params = SearchParams::from(criteria);
            commands::bulk::bulk_tag(store, params, add_tags, conditions, *dry_run, *yes, quiet, writer)?;
        }
        BulkCommands::Untag {
            criteria,
            remove_tags,
            all,
            conditions,
            dry_run,
            yes,
        } => {
            let params = SearchParams::from(criteria);
            commands::bulk::bulk_untag(store, params, remove_tags, *all, conditions, *dry_run, *yes, quiet, writer)?;
        }
        BulkCommands::RenameTag {
            old_tag,
            new_tag,
            dry_run,
            yes,
        } => {
            commands::bulk::rename_tag(store, old_tag, new_tag, *dry_run, *yes, quiet, writer)?;
        }
        BulkCommands::MergeTags {
            source_tags,
            target_tag,
            dry_run,
            yes,
        } => {
            commands::bulk::merge_tags(store, source_tags, target_tag, *dry_run, *yes, quiet, writer)?;
        }
        BulkCommands::CopyTags {
            source,
            criteria,
            specific_tags,
            exclude,
            dry_run,
            yes,
        } => {
            let params = SearchParams::from(criteria);
            let specific = if specific_tags.is_empty() {
                None
            } else {
                Some(specific_tags.as_slice())
            };
            commands::bulk::copy_tags(
                store,
                source,
                params,
                CopyTagsConfig {
                    specific_tags: specific,
                    exclude_tags: exclude,
                    dry_run: *dry_run,
                    yes: *yes,
                    quiet,
                },
                writer,
            )?;
        }
        BulkCommands::FromFile {
            input,
            format,
            delimiter,
            dry_run,
            yes,
        } => {
            let fmt = convert_batch_format(format, *delimiter);
            commands::bulk::batch_from_file(store, input, fmt, *dry_run, *yes, quiet, writer)?;
        }
        BulkCommands::MapTags {
            input,
            format,
            delimiter,
            dry_run,
            yes,
        } => {
            let fmt = convert_batch_format(format, *delimiter);
            commands::bulk::bulk_map_tags(store, input, fmt, *dry_run, *yes, quiet, writer)?;
        }
        BulkCommands::DeleteFiles {
            input,
            format,
            delimiter,
            dry_run,
            yes,
        } => {
            let fmt = convert_batch_format(format, *delimiter);
            commands::bulk::bulk_delete_files(store, input, fmt, *dry_run, *yes, quiet, writer)?;
        }
        BulkCommands::PropagateByDir {
            root,
            mappings,
            hierarchy,
            dry_run,
            yes,
        } => {
            commands::bulk::propagate_by_directory(
                store,
                root.as_deref(),
                mappings,
                *hierarchy,
                *dry_run,
                *yes,
                quiet,
                writer,
            )?;
        }
        BulkCommands::PropagateByExt {
            mappings,
            no_defaults,
            dry_run,
            yes,
        } => {
            commands::bulk::propagate_by_extension(store, mappings, *no_defaults, *dry_run, *yes, quiet, writer)?;
        }
        BulkCommands::Transform {
            transformation,
            param,
            replacement,
            filter,
            dry_run,
            yes,
        } => dispatch_transform(transformation, param, replacement, filter, store, *dry_run, *yes, quiet, writer)?,
    }
    Ok(())
}

/// Handle bulk transform command — convert CLI transformation type to library type.
fn dispatch_transform(
    transformation: &TransformationType,
    param: &Option<String>,
    replacement: &Option<String>,
    filter: &[String],
    store: &dyn TagStore,
    dry_run: bool,
    yes: bool,
    quiet: bool,
    writer: &mut impl Write,
) -> Result<(), TagrError> {
    let required_param = |name: &str| -> Result<String, TagrError> {
        param.clone().ok_or_else(|| required_arg(name))
    };

    let trans = match transformation {
        TransformationType::Lowercase => TagTransformation::Lowercase,
        TransformationType::Uppercase => TagTransformation::Uppercase,
        TransformationType::KebabCase => TagTransformation::KebabCase,
        TransformationType::SnakeCase => TagTransformation::SnakeCase,
        TransformationType::CamelCase => TagTransformation::CamelCase,
        TransformationType::PascalCase => TagTransformation::PascalCase,
        TransformationType::AddPrefix => {
            TagTransformation::AddPrefix(required_param("param")?)
        }
        TransformationType::AddSuffix => {
            TagTransformation::AddSuffix(required_param("param")?)
        }
        TransformationType::RemovePrefix => {
            TagTransformation::RemovePrefix(required_param("param")?)
        }
        TransformationType::RemoveSuffix => {
            TagTransformation::RemoveSuffix(required_param("param")?)
        }
        TransformationType::RegexReplace => TagTransformation::RegexReplace {
            pattern: required_param("param")?,
            replacement: replacement
                .clone()
                .ok_or_else(|| required_arg("replacement"))?,
        },
    };

    let filter_tags = if filter.is_empty() {
        None
    } else {
        Some(filter)
    };

    commands::bulk::transform_tags(store, &trans, filter_tags, dry_run, yes, quiet, writer)?;
    Ok(())
}

fn convert_batch_format(format: &crate::cli::BatchFormatArg, delimiter: char) -> BatchFormat {
    use crate::cli::BatchFormatArg;

    match format {
        BatchFormatArg::Text => BatchFormat::PlainText,
        BatchFormatArg::Csv => BatchFormat::Csv(delimiter),
        BatchFormatArg::Json => BatchFormat::Json,
    }
}
