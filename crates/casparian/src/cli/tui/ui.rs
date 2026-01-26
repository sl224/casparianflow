//! UI rendering for the TUI

use ratatui::{
    prelude::*,
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
};

use super::app::{
    App, ApprovalsViewState, CommandPaletteMode, CommandPaletteState, DiscoverFocus,
    DiscoverViewState, IngestTab, JobInfo, JobStatus, JobType, JobsListSection, JobsViewState,
    ParserHealth, ReviewTab, RunTab, ShellFocus, SuggestedFix, ThroughputSample, TriageTab,
    TuiMode, ViolationSummary, ViolationType,
};
use super::components::{action_bar, modal};
use super::layout::{viewport_class, ViewportClass};
use super::nav;
use crate::cli::config::state_store_path;
use crate::cli::output::format_number;
use casparian_intent::IntentState;
use chrono::{DateTime, Local};
use ratatui::widgets::Clear;

// ============================================================================
// Shared UI Utilities
// ============================================================================

/// Spinner animation character based on tick count.
/// Returns one of: ⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏
pub fn spinner_char(tick: u64) -> char {
    const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    SPINNER[(tick / 2) as usize % SPINNER.len()]
}

/// ASCII spinner for contexts where unicode might not work.
/// Returns one of: - \ | /
pub fn spinner_ascii(tick: u64) -> char {
    const SPINNER: [char; 4] = ['-', '\\', '|', '/'];
    SPINNER[(tick / 2) as usize % SPINNER.len()]
}

/// Create a centered dialog area and clear it.
/// Returns the dialog Rect. Caller should render_widget(Clear, area) before using.
pub fn centered_dialog_area(area: Rect, max_width: u16, max_height: u16) -> Rect {
    let width = area.width.min(max_width);
    let height = area.height.min(max_height);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width, height)
}

/// Render a centered dialog with Clear background.
/// Returns the dialog Rect for content rendering.
pub fn render_centered_dialog(
    frame: &mut Frame,
    area: Rect,
    max_width: u16,
    max_height: u16,
) -> Rect {
    let dialog_area = centered_dialog_area(area, max_width, max_height);
    frame.render_widget(Clear, dialog_area);
    dialog_area
}

fn format_optional_i64(value: Option<i64>) -> String {
    value
        .map(|v| v.to_string())
        .unwrap_or_else(|| "—".to_string())
}

fn format_schema_detail(
    name: &Option<String>,
    dtype: &Option<String>,
    index: Option<i64>,
) -> String {
    let name = name.as_deref().unwrap_or("—");
    let dtype = dtype.as_deref().unwrap_or("—");
    let index = format_optional_i64(index);
    format!("{} ({}) idx {}", name, dtype, index)
}

pub(super) fn help_overlay_area(area: Rect) -> Rect {
    let help_width = 76.min(area.width.saturating_sub(4));
    let help_height = 36.min(area.height.saturating_sub(2));
    let help_x = (area.width.saturating_sub(help_width)) / 2;
    let help_y = (area.height.saturating_sub(help_height)) / 2;

    Rect {
        x: area.x + help_x,
        y: area.y + help_y,
        width: help_width,
        height: help_height,
    }
}

pub(super) fn command_palette_area(palette: &CommandPaletteState, area: Rect) -> Rect {
    let dialog_width = 56.min(area.width.saturating_sub(4));
    let suggestion_count = palette.suggestions.len().min(6);
    let dialog_height = (4 + (suggestion_count as u16 * 2) + 3).min(area.height.saturating_sub(4));

    centered_dialog_area(area, dialog_width, dialog_height)
}

/// Clear the content area beneath the top bar for modal overlays.
fn draw_modal_scrim(frame: &mut Frame, area: Rect, top_bar: Rect) {
    modal::render_scrim(frame, area, top_bar);
}

/// Calculate scroll offset to keep selected item centered in view.
/// Works for any scrollable list (files, folders, dropdowns).
pub fn centered_scroll_offset(selected: usize, visible: usize, total: usize) -> usize {
    if visible >= total {
        0 // All items fit, no scrolling needed
    } else if selected < visible / 2 {
        0 // Near start, show from beginning
    } else if selected > total.saturating_sub(visible / 2) {
        total.saturating_sub(visible) // Near end, show last items
    } else {
        selected.saturating_sub(visible / 2) // Center the selection
    }
}

/// Create a styled confidence badge for violation fix suggestions.
/// Returns a styled Span with background color indicating confidence level.
fn confidence_badge(level: &str) -> Span<'static> {
    match level.to_uppercase().as_str() {
        "HIGH" => Span::styled(" HIGH ", Style::default().bg(Color::Green).fg(Color::Black)),
        "MEDIUM" | "MED" => {
            Span::styled(" MED ", Style::default().bg(Color::Yellow).fg(Color::Black))
        }
        "LOW" => Span::styled(" LOW ", Style::default().bg(Color::Red).fg(Color::White)),
        _ => Span::raw(format!(" {} ", level)),
    }
}

/// Get the display icon for a violation type
fn violation_icon(vtype: ViolationType) -> &'static str {
    vtype.symbol()
}

/// Get color for violation type
fn violation_color(vtype: ViolationType) -> Color {
    match vtype {
        ViolationType::TypeMismatch => Color::Red,
        ViolationType::NullNotAllowed => Color::Yellow,
        ViolationType::FormatMismatch => Color::Magenta,
        ViolationType::ColumnNameMismatch => Color::Cyan,
        ViolationType::ColumnCountMismatch => Color::Red,
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ShellAreas {
    pub(super) top: Rect,
    pub(super) rail: Rect,
    pub(super) main: Rect,
    pub(super) inspector: Rect,
    pub(super) bottom: Rect,
}

pub(super) fn shell_layout(area: Rect, inspector_visible: bool) -> ShellAreas {
    let top_height = 3u16.min(area.height);
    let bottom_height = 3u16.min(area.height.saturating_sub(top_height));
    let content_height = area.height.saturating_sub(top_height + bottom_height);

    let rail_width = 20u16.min(area.width);
    let mut inspector_width = if inspector_visible {
        let pct = area.width.saturating_mul(30) / 100;
        pct.max(24)
    } else {
        0
    };

    if rail_width + inspector_width >= area.width {
        inspector_width = area.width.saturating_sub(rail_width).saturating_sub(1);
    }

    let main_width = area.width.saturating_sub(rail_width + inspector_width);

    let top = Rect::new(area.x, area.y, area.width, top_height);
    let bottom = Rect::new(
        area.x,
        area.y + area.height.saturating_sub(bottom_height),
        area.width,
        bottom_height,
    );
    let content_y = area.y + top_height;

    let rail = Rect::new(area.x, content_y, rail_width, content_height);
    let main = Rect::new(area.x + rail_width, content_y, main_width, content_height);
    let inspector = Rect::new(
        area.x + rail_width + main_width,
        content_y,
        inspector_width,
        content_height,
    );

    ShellAreas {
        top,
        rail,
        main,
        inspector,
        bottom,
    }
}

fn inspector_visible_for(app: &App, area: Rect) -> bool {
    if matches!(viewport_class(area), ViewportClass::Narrow) {
        return false;
    }
    !app.inspector_collapsed
}

pub(super) fn right_drawer_area(shell: &ShellAreas) -> Rect {
    let total_width = shell.top.width;
    let drawer_width = (total_width * 40 / 100).max(50).min(total_width);
    Rect::new(
        shell.top.x + total_width.saturating_sub(drawer_width),
        shell.main.y,
        drawer_width,
        shell.main.height,
    )
}

fn workspace_switcher_area(area: Rect) -> Rect {
    let width = (area.width * 70 / 100).max(50).min(area.width);
    let height = (area.height * 70 / 100).max(16).min(area.height);
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width, height)
}

fn draw_shell_top_bar(frame: &mut Frame, app: &App, area: Rect) {
    let view_label = app.view_label();
    if area.height == 0 {
        return;
    }

    let _db_path = app.config.database.clone().unwrap_or_else(state_store_path);
    let backend_label = "sqlite";

    let running = app
        .jobs_state
        .jobs
        .iter()
        .filter(|j| j.status == JobStatus::Running)
        .count();
    let failed = app
        .jobs_state
        .jobs
        .iter()
        .filter(|j| j.status == JobStatus::Failed)
        .count();
    let quarantine_total: u64 = app
        .jobs_state
        .jobs
        .iter()
        .filter_map(|j| j.quarantine_rows)
        .map(|rows| rows.max(0) as u64)
        .sum();

    let workspace_label = app
        .active_workspace
        .as_ref()
        .map(|ws| ws.name.as_str())
        .unwrap_or("(none)");
    let mut text = format!(
        " Casparian Flow | View: {} | WS: {} | DB: {} | Run: {} | Fail: {} | Quarantine: {} ",
        view_label, workspace_label, backend_label, running, failed, quarantine_total
    );
    if let Some(status) = &app.global_status {
        let prefix = if status.is_error { " !" } else { " " };
        text.push_str(prefix);
        text.push_str(&status.message);
        text.push(' ');
    }
    let line = truncate_end(&text, area.width as usize);
    let bar = Paragraph::new(line)
        .style(Style::default().fg(Color::Cyan).bold())
        .alignment(Alignment::Left)
        .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(bar, area);
}

fn draw_shell_action_bar(
    frame: &mut Frame,
    hints: &[action_bar::ActionHint],
    style: Style,
    area: Rect,
) {
    action_bar::render_action_bar(frame, area, hints, style);
}

fn draw_shell_action_message(frame: &mut Frame, message: &str, style: Style, area: Rect) {
    action_bar::render_action_bar_message(frame, area, message, style);
}

fn draw_shell_rail(frame: &mut Frame, app: &App, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let is_focused = app.shell_focus == ShellFocus::Rail;
    let (border_style, border_type) = if is_focused {
        (Style::default().fg(Color::Cyan), BorderType::Double)
    } else {
        (Style::default().fg(Color::DarkGray), BorderType::Rounded)
    };

    let block = Block::default()
        .title(" NAV ")
        .borders(Borders::ALL)
        .border_style(border_style)
        .border_type(border_type);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        " Tasks",
        if is_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        },
    )));

    for (idx, item) in nav::NAV_ITEMS.iter().enumerate() {
        let is_active = app.mode == item.mode;
        let is_selected = is_focused && app.nav_selected == idx;
        let style = if is_selected {
            Style::default().fg(Color::White).bold().bg(Color::DarkGray)
        } else if is_active {
            if is_focused {
                Style::default().fg(Color::Cyan).bold()
            } else {
                Style::default().fg(Color::White).bold().bg(Color::DarkGray)
            }
        } else {
            Style::default().fg(Color::Gray)
        };
        let line = format!(" [{}] {}", item.key, item.label);
        lines.push(Line::from(Span::styled(
            truncate_end(&line, inner.width as usize),
            style,
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " Context",
        Style::default().fg(Color::DarkGray),
    )));
    let workspace_label = app
        .active_workspace
        .as_ref()
        .map(|ws| ws.name.as_str())
        .unwrap_or("(none)");
    let workspace_line = format!(" Workspace: {}", workspace_label);
    lines.push(Line::from(Span::styled(
        truncate_end(&workspace_line, inner.width as usize),
        Style::default().fg(Color::Gray),
    )));

    match app.mode {
        TuiMode::Ingest => {
            if app.ingest_tab == IngestTab::Sources {
                if let Some(source) = app.discover.sources.get(app.sources_state.selected_index) {
                    let line = format!(" Source: {}", source.name);
                    lines.push(Line::from(Span::styled(
                        truncate_end(&line, inner.width as usize),
                        Style::default().fg(Color::Gray),
                    )));
                }
            } else if let Some(source) = app.discover.selected_source() {
                let line = format!(" Source: {}", source.name);
                lines.push(Line::from(Span::styled(
                    truncate_end(&line, inner.width as usize),
                    Style::default().fg(Color::Gray),
                )));
                let tag_label = app
                    .discover
                    .selected_tag
                    .and_then(|idx| app.discover.tags.get(idx))
                    .map(|t| t.name.as_str())
                    .unwrap_or("All");
                let tag_line = format!(" Tag: {}", tag_label);
                lines.push(Line::from(Span::styled(
                    truncate_end(&tag_line, inner.width as usize),
                    Style::default().fg(Color::Gray),
                )));
            } else {
                lines.push(Line::from(Span::styled(
                    " Source: (none)",
                    Style::default().fg(Color::DarkGray),
                )));
            }
        }
        TuiMode::Run => {
            if app.run_tab == RunTab::Jobs {
                let status = app
                    .jobs_state
                    .status_filter
                    .map(|s| s.as_str())
                    .unwrap_or("All");
                let job_type = app
                    .jobs_state
                    .type_filter
                    .map(|t| t.as_str())
                    .unwrap_or("All");
                lines.push(Line::from(Span::styled(
                    truncate_end(&format!(" Status: {}", status), inner.width as usize),
                    Style::default().fg(Color::Gray),
                )));
                lines.push(Line::from(Span::styled(
                    truncate_end(&format!(" Type: {}", job_type), inner.width as usize),
                    Style::default().fg(Color::Gray),
                )));
            } else {
                lines.push(Line::from(Span::styled(
                    format!(" Tab: {}", app.catalog_state.tab.label()),
                    Style::default().fg(Color::Gray),
                )));
            }
        }
        TuiMode::Review => match app.review_tab {
            ReviewTab::Approvals => {
                lines.push(Line::from(Span::styled(
                    " MCP Approvals",
                    Style::default().fg(Color::Gray),
                )));
            }
            ReviewTab::Sessions => {
                let session_count = app.sessions_state.sessions.len();
                lines.push(Line::from(Span::styled(
                    format!(" Sessions: {}", session_count),
                    Style::default().fg(Color::Gray),
                )));
            }
            ReviewTab::Triage => {
                lines.push(Line::from(Span::styled(
                    format!(" Tab: {}", app.triage_state.tab.label()),
                    Style::default().fg(Color::Gray),
                )));
            }
        },
        TuiMode::Settings => {
            lines.push(Line::from(Span::styled(
                truncate_end(
                    &format!(" Category: {:?}", app.settings.category),
                    inner.width as usize,
                ),
                Style::default().fg(Color::Gray),
            )));
        }
        TuiMode::Home => {
            let source_count = app.discover.sources.len();
            lines.push(Line::from(Span::styled(
                format!(" Sources: {}", source_count),
                Style::default().fg(Color::Gray),
            )));
        }
        TuiMode::Query => {
            lines.push(Line::from(Span::styled(
                " SQL Query Console",
                Style::default().fg(Color::Gray),
            )));
        }
    }

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
    frame.render_widget(paragraph, inner);
}

fn draw_shell_inspector_block(frame: &mut Frame, title: &str, area: Rect) -> Rect {
    if area.width == 0 || area.height == 0 {
        return area;
    }
    let block = Block::default()
        .title(format!(" {} ", title))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    inner
}

// ============================================================================
// Helper Functions for Truncation
// ============================================================================

/// Truncate a path from the START to fit within max_width.
/// Shows the END of the path (most relevant part).
/// Example: "/very/long/nested/path/to/file.py" -> ".../path/to/file.py"
fn truncate_path_start(path: &str, max_width: usize) -> String {
    let char_count = path.chars().count();
    if char_count <= max_width {
        return path.to_string();
    }

    // We need room for ".../" prefix (4 chars)
    if max_width <= 4 {
        return path.chars().take(max_width).collect();
    }

    // Take the last (max_width - 4) characters from the path
    let suffix: String = path
        .chars()
        .rev()
        .take(max_width - 4)
        .collect::<String>()
        .chars()
        .rev()
        .collect();

    format!(".../{}", suffix)
}

fn truncate_start(text: &str, max_width: usize) -> String {
    let char_count = text.chars().count();
    if char_count <= max_width {
        return text.to_string();
    }

    if max_width <= 3 {
        return text.chars().take(max_width).collect();
    }

    let suffix: String = text
        .chars()
        .rev()
        .take(max_width - 3)
        .collect::<String>()
        .chars()
        .rev()
        .collect();

    format!("...{}", suffix)
}

fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return Vec::new();
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if word.len() > max_width {
            if !current.is_empty() {
                lines.push(current);
                current = String::new();
            }
            let mut chunk = String::new();
            for ch in word.chars() {
                if chunk.chars().count() >= max_width {
                    lines.push(chunk);
                    chunk = String::new();
                }
                chunk.push(ch);
            }
            if !chunk.is_empty() {
                lines.push(chunk);
            }
            continue;
        }

        let pending = if current.is_empty() {
            word.len()
        } else {
            current.len() + 1 + word.len()
        };
        if pending > max_width {
            if !current.is_empty() {
                lines.push(current);
            }
            current = word.to_string();
        } else {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

#[derive(Debug)]
struct PathParts {
    root: String,
    segments: Vec<String>,
    full: String,
}

fn split_path_parts(path: &str) -> PathParts {
    let normalized = path.replace('\\', "/");
    let mut rest = normalized.as_str();
    let root = if rest.starts_with("~/") {
        rest = &rest[2..];
        "~".to_string()
    } else if rest.starts_with('/') {
        rest = &rest[1..];
        "/".to_string()
    } else if rest.len() >= 2 && rest.as_bytes()[1] == b':' {
        let drive = &rest[..2];
        rest = rest.get(2..).unwrap_or("");
        if rest.starts_with('/') {
            rest = &rest[1..];
        }
        drive.to_string()
    } else {
        String::new()
    };

    let segments = rest
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    PathParts {
        root,
        segments,
        full: normalized,
    }
}

fn build_suffix_display(parts: &PathParts, suffix_len: usize) -> String {
    if parts.segments.is_empty() {
        return parts.root.clone();
    }

    let suffix_len = suffix_len.min(parts.segments.len());
    let suffix = parts.segments[parts.segments.len() - suffix_len..].join("/");
    if suffix_len == parts.segments.len() {
        if parts.root.is_empty() {
            return suffix;
        }
        if parts.root.ends_with('/') {
            return format!("{}{}", parts.root, suffix);
        }
        return format!("{}/{}", parts.root, suffix);
    }

    if parts.root.is_empty() {
        format!(".../{}", suffix)
    } else if parts.root.ends_with('/') {
        format!("{}.../{}", parts.root, suffix)
    } else {
        format!("{}/.../{}", parts.root, suffix)
    }
}

fn format_path_list(paths: &[String], max_width: usize, min_suffix_segments: usize) -> Vec<String> {
    let parts: Vec<PathParts> = paths.iter().map(|p| split_path_parts(p)).collect();
    let mut suffix_lens: Vec<usize> = parts
        .iter()
        .map(|p| min_suffix_segments.min(p.segments.len()).max(1))
        .collect();

    loop {
        let mut display_map: std::collections::HashMap<String, Vec<usize>> =
            std::collections::HashMap::new();
        for (idx, part) in parts.iter().enumerate() {
            let display = build_suffix_display(part, suffix_lens[idx]);
            display_map.entry(display).or_default().push(idx);
        }

        let mut updated = false;
        for indices in display_map.values() {
            if indices.len() <= 1 {
                continue;
            }
            for &idx in indices {
                if suffix_lens[idx] < parts[idx].segments.len() {
                    suffix_lens[idx] += 1;
                    updated = true;
                }
            }
        }

        if !updated {
            break;
        }
    }

    parts
        .iter()
        .enumerate()
        .map(|(idx, part)| {
            let display = build_suffix_display(part, suffix_lens[idx]);
            if display.chars().count() > max_width {
                truncate_path_start(&part.full, max_width)
            } else {
                display
            }
        })
        .collect()
}

/// Format file size in human-readable form (fixed 8 char width)
fn format_size(size: u64) -> String {
    if size < 1024 {
        format!("{:>6} B", size)
    } else if size < 1024 * 1024 {
        format!("{:>5.1}KB", size as f64 / 1024.0)
    } else if size < 1024 * 1024 * 1024 {
        format!("{:>5.1}MB", size as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:>5.1}GB", size as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Draw the entire UI
pub fn draw(frame: &mut Frame, app: &App) {
    #[cfg(feature = "profiling")]
    let _zone = app.profiler.zone("tui.draw");

    let area = frame.area();
    let inspector_visible = inspector_visible_for(app, area);
    let shell = shell_layout(area, inspector_visible);

    let main_area = area;

    // Draw Main Content
    match app.mode {
        TuiMode::Home => draw_home_screen(frame, app, main_area),
        TuiMode::Ingest => match app.ingest_tab {
            IngestTab::Sources => draw_sources_screen(frame, app, main_area),
            _ => draw_discover_screen(frame, app, main_area),
        },
        TuiMode::Run => match app.run_tab {
            RunTab::Jobs => draw_jobs_screen(frame, app, main_area),
            RunTab::Outputs => draw_catalog_screen(frame, app, main_area),
        },
        TuiMode::Review => match app.review_tab {
            ReviewTab::Approvals => draw_approvals_screen(frame, app, main_area),
            ReviewTab::Sessions => draw_sessions_screen(frame, app, main_area),
            ReviewTab::Triage => draw_triage_screen(frame, app, main_area),
        },
        TuiMode::Query => draw_query_screen(frame, app, main_area),
        TuiMode::Settings => draw_settings_screen(frame, app, main_area),
    }

    // Draw Jobs Drawer Overlay (toggle with J key)
    if app.jobs_drawer_open {
        draw_jobs_drawer(frame, app, &shell);
    }

    // Draw Sources Drawer Overlay (toggle with S key)
    if app.sources_drawer_open {
        draw_sources_drawer(frame, app, &shell);
    }

    let jobs_modal_active = app.mode == TuiMode::Run
        && app.run_tab == RunTab::Jobs
        && matches!(
            app.jobs_state.view_state,
            JobsViewState::FilterDialog | JobsViewState::LogViewer
        );

    let global_modal_active = app.show_help
        || app.command_palette.visible
        || app.workspace_switcher.visible
        || jobs_modal_active
        || app
            .discover
            .rule_builder
            .as_ref()
            .map(|builder| builder.suggestions_help_open || builder.suggestions_detail_open)
            .unwrap_or(false);

    if global_modal_active {
        draw_modal_scrim(frame, area, shell.top);
    }

    if jobs_modal_active {
        match app.jobs_state.view_state {
            JobsViewState::FilterDialog => draw_jobs_filter_dialog(frame, app, area),
            JobsViewState::LogViewer => draw_jobs_log_viewer(frame, app, area),
            _ => {}
        }
    }

    if let Some(builder) = &app.discover.rule_builder {
        if builder.suggestions_help_open {
            draw_suggestions_help_overlay(frame, area, builder);
        }
        if builder.suggestions_detail_open {
            draw_suggestions_detail_overlay(frame, area, builder);
        }
    }

    // Draw Help Overlay (on top of everything)
    if app.show_help {
        draw_help_overlay(frame, area, app);
    }

    // Draw Workspace Switcher Overlay
    if app.workspace_switcher.visible {
        draw_workspace_switcher_overlay(frame, app, area);
    }

    // Draw Command Palette Overlay (highest z-index, blocks all input when visible)
    if app.command_palette.visible {
        draw_command_palette(frame, app, area);
    }
}

/// Draw the Jobs drawer overlay (global, toggles with J)
fn draw_jobs_drawer(frame: &mut Frame, app: &App, shell: &ShellAreas) {
    let drawer_area = right_drawer_area(shell);

    // Clear background
    frame.render_widget(Clear, drawer_area);

    let block = Block::default()
        .title(" Jobs ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .border_type(BorderType::Rounded);
    let inner = block.inner(drawer_area);
    frame.render_widget(block, drawer_area);

    // Split drawer into: status summary, job list, footer hints
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Status counts
            Constraint::Min(0),    // Job list
            Constraint::Length(2), // Footer hints
        ])
        .split(inner);

    // Status summary line
    let running = app
        .jobs_state
        .jobs
        .iter()
        .filter(|j| j.status == super::app::JobStatus::Running)
        .count();
    let pending = app
        .jobs_state
        .jobs
        .iter()
        .filter(|j| j.status == super::app::JobStatus::Pending)
        .count();
    let failed = app
        .jobs_state
        .jobs
        .iter()
        .filter(|j| j.status == super::app::JobStatus::Failed)
        .count();
    let completed = app
        .jobs_state
        .jobs
        .iter()
        .filter(|j| {
            matches!(
                j.status,
                super::app::JobStatus::Completed | super::app::JobStatus::PartialSuccess
            )
        })
        .count();

    let status_line = ratatui::text::Line::from(vec![
        ratatui::text::Span::styled(
            format!("Running {} ", running),
            Style::default().fg(Color::Yellow),
        ),
        ratatui::text::Span::styled(
            format!("Pending {} ", pending),
            Style::default().fg(Color::DarkGray),
        ),
        ratatui::text::Span::styled(
            format!("Failed {} ", failed),
            Style::default().fg(Color::Red),
        ),
        ratatui::text::Span::styled(
            format!("Done {}", completed),
            Style::default().fg(Color::Green),
        ),
    ]);
    let status = Paragraph::new(status_line);
    frame.render_widget(status, chunks[0]);

    // Job list (most recent first, limit to visible area)
    let visible_height = chunks[1].height as usize;
    let jobs: Vec<&super::app::JobInfo> = app.jobs_state.jobs.iter().take(visible_height).collect();

    if jobs.is_empty() {
        let empty = Paragraph::new("No jobs")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(empty, chunks[1]);
    } else {
        let items: Vec<ratatui::widgets::ListItem> = jobs
            .iter()
            .enumerate()
            .map(|(i, job)| {
                let is_selected = i == app.jobs_drawer_selected;
                let prefix = if is_selected { "► " } else { "  " };

                let status_style = match job.status {
                    super::app::JobStatus::Running => Style::default().fg(Color::Yellow),
                    super::app::JobStatus::Completed => Style::default().fg(Color::Green),
                    super::app::JobStatus::PartialSuccess => Style::default().fg(Color::Yellow),
                    super::app::JobStatus::Failed => Style::default().fg(Color::Red),
                    super::app::JobStatus::Pending => Style::default().fg(Color::DarkGray),
                    super::app::JobStatus::Cancelled => Style::default().fg(Color::Magenta),
                };

                // Calculate progress percentage if applicable
                let progress_pct = if job.items_total > 0 {
                    Some((job.items_processed as f32 / job.items_total as f32 * 100.0) as u8)
                } else {
                    None
                };

                let status_char = match job.status {
                    super::app::JobStatus::Running => {
                        if let Some(pct) = progress_pct {
                            format!("{}%", pct)
                        } else {
                            "⋯".to_string()
                        }
                    }
                    super::app::JobStatus::Completed => "✓".to_string(),
                    super::app::JobStatus::PartialSuccess => "⚠".to_string(),
                    super::app::JobStatus::Failed => "✗".to_string(),
                    super::app::JobStatus::Pending => "○".to_string(),
                    super::app::JobStatus::Cancelled => "⊘".to_string(),
                };

                let job_type = match job.job_type {
                    super::app::JobType::Scan => "scan",
                    super::app::JobType::Parse => "parse",
                    super::app::JobType::Backtest => "test",
                    super::app::JobType::SchemaEval => "schema",
                };

                // Use job name as description, truncate to fit
                let max_desc_len = (drawer_area.width as usize).saturating_sub(20);
                let desc = if job.name.len() > max_desc_len {
                    format!("{}…", &job.name[..max_desc_len.saturating_sub(1)])
                } else {
                    job.name.clone()
                };

                let line = ratatui::text::Line::from(vec![
                    ratatui::text::Span::raw(prefix),
                    ratatui::text::Span::raw(format!("[{}] ", job_type)),
                    ratatui::text::Span::raw(desc),
                    ratatui::text::Span::raw(" "),
                    ratatui::text::Span::styled(status_char, status_style),
                ]);

                let style = if is_selected {
                    Style::default().bg(Color::DarkGray)
                } else {
                    Style::default()
                };

                ratatui::widgets::ListItem::new(line).style(style)
            })
            .collect();

        let list = ratatui::widgets::List::new(items);
        frame.render_widget(list, chunks[1]);
    }

    // Footer hints
    let footer = Paragraph::new("[Enter] Open Jobs  [3] Full Jobs View  [J/Esc] Close")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    frame.render_widget(footer, chunks[2]);
}

/// Draw the Sources drawer overlay (global, toggles with S key)
fn draw_sources_drawer(frame: &mut Frame, app: &App, shell: &ShellAreas) {
    let drawer_area = right_drawer_area(shell);

    // Clear background
    frame.render_widget(Clear, drawer_area);

    let block = Block::default()
        .title(" Sources ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .border_type(BorderType::Rounded);
    let inner = block.inner(drawer_area);
    frame.render_widget(block, drawer_area);

    // Split drawer into: header line, source list, footer hints
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(2),
        ])
        .split(inner);

    let header =
        Paragraph::new("Recent sources (MRU, max 5)").style(Style::default().fg(Color::DarkGray));
    frame.render_widget(header, chunks[0]);

    let source_indices = app.sources_drawer_sources();
    let visible_height = chunks[1].height as usize;
    let sources: Vec<usize> = source_indices.into_iter().take(visible_height).collect();

    if sources.is_empty() {
        let empty = Paragraph::new("No sources yet")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(empty, chunks[1]);
    } else {
        let items: Vec<ratatui::widgets::ListItem> = sources
            .iter()
            .enumerate()
            .map(|(i, source_idx)| {
                let source = &app.discover.sources[*source_idx];
                let is_selected = i == app.sources_drawer_selected;
                let prefix = if is_selected { "► " } else { "  " };

                let mut name = source.name.clone();
                if name.is_empty() {
                    name = source.path.display().to_string();
                }
                let max_name_len = (drawer_area.width as usize).saturating_sub(18);
                let display_name = if name.len() > max_name_len {
                    format!("{}…", &name[..max_name_len.saturating_sub(1)])
                } else {
                    name
                };

                let file_count = format_number(source.file_count as u64);
                let line = ratatui::text::Line::from(vec![
                    ratatui::text::Span::raw(prefix),
                    ratatui::text::Span::raw(display_name),
                    ratatui::text::Span::raw("  "),
                    ratatui::text::Span::styled(
                        format!("{} files", file_count),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]);

                let style = if is_selected {
                    Style::default().bg(Color::DarkGray)
                } else {
                    Style::default()
                };

                ratatui::widgets::ListItem::new(line).style(style)
            })
            .collect();

        let list = ratatui::widgets::List::new(items);
        frame.render_widget(list, chunks[1]);
    }

    let footer =
        Paragraph::new("[s] Scan  [e] Edit  [d] Delete  [n] New  [Enter] Open  [S/Esc] Close")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
    frame.render_widget(footer, chunks[2]);
}

fn draw_workspace_switcher_overlay(frame: &mut Frame, app: &App, area: Rect) {
    use ratatui::widgets::{List, ListItem, ListState};

    let overlay = workspace_switcher_area(area);
    frame.render_widget(Clear, overlay);

    let title = match app.workspace_switcher.mode {
        super::app::WorkspaceSwitcherMode::Creating => " Workspaces (New) ",
        super::app::WorkspaceSwitcherMode::List => " Workspaces ",
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .border_type(BorderType::Rounded);
    let inner = block.inner(overlay);
    frame.render_widget(block, overlay);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(5),
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Length(2),
        ])
        .split(inner);

    let active_label = app
        .active_workspace
        .as_ref()
        .map(|ws| ws.name.as_str())
        .unwrap_or("(none)");
    let header = Paragraph::new(format!("Active workspace: {}", active_label))
        .style(Style::default().fg(Color::Gray));
    frame.render_widget(header, chunks[0]);

    if app.workspace_switcher.workspaces.is_empty() && app.workspace_switcher.loaded {
        let msg = Paragraph::new("No workspaces found. Press [n] to create one.")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(msg, chunks[1]);
    } else {
        let items: Vec<ListItem> = app
            .workspace_switcher
            .workspaces
            .iter()
            .map(|ws| {
                let marker = if Some(ws.id) == app.active_workspace.as_ref().map(|w| w.id) {
                    "*"
                } else {
                    " "
                };
                let id = ws.id.to_string();
                let short_id = &id[..8.min(id.len())];
                let line = format!("{} {} ({})", marker, ws.name, short_id);
                ListItem::new(truncate_end(&line, chunks[1].width as usize))
            })
            .collect();
        let mut state = ListState::default();
        if !app.workspace_switcher.workspaces.is_empty() {
            state.select(Some(app.workspace_switcher.selected_index));
        }
        let list = List::new(items).highlight_style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD)
                .bg(Color::DarkGray),
        );
        frame.render_stateful_widget(list, chunks[1], &mut state);
    }

    let input_text = match app.workspace_switcher.mode {
        super::app::WorkspaceSwitcherMode::Creating => {
            format!("Name: {}_", app.workspace_switcher.input)
        }
        super::app::WorkspaceSwitcherMode::List => {
            "Press [n] to create a new workspace.".to_string()
        }
    };
    let input_block = Block::default()
        .title(" Workspace ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let input = Paragraph::new(input_text).block(input_block);
    frame.render_widget(input, chunks[2]);

    if let Some(msg) = &app.workspace_switcher.status_message {
        let status = Paragraph::new(msg.as_str())
            .style(Style::default().fg(Color::Yellow))
            .alignment(Alignment::Left);
        frame.render_widget(status, chunks[3]);
    }

    let footer_text = match app.workspace_switcher.mode {
        super::app::WorkspaceSwitcherMode::Creating => "[Enter] Create  [Esc] Cancel".to_string(),
        super::app::WorkspaceSwitcherMode::List => {
            "[↑/↓] Select  [Enter] Set Active  [n] New  [r] Refresh  [Esc] Close".to_string()
        }
    };
    let footer = Paragraph::new(footer_text)
        .style(Style::default().fg(Color::Gray))
        .alignment(Alignment::Center);
    frame.render_widget(footer, chunks[4]);
}

/// Draw the home hub screen (Quick Start + Status dashboard)
fn draw_home_screen(frame: &mut Frame, app: &App, area: Rect) {
    let inspector_visible = inspector_visible_for(app, area);
    let shell = shell_layout(area, inspector_visible);

    draw_shell_top_bar(frame, app, shell.top);
    draw_shell_rail(frame, app, shell.rail);

    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(shell.main);

    draw_home_sources_panel(frame, app, main_chunks[0]);
    draw_home_readiness_panel(frame, app, main_chunks[1]);

    if inspector_visible {
        let inner = draw_shell_inspector_block(frame, "Inspector", shell.inspector);
        draw_home_inspector(frame, app, inner);
    }

    draw_shell_action_bar(
        frame,
        &app.effective_actions(),
        Style::default().fg(Color::DarkGray),
        shell.bottom,
    );
}

/// Draw the Sources panel (left side of Home)
fn draw_home_sources_panel(frame: &mut Frame, app: &App, area: Rect) {
    let filter_label = if app.home.filtering {
        format!(" Quick Start: Scan a Source /{}_", app.home.filter)
    } else if !app.home.filter.is_empty() {
        format!(" Quick Start: Scan a Source /{} ", app.home.filter)
    } else {
        " Quick Start: Scan a Source ".to_string()
    };
    let block = Block::default()
        .title(filter_label)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.discover.sources.is_empty() {
        let msg = Paragraph::new("No sources configured.\n\nPress [s] to scan a folder.")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(msg, inner);
        return;
    }

    let filter_lower = app.home.filter.to_lowercase();
    let filtered_sources: Vec<_> = app
        .discover
        .sources
        .iter()
        .enumerate()
        .filter(|(_, source)| {
            filter_lower.is_empty()
                || source.name.to_lowercase().contains(&filter_lower)
                || source
                    .path
                    .display()
                    .to_string()
                    .to_lowercase()
                    .contains(&filter_lower)
        })
        .collect();

    if filtered_sources.is_empty() {
        let msg = Paragraph::new("No sources match the filter.\n\nPress [/] to edit.")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(msg, inner);
        return;
    }

    // Build source list items
    let items: Vec<ratatui::widgets::ListItem> = app
        .discover
        .sources
        .iter()
        .enumerate()
        .filter(|(i, _)| filtered_sources.iter().any(|(idx, _)| idx == i))
        .map(|(i, source)| {
            let is_selected = i == app.home.selected_source_index;
            let style = if is_selected {
                Style::default().fg(Color::Cyan).bold()
            } else {
                Style::default()
            };
            let prefix = if is_selected { "► " } else { "  " };
            let text = format!("{}{} ({} files)", prefix, source.name, source.file_count);
            ratatui::widgets::ListItem::new(text).style(style)
        })
        .collect();

    let list = ratatui::widgets::List::new(items).highlight_style(Style::default().fg(Color::Cyan));
    frame.render_widget(list, inner);
}

/// Draw the Readiness panel (bottom of Home)
fn draw_home_readiness_panel(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Readiness ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines = Vec::new();

    lines.push(Line::from(Span::styled(
        format!(
            "Running {}  Pending {}  Failed {}",
            app.home.stats.running_jobs, app.home.stats.pending_jobs, app.home.stats.failed_jobs
        ),
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));

    let ready: Vec<_> = app
        .home
        .recent_jobs
        .iter()
        .filter(|j| {
            matches!(
                j.status,
                super::app::JobStatus::Completed | super::app::JobStatus::PartialSuccess
            )
        })
        .collect();
    let active: Vec<_> = app
        .home
        .recent_jobs
        .iter()
        .filter(|j| {
            matches!(
                j.status,
                super::app::JobStatus::Running | super::app::JobStatus::Pending
            )
        })
        .collect();
    let warnings: Vec<_> = app
        .home
        .recent_jobs
        .iter()
        .filter(|j| {
            matches!(
                j.status,
                super::app::JobStatus::Failed | super::app::JobStatus::Cancelled
            )
        })
        .collect();

    lines.push(Line::from(Span::styled(
        "READY OUTPUTS",
        Style::default().fg(Color::Green).bold(),
    )));
    if ready.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No ready outputs.",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for job in ready.iter().take(3) {
            let (label, style) = match job.status {
                super::app::JobStatus::PartialSuccess => {
                    ("[WARN]", Style::default().fg(Color::Yellow))
                }
                _ => ("[READY]", Style::default().fg(Color::Green)),
            };
            lines.push(Line::from(Span::styled(
                format!("  {} {} {}", label, job.job_type, job.description),
                style,
            )));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "ACTIVE RUNS",
        Style::default().fg(Color::Yellow).bold(),
    )));
    if active.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No active runs.",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for job in active.iter().take(3) {
            let status = match job.status {
                super::app::JobStatus::Running => job
                    .progress_percent
                    .map(|pct| format!("{}%", pct))
                    .unwrap_or_else(|| "running".to_string()),
                super::app::JobStatus::Pending => "queued".to_string(),
                _ => "active".to_string(),
            };
            lines.push(Line::from(Span::styled(
                format!("  [RUN] {} {} {}", job.job_type, job.description, status),
                Style::default().fg(Color::Yellow),
            )));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "WARNINGS",
        Style::default().fg(Color::Red).bold(),
    )));
    if warnings.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No warnings.",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for job in warnings.iter().take(2) {
            lines.push(Line::from(Span::styled(
                format!("  [FAIL] {} {}", job.job_type, job.description),
                Style::default().fg(Color::Red),
            )));
        }
    }

    if let Some(ref err) = app.home.last_error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("Last error: {}", err),
            Style::default().fg(Color::Red),
        )));
    }

    let para = Paragraph::new(lines).wrap(Wrap { trim: true });
    frame.render_widget(para, inner);
}

fn draw_home_inspector(frame: &mut Frame, app: &App, area: Rect) {
    let mut lines = Vec::new();
    if let Some(source) = app.discover.sources.get(app.home.selected_source_index) {
        lines.push(Line::from(Span::styled(
            format!("Name: {}", source.name),
            Style::default().fg(Color::White),
        )));
        lines.push(Line::from(Span::styled(
            format!(
                "Path: {}",
                truncate_path_start(&source.path.display().to_string(), 30)
            ),
            Style::default().fg(Color::Gray),
        )));
        lines.push(Line::from(Span::styled(
            format!("Files: {}", source.file_count),
            Style::default().fg(Color::Gray),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "Select a source to see details.",
            Style::default().fg(Color::DarkGray),
        )));
    }
    let para = Paragraph::new(lines).wrap(Wrap { trim: true });
    frame.render_widget(para, area);
}

/// Draw the Sources view screen (key 4)
fn draw_sources_screen(frame: &mut Frame, app: &App, area: Rect) {
    let inspector_visible = inspector_visible_for(app, area);
    let shell = shell_layout(area, inspector_visible);

    draw_shell_top_bar(frame, app, shell.top);
    draw_shell_rail(frame, app, shell.rail);

    let block = Block::default()
        .title(" Configured Sources ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(shell.main);
    frame.render_widget(block, shell.main);

    if app.discover.sources.is_empty() {
        let msg = Paragraph::new(
            "No sources configured.\n\nPress [n] to add a source or [1] Discover then [s] to scan.",
        )
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
        frame.render_widget(msg, inner);
    } else {
        let items: Vec<ratatui::widgets::ListItem> = app
            .discover
            .sources
            .iter()
            .enumerate()
            .map(|(i, source)| {
                let is_selected = i == app.sources_state.selected_index;
                let style = if is_selected {
                    Style::default().fg(Color::White).bold().bg(Color::DarkGray)
                } else {
                    Style::default().fg(Color::Gray)
                };
                let prefix = if is_selected { "► " } else { "  " };
                let text = format!(
                    "{}{}\n   {} ({} files)",
                    prefix,
                    source.name,
                    source.path.display(),
                    source.file_count
                );
                ratatui::widgets::ListItem::new(text).style(style)
            })
            .collect();

        let list = ratatui::widgets::List::new(items);
        frame.render_widget(list, inner);
    }

    if inspector_visible {
        let inner = draw_shell_inspector_block(frame, "Inspector", shell.inspector);
        draw_sources_inspector(frame, app, inner);
    }

    let modal_active = app.sources_state.confirm_delete || app.sources_state.editing;
    if modal_active {
        draw_modal_scrim(frame, area, shell.top);
    }

    // Delete confirmation overlay
    if app.sources_state.confirm_delete {
        let dialog_area = render_centered_dialog(frame, area, 50, 7);
        let para = Paragraph::new("Delete this source?\n\n[y] Yes  [n] No")
            .style(Style::default())
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .title(" Confirm Delete ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Red)),
            );
        frame.render_widget(para, dialog_area);
    }

    // Edit overlay
    if app.sources_state.editing {
        let dialog_area = render_centered_dialog(frame, area, 60, 7);
        let title = if app.sources_state.creating {
            " Add Source "
        } else {
            " Edit Source "
        };
        let para = Paragraph::new(format!(
            "Path: {}_\n\n[Enter] Save  [Esc] Cancel",
            app.sources_state.edit_value
        ))
        .style(Style::default())
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        );
        frame.render_widget(para, dialog_area);
    }

    if modal_active {
        frame.render_widget(Clear, shell.bottom);
    } else {
        draw_shell_action_bar(
            frame,
            &app.effective_actions(),
            Style::default().fg(Color::DarkGray),
            shell.bottom,
        );
    }
}

/// Draw the Approvals view screen (key 5)
fn draw_approvals_screen(frame: &mut Frame, app: &App, area: Rect) {
    let inspector_visible = inspector_visible_for(app, area);
    let shell = shell_layout(area, inspector_visible);

    draw_shell_top_bar(frame, app, shell.top);
    draw_shell_rail(frame, app, shell.rail);

    // Calculate aggregate stats
    let (pending, approved, rejected, expired) = app.approvals_state.stats();

    // Status bar with aggregate stats
    let status_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Status bar
            Constraint::Min(0),    // Approval list
        ])
        .split(shell.main);

    let status_text = format!(
        "  PENDING: {}  |  APPROVED: {}  |  REJECTED: {}  |  EXPIRED: {}  |  Filter: [{}]",
        pending,
        approved,
        rejected,
        expired,
        app.approvals_state.filter.as_str()
    );

    let status_bar = Paragraph::new(status_text)
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .title(" APPROVALS ")
                .title_style(Style::default().fg(Color::Cyan).bold()),
        );
    frame.render_widget(status_bar, status_chunks[0]);

    // Approval list
    draw_approvals_list(frame, app, status_chunks[1]);

    // Inspector panel
    if inspector_visible {
        let inner = draw_shell_inspector_block(frame, "Approval Details", shell.inspector);
        draw_approvals_inspector(frame, app, inner);
    }

    if app.approvals_state.view_state != ApprovalsViewState::List {
        draw_modal_scrim(frame, area, shell.top);
    }

    // Dialogs
    match app.approvals_state.view_state {
        ApprovalsViewState::ConfirmApprove => {
            let dialog_area = render_centered_dialog(frame, area, 50, 7);
            let para = Paragraph::new("Approve this request?\n\n[y] Yes  [n] No")
                .style(Style::default())
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .title(" Confirm Approve ")
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Green)),
                );
            frame.render_widget(para, dialog_area);
        }
        ApprovalsViewState::ConfirmReject => {
            let dialog_area = render_centered_dialog(frame, area, 60, 9);
            let reason_display = if app.approvals_state.rejection_reason.is_empty() {
                "(optional)".to_string()
            } else {
                format!("{}_", app.approvals_state.rejection_reason)
            };
            let para = Paragraph::new(format!(
                "Reject this request?\n\nReason: {}\n\n[Enter] Confirm  [Esc] Cancel",
                reason_display
            ))
            .style(Style::default())
            .block(
                Block::default()
                    .title(" Reject Approval ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Red)),
            );
            frame.render_widget(para, dialog_area);
        }
        ApprovalsViewState::Detail => {
            // Full detail view as overlay
            let dialog_area = render_centered_dialog(frame, area, 80, 20);
            draw_approval_detail_dialog(frame, app, dialog_area);
        }
        ApprovalsViewState::List => {}
    }

    if app.approvals_state.view_state == ApprovalsViewState::List {
        draw_shell_action_bar(
            frame,
            &app.effective_actions(),
            Style::default().fg(Color::DarkGray),
            shell.bottom,
        );
    } else {
        frame.render_widget(Clear, shell.bottom);
    }
}

/// Draw the approvals list
fn draw_approvals_list(frame: &mut Frame, app: &App, area: Rect) {
    let filtered = app.approvals_state.filtered_approvals();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if filtered.is_empty() {
        let msg = Paragraph::new(format!(
            "No {} approvals found.\n\nPress [f] to change filter.",
            app.approvals_state.filter.as_str().to_lowercase()
        ))
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
        frame.render_widget(msg, inner);
        return;
    }

    let mut lines: Vec<Line> = Vec::new();

    for (i, approval) in filtered.iter().enumerate() {
        let is_selected = i == app.approvals_state.selected_index;
        let is_pinned = app.approvals_state.pinned_approval_id.as_ref() == Some(&approval.id);

        let prefix = if is_selected && is_pinned {
            "▸*"
        } else if is_selected {
            "▸ "
        } else if is_pinned {
            " *"
        } else {
            "  "
        };

        let status_symbol = approval.status_symbol();
        let status_color = match approval.status {
            super::app::ApprovalDisplayStatus::Pending => Color::Yellow,
            super::app::ApprovalDisplayStatus::Approved => Color::Green,
            super::app::ApprovalDisplayStatus::Rejected => Color::Red,
            super::app::ApprovalDisplayStatus::Expired => Color::DarkGray,
        };

        let expiry_text = if approval.is_pending() {
            format!(" ({})", approval.expiry_countdown())
        } else {
            String::new()
        };

        let name_style = if is_selected {
            Style::default().fg(Color::Cyan).bold()
        } else {
            Style::default().fg(Color::White)
        };

        // Main line: status symbol, operation type, plugin
        let main_line = Line::from(vec![
            Span::styled(prefix, name_style),
            Span::styled(status_symbol, Style::default().fg(status_color)),
            Span::raw(" "),
            Span::styled(approval.operation_type.as_str(), name_style),
            Span::raw(" - "),
            Span::styled(
                approval.plugin_ref.clone(),
                Style::default().fg(Color::Gray),
            ),
            Span::styled(expiry_text, Style::default().fg(Color::Yellow)),
        ]);
        lines.push(main_line);

        // Summary line (indented)
        let summary_style = if is_selected {
            Style::default().fg(Color::Gray)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let summary_truncated = if approval.summary.len() > 70 {
            format!("{}...", &approval.summary[..67])
        } else {
            approval.summary.clone()
        };
        lines.push(Line::from(Span::styled(
            format!("     {}", summary_truncated),
            summary_style,
        )));

        // Blank line between items
        if i < filtered.len() - 1 {
            lines.push(Line::from(""));
        }
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}

/// Draw the approval inspector panel (right side detail)
fn draw_approvals_inspector(frame: &mut Frame, app: &App, area: Rect) {
    let approval = app.approvals_state.selected_approval();

    if let Some(approval) = approval {
        let mut lines: Vec<Line> = Vec::new();

        lines.push(Line::from(Span::styled(
            "ID:",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(Span::styled(
            format!("  {}", &approval.id[..8.min(approval.id.len())]),
            Style::default().fg(Color::White),
        )));
        lines.push(Line::from(""));

        lines.push(Line::from(Span::styled(
            "Operation:",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(Span::styled(
            format!("  {}", approval.operation_type.as_str()),
            Style::default().fg(Color::Cyan),
        )));
        lines.push(Line::from(""));

        lines.push(Line::from(Span::styled(
            "Plugin:",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(Span::styled(
            format!("  {}", approval.plugin_ref),
            Style::default().fg(Color::White),
        )));
        lines.push(Line::from(""));

        let status_color = match approval.status {
            super::app::ApprovalDisplayStatus::Pending => Color::Yellow,
            super::app::ApprovalDisplayStatus::Approved => Color::Green,
            super::app::ApprovalDisplayStatus::Rejected => Color::Red,
            super::app::ApprovalDisplayStatus::Expired => Color::DarkGray,
        };
        lines.push(Line::from(Span::styled(
            "Status:",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(Span::styled(
            format!(
                "  {} {}",
                approval.status_symbol(),
                approval.status.as_str()
            ),
            Style::default().fg(status_color),
        )));
        lines.push(Line::from(""));

        if approval.is_pending() {
            lines.push(Line::from(Span::styled(
                "Expires in:",
                Style::default().fg(Color::DarkGray),
            )));
            lines.push(Line::from(Span::styled(
                format!("  {}", approval.expiry_countdown()),
                Style::default().fg(Color::Yellow),
            )));
            lines.push(Line::from(""));
        }

        if let Some(ref input_dir) = approval.input_dir {
            lines.push(Line::from(Span::styled(
                "Input:",
                Style::default().fg(Color::DarkGray),
            )));
            lines.push(Line::from(Span::styled(
                format!("  {}", input_dir),
                Style::default().fg(Color::White),
            )));
            lines.push(Line::from(""));
        }

        if let Some(file_count) = approval.file_count {
            lines.push(Line::from(Span::styled(
                "Files:",
                Style::default().fg(Color::DarkGray),
            )));
            lines.push(Line::from(Span::styled(
                format!("  {} files", file_count),
                Style::default().fg(Color::White),
            )));
            lines.push(Line::from(""));
        }

        lines.push(Line::from(Span::styled(
            "Summary:",
            Style::default().fg(Color::DarkGray),
        )));
        // Word-wrap summary
        for chunk in approval.summary.chars().collect::<Vec<_>>().chunks(30) {
            lines.push(Line::from(Span::styled(
                format!("  {}", chunk.iter().collect::<String>()),
                Style::default().fg(Color::White),
            )));
        }

        if approval.is_pending() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "[a] Approve  [r] Reject",
                Style::default().fg(Color::Cyan),
            )));
        }

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, area);
    } else {
        let msg = Paragraph::new("Select an approval to view details")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(msg, area);
    }
}

/// Draw the approval detail dialog (full screen overlay)
fn draw_approval_detail_dialog(frame: &mut Frame, app: &App, area: Rect) {
    let approval = app.approvals_state.selected_approval();

    let block = Block::default()
        .title(" Approval Details ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if let Some(approval) = approval {
        let mut text = format!(
            "ID: {}\n\
             Operation: {}\n\
             Plugin: {}\n\
             Status: {} {}\n\
             Created: {}\n\
             Expires: {}\n\n\
             Summary:\n{}\n",
            approval.id,
            approval.operation_type.as_str(),
            approval.plugin_ref,
            approval.status_symbol(),
            approval.status.as_str(),
            approval.created_at.format("%Y-%m-%d %H:%M:%S"),
            approval.expires_at.format("%Y-%m-%d %H:%M:%S"),
            approval.summary
        );

        if let Some(ref input_dir) = approval.input_dir {
            text.push_str(&format!("\nInput Directory: {}\n", input_dir));
        }
        if let Some(file_count) = approval.file_count {
            text.push_str(&format!("File Count: {}\n", file_count));
        }
        if let Some(ref job_id) = approval.job_id {
            text.push_str(&format!("Job ID: {}\n", job_id));
        }

        text.push_str("\n[Enter/Esc] Close");
        if approval.is_pending() {
            text.push_str("  [a] Approve  [r] Reject");
        }

        let paragraph = Paragraph::new(text).style(Style::default().fg(Color::White));
        frame.render_widget(paragraph, inner);
    }
}
/// Draw the Discover mode screen (File Explorer)
fn draw_discover_screen(frame: &mut Frame, app: &App, area: Rect) {
    #[cfg(feature = "profiling")]
    let _zone = app.profiler.zone("tui.discover");

    let inspector_visible = inspector_visible_for(app, area);
    let shell = shell_layout(area, inspector_visible);

    draw_shell_top_bar(frame, app, shell.top);
    draw_shell_rail(frame, app, shell.rail);

    // Rule Builder is the ONLY view in Discover mode (replaces old GlobExplorer)
    draw_rule_builder_screen(frame, app, shell.main);

    if inspector_visible {
        let inner = draw_shell_inspector_block(frame, "Inspector", shell.inspector);
        draw_discover_inspector(frame, app, inner);
    }

    let modal_active = matches!(
        app.discover.view_state,
        DiscoverViewState::SourcesDropdown
            | DiscoverViewState::TagsDropdown
            | DiscoverViewState::RulesManager
            | DiscoverViewState::RuleCreation
            | DiscoverViewState::SourcesManager
            | DiscoverViewState::SourceEdit
            | DiscoverViewState::SourceDeleteConfirm
            | DiscoverViewState::Scanning
            | DiscoverViewState::EnteringPath
            | DiscoverViewState::ScanConfirm
            | DiscoverViewState::Filtering
            | DiscoverViewState::Tagging
            | DiscoverViewState::BulkTagging
            | DiscoverViewState::CreatingSource
    ) || app
        .discover
        .rule_builder
        .as_ref()
        .map(|builder| builder.manual_tag_confirm_open || builder.confirm_exit_open)
        .unwrap_or(false);

    if modal_active {
        frame.render_widget(Clear, shell.bottom);
    } else if let Some((message, style)) = discover_action_message(app) {
        draw_shell_action_message(frame, &message, style, shell.bottom);
    } else {
        draw_shell_action_bar(
            frame,
            &app.effective_actions(),
            Style::default().fg(Color::DarkGray),
            shell.bottom,
        );
    }
    if modal_active {
        draw_modal_scrim(frame, area, shell.top);
    }

    // Render dropdown/dialog overlays on top of Rule Builder
    match app.discover.view_state {
        DiscoverViewState::SourcesDropdown => draw_sources_dropdown(frame, app, area),
        DiscoverViewState::TagsDropdown => draw_tags_dropdown(frame, app, area),
        DiscoverViewState::RulesManager => draw_rules_manager_dialog(frame, app, area),
        DiscoverViewState::RuleCreation => draw_rule_creation_dialog(frame, app, area),
        DiscoverViewState::SourcesManager => draw_sources_manager_dialog(frame, app, area),
        DiscoverViewState::SourceEdit => draw_source_edit_dialog(frame, app, area),
        DiscoverViewState::SourceDeleteConfirm => {
            draw_source_delete_confirm_dialog(frame, app, area)
        }
        DiscoverViewState::Scanning => draw_scanning_dialog(frame, app, area),
        DiscoverViewState::EnteringPath => draw_add_source_dialog(frame, app, area),
        DiscoverViewState::ScanConfirm => draw_scan_confirm_dialog(frame, app, area),
        DiscoverViewState::Filtering => draw_filter_dialog(frame, app, area),
        DiscoverViewState::Tagging => draw_tagging_dialog(frame, app, area),
        DiscoverViewState::BulkTagging => draw_bulk_tag_dialog(frame, app, area),
        DiscoverViewState::CreatingSource => draw_create_source_dialog(frame, app, area),
        _ => {}
    }

    if let Some(builder) = &app.discover.rule_builder {
        if builder.manual_tag_confirm_open {
            draw_rule_builder_manual_tag_confirm(
                frame,
                area,
                builder.manual_tag_confirm_count,
                builder.tag.as_str(),
            );
        }
        if builder.confirm_exit_open {
            draw_rule_builder_confirm_exit(frame, area);
        }
    }
}

fn discover_action_message(app: &App) -> Option<(String, Style)> {
    if let Some((ref msg, is_error)) = app.discover.status_message {
        let style = if is_error {
            Style::default().fg(Color::Red)
        } else {
            Style::default().fg(Color::Green)
        };
        return Some((format!(" {} ", msg), style));
    }

    if let Some(ref progress) = app.cache_load_progress {
        let spinner = spinner_char(app.tick_count);
        return Some((
            format!(" {} {} ", spinner, progress.status_line()),
            Style::default().fg(Color::Yellow),
        ));
    }

    None
}

/// Draw the Sources dropdown as a proper overlay dialog
fn draw_sources_dropdown(frame: &mut Frame, app: &App, area: Rect) {
    let max_width = area.width.saturating_sub(4).min(72).max(30);
    let max_height = area.height.saturating_sub(6).min(20).max(10);
    let dialog_area = render_centered_dialog(frame, area, max_width, max_height);

    // Expanded dropdown with optional filter line
    let is_filtering = app.discover.sources_filtering;

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Span::styled(
            " [S] Select Source ",
            Style::default().fg(Color::Cyan).bold(),
        ))
        .title_alignment(Alignment::Left);
    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .margin(1)
        .split(inner);

    // Top line: filter input OR hint
    if is_filtering {
        let filter_text = format!("/{}", app.discover.sources_filter);
        let filter_line = Paragraph::new(vec![Line::from(vec![
            Span::styled(filter_text, Style::default().fg(Color::Yellow)),
            Span::styled("█", Style::default().fg(Color::Yellow)),
        ])]);
        frame.render_widget(filter_line, inner_chunks[0]);
    } else if !app.discover.sources_filter.is_empty() {
        // Show active filter (but not in filter mode)
        let hint = format!("/{} (Enter:clear)", app.discover.sources_filter);
        let hint_line = Paragraph::new(hint).style(Style::default().fg(Color::DarkGray));
        frame.render_widget(hint_line, inner_chunks[0]);
    } else {
        // Show keybinding hints
        let hint_line = Paragraph::new("[/]filter [s]scan [↑/↓]nav [Enter]select [Esc]close")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(hint_line, inner_chunks[0]);
    }

    // Filtered list
    let filtered: Vec<_> = app
        .discover
        .sources
        .iter()
        .enumerate()
        .filter(|(_, s)| {
            app.discover.sources_filter.is_empty()
                || s.name
                    .to_lowercase()
                    .contains(&app.discover.sources_filter.to_lowercase())
        })
        .collect();

    let mut lines: Vec<Line> = Vec::new();
    let visible_height = inner_chunks[1].height.saturating_sub(2) as usize;

    if filtered.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No sources found. Press 's' to scan a folder.",
            Style::default().fg(Color::DarkGray).italic(),
        )));
    } else {
        // Find scroll offset based on preview position - keep selection visible with centering
        let preview_idx = app
            .discover
            .preview_source
            .unwrap_or_else(|| app.discover.selected_source_index());
        let preview_pos = filtered
            .iter()
            .position(|(i, _)| *i == preview_idx)
            .unwrap_or(0);
        let total_items = filtered.len();
        let scroll_offset = centered_scroll_offset(preview_pos, visible_height, total_items);

        for (i, source) in filtered.iter().skip(scroll_offset).take(visible_height) {
            let is_preview = app.discover.preview_source == Some(*i);
            let is_selected = *i == app.discover.selected_source_index();
            let prefix = if is_preview { "► " } else { "  " };

            let item_style = if is_preview {
                Style::default().fg(Color::White).bold()
            } else if is_selected {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::Gray)
            };

            let text = format!("{}{} ({})", prefix, source.name, source.file_count);
            lines.push(Line::from(Span::styled(text, item_style)));
        }
    }

    let list = Paragraph::new(lines);
    frame.render_widget(list, inner_chunks[1]);
}

/// Draw the Tags dropdown (collapsed or expanded)
fn draw_tags_dropdown(frame: &mut Frame, app: &App, area: Rect) {
    let is_open = app.discover.view_state == DiscoverViewState::TagsDropdown;
    let is_focused = app.discover.focus == DiscoverFocus::Tags;

    let style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    if is_open {
        let max_width = area.width.saturating_sub(4).min(72).max(30);
        let max_height = area.height.saturating_sub(6).min(20).max(10);
        let dialog_area = render_centered_dialog(frame, area, max_width, max_height);

        // Expanded dropdown with optional filter line
        let is_filtering = app.discover.tags_filtering;

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .border_type(BorderType::Double)
            .title(Span::styled(
            " [T] Select Tag ",
            Style::default().fg(Color::Cyan).bold(),
        ));
        let inner = block.inner(dialog_area);
        frame.render_widget(block, dialog_area);

        let inner_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .margin(1)
            .split(inner);

        // Top line: filter input OR hint
        if is_filtering {
            let filter_text = format!("/{}", app.discover.tags_filter);
            let filter_line = Paragraph::new(vec![Line::from(vec![
                Span::styled(filter_text, Style::default().fg(Color::Yellow)),
                Span::styled("█", Style::default().fg(Color::Yellow)),
            ])]);
            frame.render_widget(filter_line, inner_chunks[0]);
        } else if !app.discover.tags_filter.is_empty() {
            // Show active filter (but not in filter mode)
            let hint = format!("/{} (Enter:clear)", app.discover.tags_filter);
            let hint_line = Paragraph::new(hint).style(Style::default().fg(Color::DarkGray));
            frame.render_widget(hint_line, inner_chunks[0]);
        } else {
            // Show keybinding hints (same as Sources dropdown for consistency)
            let hint_line = Paragraph::new("[/]filter [↑/↓]nav [Enter]select [Esc]close")
                .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(hint_line, inner_chunks[0]);
        }

        // Filtered list
        let filtered: Vec<_> = app
            .discover
            .tags
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                app.discover.tags_filter.is_empty()
                    || t.name
                        .to_lowercase()
                        .contains(&app.discover.tags_filter.to_lowercase())
            })
            .collect();

        let mut lines: Vec<Line> = Vec::new();
        let visible_height = inner_chunks[1].height.saturating_sub(2) as usize;

        if filtered.is_empty() && !app.discover.tags.is_empty() {
            lines.push(Line::from(Span::styled(
                "  No matches",
                Style::default().fg(Color::DarkGray).italic(),
            )));
        } else if filtered.is_empty() {
            lines.push(Line::from(Span::styled(
                "  No tags yet",
                Style::default().fg(Color::DarkGray).italic(),
            )));
        } else {
            // Find scroll offset based on preview position - keep selection centered
            let total_items = filtered.len();
            let scroll_offset = if let Some(preview_idx) = app.discover.preview_tag {
                let preview_pos = filtered
                    .iter()
                    .position(|(i, _)| *i == preview_idx)
                    .unwrap_or(0);
                centered_scroll_offset(preview_pos, visible_height, total_items)
            } else {
                0
            };

            for (i, tag) in filtered.iter().skip(scroll_offset).take(visible_height) {
                let is_preview = app.discover.preview_tag == Some(*i);
                let is_selected = app.discover.selected_tag == Some(*i);
                let prefix = if is_preview { "► " } else { "  " };

                let item_style = if is_preview {
                    Style::default().fg(Color::White).bold()
                } else if is_selected {
                    Style::default().fg(Color::Magenta)
                } else if tag.is_special {
                    Style::default().fg(Color::Blue)
                } else {
                    Style::default().fg(Color::Gray)
                };

                let text = format!("{}{} ({})", prefix, tag.name, tag.count);
                lines.push(Line::from(Span::styled(text, item_style)));
            }
        }

        let list = Paragraph::new(lines);
        frame.render_widget(list, inner_chunks[1]);
    } else {
        // Collapsed: show selected tag or "All files"
        let selected_text = if let Some(tag_idx) = app.discover.selected_tag {
            if let Some(tag) = app.discover.tags.get(tag_idx) {
                format!("{} ({})", tag.name, tag.count)
            } else {
                "All files".to_string()
            }
        } else {
            // Get total count from first tag if available
            let count = app.discover.tags.first().map(|t| t.count).unwrap_or(0);
            format!("All files ({})", count)
        };

        // Use double borders when focused for visual prominence
        let (border_style, border_type) = if is_focused {
            (Style::default().fg(Color::Cyan), BorderType::Double)
        } else {
            (Style::default().fg(Color::DarkGray), BorderType::Rounded)
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .border_type(border_type)
            .title(Span::styled(" [T] Tags ", style.bold()));

        let content = Paragraph::new(selected_text)
            .style(if is_focused {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::Gray)
            })
            .block(block);

        frame.render_widget(content, area);
    }
}

/// Draw the Rules Manager dialog as an overlay
fn draw_rules_manager_dialog(frame: &mut Frame, app: &App, area: Rect) {
    let dialog_area = render_centered_dialog(frame, area, 60, 20);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Span::styled(
            " Tagging Rules ",
            Style::default().fg(Color::Cyan).bold(),
        ));

    let inner_area = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    // Split inner area: list + footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(2)])
        .split(inner_area);

    // Rules list
    let mut lines: Vec<Line> = Vec::new();

    if app.discover.rules.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  No tagging rules yet",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Press [n] to create one",
            Style::default().fg(Color::Cyan),
        )));
    } else {
        // Header
        lines.push(Line::from(vec![
            Span::styled("  Pattern", Style::default().fg(Color::DarkGray)),
            Span::raw("              "),
            Span::styled("Tag", Style::default().fg(Color::DarkGray)),
            Span::raw("        "),
            Span::styled("On", Style::default().fg(Color::DarkGray)),
        ]));
        lines.push(Line::from(Span::styled(
            "  ────────────────────────────────────────────",
            Style::default().fg(Color::DarkGray),
        )));

        for (i, rule) in app.discover.rules.iter().enumerate() {
            let is_selected = i == app.discover.selected_rule;
            let prefix = if is_selected { "► " } else { "  " };

            let enabled_mark = if rule.enabled { "✓" } else { " " };

            let style = if is_selected {
                Style::default().fg(Color::White).bold()
            } else if !rule.enabled {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::Gray)
            };

            // Truncate pattern if too long
            let pattern = if rule.pattern.len() > 18 {
                format!("{}...", &rule.pattern[..15])
            } else {
                format!("{:<18}", rule.pattern)
            };

            let tag = if rule.tag.len() > 10 {
                format!("{}...", &rule.tag[..7])
            } else {
                format!("{:<10}", rule.tag)
            };

            lines.push(Line::from(Span::styled(
                format!("{}{}  {}  {}", prefix, pattern, tag, enabled_mark),
                style,
            )));
        }
    }

    let rules_list = Paragraph::new(lines);
    frame.render_widget(rules_list, chunks[0]);

    // Footer with keybindings
    let footer = Paragraph::new(Line::from(vec![
        Span::styled(" [n]", Style::default().fg(Color::Cyan)),
        Span::raw(" New  "),
        Span::styled("[e]", Style::default().fg(Color::Cyan)),
        Span::raw(" Edit  "),
        Span::styled("[d]", Style::default().fg(Color::Cyan)),
        Span::raw(" Del  "),
        Span::styled("[Enter]", Style::default().fg(Color::Cyan)),
        Span::raw(" Toggle  "),
        Span::styled("[Esc]", Style::default().fg(Color::Cyan)),
        Span::raw(" Close"),
    ]))
    .alignment(Alignment::Center)
    .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(footer, chunks[1]);
}

/// Draw the Sources Manager dialog as an overlay (spec v1.7)
fn draw_sources_manager_dialog(frame: &mut Frame, app: &App, area: Rect) {
    let dialog_area = render_centered_dialog(frame, area, 65, 20);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Span::styled(
            " Sources Manager ",
            Style::default().fg(Color::Cyan).bold(),
        ));

    let inner_area = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    // Split inner area: list + footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(2)])
        .split(inner_area);

    // Sources list
    let mut lines: Vec<Line> = Vec::new();

    if app.discover.sources.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  No sources yet",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Press [n] to scan a directory",
            Style::default().fg(Color::Cyan),
        )));
    } else {
        // Header
        lines.push(Line::from(vec![
            Span::styled("  Name", Style::default().fg(Color::DarkGray)),
            Span::raw("                "),
            Span::styled("Path", Style::default().fg(Color::DarkGray)),
            Span::raw("                    "),
            Span::styled("Files", Style::default().fg(Color::DarkGray)),
        ]));
        lines.push(Line::from(Span::styled(
            "  ─────────────────────────────────────────────────────────",
            Style::default().fg(Color::DarkGray),
        )));

        for (i, source) in app.discover.sources.iter().enumerate() {
            let is_selected = i == app.discover.sources_manager_selected;
            let prefix = if is_selected { "► " } else { "  " };

            let style = if is_selected {
                Style::default().fg(Color::White).bold()
            } else {
                Style::default().fg(Color::Gray)
            };

            // Truncate name if too long
            let name = if source.name.len() > 16 {
                format!("{}...", &source.name[..13])
            } else {
                format!("{:<16}", source.name)
            };

            // Truncate path if too long
            let path_str = source.path.to_string_lossy();
            let path = if path_str.len() > 22 {
                format!("...{}", &path_str[path_str.len() - 19..])
            } else {
                format!("{:<22}", path_str)
            };

            lines.push(Line::from(Span::styled(
                format!("{}{}  {}  {:>5}", prefix, name, path, source.file_count),
                style,
            )));
        }
    }

    let sources_list = Paragraph::new(lines);
    frame.render_widget(sources_list, chunks[0]);

    // Footer with keybindings
    let footer = Paragraph::new(Line::from(vec![
        Span::styled(" [n]", Style::default().fg(Color::Cyan)),
        Span::raw(" New  "),
        Span::styled("[e]", Style::default().fg(Color::Cyan)),
        Span::raw(" Edit  "),
        Span::styled("[d]", Style::default().fg(Color::Cyan)),
        Span::raw(" Del  "),
        Span::styled("[r]", Style::default().fg(Color::Cyan)),
        Span::raw(" Rescan  "),
        Span::styled("[Esc]", Style::default().fg(Color::Cyan)),
        Span::raw(" Close"),
    ]))
    .alignment(Alignment::Center)
    .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(footer, chunks[1]);
}

/// Draw the Source Edit dialog as an overlay (spec v1.7)
fn draw_source_edit_dialog(frame: &mut Frame, app: &App, area: Rect) {
    let dialog_area = render_centered_dialog(frame, area, 50, 8);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(Span::styled(
            " Edit Source ",
            Style::default().fg(Color::Yellow).bold(),
        ));

    let inner_area = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    // Layout: name field + path (read-only) + footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Name field
            Constraint::Length(1), // Path (read-only)
            Constraint::Length(1), // Footer
        ])
        .split(inner_area);

    // Name input field
    let name_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Span::styled(" Name ", Style::default().fg(Color::Cyan)));

    let name_text = format!("{}█", app.discover.source_edit_input);
    let name_para = Paragraph::new(name_text)
        .style(Style::default().fg(Color::White))
        .block(name_block);
    frame.render_widget(name_para, chunks[0]);

    // Path (read-only)
    let path_text = if let Some(ref source_id) = app.discover.editing_source {
        app.discover
            .sources
            .iter()
            .find(|s| &s.id == source_id)
            .map(|s| format!("  Path: {} (read-only)", s.path.to_string_lossy()))
            .unwrap_or_default()
    } else {
        String::new()
    };
    let path_para = Paragraph::new(path_text).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(path_para, chunks[1]);

    // Footer
    let footer = Paragraph::new(" [Enter] Save  [Esc] Cancel ")
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(footer, chunks[2]);
}

/// Draw the Source Delete Confirmation dialog as an overlay (spec v1.7)
fn draw_source_delete_confirm_dialog(frame: &mut Frame, app: &App, area: Rect) {
    let dialog_area = render_centered_dialog(frame, area, 55, 10);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red))
        .title(Span::styled(
            " Delete Source ",
            Style::default().fg(Color::Red).bold(),
        ));

    let inner_area = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    // Get source info for display
    let (source_name, file_count) = app
        .discover
        .source_to_delete
        .as_ref()
        .and_then(|id| app.discover.sources.iter().find(|s| &s.id == id))
        .map(|s| (s.name.clone(), s.file_count))
        .unwrap_or_else(|| ("Unknown".to_string(), 0));

    // Content
    let content = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  Delete source \"{}\"?", source_name),
            Style::default().fg(Color::White).bold(),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!(
                "  This will remove the source and all {} tracked",
                file_count
            ),
            Style::default().fg(Color::Gray),
        )),
        Line::from(Span::styled(
            "  files from the database. Files on disk will NOT",
            Style::default().fg(Color::Gray),
        )),
        Line::from(Span::styled(
            "  be deleted.",
            Style::default().fg(Color::Gray),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(" [Enter/y]", Style::default().fg(Color::Red)),
            Span::raw(" Confirm  "),
            Span::styled("[Esc/n]", Style::default().fg(Color::Cyan)),
            Span::raw(" Cancel"),
        ]),
    ];

    let para = Paragraph::new(content);
    frame.render_widget(para, inner_area);
}

/// Draw the Add Source dialog as an overlay (improved UX for path input)
fn draw_add_source_dialog(frame: &mut Frame, app: &App, area: Rect) {
    // Calculate height based on content
    let suggestions_count = app.discover.path_suggestions.len().min(8);
    let has_suggestions = suggestions_count > 0;
    let has_error = app.discover.scan_error.is_some();

    // Base height + suggestions + error
    let dialog_height =
        9 + if has_suggestions {
            suggestions_count as u16 + 2
        } else {
            0
        } + if has_error { 2 } else { 0 };

    let dialog_area = render_centered_dialog(frame, area, 70, dialog_height);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Span::styled(
            " Add Source ",
            Style::default().fg(Color::Cyan).bold(),
        ));

    let inner_area = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    // Build content
    let mut content = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Enter the path to a folder to scan:",
            Style::default().fg(Color::White),
        )),
        Line::from(""),
    ];

    // Path input field with cursor
    let input = &app.discover.scan_path_input;
    let input_display = if input.is_empty() {
        "  Path: _".to_string()
    } else {
        format!("  Path: {}_", input)
    };
    content.push(Line::from(Span::styled(
        input_display,
        Style::default().fg(Color::Yellow).bold(),
    )));

    // Show suggestions if available
    if has_suggestions {
        content.push(Line::from(""));
        for (idx, suggestion) in app.discover.path_suggestions.iter().enumerate() {
            let is_selected = idx == app.discover.path_suggestion_idx;
            let prefix = if is_selected { "  ► " } else { "    " };
            let style = if is_selected {
                Style::default().fg(Color::Cyan).bold()
            } else {
                Style::default().fg(Color::DarkGray)
            };
            content.push(Line::from(Span::styled(
                format!("{}{}", prefix, suggestion),
                style,
            )));
        }
    }

    // Show error if present
    if has_error {
        if let Some(ref err) = app.discover.scan_error {
            content.push(Line::from(""));
            content.push(Line::from(Span::styled(
                format!("  Error: {}", err),
                Style::default().fg(Color::Red),
            )));
        }
    }

    // Help text
    content.push(Line::from(""));
    let help_text = if has_suggestions {
        "  [Tab] complete  [↑↓] select  [Enter] confirm  [Esc] cancel"
    } else {
        "  Tip: Use ~ for home directory (e.g., ~/Documents)"
    };
    content.push(Line::from(Span::styled(
        help_text,
        Style::default().fg(Color::DarkGray),
    )));

    let para = Paragraph::new(content);
    frame.render_widget(para, inner_area);
}

fn draw_scan_confirm_dialog(frame: &mut Frame, app: &App, area: Rect) {
    let dialog_area = render_centered_dialog(frame, area, 72, 9);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red))
        .title(Span::styled(
            " Confirm Scan ",
            Style::default().fg(Color::Red).bold(),
        ));

    let inner_area = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let path = app
        .discover
        .scan_confirm_path
        .as_deref()
        .unwrap_or("<unknown>");
    let path_display = truncate_path_start(path, inner_area.width.saturating_sub(4) as usize);

    let content = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  You are about to scan a high-risk path:",
            Style::default().fg(Color::White),
        )),
        Line::from(Span::styled(
            format!("  {}", path_display),
            Style::default().fg(Color::Yellow).bold(),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  This may take a long time and touch many files.",
            Style::default().fg(Color::Gray),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(" [Enter/y]", Style::default().fg(Color::Red)),
            Span::raw(" Confirm  "),
            Span::styled("[Esc/n]", Style::default().fg(Color::Cyan)),
            Span::raw(" Cancel"),
        ]),
    ];

    let para = Paragraph::new(content);
    frame.render_widget(para, inner_area);
}

/// Draw the scan started dialog as an overlay
fn draw_scanning_dialog(frame: &mut Frame, _app: &App, area: Rect) {
    let dialog_area = render_centered_dialog(frame, area, 60, 7);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green))
        .title(Span::styled(
            " Scan Started ",
            Style::default().fg(Color::Green).bold(),
        ));

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let content = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Scan started.",
            Style::default().fg(Color::White),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  View progress in Jobs: press [2]",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    frame.render_widget(Paragraph::new(content), inner);
}

fn tag_suggestions<'a>(input: &str, tags: &'a [String], limit: usize) -> Vec<&'a str> {
    if input.is_empty() {
        return Vec::new();
    }

    let input_lower = input.to_lowercase();
    tags.iter()
        .filter(|tag| tag.to_lowercase().starts_with(&input_lower))
        .take(limit)
        .map(|tag| tag.as_str())
        .collect()
}

fn draw_filter_dialog(frame: &mut Frame, app: &App, area: Rect) {
    let dialog_area = render_centered_dialog(frame, area, 60, 7);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(Span::styled(
            " Filter Files ",
            Style::default().fg(Color::Yellow).bold(),
        ));

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let filter_text = format!("/{}", app.discover.filter);
    let input_line = Line::from(vec![
        Span::raw("  "),
        Span::styled(filter_text, Style::default().fg(Color::Yellow).bold()),
        Span::styled("█", Style::default().fg(Color::Yellow)),
    ]);

    let content = vec![
        Line::from(""),
        input_line,
        Line::from(""),
        Line::from(Span::styled(
            "  Matches substrings or globs (e.g., *.csv)",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(Span::styled(
            "  [Enter] Apply  [Esc] Cancel",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    frame.render_widget(Paragraph::new(content), inner);
}

fn draw_tagging_dialog(frame: &mut Frame, app: &App, area: Rect) {
    let suggestions = tag_suggestions(&app.discover.tag_input, &app.discover.available_tags, 4);
    let suggestion_rows = if suggestions.is_empty() {
        0
    } else {
        suggestions.len() as u16 + 2
    };
    let dialog_height = 9 + suggestion_rows;
    let dialog_area = render_centered_dialog(frame, area, 70, dialog_height);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta))
        .title(Span::styled(
            " Apply Tag ",
            Style::default().fg(Color::Magenta).bold(),
        ));

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let filtered = app.filtered_files();
    let selected_path = filtered
        .get(app.discover.selected)
        .map(|file| file.rel_path.as_str())
        .unwrap_or("<no file selected>");
    let path_display = truncate_path_start(selected_path, inner.width.saturating_sub(4) as usize);

    let tag_input = if app.discover.tag_input.is_empty() {
        "  Tag: _".to_string()
    } else {
        format!("  Tag: {}_", app.discover.tag_input)
    };

    let mut content = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Apply tag to file:",
            Style::default().fg(Color::White),
        )),
        Line::from(Span::styled(
            format!("  {}", path_display),
            Style::default().fg(Color::Gray),
        )),
        Line::from(""),
        Line::from(Span::styled(
            tag_input,
            Style::default().fg(Color::Magenta).bold(),
        )),
    ];

    if !suggestions.is_empty() {
        content.push(Line::from(""));
        content.push(Line::from(Span::styled(
            "  Suggestions:",
            Style::default().fg(Color::DarkGray),
        )));
        for suggestion in suggestions {
            content.push(Line::from(Span::styled(
                format!("   - {}", suggestion),
                Style::default().fg(Color::Magenta),
            )));
        }
    }

    content.push(Line::from(""));
    content.push(Line::from(Span::styled(
        "  [Tab] autocomplete  [Enter] apply  [Esc] cancel",
        Style::default().fg(Color::DarkGray),
    )));

    frame.render_widget(Paragraph::new(content), inner);
}

fn draw_bulk_tag_dialog(frame: &mut Frame, app: &App, area: Rect) {
    let suggestions =
        tag_suggestions(&app.discover.bulk_tag_input, &app.discover.available_tags, 4);
    let suggestion_rows = if suggestions.is_empty() {
        0
    } else {
        suggestions.len() as u16 + 2
    };
    let dialog_height = 10 + suggestion_rows;
    let dialog_area = render_centered_dialog(frame, area, 72, dialog_height);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta))
        .title(Span::styled(
            " Bulk Tag ",
            Style::default().fg(Color::Magenta).bold(),
        ));

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let file_count = app.filtered_files().len();
    let tag_input = if app.discover.bulk_tag_input.is_empty() {
        "  Tag: _".to_string()
    } else {
        format!("  Tag: {}_", app.discover.bulk_tag_input)
    };
    let save_rule_mark = if app.discover.bulk_tag_save_as_rule {
        "[x]"
    } else {
        "[ ]"
    };

    let mut content = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  Apply tag to {} files", file_count),
            Style::default().fg(Color::White),
        )),
        Line::from(""),
        Line::from(Span::styled(
            tag_input,
            Style::default().fg(Color::Magenta).bold(),
        )),
    ];

    if !suggestions.is_empty() {
        content.push(Line::from(""));
        content.push(Line::from(Span::styled(
            "  Suggestions:",
            Style::default().fg(Color::DarkGray),
        )));
        for suggestion in suggestions {
            content.push(Line::from(Span::styled(
                format!("   - {}", suggestion),
                Style::default().fg(Color::Magenta),
            )));
        }
    }

    content.push(Line::from(""));
    content.push(Line::from(Span::styled(
        format!("  {} Save as rule", save_rule_mark),
        Style::default().fg(Color::DarkGray),
    )));
    content.push(Line::from(Span::styled(
        "  [Space] toggle  [Enter] apply  [Esc] cancel",
        Style::default().fg(Color::DarkGray),
    )));

    frame.render_widget(Paragraph::new(content), inner);
}

fn draw_create_source_dialog(frame: &mut Frame, app: &App, area: Rect) {
    let dialog_area = render_centered_dialog(frame, area, 70, 9);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Span::styled(
            " Create Source ",
            Style::default().fg(Color::Cyan).bold(),
        ));

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let path = app
        .discover
        .pending_source_path
        .as_deref()
        .unwrap_or("<no directory selected>");
    let path_display = truncate_path_start(path, inner.width.saturating_sub(4) as usize);

    let name_input = if app.discover.source_name_input.is_empty() {
        "  Name: _".to_string()
    } else {
        format!("  Name: {}_", app.discover.source_name_input)
    };

    let content = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Create source from directory:",
            Style::default().fg(Color::White),
        )),
        Line::from(Span::styled(
            format!("  {}", path_display),
            Style::default().fg(Color::Gray),
        )),
        Line::from(""),
        Line::from(Span::styled(
            name_input,
            Style::default().fg(Color::Yellow).bold(),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  [Enter] Create  [Esc] Cancel",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    frame.render_widget(Paragraph::new(content), inner);
}

/// Draw the Rule Builder as a full-screen view (replaces Discover, not overlay)
/// Layout: Header | Split View (40% left / 60% right) | Footer
fn draw_rule_builder_screen(frame: &mut Frame, app: &App, area: Rect) {
    let builder = match &app.discover.rule_builder {
        Some(b) => b,
        None => return,
    };

    // Main layout: Header (2) | Content (flex)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Header (step band)
            Constraint::Min(0),    // Content (split view)
        ])
        .split(area);

    let source_name = app
        .discover
        .selected_source()
        .map(|s| s.name.clone())
        .unwrap_or_else(|| "Select source".to_string());
    let match_count = builder.match_count;
    let error_hint = app
        .discover
        .status_message
        .as_ref()
        .and_then(|(msg, is_error)| if *is_error { Some(msg.as_str()) } else { None });

    let (range_start, range_end, total_files) = app.discover_page_bounds();
    let file_range = if total_files == 0 {
        "Files: 0".to_string()
    } else {
        format!("Files: {}-{} of {}", range_start, range_end, total_files)
    };

    let mut header_text = format!(
        " [Select] [Rules] [Validate]  |  Source: {}  |  Matches: {}  |  {}",
        source_name, match_count, file_range
    );
    if let Some(msg) = error_hint {
        header_text.push_str(&format!("  |  ⚠ {}", msg));
    }

    let header = Paragraph::new(truncate_end(&header_text, chunks[0].width as usize))
        .style(Style::default().fg(Color::Cyan).bold())
        .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(header, chunks[0]);

    // === CONTENT: Split View ===
    let content_area = chunks[1];

    // Split into left (40%) and right (60%) panels
    let h_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(content_area);

    // Left panel: Tab-specific rule config
    match app.ingest_tab {
        IngestTab::Select => draw_rule_builder_left_panel_select(frame, builder, h_chunks[0]),
        IngestTab::Rules | IngestTab::Sources => {
            draw_rule_builder_left_panel(frame, builder, h_chunks[0])
        }
        IngestTab::Validate => draw_rule_builder_left_panel_validate(frame, builder, h_chunks[0]),
    }

    // Right panel: Tab-specific context
    match app.ingest_tab {
        IngestTab::Rules => draw_rule_builder_right_panel_rules(frame, builder, h_chunks[1]),
        _ => draw_rule_builder_right_panel(
            frame,
            builder,
            h_chunks[1],
            app.discover.scan_error.as_deref(),
        ),
    }

}

fn draw_discover_inspector(frame: &mut Frame, app: &App, area: Rect) {
    let builder = match &app.discover.rule_builder {
        Some(b) => b,
        None => {
            let para = Paragraph::new("No rule builder state.")
                .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(para, area);
            return;
        }
    };

    let source_name = app
        .discover
        .selected_source()
        .map(|s| s.name.as_str())
        .unwrap_or("(none)");
    let tag_label = app
        .discover
        .selected_tag
        .and_then(|idx| app.discover.tags.get(idx))
        .map(|t| t.name.as_str())
        .unwrap_or("All");
    let dirty = if builder.dirty { "yes" } else { "no" };
    let editing = if builder.editing_rule_id.is_some() {
        "yes"
    } else {
        "no"
    };

    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        format!("Source: {}", source_name),
        Style::default().fg(Color::White),
    )));
    lines.push(Line::from(Span::styled(
        format!("Tag: {}", tag_label),
        Style::default().fg(Color::Gray),
    )));
    lines.push(Line::from(Span::styled(
        format!("Matches: {}", builder.match_count),
        Style::default().fg(Color::Gray),
    )));
    lines.push(Line::from(Span::styled(
        format!("Dirty: {}  Editing: {}", dirty, editing),
        Style::default().fg(Color::Gray),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("Pattern: {}", truncate_end(&builder.pattern, 32)),
        Style::default().fg(Color::Gray),
    )));
    if !builder.tag.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("Rule tag: {}", truncate_end(&builder.tag, 32)),
            Style::default().fg(Color::Gray),
        )));
    }

    let para = Paragraph::new(lines).wrap(Wrap { trim: true });
    frame.render_widget(para, area);
}

/// Draw the left panel of Rule Builder (PATTERN, EXCLUDES, TAG, EXTRACTIONS, OPTIONS)
fn draw_rule_builder_left_panel(
    frame: &mut Frame,
    builder: &super::extraction::RuleBuilderState,
    area: Rect,
) {
    use super::extraction::RuleBuilderFocus;

    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Pattern
            Constraint::Length(4), // Excludes
            Constraint::Length(3), // Tag
            Constraint::Length(5), // Extractions
            Constraint::Length(3), // Options
            Constraint::Min(8),    // Schema Suggestions (new)
        ])
        .split(area);

    // --- PATTERN field ---
    let pattern_focused = matches!(builder.focus, RuleBuilderFocus::Pattern);
    let pattern_style = if pattern_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let pattern_title = if let Some(ref err) = builder.pattern_error {
        format!(" PATTERN [!] {} ", err)
    } else if pattern_focused {
        " PATTERN (editing) ".to_string()
    } else {
        " PATTERN ".to_string()
    };

    let pattern_block = Block::default()
        .borders(Borders::ALL)
        .border_type(if pattern_focused {
            BorderType::Double
        } else {
            BorderType::Plain
        })
        .border_style(if builder.pattern_error.is_some() {
            Style::default().fg(Color::Red)
        } else if pattern_focused {
            Style::default().fg(Color::Cyan).bold()
        } else {
            pattern_style
        })
        .title(Span::styled(
            pattern_title,
            if pattern_focused {
                Style::default().fg(Color::Cyan).bold()
            } else {
                pattern_style
            },
        ));

    let pattern_text = if pattern_focused {
        format!("{}█", builder.pattern)
    } else {
        builder.pattern.clone()
    };
    let pattern_para = Paragraph::new(pattern_text)
        .style(Style::default().fg(Color::White))
        .block(pattern_block);
    frame.render_widget(pattern_para, left_chunks[0]);

    // --- EXCLUDES field ---
    let excludes_focused = matches!(
        builder.focus,
        RuleBuilderFocus::Excludes | RuleBuilderFocus::ExcludeInput
    );
    let excludes_style = if excludes_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let excludes_title = if excludes_focused {
        format!(
            " EXCLUDES ({}) [Enter: add, d: delete] ",
            builder.excludes.len()
        )
    } else {
        format!(" EXCLUDES ({}) ", builder.excludes.len())
    };
    let excludes_block = Block::default()
        .borders(Borders::ALL)
        .border_type(if excludes_focused {
            BorderType::Double
        } else {
            BorderType::Plain
        })
        .border_style(if excludes_focused {
            Style::default().fg(Color::Cyan).bold()
        } else {
            excludes_style
        })
        .title(Span::styled(
            excludes_title,
            if excludes_focused {
                Style::default().fg(Color::Cyan).bold()
            } else {
                excludes_style
            },
        ));

    let excludes_content = if matches!(builder.focus, RuleBuilderFocus::ExcludeInput) {
        format!("{}█", builder.exclude_input)
    } else if builder.excludes.is_empty() {
        "Press [Enter] to add exclude pattern".to_string()
    } else {
        builder
            .excludes
            .iter()
            .enumerate()
            .map(|(i, e)| {
                if i == builder.selected_exclude && excludes_focused {
                    format!("► {}", e)
                } else {
                    format!("  {}", e)
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let excludes_para = Paragraph::new(excludes_content)
        .style(Style::default().fg(Color::Gray))
        .block(excludes_block);
    frame.render_widget(excludes_para, left_chunks[1]);

    // --- TAG field ---
    let tag_focused = matches!(builder.focus, RuleBuilderFocus::Tag);
    let tag_style = if tag_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let tag_title = if tag_focused {
        " TAG (editing) "
    } else {
        " TAG "
    };
    let tag_block = Block::default()
        .borders(Borders::ALL)
        .border_type(if tag_focused {
            BorderType::Double
        } else {
            BorderType::Plain
        })
        .border_style(if tag_focused {
            Style::default().fg(Color::Cyan).bold()
        } else {
            tag_style
        })
        .title(Span::styled(
            tag_title,
            if tag_focused {
                Style::default().fg(Color::Cyan).bold()
            } else {
                tag_style
            },
        ));

    let tag_text = if tag_focused {
        if builder.tag.is_empty() {
            "█".to_string() // Show cursor in empty field
        } else {
            format!("{}█", builder.tag)
        }
    } else if builder.tag.is_empty() {
        "Type tag name to apply to matched files".to_string()
    } else {
        builder.tag.clone()
    };
    let tag_para = Paragraph::new(tag_text)
        .style(if builder.tag.is_empty() && !tag_focused {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::White)
        })
        .block(tag_block);
    frame.render_widget(tag_para, left_chunks[2]);

    // --- EXTRACTIONS field ---
    let extractions_focused = matches!(
        builder.focus,
        RuleBuilderFocus::Extractions | RuleBuilderFocus::ExtractionEdit(_)
    );
    let extractions_style = if extractions_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let extractions_title = if extractions_focused {
        format!(
            " EXTRACTIONS ({}) [Space: toggle] ",
            builder.extractions.len()
        )
    } else {
        format!(" EXTRACTIONS ({}) ", builder.extractions.len())
    };
    let extractions_block = Block::default()
        .borders(Borders::ALL)
        .border_type(if extractions_focused {
            BorderType::Double
        } else {
            BorderType::Plain
        })
        .border_style(if extractions_focused {
            Style::default().fg(Color::Cyan).bold()
        } else {
            extractions_style
        })
        .title(Span::styled(
            extractions_title,
            if extractions_focused {
                Style::default().fg(Color::Cyan).bold()
            } else {
                extractions_style
            },
        ));

    let extractions_content = if builder.extractions.is_empty() {
        "Add <name> to pattern, e.g. **/<year>/*.csv".to_string()
    } else {
        builder
            .extractions
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let selected = i == builder.selected_extraction && extractions_focused;
                let prefix = if selected { "► " } else { "  " };
                let enabled = if f.enabled { "✓" } else { " " };
                format!(
                    "{}{} {} {}",
                    prefix,
                    f.name,
                    f.source.display_name(),
                    enabled
                )
            })
            .take(3)
            .collect::<Vec<_>>()
            .join("\n")
    };
    let extractions_para = Paragraph::new(extractions_content)
        .style(Style::default().fg(Color::Gray))
        .block(extractions_block);
    frame.render_widget(extractions_para, left_chunks[3]);

    // --- OPTIONS field ---
    let options_focused = matches!(builder.focus, RuleBuilderFocus::Options);
    let options_style = if options_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let options_title = if options_focused {
        " OPTIONS [Space: toggle] "
    } else {
        " OPTIONS "
    };
    let options_block = Block::default()
        .borders(Borders::ALL)
        .border_type(if options_focused {
            BorderType::Double
        } else {
            BorderType::Plain
        })
        .border_style(if options_focused {
            Style::default().fg(Color::Cyan).bold()
        } else {
            options_style
        })
        .title(Span::styled(
            options_title,
            if options_focused {
                Style::default().fg(Color::Cyan).bold()
            } else {
                options_style
            },
        ));

    let enabled_check = if builder.enabled { "[x]" } else { "[ ]" };
    let job_check = if builder.run_job_on_save {
        "[x]"
    } else {
        "[ ]"
    };
    let options_para = Paragraph::new(format!(
        "{} Enable rule  {} Run job on save",
        enabled_check, job_check
    ))
    .style(if options_focused {
        Style::default().fg(Color::White)
    } else {
        Style::default().fg(Color::Gray)
    })
    .block(options_block);
    frame.render_widget(options_para, left_chunks[4]);

    // --- SCHEMA SUGGESTIONS (new per RULE_BUILDER_UI_PLAN.md) ---
    draw_schema_suggestions(frame, builder, left_chunks[5]);
}

/// Simplified left panel for Select tab (pattern + excludes + selection summary)
fn draw_rule_builder_left_panel_select(
    frame: &mut Frame,
    builder: &super::extraction::RuleBuilderState,
    area: Rect,
) {
    use super::extraction::RuleBuilderFocus;

    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Pattern
            Constraint::Length(4), // Excludes
            Constraint::Min(6),    // Selection summary
        ])
        .split(area);

    // --- PATTERN field ---
    let pattern_focused = matches!(builder.focus, RuleBuilderFocus::Pattern);
    let pattern_style = if pattern_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let pattern_title = if let Some(ref err) = builder.pattern_error {
        format!(" PATTERN [!] {} ", err)
    } else if pattern_focused {
        " PATTERN (editing) ".to_string()
    } else {
        " PATTERN ".to_string()
    };

    let pattern_block = Block::default()
        .borders(Borders::ALL)
        .border_type(if pattern_focused {
            BorderType::Double
        } else {
            BorderType::Plain
        })
        .border_style(if builder.pattern_error.is_some() {
            Style::default().fg(Color::Red)
        } else if pattern_focused {
            Style::default().fg(Color::Cyan).bold()
        } else {
            pattern_style
        })
        .title(Span::styled(
            pattern_title,
            if pattern_focused {
                Style::default().fg(Color::Cyan).bold()
            } else {
                pattern_style
            },
        ));

    let pattern_text = if pattern_focused {
        format!("{}█", builder.pattern)
    } else {
        builder.pattern.clone()
    };
    let pattern_para = Paragraph::new(pattern_text)
        .style(Style::default().fg(Color::White))
        .block(pattern_block);
    frame.render_widget(pattern_para, left_chunks[0]);

    // --- EXCLUDES field ---
    let excludes_focused = matches!(
        builder.focus,
        RuleBuilderFocus::Excludes | RuleBuilderFocus::ExcludeInput
    );
    let excludes_style = if excludes_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let excludes_title = if excludes_focused {
        format!(
            " EXCLUDES ({}) [Enter: add, d: delete] ",
            builder.excludes.len()
        )
    } else {
        format!(" EXCLUDES ({}) ", builder.excludes.len())
    };
    let excludes_block = Block::default()
        .borders(Borders::ALL)
        .border_type(if excludes_focused {
            BorderType::Double
        } else {
            BorderType::Plain
        })
        .border_style(if excludes_focused {
            Style::default().fg(Color::Cyan).bold()
        } else {
            excludes_style
        })
        .title(Span::styled(
            excludes_title,
            if excludes_focused {
                Style::default().fg(Color::Cyan).bold()
            } else {
                excludes_style
            },
        ));

    let excludes_content = if matches!(builder.focus, RuleBuilderFocus::ExcludeInput) {
        format!("{}█", builder.exclude_input)
    } else if builder.excludes.is_empty() {
        "Press [Enter] to add exclude pattern".to_string()
    } else {
        builder
            .excludes
            .iter()
            .enumerate()
            .map(|(i, e)| {
                if i == builder.selected_exclude && excludes_focused {
                    format!("► {}", e)
                } else {
                    format!("  {}", e)
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let excludes_para = Paragraph::new(excludes_content)
        .style(Style::default().fg(Color::Gray))
        .block(excludes_block);
    frame.render_widget(excludes_para, left_chunks[1]);

    // --- SELECTION summary ---
    let summary_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(
            " SELECTION ",
            Style::default().fg(Color::DarkGray),
        ));
    let summary_inner = summary_block.inner(left_chunks[2]);
    frame.render_widget(summary_block, left_chunks[2]);

    let lines = vec![
        Line::from(format!(" Matches: {}", builder.match_count)),
        Line::from(" Focus: pattern + excludes"),
        Line::from(" [s] scan source  [Enter] drill-down"),
    ];
    let para = Paragraph::new(lines)
        .style(Style::default().fg(Color::Gray))
        .wrap(Wrap { trim: true });
    frame.render_widget(para, summary_inner);
}

/// Simplified left panel for Validate tab (pattern + validation summary)
fn draw_rule_builder_left_panel_validate(
    frame: &mut Frame,
    builder: &super::extraction::RuleBuilderState,
    area: Rect,
) {
    use super::extraction::{FileResultsState, RuleBuilderFocus};

    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Pattern
            Constraint::Min(6),    // Validation summary
        ])
        .split(area);

    // --- PATTERN field ---
    let pattern_focused = matches!(builder.focus, RuleBuilderFocus::Pattern);
    let pattern_style = if pattern_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let pattern_title = if let Some(ref err) = builder.pattern_error {
        format!(" PATTERN [!] {} ", err)
    } else if pattern_focused {
        " PATTERN (editing) ".to_string()
    } else {
        " PATTERN ".to_string()
    };

    let pattern_block = Block::default()
        .borders(Borders::ALL)
        .border_type(if pattern_focused {
            BorderType::Double
        } else {
            BorderType::Plain
        })
        .border_style(if builder.pattern_error.is_some() {
            Style::default().fg(Color::Red)
        } else if pattern_focused {
            Style::default().fg(Color::Cyan).bold()
        } else {
            pattern_style
        })
        .title(Span::styled(
            pattern_title,
            if pattern_focused {
                Style::default().fg(Color::Cyan).bold()
            } else {
                pattern_style
            },
        ));

    let pattern_text = if pattern_focused {
        format!("{}█", builder.pattern)
    } else {
        builder.pattern.clone()
    };
    let pattern_para = Paragraph::new(pattern_text)
        .style(Style::default().fg(Color::White))
        .block(pattern_block);
    frame.render_widget(pattern_para, left_chunks[0]);

    // --- VALIDATION summary ---
    let summary_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(
            " VALIDATION ",
            Style::default().fg(Color::DarkGray),
        ));
    let summary_inner = summary_block.inner(left_chunks[1]);
    frame.render_widget(summary_block, left_chunks[1]);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(format!(" Matches: {}", builder.match_count)));
    match &builder.file_results {
        FileResultsState::Exploration { folder_matches, .. } => {
            let folder_count = folder_matches.len();
            lines.push(Line::from(format!(" Folders: {}", folder_count)));
            lines.push(Line::from(" Run a scan to preview results"));
        }
        FileResultsState::ExtractionPreview { preview_files } => {
            let warning_count: usize = preview_files.iter().map(|f| f.warnings.len()).sum();
            lines.push(Line::from(format!(" Preview files: {}", preview_files.len())));
            lines.push(Line::from(format!(" Warnings: {}", warning_count)));
        }
        FileResultsState::BacktestResults { backtest, .. } => {
            lines.push(Line::from(format!(" Pass: {}", backtest.pass_count)));
            lines.push(Line::from(format!(" Fail: {}", backtest.fail_count)));
            lines.push(Line::from(format!(" Skip: {}", backtest.excluded_count)));
        }
    }
    lines.push(Line::from(" Review results in the right panel"));

    let para = Paragraph::new(lines)
        .style(Style::default().fg(Color::Gray))
        .wrap(Wrap { trim: true });
    frame.render_widget(para, summary_inner);
}

/// Draw schema suggestions panel (RULE_BUILDER_UI_PLAN.md)
/// Shows pattern seeds, path archetypes, naming schemes, and synonym suggestions.
fn draw_schema_suggestions(
    frame: &mut Frame,
    builder: &super::extraction::RuleBuilderState,
    area: Rect,
) {
    use super::extraction::RuleBuilderFocus;
    use super::extraction::SuggestionSection;
    use super::extraction::SynonymConfidence;

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(
            " SUGGESTIONS ",
            Style::default().fg(Color::DarkGray),
        ));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Build content lines
    let mut lines: Vec<Line> = Vec::new();

    let suggestions_focused = builder.focus == RuleBuilderFocus::Suggestions;
    let header_line = |label: &str, selected: bool, show_help: bool| -> Line<'static> {
        let mut spans = Vec::new();
        if selected {
            spans.push(Span::styled("▸ ", Style::default().fg(Color::Cyan).bold()));
        } else {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(
            label.to_string(),
            Style::default().fg(Color::Yellow).bold(),
        ));
        if selected && show_help {
            spans.push(Span::raw("  "));
            spans.push(Span::styled("[?]", Style::default().fg(Color::DarkGray)));
        }
        Line::from(spans)
    };

    // Pattern Seeds (show top 3)
    if !builder.pattern_seeds.is_empty() {
        lines.push(header_line(
            "Detected Patterns",
            builder.suggestions_section == SuggestionSection::Patterns,
            suggestions_focused,
        ));
        for (idx, seed) in builder.pattern_seeds.iter().take(3).enumerate() {
            let selected = suggestions_focused
                && builder.suggestions_section == SuggestionSection::Patterns
                && idx == builder.selected_pattern_seed;
            let pattern_style = if selected {
                Style::default().fg(Color::Cyan).bold()
            } else {
                Style::default().fg(Color::Cyan)
            };
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(&seed.pattern, pattern_style),
                Span::styled(
                    format!(" ({})", seed.match_count),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        }
    }

    // Path Archetypes (show top 2)
    if !builder.path_archetypes.is_empty() && lines.len() < inner.height as usize - 2 {
        if !lines.is_empty() {
            lines.push(Line::from(""));
        }
        lines.push(header_line(
            "Detected Structures",
            builder.suggestions_section == SuggestionSection::Structures,
            suggestions_focused,
        ));
        let archetypes: Vec<_> = builder.path_archetypes.iter().take(2).collect();
        let display_paths: Vec<String> = archetypes
            .iter()
            .map(|arch| {
                arch.sample_paths
                    .first()
                    .cloned()
                    .unwrap_or_else(|| arch.template.clone())
            })
            .collect();
        let max_meta_width = archetypes
            .iter()
            .map(|arch| format!(" ({} files)", arch.file_count).chars().count())
            .max()
            .unwrap_or(0) as u16;
        let max_template_width = inner
            .width
            .saturating_sub(2)
            .saturating_sub(max_meta_width)
            .max(10) as usize;
        let formatted_paths = format_path_list(&display_paths, max_template_width, 3);

        for (idx, (arch, template)) in archetypes
            .into_iter()
            .zip(formatted_paths.into_iter())
            .enumerate()
        {
            let selected = suggestions_focused
                && builder.suggestions_section == SuggestionSection::Structures
                && idx == builder.selected_archetype;
            let template_style = if selected {
                Style::default().fg(Color::Green).bold()
            } else {
                Style::default().fg(Color::Green)
            };
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(template, template_style),
                Span::styled(
                    format!(" ({} files)", arch.file_count),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        }
    }

    // Naming Schemes (show top 2)
    if !builder.naming_schemes.is_empty() && lines.len() < inner.height as usize - 2 {
        if !lines.is_empty() {
            lines.push(Line::from(""));
        }
        lines.push(header_line(
            "Detected Filenames",
            builder.suggestions_section == SuggestionSection::Filenames,
            suggestions_focused,
        ));
        let max_meta_width = builder
            .naming_schemes
            .iter()
            .take(2)
            .map(|scheme| format!(" ({})", scheme.file_count).chars().count())
            .max()
            .unwrap_or(0) as u16;
        let max_template_width = inner
            .width
            .saturating_sub(2)
            .saturating_sub(max_meta_width)
            .max(10) as usize;
        for (idx, scheme) in builder.naming_schemes.iter().take(2).enumerate() {
            let template = if scheme.template.chars().count() > max_template_width {
                truncate_start(&scheme.template, max_template_width)
            } else {
                scheme.template.clone()
            };
            let selected = suggestions_focused
                && builder.suggestions_section == SuggestionSection::Filenames
                && idx == builder.selected_naming_scheme;
            let template_style = if selected {
                Style::default().fg(Color::Magenta).bold()
            } else {
                Style::default().fg(Color::Magenta)
            };
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(template, template_style),
                Span::styled(
                    format!(" ({})", scheme.file_count),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        }
    }

    // Synonym Suggestions (show top 2)
    if !builder.synonym_suggestions.is_empty() && lines.len() < inner.height as usize - 2 {
        if !lines.is_empty() {
            lines.push(Line::from(""));
        }
        lines.push(header_line(
            "Detected Synonyms",
            builder.suggestions_section == SuggestionSection::Synonyms,
            suggestions_focused,
        ));
        for (idx, syn) in builder.synonym_suggestions.iter().take(2).enumerate() {
            let conf_style = match syn.confidence {
                SynonymConfidence::High => Style::default().fg(Color::Green),
                SynonymConfidence::Medium => Style::default().fg(Color::Yellow),
                SynonymConfidence::Low => Style::default().fg(Color::DarkGray),
            };
            let selected = suggestions_focused
                && builder.suggestions_section == SuggestionSection::Synonyms
                && idx == builder.selected_synonym;
            let token_style = if selected {
                Style::default().fg(Color::White).bold()
            } else {
                Style::default().fg(Color::White)
            };
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(&syn.short_form, token_style),
                Span::raw(" → "),
                Span::styled(&syn.canonical_form, token_style),
                Span::raw(" "),
                Span::styled(
                    match syn.confidence {
                        SynonymConfidence::High => "●",
                        SynonymConfidence::Medium => "◐",
                        SynonymConfidence::Low => "○",
                    },
                    conf_style,
                ),
            ]));
        }
    }

    // Show placeholder if no suggestions
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "  Scan files to see suggestions",
            Style::default().fg(Color::DarkGray).italic(),
        )));
    }

    let para = Paragraph::new(lines).style(Style::default());
    frame.render_widget(para, inner);
}

/// Draw the right panel for Rules tab (summary view)
fn draw_rule_builder_right_panel_rules(
    frame: &mut Frame,
    builder: &super::extraction::RuleBuilderState,
    area: Rect,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" RULES SUMMARY ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    let tag_display = if builder.tag.is_empty() {
        "-".to_string()
    } else {
        builder.tag.clone()
    };
    lines.push(Line::from(format!(
        " Excludes: {}  Tag: {}",
        builder.excludes.len(),
        tag_display
    )));
    lines.push(Line::from(format!(
        " Extractions: {}  Enabled: {}",
        builder.extractions.len(),
        if builder.enabled { "yes" } else { "no" }
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(" Fields:"));

    if builder.extractions.is_empty() {
        lines.push(Line::from("  No extraction fields yet"));
    } else {
        for field in &builder.extractions {
            let enabled = if field.enabled { "[x]" } else { "[ ]" };
            let line = format!(
                "  {} {}: {}",
                enabled,
                field.name,
                field.field_type.display_name()
            );
            lines.push(Line::from(truncate_end(&line, inner.width as usize)));
            if let Some(pattern) = &field.pattern {
                let pattern_line = format!("     pattern: {}", pattern);
                lines.push(Line::from(Span::styled(
                    truncate_end(&pattern_line, inner.width as usize),
                    Style::default().fg(Color::DarkGray),
                )));
            }
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " Preview results in Select/Validate tabs",
        Style::default().fg(Color::DarkGray),
    )));

    let paragraph = Paragraph::new(lines)
        .style(Style::default().fg(Color::Gray))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, inner);
}

/// Draw the right panel of Rule Builder (file results list)
fn draw_rule_builder_right_panel(
    frame: &mut Frame,
    builder: &super::extraction::RuleBuilderState,
    area: Rect,
    scan_error: Option<&str>,
) {
    use super::extraction::{FileResultsState, RuleBuilderFocus};

    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Header with filter
            Constraint::Min(1),    // File list
            Constraint::Length(2), // Status bar
        ])
        .split(area);

    let file_list_focused = matches!(builder.focus, RuleBuilderFocus::FileList);

    // Phase-specific header
    let header_text = match &builder.file_results {
        FileResultsState::Exploration { folder_matches, .. } => {
            let folder_count = folder_matches.len();
            let file_count: usize = folder_matches.iter().map(|f| f.count).sum();
            if builder.is_streaming {
                format!(
                    " FOLDERS  {} folders ({} files)  {}",
                    folder_count,
                    file_count,
                    spinner_char(builder.stream_elapsed_ms / 100)
                )
            } else {
                format!(
                    " FOLDERS  {} folders ({} files)  [Enter] drill down",
                    folder_count, file_count
                )
            }
        }
        FileResultsState::ExtractionPreview { preview_files } => {
            let file_count = preview_files.len();
            let warning_count: usize = preview_files.iter().map(|f| f.warnings.len()).sum();
            if warning_count > 0 {
                format!(
                    " PREVIEW  {} files  ⚠ {} warnings  t:tag",
                    file_count, warning_count
                )
            } else {
                format!(" PREVIEW  {} files  ✓ extractions OK  t:tag", file_count)
            }
        }
        FileResultsState::BacktestResults { result_filter, .. } => {
            let filter_label = result_filter.label();
            format!(" RESULTS [{}]  a/p/f to filter", filter_label)
        }
    };

    let header = Paragraph::new(header_text).style(Style::default().fg(if file_list_focused {
        Color::Cyan
    } else {
        Color::DarkGray
    }));
    frame.render_widget(header, right_chunks[0]);

    // File list block
    let file_block = Block::default()
        .borders(Borders::ALL)
        .border_style(if file_list_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        });

    let file_list_inner = file_block.inner(right_chunks[1]);
    frame.render_widget(file_block, right_chunks[1]);

    // Phase-specific content rendering
    match &builder.file_results {
        FileResultsState::Exploration { .. } => {
            draw_exploration_phase(
                frame,
                builder,
                file_list_inner,
                file_list_focused,
                scan_error,
            );
        }
        FileResultsState::ExtractionPreview { .. } => {
            draw_extraction_preview_phase(frame, builder, file_list_inner, file_list_focused);
        }
        FileResultsState::BacktestResults { .. } => {
            draw_backtest_results_phase(frame, builder, file_list_inner, file_list_focused);
        }
    }

    // Phase-specific status bar
    let status = match &builder.file_results {
        FileResultsState::Exploration { folder_matches, .. } => {
            if folder_matches.is_empty() {
                " Type a pattern to find files...".to_string()
            } else {
                let total_files: usize = folder_matches.iter().map(|f| f.count).sum();
                let selected = folder_matches
                    .get(builder.selected_file)
                    .map(|f| f.path.trim_end_matches('/'))
                    .unwrap_or("");
                if selected.is_empty() {
                    format!(
                        " {} folders  {} files matched",
                        folder_matches.len(),
                        total_files
                    )
                } else {
                    format!(
                        " {} folders  {} files matched  Selected: {}",
                        folder_matches.len(),
                        total_files,
                        selected
                    )
                }
            }
        }
        FileResultsState::ExtractionPreview { preview_files } => {
            if preview_files.is_empty() {
                " No files match current pattern".to_string()
            } else {
                let ok_count = preview_files
                    .iter()
                    .filter(|f| f.warnings.is_empty())
                    .count();
                let selected_count = builder.selected_preview_files.len();
                format!(
                    " {} files  {} OK  {} warnings  Selected: {}  [t] apply tag",
                    preview_files.len(),
                    ok_count,
                    preview_files.len() - ok_count,
                    selected_count
                )
            }
        }
        FileResultsState::BacktestResults { backtest, .. } => {
            let selected_count = builder.selected_preview_files.len();
            format!(
                " Pass: {}  Fail: {}  Skip: {}  Selected: {}",
                backtest.pass_count, backtest.fail_count, backtest.excluded_count, selected_count
            )
        }
    };
    let status_para = Paragraph::new(status).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(status_para, right_chunks[2]);
}

/// Phase 1: Exploration - folder counts + sample filenames
fn draw_exploration_phase(
    frame: &mut Frame,
    builder: &super::extraction::RuleBuilderState,
    area: Rect,
    focused: bool,
    scan_error: Option<&str>,
) {
    let (folder_matches, expanded_folder_indices) = match &builder.file_results {
        super::extraction::FileResultsState::Exploration {
            folder_matches,
            expanded_folder_indices,
            ..
        } => (folder_matches, expanded_folder_indices),
        _ => return,
    };

    let lines: Vec<Line> = if folder_matches.is_empty() {
        // Show error if present, otherwise show "No matching folders"
        if let Some(err) = scan_error {
            vec![
                Line::from(""),
                Line::from(Span::styled(
                    format!("  ⚠ {}", err),
                    Style::default().fg(Color::Yellow),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "  Press [s] to scan this folder",
                    Style::default().fg(Color::Cyan),
                )),
            ]
        } else {
            vec![Line::from(Span::styled(
                "  No matching folders",
                Style::default().fg(Color::DarkGray),
            ))]
        }
    } else {
        let max_display = area.height.saturating_sub(1) as usize;
        let start =
            centered_scroll_offset(builder.selected_file, max_display, folder_matches.len());

        // Calculate column widths for alignment
        // Use width proportions: 60% folder path, 15% count, 25% sample filename
        let available_width = area.width.saturating_sub(6) as usize; // Reserve space for prefix/icon
        let path_width = (available_width * 50 / 100).min(35);
        let count_width = 8;

        folder_matches
            .iter()
            .enumerate()
            .skip(start)
            .take(max_display)
            .map(|(i, folder)| {
                let is_selected = i == builder.selected_file && focused;
                let is_expanded = expanded_folder_indices.contains(&i);

                let expand_icon = if is_expanded { "▼" } else { "▸" };

                // Truncate folder path if too long
                let folder_path = folder.path.trim_end_matches('/');
                let truncated_path = if folder_path.len() > path_width {
                    format!("…{}", &folder_path[folder_path.len() - path_width + 1..])
                } else {
                    folder_path.to_string()
                };

                // Format count with parentheses, right-aligned
                let count_str = format!("({})", folder.count);

                // Build styled spans for proper alignment
                let mut spans = Vec::new();

                // Selection indicator
                if is_selected {
                    spans.push(Span::styled("► ", Style::default().fg(Color::Cyan)));
                } else {
                    spans.push(Span::raw("  "));
                }

                // Expand icon
                spans.push(Span::styled(
                    format!("{} ", expand_icon),
                    if is_selected {
                        Style::default().fg(Color::Cyan)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    },
                ));

                // Folder path (left-aligned, padded)
                spans.push(Span::styled(
                    format!(
                        "{:<width$}",
                        format!("{}/", truncated_path),
                        width = path_width + 1
                    ),
                    if is_selected {
                        Style::default().fg(Color::White).bold()
                    } else {
                        Style::default().fg(Color::Gray)
                    },
                ));

                // Count (right-aligned)
                spans.push(Span::styled(
                    format!("{:>width$}", count_str, width = count_width),
                    if is_selected {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    },
                ));

                // Sample filename
                spans.push(Span::styled(
                    format!("  {}", folder.sample_filename),
                    if is_selected {
                        Style::default().fg(Color::White).italic()
                    } else {
                        Style::default().fg(Color::DarkGray)
                    },
                ));

                Line::from(spans)
            })
            .collect()
    };

    let file_list = Paragraph::new(lines);
    frame.render_widget(file_list, area);
}

/// Phase 2: Extraction Preview - per-file with extracted values
fn draw_extraction_preview_phase(
    frame: &mut Frame,
    builder: &super::extraction::RuleBuilderState,
    area: Rect,
    focused: bool,
) {
    let preview_files = match &builder.file_results {
        super::extraction::FileResultsState::ExtractionPreview { preview_files } => preview_files,
        _ => return,
    };

    let lines: Vec<Line> = if preview_files.is_empty() {
        vec![Line::from(Span::styled(
            "  No files with extractions",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        let max_display = area.height.saturating_sub(1) as usize;
        let start = centered_scroll_offset(builder.selected_file, max_display, preview_files.len());

        preview_files
            .iter()
            .enumerate()
            .skip(start)
            .take(max_display)
            .map(|(i, file)| {
                let is_selected = i == builder.selected_file && focused;
                let has_warnings = !file.warnings.is_empty();

                let marked = builder.selected_preview_files.contains(&file.relative_path);
                let select_mark = if marked { "[x] " } else { "[ ] " };
                let prefix = if is_selected { "► " } else { "  " };
                let status_icon = if has_warnings { "⚠" } else { "✓" };

                let style = if is_selected {
                    Style::default().fg(Color::White).bold()
                } else if has_warnings {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default().fg(Color::Gray)
                };

                // Format extractions as "field=value, field=value"
                let extractions_str: String = file
                    .extractions
                    .iter()
                    .take(3) // Limit to 3 extractions for display
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect::<Vec<_>>()
                    .join(", ");

                let display = format!(
                    "{}{}{} {}  [{}]",
                    select_mark, prefix, status_icon, file.relative_path, extractions_str
                );

                Line::from(Span::styled(display, style))
            })
            .collect()
    };

    let file_list = Paragraph::new(lines);
    frame.render_widget(file_list, area);
}

/// Phase 3: Backtest Results - per-file pass/fail with errors
fn draw_backtest_results_phase(
    frame: &mut Frame,
    builder: &super::extraction::RuleBuilderState,
    area: Rect,
    focused: bool,
) {
    let (matched_files, visible_indices) = match &builder.file_results {
        super::extraction::FileResultsState::BacktestResults {
            matched_files,
            visible_indices,
            ..
        } => (matched_files, visible_indices),
        _ => return,
    };

    let lines: Vec<Line> = if visible_indices.is_empty() {
        vec![Line::from(Span::styled(
            "  No matching files",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        let max_display = area.height.saturating_sub(1) as usize;
        let start =
            centered_scroll_offset(builder.selected_file, max_display, visible_indices.len());

        visible_indices
            .iter()
            .skip(start)
            .take(max_display)
            .enumerate()
            .map(|(i, &idx)| {
                let file = &matched_files[idx];
                let is_selected = i + start == builder.selected_file && focused;
                let marked = builder.selected_preview_files.contains(&file.relative_path);
                let select_mark = if marked { "[x] " } else { "[ ] " };
                let prefix = if is_selected { "► " } else { "  " };
                let indicator = file.test_result.indicator();

                let style = if is_selected {
                    Style::default().fg(Color::White).bold()
                } else {
                    Style::default().fg(Color::Gray)
                };

                Line::from(Span::styled(
                    format!(
                        "{}{}{} {}",
                        select_mark, prefix, indicator, file.relative_path
                    ),
                    style,
                ))
            })
            .collect()
    };

    let file_list = Paragraph::new(lines);
    frame.render_widget(file_list, area);
}

fn draw_rule_builder_manual_tag_confirm(frame: &mut Frame, area: Rect, count: usize, tag: &str) {
    let dialog_area = render_centered_dialog(frame, area, 60, 7);
    let title = " Apply Tag ";
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(Span::styled(
            title,
            Style::default().fg(Color::Yellow).bold(),
        ));
    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let lines = vec![
        Line::from(Span::styled(
            format!("Apply tag '{}' to {} files?", tag, count),
            Style::default().fg(Color::White),
        )),
        Line::from(Span::styled(
            "[Y] Yes   [N] No",
            Style::default().fg(Color::DarkGray),
        )),
    ];
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_sources_inspector(frame: &mut Frame, app: &App, area: Rect) {
    let mut lines = Vec::new();
    if let Some(source) = app.discover.sources.get(app.sources_state.selected_index) {
        lines.push(Line::from(Span::styled(
            format!("Name: {}", source.name),
            Style::default().fg(Color::White),
        )));
        lines.push(Line::from(Span::styled(
            format!(
                "Path: {}",
                truncate_path_start(&source.path.display().to_string(), 32)
            ),
            Style::default().fg(Color::Gray),
        )));
        lines.push(Line::from(Span::styled(
            format!("Files: {}", source.file_count),
            Style::default().fg(Color::Gray),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "Select a source to see details.",
            Style::default().fg(Color::DarkGray),
        )));
    }

    let para = Paragraph::new(lines).wrap(Wrap { trim: true });
    frame.render_widget(para, area);
}

fn draw_rule_builder_confirm_exit(frame: &mut Frame, area: Rect) {
    let dialog_area = render_centered_dialog(frame, area, 58, 7);
    let title = " Discard Changes ";
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(Span::styled(
            title,
            Style::default().fg(Color::Yellow).bold(),
        ));
    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let lines = vec![
        Line::from(Span::styled(
            "You have unsaved changes. Discard them?",
            Style::default().fg(Color::White),
        )),
        Line::from(Span::styled(
            "[Y] Discard   [N] Keep editing",
            Style::default().fg(Color::DarkGray),
        )),
    ];
    frame.render_widget(Paragraph::new(lines), inner);
}

/// Draw the Rule Creation dialog as an overlay (simplified version)
/// Layout: PATTERN → TAG → LIVE PREVIEW
fn draw_rule_creation_dialog(frame: &mut Frame, app: &App, area: Rect) {
    use super::app::RuleDialogFocus;

    // Calculate dialog height
    let dialog_height = 3   // Pattern field
        + 3                 // Tag field
        + 1                 // Separator
        + 8                 // Preview
        + 2; // Footer

    let dialog_area = render_centered_dialog(frame, area, 80, area.height.min(dialog_height));

    let title = if app.discover.editing_rule_id.is_some() {
        " Edit Rule "
    } else {
        " Create Rule "
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Span::styled(title, Style::default().fg(Color::Cyan).bold()));

    let inner_area = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Pattern field with file count
            Constraint::Length(3), // Tag field
            Constraint::Length(1), // Separator
            Constraint::Min(1),    // Live preview
            Constraint::Length(2), // Footer
        ])
        .split(inner_area);

    // === PATTERN field with file count ===
    let pattern_focused = app.discover.rule_dialog_focus == RuleDialogFocus::Pattern;
    let pattern_style = if pattern_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let file_count = app.discover.rule_preview_count;
    let pattern_title = if file_count > 0 {
        format!(" PATTERN [{} files] ", file_count)
    } else {
        " PATTERN ".to_string()
    };

    let pattern_block = Block::default()
        .borders(Borders::ALL)
        .border_style(pattern_style)
        .title(Span::styled(pattern_title, pattern_style));

    let pattern_text = if pattern_focused {
        format!("{}█", app.discover.rule_pattern_input)
    } else {
        app.discover.rule_pattern_input.clone()
    };
    let pattern_para = Paragraph::new(pattern_text)
        .style(Style::default().fg(Color::White))
        .block(pattern_block);
    frame.render_widget(pattern_para, chunks[0]);

    // === TAG field ===
    let tag_focused = app.discover.rule_dialog_focus == RuleDialogFocus::Tag;
    let tag_style = if tag_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let tag_block = Block::default()
        .borders(Borders::ALL)
        .border_style(tag_style)
        .title(Span::styled(" TAG ", tag_style));

    let tag_text = if tag_focused {
        format!("{}█", app.discover.rule_tag_input)
    } else {
        app.discover.rule_tag_input.clone()
    };
    let tag_para = Paragraph::new(tag_text)
        .style(Style::default().fg(Color::White))
        .block(tag_block);
    frame.render_widget(tag_para, chunks[1]);

    // === Separator ===
    let sep_text = format!(
        "─ LIVE PREVIEW {}─",
        "─".repeat(inner_area.width.saturating_sub(16) as usize)
    );
    let sep = Paragraph::new(sep_text).style(Style::default().fg(Color::Yellow));
    frame.render_widget(sep, chunks[2]);

    // === LIVE PREVIEW section ===
    let mut preview_lines: Vec<Line> = Vec::new();

    if app.discover.rule_preview_files.is_empty() {
        if app.discover.rule_pattern_input.is_empty() {
            preview_lines.push(Line::from(Span::styled(
                "  Enter a pattern to see matching files",
                Style::default().fg(Color::DarkGray).italic(),
            )));
        } else {
            preview_lines.push(Line::from(Span::styled(
                "  No files match this pattern",
                Style::default().fg(Color::Red).italic(),
            )));
        }
    } else {
        for (i, preview_file) in app.discover.rule_preview_files.iter().take(5).enumerate() {
            let display_path = truncate_path_start(preview_file, 60);
            preview_lines.push(Line::from(Span::styled(
                format!("  {}", display_path),
                Style::default().fg(Color::Gray),
            )));

            if i == 4 && file_count > 5 {
                preview_lines.push(Line::from(Span::styled(
                    format!("  ... and {} more files", file_count - 5),
                    Style::default().fg(Color::DarkGray).italic(),
                )));
            }
        }
    }

    let preview_para = Paragraph::new(preview_lines);
    frame.render_widget(preview_para, chunks[3]);

    // === Footer with keybindings ===
    let footer = Paragraph::new(Line::from(vec![
        Span::styled("[Enter]", Style::default().fg(Color::Cyan)),
        Span::raw(" Save  "),
        Span::styled("[Tab]", Style::default().fg(Color::Cyan)),
        Span::raw(" Next field  "),
        Span::styled("[Esc]", Style::default().fg(Color::Cyan)),
        Span::raw(" Cancel"),
    ]))
    .alignment(Alignment::Center)
    .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(footer, chunks[4]);
}

// =============================================================================
// Parser Bench Screen
// =============================================================================

/// Draw the Parser Bench screen - parser development workbench
fn draw_parser_bench_screen(frame: &mut Frame, app: &App, area: Rect) {
    let inspector_visible = inspector_visible_for(app, area);
    let shell = shell_layout(area, inspector_visible);

    draw_shell_top_bar(frame, app, shell.top);
    draw_shell_rail(frame, app, shell.rail);

    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(35), // Left panel: parser list
            Constraint::Percentage(65), // Right panel: details/results
        ])
        .split(shell.main);

    draw_parser_list(frame, app, content_chunks[0]);
    draw_parser_details(frame, app, content_chunks[1]);

    if inspector_visible {
        let inner = draw_shell_inspector_block(frame, "Inspector", shell.inspector);
        draw_parser_bench_inspector(frame, app, inner);
    }

    draw_shell_action_bar(
        frame,
        &app.effective_actions(),
        Style::default().fg(Color::DarkGray),
        shell.bottom,
    );
}

/// Draw the parser list panel (left side)
fn draw_parser_list(frame: &mut Frame, app: &App, area: Rect) {
    use ratatui::widgets::{List, ListItem, ListState};

    let parsers_dir = crate::cli::config::parsers_dir();
    let title = format!(" Parsers ({}) ", parsers_dir.display());
    let filtered_indices = app.filtered_parser_indices();

    let block = Block::default().borders(Borders::ALL).title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);

    if app.parser_bench.is_filtering {
        let filter_text = format!("/{}", app.parser_bench.filter);
        let filter_line = Paragraph::new(vec![Line::from(vec![
            Span::styled(filter_text, Style::default().fg(Color::Yellow)),
            Span::styled("█", Style::default().fg(Color::Yellow)),
        ])]);
        frame.render_widget(filter_line, chunks[0]);
    } else if !app.parser_bench.filter.is_empty() {
        let hint = format!("/{} (Esc:clear)", app.parser_bench.filter);
        let hint_line = Paragraph::new(hint).style(Style::default().fg(Color::DarkGray));
        frame.render_widget(hint_line, chunks[0]);
    } else {
        let hint_line =
            Paragraph::new("[/] filter  [r] refresh").style(Style::default().fg(Color::DarkGray));
        frame.render_widget(hint_line, chunks[0]);
    }

    if app.parser_bench.parsers.is_empty() {
        // Show empty state with instructions
        let parsers_path_line = format!("  {}", parsers_dir.display());
        let empty_msg = vec![
            "",
            "  No parsers found.",
            "",
            "  Add parsers to:",
            &parsers_path_line,
            "",
            "  Parsers are .py files with:",
            "    name = 'my_parser'",
            "    version = '1.0.0'",
            "    topics = ['data']",
            "",
            "  [n] Quick test any .py file",
        ];
        let content = empty_msg.join("\n");
        let widget = Paragraph::new(content).style(Style::default().fg(Color::DarkGray));
        frame.render_widget(widget, chunks[1]);
        return;
    }

    if filtered_indices.is_empty() {
        let widget = Paragraph::new("  No parsers match the filter.")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(widget, chunks[1]);
        return;
    }

    // Build list items
    let items: Vec<ListItem> = filtered_indices
        .iter()
        .map(|i| {
            let parser = &app.parser_bench.parsers[*i];
            let symbol = parser.health.symbol();
            let version = parser.version.as_deref().unwrap_or("—");
            let name = &parser.name;
            let is_selected = *i == app.parser_bench.selected_parser;
            let prefix = if is_selected { "▸" } else { " " };

            // Format: ● parser_name     v1.0.0
            let line = format!(
                "{} {} {:<20} {}",
                prefix,
                symbol,
                truncate_end(name, 20),
                version
            );

            let style = if is_selected {
                Style::default().bg(Color::DarkGray).fg(Color::White)
            } else if parser.symlink_broken {
                Style::default().fg(Color::Red)
            } else {
                match &parser.health {
                    ParserHealth::Healthy { .. } => Style::default().fg(Color::Green),
                    ParserHealth::Warning { .. } => Style::default().fg(Color::Yellow),
                    ParserHealth::Paused { .. } => Style::default().fg(Color::Red),
                    ParserHealth::Unknown => Style::default().fg(Color::Gray),
                    ParserHealth::BrokenLink => Style::default().fg(Color::Red),
                }
            };

            ListItem::new(line).style(style)
        })
        .collect();

    let list = List::new(items);

    let selected_pos = filtered_indices
        .iter()
        .position(|idx| *idx == app.parser_bench.selected_parser)
        .unwrap_or(0);
    let mut state = ListState::default();
    state.select(Some(selected_pos));
    frame.render_stateful_widget(list, chunks[1], &mut state);
}

/// Truncate string at the end if too long
fn truncate_end(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        format!("{}...", s.chars().take(max_len - 3).collect::<String>())
    }
}

/// Draw the details/results panel (right side)
fn draw_parser_details(frame: &mut Frame, app: &App, area: Rect) {
    if app.parser_bench.parsers.is_empty() {
        // No parser selected, show instructions
        let content = Paragraph::new(
            "\n\n  Select a parser to see details\n\n  or press [n] to quick test any .py file",
        )
        .style(Style::default().fg(Color::DarkGray))
        .block(Block::default().borders(Borders::ALL).title(" Details "));
        frame.render_widget(content, area);
        return;
    }

    if app.filtered_parser_indices().is_empty() {
        let content = Paragraph::new("\n\n  No parsers match the current filter.")
            .style(Style::default().fg(Color::DarkGray))
            .block(Block::default().borders(Borders::ALL).title(" Details "));
        frame.render_widget(content, area);
        return;
    }

    let parser = &app.parser_bench.parsers[app.parser_bench.selected_parser];

    // Show test result if available, otherwise show parser details
    if let Some(ref result) = app.parser_bench.test_result {
        draw_test_result(frame, result, area);
    } else {
        draw_parser_info(frame, parser, &app.parser_bench.bound_files, area);
    }
}

fn draw_parser_bench_inspector(frame: &mut Frame, app: &App, area: Rect) {
    let mut lines = Vec::new();
    if app.parser_bench.parsers.is_empty() || app.filtered_parser_indices().is_empty() {
        lines.push(Line::from(Span::styled(
            "Select a parser to see details.",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        let parser = &app.parser_bench.parsers[app.parser_bench.selected_parser];
        let version = parser.version.as_deref().unwrap_or("—");
        lines.push(Line::from(Span::styled(
            format!("Name: {}", parser.name),
            Style::default().fg(Color::White),
        )));
        lines.push(Line::from(Span::styled(
            format!("Version: {}", version),
            Style::default().fg(Color::Gray),
        )));
        lines.push(Line::from(Span::styled(
            format!("Health: {}", parser.health.symbol()),
            Style::default().fg(Color::Gray),
        )));
        lines.push(Line::from(Span::styled(
            format!(
                "Path: {}",
                truncate_path_start(&parser.path.display().to_string(), 32)
            ),
            Style::default().fg(Color::Gray),
        )));
        lines.push(Line::from(Span::styled(
            format!("Modified: {}", parser.modified.format("%Y-%m-%d %H:%M")),
            Style::default().fg(Color::Gray),
        )));
    }

    let para = Paragraph::new(lines).wrap(Wrap { trim: true });
    frame.render_widget(para, area);
}

/// Draw parser info when no test result is showing
fn draw_parser_info(
    frame: &mut Frame,
    parser: &super::app::ParserInfo,
    bound_files: &[super::app::BoundFileInfo],
    area: Rect,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8), // Parser info
            Constraint::Min(0),    // Bound files
        ])
        .split(area);

    // Parser info section
    let version = parser.version.as_deref().unwrap_or("—");
    let topics = if parser.topics.is_empty() {
        "none".to_string()
    } else {
        parser.topics.join(", ")
    };
    let health_str = match &parser.health {
        ParserHealth::Healthy {
            success_rate,
            total_runs,
        } => {
            format!(
                "Healthy ({:.1}%, {} runs)",
                success_rate * 100.0,
                total_runs
            )
        }
        ParserHealth::Warning {
            consecutive_failures,
        } => {
            format!("Warning ({} failures)", consecutive_failures)
        }
        ParserHealth::Paused { reason } => format!("Paused: {}", reason),
        ParserHealth::Unknown => "Unknown".to_string(),
        ParserHealth::BrokenLink => "Broken symlink".to_string(),
    };

    let info_lines = vec![
        format!("  Name:     {}", parser.name),
        format!("  Version:  {}", version),
        format!("  Topics:   {}", topics),
        format!(
            "  Path:     {}",
            truncate_path_start(&parser.path.display().to_string(), 50)
        ),
        format!("  Health:   {} {}", parser.health.symbol(), health_str),
        format!("  Modified: {}", parser.modified.format("%Y-%m-%d %H:%M")),
    ];

    let info_widget = Paragraph::new(info_lines.join("\n")).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" {} ", parser.name)),
    );
    frame.render_widget(info_widget, chunks[0]);

    // Bound files section
    let files_title = format!(" Bound Files ({}) ", bound_files.len());
    if bound_files.is_empty() {
        let empty_msg =
            "\n  No files match this parser's topics.\n\n  Use Discover mode to tag files.";
        let widget = Paragraph::new(empty_msg)
            .style(Style::default().fg(Color::DarkGray))
            .block(Block::default().borders(Borders::ALL).title(files_title));
        frame.render_widget(widget, chunks[1]);
    } else {
        use ratatui::widgets::{List, ListItem};

        let items: Vec<ListItem> = bound_files
            .iter()
            .take(20) // Limit display
            .map(|f| {
                let status_sym = f.status.symbol();
                let path = truncate_path_start(&f.path.display().to_string(), 40);
                let size = format_size(f.size);
                ListItem::new(format!(" {} {} {}", status_sym, path, size))
            })
            .collect();

        let list =
            List::new(items).block(Block::default().borders(Borders::ALL).title(files_title));
        frame.render_widget(list, chunks[1]);
    }
}

/// Draw test result panel
fn draw_test_result(frame: &mut Frame, result: &super::app::ParserTestResult, area: Rect) {
    let title = if result.success {
        format!(
            " Test Result - PASSED ({} rows, {}ms) ",
            result.rows_processed, result.execution_time_ms
        )
    } else {
        format!(" Test Result - FAILED ({}ms) ", result.execution_time_ms)
    };

    let title_style = if result.success {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::Red)
    };

    if result.success {
        // Show schema and preview
        let mut lines = vec![];
        lines.push(String::new());

        // Schema
        if let Some(ref schema) = result.schema {
            lines.push("  SCHEMA".to_string());
            lines.push("  ------".to_string());
            for col in schema {
                lines.push(format!("    {}: {}", col.name, col.dtype));
            }
            lines.push(String::new());
        }

        // Preview rows
        if !result.preview_rows.is_empty() {
            lines.push("  PREVIEW".to_string());
            lines.push("  -------".to_string());
            // Headers
            if !result.headers.is_empty() {
                lines.push(format!("    {}", result.headers.join(" | ")));
                lines.push(format!("    {}", "-".repeat(50)));
            }
            for row in result.preview_rows.iter().take(5) {
                lines.push(format!("    {}", row.join(" | ")));
            }
            if result.truncated {
                lines.push("    ... (truncated)".to_string());
            }
        }

        let widget = Paragraph::new(lines.join("\n")).block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .title_style(title_style),
        );
        frame.render_widget(widget, area);
    } else {
        // Show errors
        let mut lines = vec![];
        lines.push(String::new());

        if let Some(ref error_type) = result.error_type {
            lines.push(format!("  Error Type: {}", error_type));
            lines.push(String::new());
        }

        lines.push("  ERRORS".to_string());
        lines.push("  ------".to_string());
        for err in &result.errors {
            lines.push(format!("    {}", err));
        }

        if !result.suggestions.is_empty() {
            lines.push(String::new());
            lines.push("  SUGGESTIONS".to_string());
            lines.push("  -----------".to_string());
            for sug in &result.suggestions {
                lines.push(format!("    - {}", sug));
            }
        }

        let widget = Paragraph::new(lines.join("\n")).block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .title_style(title_style),
        );
        frame.render_widget(widget, area);
    }
}

/// Draw the Jobs mode screen - per jobs_redesign.md spec
fn draw_jobs_screen(frame: &mut Frame, app: &App, area: Rect) {
    // Check view state for monitoring panel
    if app.jobs_state.view_state == JobsViewState::MonitoringPanel {
        draw_monitoring_panel(frame, app, area);
        return;
    }

    let inspector_visible = inspector_visible_for(app, area);
    let shell = shell_layout(area, inspector_visible);

    draw_shell_top_bar(frame, app, shell.top);
    draw_shell_rail(frame, app, shell.rail);

    // Calculate layout based on whether pipeline is shown
    let (pipeline_height, _content_height) = if app.jobs_state.show_pipeline {
        (8, shell.main.height.saturating_sub(14)) // Pipeline + status bar + footer
    } else {
        (0, shell.main.height.saturating_sub(6)) // Just status bar + footer
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),               // Status bar
            Constraint::Length(pipeline_height), // Pipeline (if shown)
            Constraint::Min(0),                  // Job list
        ])
        .split(shell.main);

    // Status bar with aggregate stats (per spec Section 4.1)
    let ready = app
        .jobs_state
        .jobs
        .iter()
        .filter(|j| matches!(j.status, JobStatus::Completed | JobStatus::PartialSuccess))
        .count() as u32;
    let active = app
        .jobs_state
        .jobs
        .iter()
        .filter(|j| matches!(j.status, JobStatus::Pending | JobStatus::Running))
        .count() as u32;
    let failed = app
        .jobs_state
        .jobs
        .iter()
        .filter(|j| matches!(j.status, JobStatus::Failed | JobStatus::Cancelled))
        .count() as u32;
    let queue = app
        .jobs_state
        .jobs
        .iter()
        .filter(|j| j.status == JobStatus::Pending)
        .count() as u32;
    let output_bytes: u64 = app
        .jobs_state
        .jobs
        .iter()
        .filter_map(|j| j.output_size_bytes)
        .sum();

    let status_text = format!(
        "  READY: {}  •  ACTIVE: {}  •  FAILED: {}  •  Queue: {}  •  Output: {}",
        ready,
        active,
        failed,
        queue,
        format_size(output_bytes)
    );

    let status_bar = Paragraph::new(status_text)
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .title(" JOBS ")
                .title_style(Style::default().fg(Color::Cyan).bold()),
        );
    frame.render_widget(status_bar, chunks[0]);

    // Pipeline summary (if toggled)
    let job_list_area = if app.jobs_state.show_pipeline && pipeline_height > 0 {
        draw_pipeline_summary(frame, app, chunks[1]);
        chunks[2]
    } else {
        // Combine pipeline slot with job list
        Rect::new(
            chunks[1].x,
            chunks[1].y,
            chunks[1].width,
            chunks[1].height + chunks[2].height,
        )
    };

    draw_jobs_list(frame, app, job_list_area);

    if inspector_visible {
        // In ViolationDetail view state, show violations panel instead of job detail
        if app.jobs_state.view_state == JobsViewState::ViolationDetail {
            if let Some(job) = app.jobs_state.selected_job() {
                draw_violations_panel(frame, job, shell.inspector, job.selected_violation_index);
            } else {
                draw_job_detail(frame, app, shell.inspector);
            }
        } else {
            draw_job_detail(frame, app, shell.inspector);
        }
    }

    draw_shell_action_bar(
        frame,
        &app.effective_actions(),
        Style::default().fg(Color::DarkGray),
        shell.bottom,
    );
}

/// Draw pipeline summary (per spec Section 3.2)
fn draw_pipeline_summary(frame: &mut Frame, app: &App, area: Rect) {
    let pipeline = &app.jobs_state.pipeline;

    let source_in_progress = if pipeline.source.in_progress > 0 {
        format!("@{}", pipeline.source.in_progress)
    } else {
        String::new()
    };

    let parsed_in_progress = if pipeline.parsed.in_progress > 0 {
        format!("@{}", pipeline.parsed.in_progress)
    } else {
        String::new()
    };

    let output_in_progress = if pipeline.output.in_progress > 0 {
        format!("{} run", pipeline.output.in_progress)
    } else {
        String::new()
    };

    let content = format!(
        "   SOURCE              PARSED               OUTPUT\n\
         \n\
           {} files  ────▶  {} files  ────▶   {} ready\n\
           {}              {}              {}\n\
         \n\
           {}",
        pipeline.source.count,
        pipeline.parsed.count,
        pipeline.output.count,
        source_in_progress,
        parsed_in_progress,
        output_in_progress,
        pipeline.active_parser.as_deref().unwrap_or("")
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" PIPELINE ")
        .border_style(Style::default().fg(Color::DarkGray));

    let paragraph = Paragraph::new(content).block(block);
    frame.render_widget(paragraph, area);
}

/// Draw the jobs list (per spec Section 4)
fn draw_jobs_list(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::White));
    let inner = block.inner(area);
    let width = inner.width as usize;

    let mut lines: Vec<Line> = Vec::new();
    let mut selected_line: Option<usize> = None;
    let actionable_jobs = app.jobs_state.actionable_jobs();
    let ready_jobs = app.jobs_state.ready_jobs();

    if actionable_jobs.is_empty() && ready_jobs.is_empty() {
        lines.push(Line::from(Span::styled(
            "No jobs found.",
            Style::default().fg(Color::DarkGray),
        )));
        if app.jobs_state.status_filter.is_some() || app.jobs_state.type_filter.is_some() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Press [Del] to clear filters.",
                Style::default().fg(Color::DarkGray),
            )));
        }
    } else {
        let actionable_style = if app.jobs_state.section_focus == JobsListSection::Actionable {
            Style::default().fg(Color::Cyan).bold()
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let ready_style = if app.jobs_state.section_focus == JobsListSection::Ready {
            Style::default().fg(Color::Green).bold()
        } else {
            Style::default().fg(Color::DarkGray)
        };

        lines.push(Line::from(Span::styled(
            " ACTIONABLE (running, queued, or failed)",
            actionable_style,
        )));
        if actionable_jobs.is_empty() {
            lines.push(Line::from(Span::styled(
                "  No actionable jobs.",
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            for (i, job) in actionable_jobs.iter().enumerate() {
                let is_selected = app.jobs_state.section_focus == JobsListSection::Actionable
                    && i == app.jobs_state.selected_index;
                if is_selected {
                    selected_line = Some(lines.len());
                }
                render_actionable_job_line(&mut lines, job, is_selected, width);
            }
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(" READY OUTPUTS", ready_style)));

        if ready_jobs.is_empty() {
            lines.push(Line::from(Span::styled(
                "  No ready outputs.",
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            for (i, job) in ready_jobs.iter().enumerate() {
                let is_selected = app.jobs_state.section_focus == JobsListSection::Ready
                    && i == app.jobs_state.selected_index;
                if is_selected {
                    selected_line = Some(lines.len());
                }
                render_ready_job_line(&mut lines, job, is_selected, width);
            }
        }
    }

    let visible_lines = inner.height as usize;
    let total_lines = lines.len().max(1);
    let scroll = selected_line
        .map(|line| centered_scroll_offset(line, visible_lines, total_lines))
        .unwrap_or(0);

    let paragraph = Paragraph::new(lines)
        .block(block)
        .scroll((scroll.min(u16::MAX as usize) as u16, 0));
    frame.render_widget(paragraph, area);
}

fn draw_jobs_filter_dialog(frame: &mut Frame, app: &App, area: Rect) {
    let dialog_area = render_centered_dialog(frame, area, 52, 9);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Jobs Filter ");
    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let status_label = app
        .jobs_state
        .status_filter
        .map(|status| status.as_str())
        .unwrap_or("Any");
    let status_style = if app.jobs_state.status_filter.is_some() {
        Style::default().fg(Color::Cyan).bold()
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let type_label = app
        .jobs_state
        .type_filter
        .map(|job_type| job_type.as_str())
        .unwrap_or("Any");
    let type_style = if app.jobs_state.type_filter.is_some() {
        Style::default().fg(Color::Cyan).bold()
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        " Filters",
        Style::default().fg(Color::Cyan).bold(),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(" Status: ", Style::default().fg(Color::DarkGray)),
        Span::styled(status_label, status_style),
    ]));
    lines.push(Line::from(vec![
        Span::styled(" Type:   ", Style::default().fg(Color::DarkGray)),
        Span::styled(type_label, type_style),
    ]));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " [s] status  [t] type  [x] clear",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::styled(
        " [Enter/Esc] close",
        Style::default().fg(Color::DarkGray),
    )));

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
    frame.render_widget(paragraph, inner);
}

fn draw_jobs_log_viewer(frame: &mut Frame, app: &App, area: Rect) {
    let max_width = area.width.saturating_sub(4).min(90).max(50);
    let max_height = area.height.saturating_sub(4).min(22).max(10);
    let dialog_area = render_centered_dialog(frame, area, max_width, max_height);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Logs ");
    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let max_line_width = inner.width.saturating_sub(1) as usize;
    let mut lines: Vec<Line> = Vec::new();

    if let Some(job) = app.jobs_state.selected_job() {
        let job_name = truncate_end(&job.name, max_line_width.saturating_sub(12));
        lines.push(Line::from(vec![
            Span::styled(" Job: ", Style::default().fg(Color::DarkGray)),
            Span::styled(job_name, Style::default().fg(Color::White).bold()),
            Span::styled(
                format!(" ({})", job.job_type.as_str()),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled(" Status: ", Style::default().fg(Color::DarkGray)),
            Span::styled(job.status.as_str(), Style::default().fg(Color::White)),
        ]));
        let output_path = job.output_path.as_deref().unwrap_or("-");
        let output_display =
            truncate_path_start(output_path, max_line_width.saturating_sub(10));
        lines.push(Line::from(vec![
            Span::styled(" Output: ", Style::default().fg(Color::DarkGray)),
            Span::styled(output_display, Style::default().fg(Color::White)),
        ]));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            " Failures",
            Style::default().fg(Color::Cyan).bold(),
        )));
        if job.failures.is_empty() {
            lines.push(Line::from(Span::styled(
                "  No failure details recorded.",
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            for failure in &job.failures {
                let location = failure
                    .line
                    .map(|line| format!(":{}", line))
                    .unwrap_or_default();
                let path_display =
                    truncate_path_start(&failure.file_path, max_line_width.saturating_sub(6));
                let line_text = format!("  {}{} - {}", path_display, location, failure.error);
                lines.push(Line::from(Span::styled(
                    truncate_end(&line_text, max_line_width),
                    Style::default().fg(Color::White),
                )));
            }
        }
    } else {
        lines.push(Line::from(Span::styled(
            " No job selected.",
            Style::default().fg(Color::DarkGray),
        )));
    }

    let visible_lines = inner.height as usize;
    let max_scroll = lines.len().saturating_sub(visible_lines);
    let scroll = app.jobs_state.log_viewer_scroll.min(max_scroll);

    let paragraph = Paragraph::new(lines)
        .scroll((scroll.min(u16::MAX as usize) as u16, 0));
    frame.render_widget(paragraph, inner);
}

fn render_ready_job_line(lines: &mut Vec<Line>, job: &JobInfo, is_selected: bool, width: usize) {
    let prefix = if is_selected { "▸ " } else { "  " };
    let name_style = if is_selected {
        Style::default().fg(Color::Cyan).bold()
    } else {
        Style::default().fg(Color::White)
    };

    let version_str = job
        .version
        .as_deref()
        .map(|v| format!(" v{}", v))
        .unwrap_or_default();
    let time_ref = job.completed_at.unwrap_or(job.started_at);
    let time_str = format_relative_time(time_ref);

    let name = truncate_end(&job.name, width.saturating_sub(28));
    let (label, label_style) = match job.status {
        JobStatus::PartialSuccess => ("[WARN] ", Style::default().fg(Color::Yellow)),
        _ => ("[READY] ", Style::default().fg(Color::Green)),
    };
    lines.push(Line::from(vec![
        Span::styled(prefix, Style::default()),
        Span::styled(label, label_style),
        Span::styled(
            format!("{} {}{}", job.job_type.as_str(), name, version_str),
            name_style,
        ),
        Span::styled(
            format!("  {}", time_str),
            Style::default().fg(Color::DarkGray),
        ),
    ]));

    let path = job.output_path.as_deref().unwrap_or("-");
    let size = job
        .output_size_bytes
        .map(format_size)
        .unwrap_or_else(|| "-".to_string());
    let quarantine = job
        .quarantine_rows
        .filter(|rows| *rows > 0)
        .map(|rows| format!(" • quarantine {}", rows))
        .unwrap_or_default();
    let path_display = truncate_path_start(path, width.saturating_sub(18));
    lines.push(Line::from(Span::styled(
        format!("    Output: {} ({}){}", path_display, size, quarantine),
        Style::default().fg(Color::DarkGray),
    )));

    lines.push(Line::from(""));
}

fn render_actionable_job_line(
    lines: &mut Vec<Line>,
    job: &JobInfo,
    is_selected: bool,
    width: usize,
) {
    let prefix = if is_selected { "▸ " } else { "  " };
    let name_style = if is_selected {
        Style::default().fg(Color::Cyan).bold()
    } else {
        Style::default().fg(Color::White)
    };
    let version_str = job
        .version
        .as_deref()
        .map(|v| format!(" v{}", v))
        .unwrap_or_default();
    let name = truncate_end(&job.name, width.saturating_sub(28));

    let (label, label_style) = match job.status {
        JobStatus::Running => ("[RUN] ", Style::default().fg(Color::Yellow)),
        JobStatus::Pending => ("[QUEUE] ", Style::default().fg(Color::DarkGray)),
        JobStatus::Failed => ("[FAIL] ", Style::default().fg(Color::Red)),
        JobStatus::Cancelled => ("[CANCEL] ", Style::default().fg(Color::DarkGray)),
        JobStatus::Completed => ("[READY] ", Style::default().fg(Color::Green)),
        JobStatus::PartialSuccess => ("[WARN] ", Style::default().fg(Color::Yellow)),
    };

    let trailing = if job.status == JobStatus::Running {
        if job.job_type == JobType::Scan {
            "scanning".to_string()
        } else {
            let pct = if job.items_total > 0 {
                (job.items_processed as f64 / job.items_total as f64 * 100.0) as u8
            } else {
                0
            };
            format!("{} {}%", render_progress_bar(pct, 10), pct)
        }
    } else if job.status == JobStatus::Pending {
        "queued".to_string()
    } else {
        format_relative_time(job.started_at)
    };

    lines.push(Line::from(vec![
        Span::styled(prefix, Style::default()),
        Span::styled(label, label_style),
        Span::styled(
            format!("{} {}{}", job.job_type.as_str(), name, version_str),
            name_style,
        ),
        Span::styled(
            format!("  {}", trailing),
            Style::default().fg(Color::DarkGray),
        ),
    ]));

    let detail = match job.status {
        JobStatus::Running => {
            if job.job_type == JobType::Scan {
                let processed = format_number(job.items_processed as u64);
                let discovered = format_number(job.items_total as u64);
                format!("    Indexed {} • Found {}", processed, discovered)
            } else {
                let eta = calculate_eta(job.items_processed, job.items_total, job.started_at);
                format!(
                    "    {}/{} files • ETA {}",
                    job.items_processed, job.items_total, eta
                )
            }
        }
        JobStatus::Failed => {
            let err = job
                .failures
                .first()
                .map(|f| f.error.as_str())
                .unwrap_or("Unknown error");
            format!("    Error: {}", truncate_end(err, width.saturating_sub(12)))
        }
        JobStatus::Pending => "    Queued".to_string(),
        JobStatus::Cancelled => "    Cancelled".to_string(),
        JobStatus::Completed => "    Ready".to_string(),
        JobStatus::PartialSuccess => "    Ready (warn)".to_string(),
    };
    lines.push(Line::from(Span::styled(
        detail,
        Style::default().fg(Color::DarkGray),
    )));

    lines.push(Line::from(""));
}

/// Draw job detail panel (per spec Section 7)
fn draw_job_detail(frame: &mut Frame, app: &App, area: Rect) {
    let content = if let Some(job) = app.jobs_state.selected_job() {
        let mut detail = String::new();
        detail.push_str(&format!("{} {}\n", job.job_type.as_str(), job.name));
        detail.push_str(&format!("Status: {}\n", job.status.as_str()));

        let quarantine_rows = job.quarantine_rows.filter(|rows| *rows > 0);
        if let Some(ref path) = job.output_path {
            detail.push_str("\nOUTPUT\n");
            detail.push_str(&format!("{}\n", path));
            if let Some(bytes) = job.output_size_bytes {
                detail.push_str(&format!(
                    "{} files • {}\n",
                    job.items_processed,
                    format_size(bytes)
                ));
            }
            if let Some(rows) = quarantine_rows {
                detail.push_str(&format!(
                    "Quarantine: {} rows\n",
                    format_number(rows as u64)
                ));
                detail.push_str("Press [Q] to view\n");
            }
        } else if let Some(rows) = quarantine_rows {
            detail.push_str("\nQUARANTINE\n");
            detail.push_str(&format!("{} rows\n", format_number(rows as u64)));
            detail.push_str("Press [Q] to view\n");
        }

        if job.status == JobStatus::Running {
            if job.job_type == JobType::Scan {
                detail.push_str(&format!(
                    "\nProgress: Indexed {} • Found {}\n",
                    format_number(job.items_processed as u64),
                    format_number(job.items_total as u64)
                ));
            } else {
                let eta = calculate_eta(job.items_processed, job.items_total, job.started_at);
                detail.push_str(&format!(
                    "\nProgress: {}/{} files • ETA {}\n",
                    job.items_processed, job.items_total, eta
                ));
            }
        } else if job.status == JobStatus::Pending {
            detail.push_str("\nQueued\n");
        }

        detail.push_str(&format!(
            "\nStarted:  {}\nDuration: {}\n",
            job.started_at.format("%H:%M:%S"),
            format_duration(job.started_at, job.completed_at)
        ));

        if job.pipeline_run_id.is_some()
            || job.logical_date.is_some()
            || job.selection_snapshot_hash.is_some()
        {
            detail.push_str("\nPIPELINE\n");
            if let Some(ref run_id) = job.pipeline_run_id {
                detail.push_str(&format!("Run:      {}\n", run_id));
            }
            if let Some(ref logical_date) = job.logical_date {
                detail.push_str(&format!("Logical:  {}\n", logical_date));
            }
            if let Some(ref snapshot) = job.selection_snapshot_hash {
                detail.push_str(&format!("Snapshot: {}\n", snapshot));
            }
        }

        if !job.failures.is_empty() {
            detail.push_str(&format!("\nFAILURES ({})\n", job.failures.len()));
            for failure in job.failures.iter().take(5) {
                let hint = if failure.file_path.is_empty() {
                    "unknown file"
                } else {
                    failure.file_path.as_str()
                };
                detail.push_str(&format!(
                    "{}\n  {}\n",
                    truncate_path_start(hint, 42),
                    failure.error
                ));
            }
            if job.failures.len() > 5 {
                detail.push_str(&format!("... ({} more)\n", job.failures.len() - 5));
            }
        }

        // Show violations summary for backtest jobs
        if job.job_type == JobType::Backtest {
            draw_violations_summary(job, &mut detail);
        }

        detail
    } else {
        "Select a job to view details.".to_string()
    };

    let title = if app.jobs_state.pinned_job_id.is_some() {
        " JOB DETAILS (PINNED) "
    } else {
        " JOB DETAILS "
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::Cyan));

    let paragraph = Paragraph::new(content)
        .block(block)
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

/// Draw violations panel for backtest jobs
/// Shows top violations with icons, counts, percentages, and suggested fixes
fn draw_violations_panel(frame: &mut Frame, job: &JobInfo, area: Rect, selected_idx: usize) {
    let mut lines: Vec<Line> = Vec::new();

    if job.violations.is_empty() {
        if job.top_violations_loaded {
            lines.push(Line::from(Span::styled(
                "  No violations found.",
                Style::default().fg(Color::Green),
            )));
        } else {
            lines.push(Line::from(Span::styled(
                "  Loading violations...",
                Style::default().fg(Color::DarkGray),
            )));
        }
    } else {
        for (idx, violation) in job.violations.iter().enumerate() {
            let is_selected = idx == selected_idx;
            let prefix = if is_selected { ">" } else { " " };
            let highlight = if is_selected {
                Style::default().fg(Color::White).bold()
            } else {
                Style::default()
            };

            // First line: icon, type, column, count, percentage
            let icon = violation_icon(violation.violation_type);
            let icon_color = violation_color(violation.violation_type);

            let mut spans = vec![
                Span::styled(format!("{} ", prefix), highlight),
                Span::styled(format!("{} ", icon), Style::default().fg(icon_color)),
                Span::styled(
                    format!("{}: ", violation.violation_type.as_str()),
                    Style::default().fg(icon_color).bold(),
                ),
                Span::styled(
                    format!("{} ", violation.column),
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(
                    format!("({} rows, {:.1}%)", violation.count, violation.pct_of_rows),
                    Style::default().fg(Color::DarkGray),
                ),
            ];

            // Add confidence badge if available
            if let Some(ref confidence) = violation.confidence {
                spans.push(Span::raw(" "));
                spans.push(confidence_badge(confidence));
            }

            lines.push(Line::from(spans));

            // Second line: expected vs actual (if available)
            if violation.expected.is_some() || violation.actual.is_some() {
                let expected = violation.expected.as_deref().unwrap_or("?");
                let actual = violation.actual.as_deref().unwrap_or("?");
                lines.push(Line::from(Span::styled(
                    format!("    Expected: {}, Got: {}", expected, actual),
                    Style::default().fg(Color::DarkGray),
                )));
            }

            // Third line: samples (if available)
            if !violation.samples.is_empty() {
                let samples_str = violation
                    .samples
                    .iter()
                    .take(3)
                    .map(|s| format!("\"{}\"", truncate_end(s, 20)))
                    .collect::<Vec<_>>()
                    .join(", ");
                lines.push(Line::from(Span::styled(
                    format!("    Samples: {}", samples_str),
                    Style::default().fg(Color::DarkGray),
                )));
            }

            // Fourth line: suggested fix (if available)
            if let Some(ref fix) = violation.suggested_fix {
                lines.push(Line::from(vec![
                    Span::styled("    Suggested: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        format!("-> {}", fix.display()),
                        Style::default().fg(Color::Green),
                    ),
                ]));
            }

            // Empty line between violations
            lines.push(Line::from(""));
        }
    }

    // Footer hint
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "[v] View all violations  [a] Apply fix  [Esc] Back",
        Style::default().fg(Color::DarkGray),
    )));

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Top Violations ")
        .border_style(Style::default().fg(Color::Yellow));

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

/// Draw a compact violations summary section for the job detail panel
fn draw_violations_summary(job: &JobInfo, detail: &mut String) {
    if job.violations.is_empty() {
        if job.top_violations_loaded {
            return; // No violations, nothing to show
        } else {
            detail.push_str("\nVIOLATIONS\n");
            detail.push_str("  Loading...\n");
            return;
        }
    }

    detail.push_str(&format!("\nVIOLATIONS ({})\n", job.violations.len()));

    for violation in job.violations.iter().take(3) {
        let icon = violation_icon(violation.violation_type);
        detail.push_str(&format!(
            "{} {}: {} ({} rows, {:.1}%)\n",
            icon,
            violation.violation_type.as_str(),
            violation.column,
            violation.count,
            violation.pct_of_rows
        ));

        if let Some(ref fix) = violation.suggested_fix {
            detail.push_str(&format!("  -> {}\n", fix.display()));
        }
    }

    if job.violations.len() > 3 {
        detail.push_str(&format!(
            "... ({} more, press [v] to view)\n",
            job.violations.len() - 3
        ));
    }
}

/// Draw monitoring panel (per spec Section 5)
fn draw_monitoring_panel(frame: &mut Frame, app: &App, area: Rect) {
    let inspector_visible = inspector_visible_for(app, area);
    let shell = shell_layout(area, inspector_visible);

    draw_shell_top_bar(frame, app, shell.top);
    draw_shell_rail(frame, app, shell.rail);

    let content_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(shell.main);

    let top_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(content_chunks[0]);

    // Queue panel
    let queue = &app.jobs_state.monitoring.queue;
    let queue_content =
        format!(
        "  Pending:  {:>6}\n  Running:  {:>6}\n  Done:     {:>6}\n  Failed:   {:>6}\n\n  Depth: {}",
        queue.pending, queue.running, queue.completed, queue.failed,
        render_sparkline(&queue.depth_history)
    );
    let queue_block = Block::default()
        .borders(Borders::ALL)
        .title(" QUEUE ")
        .border_style(Style::default().fg(Color::DarkGray));
    frame.render_widget(Paragraph::new(queue_content).block(queue_block), top_row[0]);

    // Throughput panel
    let throughput_sparkline =
        render_throughput_sparkline(&app.jobs_state.monitoring.throughput_history);
    let current_rps = app
        .jobs_state
        .monitoring
        .throughput_history
        .back()
        .map(|s| s.rows_per_second)
        .unwrap_or(0.0);
    let avg_rps: f64 = if app.jobs_state.monitoring.throughput_history.is_empty() {
        0.0
    } else {
        app.jobs_state
            .monitoring
            .throughput_history
            .iter()
            .map(|s| s.rows_per_second)
            .sum::<f64>()
            / app.jobs_state.monitoring.throughput_history.len() as f64
    };

    let throughput_content = format!(
        "\n{}\n\n  {:.1}k rows/s avg      {:.1}k now",
        throughput_sparkline,
        avg_rps / 1000.0,
        current_rps / 1000.0
    );
    let throughput_block = Block::default()
        .borders(Borders::ALL)
        .title(" THROUGHPUT (5m) ")
        .border_style(Style::default().fg(Color::DarkGray));
    frame.render_widget(
        Paragraph::new(throughput_content).block(throughput_block),
        top_row[1],
    );

    // Sinks panel
    let mut sinks_content = String::new();
    for sink in &app.jobs_state.monitoring.sinks {
        sinks_content.push_str(&format!(
            "  {}   {} total   {} errors\n",
            sink.uri,
            format_size(sink.total_bytes),
            sink.error_count
        ));
        for output in &sink.outputs {
            sinks_content.push_str(&format!(
                "    └─ {}   {}   {} rows\n",
                output.name,
                format_size(output.bytes),
                output.rows
            ));
        }
        sinks_content.push('\n');
    }
    if sinks_content.is_empty() {
        sinks_content = "  No sink data yet.".to_string();
    }

    let sinks_block = Block::default()
        .borders(Borders::ALL)
        .title(" SINKS ")
        .border_style(Style::default().fg(Color::DarkGray));
    frame.render_widget(
        Paragraph::new(sinks_content).block(sinks_block),
        content_chunks[1],
    );

    if inspector_visible {
        let inner = draw_shell_inspector_block(frame, "Inspector", shell.inspector);
        let text = if app.jobs_state.monitoring.paused {
            "Updates paused."
        } else {
            "Live throughput metrics."
        };
        let para = Paragraph::new(text).style(Style::default().fg(Color::DarkGray));
        frame.render_widget(para, inner);
    }

    draw_shell_action_bar(
        frame,
        &app.effective_actions(),
        Style::default().fg(Color::DarkGray),
        shell.bottom,
    );
}

/// Render a progress bar
fn render_progress_bar(percent: u8, width: usize) -> String {
    let filled = (percent as usize * width) / 100;
    let empty = width.saturating_sub(filled);
    format!("{}{}", "█".repeat(filled), "░".repeat(empty))
}

/// Render sparkline from queue depth history
fn render_sparkline(history: &std::collections::VecDeque<u32>) -> String {
    const BLOCKS: [char; 8] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇'];

    if history.is_empty() {
        return " ".repeat(20);
    }

    let max = *history.iter().max().unwrap_or(&1) as f64;
    let min = *history.iter().min().unwrap_or(&0) as f64;
    let range = if max == min { 1.0 } else { max - min };

    history
        .iter()
        .take(20)
        .map(|&v| {
            let idx = ((v as f64 - min) / range * 7.0).round() as usize;
            BLOCKS[idx.min(7)]
        })
        .collect()
}

/// Render throughput sparkline
fn render_throughput_sparkline(history: &std::collections::VecDeque<ThroughputSample>) -> String {
    const BLOCKS: [char; 8] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇'];

    if history.is_empty() {
        return "  ".to_string() + &" ".repeat(40);
    }

    let values: Vec<f64> = history.iter().map(|s| s.rows_per_second).collect();
    let max = values.iter().cloned().fold(f64::MIN, f64::max);
    let min = values.iter().cloned().fold(f64::MAX, f64::min);
    let range = if max == min { 1.0 } else { max - min };

    let sparkline: String = values
        .iter()
        .take(40)
        .map(|&v| {
            let idx = ((v - min) / range * 7.0).round() as usize;
            BLOCKS[idx.min(7)]
        })
        .collect();

    format!("  {}", sparkline)
}

/// Format relative time (e.g., "2m ago", "1h ago")
fn format_relative_time(dt: DateTime<Local>) -> String {
    let now = Local::now();
    let diff = now.signed_duration_since(dt);

    if diff.num_seconds() < 60 {
        format!("{}s ago", diff.num_seconds())
    } else if diff.num_minutes() < 60 {
        format!("{}m ago", diff.num_minutes())
    } else if diff.num_hours() < 24 {
        format!("{}h ago", diff.num_hours())
    } else {
        format!("{}d ago", diff.num_days())
    }
}

/// Format duration between two times
fn format_duration(start: DateTime<Local>, end: Option<DateTime<Local>>) -> String {
    let end_time = end.unwrap_or_else(Local::now);
    let diff = end_time.signed_duration_since(start);

    if diff.num_seconds() < 60 {
        format!("{}s", diff.num_seconds())
    } else if diff.num_minutes() < 60 {
        format!("{}m {}s", diff.num_minutes(), diff.num_seconds() % 60)
    } else {
        format!("{}h {}m", diff.num_hours(), diff.num_minutes() % 60)
    }
}

/// Calculate ETA based on progress
fn calculate_eta(processed: u32, total: u32, started: DateTime<Local>) -> String {
    if processed == 0 || total == 0 {
        return "calculating...".to_string();
    }

    let elapsed = Local::now().signed_duration_since(started).num_seconds() as f64;
    let rate = processed as f64 / elapsed;
    let remaining = (total - processed) as f64;
    let eta_secs = (remaining / rate) as i64;

    if eta_secs < 60 {
        format!("{}s", eta_secs)
    } else if eta_secs < 3600 {
        format!("{}m", eta_secs / 60)
    } else {
        format!("{}h", eta_secs / 3600)
    }
}

/// Draw the command palette overlay
/// Layout:
/// ```text
/// +-- > Intent -------------------------------------------+
/// | find all sales_                                       |
/// +-------------------------------------------------------+
/// | > Find files matching pattern                         |
/// |   Start new intent pipeline with file discovery       |
/// |                                                       |
/// |   Recent: "process sales reports"                     |
/// |   Recent: "find all csv files in /data"               |
/// +-------------------------------------------------------+
/// | [Tab] Switch mode  [Up/Down] Navigate  [Enter] Execute|
/// +-------------------------------------------------------+
/// ```
fn draw_command_palette(frame: &mut Frame, app: &App, area: Rect) {
    let palette = &app.command_palette;

    let dialog_area = command_palette_area(palette, area);
    frame.render_widget(Clear, dialog_area);

    // Mode indicator and title
    let mode_indicator = palette.mode.indicator();
    let mode_name = palette.mode.name();
    let title = format!(" {} {} ", mode_indicator, mode_name);

    // Border color based on mode
    let border_color = match palette.mode {
        CommandPaletteMode::Intent => Color::Cyan,
        CommandPaletteMode::Command => Color::Yellow,
        CommandPaletteMode::Navigation => Color::Green,
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .border_type(BorderType::Rounded)
        .title(Span::styled(
            title,
            Style::default().fg(border_color).bold(),
        ));

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    if inner.height < 3 || inner.width < 10 {
        return;
    }

    // Split inner area: input (2 lines) | suggestions | footer (1 line)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Input area
            Constraint::Min(1),    // Suggestions
            Constraint::Length(1), // Footer
        ])
        .split(inner);

    // --- Input Area ---
    let input_area = chunks[0];
    let input_text = &palette.input;
    let cursor_pos = palette.cursor;

    // Build input line with cursor
    let mut input_spans = Vec::new();
    input_spans.push(Span::raw(" "));

    if input_text.is_empty() {
        // Show placeholder
        let placeholder = match palette.mode {
            CommandPaletteMode::Intent => "Type a natural language query...",
            CommandPaletteMode::Command => "Type a command (e.g., /scan)...",
            CommandPaletteMode::Navigation => "Type to filter views...",
        };
        input_spans.push(Span::styled(
            placeholder,
            Style::default().fg(Color::DarkGray),
        ));
    } else {
        // Show input with cursor
        let before_cursor: String = input_text.chars().take(cursor_pos).collect();
        let cursor_char: String = input_text.chars().skip(cursor_pos).take(1).collect();
        let after_cursor: String = input_text.chars().skip(cursor_pos + 1).collect();

        input_spans.push(Span::raw(before_cursor));
        if cursor_char.is_empty() {
            input_spans.push(Span::styled(
                "_",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            ));
        } else {
            input_spans.push(Span::styled(
                cursor_char,
                Style::default().bg(Color::Cyan).fg(Color::Black),
            ));
        }
        input_spans.push(Span::raw(after_cursor));
    }

    let input_line = Line::from(input_spans);
    let input_paragraph = Paragraph::new(vec![input_line, Line::raw("")]);
    frame.render_widget(input_paragraph, input_area);

    // --- Suggestions Area ---
    let suggestions_area = chunks[1];
    let mut suggestion_lines: Vec<Line> = Vec::new();

    for (i, suggestion) in palette.suggestions.iter().enumerate() {
        if i >= 6 {
            break; // Max 6 suggestions visible
        }

        let is_selected = i == palette.selected_suggestion;

        // Selection indicator
        let indicator = if is_selected { " > " } else { "   " };

        // Text style
        let text_style = if is_selected {
            Style::default()
                .fg(Color::White)
                .add_modifier(ratatui::style::Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };

        // Main text line
        let main_line = Line::from(vec![
            Span::styled(
                indicator,
                if is_selected {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default().fg(Color::DarkGray)
                },
            ),
            Span::styled(&suggestion.text, text_style),
        ]);
        suggestion_lines.push(main_line);

        // Description line (indented)
        let desc_style = Style::default().fg(Color::DarkGray);
        let desc_line = Line::from(vec![
            Span::raw("     "),
            Span::styled(&suggestion.description, desc_style),
        ]);
        suggestion_lines.push(desc_line);
    }

    if suggestion_lines.is_empty() {
        suggestion_lines.push(Line::from(Span::styled(
            "   No suggestions",
            Style::default().fg(Color::DarkGray),
        )));
    }

    let suggestions_paragraph = Paragraph::new(suggestion_lines);
    frame.render_widget(suggestions_paragraph, suggestions_area);

    // --- Footer ---
    let footer_area = chunks[2];
    let footer_text = "[Tab] Mode  [Up/Down] Select  [Enter] Run  [Esc] Close";
    let footer = Paragraph::new(Line::from(Span::styled(
        footer_text,
        Style::default().fg(Color::DarkGray),
    )))
    .alignment(Alignment::Center);
    frame.render_widget(footer, footer_area);
}

/// Draw the help overlay (per spec Section 3.1)
fn draw_help_overlay(frame: &mut Frame, area: Rect, app: &App) {
    use ratatui::widgets::Clear;

    let help_area = help_overlay_area(area);

    // Clear the background
    frame.render_widget(Clear, help_area);

    let mode_label = app.view_label();

    let mut help_lines = Vec::new();
    help_lines.push(String::new());
    help_lines.push(format!("  Mode: {}", mode_label));
    help_lines.push(String::new());
    help_lines.push("  ACTIONS".to_string());
    help_lines.push("  -------".to_string());
    help_lines.extend(action_bar::format_help_lines(&app.effective_actions()));
    help_lines.push(String::new());
    help_lines.push("  GLOBAL".to_string());
    help_lines.push("  ------".to_string());
    help_lines.extend(action_bar::format_help_lines(&app.global_actions()));
    help_lines.push(String::new());
    help_lines.push("  Press ? or Esc to close".to_string());

    let help_paragraph = Paragraph::new(help_lines.join("\n"))
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .title(" Help ")
                .title_style(Style::default().fg(Color::Cyan).bold())
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        );

    frame.render_widget(help_paragraph, help_area);
}

fn draw_suggestions_help_overlay(
    frame: &mut Frame,
    area: Rect,
    builder: &super::extraction::RuleBuilderState,
) {
    use super::extraction::SuggestionSection;

    let help_text = match builder.suggestions_section {
        SuggestionSection::Patterns => vec![
            "Detected Patterns".to_string(),
            "".to_string(),
            "Quick glob seeds based on common extensions".to_string(),
            "or filename patterns in the scanned data.".to_string(),
        ],
        SuggestionSection::Structures => vec![
            "Detected Structures".to_string(),
            "".to_string(),
            "Repeated folder layouts inferred from paths.".to_string(),
            "Use these to scope scans or rules precisely.".to_string(),
        ],
        SuggestionSection::Filenames => vec![
            "Detected Filenames".to_string(),
            "".to_string(),
            "Common filename templates extracted from files.".to_string(),
            "Useful for precise match rules or extraction.".to_string(),
        ],
        SuggestionSection::Synonyms => vec![
            "Detected Synonyms".to_string(),
            "".to_string(),
            "Token normalizations for consistent tagging.".to_string(),
            "Example: env → environment.".to_string(),
        ],
    };

    let dialog_area = render_centered_dialog(frame, area, 62, 9);
    let block = Block::default()
        .title(" Suggestions Help ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .border_type(BorderType::Rounded);
    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let lines = help_text.into_iter().map(Line::from).collect::<Vec<_>>();
    let para = Paragraph::new(lines).style(Style::default().fg(Color::White));
    frame.render_widget(para, inner);
}

fn draw_suggestions_detail_overlay(
    frame: &mut Frame,
    area: Rect,
    builder: &super::extraction::RuleBuilderState,
) {
    use super::extraction::SuggestionSection;

    let (title, detail_lines) = match builder.suggestions_section {
        SuggestionSection::Patterns => {
            let idx = builder
                .selected_pattern_seed
                .min(builder.pattern_seeds.len().saturating_sub(1));
            if let Some(seed) = builder.pattern_seeds.get(idx) {
                (
                    "Pattern Details",
                    vec![
                        format!("Pattern: {}", seed.pattern),
                        format!("Matches: {}", seed.match_count),
                    ],
                )
            } else {
                (
                    "Pattern Details",
                    vec!["No pattern suggestions available.".to_string()],
                )
            }
        }
        SuggestionSection::Structures => {
            let idx = builder
                .selected_archetype
                .min(builder.path_archetypes.len().saturating_sub(1));
            if let Some(arch) = builder.path_archetypes.get(idx) {
                let sample = arch
                    .sample_paths
                    .first()
                    .cloned()
                    .unwrap_or_else(|| arch.template.clone());
                (
                    "Structure Details",
                    vec![
                        format!("Template: {}", arch.template),
                        format!("Sample: {}", sample),
                        format!("Files: {}", arch.file_count),
                    ],
                )
            } else {
                (
                    "Structure Details",
                    vec!["No structure suggestions available.".to_string()],
                )
            }
        }
        SuggestionSection::Filenames => {
            let idx = builder
                .selected_naming_scheme
                .min(builder.naming_schemes.len().saturating_sub(1));
            if let Some(scheme) = builder.naming_schemes.get(idx) {
                (
                    "Filename Details",
                    vec![
                        format!("Template: {}", scheme.template),
                        format!("Example: {}", scheme.example),
                        format!("Files: {}", scheme.file_count),
                    ],
                )
            } else {
                (
                    "Filename Details",
                    vec!["No filename suggestions available.".to_string()],
                )
            }
        }
        SuggestionSection::Synonyms => {
            let idx = builder
                .selected_synonym
                .min(builder.synonym_suggestions.len().saturating_sub(1));
            if let Some(syn) = builder.synonym_suggestions.get(idx) {
                (
                    "Synonym Details",
                    vec![format!("{} → {}", syn.short_form, syn.canonical_form)],
                )
            } else {
                (
                    "Synonym Details",
                    vec!["No synonym suggestions available.".to_string()],
                )
            }
        }
    };

    let dialog_area = render_centered_dialog(frame, area, 80, 9);
    let block = Block::default()
        .title(format!(" {} ", title))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .border_type(BorderType::Rounded);
    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let mut wrapped = Vec::new();
    for line in detail_lines {
        wrapped.extend(wrap_text(&line, inner.width.saturating_sub(2) as usize));
    }
    let lines = wrapped.into_iter().map(Line::from).collect::<Vec<_>>();
    let para = Paragraph::new(lines).style(Style::default().fg(Color::White));
    frame.render_widget(para, inner);
}

// ============================================================================
// Glob Explorer Phase UI: Rule Editing, Testing, Publishing
// ============================================================================

// ======== Settings Screen ========

/// Draw the Settings screen - per specs/views/settings.md
fn draw_settings_screen(frame: &mut Frame, app: &App, area: Rect) {
    use crate::cli::tui::app::SettingsCategory;

    let inspector_visible = inspector_visible_for(app, area);
    let shell = shell_layout(area, inspector_visible);

    draw_shell_top_bar(frame, app, shell.top);
    draw_shell_rail(frame, app, shell.rail);

    let content_chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(6), // General
            Constraint::Length(6), // Display
            Constraint::Length(5), // About
        ])
        .split(shell.main);

    // General section
    draw_settings_category(
        frame,
        app,
        content_chunks[0],
        "General",
        SettingsCategory::General,
        &[
            (
                "Default source path",
                &app.settings.default_source_path,
                "[Edit]",
            ),
            (
                "Auto-scan on startup",
                if app.settings.auto_scan_on_startup {
                    "Yes"
                } else {
                    "No"
                },
                "[Toggle]",
            ),
            (
                "Confirm destructive",
                if app.settings.confirm_destructive {
                    "Yes"
                } else {
                    "No"
                },
                "[Toggle]",
            ),
        ],
    );

    // Display section
    draw_settings_category(
        frame,
        app,
        content_chunks[1],
        "Display",
        SettingsCategory::Display,
        &[
            ("Theme", &app.settings.theme, "[Cycle]"),
            (
                "Unicode symbols",
                if app.settings.unicode_symbols {
                    "Yes"
                } else {
                    "No"
                },
                "[Toggle]",
            ),
            (
                "Show hidden files",
                if app.settings.show_hidden_files {
                    "Yes"
                } else {
                    "No"
                },
                "[Toggle]",
            ),
        ],
    );

    // About section (read-only)
    let about_block = Block::default()
        .title(" About ")
        .borders(Borders::ALL)
        .border_style(
            Style::default().fg(if app.settings.category == SettingsCategory::About {
                Color::Cyan
            } else {
                Color::DarkGray
            }),
        );

    let db_path = app.config.database.clone().unwrap_or_else(state_store_path);
    let backend_label = "sqlite";

    let about_lines = vec![
        Line::from(format!("  Version:    {}", env!("CARGO_PKG_VERSION"))),
        Line::from(format!(
            "  Database:   {} ({})",
            db_path.display(),
            backend_label
        )),
        Line::from(format!("  Config:     ~/.casparian_flow/config.toml")),
    ];
    let about = Paragraph::new(about_lines).block(about_block);
    frame.render_widget(about, content_chunks[2]);

    if inspector_visible {
        let inner = draw_shell_inspector_block(frame, "Inspector", shell.inspector);
        draw_settings_inspector(frame, app, inner);
    }

    draw_shell_action_bar(
        frame,
        &app.effective_actions(),
        Style::default().fg(Color::DarkGray),
        shell.bottom,
    );
}

/// Draw a settings category with its settings
fn draw_settings_category(
    frame: &mut Frame,
    app: &App,
    area: Rect,
    title: &str,
    category: crate::cli::tui::app::SettingsCategory,
    settings: &[(&str, &str, &str)],
) {
    let is_active = app.settings.category == category;
    let border_color = if is_active {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let block = Block::default()
        .title(format!(" {} ", title))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Render each setting
    for (i, (label, value, action)) in settings.iter().enumerate() {
        let y = inner.y + i as u16;
        if y >= inner.y + inner.height {
            break;
        }

        let is_selected = is_active && app.settings.selected_index == i;
        let style = if is_selected {
            Style::default().bg(Color::DarkGray).fg(Color::White)
        } else {
            Style::default()
        };

        // Format: "  Label:  Value  [Action]"
        let display_value = if is_selected && app.settings.editing {
            format!("{}█", app.settings.edit_value)
        } else {
            value.to_string()
        };

        let line = format!(
            "  {:<22} {:<20} {}",
            format!("{}:", label),
            display_value,
            action
        );
        let span = Span::styled(line, style);
        let row_area = Rect::new(inner.x, y, inner.width, 1);
        frame.render_widget(Paragraph::new(span), row_area);
    }
}

fn draw_settings_inspector(frame: &mut Frame, app: &App, area: Rect) {
    let db_path = app.config.database.clone().unwrap_or_else(state_store_path);
    let backend_label = "sqlite";

    let lines = vec![
        Line::from(Span::styled(
            format!("Version: {}", env!("CARGO_PKG_VERSION")),
            Style::default().fg(Color::White),
        )),
        Line::from(Span::styled(
            format!("Database: {} ({})", db_path.display(), backend_label),
            Style::default().fg(Color::Gray),
        )),
        Line::from(Span::styled(
            "Config: ~/.casparian_flow/config.toml".to_string(),
            Style::default().fg(Color::Gray),
        )),
    ];
    let para = Paragraph::new(lines).wrap(Wrap { trim: true });
    frame.render_widget(para, area);
}

// ======== Query Console Screen ========

/// Draw the Query Console screen - SQL query interface
fn draw_query_screen(frame: &mut Frame, app: &App, area: Rect) {
    use crate::cli::tui::app::QueryViewState;

    let inspector_visible = inspector_visible_for(app, area);
    let shell = shell_layout(area, inspector_visible);

    draw_shell_top_bar(frame, app, shell.top);
    draw_shell_rail(frame, app, shell.rail);

    // Main content: SQL Editor (top 30%) + Results (bottom 70%)
    let content_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(30), // SQL Editor
            Constraint::Percentage(70), // Results/Error area
        ])
        .split(shell.main);

    // Draw SQL Editor
    draw_query_editor(frame, app, content_chunks[0]);

    // Draw Results or Error
    draw_query_results(frame, app, content_chunks[1]);

    // Draw Inspector (if visible)
    if inspector_visible {
        let inner = draw_shell_inspector_block(frame, "Query Info", shell.inspector);
        draw_query_inspector(frame, app, inner);
    }

    if matches!(
        app.query_state.view_state,
        QueryViewState::TableBrowser | QueryViewState::SavedQueries
    ) {
        draw_modal_scrim(frame, area, shell.top);
    }

    if app.query_state.view_state == QueryViewState::TableBrowser {
        draw_query_table_browser_overlay(frame, area, app);
    }
    if app.query_state.view_state == QueryViewState::SavedQueries {
        draw_query_saved_queries_overlay(frame, area, app);
    }

    draw_shell_action_bar(
        frame,
        &app.effective_actions(),
        Style::default().fg(Color::DarkGray),
        shell.bottom,
    );
}

/// Draw the SQL editor area with line numbers
fn draw_query_editor(frame: &mut Frame, app: &App, area: Rect) {
    use crate::cli::tui::app::QueryViewState;

    let is_focused = app.query_state.view_state == QueryViewState::Editing;

    let border_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .title(" SQL Editor ")
        .borders(Borders::ALL)
        .border_style(border_style)
        .border_type(if is_focused {
            BorderType::Double
        } else {
            BorderType::Rounded
        });

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Split inner area for line numbers and content
    let editor_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(4), // Line numbers
            Constraint::Min(1),    // SQL content
        ])
        .split(inner);

    // Calculate lines from input
    let lines: Vec<&str> = app.query_state.sql_input.split('\n').collect();
    let line_count = lines.len().max(1);

    // Draw line numbers
    let line_numbers: Vec<Line> = (1..=line_count)
        .map(|n| {
            Line::from(Span::styled(
                format!("{:>3} ", n),
                Style::default().fg(Color::DarkGray),
            ))
        })
        .collect();
    let line_nums_para = Paragraph::new(line_numbers);
    frame.render_widget(line_nums_para, editor_chunks[0]);

    // Draw SQL content with cursor
    let mut content_lines: Vec<Line> = Vec::new();
    let mut char_pos = 0;
    let cursor_pos = app.query_state.cursor_position;

    for line_text in lines.iter() {
        let line_start = char_pos;
        let line_end = char_pos + line_text.len();

        // Check if cursor is on this line
        let cursor_on_line = is_focused && cursor_pos >= line_start && cursor_pos <= line_end;

        if cursor_on_line {
            let cursor_offset = cursor_pos - line_start;
            let before = &line_text[..cursor_offset.min(line_text.len())];
            let after = if cursor_offset < line_text.len() {
                &line_text[cursor_offset..]
            } else {
                ""
            };

            // Add cursor character
            let cursor_char = if cursor_offset < line_text.len() {
                line_text.chars().nth(cursor_offset).unwrap_or(' ')
            } else {
                ' '
            };

            content_lines.push(Line::from(vec![
                Span::styled(before.to_string(), Style::default().fg(Color::White)),
                Span::styled(
                    cursor_char.to_string(),
                    Style::default().fg(Color::Black).bg(Color::White),
                ),
                Span::styled(
                    if cursor_offset < line_text.len() && after.len() > 1 {
                        after[1..].to_string()
                    } else {
                        String::new()
                    },
                    Style::default().fg(Color::White),
                ),
            ]));
        } else {
            // Simple syntax highlighting for SQL keywords
            let highlighted = highlight_sql(line_text);
            content_lines.push(highlighted);
        }

        char_pos = line_end + 1; // +1 for newline
    }

    // If empty, show placeholder
    if app.query_state.sql_input.is_empty() && is_focused {
        content_lines = vec![Line::from(vec![
            Span::styled(" ", Style::default().fg(Color::Black).bg(Color::White)),
            Span::styled("Enter SQL query...", Style::default().fg(Color::DarkGray)),
        ])];
    } else if app.query_state.sql_input.is_empty() {
        content_lines = vec![Line::from(Span::styled(
            "Enter SQL query...",
            Style::default().fg(Color::DarkGray),
        ))];
    }

    // Show history indicator if browsing
    if let Some(idx) = app.query_state.history_index {
        let history_hint = format!("History [{}/{}]", idx + 1, app.query_state.history.len());
        content_lines.push(Line::from(""));
        content_lines.push(Line::from(Span::styled(
            history_hint,
            Style::default().fg(Color::Yellow),
        )));
    }

    let sql_para = Paragraph::new(content_lines);
    frame.render_widget(sql_para, editor_chunks[1]);
}

/// Simple SQL keyword highlighting
fn highlight_sql(text: &str) -> Line<'static> {
    let keywords = [
        "SELECT", "FROM", "WHERE", "AND", "OR", "ORDER", "BY", "GROUP", "HAVING", "INSERT", "INTO",
        "VALUES", "UPDATE", "SET", "DELETE", "CREATE", "DROP", "TABLE", "INDEX", "VIEW", "JOIN",
        "LEFT", "RIGHT", "INNER", "OUTER", "ON", "AS", "DISTINCT", "LIMIT", "OFFSET", "NULL",
        "NOT", "IN", "LIKE", "BETWEEN", "EXISTS", "CASE", "WHEN", "THEN", "ELSE", "END", "COUNT",
        "SUM", "AVG", "MIN", "MAX", "ASC", "DESC", "UNION", "ALL", "WITH",
    ];

    let mut spans: Vec<Span> = Vec::new();
    let words: Vec<&str> = text
        .split_inclusive(|c: char| c.is_whitespace() || c == ',' || c == '(' || c == ')')
        .collect();

    for word in words {
        let trimmed =
            word.trim_matches(|c: char| c.is_whitespace() || c == ',' || c == '(' || c == ')');
        let is_keyword = keywords.iter().any(|k| trimmed.eq_ignore_ascii_case(k));

        if is_keyword {
            // Find the keyword part and preserve surrounding chars
            let start_idx = word.find(trimmed).unwrap_or(0);
            let before = &word[..start_idx];
            let after = &word[start_idx + trimmed.len()..];

            if !before.is_empty() {
                spans.push(Span::styled(
                    before.to_string(),
                    Style::default().fg(Color::White),
                ));
            }
            spans.push(Span::styled(
                trimmed.to_string(),
                Style::default().fg(Color::Cyan).bold(),
            ));
            if !after.is_empty() {
                spans.push(Span::styled(
                    after.to_string(),
                    Style::default().fg(Color::White),
                ));
            }
        } else if trimmed.starts_with('\'') || trimmed.starts_with('"') {
            // String literal
            spans.push(Span::styled(
                word.to_string(),
                Style::default().fg(Color::Green),
            ));
        } else if trimmed.parse::<f64>().is_ok() {
            // Number
            spans.push(Span::styled(
                word.to_string(),
                Style::default().fg(Color::Yellow),
            ));
        } else {
            spans.push(Span::styled(
                word.to_string(),
                Style::default().fg(Color::White),
            ));
        }
    }

    Line::from(spans)
}

/// Draw the query results area (table or error)
fn draw_query_results(frame: &mut Frame, app: &App, area: Rect) {
    use crate::cli::tui::app::QueryViewState;

    let is_focused = app.query_state.view_state == QueryViewState::ViewingResults;
    let is_executing = app.query_state.executing;

    let border_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    // Determine title based on state
    let title = if is_executing {
        " Executing... ".to_string()
    } else if let Some(ref results) = app.query_state.results {
        if results.truncated {
            format!(
                " Results ({} rows, showing {}) ",
                results.row_count,
                results.rows.len()
            )
        } else {
            format!(" Results ({} rows) ", results.row_count)
        }
    } else if app.query_state.error.is_some() {
        " Error ".to_string()
    } else {
        " Results ".to_string()
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style)
        .border_type(if is_focused {
            BorderType::Double
        } else {
            BorderType::Rounded
        });

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Show spinner if executing
    if is_executing {
        let spinner_frames = [".", "..", "...", "...."];
        let frame_idx = (app.tick_count / 5) as usize % spinner_frames.len();
        let spinner = spinner_frames[frame_idx];
        let loading = Paragraph::new(format!("  Running query{}", spinner))
            .style(Style::default().fg(Color::Yellow));
        frame.render_widget(loading, inner);
        return;
    }

    // Show error if present
    if let Some(ref error) = app.query_state.error {
        let error_lines: Vec<Line> = error
            .lines()
            .map(|l| {
                Line::from(Span::styled(
                    format!("  {}", l),
                    Style::default().fg(Color::Red),
                ))
            })
            .collect();
        let error_para = Paragraph::new(error_lines);
        frame.render_widget(error_para, inner);
        return;
    }

    // Show results if present
    if let Some(ref results) = app.query_state.results {
        if results.columns.is_empty() {
            let empty = Paragraph::new("  Query completed. No columns returned.")
                .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(empty, inner);
            return;
        }

        // Calculate column widths
        let col_widths: Vec<usize> = results
            .columns
            .iter()
            .enumerate()
            .map(|(i, col)| {
                let header_width = col.len();
                let max_data_width = results
                    .rows
                    .iter()
                    .map(|row| row.get(i).map(|s| s.len()).unwrap_or(0))
                    .max()
                    .unwrap_or(0);
                header_width.max(max_data_width).min(30) // Cap at 30 chars
            })
            .collect();

        // Determine visible columns based on scroll_x
        let available_width = inner.width as usize;
        let scroll_x = results.scroll_x;

        // Build header row
        let mut header_spans: Vec<Span> = Vec::new();
        let mut current_width = 0;
        for (i, (col, &width)) in results
            .columns
            .iter()
            .zip(col_widths.iter())
            .enumerate()
            .skip(scroll_x)
        {
            if current_width + width + 3 > available_width && i > scroll_x {
                break;
            }
            let display = if col.len() > width {
                format!("{}...", &col[..width.saturating_sub(3)])
            } else {
                format!("{:width$}", col, width = width)
            };
            header_spans.push(Span::styled(
                format!(" {} ", display),
                Style::default().fg(Color::Cyan).bold(),
            ));
            header_spans.push(Span::styled("|", Style::default().fg(Color::DarkGray)));
            current_width += width + 3;
        }

        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(header_spans));

        // Separator line
        let separator: String = col_widths
            .iter()
            .skip(scroll_x)
            .take_while(|_| current_width < available_width)
            .map(|&w| format!("-{}-+", "-".repeat(w)))
            .collect();
        lines.push(Line::from(Span::styled(
            separator,
            Style::default().fg(Color::DarkGray),
        )));

        // Data rows
        let visible_height = inner.height.saturating_sub(3) as usize;
        let scroll_offset = if results.selected_row >= visible_height {
            results.selected_row - visible_height + 1
        } else {
            0
        };

        for (row_idx, row) in results
            .rows
            .iter()
            .enumerate()
            .skip(scroll_offset)
            .take(visible_height)
        {
            let is_selected = is_focused && row_idx == results.selected_row;
            let row_style = if is_selected {
                Style::default().bg(Color::DarkGray).fg(Color::White)
            } else {
                Style::default()
            };

            let mut row_spans: Vec<Span> = Vec::new();
            let mut row_width = 0;
            for (i, (cell, &width)) in row.iter().zip(col_widths.iter()).enumerate().skip(scroll_x)
            {
                if i > scroll_x && row_width + width + 3 > available_width {
                    break;
                }
                let display = if cell.len() > width {
                    format!("{}...", &cell[..width.saturating_sub(3)])
                } else {
                    format!("{:width$}", cell, width = width)
                };
                row_spans.push(Span::styled(format!(" {} ", display), row_style));
                row_spans.push(Span::styled("|", Style::default().fg(Color::DarkGray)));
                row_width += width + 3;
            }
            lines.push(Line::from(row_spans));
        }

        let table = Paragraph::new(lines);
        frame.render_widget(table, inner);
    } else {
        // Empty state
        let empty = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "  No query results yet.",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Enter a SQL query and press Ctrl+Enter to execute.",
                Style::default().fg(Color::DarkGray),
            )),
        ]);
        frame.render_widget(empty, inner);
    }
}

/// Draw query inspector panel
fn draw_query_inspector(frame: &mut Frame, app: &App, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();

    // Execution time
    if let Some(ms) = app.query_state.execution_time_ms {
        lines.push(Line::from(vec![
            Span::styled("Execution: ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{}ms", ms), Style::default().fg(Color::Cyan)),
        ]));
    }

    // Row count
    if let Some(ref results) = app.query_state.results {
        lines.push(Line::from(vec![
            Span::styled("Rows: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{}", results.row_count),
                Style::default().fg(Color::White),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Columns: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{}", results.columns.len()),
                Style::default().fg(Color::White),
            ),
        ]));
        if results.truncated {
            lines.push(Line::from(Span::styled(
                "Results truncated",
                Style::default().fg(Color::Yellow),
            )));
        }
    }

    // History count
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("History: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{} queries", app.query_state.history.len()),
            Style::default().fg(Color::White),
        ),
    ]));

    // Current state
    lines.push(Line::from(""));
    let state_label = if app.query_state.executing {
        "Executing"
    } else {
        match app.query_state.view_state {
            crate::cli::tui::app::QueryViewState::Editing => "Editing",
            crate::cli::tui::app::QueryViewState::Executing => "Executing",
            crate::cli::tui::app::QueryViewState::ViewingResults => "Viewing Results",
            crate::cli::tui::app::QueryViewState::TableBrowser => "Table Browser",
            crate::cli::tui::app::QueryViewState::SavedQueries => "Saved Queries",
        }
    };
    lines.push(Line::from(vec![
        Span::styled("State: ", Style::default().fg(Color::DarkGray)),
        Span::styled(state_label, Style::default().fg(Color::Cyan)),
    ]));

    let para = Paragraph::new(lines);
    frame.render_widget(para, area);
}

fn draw_query_table_browser_overlay(frame: &mut Frame, area: Rect, app: &App) {
    let dialog = centered_dialog_area(area, 60, 20);
    frame.render_widget(Clear, dialog);

    let block = Block::default()
        .title(" Tables ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .border_type(BorderType::Rounded);
    let inner = block.inner(dialog);
    frame.render_widget(block, dialog);

    if let Some(error) = &app.query_state.table_browser.error {
        let msg = Paragraph::new(error.as_str())
            .style(Style::default().fg(Color::Red))
            .alignment(Alignment::Center);
        frame.render_widget(msg, inner);
        return;
    }

    if app.query_state.table_browser.tables.is_empty() {
        let msg = Paragraph::new("No tables found.")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(msg, inner);
        return;
    }

    let items: Vec<ratatui::widgets::ListItem> = app
        .query_state
        .table_browser
        .tables
        .iter()
        .enumerate()
        .map(|(idx, name)| {
            let is_selected = idx == app.query_state.table_browser.selected_index;
            let style = if is_selected {
                Style::default().fg(Color::White).bold().bg(Color::DarkGray)
            } else {
                Style::default().fg(Color::Gray)
            };
            ratatui::widgets::ListItem::new(name.as_str()).style(style)
        })
        .collect();

    let list = ratatui::widgets::List::new(items);
    frame.render_widget(list, inner);
}

fn draw_query_saved_queries_overlay(frame: &mut Frame, area: Rect, app: &App) {
    let dialog = centered_dialog_area(area, 60, 20);
    frame.render_widget(Clear, dialog);

    let block = Block::default()
        .title(" Saved Queries ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .border_type(BorderType::Rounded);
    let inner = block.inner(dialog);
    frame.render_widget(block, dialog);

    if let Some(error) = &app.query_state.saved_queries.error {
        let msg = Paragraph::new(error.as_str())
            .style(Style::default().fg(Color::Red))
            .alignment(Alignment::Center);
        frame.render_widget(msg, inner);
        return;
    }

    if app.query_state.saved_queries.entries.is_empty() {
        let msg = Paragraph::new("No saved queries.")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(msg, inner);
        return;
    }

    let items: Vec<ratatui::widgets::ListItem> = app
        .query_state
        .saved_queries
        .entries
        .iter()
        .enumerate()
        .map(|(idx, entry)| {
            let is_selected = idx == app.query_state.saved_queries.selected_index;
            let style = if is_selected {
                Style::default().fg(Color::White).bold().bg(Color::DarkGray)
            } else {
                Style::default().fg(Color::Gray)
            };
            ratatui::widgets::ListItem::new(entry.name.as_str()).style(style)
        })
        .collect();

    let list = ratatui::widgets::List::new(items);
    frame.render_widget(list, inner);
}

// ======== Sessions Screen (Intent Pipeline Workflow) ========

/// Draw the Sessions screen - View 7 for Intent Pipeline Workflows
fn draw_sessions_screen(frame: &mut Frame, app: &App, area: Rect) {
    use crate::cli::tui::app::SessionsViewState;

    let inspector_visible = inspector_visible_for(app, area);
    let shell = shell_layout(area, inspector_visible);

    draw_shell_top_bar(frame, app, shell.top);
    draw_shell_rail(frame, app, shell.rail);

    // Three-panel layout: Session list | Workflow progress | Proposal/Gate details
    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30), // Session list
            Constraint::Percentage(35), // Workflow progress
            Constraint::Percentage(35), // Proposal/Gate details
        ])
        .split(shell.main);

    // Draw the three panels
    draw_sessions_list_panel(frame, app, content_chunks[0]);
    draw_workflow_progress_panel(frame, app, content_chunks[1]);
    draw_gate_details_panel(frame, app, content_chunks[2]);

    if inspector_visible {
        let inner = draw_shell_inspector_block(frame, "Inspector", shell.inspector);
        draw_sessions_inspector(frame, app, inner);
    }

    draw_shell_action_bar(
        frame,
        &app.effective_actions(),
        Style::default().fg(Color::DarkGray),
        shell.bottom,
    );
}

/// Draw the sessions list panel (left)
fn draw_sessions_list_panel(frame: &mut Frame, app: &App, area: Rect) {
    let is_active = matches!(
        app.sessions_state.view_state,
        super::app::SessionsViewState::SessionList
    );
    let border_color = if is_active {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let block = Block::default()
        .title(" Sessions ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.sessions_state.sessions.is_empty() {
        let msg = Paragraph::new("No sessions yet.\n\nPress [n] to start a new session.")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(msg, inner);
        return;
    }

    // Build session list items
    let items: Vec<ratatui::widgets::ListItem> = app
        .sessions_state
        .sessions
        .iter()
        .enumerate()
        .map(|(i, session)| {
            let is_selected = i == app.sessions_state.selected_index;
            let style = if is_selected {
                Style::default().fg(Color::White).bold().bg(Color::DarkGray)
            } else {
                Style::default().fg(Color::Gray)
            };

            let prefix = if is_selected { ">" } else { " " };
            let gate_badge = session
                .pending_gate
                .as_ref()
                .map(|g| format!(" [{}]", g))
                .unwrap_or_default();

            // Truncate intent if too long
            let max_intent_len = (inner.width as usize).saturating_sub(12);
            let intent_display = if session.intent.len() > max_intent_len {
                format!("{}...", &session.intent[..max_intent_len.saturating_sub(3)])
            } else {
                session.intent.clone()
            };

            let line = format!("{} {}{}", prefix, intent_display, gate_badge);
            ratatui::widgets::ListItem::new(line).style(style)
        })
        .collect();

    let list = ratatui::widgets::List::new(items);
    frame.render_widget(list, inner);
}

/// Draw the workflow progress panel (center)
fn draw_workflow_progress_panel(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Workflow Progress ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Get the selected session or show empty state
    let Some(session) = app.sessions_state.selected_session() else {
        let msg = Paragraph::new("Select a session to view workflow progress")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(msg, inner);
        return;
    };

    // Draw workflow state diagram
    draw_workflow_state_diagram(frame, inner, session.state);
}

/// Draw the workflow state diagram as a vertical list
fn draw_workflow_state_diagram(frame: &mut Frame, area: Rect, current_state: Option<IntentState>) {
    let states: Vec<IntentState> = IntentState::ALL
        .iter()
        .copied()
        .filter(|state| !state.is_terminal())
        .collect();
    let (current_index, terminal_label, unknown_state) = match current_state {
        Some(state) if state.is_terminal() => (None, Some(state.as_str()), false),
        Some(state) => (states.iter().position(|s| *s == state), None, false),
        None => (None, None, true),
    };

    let mut lines: Vec<Line> = Vec::new();

    if let Some(label) = terminal_label {
        lines.push(Line::from(Span::styled(
            format!(" ! Terminal: {}", label),
            Style::default().fg(Color::Yellow),
        )));
        lines.push(Line::from(""));
    } else if unknown_state {
        lines.push(Line::from(Span::styled(
            " ? Unknown state",
            Style::default().fg(Color::Yellow),
        )));
        lines.push(Line::from(""));
    }

    for (idx, state) in states.iter().enumerate() {
        let (id, name) = intent_state_label(*state);
        let is_gate = state.is_gate();
        let state_matches = current_index == Some(idx);
        let past_current = current_index.map(|cur| idx < cur).unwrap_or(false);

        let (symbol, style) = if state_matches {
            if is_gate {
                ("*", Style::default().fg(Color::Yellow).bold()) // Awaiting approval
            } else {
                ("@", Style::default().fg(Color::Cyan).bold()) // In progress
            }
        } else if past_current {
            ("o", Style::default().fg(Color::Green)) // Complete
        } else {
            (".", Style::default().fg(Color::DarkGray)) // Pending
        };

        let status_text = if state_matches && is_gate {
            " [AWAITING APPROVAL]"
        } else if state_matches {
            " [IN PROGRESS]"
        } else {
            ""
        };

        let line_text = format!(" {} {} {}{}", symbol, id, name, status_text);
        lines.push(Line::from(Span::styled(line_text, style)));
    }

    let para = Paragraph::new(lines);
    frame.render_widget(para, area);
}

fn intent_state_label(state: IntentState) -> (String, String) {
    let raw = state.as_str();
    let mut parts = raw.splitn(2, '_');
    let id = parts.next().unwrap_or(raw).to_string();
    let remainder = parts.next().unwrap_or("");
    let name = if remainder.is_empty() {
        id.clone()
    } else {
        remainder
            .split('_')
            .filter(|part| !part.is_empty())
            .map(|part| {
                let mut chars = part.chars();
                match chars.next() {
                    Some(first) => {
                        let lower = chars.as_str().to_lowercase();
                        format!("{}{}", first, lower)
                    }
                    None => String::new(),
                }
            })
            .collect::<Vec<String>>()
            .join(" ")
    };
    (id, name)
}

/// Draw the gate/proposal details panel (right)
fn draw_gate_details_panel(frame: &mut Frame, app: &App, area: Rect) {
    let is_gate_view = matches!(
        app.sessions_state.view_state,
        super::app::SessionsViewState::GateApproval
    );
    let border_color = if is_gate_view {
        Color::Yellow
    } else {
        Color::DarkGray
    };
    let title = if is_gate_view {
        " Gate Approval "
    } else {
        " Details "
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Check for pending gate
    if let Some(ref gate) = app.sessions_state.pending_gate {
        draw_gate_approval_content(frame, inner, gate);
        return;
    }

    // Check for selected session
    let Some(session) = app.sessions_state.selected_session() else {
        let msg = Paragraph::new("No active gate or proposal")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(msg, inner);
        return;
    };

    // Show session summary
    let mut lines = vec![
        Line::from(Span::styled(
            format!("Session: {}", &session.id[..8.min(session.id.len())]),
            Style::default().fg(Color::White),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!("Intent: {}", session.intent),
            Style::default().fg(Color::Gray),
        )),
        Line::from(Span::styled(
            format!("State: {}", session.state_label),
            Style::default().fg(Color::Gray),
        )),
        Line::from(Span::styled(
            format!("Files: {}", session.file_count),
            Style::default().fg(Color::Gray),
        )),
    ];

    if let Some(ref gate_id) = session.pending_gate {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("Pending Gate: {}", gate_id),
            Style::default().fg(Color::Yellow).bold(),
        )));
        lines.push(Line::from(Span::styled(
            "Press [Enter] to review",
            Style::default().fg(Color::DarkGray),
        )));
    }

    let para = Paragraph::new(lines).wrap(Wrap { trim: true });
    frame.render_widget(para, inner);
}

/// Draw the gate approval content
fn draw_gate_approval_content(frame: &mut Frame, area: Rect, gate: &super::app::GateInfo) {
    let confidence_style = match gate.confidence.to_uppercase().as_str() {
        "HIGH" => Style::default().fg(Color::Green).bold(),
        "MEDIUM" | "MED" => Style::default().fg(Color::Yellow).bold(),
        _ => Style::default().fg(Color::Red).bold(),
    };

    let mut lines = vec![
        Line::from(vec![
            Span::styled("Gate: ", Style::default().fg(Color::White)),
            Span::styled(&gate.gate_id, Style::default().fg(Color::Yellow).bold()),
            Span::raw(" - "),
            Span::styled(&gate.gate_name, Style::default().fg(Color::White)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Confidence: ", Style::default().fg(Color::Gray)),
            Span::styled(&gate.confidence, confidence_style),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Summary:",
            Style::default().fg(Color::White).bold(),
        )),
        Line::from(Span::styled(
            &gate.proposal_summary,
            Style::default().fg(Color::Gray),
        )),
    ];

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Selected Examples:",
        Style::default().fg(Color::White).bold(),
    )));
    if gate.selected_examples.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (none)",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for example in gate.selected_examples.iter().take(5) {
            lines.push(Line::from(Span::styled(
                format!("  - {}", example),
                Style::default().fg(Color::Gray),
            )));
        }
        if gate.selected_examples.len() > 5 {
            lines.push(Line::from(Span::styled(
                "  ...",
                Style::default().fg(Color::DarkGray),
            )));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Near Misses:",
        Style::default().fg(Color::White).bold(),
    )));
    if gate.near_miss_examples.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (none)",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for example in gate.near_miss_examples.iter().take(5) {
            lines.push(Line::from(Span::styled(
                format!("  - {}", example),
                Style::default().fg(Color::Gray),
            )));
        }
        if gate.near_miss_examples.len() > 5 {
            lines.push(Line::from(Span::styled(
                "  ...",
                Style::default().fg(Color::DarkGray),
            )));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Evidence:",
        Style::default().fg(Color::White).bold(),
    )));
    for evidence in &gate.evidence {
        lines.push(Line::from(Span::styled(
            format!("  - {}", evidence),
            Style::default().fg(Color::Gray),
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Next Actions:",
        Style::default().fg(Color::White).bold(),
    )));
    if gate.next_actions.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (none)",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for action in &gate.next_actions {
            lines.push(Line::from(Span::styled(
                format!("  - {}", action),
                Style::default().fg(Color::Gray),
            )));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "[a] Approve  [r] Reject  [Esc] Cancel",
        Style::default().fg(Color::DarkGray),
    )));

    let para = Paragraph::new(lines).wrap(Wrap { trim: true });
    frame.render_widget(para, area);
}

/// Draw the sessions inspector panel
fn draw_sessions_inspector(frame: &mut Frame, app: &App, area: Rect) {
    let mut lines = Vec::new();

    if let Some(session) = app.sessions_state.selected_session() {
        lines.push(Line::from(Span::styled(
            "Selected Session",
            Style::default().fg(Color::White).bold(),
        )));
        lines.push(Line::from(Span::styled(
            format!("ID: {}", session.id),
            Style::default().fg(Color::Gray),
        )));
        lines.push(Line::from(Span::styled(
            format!("State: {}", session.state_label),
            Style::default().fg(Color::Gray),
        )));
        lines.push(Line::from(Span::styled(
            format!("Created: {}", session.created_at.format("%Y-%m-%d %H:%M")),
            Style::default().fg(Color::Gray),
        )));
        lines.push(Line::from(Span::styled(
            format!("Files: {}", session.file_count),
            Style::default().fg(Color::Gray),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "No session selected",
            Style::default().fg(Color::DarkGray),
        )));
    }

    let para = Paragraph::new(lines).wrap(Wrap { trim: true });
    frame.render_widget(para, area);
}

// ======== Triage Screen (Quarantine + Schema + Dead Letter) ========

fn draw_triage_screen(frame: &mut Frame, app: &App, area: Rect) {
    let inspector_visible = inspector_visible_for(app, area);
    let shell = shell_layout(area, inspector_visible);

    draw_shell_top_bar(frame, app, shell.top);
    draw_shell_rail(frame, app, shell.rail);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(shell.main);

    draw_triage_tabs(frame, app, chunks[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(chunks[1]);

    draw_triage_list_panel(frame, app, body[0]);
    draw_triage_detail_panel(frame, app, body[1]);

    if inspector_visible {
        let inner = draw_shell_inspector_block(frame, "Inspector", shell.inspector);
        draw_triage_inspector(frame, app, inner);
    }

    draw_shell_action_bar(
        frame,
        &app.effective_actions(),
        Style::default().fg(Color::DarkGray),
        shell.bottom,
    );
}

fn draw_triage_tabs(frame: &mut Frame, app: &App, area: Rect) {
    let mut spans = Vec::new();
    let tabs = [
        TriageTab::Quarantine,
        TriageTab::SchemaMismatch,
        TriageTab::DeadLetter,
    ];
    for (idx, tab) in tabs.iter().enumerate() {
        let is_active = app.triage_state.tab == *tab;
        let style = if is_active {
            Style::default().fg(Color::Cyan).bold()
        } else {
            Style::default().fg(Color::DarkGray)
        };
        spans.push(Span::styled(tab.label(), style));
        if idx + 1 < tabs.len() {
            spans.push(Span::styled("  |  ", Style::default().fg(Color::DarkGray)));
        }
    }

    let filter_label = if let Some(job_id) = app.triage_state.job_filter {
        format!("Job filter: {}", job_id)
    } else {
        "All jobs".to_string()
    };

    let line = Line::from(spans);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {}", filter_label));
    let para = Paragraph::new(line).block(block);
    frame.render_widget(para, area);
}

fn draw_triage_list_panel(frame: &mut Frame, app: &App, area: Rect) {
    let title = format!(" {} ", app.triage_state.tab.label());
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    match app.triage_state.tab {
        TriageTab::Quarantine => match app.triage_state.quarantine_rows.as_ref() {
            None => {
                let msg = Paragraph::new("Table not available.")
                    .style(Style::default().fg(Color::DarkGray))
                    .alignment(Alignment::Center);
                frame.render_widget(msg, inner);
            }
            Some(rows) if rows.is_empty() => {
                let msg = Paragraph::new("No rows.")
                    .style(Style::default().fg(Color::DarkGray))
                    .alignment(Alignment::Center);
                frame.render_widget(msg, inner);
            }
            Some(rows) => {
                let items: Vec<ratatui::widgets::ListItem> = rows
                    .iter()
                    .enumerate()
                    .map(|(idx, row)| {
                        let is_selected = idx == app.triage_state.selected_index;
                        let style = if is_selected {
                            Style::default().fg(Color::White).bold().bg(Color::DarkGray)
                        } else {
                            Style::default().fg(Color::Gray)
                        };
                        let text = format!(
                            "#{} job {} row {} {}",
                            row.id, row.job_id, row.row_index, row.error_reason
                        );
                        let line = truncate_end(&text, inner.width as usize);
                        ratatui::widgets::ListItem::new(line).style(style)
                    })
                    .collect();
                let list = ratatui::widgets::List::new(items);
                frame.render_widget(list, inner);
            }
        },
        TriageTab::SchemaMismatch => match app.triage_state.schema_mismatches.as_ref() {
            None => {
                let msg = Paragraph::new("Table not available.")
                    .style(Style::default().fg(Color::DarkGray))
                    .alignment(Alignment::Center);
                frame.render_widget(msg, inner);
            }
            Some(rows) if rows.is_empty() => {
                let msg = Paragraph::new("No rows.")
                    .style(Style::default().fg(Color::DarkGray))
                    .alignment(Alignment::Center);
                frame.render_widget(msg, inner);
            }
            Some(rows) => {
                let items: Vec<ratatui::widgets::ListItem> = rows
                    .iter()
                    .enumerate()
                    .map(|(idx, row)| {
                        let is_selected = idx == app.triage_state.selected_index;
                        let style = if is_selected {
                            Style::default().fg(Color::White).bold().bg(Color::DarkGray)
                        } else {
                            Style::default().fg(Color::Gray)
                        };
                        let text = format!("#{} job {} {}", row.id, row.job_id, row.mismatch_kind);
                        let line = truncate_end(&text, inner.width as usize);
                        ratatui::widgets::ListItem::new(line).style(style)
                    })
                    .collect();
                let list = ratatui::widgets::List::new(items);
                frame.render_widget(list, inner);
            }
        },
        TriageTab::DeadLetter => match app.triage_state.dead_letters.as_ref() {
            None => {
                let msg = Paragraph::new("Table not available.")
                    .style(Style::default().fg(Color::DarkGray))
                    .alignment(Alignment::Center);
                frame.render_widget(msg, inner);
            }
            Some(rows) if rows.is_empty() => {
                let msg = Paragraph::new("No rows.")
                    .style(Style::default().fg(Color::DarkGray))
                    .alignment(Alignment::Center);
                frame.render_widget(msg, inner);
            }
            Some(rows) => {
                let items: Vec<ratatui::widgets::ListItem> = rows
                    .iter()
                    .enumerate()
                    .map(|(idx, row)| {
                        let is_selected = idx == app.triage_state.selected_index;
                        let style = if is_selected {
                            Style::default().fg(Color::White).bold().bg(Color::DarkGray)
                        } else {
                            Style::default().fg(Color::Gray)
                        };
                        let text = format!(
                            "#{} job {} {}",
                            row.id, row.original_job_id, row.plugin_name
                        );
                        let line = truncate_end(&text, inner.width as usize);
                        ratatui::widgets::ListItem::new(line).style(style)
                    })
                    .collect();
                let list = ratatui::widgets::List::new(items);
                frame.render_widget(list, inner);
            }
        },
    }
}

fn draw_triage_detail_panel(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Details ")
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    match app.triage_state.tab {
        TriageTab::Quarantine => match app.triage_state.quarantine_rows.as_ref() {
            None => {
                let msg = Paragraph::new("Table not available.")
                    .style(Style::default().fg(Color::DarkGray))
                    .alignment(Alignment::Center);
                frame.render_widget(msg, inner);
                return;
            }
            Some(rows) if rows.is_empty() => {
                let msg = Paragraph::new("No selection.")
                    .style(Style::default().fg(Color::DarkGray))
                    .alignment(Alignment::Center);
                frame.render_widget(msg, inner);
                return;
            }
            Some(rows) => {
                let row = &rows[app.triage_state.selected_index];
                lines.push(Line::from(format!("Row ID: {}", row.id)));
                lines.push(Line::from(format!("Job ID: {}", row.job_id)));
                lines.push(Line::from(format!("Row Index: {}", row.row_index)));
                lines.push(Line::from(format!("Reason: {}", row.error_reason)));
                lines.push(Line::from(format!("Created: {}", row.created_at)));
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "Raw Data:",
                    Style::default().fg(Color::White).bold(),
                )));
                for line in raw_data_preview(&row.raw_data) {
                    lines.push(Line::from(Span::styled(
                        format!("  {}", line),
                        Style::default().fg(Color::Gray),
                    )));
                }
            }
        },
        TriageTab::SchemaMismatch => match app.triage_state.schema_mismatches.as_ref() {
            None => {
                let msg = Paragraph::new("Table not available.")
                    .style(Style::default().fg(Color::DarkGray))
                    .alignment(Alignment::Center);
                frame.render_widget(msg, inner);
                return;
            }
            Some(rows) if rows.is_empty() => {
                let msg = Paragraph::new("No selection.")
                    .style(Style::default().fg(Color::DarkGray))
                    .alignment(Alignment::Center);
                frame.render_widget(msg, inner);
                return;
            }
            Some(rows) => {
                let row = &rows[app.triage_state.selected_index];
                let expected = format_schema_detail(
                    &row.expected_name,
                    &row.expected_type,
                    row.expected_index,
                );
                let actual =
                    format_schema_detail(&row.actual_name, &row.actual_type, row.actual_index);
                lines.push(Line::from(format!("Row ID: {}", row.id)));
                lines.push(Line::from(format!("Job ID: {}", row.job_id)));
                lines.push(Line::from(format!("Output: {}", row.output_name)));
                lines.push(Line::from(format!("Kind: {}", row.mismatch_kind)));
                lines.push(Line::from(format!("Expected: {}", expected)));
                lines.push(Line::from(format!("Actual:   {}", actual)));
                lines.push(Line::from(format!("Created: {}", row.created_at)));
            }
        },
        TriageTab::DeadLetter => match app.triage_state.dead_letters.as_ref() {
            None => {
                let msg = Paragraph::new("Table not available.")
                    .style(Style::default().fg(Color::DarkGray))
                    .alignment(Alignment::Center);
                frame.render_widget(msg, inner);
                return;
            }
            Some(rows) if rows.is_empty() => {
                let msg = Paragraph::new("No selection.")
                    .style(Style::default().fg(Color::DarkGray))
                    .alignment(Alignment::Center);
                frame.render_widget(msg, inner);
                return;
            }
            Some(rows) => {
                let row = &rows[app.triage_state.selected_index];
                lines.push(Line::from(format!("Row ID: {}", row.id)));
                lines.push(Line::from(format!("Original Job: {}", row.original_job_id)));
                lines.push(Line::from(format!(
                    "File ID: {}",
                    format_optional_i64(row.file_id)
                )));
                lines.push(Line::from(format!("Plugin: {}", row.plugin_name)));
                lines.push(Line::from(format!("Retry: {}", row.retry_count)));
                lines.push(Line::from(format!("Moved: {}", row.moved_at)));
                if let Some(reason) = &row.reason {
                    lines.push(Line::from(format!("Reason: {}", reason)));
                }
                if let Some(error) = &row.error_message {
                    lines.push(Line::from(""));
                    lines.push(Line::from(Span::styled(
                        "Error:",
                        Style::default().fg(Color::White).bold(),
                    )));
                    lines.push(Line::from(Span::styled(
                        error,
                        Style::default().fg(Color::Gray),
                    )));
                }
            }
        },
    }

    if let Some(message) = &app.triage_state.status_message {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            message,
            Style::default().fg(Color::Yellow),
        )));
    }

    let para = Paragraph::new(lines).wrap(Wrap { trim: true });
    frame.render_widget(para, inner);
}

fn draw_triage_inspector(frame: &mut Frame, app: &App, area: Rect) {
    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        "Triage",
        Style::default().fg(Color::White).bold(),
    )));
    lines.push(Line::from(Span::styled(
        format!("Tab: {}", app.triage_state.tab.label()),
        Style::default().fg(Color::Gray),
    )));
    if let Some(job_id) = app.triage_state.job_filter {
        lines.push(Line::from(Span::styled(
            format!("Filter: job {}", job_id),
            Style::default().fg(Color::Gray),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "Filter: (none)".to_string(),
            Style::default().fg(Color::Gray),
        )));
    }
    if let Some(msg) = &app.triage_state.status_message {
        lines.push(Line::from(Span::styled(
            format!("Status: {}", msg),
            Style::default().fg(Color::Yellow),
        )));
    }

    let para = Paragraph::new(lines).wrap(Wrap { trim: true });
    frame.render_widget(para, area);
}

fn raw_data_preview(raw: &Option<Vec<u8>>) -> Vec<String> {
    let Some(bytes) = raw else {
        return vec!["(empty)".to_string()];
    };
    if let Ok(text) = std::str::from_utf8(bytes) {
        let mut snippet: String = text.chars().take(400).collect();
        if text.chars().count() > 400 {
            snippet.push_str("...");
        }
        return snippet.lines().map(|line| line.to_string()).collect();
    }

    let hex = bytes
        .iter()
        .take(64)
        .map(|b| format!("{:02x}", b))
        .collect::<Vec<String>>()
        .join(" ");
    vec![format!(
        "0x{}{}",
        hex,
        if bytes.len() > 64 { " ..." } else { "" }
    )]
}

// ======== Catalog Screen (Pipelines + Runs) ========

fn draw_catalog_screen(frame: &mut Frame, app: &App, area: Rect) {
    let inspector_visible = inspector_visible_for(app, area);
    let shell = shell_layout(area, inspector_visible);

    draw_shell_top_bar(frame, app, shell.top);
    draw_shell_rail(frame, app, shell.rail);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(shell.main);

    draw_catalog_tabs(frame, app, chunks[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(chunks[1]);

    draw_catalog_list_panel(frame, app, body[0]);
    draw_catalog_detail_panel(frame, app, body[1]);

    if inspector_visible {
        let inner = draw_shell_inspector_block(frame, "Inspector", shell.inspector);
        draw_catalog_inspector(frame, app, inner);
    }

    draw_shell_action_bar(
        frame,
        &app.effective_actions(),
        Style::default().fg(Color::DarkGray),
        shell.bottom,
    );
}

fn draw_catalog_tabs(frame: &mut Frame, app: &App, area: Rect) {
    let mut spans = Vec::new();
    let tabs = [
        super::app::CatalogTab::Pipelines,
        super::app::CatalogTab::Runs,
    ];
    for (idx, tab) in tabs.iter().enumerate() {
        let is_active = app.catalog_state.tab == *tab;
        let style = if is_active {
            Style::default().fg(Color::Cyan).bold()
        } else {
            Style::default().fg(Color::DarkGray)
        };
        spans.push(Span::styled(tab.label(), style));
        if idx + 1 < tabs.len() {
            spans.push(Span::styled("  |  ", Style::default().fg(Color::DarkGray)));
        }
    }

    let line = Line::from(spans);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Pipelines Catalog ");
    let para = Paragraph::new(line).block(block);
    frame.render_widget(para, area);
}

fn draw_catalog_list_panel(frame: &mut Frame, app: &App, area: Rect) {
    let title = format!(" {} ", app.catalog_state.tab.label());
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    match app.catalog_state.tab {
        super::app::CatalogTab::Pipelines => match app.catalog_state.pipelines.as_ref() {
            None => {
                let msg = Paragraph::new("Table not available.")
                    .style(Style::default().fg(Color::DarkGray))
                    .alignment(Alignment::Center);
                frame.render_widget(msg, inner);
            }
            Some(rows) if rows.is_empty() => {
                let msg = Paragraph::new("No pipelines.")
                    .style(Style::default().fg(Color::DarkGray))
                    .alignment(Alignment::Center);
                frame.render_widget(msg, inner);
            }
            Some(rows) => {
                let items: Vec<ratatui::widgets::ListItem> = rows
                    .iter()
                    .enumerate()
                    .map(|(idx, row)| {
                        let is_selected = idx == app.catalog_state.selected_index;
                        let style = if is_selected {
                            Style::default().fg(Color::White).bold().bg(Color::DarkGray)
                        } else {
                            Style::default().fg(Color::Gray)
                        };
                        let text = format!("{} v{} ({})", row.name, row.version, row.id);
                        let line = truncate_end(&text, inner.width as usize);
                        ratatui::widgets::ListItem::new(line).style(style)
                    })
                    .collect();
                let list = ratatui::widgets::List::new(items);
                frame.render_widget(list, inner);
            }
        },
        super::app::CatalogTab::Runs => match app.catalog_state.runs.as_ref() {
            None => {
                let msg = Paragraph::new("Table not available.")
                    .style(Style::default().fg(Color::DarkGray))
                    .alignment(Alignment::Center);
                frame.render_widget(msg, inner);
            }
            Some(rows) if rows.is_empty() => {
                let msg = Paragraph::new("No runs.")
                    .style(Style::default().fg(Color::DarkGray))
                    .alignment(Alignment::Center);
                frame.render_widget(msg, inner);
            }
            Some(rows) => {
                let items: Vec<ratatui::widgets::ListItem> = rows
                    .iter()
                    .enumerate()
                    .map(|(idx, row)| {
                        let is_selected = idx == app.catalog_state.selected_index;
                        let style = if is_selected {
                            Style::default().fg(Color::White).bold().bg(Color::DarkGray)
                        } else {
                            Style::default().fg(Color::Gray)
                        };
                        let name = row
                            .pipeline_name
                            .as_deref()
                            .unwrap_or(row.pipeline_id.as_str());
                        let text = format!("{} • {} • {}", row.id, name, row.status);
                        let line = truncate_end(&text, inner.width as usize);
                        ratatui::widgets::ListItem::new(line).style(style)
                    })
                    .collect();
                let list = ratatui::widgets::List::new(items);
                frame.render_widget(list, inner);
            }
        },
    }
}

fn draw_catalog_detail_panel(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Details ")
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    match app.catalog_state.tab {
        super::app::CatalogTab::Pipelines => match app.catalog_state.pipelines.as_ref() {
            None => {
                let msg = Paragraph::new("Table not available.")
                    .style(Style::default().fg(Color::DarkGray))
                    .alignment(Alignment::Center);
                frame.render_widget(msg, inner);
                return;
            }
            Some(rows) if rows.is_empty() => {
                let msg = Paragraph::new("No selection.")
                    .style(Style::default().fg(Color::DarkGray))
                    .alignment(Alignment::Center);
                frame.render_widget(msg, inner);
                return;
            }
            Some(rows) => {
                let row = &rows[app.catalog_state.selected_index];
                lines.push(Line::from(format!("ID: {}", row.id)));
                lines.push(Line::from(format!("Name: {}", row.name)));
                lines.push(Line::from(format!("Version: {}", row.version)));
                lines.push(Line::from(format!("Created: {}", row.created_at)));
            }
        },
        super::app::CatalogTab::Runs => match app.catalog_state.runs.as_ref() {
            None => {
                let msg = Paragraph::new("Table not available.")
                    .style(Style::default().fg(Color::DarkGray))
                    .alignment(Alignment::Center);
                frame.render_widget(msg, inner);
                return;
            }
            Some(rows) if rows.is_empty() => {
                let msg = Paragraph::new("No selection.")
                    .style(Style::default().fg(Color::DarkGray))
                    .alignment(Alignment::Center);
                frame.render_widget(msg, inner);
                return;
            }
            Some(rows) => {
                let row = &rows[app.catalog_state.selected_index];
                lines.push(Line::from(format!("Run ID: {}", row.id)));
                lines.push(Line::from(format!("Pipeline: {}", row.pipeline_id)));
                if let Some(name) = &row.pipeline_name {
                    lines.push(Line::from(format!("Name: {}", name)));
                }
                if let Some(version) = row.pipeline_version {
                    lines.push(Line::from(format!("Version: {}", version)));
                }
                lines.push(Line::from(format!("Logical Date: {}", row.logical_date)));
                lines.push(Line::from(format!("Status: {}", row.status)));
                if let Some(snapshot) = &row.selection_snapshot_hash {
                    lines.push(Line::from(format!("Snapshot: {}", snapshot)));
                }
                if let Some(started) = &row.started_at {
                    lines.push(Line::from(format!("Started: {}", started)));
                }
                if let Some(done) = &row.completed_at {
                    lines.push(Line::from(format!("Completed: {}", done)));
                }
            }
        },
    }

    if let Some(message) = &app.catalog_state.status_message {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            message,
            Style::default().fg(Color::Yellow),
        )));
    }

    let para = Paragraph::new(lines).wrap(Wrap { trim: true });
    frame.render_widget(para, inner);
}

fn draw_catalog_inspector(frame: &mut Frame, app: &App, area: Rect) {
    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        "Catalog",
        Style::default().fg(Color::White).bold(),
    )));
    lines.push(Line::from(Span::styled(
        format!("Tab: {}", app.catalog_state.tab.label()),
        Style::default().fg(Color::Gray),
    )));
    if let Some(msg) = &app.catalog_state.status_message {
        lines.push(Line::from(Span::styled(
            format!("Status: {}", msg),
            Style::default().fg(Color::Yellow),
        )));
    }

    let para = Paragraph::new(lines).wrap(Wrap { trim: true });
    frame.render_widget(para, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::tui::app::{JobInfo, JobOrigin, JobType, JobsState};
    use crate::cli::tui::flow_record::RecordRedaction;
    use crate::cli::tui::TuiArgs;
    use chrono::Local;
    use ratatui::backend::TestBackend;

    fn test_args() -> TuiArgs {
        TuiArgs {
            database: Some(
                std::env::temp_dir()
                    .join(format!("casparian_test_{}.duckdb", uuid::Uuid::new_v4())),
            ),
            standalone_writer: false,
            record_flow: None,
            record_redaction: RecordRedaction::Plaintext,
            record_checkpoint_every: None,
        }
    }

    fn create_test_job(id: i64, name: &str, status: JobStatus) -> JobInfo {
        JobInfo {
            id,
            file_id: Some(id * 100),
            job_type: JobType::Parse,
            origin: JobOrigin::Persistent,
            name: name.to_string(),
            version: Some("1.0.0".to_string()),
            status,
            started_at: Local::now(),
            completed_at: None,
            pipeline_run_id: None,
            logical_date: None,
            selection_snapshot_hash: None,
            quarantine_rows: None,
            items_total: 100,
            items_processed: 50,
            items_failed: 0,
            output_path: Some(format!("/data/output/{}.parquet", name)),
            output_size_bytes: None,
            backtest: None,
            failures: vec![],
            violations: vec![],
            top_violations_loaded: false,
            selected_violation_index: 0,
        }
    }

    /// Test that UTF-8 names are rendered safely without panicking
    #[test]
    fn test_utf8_path_truncation() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Run;
        app.run_tab = RunTab::Jobs;

        // Create a job with a UTF-8 name containing multi-byte characters
        let jobs_state = JobsState {
            jobs: vec![create_test_job(
                1,
                "文件夹_数据_测试解析器",
                JobStatus::Running,
            )],
            ..Default::default()
        };
        app.jobs_state = jobs_state;

        // This should not panic - the bug was byte-based truncation on UTF-8
        let result = terminal.draw(|f| draw(f, &app));
        assert!(result.is_ok(), "Rendering UTF-8 name should not panic");
    }

    /// Test with emoji in name (4-byte UTF-8 characters)
    #[test]
    fn test_emoji_path_truncation() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Run;
        app.run_tab = RunTab::Jobs;

        // Emoji are 4-byte UTF-8 sequences
        let jobs_state = JobsState {
            jobs: vec![create_test_job(
                1,
                "📁_parser_📊_reports_📈",
                JobStatus::Pending,
            )],
            ..Default::default()
        };
        app.jobs_state = jobs_state;

        let result = terminal.draw(|f| draw(f, &app));
        assert!(result.is_ok(), "Rendering emoji name should not panic");
    }

    #[test]
    fn test_draw_home_view() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let app = App::new(test_args(), None);

        terminal.draw(|f| draw(f, &app)).unwrap();

        // Check that home hub elements are rendered
        let buffer = terminal.backend().buffer();
        let content: String = buffer
            .content
            .iter()
            .map(|cell| cell.symbol().chars().next().unwrap_or(' '))
            .collect();

        // Home hub: Quick Start (Sources) + Readiness panel
        assert!(content.contains("Quick Start: Scan a Source"));
        assert!(content.contains("Readiness"));
    }

    #[test]
    fn test_draw_discover_screen() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut app = App::new(test_args(), None);
        app.enter_discover_mode();

        // Add some mock files
        app.discover.files.push(crate::cli::tui::app::FileInfo {
            file_id: 1,
            path: "test_file.csv".into(),
            rel_path: "test_file.csv".into(),
            size: 1024,
            modified: Local::now(),
            is_dir: false,
            tags: vec![],
        });

        terminal.draw(|f| draw(f, &app)).unwrap();

        // Check that key elements are rendered
        let buffer = terminal.backend().buffer();
        let content: String = buffer
            .content
            .iter()
            .map(|cell| cell.symbol().chars().next().unwrap_or(' '))
            .collect();

        assert!(content.contains("Scope"));
    }

}
