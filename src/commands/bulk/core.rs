use colored::Colorize;
use dialoguer::Confirm;
use std::path::PathBuf;

use crate::TagrError;

type Result<T> = std::result::Result<T, TagrError>;

/// Reason a file was skipped during bulk operation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    AlreadyExists,
    ConditionNotMet,
    Other,
}

/// Action type for bulk operations (used in preview and confirmation)
#[derive(Debug, Clone, Copy)]
pub enum BulkAction {
    Add,
    Remove,
    RemoveAll,
}

impl BulkAction {
    #[must_use]
    pub const fn verb(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Remove => "remove",
            Self::RemoveAll => "remove all tags from",
        }
    }
    #[must_use]
    pub const fn preposition(self) -> &'static str {
        match self {
            Self::Add => "to",
            Self::Remove => "from",
            Self::RemoveAll => "",
        }
    }
    #[must_use]
    pub const fn prompt_name(self) -> &'static str {
        match self {
            Self::Add => "tag",
            Self::Remove => "untag",
            Self::RemoveAll => "remove ALL tags from",
        }
    }
}

/// Summary of bulk operation results
#[derive(Debug, Default)]
pub struct BulkOpSummary {
    pub success: usize,
    pub skipped: usize,
    pub skipped_condition: usize,
    pub errors: usize,
    pub error_messages: Vec<String>,
}

impl BulkOpSummary {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    pub const fn add_success(&mut self) {
        self.success += 1;
    }
    pub const fn add_skip(&mut self) {
        self.skipped += 1;
    }
    pub const fn add_skip_condition(&mut self) {
        self.skipped_condition += 1;
    }
    pub fn add_error(&mut self, msg: String) {
        self.errors += 1;
        self.error_messages.push(msg);
    }
    pub fn print(&self, operation: &str) {
        println!("\n{}", format!("=== {operation} Summary ===").bold());
        println!("  {} {}", "✓ Success:".green(), self.success);
        if self.skipped > 0 {
            println!("  {} {}", "⊘ Skipped:".yellow(), self.skipped);
        }
        if self.skipped_condition > 0 {
            println!(
                "  {} {}",
                "⊘ Skipped (condition):".yellow(),
                self.skipped_condition
            );
        }
        if self.errors > 0 {
            println!("  {} {}", "✗ Errors:".red(), self.errors);
            if !self.error_messages.is_empty() {
                println!("\n{}", "Error details:".red().bold());
                for msg in &self.error_messages {
                    println!("  - {msg}");
                }
            }
        }
    }
}

/// Print dry-run preview of bulk operation
pub fn print_dry_run_preview(files: &[PathBuf], tags: &[String], action: BulkAction) {
    println!("{}", "=== Dry Run Mode ===".yellow().bold());
    println!(
        "Would {} tags {} {} {} file(s)",
        action.verb(),
        if tags.is_empty() {
            String::new()
        } else {
            format!("[{}]", tags.join(", ").cyan())
        },
        action.preposition(),
        files.len()
    );
    println!("\n{}", "Affected files:".bold());
    for (i, file) in files.iter().enumerate().take(10) {
        println!("  {}. {}", i + 1, file.display());
    }
    if files.len() > 10 {
        println!("  ... and {} more", files.len() - 10);
    }
    println!("\n{}", "Run without --dry-run to apply changes.".yellow());
}

/// Show confirmation prompt for bulk operation
pub fn confirm_bulk_operation(
    files: &[PathBuf],
    tags: &[String],
    action: BulkAction,
) -> Result<bool> {
    let prompt = if tags.is_empty() {
        format!(
            "{} {} file(s)?",
            action.prompt_name().to_uppercase(),
            files.len()
        )
    } else {
        format!(
            "{} {} file(s) with tags [{}]?",
            action.prompt_name().to_uppercase(),
            files.len(),
            tags.join(", ")
        )
    };
    Confirm::new()
        .with_prompt(prompt)
        .interact()
        .map_err(|e| TagrError::InvalidInput(format!("Failed to get confirmation: {e}")))
}
