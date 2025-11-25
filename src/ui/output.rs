//! Output abstraction layer
//!
//! This module provides a backend-agnostic interface for output operations,
//! allowing different implementations for CLI (stdout) and TUI (status bars).

use colored::Colorize;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Trait for output operations
///
/// Abstracts away output mechanism, allowing CLI (stdout) and TUI (status bars).
///
/// # Examples
///
/// ```no_run
/// use tagr::ui::output::{OutputWriter, StdoutWriter};
///
/// let output = StdoutWriter::new();
/// output.write("Normal message");
/// output.success("Operation completed!");
/// output.error("Something went wrong");
/// ```
pub trait OutputWriter: Send + Sync {
    /// Write a normal message
    fn write(&self, message: &str);

    /// Write an error message
    fn error(&self, message: &str);

    /// Write a success message
    fn success(&self, message: &str);

    /// Write a warning message
    fn warning(&self, message: &str);

    /// Write an info message (dimmed/secondary)
    fn info(&self, message: &str);

    /// Clear all messages (for TUI status bars)
    fn clear(&self);
}

/// CLI implementation - writes to stdout/stderr
///
/// This implementation uses colored output to stdout/stderr for a
/// traditional command-line interface.
///
/// # Examples
///
/// ```
/// use tagr::ui::output::{OutputWriter, StdoutWriter};
///
/// let output = StdoutWriter::new();
/// output.success("File saved successfully");
/// output.error("Failed to open file");
/// ```
pub struct StdoutWriter;

impl StdoutWriter {
    /// Create a new stdout writer
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for StdoutWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputWriter for StdoutWriter {
    fn write(&self, message: &str) {
        println!("{message}");
    }

    fn error(&self, message: &str) {
        eprintln!("{} {}", "❌".red(), message);
    }

    fn success(&self, message: &str) {
        println!("{} {}", "✓".green(), message);
    }

    fn warning(&self, message: &str) {
        println!("{} {}", "⚠️".yellow(), message);
    }

    fn info(&self, message: &str) {
        println!("{}", message.dimmed());
    }

    fn clear(&self) {
        // No-op for CLI
    }
}

/// Message level for categorizing output
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageLevel {
    /// Normal message
    Normal,
    /// Error message
    Error,
    /// Success message
    Success,
    /// Warning message
    Warning,
    /// Info message
    Info,
}

/// Buffered writer for TUI status bars
///
/// This implementation buffers messages for display in a TUI status bar,
/// with automatic expiration of old messages.
///
/// # Examples
///
/// ```
/// use tagr::ui::output::{OutputWriter, StatusBarWriter};
/// use std::time::Duration;
///
/// let writer = StatusBarWriter::new();
/// writer.success("File saved");
///
/// // Get recent messages for display
/// let messages = writer.recent_messages();
/// for (level, msg) in messages {
///     println!("{:?}: {}", level, msg);
/// }
/// ```
pub struct StatusBarWriter {
    messages: Arc<Mutex<Vec<(MessageLevel, String, Instant)>>>,
    ttl: Duration,
}

impl StatusBarWriter {
    /// Create a new status bar writer with default TTL (10 seconds)
    #[must_use]
    pub fn new() -> Self {
        Self::with_ttl(Duration::from_secs(10))
    }

    /// Create a new status bar writer with custom TTL
    #[must_use]
    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            messages: Arc::new(Mutex::new(Vec::new())),
            ttl,
        }
    }

    /// Get recent messages for display (within TTL)
    ///
    /// Returns messages that haven't expired yet, useful for rendering
    /// in a TUI status bar.
    #[must_use]
    pub fn recent_messages(&self) -> Vec<(MessageLevel, String)> {
        let now = Instant::now();
        let messages = self.messages.lock().unwrap();

        messages
            .iter()
            .filter(|(_, _, time)| now.duration_since(*time) < self.ttl)
            .map(|(level, msg, _)| (*level, msg.clone()))
            .collect()
    }

    /// Get the most recent message, if any
    #[must_use]
    pub fn latest_message(&self) -> Option<(MessageLevel, String)> {
        let now = Instant::now();
        let messages = self.messages.lock().unwrap();

        messages
            .iter()
            .rev()
            .find(|(_, _, time)| now.duration_since(*time) < self.ttl)
            .map(|(level, msg, _)| (*level, msg.clone()))
    }

    /// Get count of active messages
    #[must_use]
    pub fn message_count(&self) -> usize {
        let now = Instant::now();
        let messages = self.messages.lock().unwrap();

        messages
            .iter()
            .filter(|(_, _, time)| now.duration_since(*time) < self.ttl)
            .count()
    }

    fn add_message(&self, level: MessageLevel, message: String) {
        let mut messages = self.messages.lock().unwrap();
        messages.push((level, message, Instant::now()));

        // Keep only last 100 messages
        if messages.len() > 100 {
            messages.drain(0..50);
        }
    }
}

impl Default for StatusBarWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputWriter for StatusBarWriter {
    fn write(&self, message: &str) {
        self.add_message(MessageLevel::Normal, message.to_string());
    }

    fn error(&self, message: &str) {
        self.add_message(MessageLevel::Error, message.to_string());
    }

    fn success(&self, message: &str) {
        self.add_message(MessageLevel::Success, message.to_string());
    }

    fn warning(&self, message: &str) {
        self.add_message(MessageLevel::Warning, message.to_string());
    }

    fn info(&self, message: &str) {
        self.add_message(MessageLevel::Info, message.to_string());
    }

    fn clear(&self) {
        self.messages.lock().unwrap().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stdout_writer_creation() {
        let _writer = StdoutWriter::new();
        let _writer2 = StdoutWriter::default();
    }

    #[test]
    fn test_status_bar_writer_messages() {
        let writer = StatusBarWriter::new();

        writer.success("Test success");
        writer.error("Test error");
        writer.warning("Test warning");

        let messages = writer.recent_messages();
        assert_eq!(messages.len(), 3);

        assert_eq!(messages[0].0, MessageLevel::Success);
        assert_eq!(messages[0].1, "Test success");

        assert_eq!(messages[1].0, MessageLevel::Error);
        assert_eq!(messages[2].0, MessageLevel::Warning);
    }

    #[test]
    fn test_status_bar_writer_clear() {
        let writer = StatusBarWriter::new();

        writer.write("Message 1");
        writer.write("Message 2");

        assert_eq!(writer.message_count(), 2);

        writer.clear();
        assert_eq!(writer.message_count(), 0);
    }

    #[test]
    fn test_status_bar_writer_latest() {
        let writer = StatusBarWriter::new();

        writer.write("First");
        writer.success("Latest");

        let latest = writer.latest_message().unwrap();
        assert_eq!(latest.0, MessageLevel::Success);
        assert_eq!(latest.1, "Latest");
    }

    #[test]
    fn test_status_bar_writer_ttl() {
        let writer = StatusBarWriter::with_ttl(Duration::from_millis(50));

        writer.write("Message");
        assert_eq!(writer.message_count(), 1);

        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(writer.message_count(), 0);
    }

    #[test]
    fn test_message_level_equality() {
        assert_eq!(MessageLevel::Normal, MessageLevel::Normal);
        assert_ne!(MessageLevel::Error, MessageLevel::Success);
    }
}
