//! Ratatui-based fuzzy finder implementation
//!
//! Implements the `FuzzyFinder` trait using ratatui for UI and nucleo for matching.

use super::events::{EventResult, KeybindMap, poll_and_handle};
use super::state::{AppState, Mode};
use super::styled_preview::{StyledPreview, StyledPreviewGenerator};
use super::theme::Theme;
use super::widgets::{
    ConfirmDialog, HelpBar, HelpOverlay, ItemList, KeyHint, PreviewPane, RefineSearchOverlay,
    SearchBar, StatusBar, TextInputModal,
};
use crate::ui::error::Result;
use crate::ui::traits::{FinderConfig, FuzzyFinder, PreviewProvider, PreviewText};
use crate::ui::types::{FinderResult, PreviewPosition};
use crossterm::{
    event::KeyEvent,
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use nucleo::{
    Config, Nucleo,
    pattern::{CaseMatching, Normalization},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
};
use std::io::{self, Stdout};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// Ratatui-based fuzzy finder implementation
pub struct RatatuiFinder {
    /// Legacy preview provider (for trait compatibility - may be used in future)
    #[allow(dead_code)]
    preview_provider: Option<Arc<dyn PreviewProvider>>,
    /// Native styled preview generator (preferred)
    styled_generator: Option<StyledPreviewGenerator>,
    theme: Theme,
}

impl RatatuiFinder {
    /// Create a new ratatui finder
    #[must_use]
    pub fn new() -> Self {
        Self {
            preview_provider: None,
            styled_generator: None,
            theme: Theme::default(),
        }
    }

    /// Create a ratatui finder with native styled preview generator
    #[must_use]
    pub fn with_styled_preview(max_lines: usize) -> Self {
        Self {
            preview_provider: None,
            styled_generator: Some(StyledPreviewGenerator::new(max_lines)),
            theme: Theme::default(),
        }
    }

    /// Create a ratatui finder with legacy preview provider
    #[must_use]
    pub fn with_preview_provider(preview_provider: impl PreviewProvider + 'static) -> Self {
        Self {
            preview_provider: Some(Arc::new(preview_provider)),
            styled_generator: None,
            theme: Theme::default(),
        }
    }

    /// Set custom theme
    #[must_use]
    pub const fn with_theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    /// Setup terminal for TUI
    fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        Terminal::new(backend).map_err(Into::into)
    }

    /// Cleanup terminal after TUI
    fn cleanup_terminal() -> Result<()> {
        disable_raw_mode()?;
        execute!(io::stdout(), LeaveAlternateScreen)?;
        Ok(())
    }

    /// Parse keybinds from config format to `KeyEvent` map
    fn parse_keybinds(binds: &[String]) -> KeybindMap {
        let mut map = KeybindMap::new();

        for bind in binds {
            // Format: "key:action" e.g., "ctrl-t:add_tag"
            if let Some((key_str, action)) = bind.split_once(':')
                && let Some(key_event) = Self::parse_key_string(key_str)
            {
                // Skip navigation/toggle actions - we handle those internally
                if !matches!(action, "accept" | "abort" | "toggle" | "up" | "down") {
                    map.insert(key_event, action.to_string());
                }
            }
        }

        map
    }

    /// Parse a key string like "ctrl-t" into a `KeyEvent`
    fn parse_key_string(s: &str) -> Option<KeyEvent> {
        use crossterm::event::{KeyCode, KeyModifiers};

        let parts: Vec<&str> = s.split('-').collect();

        let mut modifiers = KeyModifiers::NONE;
        let key_part = parts.last()?;

        for part in &parts[..parts.len().saturating_sub(1)] {
            match part.to_lowercase().as_str() {
                "ctrl" | "control" => modifiers |= KeyModifiers::CONTROL,
                "alt" => modifiers |= KeyModifiers::ALT,
                "shift" => modifiers |= KeyModifiers::SHIFT,
                _ => {}
            }
        }

        let code = match key_part.to_lowercase().as_str() {
            "enter" => KeyCode::Enter,
            "esc" => KeyCode::Esc,
            "tab" => KeyCode::Tab,
            "btab" | "backtab" => KeyCode::BackTab,
            "bspace" | "backspace" => KeyCode::Backspace,
            "del" | "delete" => KeyCode::Delete,
            "up" => KeyCode::Up,
            "down" => KeyCode::Down,
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            "home" => KeyCode::Home,
            "end" => KeyCode::End,
            "pgup" | "pageup" => KeyCode::PageUp,
            "pgdn" | "pagedown" => KeyCode::PageDown,
            s if s.starts_with('f') && s.len() > 1 => s[1..].parse().ok().map(KeyCode::F)?,
            s if s.len() == 1 => KeyCode::Char(s.chars().next()?),
            _ => return None,
        };

        Some(KeyEvent::new(code, modifiers))
    }

    /// Create nucleo matcher with items
    fn create_matcher(items: &[crate::ui::DisplayItem]) -> Nucleo<u32> {
        let config = Config::DEFAULT.match_paths();
        let nucleo: Nucleo<u32> = Nucleo::new(config, Arc::new(|| {}), None, 1);

        // Inject items
        let injector = nucleo.injector();
        for (idx, item) in items.iter().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            let _ = injector.push(idx as u32, |_, cols| {
                cols[0] = item.searchable.clone().into();
            });
        }

        nucleo
    }

    /// Update nucleo pattern and get filtered indices
    fn update_filter(nucleo: &mut Nucleo<u32>, query: &str, prev_query: &str) -> Vec<u32> {
        nucleo.pattern.reparse(
            0,
            query,
            CaseMatching::Smart,
            Normalization::Smart,
            query.starts_with(prev_query),
        );

        // Tick to process matching
        nucleo.tick(100);

        let snapshot = nucleo.snapshot();
        snapshot.matched_items(..).map(|item| *item.data).collect()
    }

    /// Build minimal help hints for the bottom bar
    fn build_hints() -> Vec<KeyHint> {
        vec![
            KeyHint::new("↑/↓", "navigate"),
            KeyHint::new("TAB", "select"),
            KeyHint::new("Enter", "confirm"),
            KeyHint::new("ESC", "cancel"),
            KeyHint::new("F1", "help"),
            KeyHint::new("F2", "refine"),
        ]
    }

    /// Build full keybind list for help overlay
    fn build_overlay_binds(custom_binds: &KeybindMap) -> Vec<(String, String)> {
        let mut binds: Vec<(String, String)> = custom_binds
            .iter()
            .filter_map(|(key, action)| {
                super::events::key_to_string(key)
                    .map(|key_str| (key_str, Self::format_action_name(action)))
            })
            .collect();

        // Sort by key for consistent display
        binds.sort_by(|a, b| a.0.cmp(&b.0));

        // Add preview scroll hint (always available)
        binds.push(("Shift+↑/↓".to_string(), "scroll preview".to_string()));

        binds
    }

    /// Format action name for display in help overlay
    fn format_action_name(action: &str) -> String {
        match action {
            "add_tag" => "add tag(s)".to_string(),
            "remove_tag" => "remove tag(s)".to_string(),
            "delete_from_db" => "delete from database".to_string(),
            "open_file" => "open file".to_string(),
            "edit_file" => "edit file".to_string(),
            "copy_files" => "copy files".to_string(),
            "refine_search" => "refine search criteria".to_string(),
            "show_help" => "show help".to_string(),
            "select_all" => "select all".to_string(),
            "clear_selection" => "clear selection".to_string(),
            other => other.replace('_', " "),
        }
    }

    /// Render the UI
    fn render(
        frame: &mut Frame,
        state: &mut AppState,
        theme: &Theme,
        prompt: &str,
        preview_config: Option<&crate::ui::PreviewConfig>,
        preview_content: Option<&StyledPreview>,
        hints: &[KeyHint],
    ) {
        let area = frame.area();

        state.visible_height = (area.height.saturating_sub(8)) as usize;

        let main_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Search bar
                Constraint::Min(5),    // Content (items + preview)
                Constraint::Length(3), // Status bar
                Constraint::Length(1), // Help bar
            ])
            .split(area);

        // Render search bar
        let search_bar = SearchBar::new(&state.query, state.query_cursor, prompt, theme);
        frame.render_widget(search_bar, main_layout[0]);

        // Content area: items list and optional preview
        let content_area = main_layout[1];
        Self::render_content(
            frame,
            state,
            theme,
            content_area,
            preview_config,
            preview_content,
        );

        // Render status bar
        let messages: Vec<_> = state.active_messages();
        let status_bar = StatusBar::new(&messages, theme);
        frame.render_widget(status_bar, main_layout[2]);

        // Render help bar
        let help_bar = HelpBar::new(hints, theme);
        frame.render_widget(help_bar, main_layout[3]);
    }

    /// Render overlays (help, refine search, text input, etc.)
    fn render_overlays(
        frame: &mut Frame,
        state: &AppState,
        theme: &Theme,
        overlay_binds: &[(String, String)],
    ) {
        match state.mode {
            Mode::Help => {
                let help_overlay =
                    HelpOverlay::new(theme).with_custom_binds(overlay_binds.to_vec());
                frame.render_widget(help_overlay, frame.area());
            }
            Mode::RefineSearch => {
                if let Some(refine_state) = state.refine_search_state() {
                    let refine_overlay = RefineSearchOverlay::new(theme, refine_state);
                    frame.render_widget(refine_overlay, frame.area());
                }
            }
            Mode::Input => {
                if let Some(input_state) = state.text_input_state() {
                    let input_modal = TextInputModal::new(input_state, theme);
                    frame.render_widget(input_modal, frame.area());
                }
            }
            Mode::Confirm => {
                if let Some(confirm_state) = state.confirm_state() {
                    let confirm_dialog = ConfirmDialog::new(confirm_state, theme);
                    frame.render_widget(confirm_dialog, frame.area());
                }
            }
            Mode::Normal => {}
        }
    }

    /// Render the content area (items + preview OR tag tree + live results)
    fn render_content(
        frame: &mut Frame,
        state: &mut AppState,
        theme: &Theme,
        area: Rect,
        preview_config: Option<&crate::ui::PreviewConfig>,
        preview_content: Option<&StyledPreview>,
    ) {
        use super::widgets::tag_tree_with_border;
        use crate::ui::types::BrowsePhase;

        // Special rendering for TagSelection phase - show tag tree + live results
        if state.phase == BrowsePhase::TagSelection {
            // Split horizontally: tag tree (left) | result preview (right)
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(50), // Tag tree
                    Constraint::Percentage(50), // Live results
                ])
                .split(area);

            // Render tag tree on the left
            if let Some(tag_tree_state) = &mut state.tag_tree_state {
                let tag_tree = tag_tree_with_border("Tag Hierarchy");
                frame.render_stateful_widget(tag_tree, chunks[0], tag_tree_state);
            }

            // Render live results on the right (filtered file list based on selected tags)
            // For now, just show "Results will appear here" placeholder
            // TODO: Implement live result updates
            let item_list = ItemList::new(state, theme);
            frame.render_widget(item_list, chunks[1]);
            return;
        }

        // Regular FileSelection phase rendering
        let show_preview = preview_config.is_some_and(|c| c.enabled) && preview_content.is_some();

        if !show_preview {
            // Just render item list
            let item_list = ItemList::new(state, theme);
            frame.render_widget(item_list, area);
            return;
        }

        let preview_config = preview_config.unwrap();
        let width_percent = u16::from(preview_config.width_percent);

        // Split based on preview position
        let (items_area, preview_area) = match preview_config.position {
            PreviewPosition::Right => {
                let chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Percentage(100 - width_percent),
                        Constraint::Percentage(width_percent),
                    ])
                    .split(area);
                (chunks[0], chunks[1])
            }
            PreviewPosition::Bottom | PreviewPosition::Top => {
                let constraints = if preview_config.position == PreviewPosition::Top {
                    [
                        Constraint::Percentage(width_percent),
                        Constraint::Percentage(100 - width_percent),
                    ]
                } else {
                    [
                        Constraint::Percentage(100 - width_percent),
                        Constraint::Percentage(width_percent),
                    ]
                };
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints(constraints)
                    .split(area);

                if preview_config.position == PreviewPosition::Top {
                    (chunks[1], chunks[0])
                } else {
                    (chunks[0], chunks[1])
                }
            }
        };

        // Render item list
        let item_list = ItemList::new(state, theme);
        frame.render_widget(item_list, items_area);

        // Render preview pane
        let preview_pane = PreviewPane::new(preview_content, theme).scroll(state.preview_scroll);
        frame.render_widget(preview_pane, preview_area);
    }

    /// Run the finder event loop
    fn run_loop(
        &self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
        config: &FinderConfig,
    ) -> Result<FinderResult> {
        let mut state = AppState::new(config.items.clone(), config.multi_select);
        // Set available tags for autocomplete in text input modals
        state.available_tags.clone_from(&config.available_tags);

        // Set phase and initialize tag tree if in TagSelection phase
        state.phase = config.phase;
        if config.phase == crate::ui::types::BrowsePhase::TagSelection {
            use super::widgets::TagTreeState;
            let mut tag_tree_state = TagTreeState::new();

            // Build tag tree from items (extract tag names and file counts)
            let tags: Vec<(String, usize)> = config
                .items
                .iter()
                .filter_map(|item| {
                    // Extract file count from metadata.index field
                    item.metadata.index.map(|count| (item.key.clone(), count))
                })
                .collect();

            tag_tree_state.build_from_tags(tags);
            state.tag_tree_state = Some(tag_tree_state);

            // Synchronize the initial cursor position
            state.sync_cursor_with_tag_tree();
        }

        let mut nucleo = Self::create_matcher(&config.items);
        let custom_binds = Self::parse_keybinds(&config.bind);
        let hints = Self::build_hints();
        let overlay_binds = Self::build_overlay_binds(&custom_binds);
        let mut prev_query = String::new();

        // Initial filter (show all)
        state.update_filtered(Self::update_filter(&mut nucleo, "", ""));

        let mut cached_preview: Option<StyledPreview> = None;
        let mut cached_preview_key: Option<String> = None;

        loop {
            // Update preview if needed - prefer styled_generator (native ratatui) over preview_provider (ANSI)
            if let Some(preview_config) = &config.preview_config
                && preview_config.enabled
                && let Some(current_key) = state
                    .current_key()
                    .filter(|&k| cached_preview_key.as_deref() != Some(k))
            {
                // Use styled_generator for native ratatui styling
                if let Some(generator) = &self.styled_generator {
                    cached_preview = generator.generate(Path::new(current_key)).ok();
                }
                cached_preview_key = Some(current_key.to_string());
            }

            // Render
            terminal.draw(|frame| {
                Self::render(
                    frame,
                    &mut state,
                    &self.theme,
                    &config.prompt,
                    config.preview_config.as_ref(),
                    cached_preview.as_ref(),
                    &hints,
                );
                Self::render_overlays(frame, &state, &self.theme, &overlay_binds);
            })?;

            // Handle events
            let result = poll_and_handle(&mut state, &custom_binds, Duration::from_millis(50))?;

            match result {
                EventResult::Confirm(Some(ref key)) if key == "refine_search" => {
                    // Open the refine search overlay
                    let criteria = config.search_criteria.as_ref();
                    state.enter_refine_search(
                        criteria.map_or_else(Vec::new, |c| c.include_tags.clone()),
                        criteria.map_or_else(Vec::new, |c| c.exclude_tags.clone()),
                        criteria.map_or_else(Vec::new, |c| c.file_patterns.clone()),
                        criteria.map_or_else(Vec::new, |c| c.virtual_tags.clone()),
                        config.available_tags.clone(),
                    );
                }
                EventResult::Confirm(Some(ref key)) if key == "refine_search_done" => {
                    // Apply the refined search criteria - return with special action
                    if let Some(refine_state) = state.exit_refine_search() {
                        // Build a result that signals refine search was applied
                        return Ok(FinderResult::with_refine_search(
                            refine_state.include_tags,
                            refine_state.exclude_tags,
                            refine_state.file_patterns,
                            refine_state.virtual_tags,
                        ));
                    }
                }
                EventResult::Confirm(key) => {
                    state.confirm(key);
                }
                EventResult::Abort => {
                    state.abort();
                }
                EventResult::QueryChanged => {
                    let indices = Self::update_filter(&mut nucleo, &state.query, &prev_query);
                    prev_query.clone_from(&state.query);
                    state.update_filtered(indices);
                    // Reset preview cache when query changes
                    cached_preview_key = None;
                }
                EventResult::InputSubmitted { action_id, values } => {
                    // The input modal was submitted - return to caller with action info
                    return Ok(FinderResult::with_action(
                        state.selected_keys(),
                        action_id,
                        values,
                    ));
                }
                EventResult::ConfirmSubmitted { action_id, context } => {
                    // Confirmation dialog was confirmed - return to caller with action info
                    // The context contains the file paths that were selected for the action
                    return Ok(FinderResult::with_action(
                        context,
                        action_id,
                        Vec::new(), // No additional values for confirmation-only actions
                    ));
                }
                EventResult::InputCancelled
                | EventResult::ConfirmCancelled
                | EventResult::Continue
                | EventResult::Ignored => {
                    // Input/Confirmation cancelled or ignored, just continue browsing
                }
            }

            if state.should_exit {
                break;
            }

            state.cleanup_messages();
        }

        if state.aborted {
            Ok(FinderResult::aborted())
        } else {
            Ok(FinderResult::with_key(
                state.selected_keys(),
                state.final_key,
            ))
        }
    }
}

impl Default for RatatuiFinder {
    fn default() -> Self {
        Self::new()
    }
}

impl FuzzyFinder for RatatuiFinder {
    fn run(&self, config: FinderConfig) -> Result<FinderResult> {
        // Setup terminal
        let mut terminal = Self::setup_terminal()?;

        // Run the event loop, ensuring cleanup happens
        let result = self.run_loop(&mut terminal, &config);

        // Cleanup terminal (always, even on error)
        if let Err(e) = Self::cleanup_terminal() {
            // Log cleanup error but prioritize the main result
            eprintln!("Warning: terminal cleanup failed: {e}");
        }

        result
    }
}

/// Preview provider that wraps the existing `PreviewGenerator`
pub struct RatatuiPreviewProvider {
    generator: Arc<crate::preview::PreviewGenerator>,
}

impl RatatuiPreviewProvider {
    /// Create a new preview provider
    #[must_use]
    pub const fn new(generator: Arc<crate::preview::PreviewGenerator>) -> Self {
        Self { generator }
    }
}

impl PreviewProvider for RatatuiPreviewProvider {
    fn preview(&self, item: &str) -> Result<PreviewText> {
        use crate::preview::PreviewContent;
        use std::path::PathBuf;

        let path = PathBuf::from(item);
        match self.generator.generate(&path) {
            Ok(content) => {
                let display = content.to_string();
                let has_ansi = matches!(content, PreviewContent::Text { has_ansi: true, .. });
                Ok(if has_ansi {
                    PreviewText::ansi(display)
                } else {
                    PreviewText::plain(display)
                })
            }
            Err(e) => Ok(PreviewText::plain(format!("Preview error: {e}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_finder_creation() {
        let finder = RatatuiFinder::new();
        assert!(finder.preview_provider.is_none());
    }

    #[test]
    fn test_parse_key_string() {
        use crossterm::event::{KeyCode, KeyModifiers};

        let key = RatatuiFinder::parse_key_string("ctrl-t").unwrap();
        assert_eq!(key.code, KeyCode::Char('t'));
        assert!(key.modifiers.contains(KeyModifiers::CONTROL));

        let key = RatatuiFinder::parse_key_string("ctrl-/").unwrap();
        assert_eq!(key.code, KeyCode::Char('/'));
        assert!(key.modifiers.contains(KeyModifiers::CONTROL));

        let key = RatatuiFinder::parse_key_string("enter").unwrap();
        assert_eq!(key.code, KeyCode::Enter);
        assert_eq!(key.modifiers, KeyModifiers::NONE);

        let key = RatatuiFinder::parse_key_string("f1").unwrap();
        assert_eq!(key.code, KeyCode::F(1));
    }

    #[test]
    fn test_parse_keybinds() {
        let binds = vec![
            "ctrl-t:add_tag".to_string(),
            "ctrl-d:delete".to_string(),
            "enter:accept".to_string(), // Should be skipped
        ];

        let map = RatatuiFinder::parse_keybinds(&binds);
        assert_eq!(map.len(), 2);
    }
}
