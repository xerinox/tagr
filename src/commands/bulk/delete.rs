use colored::Colorize;
use dialoguer::Confirm;
use std::path::{Path, PathBuf};

use super::batch::{BatchFormat, format_mismatch_hint_parsed};
use super::core::{BulkOpSummary, SkipReason};
use crate::{TagrError, db::Database};

type Result<T> = std::result::Result<T, TagrError>;

/// Delete files from the database using a batch input list.
///
/// # Errors
/// Returns `TagrError::InvalidInput` if the input cannot be read or parsed,
/// or if records are malformed (e.g., empty path fields).
pub fn bulk_delete_files(
    db: &Database,
    input_path: &Path,
    format: BatchFormat,
    dry_run: bool,
    yes: bool,
    quiet: bool,
) -> Result<()> {
    let content = std::fs::read_to_string(input_path).map_err(|e| {
        TagrError::InvalidInput(format!("Failed to read {}: {}", input_path.display(), e))
    })?;
    let mut files = match format {
        BatchFormat::PlainText => parse_delete_plaintext(&content)?,
        BatchFormat::Csv(d) => parse_delete_csv(&content, d)?,
        BatchFormat::Json => parse_delete_json(&content)?,
    };
    if files.is_empty() {
        if !quiet {
            println!("No file paths found in input.");
        }
        return Ok(());
    }
    let set: std::collections::HashSet<_> = files.into_iter().collect();
    files = set.into_iter().collect();
    if dry_run {
        println!("{}", "=== Dry Run Mode ===".yellow().bold());
        println!("Would delete {} file(s) from database", files.len());
        for (i, f) in files.iter().enumerate().take(15) {
            println!("  {}. {}", i + 1, f.display());
        }
        if files.len() > 15 {
            println!("  ... and {} more", files.len() - 15);
        }
        println!("\n{}", "Run without --dry-run to apply changes.".yellow());
        return Ok(());
    }
    if !yes {
        let prompt = format!("Delete {} file(s) from database?", files.len());
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
    for file in files {
        match db.remove(&file) {
            Ok(existed) => {
                if existed {
                    summary.add_success();
                    if !quiet {
                        println!("✓ Deleted: {}", file.display());
                    }
                } else {
                    let _ = SkipReason::Other;
                    summary.add_skip();
                    if !quiet {
                        println!("⊘ Skipped (not in db): {}", file.display());
                    }
                }
            }
            Err(e) => {
                summary.add_error(format!("{}: {}", file.display(), e));
                if !quiet {
                    eprintln!("✗ Failed to delete {}: {}", file.display(), e);
                }
            }
        }
    }
    if !quiet {
        summary.print("Delete Files");
    }
    Ok(())
}

pub fn parse_delete_plaintext(content: &str) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let first = trimmed.split_whitespace().next().ok_or_else(|| {
            TagrError::InvalidInput(format!("Invalid delete entry at line {}", i + 1))
        })?;
        files.push(PathBuf::from(first));
    }
    Ok(files)
}

pub fn parse_delete_csv(content: &str, delimiter: char) -> Result<Vec<PathBuf>> {
    let trimmed = content.trim_start();
    if trimmed.starts_with('[') || trimmed.starts_with('{') {
        let base = "Invalid CSV delete list: content appears to be JSON".to_string();
        let msg = match format_mismatch_hint_parsed(content, BatchFormat::Csv(delimiter), delimiter)
        {
            Some(h) => format!("{base}\n{h}"),
            None => base,
        };
        return Err(TagrError::InvalidInput(msg));
    }
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .delimiter(delimiter as u8)
        .from_reader(content.as_bytes());
    let mut files = Vec::new();
    for (i, rec) in rdr.records().enumerate() {
        let record = rec.map_err(|e| {
            let base = format!("Invalid CSV delete list at record {}: {}", i + 1, e);
            match format_mismatch_hint_parsed(content, BatchFormat::Csv(delimiter), delimiter) {
                Some(h) => TagrError::InvalidInput(format!("{base}\n{h}")),
                None => TagrError::InvalidInput(base),
            }
        })?;
        if record.is_empty() {
            continue;
        }
        let path = record.get(0).unwrap().trim();
        if path.is_empty() {
            return Err(TagrError::InvalidInput(format!(
                "Invalid CSV delete list at record {}: empty path",
                i + 1
            )));
        }
        files.push(PathBuf::from(path));
    }
    Ok(files)
}

pub fn parse_delete_json(content: &str) -> Result<Vec<PathBuf>> {
    #[derive(serde::Deserialize)]
    struct JsonDelete {
        file: String,
    }
    let parsed: Vec<JsonDelete> = match serde_json::from_str(content) {
        Ok(v) => v,
        Err(e) => {
            let base = format!("Invalid JSON delete list: {e}");
            let msg = match format_mismatch_hint_parsed(content, BatchFormat::Json, ',') {
                Some(h) => format!("{base}\n{h}"),
                None => base,
            };
            return Err(TagrError::InvalidInput(msg));
        }
    };
    Ok(parsed.into_iter().map(|j| PathBuf::from(j.file)).collect())
}
