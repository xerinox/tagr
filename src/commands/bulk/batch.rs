use std::path::{Path, PathBuf};

use colored::Colorize;
use dialoguer::Confirm;

use super::core::{BulkOpSummary, SkipReason};
use crate::{TagrError, db::Database};

type Result<T> = std::result::Result<T, TagrError>;

/// Batch input format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchFormat {
    PlainText,
    Csv(char),
    Json,
}

#[derive(Debug, Clone)]
pub struct BatchEntry {
    pub file: PathBuf,
    pub tags: Vec<String>,
}

pub fn format_mismatch_hint_parsed(
    content: &str,
    attempted: BatchFormat,
    delimiter: char,
) -> Option<String> {
    let is_json = serde_json::from_str::<serde_json::Value>(content).is_ok();
    let is_csv = {
        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(false)
            .delimiter(delimiter as u8)
            .from_reader(content.as_bytes());
        let mut ok = false;
        for rec in rdr.records() {
            match rec {
                Ok(r) => {
                    if !r.is_empty() {
                        ok = true;
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        ok
    };
    let is_plain = content.lines().any(|l| {
        let t = l.trim();
        !t.is_empty() && !t.starts_with('#') && t.split_whitespace().count() >= 2
    });
    match attempted {
        BatchFormat::Json if is_csv => {
            Some("Hint: The file appears to be CSV. Use '--format csv'.".to_string())
        }
        BatchFormat::Json if is_plain => {
            Some("Hint: The file appears to be plain text. Use '--format text'.".to_string())
        }
        BatchFormat::Csv(_) | BatchFormat::PlainText if is_json => {
            Some("Hint: The file may be JSON. Use '--format json'.".to_string())
        }
        BatchFormat::PlainText if is_csv => {
            Some("Hint: The file appears to be CSV. Use '--format csv'.".to_string())
        }
        _ => None,
    }
}

/// Apply tags to files from a batch input file in one of the supported formats.
///
/// # Errors
/// Returns `TagrError::InvalidInput` if the input cannot be read or parsed,
/// or if records are malformed (missing file path, invalid CSV/JSON).
#[allow(clippy::too_many_arguments)]
pub fn batch_from_file(
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
    let entries = match format {
        BatchFormat::PlainText => parse_plaintext(&content)?,
        BatchFormat::Csv(d) => parse_csv(&content, d)?,
        BatchFormat::Json => parse_json(&content)?,
    };
    if entries.is_empty() {
        if !quiet {
            println!("No valid entries found in input.");
        }
        return Ok(());
    }
    if dry_run {
        println!("{}", "=== Dry Run Mode ===".yellow().bold());
        println!("Would apply tags to {} file(s)", entries.len());
        for (i, e) in entries.iter().enumerate().take(10) {
            println!(
                "  {}. {} <- [{}]",
                i + 1,
                e.file.display(),
                e.tags.join(", ")
            );
        }
        if entries.len() > 10 {
            println!("  ... and {} more", entries.len() - 10);
        }
        println!("\n{}", "Run without --dry-run to apply changes.".yellow());
        return Ok(());
    }
    if !yes {
        let prompt = format!(
            "Apply tags from '{}' to {} file entries?",
            input_path.display(),
            entries.len()
        );
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
    for entry in entries {
        if entry.tags.is_empty() {
            let _ = SkipReason::AlreadyExists;
            summary.add_skip();
            continue;
        }
        match db.add_tags(&entry.file, entry.tags) {
            Ok(()) => {
                summary.add_success();
                if !quiet {
                    println!("✓ Tagged: {}", entry.file.display());
                }
            }
            Err(e) => {
                summary.add_error(format!("{}: {}", entry.file.display(), e));
                if !quiet {
                    eprintln!("✗ Failed to tag {}: {}", entry.file.display(), e);
                }
            }
        }
    }
    if !quiet {
        summary.print("Batch From File");
    }
    Ok(())
}

pub fn parse_plaintext(content: &str) -> Result<Vec<BatchEntry>> {
    let mut entries = Vec::new();
    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 2 {
            return Err(TagrError::InvalidInput(format!(
                "Invalid format at line {}: expected 'file tag1 tag2'",
                i + 1
            )));
        }
        let file = PathBuf::from(parts[0]);
        let tags = parts[1..].iter().map(|s| (*s).to_string()).collect();
        entries.push(BatchEntry { file, tags });
    }
    Ok(entries)
}

pub fn parse_csv(content: &str, delimiter: char) -> Result<Vec<BatchEntry>> {
    let trimmed = content.trim_start();
    if trimmed.starts_with('[') || trimmed.starts_with('{') {
        let base = "Invalid CSV: content appears to be JSON".to_string();
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
    let mut entries = Vec::new();
    for (i, result) in rdr.records().enumerate() {
        let record = result.map_err(|e| {
            let base = format!("Invalid CSV at record {}: {}", i + 1, e);
            match format_mismatch_hint_parsed(content, BatchFormat::Csv(delimiter), delimiter) {
                Some(h) => TagrError::InvalidInput(format!("{base}\n{h}")),
                None => TagrError::InvalidInput(base),
            }
        })?;
        if record.is_empty() || record.get(0).is_none_or(|s| s.trim().is_empty()) {
            return Err(TagrError::InvalidInput(format!(
                "Invalid CSV at record {}: missing file path",
                i + 1
            )));
        }
        let file = PathBuf::from(record.get(0).unwrap().trim());
        let mut tags: Vec<String> = Vec::new();
        for field in record.iter().enumerate().skip(1).map(|(_, f)| f) {
            let f = field.trim();
            if f.is_empty() {
                continue;
            }
            if f.contains(',') {
                tags.extend(
                    f.split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(std::string::ToString::to_string),
                );
            } else {
                tags.push(f.to_string());
            }
        }
        entries.push(BatchEntry { file, tags });
    }
    Ok(entries)
}

pub fn parse_json(content: &str) -> Result<Vec<BatchEntry>> {
    #[derive(serde::Deserialize)]
    struct JsonEntry {
        file: String,
        tags: Vec<String>,
    }
    let parsed: Vec<JsonEntry> = match serde_json::from_str(content) {
        Ok(v) => v,
        Err(e) => {
            let base = format!("Invalid JSON: {e}");
            let msg = match format_mismatch_hint_parsed(content, BatchFormat::Json, ',') {
                Some(h) => format!("{base}\n{h}"),
                None => base,
            };
            return Err(TagrError::InvalidInput(msg));
        }
    };
    Ok(parsed
        .into_iter()
        .map(|je| BatchEntry {
            file: PathBuf::from(je.file),
            tags: je.tags,
        })
        .collect())
}
