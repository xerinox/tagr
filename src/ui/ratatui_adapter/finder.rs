//! Ratatui-based fuzzy finder implementation
//!
//! Implements the `FuzzyFinder` trait using ratatui for UI and nucleo for matching.

use super::events::{EventResult, KeybindMap, poll_and_handle};
use super::state::{AppState, Mode};
use super::styled_preview::{StyledPreview, StyledPreviewGenerator};
use super::theme::Theme;
use super::widgets::TagTreeState;
use super::widgets::{
    ConfirmDialog, DetailsModal, HelpBar, HelpOverlay, KeyHint, PreviewPane, RefineSearchOverlay,
    SearchBar, StatusBar, TextInputModal,
};
use crate::commands::note::create_temp_note_file;
use crate::keybinds::actions::BrowseAction;
use crate::ui::error::Result;
use crate::ui::traits::{FinderConfig, FuzzyFinder, PreviewProvider, PreviewText};
use crate::ui::types::FinderResult;
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
    /// Daemon event receiver for live updates (only in Remote mode).
    /// Wrapped in `RefCell` to allow taking from `&self` (`FuzzyFinder` trait requires `&self`).
    event_rx: std::cell::RefCell<Option<tokio::sync::mpsc::Receiver<crate::ipc::wire::ServerEvent>>>,
}

impl RatatuiFinder {
    /// Create a new ratatui finder
    #[must_use]
    pub fn new() -> Self {
        Self {
            preview_provider: None,
            styled_generator: None,
            theme: Theme::default(),
            event_rx: std::cell::RefCell::new(None),
        }
    }

    /// Create a ratatui finder with native styled preview generator
    #[must_use]
    pub fn with_styled_preview() -> Self {
        Self {
            preview_provider: None,
            styled_generator: Some(StyledPreviewGenerator::new()),
            theme: Theme::default(),
            event_rx: std::cell::RefCell::new(None),
        }
    }

    /// Create a ratatui finder with legacy preview provider
    #[must_use]
    pub fn with_preview_provider(preview_provider: impl PreviewProvider + 'static) -> Self {
        Self {
            preview_provider: Some(Arc::new(preview_provider)),
            styled_generator: None,
            theme: Theme::default(),
            event_rx: std::cell::RefCell::new(None),
        }
    }

    /// Set the daemon event receiver for live updates in Remote mode.
    #[must_use]
    pub fn with_event_receiver(
        self,
        rx: tokio::sync::mpsc::Receiver<crate::ipc::wire::ServerEvent>,
    ) -> Self {
        *self.event_rx.borrow_mut() = Some(rx);
        self
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
    ///
    /// # Examples
    /// ```ignore
    /// let key = RatatuiFinder::parse_key_string("ctrl-t");
    /// assert!(key.is_some());
    /// ```
    pub(crate) fn parse_key_string(s: &str) -> Option<KeyEvent> {
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
        // Use the metadata registry to get proper action descriptions
        use crate::keybinds::metadata::{ActionMetadata, ActionRegistry};

        let mut binds: Vec<(String, String)> = custom_binds
            .iter()
            .filter_map(|(key, action)| {
                // Get human-readable key format
                super::events::key_to_string(key).and_then(|key_str| {
                    // Get description from metadata registry
                    ActionRegistry::get_by_id(action).map(|meta| {
                        (
                            ActionMetadata::format_key(&key_str),
                            meta.short_name.to_string(),
                        )
                    })
                })
            })
            .collect();

        // Sort by key for consistent display
        binds.sort_by(|a, b| a.0.cmp(&b.0));

        // Add preview scroll hint (always available)
        binds.push(("Shift+↑/↓".to_string(), "scroll preview".to_string()));

        binds
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
            Mode::Details => {
                if let Some(file_details) = state.file_details() {
                    let details_modal = DetailsModal::new(file_details, theme);
                    frame.render_widget(details_modal, frame.area());
                }
            }
            Mode::Normal => {}
        }
    }

    /// Render the content area (items + preview OR tag tree + live results)
    #[allow(clippy::too_many_lines, clippy::unused_self)]
    fn render_content(
        &self,
        frame: &mut Frame,
        state: &mut AppState,
        theme: &Theme,
        area: Rect,
        preview_content: Option<&StyledPreview>,
    ) {
        let preview_enabled = state.preview_config.as_ref().is_some_and(|c| c.enabled);

        if preview_enabled {
            // Determine preview width from config (default 35%)
            let preview_width = state
                .preview_config
                .as_ref()
                .map_or(35_u16, |cfg| u16::from(cfg.width_percent));
            let remaining = 100_u16.saturating_sub(preview_width);
            let tag_width = remaining * 45 / 100;
            let file_width = remaining.saturating_sub(tag_width);

            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(tag_width),
                    Constraint::Percentage(file_width),
                    Constraint::Percentage(preview_width),
                ])
                .split(area);

            Self::render_tag_tree(frame, state, theme, chunks[0]);
            Self::render_file_list_pane(frame, state, theme, chunks[1]);
            Self::render_preview_pane(frame, state, theme, chunks[2], preview_content);
        } else {
            // No preview: 2-pane layout (tag tree | files)
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(35),
                    Constraint::Percentage(65),
                ])
                .split(area);

            Self::render_tag_tree(frame, state, theme, chunks[0]);
            Self::render_file_list_pane(frame, state, theme, chunks[1]);
        }
    }

    fn render_tag_tree(frame: &mut Frame, state: &mut AppState, theme: &Theme, area: Rect) {
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
            frame.render_stateful_widget(tag_tree, area, tag_tree_state);
        }
    }

    fn render_file_list_pane(frame: &mut Frame, state: &mut AppState, theme: &Theme, area: Rect) {
        let is_file_focused = state.focused_pane == super::state::FocusPane::FilePreview;
        let (file_border_style, file_title_style) = if is_file_focused {
            (theme.focused_border_style(), theme.focused_title_style())
        } else {
            (theme.border_style(), theme.unfocused_title_style())
        };

        let file_block = ratatui::widgets::Block::default()
            .borders(ratatui::widgets::Borders::ALL)
            .border_style(file_border_style)
            .title(ratatui::text::Span::styled(" Files ", file_title_style));

        let inner = file_block.inner(area);
        frame.render_widget(file_block, area);
        Self::render_file_preview_list(frame, state, theme, inner);
    }

    fn render_preview_pane(
        frame: &mut Frame,
        state: &AppState,
        theme: &Theme,
        area: Rect,
        preview_content: Option<&StyledPreview>,
    ) {
        let preview_block = ratatui::widgets::Block::default()
            .borders(ratatui::widgets::Borders::ALL)
            .border_style(theme.border_style())
            .title(" Preview ");

        let preview_inner = preview_block.inner(area);
        frame.render_widget(preview_block, area);

        if !state.file_preview_items.is_empty() && preview_content.is_some() {
            let preview_pane =
                PreviewPane::new(preview_content, theme).scroll(state.preview_scroll);
            frame.render_widget(preview_pane, preview_inner);
        }
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

                // Green checkmark for selected items (outside of highlight, like tag tree)
                if is_selected {
                    spans.push(Span::styled(
                        "✓ ",
                        ratatui::style::Style::default().fg(Color::Green),
                    ));
                } else {
                    spans.push(Span::raw("  "));
                }

                // Add the display text (only this gets highlighted)
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
                ListItem::new(line)
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
        state.load_note_cache();
        // Set available tags for autocomplete in text input modals
        state.available_tags.clone_from(&config.available_tags);

        // Always initialize tag tree (3-pane layout)
        let mut tag_tree_state = TagTreeState::new();

        // Build tag tree from database (always available in 3-pane layout)
        if let Some(database) = &config.database {
            // Get all tags and compute file counts
            let all_tags = database.list_all_tags().unwrap_or_default();
            let tags_with_counts: Vec<(String, usize)> = all_tags
                .into_iter()
                .filter_map(|tag| {
                    database
                        .find_by_tag(&tag)
                        .ok()
                        .map(|files| (tag.to_string(), files.len()))
                })
                .collect();

            // Build display map from schema (for aliases)
            let display_map: std::collections::HashMap<String, String> =
                config.tag_schema.as_ref().map_or_else(
                    || {
                        tags_with_counts
                            .iter()
                            .map(|(tag, _)| (tag.clone(), tag.clone()))
                            .collect()
                    },
                    |schema| {
                        tags_with_counts
                            .iter()
                            .map(|(tag, _)| {
                                let canonical = schema.canonicalize(tag);
                                let display = if canonical == tag.as_str() {
                                    tag.clone()
                                } else {
                                    format!("{tag} ({canonical})")
                                };
                                (tag.clone(), display)
                            })
                            .collect()
                    },
                );

            tag_tree_state.build_from_tags_with_display(&tags_with_counts, &display_map);

            // Pre-select tags from search criteria (e.g., from -t flag)
            // For parent tags (e.g., "notes"), select children instead (notes:markdown, etc.)
            // Only insert the tag itself if it's a leaf (no descendants), so parent-only
            // nodes don't get stuck in selected_tags (they can't be deselected via toggle).
            if let Some(criteria) = &config.search_criteria {
                for tag in &criteria.include_tags {
                    let descendants = tag_tree_state.get_all_descendant_tags(tag);
                    if descendants.is_empty() {
                        tag_tree_state.selected_tags.insert(tag.clone());
                    } else {
                        for child in descendants {
                            tag_tree_state.selected_tags.insert(child);
                        }
                    }
                }
            }
        }
        state.tag_tree_state = Some(tag_tree_state);

        // Sync active_filter from tag tree selections (for CLI preview)
        state.sync_filter_from_tag_tree();

        // Synchronize the initial cursor position
        state.sync_cursor_with_tag_tree();

        // Initialize file preview (empty at start)
        state.update_file_preview();

        // If search criteria with actual tag filters were provided, start with file pane focused
        let has_tag_filters = config
            .search_criteria
            .as_ref()
            .is_some_and(|c| !c.include_tags.is_empty() || !c.exclude_tags.is_empty());

        if has_tag_filters {
            use super::state::FocusPane;
            state.focused_pane = FocusPane::FilePreview;
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

        // Take daemon event receiver out of RefCell (consumed for this run)
        let mut daemon_event_rx = self.event_rx.borrow_mut().take();

        loop {
            // Drain daemon events (non-blocking)
            Self::drain_daemon_events(&mut daemon_event_rx, &mut state, &mut cached_preview_key, &mut cached_preview_mode);

            // Update preview if needed - prefer styled_generator (native ratatui) over preview_provider (ANSI)
            if let Some(preview_config) = &config.preview_config
                && preview_config.enabled
            {
                // Always preview the file at file_preview_cursor (3-pane layout)
                let preview_file_key = state
                    .file_preview_items
                    .get(state.file_preview_cursor)
                    .map(|item| item.key.as_str());

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
                                let note_preview = state
                                    .cached_note(Path::new(current_key))
                                    .map_or_else(StyledPreview::no_note, StyledPreview::note);
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
                EventResult::Action {
                    action: BrowseAction::EditNote,
                    context,
                } => {
                    // Suspend TUI to edit note
                    Self::cleanup_terminal()?;

                    // Use the captured context (selected file when action was triggered)
                    let file_to_edit = context.first().map(std::path::PathBuf::from);

                    if let Some(file_path) = file_to_edit {
                        // Get canonical path
                        if let Ok(canonical_path) = file_path.canonicalize() {
                            // Get editor from environment
                            let editor =
                                std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());

                            // Get existing note or create new one
                            let existing_note = state
                                .cached_note(&canonical_path)
                                .cloned();

                            let initial_content = existing_note
                                .as_ref()
                                .map(|n| n.content.clone())
                                .unwrap_or_default();

                            // Create temp file
                            if let Ok(temp_path) = create_temp_note_file(&initial_content) {
                                // Open editor
                                let status =
                                    std::process::Command::new(&editor).arg(&temp_path).status();

                                if let Ok(status) = status
                                    && status.success()
                                {
                                    // Read updated content
                                    if let Ok(updated_content) = std::fs::read_to_string(&temp_path)
                                    {
                                        // Save or delete note based on content
                                        if let Some(ds) = state.database.as_ref() {
                                            let is_empty = updated_content.trim().is_empty();
                                            let tagr_path = crate::types::TagrPath::new(&canonical_path);

                                            if is_empty && existing_note.is_some() {
                                                // Delete note if content cleared
                                                if let Ok(tp) = &tagr_path {
                                                    let _ = ds.delete_note(tp);
                                                }
                                                state.note_cache.remove(&canonical_path);

                                                // Update has_note metadata
                                                if state.is_tag_selection_phase() {
                                                    if let Some(item) = state
                                                        .file_preview_items
                                                        .get_mut(state.file_preview_cursor)
                                                    {
                                                        item.metadata.has_note = false;
                                                    }
                                                    if let Some(item) = state
                                                        .file_preview_items_unfiltered
                                                        .iter_mut()
                                                        .find(|i| {
                                                            i.key == file_path.to_string_lossy()
                                                        })
                                                    {
                                                        item.metadata.has_note = false;
                                                    }
                                                } else if let Some(item) =
                                                    state.items.get_mut(state.cursor)
                                                {
                                                    item.metadata.has_note = false;
                                                }
                                            } else if !is_empty {
                                                // Save note only if content is not empty
                                                let note = if let Some(mut existing) = existing_note
                                                {
                                                    existing.update_content(updated_content);
                                                    existing
                                                } else {
                                                    crate::types::NoteRecord::new(updated_content)
                                                };

                                                let _ = tagr_path.as_ref().map(|tp| ds.set_note(tp, &note));
                                                state.note_cache.insert(canonical_path.clone(), note);

                                                // Update has_note metadata for the current item
                                                if state.is_tag_selection_phase() {
                                                    if let Some(item) = state
                                                        .file_preview_items
                                                        .get_mut(state.file_preview_cursor)
                                                    {
                                                        item.metadata.has_note = true;
                                                    }
                                                    if let Some(item) = state
                                                        .file_preview_items_unfiltered
                                                        .iter_mut()
                                                        .find(|i| {
                                                            i.key == file_path.display().to_string()
                                                        })
                                                    {
                                                        item.metadata.has_note = true;
                                                    }
                                                } else if let Some(current_idx) =
                                                    state.filtered_indices.get(state.cursor)
                                                    && let Some(item) =
                                                        state.items.get_mut(*current_idx as usize)
                                                {
                                                    item.metadata.has_note = true;
                                                }
                                            }
                                            // else: empty content and no existing note - don't create

                                            // Invalidate preview cache to show updated note
                                            cached_preview_key = None;
                                            cached_preview_mode = None;
                                        }
                                    }
                                }
                                let _ = std::fs::remove_file(&temp_path);
                            }
                        }
                    }

                    // Resume TUI
                    *terminal = Self::setup_terminal()?;
                }
                EventResult::Action {
                    action: BrowseAction::RefineSearch,
                    context: _,
                } => {
                    // Open the refine search overlay populated with CURRENT live state
                    let include_tags = state.tag_tree_selected_tags();
                    let exclude_tags = state
                        .active_filter
                        .flat_exclude_tags()
                        .unwrap_or_default()
                        .into_iter()
                        .map(|t| t.as_str().to_owned())
                        .collect();
                    let file_patterns = state.active_filter.file_patterns.clone();
                    let virtual_tags = state.active_filter.virtual_tags.clone();
                    state.enter_refine_search(
                        include_tags,
                        exclude_tags,
                        file_patterns,
                        virtual_tags,
                        config.available_tags.clone(),
                    );
                }
                EventResult::RefineSearchDone => {
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
                EventResult::Action { action, context }
                | EventResult::ConfirmSubmitted { action, context } => {
                    return Ok(FinderResult::with_action(
                        context,
                        action.as_str().to_string(),
                        Vec::new(),
                    ));
                }
                EventResult::Confirm => {
                    state.confirm(None);
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
                        // Filter tag tree (left pane) based on query
                        if let Some(ref mut tag_tree_state) = state.tag_tree_state {
                            if state.query.is_empty() {
                                // No query - show all tags
                                tag_tree_state.rebuild_visible_cache();
                            } else {
                                // Filter tags by fuzzy matching on name
                                let matching_tags: Vec<String> = tag_tree_state
                                    .all_tag_paths()
                                    .into_iter()
                                    .filter(|tag| {
                                        // Simple substring match (case-insensitive)
                                        tag.to_lowercase().contains(&state.query.to_lowercase())
                                    })
                                    .collect();
                                tag_tree_state.filter_visible_tags(&matching_tags);
                            }
                        }

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
                EventResult::InputSubmitted {
                    action,
                    values,
                    context,
                } => {
                    // The input modal was submitted - return to caller with action info
                    // Use the stored context (selected files when modal was opened)
                    return Ok(FinderResult::with_action(
                        context,
                        action.as_str().to_string(),
                        values,
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

    /// Drain all pending daemon events and apply them to state.
    /// Non-blocking — processes whatever is available right now.
    fn drain_daemon_events(
        event_rx: &mut Option<tokio::sync::mpsc::Receiver<crate::ipc::wire::ServerEvent>>,
        state: &mut AppState,
        cached_preview_key: &mut Option<String>,
        cached_preview_mode: &mut Option<crate::ui::ratatui_adapter::state::PreviewMode>,
    ) {
        let Some(rx) = event_rx else { return };

        let mut tag_tree_dirty = false;
        let mut file_preview_dirty = false;

        while let Ok(event) = rx.try_recv() {
            match event {
                crate::ipc::wire::ServerEvent::FileTagged { .. }
                | crate::ipc::wire::ServerEvent::FileUntagged { .. }
                | crate::ipc::wire::ServerEvent::FileRemoved { .. }
                | crate::ipc::wire::ServerEvent::ConfigReloaded => {
                    tag_tree_dirty = true;
                    file_preview_dirty = true;
                }
                crate::ipc::wire::ServerEvent::NoteChanged { file, content } => {
                    let path = std::path::PathBuf::from(&file);
                    if let Some(content) = content {
                        let record = crate::types::NoteRecord::new(content);
                        // Insert canonical form too for consistent lookups
                        if let Ok(canonical) = path.canonicalize()
                            && canonical != path
                        {
                            state.note_cache.insert(canonical, record.clone());
                        }
                        state.note_cache.insert(path, record);
                    } else if let Ok(canonical) = path.canonicalize() {
                        state.note_cache.remove(&canonical);
                        state.note_cache.remove(&path);
                    } else {
                        state.note_cache.remove(&path);
                    }
                    // Invalidate preview cache so the note preview regenerates
                    *cached_preview_key = None;
                    *cached_preview_mode = None;
                    file_preview_dirty = true;
                }
            }
        }

        if tag_tree_dirty
            && let Some(ds) = &state.database
            && let Ok(all_tags) = ds.list_all_tags()
        {
            let tags_with_counts: Vec<(String, usize)> = all_tags
                .into_iter()
                .filter_map(|tag| {
                    ds.find_by_tag(&tag).ok().map(|files| (tag.to_string(), files.len()))
                })
                .collect();

            let display_map: std::collections::HashMap<String, String> =
                state.tag_schema.as_ref().map_or_else(
                    || {
                        tags_with_counts
                            .iter()
                            .map(|(tag, _)| (tag.clone(), tag.clone()))
                            .collect()
                    },
                    |schema| {
                        tags_with_counts
                            .iter()
                            .map(|(tag, _)| {
                                let canonical = schema.canonicalize(tag);
                                let display = if canonical == tag.as_str() {
                                    tag.clone()
                                } else {
                                    format!("{tag} ({canonical})")
                                };
                                (tag.clone(), display)
                            })
                            .collect()
                    },
                );

            if let Some(tree) = &mut state.tag_tree_state {
                tree.build_from_tags_with_display(&tags_with_counts, &display_map);
            }
        }

        if file_preview_dirty {
            // Refresh has_note flags on file preview items
            for item in &mut state.file_preview_items {
                let path = std::path::Path::new(&item.key);
                item.metadata.has_note = state.note_cache.contains_key(path);
            }
            for item in &mut state.file_preview_items_unfiltered {
                let path = std::path::Path::new(&item.key);
                item.metadata.has_note = state.note_cache.contains_key(path);
            }
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
