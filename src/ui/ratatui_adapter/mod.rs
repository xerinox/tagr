//! Ratatui-based fuzzy finder adapter
//!
//! This module provides an implementation of the `FuzzyFinder` trait
//! using ratatui (TUI framework) and nucleo (fuzzy matcher) as the backend.
//! It provides full control over the UI/UX while maintaining compatibility
//! with the existing trait abstractions.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────┐
//! │           RatatuiFinder                     │
//! │  (implements FuzzyFinder trait)             │
//! └────────────────────┬────────────────────────┘
//!                      │
//!        ┌─────────────┼─────────────┐
//!        ▼             ▼             ▼
//! ┌────────────┐ ┌───────────┐ ┌───────────┐
//! │   Nucleo   │ │  Ratatui  │ │ Crossterm │
//! │  (matcher) │ │ (widgets) │ │  (events) │
//! └────────────┘ └───────────┘ └───────────┘
//! ```
//!
//! # Features
//!
//! - **Async fuzzy matching** via nucleo (10-100x faster for large lists)
//! - **Full UI control** via ratatui widgets
//! - **Preview pane** with syntax highlighting
//! - **In-TUI dialogs** for prompts (no breaking out to dialoguer)
//! - **Status bar** for messages
//! - **Help overlay** (F1)

mod events;
mod finder;
mod state;
mod styled_preview;
mod theme;
pub mod widgets;

pub use finder::RatatuiFinder;
pub use finder::RatatuiPreviewProvider;
pub use state::{AppState, Mode};
pub use styled_preview::{StyledPreview, StyledPreviewGenerator};
pub use theme::Theme;

/// Parse a key string into a KeyEvent for testing
///
/// This is exposed for testing keybind configurations.
///
/// # Examples
/// ```
/// use tagr::ui::ratatui_adapter::parse_key_string_for_test;
/// let key = parse_key_string_for_test("ctrl-t");
/// assert!(key.is_some());
/// ```
#[cfg(test)]
pub(crate) fn parse_key_string_for_test(s: &str) -> Option<crossterm::event::KeyEvent> {
    RatatuiFinder::parse_key_string(s)
}
