//! Keybind system for interactive browse mode.
//!
//! This module provides customizable keyboard shortcuts for performing actions
//! on files directly within the fuzzy finder interface.

pub mod actions;
pub mod config;
pub mod executor;
pub mod help;
pub mod metadata;
pub mod prompts;

pub use actions::{ActionResult, BrowseAction};
pub use config::KeybindConfig;
pub use executor::{ActionContext, ActionExecutor};
