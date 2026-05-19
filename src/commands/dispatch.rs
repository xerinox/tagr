use crate::cli::{AliasCommands, Commands};
use crate::config::TagrConfig;
use crate::db::Database;
use crate::commands;
use crate::TagrError;
use std::io::Write;

fn required_arg(name: &str) -> TagrError {
    TagrError::InvalidInput(format!("Missing required argument '{name}'"))
}

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
        Commands::Bulk { command, .. } => {
            use crate::cli::{BulkCommands, TransformationType};
            use crate::commands::bulk::{CopyTagsConfig, TagTransformation};
            use crate::cli::SearchParams;

            match command {
                BulkCommands::Tag {
                    criteria,
                    add_tags,
                    conditions,
                    dry_run,
                    yes,
                } => {
                    let params = SearchParams::from(criteria);
                    commands::bulk::bulk_tag(
                        db, params, add_tags, conditions, *dry_run, *yes, quiet, writer,
                    )?;
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
                    commands::bulk::bulk_untag(
                        db, params, remove_tags, *all, conditions, *dry_run, *yes, quiet, writer,
                    )?;
                }
                BulkCommands::RenameTag {
                    old_tag,
                    new_tag,
                    dry_run,
                    yes,
                } => {
                    commands::bulk::rename_tag(db, old_tag, new_tag, *dry_run, *yes, quiet, writer)?;
                }
                BulkCommands::MergeTags {
                    source_tags,
                    target_tag,
                    dry_run,
                    yes,
                } => {
                    commands::bulk::merge_tags(
                        db, source_tags, target_tag, *dry_run, *yes, quiet, writer,
                    )?;
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
                        db,
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
                    commands::bulk::batch_from_file(db, input, fmt, *dry_run, *yes, quiet, writer)?;
                }
                BulkCommands::MapTags {
                    input,
                    format,
                    delimiter,
                    dry_run,
                    yes,
                } => {
                    let fmt = convert_batch_format(format, *delimiter);
                    commands::bulk::bulk_map_tags(db, input, fmt, *dry_run, *yes, quiet, writer)?;
                }
                BulkCommands::DeleteFiles {
                    input,
                    format,
                    delimiter,
                    dry_run,
                    yes,
                } => {
                    let fmt = convert_batch_format(format, *delimiter);
                    commands::bulk::bulk_delete_files(db, input, fmt, *dry_run, *yes, quiet, writer)?;
                }
                BulkCommands::PropagateByDir {
                    root,
                    mappings,
                    hierarchy,
                    dry_run,
                    yes,
                } => {
                    commands::bulk::propagate_by_directory(
                        db,
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
                    commands::bulk::propagate_by_extension(
                        db, mappings, *no_defaults, *dry_run, *yes, quiet, writer,
                    )?;
                }
                BulkCommands::Transform {
                    transformation,
                    param,
                    replacement,
                    filter,
                    dry_run,
                    yes,
                } => {
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
                        Some(filter.as_slice())
                    };

                    commands::bulk::transform_tags(
                        db, &trans, filter_tags, *dry_run, *yes, quiet, writer,
                    )?;
                }
            }
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

fn convert_batch_format(format: &crate::cli::BatchFormatArg, delimiter: char) -> crate::commands::bulk::BatchFormat {
    use crate::cli::BatchFormatArg;
    use crate::commands::bulk::BatchFormat;

    match format {
        BatchFormatArg::Text => BatchFormat::PlainText,
        BatchFormatArg::Csv => BatchFormat::Csv(delimiter),
        BatchFormatArg::Json => BatchFormat::Json,
    }
}
