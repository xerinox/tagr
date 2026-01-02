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
mod theme;
pub mod widgets;

pub use finder::RatatuiFinder;
pub use finder::RatatuiPreviewProvider;
pub use state::{AppState, Mode};
pub use theme::Theme;
