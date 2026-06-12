//! Watch rules modal widget for displaying daemon status and active watch rules

use crate::ui::ratatui_adapter::state::StoreMode;
use crate::ui::ratatui_adapter::theme::Theme;
use crate::watch::WatchRule;
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap},
};
use std::path::PathBuf;

/// Daemon status information gathered at modal open time
#[derive(Debug, Clone)]
pub struct DaemonInfo {
    /// Whether the daemon is reachable (Ping succeeded)
    pub running: bool,
    /// IPC socket path
    pub socket_path: Option<PathBuf>,
    /// Daemon process ID (if discoverable)
    pub pid: Option<u32>,
    /// Current store mode in the TUI session
    pub store_mode: StoreMode,
}

impl DaemonInfo {
    /// Probe daemon status by checking socket and sending Ping
    #[must_use]
    pub fn probe(store_mode: StoreMode) -> Self {
        let socket_path = crate::ipc::get_ipc_socket_path().ok();

        let socket_exists = socket_path.as_ref().is_some_and(|p| p.exists());

        let (running, pid) = if socket_exists {
            let rt_result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            rt_result.map_or((false, None), |rt| {
                let is_up = rt.block_on(async {
                    crate::daemon::client::send_request(crate::ipc::wire::Request::Ping)
                        .await
                        .is_ok()
                });
                let pid = Self::find_daemon_pid();
                (is_up, pid)
            })
        } else {
            (false, None)
        };

        Self {
            running,
            socket_path,
            pid,
            store_mode,
        }
    }

    /// Try to find the daemon PID from /proc (Linux-only best effort)
    #[cfg(target_os = "linux")]
    fn find_daemon_pid() -> Option<u32> {
        use std::fs;
        // Look for a "tagr" process with "watch start --daemon" in cmdline
        let entries = fs::read_dir("/proc").ok()?;
        for entry in entries.flatten() {
            let name = entry.file_name();
            let pid_str = name.to_str()?;
            if pid_str.chars().all(|c| c.is_ascii_digit()) {
                let cmdline_path = entry.path().join("cmdline");
                if let Ok(cmdline) = fs::read_to_string(&cmdline_path)
                    && cmdline.contains("tagr")
                    && cmdline.contains("--daemon")
                {
                    return pid_str.parse().ok();
                }
            }
        }
        None
    }

    #[cfg(not(target_os = "linux"))]
    fn find_daemon_pid() -> Option<u32> {
        None
    }
}

/// State for the watch rules modal (tracks scroll position)
#[derive(Debug, Clone)]
pub struct WatchRulesState {
    /// Loaded watch rules
    pub rules: Vec<WatchRule>,
    /// Daemon status information
    pub daemon_info: DaemonInfo,
    /// Current scroll offset
    pub scroll: usize,
}

impl WatchRulesState {
    /// Create state from loaded rules and daemon probe
    #[must_use]
    pub const fn new(rules: Vec<WatchRule>, daemon_info: DaemonInfo) -> Self {
        Self {
            rules,
            daemon_info,
            scroll: 0,
        }
    }

    /// Scroll up by one line
    pub const fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    /// Scroll down by one line (clamped to content)
    pub fn scroll_down(&mut self, visible_height: usize) {
        let content_lines = self.content_line_count();
        if content_lines > visible_height {
            self.scroll = self
                .scroll
                .min(content_lines - visible_height)
                .saturating_add(1);
            self.scroll = self
                .scroll
                .min(content_lines.saturating_sub(visible_height));
        }
    }

    /// Estimate number of rendered content lines
    fn content_line_count(&self) -> usize {
        // Daemon info section: header + status + socket + pid + mode + separator
        let mut lines = 8;
        if self.rules.is_empty() {
            return lines + 3;
        }
        // rules header
        lines += 3;
        for rule in &self.rules {
            lines += 2; // "Rule #N" + blank
            lines += 1; // patterns line
            lines += 1; // tags line
            if rule.filter.is_some() {
                lines += 1;
            }
            if !rule.vtags.is_empty() {
                lines += 1;
            }
            if !rule.filter_by_tags.is_empty() {
                lines += 1;
            }
            lines += 1; // blank separator
        }
        lines + 2 // footer
    }
}

/// Watch rules modal widget
pub struct WatchRulesModal<'a> {
    /// State with rules and scroll position
    state: &'a WatchRulesState,
    /// Theme for styling
    theme: &'a Theme,
}

impl<'a> WatchRulesModal<'a> {
    /// Create a new watch rules modal
    #[must_use]
    pub const fn new(state: &'a WatchRulesState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }

    /// Calculate centered area for the modal
    fn centered_rect(width_percent: u16, height_percent: u16, area: Rect) -> Rect {
        let w = width_percent.min(90);
        let h = height_percent.min(90);
        let popup_layout = Layout::vertical([
            Constraint::Percentage((100 - h) / 2),
            Constraint::Percentage(h),
            Constraint::Percentage((100 - h) / 2),
        ])
        .split(area);

        Layout::horizontal([
            Constraint::Percentage((100 - w) / 2),
            Constraint::Percentage(w),
            Constraint::Percentage((100 - w) / 2),
        ])
        .split(popup_layout[1])[1]
    }

    /// Build daemon status lines
    fn build_daemon_section(&self) -> Vec<Line<'static>> {
        let label_style = Style::default().fg(Color::DarkGray);
        let value_style = Style::default().fg(Color::White);
        let heading_style = Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD);
        let dim_style = Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC);

        let mut lines = Vec::new();
        let info = &self.state.daemon_info;

        lines.push(Line::default());
        lines.push(Line::from(Span::styled("Daemon Status", heading_style)));
        lines.push(Line::from("─".repeat(50)));

        let (status_text, status_style) = if info.running {
            (
                "● running",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            ("○ stopped", Style::default().fg(Color::Red))
        };
        lines.push(Line::from(vec![
            Span::styled("  Status:    ", label_style),
            Span::styled(status_text, status_style),
        ]));

        let mode_text = match info.store_mode {
            StoreMode::Daemon => "IPC (daemon-backed)",
            StoreMode::Local => "Direct (sled)",
        };
        lines.push(Line::from(vec![
            Span::styled("  Store:     ", label_style),
            Span::styled(mode_text, value_style),
        ]));

        if let Some(pid) = info.pid {
            lines.push(Line::from(vec![
                Span::styled("  PID:       ", label_style),
                Span::styled(pid.to_string(), value_style),
            ]));
        }

        if let Some(ref path) = info.socket_path {
            lines.push(Line::from(vec![
                Span::styled("  Socket:    ", label_style),
                Span::styled(path.display().to_string(), dim_style),
            ]));
        }

        lines.push(Line::default());
        lines
    }

    /// Build content lines
    fn build_content(&self) -> Vec<Line<'static>> {
        let label_style = Style::default().fg(Color::DarkGray);
        let value_style = Style::default().fg(Color::White);
        let tag_style = Style::default().fg(Color::Cyan);
        let heading_style = Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD);
        let dim_style = Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC);

        let mut lines = self.build_daemon_section();

        // === Watch Rules Section ===
        lines.push(Line::from(Span::styled("Watch Rules", heading_style)));
        lines.push(Line::from("─".repeat(50)));

        if self.state.rules.is_empty() {
            lines.push(Line::default());
            lines.push(Line::from(Span::styled(
                "No watch rules configured.",
                dim_style,
            )));
            lines.push(Line::from(Span::styled(
                "Add rules in ~/.config/tagr/watch.toml",
                dim_style,
            )));
        } else {
            #[allow(clippy::cast_possible_truncation)]
            let rule_count = self.state.rules.len();
            lines.push(Line::from(Span::styled(
                format!(
                    "{rule_count} rule{}",
                    if rule_count == 1 { "" } else { "s" }
                ),
                value_style,
            )));

            for (i, rule) in self.state.rules.iter().enumerate() {
                lines.push(Line::default());
                lines.push(Line::from(Span::styled(
                    format!("Rule #{}", i + 1),
                    heading_style,
                )));

                lines.push(Line::from(vec![
                    Span::styled("  Patterns:  ", label_style),
                    Span::styled(rule.patterns.join(", "), value_style),
                ]));

                lines.push(Line::from(vec![
                    Span::styled("  Tags:      ", label_style),
                    Span::styled(rule.tags.join(", "), tag_style),
                ]));

                if let Some(ref filter) = rule.filter {
                    lines.push(Line::from(vec![
                        Span::styled("  Filter:    ", label_style),
                        Span::styled(filter.clone(), value_style),
                    ]));
                }

                if !rule.vtags.is_empty() {
                    lines.push(Line::from(vec![
                        Span::styled("  VTags:     ", label_style),
                        Span::styled(rule.vtags.join(", "), value_style),
                    ]));
                }

                if !rule.filter_by_tags.is_empty() {
                    lines.push(Line::from(vec![
                        Span::styled("  Gate tags: ", label_style),
                        Span::styled(rule.filter_by_tags.join(", "), tag_style),
                    ]));
                }
            }
        }

        lines.push(Line::default());
        lines.push(Line::from("─".repeat(50)));
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            "ESC / any key to close   ↑↓ scroll",
            dim_style,
        )));

        lines
    }
}

impl Widget for WatchRulesModal<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let popup_area = Self::centered_rect(70, 70, area);

        Clear.render(popup_area, buf);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.cursor_style())
            .title(" Watch Status ")
            .title_alignment(Alignment::Center);

        let content = self.build_content();

        #[allow(clippy::cast_possible_truncation)]
        let scroll_offset = self.state.scroll as u16;

        Paragraph::new(content)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((scroll_offset, 0))
            .render(popup_area, buf);
    }
}
