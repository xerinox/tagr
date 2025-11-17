//! Keybind system for interactive browse mode.
//!
//! This module provides customizable keyboard shortcuts for performing actions
//! on files directly within the fuzzy finder interface.

pub mod actions;
pub mod config;
pub mod executor;
pub mod prompts;

pub use actions::{BrowseAction, ActionResult};
pub use config::KeybindConfig;
pub use executor::{ActionExecutor, ActionContext};
