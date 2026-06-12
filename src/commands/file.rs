//! File-oriented command implementations (e.g., show file metadata)

use crate::store::TagStore;
use crate::{TagrError, config, output};
use std::io::Write;
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, TagrError>;

/// Subcommands under the file command
#[derive(clap::Subcommand, Debug, Clone)]
pub enum FileCommands {
    /// Show detailed metadata, tags, and notes for a single file
    #[command(visible_alias = "s")]
    Show {
        /// Target file path
        #[arg(value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
        file: PathBuf,

        /// Output in JSON format
        #[arg(long = "json")]
        json: bool,

        /// Display absolute paths (overrides config)
        #[arg(long = "absolute", conflicts_with = "relative")]
        absolute: bool,

        /// Display relative paths (overrides config)
        #[arg(long = "relative", conflicts_with = "absolute")]
        relative: bool,
    },
}

#[derive(serde::Serialize)]
struct JsonFileShow {
    path: String,
    exists: bool,
    tags: Vec<String>,
    note: Option<JsonFileNote>,
    size_bytes: Option<u64>,
}

#[derive(serde::Serialize)]
struct JsonFileNote {
    content: String,
    created_at: i64,
    updated_at: i64,
}

/// Execute the file management command
///
/// # Errors
/// Returns an error if database operations fail
pub fn execute(
    store: &dyn TagStore,
    command: &FileCommands,
    path_format: config::PathFormat,
    writer: &mut impl Write,
) -> Result<()> {
    match command {
        FileCommands::Show {
            file,
            json,
            absolute,
            relative,
        } => {
            let active_format = if *absolute {
                config::PathFormat::Absolute
            } else if *relative {
                config::PathFormat::Relative
            } else {
                path_format
            };
            show_file(store, file, *json, active_format, writer)
        }
    }
}

fn show_file(
    store: &dyn TagStore,
    file_path: &Path,
    json: bool,
    path_format: config::PathFormat,
    writer: &mut impl Write,
) -> Result<()> {
    // Attempt canonicalization, fallback to original path if not existing on disk
    let (target_path, exists) = file_path.canonicalize().map_or_else(
        |_| (file_path.to_path_buf(), false),
        |canonical| (canonical, true),
    );

    let canonical_path = crate::types::TagrPath::new(&target_path)
        .map_err(|e| TagrError::InvalidInput(e.to_string()))?;

    let tags = store.get_tags(&canonical_path)?.unwrap_or_default();

    let note_rec = store.get_note(&canonical_path)?;

    let size_bytes = if exists {
        std::fs::metadata(&target_path).ok().map(|m| m.len())
    } else {
        None
    };

    if json {
        let json_show = JsonFileShow {
            path: output::format_path(&canonical_path, path_format),
            exists,
            tags: tags.iter().map(|t| t.as_str().to_string()).collect(),
            note: note_rec.map(|n| JsonFileNote {
                content: n.content,
                created_at: n.metadata.created_at,
                updated_at: n.metadata.updated_at,
            }),
            size_bytes,
        };
        let json_str = serde_json::to_string_pretty(&json_show)
            .map_err(|e| TagrError::InvalidInput(format!("Failed to serialize JSON: {e}")))?;
        writeln!(writer, "{json_str}")?;
    } else {
        let display_path = output::format_path(&canonical_path, path_format);
        writeln!(writer, "File: {display_path}")?;

        let status_str = if exists {
            size_bytes.map_or_else(
                || "Exists".to_string(),
                |bytes| format!("Exists ({:.2} KB)", bytes as f64 / 1024.0),
            )
        } else {
            "Not found on disk (orphaned database entry)".to_string()
        };
        writeln!(writer, "Status: {status_str}")?;

        if tags.is_empty() {
            writeln!(writer, "Tags: (no tags)")?;
        } else {
            let tags_str: Vec<&str> = tags
                .iter()
                .map(crate::types::tag_name::TagName::as_str)
                .collect();
            writeln!(writer, "Tags: [{}]", tags_str.join(", "))?;
        }

        writeln!(writer, "Note:")?;
        if let Some(n) = note_rec {
            let time_str = chrono::DateTime::from_timestamp(n.metadata.updated_at, 0).map_or_else(
                || "Unknown time".to_string(),
                |dt| dt.with_timezone(&chrono::Local).to_rfc2822(),
            );
            writeln!(writer, "  Last updated: {time_str}")?;
            writeln!(writer, "  ---")?;
            for line in n.content.lines() {
                writeln!(writer, "  {line}")?;
            }
            writeln!(writer, "  ---")?;
        } else {
            writeln!(writer, "  (no note)")?;
        }
    }

    Ok(())
}
