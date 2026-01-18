//! UI rendering for the TUI

use ratatui::{
    prelude::*,
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
};

use crate::cli::config::{active_db_path, default_db_backend, DbBackend};
use crate::cli::output::format_number;
use ratatui::widgets::Clear;
use chrono::{DateTime, Local};
use super::app::{App, DiscoverFocus, DiscoverViewState, JobInfo, JobStatus, JobType, JobsListSection, JobsViewState, ParserHealth, ThroughputSample, TuiMode};

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
pub fn render_centered_dialog(frame: &mut Frame, area: Rect, max_width: u16, max_height: u16) -> Rect {
    let dialog_area = centered_dialog_area(area, max_width, max_height);
    frame.render_widget(Clear, dialog_area);
    dialog_area
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
        let mut display_map: std::collections::HashMap<String, Vec<usize>> = std::collections::HashMap::new();
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

    let main_area = area;

    // Draw Main Content
    match app.mode {
        TuiMode::Home => draw_home_screen(frame, app, main_area),
        TuiMode::Discover => draw_discover_screen(frame, app, main_area),
        TuiMode::Jobs => draw_jobs_screen(frame, app, main_area),
        TuiMode::Sources => draw_sources_screen(frame, app, main_area),
        TuiMode::ParserBench => draw_parser_bench_screen(frame, app, main_area),
        TuiMode::Settings => draw_settings_screen(frame, app, main_area),
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

    // Draw Jobs Drawer Overlay (toggle with J key)
    if app.jobs_drawer_open {
        draw_jobs_drawer(frame, app, area);
    }

    // Draw Sources Drawer Overlay (toggle with S key)
    if app.sources_drawer_open {
        draw_sources_drawer(frame, app, area);
    }
}

/// Draw the Jobs drawer overlay (global, toggles with J)
fn draw_jobs_drawer(frame: &mut Frame, app: &App, area: Rect) {
    // Drawer takes right 40% of screen, or at least 50 chars
    let drawer_width = (area.width * 40 / 100).max(50).min(area.width);
    let drawer_area = Rect::new(
        area.width.saturating_sub(drawer_width),
        0,
        drawer_width,
        area.height,
    );

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
            Constraint::Length(2),  // Status counts
            Constraint::Min(0),     // Job list
            Constraint::Length(2),  // Footer hints
        ])
        .split(inner);

    // Status summary line
    let running = app.jobs_state.jobs.iter().filter(|j| j.status == super::app::JobStatus::Running).count();
    let pending = app.jobs_state.jobs.iter().filter(|j| j.status == super::app::JobStatus::Pending).count();
    let failed = app.jobs_state.jobs.iter().filter(|j| j.status == super::app::JobStatus::Failed).count();
    let completed = app.jobs_state.jobs.iter().filter(|j| j.status == super::app::JobStatus::Completed).count();

    let status_line = ratatui::text::Line::from(vec![
        ratatui::text::Span::styled(format!("Running {} ", running), Style::default().fg(Color::Yellow)),
        ratatui::text::Span::styled(format!("Pending {} ", pending), Style::default().fg(Color::DarkGray)),
        ratatui::text::Span::styled(format!("Failed {} ", failed), Style::default().fg(Color::Red)),
        ratatui::text::Span::styled(format!("Done {}", completed), Style::default().fg(Color::Green)),
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
fn draw_sources_drawer(frame: &mut Frame, app: &App, area: Rect) {
    // Drawer takes right 40% of screen, or at least 50 chars
    let drawer_width = (area.width * 40 / 100).max(50).min(area.width);
    let drawer_area = Rect::new(
        area.width.saturating_sub(drawer_width),
        0,
        drawer_width,
        area.height,
    );

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

    let header = Paragraph::new("Recent sources (MRU, max 5)")
        .style(Style::default().fg(Color::DarkGray));
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

    let footer = Paragraph::new("[s] Scan  [e] Edit  [d] Delete  [n] New  [Enter] Open  [S/Esc] Close")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    frame.render_widget(footer, chunks[2]);
}

/// Draw the home hub screen (Quick Start + Status dashboard)
fn draw_home_screen(frame: &mut Frame, app: &App, area: Rect) {
    // Layout: title bar, main content (two panels), footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Title bar
            Constraint::Min(0),     // Main content
            Constraint::Length(3),  // Footer/status
        ])
        .split(area);

    // Title bar
    let title = Paragraph::new(" Casparian Flow - Home ")
        .style(Style::default().fg(Color::Cyan).bold())
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(title, chunks[0]);

    // Split main content into two columns: Sources (left) and Activity (right)
    let main_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    // Left panel: Quick Start - Scan a Source
    draw_home_sources_panel(frame, app, main_cols[0]);

    // Right panel: Recent Activity
    draw_home_activity_panel(frame, app, main_cols[1]);

    // Footer with shortcuts (updated for new keybindings)
    let footer = Paragraph::new(" [↑↓] Navigate  [Enter] Scan  [/] Filter  [s] New Scan  [1] Discover  [2] Parser Bench  [3] Jobs  [4] Sources  [J] Jobs Drawer  [S] Sources Drawer  [?] Help ")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::TOP));
    frame.render_widget(footer, chunks[2]);
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
    let filtered_sources: Vec<_> = app.discover.sources.iter()
        .enumerate()
        .filter(|(_, source)| {
            filter_lower.is_empty()
                || source.name.to_lowercase().contains(&filter_lower)
                || source.path.display().to_string().to_lowercase().contains(&filter_lower)
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

    let list = ratatui::widgets::List::new(items)
        .highlight_style(Style::default().fg(Color::Cyan));
    frame.render_widget(list, inner);
}

/// Draw the Activity panel (right side of Home)
fn draw_home_activity_panel(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Recent Activity ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Show recent jobs summary
    let mut lines = vec![];

    // Stats line
    lines.push(ratatui::text::Line::from(format!(
        "{} running, {} pending, {} failed",
        app.home.stats.running_jobs,
        app.home.stats.pending_jobs,
        app.home.stats.failed_jobs
    )));
    lines.push(ratatui::text::Line::from(""));

    // Recent jobs
    for job in app.home.recent_jobs.iter().take(5) {
        let status_style = match job.status {
            super::app::JobStatus::Running => Style::default().fg(Color::Yellow),
            super::app::JobStatus::Completed => Style::default().fg(Color::Green),
            super::app::JobStatus::Failed => Style::default().fg(Color::Red),
            super::app::JobStatus::Pending => Style::default().fg(Color::DarkGray),
            super::app::JobStatus::Cancelled => Style::default().fg(Color::Magenta),
        };
        let status_text = match job.status {
            super::app::JobStatus::Running => {
                if let Some(pct) = job.progress_percent {
                    format!("{}%", pct)
                } else {
                    "running".to_string()
                }
            }
            super::app::JobStatus::Completed => "done".to_string(),
            super::app::JobStatus::Failed => "failed".to_string(),
            super::app::JobStatus::Pending => "pending".to_string(),
            super::app::JobStatus::Cancelled => "cancelled".to_string(),
        };
        lines.push(ratatui::text::Line::from(vec![
            ratatui::text::Span::raw(format!("[{}] {} ", job.job_type, job.description)),
            ratatui::text::Span::styled(status_text, status_style),
        ]));
    }

    if app.home.recent_jobs.is_empty() {
        lines.push(ratatui::text::Line::from("No recent jobs."));
    }

    // Last error
    if let Some(ref err) = app.home.last_error {
        lines.push(ratatui::text::Line::from(""));
        lines.push(ratatui::text::Line::styled(
            format!("Last error: {}", err),
            Style::default().fg(Color::Red),
        ));
    }

    // Tips
    lines.push(ratatui::text::Line::from(""));
    lines.push(ratatui::text::Line::styled(
        "Tip: Press [J] to open Jobs drawer",
        Style::default().fg(Color::DarkGray).italic(),
    ));

    let para = Paragraph::new(lines).wrap(Wrap { trim: true });
    frame.render_widget(para, inner);
}

/// Draw the Sources view screen (key 4)
fn draw_sources_screen(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Title bar
            Constraint::Min(0),     // Sources list
            Constraint::Length(3),  // Footer
        ])
        .split(area);

    // Title bar
    let title = Paragraph::new(" Sources Manager ")
        .style(Style::default().fg(Color::Cyan).bold())
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(title, chunks[0]);

    // Sources list
    let block = Block::default()
        .title(" Configured Sources ")
        .borders(Borders::ALL);
    let inner = block.inner(chunks[1]);
    frame.render_widget(block, chunks[1]);

    if app.discover.sources.is_empty() {
        let msg = Paragraph::new("No sources configured.\n\nPress [n] to add a source or [1] Discover then [s] to scan.")
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
                    Style::default().fg(Color::Cyan).bold()
                } else {
                    Style::default()
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

    // Footer
    let footer = Paragraph::new(" [↑↓] Navigate  [n] New  [e] Edit  [r] Rescan  [d] Delete  [Esc] Back  [0] Home  [?] Help ")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::TOP));
    frame.render_widget(footer, chunks[2]);
}

/// Draw the Discover mode screen (File Explorer)
fn draw_discover_screen(frame: &mut Frame, app: &App, area: Rect) {
    #[cfg(feature = "profiling")]
    let _zone = app.profiler.zone("tui.discover");

    // Rule Builder is the ONLY view in Discover mode (replaces old GlobExplorer)
    // Always render Rule Builder as the base, then overlay dropdowns/dialogs
    draw_rule_builder_screen(frame, app, area);

    // Render dropdown/dialog overlays on top of Rule Builder
    match app.discover.view_state {
        DiscoverViewState::SourcesDropdown => draw_sources_dropdown(frame, app, area),
        DiscoverViewState::TagsDropdown => draw_tags_dropdown(frame, app, area),
        DiscoverViewState::RulesManager => draw_rules_manager_dialog(frame, app, area),
        DiscoverViewState::RuleCreation => draw_rule_creation_dialog(frame, app, area),
        DiscoverViewState::SourcesManager => draw_sources_manager_dialog(frame, app, area),
        DiscoverViewState::SourceEdit => draw_source_edit_dialog(frame, app, area),
        DiscoverViewState::SourceDeleteConfirm => draw_source_delete_confirm_dialog(frame, app, area),
        DiscoverViewState::Scanning => draw_scanning_dialog(frame, app, area),
        DiscoverViewState::EnteringPath => draw_add_source_dialog(frame, app, area),
        _ => {}
    }
}

/// Draw the Sources dropdown as a proper overlay dialog
fn draw_sources_dropdown(frame: &mut Frame, app: &App, area: Rect) {
    // Calculate dialog size - takes up left 40% of screen, respecting margins
    let width = (area.width * 40 / 100).max(30).min(area.width - 4);
    let height = area.height.saturating_sub(8).max(10);

    // Position at left side of screen
    let dialog_area = Rect {
        x: area.x + 2,
        y: area.y + 3,
        width,
        height,
    };

    // Clear the area first (proper overlay)
    frame.render_widget(Clear, dialog_area);

    // Expanded dropdown with optional filter line
    let is_filtering = app.discover.sources_filtering;

    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .margin(1)
        .split(dialog_area);

    // Top line: filter input OR hint
    if is_filtering {
        let filter_text = format!("/{}", app.discover.sources_filter);
        let filter_line = Paragraph::new(vec![
            Line::from(vec![
                Span::styled(filter_text, Style::default().fg(Color::Yellow)),
                Span::styled("█", Style::default().fg(Color::Yellow)),
            ])
        ]);
        frame.render_widget(filter_line, inner_chunks[0]);
    } else if !app.discover.sources_filter.is_empty() {
        // Show active filter (but not in filter mode)
        let hint = format!("/{} (Enter:clear)", app.discover.sources_filter);
        let hint_line = Paragraph::new(hint)
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(hint_line, inner_chunks[0]);
    } else {
        // Show keybinding hints
        let hint_line = Paragraph::new("[/]filter [s]scan [↑/↓]nav [Enter]select [Esc]close")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(hint_line, inner_chunks[0]);
    }

    // Filtered list
    let filtered: Vec<_> = app.discover.sources.iter()
        .enumerate()
        .filter(|(_, s)| {
            app.discover.sources_filter.is_empty()
                || s.name.to_lowercase().contains(&app.discover.sources_filter.to_lowercase())
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
        let preview_idx = app.discover.preview_source.unwrap_or_else(|| app.discover.selected_source_index());
        let preview_pos = filtered.iter().position(|(i, _)| *i == preview_idx).unwrap_or(0);
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

    // Border with title
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Span::styled(" [1] Select Source ", Style::default().fg(Color::Cyan).bold()))
        .title_alignment(Alignment::Left);
    frame.render_widget(block, dialog_area);
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
        // Calculate dialog size - match Sources dropdown sizing
        let width = (area.width * 40 / 100).max(30).min(area.width - 4);
        let height = area.height.saturating_sub(8).max(10);

        // Position at right side of screen
        let dialog_area = Rect {
            x: area.x + area.width.saturating_sub(width + 2),
            y: area.y + 3,
            width,
            height,
        };

        // Clear the area first (proper overlay)
        frame.render_widget(Clear, dialog_area);

        // Expanded dropdown with optional filter line
        let is_filtering = app.discover.tags_filtering;

        let inner_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .margin(1)
            .split(dialog_area);

        // Top line: filter input OR hint
        if is_filtering {
            let filter_text = format!("/{}", app.discover.tags_filter);
            let filter_line = Paragraph::new(vec![
                Line::from(vec![
                    Span::styled(filter_text, Style::default().fg(Color::Yellow)),
                    Span::styled("█", Style::default().fg(Color::Yellow)),
                ])
            ]);
            frame.render_widget(filter_line, inner_chunks[0]);
        } else if !app.discover.tags_filter.is_empty() {
            // Show active filter (but not in filter mode)
            let hint = format!("/{} (Enter:clear)", app.discover.tags_filter);
            let hint_line = Paragraph::new(hint)
                .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(hint_line, inner_chunks[0]);
        } else {
            // Show keybinding hints (same as Sources dropdown for consistency)
            let hint_line = Paragraph::new("[/]filter [↑/↓]nav [Enter]select [Esc]close")
                .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(hint_line, inner_chunks[0]);
        }

        // Filtered list
        let filtered: Vec<_> = app.discover.tags.iter()
            .enumerate()
            .filter(|(_, t)| {
                app.discover.tags_filter.is_empty()
                    || t.name.to_lowercase().contains(&app.discover.tags_filter.to_lowercase())
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
                let preview_pos = filtered.iter().position(|(i, _)| *i == preview_idx).unwrap_or(0);
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

        // Border with title - always use double borders when open (like Sources dropdown)
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .border_type(BorderType::Double)
            .title(Span::styled(" [2] Select Tag ", Style::default().fg(Color::Cyan).bold()));
        frame.render_widget(block, dialog_area);
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
            .title(Span::styled(" [2] Tags ", style.bold()));

        let content = Paragraph::new(selected_text)
            .style(if is_focused { Style::default().fg(Color::Cyan) } else { Style::default().fg(Color::Gray) })
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
        .title(Span::styled(" Tagging Rules ", Style::default().fg(Color::Cyan).bold()));

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
        .title(Span::styled(" Sources Manager ", Style::default().fg(Color::Cyan).bold()));

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
                format!("...{}", &path_str[path_str.len()-19..])
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
        .title(Span::styled(" Edit Source ", Style::default().fg(Color::Yellow).bold()));

    let inner_area = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    // Layout: name field + path (read-only) + footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Name field
            Constraint::Length(1),  // Path (read-only)
            Constraint::Length(1),  // Footer
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
        app.discover.sources.iter()
            .find(|s| &s.id == source_id)
            .map(|s| format!("  Path: {} (read-only)", s.path.to_string_lossy()))
            .unwrap_or_default()
    } else {
        String::new()
    };
    let path_para = Paragraph::new(path_text)
        .style(Style::default().fg(Color::DarkGray));
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
        .title(Span::styled(" Delete Source ", Style::default().fg(Color::Red).bold()));

    let inner_area = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    // Get source info for display
    let (source_name, file_count) = app.discover.source_to_delete.as_ref()
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
            format!("  This will remove the source and all {} tracked", file_count),
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
    let dialog_height = 9
        + if has_suggestions { suggestions_count as u16 + 2 } else { 0 }
        + if has_error { 2 } else { 0 };

    let dialog_area = render_centered_dialog(frame, area, 70, dialog_height);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Span::styled(" Add Source ", Style::default().fg(Color::Cyan).bold()));

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

/// Draw the Scanning progress dialog as an overlay
fn draw_scanning_dialog(frame: &mut Frame, app: &App, area: Rect) {
    let dialog_area = render_centered_dialog(frame, area, 60, 9);

    // Animated spinner for title
    let spinner = spinner_ascii(app.tick_count);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(Span::styled(
            format!(" [{}] Scanning ", spinner),
            Style::default().fg(Color::Yellow).bold()
        ));

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    // Get progress - IMPORTANT: scan_progress contains the REAL values
    let progress = app.discover.scan_progress.as_ref();
    let files_found = progress.map(|p| p.files_found).unwrap_or(0);
    let files_persisted = progress.map(|p| p.files_persisted).unwrap_or(0);
    let dirs = progress.map(|p| p.dirs_scanned).unwrap_or(0);
    let current = progress.and_then(|p| p.current_dir.clone());

    // Elapsed time from scan_start_time
    let secs = app.discover.scan_start_time
        .map(|t| t.elapsed().as_secs())
        .unwrap_or(0);
    let time_str = if secs >= 60 {
        format!("{}m {:02}s", secs / 60, secs % 60)
    } else {
        format!("{}s", secs)
    };

    // Path being scanned
    let path = app.discover.scanning_path.as_deref().unwrap_or("...");
    let w = inner.width as usize;
    let path_display = if path.len() > w.saturating_sub(10) {
        let take = w.saturating_sub(13);
        format!("...{}", &path[path.len().saturating_sub(take)..])
    } else {
        path.to_string()
    };

    // Current directory hint
    let hint = match &current {
        Some(d) if d != "Initializing..." => {
            if d.len() > w.saturating_sub(6) {
                let take = w.saturating_sub(9);
                format!("...{}", &d[d.len().saturating_sub(take)..])
            } else {
                d.clone()
            }
        }
        _ => {
            let dots = ["   ", ".  ", ".. ", "..."][(app.tick_count / 4) as usize % 4];
            format!("scanning{}", dots)
        }
    };

    // Render each line explicitly - using raw strings to fill width
    let pad = |s: &str| format!("{:<width$}", s, width = w);

    // Line 0: Path
    let line0 = format!("  Path: {}", path_display);
    // Line 1: empty
    // Line 2: Stats (THE MAIN PROGRESS LINE)
    // Show crawled/persisted to diagnose bottlenecks
    let line2 = format!("  {}/{} files | {} dirs | {}", files_found, files_persisted, dirs, time_str);
    // Line 3: empty
    // Line 4: Current directory hint
    let line4 = format!("  {}", hint);
    // Line 5: empty
    // Line 6: Navigation
    let line6 = "  [Esc] Cancel  [0] Home  [4] Jobs";

    let text = vec![
        Line::styled(pad(&line0), Style::default().fg(Color::White)),
        Line::raw(pad("")),
        Line::styled(pad(&line2), Style::default().fg(Color::Green).add_modifier(ratatui::style::Modifier::BOLD)),
        Line::raw(pad("")),
        Line::styled(pad(&line4), Style::default().fg(Color::DarkGray)),
        Line::raw(pad("")),
        Line::styled(pad(line6), Style::default().fg(Color::DarkGray)),
    ];

    frame.render_widget(Paragraph::new(text), inner);
}

/// Draw the Rule Builder as a full-screen view (replaces Discover, not overlay)
/// Layout: Header | Split View (40% left / 60% right) | Footer
fn draw_rule_builder_screen(frame: &mut Frame, app: &App, area: Rect) {
    let builder = match &app.discover.rule_builder {
        Some(b) => b,
        None => return,
    };

    // Main layout: Header (3) | Content (flex) | Footer (3)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Content (split view)
            Constraint::Length(3), // Footer
        ])
        .split(area);

    // === HEADER ===
    // Format: " Rule Builder - [1] Source: X ▾  [2] Tags: All ▾  [R] Rules ▾  | 247 files match | Scan: 12345 files "
    let source_name = app.discover.selected_source()
        .map(|s| s.name.clone())
        .unwrap_or_else(|| "Select source".to_string());
    let match_count = builder.match_count;

    // Scan snapshot info - show source file count if cache is loaded
    let scan_info = if let Some(ref explorer) = app.discover.glob_explorer {
        if explorer.cache_loaded {
            // Get file count from selected source
            let file_count = app.discover.selected_source()
                .map(|s| s.file_count)
                .unwrap_or(0);
            if file_count > 0 {
                format!(" | Scan: {} files", file_count)
            } else {
                " | Scan: empty".to_string()
            }
        } else if app.discover.scan_error.is_some() {
            " | No scan [s]".to_string()
        } else {
            " | Loading...".to_string()
        }
    } else {
        " | No scan [s]".to_string()
    };

    let error_hint = app
        .discover
        .status_message
        .as_ref()
        .and_then(|(msg, is_error)| if *is_error { Some(msg.as_str()) } else { None });
    let edit_hint = if builder.editing_rule_id.is_some() { "  Editing" } else { "" };
    let dirty_hint = if builder.dirty { " *" } else { "" };
    let header_text = if let Some(msg) = error_hint {
        format!(
            " Rule Builder - [1] Source: {} ▾  [2] Tags: All ▾  [R] Rules ▾  │ {} files match{}  ⚠ {}{}{}",
            source_name, match_count, scan_info, msg, edit_hint, dirty_hint
        )
    } else {
        format!(
            " Rule Builder - [1] Source: {} ▾  [2] Tags: All ▾  [R] Rules ▾  │ {} files match{}{}{}",
            source_name, match_count, scan_info, edit_hint, dirty_hint
        )
    };
    let header = Paragraph::new(header_text)
        .style(Style::default().fg(Color::Green).bold())
        .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(header, chunks[0]);

    // === CONTENT: Split View ===
    let content_area = chunks[1];

    // Split into left (40%) and right (60%) panels
    let h_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(content_area);

    // Left panel: Rule config sections
    draw_rule_builder_left_panel(frame, builder, h_chunks[0]);

    // Right panel: File results
    draw_rule_builder_right_panel(frame, builder, h_chunks[1], app.discover.scan_error.as_deref());

    // === FOOTER ===
    // Priority: status_message > cache loading progress > keybindings
    let (footer_text, footer_style) = if let Some((ref msg, is_error)) = app.discover.status_message {
        (format!(" {} ", msg), if is_error { Style::default().fg(Color::Red) } else { Style::default().fg(Color::Green) })
    } else if let Some(ref progress) = app.cache_load_progress {
        // Show spinner with source name and elapsed time
        let spinner = spinner_char(app.tick_count);
        let status = progress.status_line();
        (format!(" {} {} ", spinner, status), Style::default().fg(Color::Yellow))
    } else {
        {
            use super::extraction::RuleBuilderFocus;
            let scan_hint = if matches!(builder.focus, RuleBuilderFocus::Pattern | RuleBuilderFocus::Tag | RuleBuilderFocus::ExcludeInput | RuleBuilderFocus::ExtractionEdit(_)) {
                "[Esc] Exit input then [s] Scan"
            } else {
                "[s] Scan"
            };
            (
                format!(" [e] Sample  [E] Full  [t] Apply Tag  [b] Backtest  {}  [Tab] Nav  [Ctrl+S] Save  [Ctrl+N] Clear  [Esc] Back  [0] Home  [3] Files  [J] Jobs  [4] Sources  [?] Help ", scan_hint),
                Style::default().fg(Color::DarkGray),
            )
        }
    };
    let footer = Paragraph::new(footer_text)
        .style(footer_style)
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::TOP));
    frame.render_widget(footer, chunks[2]);

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

/// Draw the left panel of Rule Builder (PATTERN, EXCLUDES, TAG, EXTRACTIONS, OPTIONS)
fn draw_rule_builder_left_panel(frame: &mut Frame, builder: &super::extraction::RuleBuilderState, area: Rect) {
    use super::extraction::RuleBuilderFocus;

    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Pattern
            Constraint::Length(4),  // Excludes
            Constraint::Length(3),  // Tag
            Constraint::Length(5),  // Extractions
            Constraint::Length(3),  // Options
            Constraint::Min(8),     // Schema Suggestions (new)
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
        .border_type(if pattern_focused { BorderType::Double } else { BorderType::Plain })
        .border_style(if builder.pattern_error.is_some() {
            Style::default().fg(Color::Red)
        } else if pattern_focused {
            Style::default().fg(Color::Cyan).bold()
        } else {
            pattern_style
        })
        .title(Span::styled(pattern_title, if pattern_focused { Style::default().fg(Color::Cyan).bold() } else { pattern_style }));

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
    let excludes_focused = matches!(builder.focus, RuleBuilderFocus::Excludes | RuleBuilderFocus::ExcludeInput);
    let excludes_style = if excludes_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let excludes_title = if excludes_focused {
        format!(" EXCLUDES ({}) [Enter: add, d: delete] ", builder.excludes.len())
    } else {
        format!(" EXCLUDES ({}) ", builder.excludes.len())
    };
    let excludes_block = Block::default()
        .borders(Borders::ALL)
        .border_type(if excludes_focused { BorderType::Double } else { BorderType::Plain })
        .border_style(if excludes_focused { Style::default().fg(Color::Cyan).bold() } else { excludes_style })
        .title(Span::styled(excludes_title, if excludes_focused { Style::default().fg(Color::Cyan).bold() } else { excludes_style }));

    let excludes_content = if matches!(builder.focus, RuleBuilderFocus::ExcludeInput) {
        format!("{}█", builder.exclude_input)
    } else if builder.excludes.is_empty() {
        "Press [Enter] to add exclude pattern".to_string()
    } else {
        builder.excludes.iter().enumerate().map(|(i, e)| {
            if i == builder.selected_exclude && excludes_focused {
                format!("► {}", e)
            } else {
                format!("  {}", e)
            }
        }).collect::<Vec<_>>().join("\n")
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

    let tag_title = if tag_focused { " TAG (editing) " } else { " TAG " };
    let tag_block = Block::default()
        .borders(Borders::ALL)
        .border_type(if tag_focused { BorderType::Double } else { BorderType::Plain })
        .border_style(if tag_focused { Style::default().fg(Color::Cyan).bold() } else { tag_style })
        .title(Span::styled(tag_title, if tag_focused { Style::default().fg(Color::Cyan).bold() } else { tag_style }));

    let tag_text = if tag_focused {
        if builder.tag.is_empty() {
            "█".to_string()  // Show cursor in empty field
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
    let extractions_focused = matches!(builder.focus, RuleBuilderFocus::Extractions | RuleBuilderFocus::ExtractionEdit(_));
    let extractions_style = if extractions_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let extractions_title = if extractions_focused {
        format!(" EXTRACTIONS ({}) [Space: toggle] ", builder.extractions.len())
    } else {
        format!(" EXTRACTIONS ({}) ", builder.extractions.len())
    };
    let extractions_block = Block::default()
        .borders(Borders::ALL)
        .border_type(if extractions_focused { BorderType::Double } else { BorderType::Plain })
        .border_style(if extractions_focused { Style::default().fg(Color::Cyan).bold() } else { extractions_style })
        .title(Span::styled(extractions_title, if extractions_focused { Style::default().fg(Color::Cyan).bold() } else { extractions_style }));

    let extractions_content = if builder.extractions.is_empty() {
        "Add <name> to pattern, e.g. **/<year>/*.csv".to_string()
    } else {
        builder.extractions.iter().enumerate().map(|(i, f)| {
            let selected = i == builder.selected_extraction && extractions_focused;
            let prefix = if selected { "► " } else { "  " };
            let enabled = if f.enabled { "✓" } else { " " };
            format!("{}{} {} {}", prefix, f.name, f.source.display_name(), enabled)
        }).take(3).collect::<Vec<_>>().join("\n")
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

    let options_title = if options_focused { " OPTIONS [Space: toggle] " } else { " OPTIONS " };
    let options_block = Block::default()
        .borders(Borders::ALL)
        .border_type(if options_focused { BorderType::Double } else { BorderType::Plain })
        .border_style(if options_focused { Style::default().fg(Color::Cyan).bold() } else { options_style })
        .title(Span::styled(options_title, if options_focused { Style::default().fg(Color::Cyan).bold() } else { options_style }));

    let enabled_check = if builder.enabled { "[x]" } else { "[ ]" };
    let job_check = if builder.run_job_on_save { "[x]" } else { "[ ]" };
    let options_para = Paragraph::new(format!("{} Enable rule  {} Run job on save", enabled_check, job_check))
        .style(if options_focused { Style::default().fg(Color::White) } else { Style::default().fg(Color::Gray) })
        .block(options_block);
    frame.render_widget(options_para, left_chunks[4]);

    // --- SCHEMA SUGGESTIONS (new per RULE_BUILDER_UI_PLAN.md) ---
    draw_schema_suggestions(frame, builder, left_chunks[5]);
}

/// Draw schema suggestions panel (RULE_BUILDER_UI_PLAN.md)
/// Shows pattern seeds, path archetypes, naming schemes, and synonym suggestions.
fn draw_schema_suggestions(frame: &mut Frame, builder: &super::extraction::RuleBuilderState, area: Rect) {
    use super::extraction::RuleBuilderFocus;
    use super::extraction::SuggestionSection;
    use super::extraction::SynonymConfidence;

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(" SUGGESTIONS ", Style::default().fg(Color::DarkGray)));

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
        spans.push(Span::styled(label.to_string(), Style::default().fg(Color::Yellow).bold()));
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
                Span::styled(format!(" ({})", seed.match_count), Style::default().fg(Color::DarkGray)),
            ]));
        }
    }

    // Path Archetypes (show top 2)
    if !builder.path_archetypes.is_empty() && lines.len() < inner.height as usize - 2 {
        if !lines.is_empty() { lines.push(Line::from("")); }
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

        for (idx, (arch, template)) in archetypes.into_iter().zip(formatted_paths.into_iter()).enumerate() {
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
                Span::styled(format!(" ({} files)", arch.file_count), Style::default().fg(Color::DarkGray)),
            ]));
        }
    }

    // Naming Schemes (show top 2)
    if !builder.naming_schemes.is_empty() && lines.len() < inner.height as usize - 2 {
        if !lines.is_empty() { lines.push(Line::from("")); }
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
                Span::styled(format!(" ({})", scheme.file_count), Style::default().fg(Color::DarkGray)),
            ]));
        }
    }

    // Synonym Suggestions (show top 2)
    if !builder.synonym_suggestions.is_empty() && lines.len() < inner.height as usize - 2 {
        if !lines.is_empty() { lines.push(Line::from("")); }
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

    let para = Paragraph::new(lines)
        .style(Style::default());
    frame.render_widget(para, inner);
}

/// Draw the right panel of Rule Builder (file results list)
fn draw_rule_builder_right_panel(frame: &mut Frame, builder: &super::extraction::RuleBuilderState, area: Rect, scan_error: Option<&str>) {
    use super::extraction::{RuleBuilderFocus, FileResultsState};

    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),  // Header with filter
            Constraint::Min(1),     // File list
            Constraint::Length(2),  // Status bar
        ])
        .split(area);

    let file_list_focused = matches!(builder.focus, RuleBuilderFocus::FileList);

    // Phase-specific header
    let header_text = match &builder.file_results {
        FileResultsState::Exploration { folder_matches, .. } => {
            let folder_count = folder_matches.len();
            let file_count: usize = folder_matches.iter().map(|f| f.count).sum();
            if builder.is_streaming {
                format!(" FOLDERS  {} folders ({} files)  {}", folder_count, file_count, spinner_char(builder.stream_elapsed_ms / 100))
            } else {
                format!(" FOLDERS  {} folders ({} files)  [Enter] drill down", folder_count, file_count)
            }
        }
        FileResultsState::ExtractionPreview { preview_files } => {
            let file_count = preview_files.len();
            let warning_count: usize = preview_files.iter().map(|f| f.warnings.len()).sum();
            if warning_count > 0 {
                format!(" PREVIEW  {} files  ⚠ {} warnings  t:tag  b:backtest", file_count, warning_count)
            } else {
                format!(" PREVIEW  {} files  ✓ extractions OK  t:tag  b:backtest", file_count)
            }
        }
        FileResultsState::BacktestResults { result_filter, .. } => {
            let filter_label = result_filter.label();
            format!(" RESULTS [{}]  a/p/f to filter", filter_label)
        }
    };

    let header = Paragraph::new(header_text)
        .style(Style::default().fg(if file_list_focused {
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
            draw_exploration_phase(frame, builder, file_list_inner, file_list_focused, scan_error);
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
                    format!(" {} folders  {} files matched", folder_matches.len(), total_files)
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
                let ok_count = preview_files.iter().filter(|f| f.warnings.is_empty()).count();
                let selected_count = builder.selected_preview_files.len();
                format!(
                    " {} files  {} OK  {} warnings  Selected: {}  [t] apply tag  [b] backtest",
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
                backtest.pass_count,
                backtest.fail_count,
                backtest.excluded_count,
                selected_count
            )
        }
    };
    let status_para = Paragraph::new(status)
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(status_para, right_chunks[2]);
}

/// Phase 1: Exploration - folder counts + sample filenames
fn draw_exploration_phase(frame: &mut Frame, builder: &super::extraction::RuleBuilderState, area: Rect, focused: bool, scan_error: Option<&str>) {
    let (folder_matches, expanded_folder_indices) = match &builder.file_results {
        super::extraction::FileResultsState::Exploration { folder_matches, expanded_folder_indices, .. } => {
            (folder_matches, expanded_folder_indices)
        }
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
        let start = centered_scroll_offset(builder.selected_file, max_display, folder_matches.len());

        // Calculate column widths for alignment
        // Use width proportions: 60% folder path, 15% count, 25% sample filename
        let available_width = area.width.saturating_sub(6) as usize; // Reserve space for prefix/icon
        let path_width = (available_width * 50 / 100).min(35);
        let count_width = 8;

        folder_matches.iter()
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
                    }
                ));

                // Folder path (left-aligned, padded)
                spans.push(Span::styled(
                    format!("{:<width$}", format!("{}/", truncated_path), width = path_width + 1),
                    if is_selected {
                        Style::default().fg(Color::White).bold()
                    } else {
                        Style::default().fg(Color::Gray)
                    }
                ));

                // Count (right-aligned)
                spans.push(Span::styled(
                    format!("{:>width$}", count_str, width = count_width),
                    if is_selected {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    }
                ));

                // Sample filename
                spans.push(Span::styled(
                    format!("  {}", folder.sample_filename),
                    if is_selected {
                        Style::default().fg(Color::White).italic()
                    } else {
                        Style::default().fg(Color::DarkGray)
                    }
                ));

                Line::from(spans)
            })
            .collect()
    };

    let file_list = Paragraph::new(lines);
    frame.render_widget(file_list, area);
}

/// Phase 2: Extraction Preview - per-file with extracted values
fn draw_extraction_preview_phase(frame: &mut Frame, builder: &super::extraction::RuleBuilderState, area: Rect, focused: bool) {
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

        preview_files.iter()
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
                let extractions_str: String = file.extractions.iter()
                    .take(3)  // Limit to 3 extractions for display
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect::<Vec<_>>()
                    .join(", ");

                let display = format!(
                    "{}{}{} {}  [{}]",
                    select_mark,
                    prefix,
                    status_icon,
                    file.relative_path,
                    extractions_str
                );

                Line::from(Span::styled(display, style))
            })
            .collect()
    };

    let file_list = Paragraph::new(lines);
    frame.render_widget(file_list, area);
}

/// Phase 3: Backtest Results - per-file pass/fail with errors
fn draw_backtest_results_phase(frame: &mut Frame, builder: &super::extraction::RuleBuilderState, area: Rect, focused: bool) {
    let (matched_files, visible_indices) = match &builder.file_results {
        super::extraction::FileResultsState::BacktestResults { matched_files, visible_indices, .. } => {
            (matched_files, visible_indices)
        }
        _ => return,
    };

    let lines: Vec<Line> = if visible_indices.is_empty() {
        vec![Line::from(Span::styled(
            "  No matching files",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        let max_display = area.height.saturating_sub(1) as usize;
        let start = centered_scroll_offset(builder.selected_file, max_display, visible_indices.len());

        visible_indices.iter()
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
                    format!("{}{}{} {}", select_mark, prefix, indicator, file.relative_path),
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
        .title(Span::styled(title, Style::default().fg(Color::Yellow).bold()));
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

fn draw_rule_builder_confirm_exit(frame: &mut Frame, area: Rect) {
    let dialog_area = render_centered_dialog(frame, area, 58, 7);
    let title = " Discard Changes ";
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(Span::styled(title, Style::default().fg(Color::Yellow).bold()));
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
        + 2;                // Footer

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
            Constraint::Length(3),  // Pattern field with file count
            Constraint::Length(3),  // Tag field
            Constraint::Length(1),  // Separator
            Constraint::Min(1),     // Live preview
            Constraint::Length(2),  // Footer
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
    let sep_text = format!("─ LIVE PREVIEW {}─", "─".repeat(inner_area.width.saturating_sub(16) as usize));
    let sep = Paragraph::new(sep_text)
        .style(Style::default().fg(Color::Yellow));
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
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Title
            Constraint::Min(0),     // Content
            Constraint::Length(3),  // Footer
        ])
        .split(area);

    // Title
    let title = Paragraph::new(" Parser Bench ")
        .style(Style::default().fg(Color::Cyan).bold())
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(title, chunks[0]);

    // Main content - two-panel layout
    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(35), // Left panel: parser list
            Constraint::Percentage(65), // Right panel: details/results
        ])
        .split(chunks[1]);

    // Left panel: Parser List
    draw_parser_list(frame, app, content_chunks[0]);

    // Right panel: Details or results
    draw_parser_details(frame, app, content_chunks[1]);

    // Footer with keybindings
    let footer_text = " [↑/↓] Navigate  [t] Test  [n] Quick test  [/] Filter  [Esc] Back  [0] Home  [?] Help ";
    let footer = Paragraph::new(footer_text)
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::TOP));
    frame.render_widget(footer, chunks[2]);
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
        let hint_line = Paragraph::new("[/] filter  [r] refresh")
            .style(Style::default().fg(Color::DarkGray));
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
        let widget = Paragraph::new(content)
            .style(Style::default().fg(Color::DarkGray));
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
        let content = Paragraph::new("\n\n  Select a parser to see details\n\n  or press [n] to quick test any .py file")
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

/// Draw parser info when no test result is showing
fn draw_parser_info(frame: &mut Frame, parser: &super::app::ParserInfo, bound_files: &[super::app::BoundFileInfo], area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),  // Parser info
            Constraint::Min(0),     // Bound files
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
        ParserHealth::Healthy { success_rate, total_runs } => {
            format!("Healthy ({:.1}%, {} runs)", success_rate * 100.0, total_runs)
        }
        ParserHealth::Warning { consecutive_failures } => {
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
        format!("  Path:     {}", truncate_path_start(&parser.path.display().to_string(), 50)),
        format!("  Health:   {} {}", parser.health.symbol(), health_str),
        format!("  Modified: {}", parser.modified.format("%Y-%m-%d %H:%M")),
    ];

    let info_widget = Paragraph::new(info_lines.join("\n"))
        .block(Block::default().borders(Borders::ALL).title(format!(" {} ", parser.name)));
    frame.render_widget(info_widget, chunks[0]);

    // Bound files section
    let files_title = format!(" Bound Files ({}) ", bound_files.len());
    if bound_files.is_empty() {
        let empty_msg = "\n  No files match this parser's topics.\n\n  Use Discover mode to tag files.";
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

        let list = List::new(items).block(Block::default().borders(Borders::ALL).title(files_title));
        frame.render_widget(list, chunks[1]);
    }
}

/// Draw test result panel
fn draw_test_result(frame: &mut Frame, result: &super::app::ParserTestResult, area: Rect) {
    let title = if result.success {
        format!(" Test Result - PASSED ({} rows, {}ms) ", result.rows_processed, result.execution_time_ms)
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

        let widget = Paragraph::new(lines.join("\n"))
            .block(Block::default().borders(Borders::ALL).title(title).title_style(title_style));
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

        let widget = Paragraph::new(lines.join("\n"))
            .block(Block::default().borders(Borders::ALL).title(title).title_style(title_style));
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

    // Calculate layout based on whether pipeline is shown
    let (pipeline_height, _content_height) = if app.jobs_state.show_pipeline {
        (8, area.height.saturating_sub(14)) // Pipeline + status bar + footer
    } else {
        (0, area.height.saturating_sub(6)) // Just status bar + footer
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),              // Status bar
            Constraint::Length(pipeline_height), // Pipeline (if shown)
            Constraint::Min(0),                 // Job list
            Constraint::Length(3),              // Footer
        ])
        .split(area);

    // Status bar with aggregate stats (per spec Section 4.1)
    let ready = app.jobs_state.jobs.iter().filter(|j| j.status == JobStatus::Completed).count() as u32;
    let active = app.jobs_state.jobs.iter().filter(|j| matches!(j.status, JobStatus::Pending | JobStatus::Running)).count() as u32;
    let failed = app.jobs_state.jobs.iter().filter(|j| matches!(j.status, JobStatus::Failed | JobStatus::Cancelled)).count() as u32;
    let queue = app.jobs_state.jobs.iter().filter(|j| j.status == JobStatus::Pending).count() as u32;
    let output_bytes: u64 = app.jobs_state.jobs.iter()
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
        .block(Block::default().borders(Borders::BOTTOM).title(" JOBS ").title_style(Style::default().fg(Color::Cyan).bold()));
    frame.render_widget(status_bar, chunks[0]);

    // Pipeline summary (if toggled)
    let job_list_area = if app.jobs_state.show_pipeline && pipeline_height > 0 {
        draw_pipeline_summary(frame, app, chunks[1]);
        chunks[2]
    } else {
        // Combine pipeline slot with job list
        Rect::new(chunks[1].x, chunks[1].y, chunks[1].width, chunks[1].height + chunks[2].height)
    };

    let detail_height = (job_list_area.height / 3).max(6).min(job_list_area.height.saturating_sub(4));
    let content_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(detail_height)])
        .split(job_list_area);
    draw_jobs_list(frame, app, content_chunks[0]);
    draw_job_detail(frame, app, content_chunks[1]);

    // Footer with shortcuts (context-sensitive per spec Section 7)
    let footer_text = match app.jobs_state.view_state {
        JobsViewState::MonitoringPanel => " [Esc] Back  [p] Pause  [r] Reset  [0] Home ",
        _ => " [↑/↓] Select  [Tab] Actionable/Ready  [Enter] Pin  [f] Filter  [R] Retry  [c] Cancel  [L] Logs  [y] Copy path  [O] Open output  [Esc] Back  [0] Home ",
    };

    let footer = Paragraph::new(footer_text)
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::TOP));
    frame.render_widget(footer, chunks[3]);
}

/// Draw pipeline summary (per spec Section 3.2)
fn draw_pipeline_summary(frame: &mut Frame, app: &App, area: Rect) {
    let pipeline = &app.jobs_state.pipeline;

    let source_in_progress = if pipeline.source.in_progress > 0 {
        format!("@{}", pipeline.source.in_progress)
    } else { String::new() };

    let parsed_in_progress = if pipeline.parsed.in_progress > 0 {
        format!("@{}", pipeline.parsed.in_progress)
    } else { String::new() };

    let output_in_progress = if pipeline.output.in_progress > 0 {
        format!("{} run", pipeline.output.in_progress)
    } else { String::new() };

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
    let mut lines: Vec<Line> = Vec::new();
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
                "Press [f] to clear filters.",
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
                render_job_line(&mut lines, job, is_selected, area.width as usize);
            }
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            " READY (outputs you can query now)",
            ready_style,
        )));

        if ready_jobs.is_empty() {
            lines.push(Line::from(Span::styled(
                "  No ready outputs.",
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            for (i, job) in ready_jobs.iter().enumerate() {
                let is_selected = app.jobs_state.section_focus == JobsListSection::Ready
                    && i == app.jobs_state.selected_index;
                render_job_line(&mut lines, job, is_selected, area.width as usize);
            }
        }
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::White));

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

/// Render a single job (1-3 lines depending on type and status)
/// Per spec Sections 4.1-4.9
fn render_job_line(lines: &mut Vec<Line>, job: &JobInfo, is_selected: bool, _width: usize) {
    let status_symbol = job.status.symbol();
    let status_color = match job.status {
        JobStatus::Pending => Color::Yellow,
        JobStatus::Running => Color::Blue,
        JobStatus::Completed => Color::Green,
        JobStatus::Failed => Color::Red,
        JobStatus::Cancelled => Color::DarkGray,
    };

    let prefix = if is_selected { "▸ " } else { "  " };
    let name_style = if is_selected {
        Style::default().fg(Color::Cyan).bold()
    } else {
        Style::default().fg(Color::White)
    };

    // Line 1: Status, Type, Name, Version, Timestamp/Progress
    let version_str = job.version.as_deref().map(|v| format!(" v{}", v)).unwrap_or_default();
    let time_str = format_relative_time(job.started_at);

    let progress_or_time = if job.status == JobStatus::Running {
        // Show progress bar for running jobs
        let pct = if job.items_total > 0 {
            (job.items_processed as f64 / job.items_total as f64 * 100.0) as u8
        } else { 0 };
        format!("{}  {}%", render_progress_bar(pct, 12), pct)
    } else if job.status == JobStatus::Pending {
        "queued".to_string()
    } else {
        time_str
    };

    // Add backtest iteration if applicable
    let backtest_iter = if job.job_type == JobType::Backtest {
        if let Some(ref bt) = job.backtest {
            format!(" (iter {})", bt.iteration)
        } else { String::new() }
    } else { String::new() };

    lines.push(Line::from(vec![
        Span::styled(prefix, Style::default()),
        Span::styled(status_symbol, Style::default().fg(status_color)),
        Span::styled(format!(" {} ", job.job_type.as_str()), Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{}{}{}", job.name, version_str, backtest_iter), name_style),
        Span::styled(format!("  {}", progress_or_time), Style::default().fg(Color::DarkGray)),
    ]));

    // Line 2: Details (varies by job type and status)
    let detail_line = match (job.job_type, job.status) {
        (JobType::Scan, _) => {
            format!("           {} files", job.items_processed)
        }
        (JobType::Parse, JobStatus::Failed) => {
            format!("           {} files failed • {}", job.items_failed,
                job.failures.first().map(|f| f.error.as_str()).unwrap_or("Unknown error"))
        }
        (JobType::Parse, JobStatus::Completed) => {
            let path = job.output_path.as_deref().unwrap_or("");
            let size = job.output_size_bytes.map(format_size).unwrap_or_default();
            format!("           {} files → {} ({})", job.items_processed, truncate_path(path, 40), size)
        }
        (JobType::Parse, JobStatus::Running) => {
            let eta = calculate_eta(job.items_processed, job.items_total, job.started_at);
            format!("           {}/{} files • ETA {}", job.items_processed, job.items_total, eta)
        }
        (JobType::Backtest, _) => {
            if let Some(ref bt) = job.backtest {
                let pct = (bt.pass_rate * 100.0) as u32;
                let passed = (job.items_processed as f64 * bt.pass_rate) as u32;
                format!("           Pass: {}/{} files ({})% • {} high-failure passed",
                    passed, job.items_processed, pct, bt.high_failure_passed)
            } else {
                format!("           {} files tested", job.items_processed)
            }
        }
        (JobType::SchemaEval, _) => {
            format!("           {} paths analyzed", job.items_processed)
        }
        _ => {
            format!("           {} items", job.items_processed)
        }
    };

    lines.push(Line::from(Span::styled(detail_line, Style::default().fg(Color::DarkGray))));

    // Line 3: First failure (for failed jobs only)
    if job.status == JobStatus::Failed && !job.failures.is_empty() {
        let first_failure = &job.failures[0];
        let failure_hint = if first_failure.file_path.is_empty() {
            first_failure.error.as_str()
        } else {
            &first_failure.file_path
        };
        lines.push(Line::from(Span::styled(
            format!("           First failure: {}", truncate_path(failure_hint, 50)),
            Style::default().fg(Color::Red),
        )));
    }

    lines.push(Line::from("")); // Spacing between jobs
}

/// Draw job detail panel (per spec Section 7)
fn draw_job_detail(frame: &mut Frame, app: &App, area: Rect) {
    let content = if let Some(job) = app.jobs_state.selected_job() {
        let mut detail = format!(
            "Type:       {}\n\
             Name:       {}{}\n\
             Status:     {}\n\
             Started:    {}\n\
             Duration:   {}\n",
            job.job_type.as_str(),
            job.name,
            job.version.as_ref().map(|v| format!(" v{}", v)).unwrap_or_default(),
            job.status.as_str(),
            job.started_at.format("%H:%M:%S"),
            format_duration(job.started_at, job.completed_at)
        );

        if let Some(ref path) = job.output_path {
            detail.push_str(&format!("\nOUTPUT\n{}\n", path));
            if let Some(bytes) = job.output_size_bytes {
                detail.push_str(&format!("{} files • {}\n", job.items_processed, format_size(bytes)));
            }
        }

        if !job.failures.is_empty() {
            detail.push_str(&format!("\nFAILURES ({})\n", job.failures.len()));
            for failure in job.failures.iter().take(10) {
                detail.push_str(&format!("{}\n  {}\n", truncate_path(&failure.file_path, 50), failure.error));
            }
            if job.failures.len() > 10 {
                detail.push_str(&format!("... ({} more)\n", job.failures.len() - 10));
            }
        }

        // Action hints
        if job.status == JobStatus::Failed {
            detail.push_str("\n[R] Retry all  [L] Logs");
        } else if job.status == JobStatus::Running {
            detail.push_str("\n[c] Cancel  [L] Logs");
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

/// Draw monitoring panel (per spec Section 5)
fn draw_monitoring_panel(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),   // Title
            Constraint::Min(0),      // Content
            Constraint::Length(3),   // Footer
        ])
        .split(area);

    // Title
    let title = Paragraph::new(" MONITORING ")
        .style(Style::default().fg(Color::Cyan).bold())
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(title, chunks[0]);

    // Content: Queue + Throughput on top, Sinks on bottom
    let content_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    let top_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(content_chunks[0]);

    // Queue panel
    let queue = &app.jobs_state.monitoring.queue;
    let queue_content = format!(
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
    let throughput_sparkline = render_throughput_sparkline(&app.jobs_state.monitoring.throughput_history);
    let current_rps = app.jobs_state.monitoring.throughput_history.back()
        .map(|s| s.rows_per_second).unwrap_or(0.0);
    let avg_rps: f64 = if app.jobs_state.monitoring.throughput_history.is_empty() {
        0.0
    } else {
        app.jobs_state.monitoring.throughput_history.iter().map(|s| s.rows_per_second).sum::<f64>()
            / app.jobs_state.monitoring.throughput_history.len() as f64
    };

    let throughput_content = format!(
        "\n{}\n\n  {:.1}k rows/s avg      {:.1}k now",
        throughput_sparkline, avg_rps / 1000.0, current_rps / 1000.0
    );
    let throughput_block = Block::default()
        .borders(Borders::ALL)
        .title(" THROUGHPUT (5m) ")
        .border_style(Style::default().fg(Color::DarkGray));
    frame.render_widget(Paragraph::new(throughput_content).block(throughput_block), top_row[1]);

    // Sinks panel
    let mut sinks_content = String::new();
    for sink in &app.jobs_state.monitoring.sinks {
        sinks_content.push_str(&format!(
            "  {}   {} total   {} errors\n",
            sink.uri, format_size(sink.total_bytes), sink.error_count
        ));
        for output in &sink.outputs {
            sinks_content.push_str(&format!(
                "    └─ {}   {}   {} rows\n",
                output.name, format_size(output.bytes), output.rows
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
    frame.render_widget(Paragraph::new(sinks_content).block(sinks_block), content_chunks[1]);

    // Footer
    let paused_indicator = if app.jobs_state.monitoring.paused { " (PAUSED)" } else { "" };
    let footer = Paragraph::new(format!(" [Esc] Back  [p] Pause updates  [r] Reset stats{} ", paused_indicator))
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::TOP));
    frame.render_widget(footer, chunks[2]);
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

    history.iter()
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

    let sparkline: String = values.iter()
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

/// Truncate path from the left, keeping filename visible
fn truncate_path(path: &str, max_len: usize) -> String {
    if path.len() <= max_len {
        return path.to_string();
    }
    format!("...{}", &path[path.len() - max_len + 3..])
}


/// Draw the help overlay (per spec Section 3.1)
fn draw_help_overlay(frame: &mut Frame, area: Rect, app: &App) {
    use ratatui::widgets::Clear;

    // Center the help panel - larger to fit all content
    let help_width = 76.min(area.width.saturating_sub(4));
    let help_height = 36.min(area.height.saturating_sub(2));
    let help_x = (area.width.saturating_sub(help_width)) / 2;
    let help_y = (area.height.saturating_sub(help_height)) / 2;

    let help_area = Rect {
        x: area.x + help_x,
        y: area.y + help_y,
        width: help_width,
        height: help_height,
    };

    // Clear the background
    frame.render_widget(Clear, help_area);

    // Context-aware help content based on current mode
    let help_text = match app.mode {
        TuiMode::Discover => {
            if app.discover.view_state == super::app::DiscoverViewState::RuleBuilder {
                vec![
                    "",
                    "  RULE BUILDER KEYS                      NAVIGATION",
                    "  ─────────────────                      ──────────",
                    "  Tab/Shift+Tab  Cycle between fields    3         Focus Files panel",
                    "  ↑/↓ arrows     Move between fields     4         Sources view",
                    "  ←/→ arrows     Switch panels           0 / H     Home",
                    "                                         Esc       Back / Close",
                    "  ↑/↓            Navigate lists",
                    "  Enter          Expand folder / Save    PATTERN SYNTAX",
                    "                                         ──────────────",
                    "  ACTIONS                                **/*      All files",
                    "  ───────                                *.rs      Rust files",
                    "  1              Open Source dropdown    **/*.csv  CSV files (recursive)",
                    "  2              Open Tags dropdown      <name>    Extract field",
                    "  s              Scan new directory",
                    "  Ctrl+S         Save rule               FOCUS INDICATOR",
                    "  t              Run backtest            ──────────────",
                    "                                         █ cursor   = text input active",
                    "                                         ► pointer  = list navigation",
                    "  IN DROPDOWNS",
                    "  ────────────                           GLOBAL",
                    "  /              Filter list             ──────",
                    "  ↑/↓           Navigate                 ?         This help",
                    "  Enter          Select item             r         Refresh view",
                    "  Esc            Close dropdown          q         Quit",
                    "",
                    "  Press ? or Esc to close",
                ]
            } else {
                vec![
                    "",
                    "  DISCOVER MODE                          NAVIGATION",
                    "  ─────────────                          ──────────",
                    "  s              Scan new directory      3         Jobs view",
                    "  n              Create new rule         4         Sources view",
                    "  Enter          Open Rule Builder       0 / H     Home",
                    "                                         Esc       Back / Close",
                    "  ↑/↓           Navigate files",
                    "  /              Filter files            GLOBAL",
                    "                                         ──────",
                    "  IN SOURCES/TAGS DROPDOWN               ?         This help",
                    "  ────────────────────────               r         Refresh view",
                    "  /              Filter list             q         Quit",
                    "  s              Scan (in Sources)",
                    "  ↑/↓           Navigate",
                    "  Enter          Select",
                    "  Esc            Close",
                    "",
                    "  Press ? or Esc to close",
                ]
            }
        }
        TuiMode::ParserBench => vec![
            "",
            "  PARSER BENCH                           NAVIGATION",
            "  ────────────                           ──────────",
            "  n              Quick test any .py      1-4       Go to view",
            "  t              Test selected parser    0 / H     Home",
            "  ↑/↓           Navigate parsers        Esc       Back / Close",
            "  /              Filter parsers",
            "  Enter          View details            GLOBAL",
            "                                         ──────",
            "                                         ?         This help",
            "                                         r         Refresh view",
            "                                         q         Quit",
            "",
            "  Press ? or Esc to close",
        ],
        TuiMode::Jobs => vec![
            "",
            "  JOBS VIEW                              NAVIGATION",
            "  ─────────                              ──────────",
            "  ↑/↓           Navigate jobs           1-4       Go to view",
            "  Tab            Switch Actionable/Ready 0 / H     Home",
            "  Enter          Pin details             Esc       Back / Close",
            "  o              Open output             GLOBAL",
            "  f              Filter jobs             GLOBAL",
            "                                         ──────",
            "                                         ?         This help",
            "                                         r         Refresh view",
            "                                         q         Quit",
            "",
            "  Press ? or Esc to close",
        ],
        _ => vec![
            "",
            "  NAVIGATION                             GLOBAL ACTIONS",
            "  ──────────                             ──────────────",
            "  1              Discover view           ?         This help",
            "  2              Parser Bench view       r         Refresh view",
            "  3              Jobs view               q         Quit",
            "  4              Sources view            P         Parser Bench",
            "  0 / H          Home                    S         Sources drawer",
            "  Esc            Back / Close",
            "  ↑/↓/←/→       Navigate",
            "  Enter          Select",
            "",
            "  Press ? or Esc to close",
        ],
    };

    let help_paragraph = Paragraph::new(help_text.join("\n"))
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

    let lines = help_text
        .into_iter()
        .map(Line::from)
        .collect::<Vec<_>>();
    let para = Paragraph::new(lines)
        .style(Style::default().fg(Color::White));
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
            let idx = builder.selected_pattern_seed.min(builder.pattern_seeds.len().saturating_sub(1));
            if let Some(seed) = builder.pattern_seeds.get(idx) {
                ("Pattern Details", vec![
                    format!("Pattern: {}", seed.pattern),
                    format!("Matches: {}", seed.match_count),
                ])
            } else {
                ("Pattern Details", vec!["No pattern suggestions available.".to_string()])
            }
        }
        SuggestionSection::Structures => {
            let idx = builder.selected_archetype.min(builder.path_archetypes.len().saturating_sub(1));
            if let Some(arch) = builder.path_archetypes.get(idx) {
                let sample = arch.sample_paths.first().cloned().unwrap_or_else(|| arch.template.clone());
                ("Structure Details", vec![
                    format!("Template: {}", arch.template),
                    format!("Sample: {}", sample),
                    format!("Files: {}", arch.file_count),
                ])
            } else {
                ("Structure Details", vec!["No structure suggestions available.".to_string()])
            }
        }
        SuggestionSection::Filenames => {
            let idx = builder.selected_naming_scheme.min(builder.naming_schemes.len().saturating_sub(1));
            if let Some(scheme) = builder.naming_schemes.get(idx) {
                ("Filename Details", vec![
                    format!("Template: {}", scheme.template),
                    format!("Example: {}", scheme.example),
                    format!("Files: {}", scheme.file_count),
                ])
            } else {
                ("Filename Details", vec!["No filename suggestions available.".to_string()])
            }
        }
        SuggestionSection::Synonyms => {
            let idx = builder.selected_synonym.min(builder.synonym_suggestions.len().saturating_sub(1));
            if let Some(syn) = builder.synonym_suggestions.get(idx) {
                ("Synonym Details", vec![
                    format!("{} → {}", syn.short_form, syn.canonical_form),
                ])
            } else {
                ("Synonym Details", vec!["No synonym suggestions available.".to_string()])
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

    // Layout: title, content, footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Title
            Constraint::Min(0),     // Content
            Constraint::Length(3),  // Footer
        ])
        .split(area);

    // Title
    let title = Paragraph::new("Settings")
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .alignment(ratatui::layout::Alignment::Center)
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray)));
    frame.render_widget(title, chunks[0]);

    // Content - three category boxes
    let content_chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(6),  // General
            Constraint::Length(6),  // Display
            Constraint::Length(5),  // About
        ])
        .split(chunks[1]);

    // General section
    draw_settings_category(
        frame,
        app,
        content_chunks[0],
        "General",
        SettingsCategory::General,
        &[
            ("Default source path", &app.settings.default_source_path, "[Edit]"),
            ("Auto-scan on startup", if app.settings.auto_scan_on_startup { "Yes" } else { "No" }, "[Toggle]"),
            ("Confirm destructive", if app.settings.confirm_destructive { "Yes" } else { "No" }, "[Toggle]"),
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
            ("Unicode symbols", if app.settings.unicode_symbols { "Yes" } else { "No" }, "[Toggle]"),
            ("Show hidden files", if app.settings.show_hidden_files { "Yes" } else { "No" }, "[Toggle]"),
        ],
    );

    // About section (read-only)
    let about_block = Block::default()
        .title(" About ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(
            if app.settings.category == SettingsCategory::About { Color::Cyan } else { Color::DarkGray }
        ));

    let (db_backend, db_path) = if let Some(path) = app.config.database.as_ref() {
        let backend = match path.extension().and_then(|ext| ext.to_str()) {
            Some("duckdb") => DbBackend::DuckDb,
            _ => DbBackend::Sqlite,
        };
        (backend, path.clone())
    } else {
        (default_db_backend(), active_db_path())
    };
    let backend_label = match db_backend {
        DbBackend::Sqlite => "sqlite",
        DbBackend::DuckDb => "duckdb",
    };

    let about_lines = vec![
        Line::from(format!("  Version:    {}", env!("CARGO_PKG_VERSION"))),
        Line::from(format!("  Database:   {} ({})", db_path.display(), backend_label)),
        Line::from(format!("  Config:     ~/.casparian_flow/config.toml")),
    ];
    let about = Paragraph::new(about_lines).block(about_block);
    frame.render_widget(about, content_chunks[2]);

    // Footer
    let footer_text = if app.settings.editing {
        "[Enter] Save  [Esc] Cancel"
    } else {
        "[↑↓] Navigate  [Tab] Category  [Enter] Edit/Toggle "
    };
    let footer = Paragraph::new(footer_text)
        .style(Style::default().fg(Color::DarkGray))
        .alignment(ratatui::layout::Alignment::Center)
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray)));
    frame.render_widget(footer, chunks[2]);
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
    let border_color = if is_active { Color::Cyan } else { Color::DarkGray };

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::tui::app::{JobInfo, JobsState, JobType};
    use crate::cli::tui::TuiArgs;
    use chrono::Local;
    use ratatui::backend::TestBackend;

    fn test_args() -> TuiArgs {
        TuiArgs {
            database: Some(std::env::temp_dir().join(format!(
                "casparian_test_{}.sqlite3",
                uuid::Uuid::new_v4()
            ))),
        }
    }

    fn create_test_job(id: i64, name: &str, status: JobStatus) -> JobInfo {
        JobInfo {
            id,
            file_version_id: Some(id * 100),
            job_type: JobType::Parse,
            name: name.to_string(),
            version: Some("1.0.0".to_string()),
            status,
            started_at: Local::now(),
            completed_at: None,
            items_total: 100,
            items_processed: 50,
            items_failed: 0,
            output_path: Some(format!("/data/output/{}.parquet", name)),
            output_size_bytes: None,
            backtest: None,
            failures: vec![],
        }
    }

    /// Test that UTF-8 names are rendered safely without panicking
    #[test]
    fn test_utf8_path_truncation() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut app = App::new(test_args());
        app.mode = TuiMode::Jobs;

        // Create a job with a UTF-8 name containing multi-byte characters
        let mut jobs_state = JobsState::default();
        jobs_state.jobs = vec![create_test_job(1, "文件夹_数据_测试解析器", JobStatus::Running)];
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

        let mut app = App::new(test_args());
        app.mode = TuiMode::Jobs;

        // Emoji are 4-byte UTF-8 sequences
        let mut jobs_state = JobsState::default();
        jobs_state.jobs = vec![create_test_job(1, "📁_parser_📊_reports_📈", JobStatus::Pending)];
        app.jobs_state = jobs_state;

        let result = terminal.draw(|f| draw(f, &app));
        assert!(result.is_ok(), "Rendering emoji name should not panic");
    }

    #[test]
    fn test_draw_home_view() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let app = App::new(test_args());

        terminal.draw(|f| draw(f, &app)).unwrap();

        // Check that home hub elements are rendered
        let buffer = terminal.backend().buffer();
        let content: String = buffer
            .content
            .iter()
            .map(|cell| cell.symbol().chars().next().unwrap_or(' '))
            .collect();

        // Home hub: Quick Start (Sources) + Recent Activity panels
        assert!(content.contains("Quick Start: Scan a Source"));
        assert!(content.contains("Recent Activity"));
    }

    #[test]
    fn test_draw_discover_screen() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut app = App::new(test_args());
        app.enter_discover_mode();
        
        // Add some mock files
        app.discover.files.push(crate::cli::tui::app::FileInfo {
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

        assert!(content.contains("Rule Builder"));
    }



    #[test]
    fn test_draw_parser_bench_mode() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        // Test Parser Bench mode
        let mut app = App::new(test_args());
        app.mode = TuiMode::ParserBench;
        terminal.draw(|f| draw(f, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = buffer
            .content
            .iter()
            .map(|cell| cell.symbol().chars().next().unwrap_or(' '))
            .collect();

        assert!(content.contains("Parser"));
        assert!(content.contains("Bench"));
    }

}
