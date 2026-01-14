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
        &self,
        frame: &mut Frame,
        state: &mut AppState,
        theme: &Theme,
        preview_content: Option<&StyledPreview>,
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
        let search_bar = SearchBar::new(&state.query, state.query_cursor, &state.prompt, theme);
        frame.render_widget(search_bar, main_layout[0]);

        // Content area: items list and optional preview
        let content_area = main_layout[1];
        self.render_content(frame, state, theme, content_area, preview_content);

        // Render status bar with optional CLI preview
        let messages: Vec<_> = state.active_messages();
        let cli_preview = state.build_cli_preview();
        let status_bar = StatusBar::new(&messages, theme, state.preview_mode)
            .with_cli_preview(cli_preview.as_deref());
        frame.render_widget(status_bar, main_layout[2]);

        // Render help bar
        let help_bar = HelpBar::new(&state.hints, theme);
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
    #[allow(clippy::too_many_lines)]
    fn render_content(
        &self,
        frame: &mut Frame,
        state: &mut AppState,
        theme: &Theme,
        area: Rect,
        preview_content: Option<&StyledPreview>,
    ) {
        use crate::ui::types::BrowsePhase;

        // Special rendering for TagSelection phase - show tag tree + files + preview
        if state.phase == BrowsePhase::TagSelection {
            // Split horizontally: tag tree (left 30%) | files (middle 35%) | preview (right 35%)
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(30), // Tag tree
                    Constraint::Percentage(35), // File list
                    Constraint::Percentage(35), // Preview
                ])
                .split(area);

            // Render tag tree on the left with focus indicator
            if let Some(tag_tree_state) = &mut state.tag_tree_state {
                let is_focused = state.focused_pane == super::state::FocusPane::TagTree;
                let (border_style, title_style) = if is_focused {
                    (theme.focused_border_style(), theme.focused_title_style())
                } else {
                    (theme.border_style(), theme.unfocused_title_style())
                };

                let tag_tree = super::widgets::TagTree::new().block(
                    ratatui::widgets::Block::default()
                        .borders(ratatui::widgets::Borders::ALL)
                        .border_style(border_style)
                        .title(ratatui::text::Span::styled(" Tags ", title_style)),
                );
                frame.render_stateful_widget(tag_tree, chunks[0], tag_tree_state);
            }

            // Render file list in the middle with focus indicator
            let is_file_focused = state.focused_pane == super::state::FocusPane::FilePreview;
            let (file_border_style, file_title_style) = if is_file_focused {
                (theme.focused_border_style(), theme.focused_title_style())
            } else {
                (theme.border_style(), theme.unfocused_title_style())
            };

            // Create a block for the file list
            let file_block = ratatui::widgets::Block::default()
                .borders(ratatui::widgets::Borders::ALL)
                .border_style(file_border_style)
                .title(ratatui::text::Span::styled(" Files ", file_title_style));

            let inner = file_block.inner(chunks[1]);
            frame.render_widget(file_block, chunks[1]);

            // Render file list directly using file_preview data from state
            Self::render_file_preview_list(frame, state, theme, inner);

            // Render preview pane on the right
            let preview_block = ratatui::widgets::Block::default()
                .borders(ratatui::widgets::Borders::ALL)
                .border_style(theme.border_style())
                .title(" Preview ");

            let preview_inner = preview_block.inner(chunks[2]);
            frame.render_widget(preview_block, chunks[2]);

            // Show preview if we have content and files to preview
            if !state.file_preview_items.is_empty() && preview_content.is_some() {
                let preview_pane =
                    PreviewPane::new(preview_content, theme).scroll(state.preview_scroll);
                frame.render_widget(preview_pane, preview_inner);
            }
            return;
        }

        // Regular FileSelection phase rendering
        let show_preview =
            state.preview_config.as_ref().is_some_and(|c| c.enabled) && preview_content.is_some();

        if !show_preview {
            // Just render item list
            let item_list = ItemList::new(state, theme);
            frame.render_widget(item_list, area);
            return;
        }

        let preview_config = state.preview_config.as_ref().unwrap();
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

    /// Render file preview list (for `TagSelection` phase)
    ///
    /// This renders the file list in the middle pane with proper key-based selection.
    fn render_file_preview_list(frame: &mut Frame, state: &AppState, theme: &Theme, area: Rect) {
        use ratatui::style::Color;
        use ratatui::text::{Line, Span};
        use ratatui::widgets::{List, ListItem};

        if area.height == 0 {
            return;
        }

        let visible_height = area.height as usize;
        let start = state.file_preview_scroll;
        let end = (start + visible_height).min(state.file_preview_items.len());

        let items: Vec<ListItem> = state.file_preview_items[start..end]
            .iter()
            .enumerate()
            .map(|(visible_idx, item)| {
                let is_cursor = start + visible_idx == state.file_preview_cursor;
                let is_selected = state.is_file_preview_selected_key(&item.key);

                // Build prefix: cursor indicator + selection indicator
                let cursor_char = if is_cursor { ">" } else { " " };

                let mut spans = vec![
                    Span::styled(cursor_char, theme.cursor_style()),
                    Span::raw(" "),
                ];

                // Green checkmark for selected items
                if is_selected {
                    spans.push(Span::styled(
                        "✓",
                        ratatui::style::Style::default().fg(Color::Green),
                    ));
                    spans.push(Span::raw(" "));
                } else {
                    spans.push(Span::raw("  "));
                }

                spans.push(Span::raw(" "));

                // Add the display text
                let text_style = if is_cursor {
                    theme.selected_style()
                } else {
                    theme.normal_style()
                };

                // Use just the filename for display
                let display = std::path::Path::new(&item.key)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(&item.key);
                spans.push(Span::styled(display.to_string(), text_style));

                // Add right-aligned note indicator if file has a note
                if item.metadata.has_note {
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(
                        "",
                        ratatui::style::Style::default().fg(Color::Cyan).dim(),
                    ));
                }

                let line = Line::from(spans);

                if is_cursor {
                    ListItem::new(line).style(theme.selected_style())
                } else {
                    ListItem::new(line)
                }
            })
            .collect();

        let list = List::new(items);
        frame.render_widget(list, area);
    }

    /// Run the finder event loop
    #[allow(clippy::too_many_lines)]
    fn run_loop(
        &self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
        config: &FinderConfig,
    ) -> Result<FinderResult> {
        let hints = Self::build_hints();
        let mut state = AppState::new(
            config.items.clone(),
            config.multi_select,
            config.tag_schema.clone(),
            config.database.clone(),
            config.prompt.clone(),
            hints,
            config.preview_config.clone(),
        );
        // Set available tags for autocomplete in text input modals
        state.available_tags.clone_from(&config.available_tags);

        // Set phase and initialize tag tree if in TagSelection phase
        state.phase = config.phase;
        if config.phase == crate::ui::types::BrowsePhase::TagSelection {
            use super::widgets::TagTreeState;
            let mut tag_tree_state = TagTreeState::new();

            // Build tag tree from items (extract tag names, file counts, and display info)
            let tags: Vec<(String, usize)> = config
                .items
                .iter()
                .filter_map(|item| {
                    // Extract file count from metadata.index field
                    item.metadata.index.map(|count| (item.key.clone(), count))
                })
                .collect();

            // Build a map of tag -> display text for alias information
            let display_map: std::collections::HashMap<String, String> = config
                .items
                .iter()
                .map(|item| (item.key.clone(), item.display.clone()))
                .collect();

            tag_tree_state.build_from_tags_with_display(&tags, &display_map);
            state.tag_tree_state = Some(tag_tree_state);

            // Synchronize the initial cursor position
            state.sync_cursor_with_tag_tree();

            // Initialize file preview (empty at start)
            state.update_file_preview();

            // If search criteria with actual tag filters were provided, start in file pane
            let has_tag_filters = config
                .search_criteria
                .as_ref()
                .is_some_and(|c| !c.include_tags.is_empty() || !c.exclude_tags.is_empty());

            if has_tag_filters {
                use super::state::FocusPane;
                state.focused_pane = FocusPane::FilePreview;
            }
        }

        let mut nucleo = Self::create_matcher(&config.items);
        let custom_binds = Self::parse_keybinds(&config.bind);
        let overlay_binds = Self::build_overlay_binds(&custom_binds);
        let mut prev_query = String::new();
        let mut prev_file_query = String::new();

        // Initial filter (show all)
        state.update_filtered(Self::update_filter(&mut nucleo, "", ""));

        let mut cached_preview: Option<StyledPreview> = None;
        let mut cached_preview_key: Option<String> = None;
        let mut cached_preview_mode: Option<crate::ui::ratatui_adapter::state::PreviewMode> = None;

        loop {
            // Update preview if needed - prefer styled_generator (native ratatui) over preview_provider (ANSI)
            if let Some(preview_config) = &config.preview_config
                && preview_config.enabled
            {
                use crate::ui::types::BrowsePhase;

                // Get the file path to preview (phase-aware)
                let preview_file_key = match state.phase {
                    BrowsePhase::TagSelection => {
                        // In tag selection, preview the file at file_preview_cursor
                        state
                            .file_preview_items
                            .get(state.file_preview_cursor)
                            .map(|item| item.key.as_str())
                    }
                    BrowsePhase::FileSelection => {
                        // In file selection, use current_key
                        state.current_key()
                    }
                };

                if let Some(current_key) = preview_file_key {
                    // Regenerate preview if:
                    // 1. File changed (cached_preview_key != current_key), OR
                    // 2. Preview mode changed (cached_preview_mode != state.preview_mode)
                    let should_regenerate = cached_preview_key.as_deref() != Some(current_key)
                        || cached_preview_mode != Some(state.preview_mode);

                    if should_regenerate {
                        // Generate preview based on preview mode
                        use crate::ui::ratatui_adapter::state::PreviewMode;
                        cached_preview = match state.preview_mode {
                            PreviewMode::File => {
                                // Use styled_generator for native ratatui styling
                                self.styled_generator.as_ref().and_then(|generator| {
                                    generator.generate(Path::new(current_key)).ok()
                                })
                            }
                            PreviewMode::Note => {
                                // Generate note preview from database
                                // Notes are stored with canonical paths, so canonicalize before lookup
                                let note_preview = state
                                    .database
                                    .as_ref()
                                    .and_then(|db| {
                                        Path::new(current_key).canonicalize().ok().and_then(
                                            |canonical_path| {
                                                db.get_note(&canonical_path).ok().flatten()
                                            },
                                        )
                                    })
                                    .map(|note| StyledPreview::note(&note))
                                    .unwrap_or_else(StyledPreview::no_note);
                                Some(note_preview)
                            }
                        };
                        cached_preview_key = Some(current_key.to_string());
                        cached_preview_mode = Some(state.preview_mode);
                    }
                }
            }

            // Render
            terminal.draw(|frame| {
                self.render(frame, &mut state, &self.theme, cached_preview.as_ref());
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

                    // In TagSelection phase, filter BOTH panes simultaneously
                    if state.is_tag_selection_phase() {
                        // Filter file preview items (right pane) from the unfiltered list
                        if !state.file_preview_items_unfiltered.is_empty() {
                            let mut temp_file_nucleo: Nucleo<u32> = Nucleo::new(
                                Config::DEFAULT.match_paths(),
                                Arc::new(|| {}),
                                None,
                                1,
                            );

                            let file_injector = temp_file_nucleo.injector();
                            for (idx, item) in
                                state.file_preview_items_unfiltered.iter().enumerate()
                            {
                                #[allow(clippy::cast_possible_truncation)]
                                let _ = file_injector.push(idx as u32, |_, cols| {
                                    cols[0] = item.searchable.clone().into();
                                });
                            }

                            let file_indices = Self::update_filter(
                                &mut temp_file_nucleo,
                                &state.query,
                                &prev_file_query,
                            );
                            prev_file_query.clone_from(&state.query);

                            state.file_preview_items = file_indices
                                .iter()
                                .filter_map(|&idx| {
                                    state
                                        .file_preview_items_unfiltered
                                        .get(idx as usize)
                                        .cloned()
                                })
                                .collect();

                            if state.file_preview_cursor >= state.file_preview_items.len() {
                                state.file_preview_cursor =
                                    state.file_preview_items.len().saturating_sub(1);
                            }
                            state.file_preview_scroll = 0;
                        }
                    }
                    // Reset preview cache when query changes
                    cached_preview_key = None;
                }
                EventResult::PreviewChanged => {
                    // Preview mode toggled - invalidate cache to force regeneration
                    cached_preview_mode = None;
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
            let direct_file_selection = state.is_direct_file_selection();
            let selected_tags = if direct_file_selection {
                state.get_filtering_tags()
            } else {
                Vec::new()
            };
            Ok(FinderResult::with_key_and_direct_selection(
                state.selected_keys(),
                state.final_key.clone(),
                direct_file_selection,
                selected_tags,
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
