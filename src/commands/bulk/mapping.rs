use std::path::Path;

use colored::Colorize;
use dialoguer::Confirm;

use super::batch::{BatchFormat, format_mismatch_hint_parsed};
use super::core::{BulkOpSummary, SkipReason};
use crate::{Pair, TagrError, db::Database};

type Result<T> = std::result::Result<T, TagrError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagMapping {
    pub from: String,
    pub to: String,
}

/// Map tags in bulk from an input file to new values.
///
/// # Errors
/// Returns `TagrError::InvalidInput` when the input cannot be read or parsed,
/// or when mapping records are invalid (empty fields, wrong column count).
#[allow(clippy::too_many_lines)]
pub fn bulk_map_tags(
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
    let mappings = match format {
        BatchFormat::PlainText => parse_mapping_text(&content)?,
        BatchFormat::Csv(d) => parse_mapping_csv(&content, d)?,
        BatchFormat::Json => parse_mapping_json(&content)?,
    };
    if mappings.is_empty() {
        if !quiet {
            println!("No valid tag mappings found in input.");
        }
        return Ok(());
    }
    if dry_run {
        println!("{}", "=== Dry Run Mode ===".yellow().bold());
        println!("Would apply {} tag mapping(s):", mappings.len());
        for (i, m) in mappings.iter().enumerate().take(15) {
            println!("  {}. '{}' → '{}'", i + 1, m.from.cyan(), m.to.green());
        }
        if mappings.len() > 15 {
            println!("  ... and {} more", mappings.len() - 15);
        }
        println!("\n{}", "Run without --dry-run to apply changes.".yellow());
        return Ok(());
    }
    if !yes {
        let prompt = format!(
            "Apply {} tag mapping(s) from '{}' ?",
            mappings.len(),
            input_path.display()
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
    for mapping in mappings {
        if mapping.from == mapping.to {
            summary.add_skip();
            if !quiet {
                println!("⊘ Skipped (identical): '{}'", mapping.from);
            }
            continue;
        }
        let files = db.find_by_tag(&mapping.from)?;
        if files.is_empty() {
            summary.add_skip();
            if !quiet {
                println!("⊘ Skipped (not found): '{}'", mapping.from);
            }
            continue;
        }
        for file in files {
            let Some(mut tags) = db.get_tags(&file)? else {
                let _ = SkipReason::Other;
                summary.add_skip();
                continue;
            };
            if !tags.iter().any(|t| t == &mapping.from) {
                summary.add_skip();
                continue;
            }
            let target_exists = tags.iter().any(|t| t == &mapping.to);
            let mut changed = false;
            for t in &mut tags {
                if t == &mapping.from {
                    if target_exists {
                        *t = String::new();
                    } else {
                        t.clone_from(&mapping.to);
                    }
                    changed = true;
                }
            }
            if !changed {
                summary.add_skip();
                continue;
            }
            let new_tags: Vec<String> = tags
                .into_iter()
                .filter(|t| !t.is_empty())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();
            let pair = Pair {
                file: file.clone(),
                tags: new_tags,
            };
            match db.insert_pair(&pair) {
                Ok(()) => {
                    summary.add_success();
                    if !quiet {
                        println!(
                            "✓ '{}' → '{}' in {}",
                            mapping.from,
                            mapping.to,
                            file.display()
                        );
                    }
                }
                Err(e) => {
                    summary.add_error(format!("{}: {}", file.display(), e));
                    if !quiet {
                        eprintln!(
                            "✗ Failed '{}' → '{}' in {}: {}",
                            mapping.from,
                            mapping.to,
                            file.display(),
                            e
                        );
                    }
                }
            }
        }
    }
    if !quiet {
        summary.print("Map Tags");
    }
    Ok(())
}

pub fn parse_mapping_text(content: &str) -> Result<Vec<TagMapping>> {
    let mut mappings = Vec::new();
    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() != 2 {
            return Err(TagrError::InvalidInput(format!(
                "Invalid mapping at line {}: expected 'old new'",
                i + 1
            )));
        }
        mappings.push(TagMapping {
            from: parts[0].to_string(),
            to: parts[1].to_string(),
        });
    }
    Ok(mappings)
}

pub fn parse_mapping_csv(content: &str, delimiter: char) -> Result<Vec<TagMapping>> {
    let trimmed = content.trim_start();
    if trimmed.starts_with('[') || trimmed.starts_with('{') {
        let base = "Invalid CSV mapping: content appears to be JSON".to_string();
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
    let mut mappings = Vec::new();
    for (i, rec) in rdr.records().enumerate() {
        let record = rec.map_err(|e| {
            let base = format!("Invalid CSV mapping at record {}: {}", i + 1, e);
            match format_mismatch_hint_parsed(content, BatchFormat::Csv(delimiter), delimiter) {
                Some(h) => TagrError::InvalidInput(format!("{base}\n{h}")),
                None => TagrError::InvalidInput(base),
            }
        })?;
        if record.len() != 2 {
            return Err(TagrError::InvalidInput(format!(
                "Invalid CSV mapping at record {}: expected exactly 2 fields (old,new)",
                i + 1
            )));
        }
        let from = record.get(0).unwrap().trim();
        let to = record.get(1).unwrap().trim();
        if from.is_empty() || to.is_empty() {
            return Err(TagrError::InvalidInput(format!(
                "Invalid CSV mapping at record {}: empty field",
                i + 1
            )));
        }
        mappings.push(TagMapping {
            from: from.to_string(),
            to: to.to_string(),
        });
    }
    Ok(mappings)
}

pub fn parse_mapping_json(content: &str) -> Result<Vec<TagMapping>> {
    #[derive(serde::Deserialize)]
    struct JsonMap {
        from: String,
        to: String,
    }
    let parsed: Vec<JsonMap> = match serde_json::from_str(content) {
        Ok(v) => v,
        Err(e) => {
            let base = format!("Invalid JSON mapping: {e}");
            let msg = match format_mismatch_hint_parsed(content, BatchFormat::Json, ',') {
                Some(h) => format!("{base}\n{h}"),
                None => base,
            };
            return Err(TagrError::InvalidInput(msg));
        }
    };
    Ok(parsed
        .into_iter()
        .map(|jm| TagMapping {
            from: jm.from,
            to: jm.to,
        })
        .collect())
}
