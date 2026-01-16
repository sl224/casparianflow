//! UI rendering for the TUI

use ratatui::{
    prelude::*,
    widgets::{Block, BorderType, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
};

use crate::cli::output::format_number;
use ratatui::widgets::Clear;
use chrono::{DateTime, Local};
use super::app::{App, DiscoverFocus, DiscoverViewState, JobInfo, JobStatus, JobType, JobsViewState, MessageRole, ParserHealth, ThroughputSample, TuiMode};

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

    // Split for Global Sidebar if active
    let (main_area, sidebar_area) = if app.show_chat_sidebar {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(area);
        (chunks[0], Some(chunks[1]))
    } else {
        (area, None)
    };

    // Draw Main Content
    match app.mode {
        TuiMode::Home => draw_home_screen(frame, app, main_area),
        TuiMode::Discover => draw_discover_screen(frame, app, main_area),
        TuiMode::ParserBench => draw_parser_bench_screen(frame, app, main_area),
        TuiMode::Inspect => draw_inspect_screen(frame, app, main_area),
        TuiMode::Jobs => draw_jobs_screen(frame, app, main_area),
        TuiMode::Settings => draw_settings_screen(frame, app, main_area),
    }

    // Draw Sidebar
    if let Some(area) = sidebar_area {
        draw_sidebar(frame, app, area);
    }

    // Draw Help Overlay (on top of everything)
    if app.show_help {
        draw_help_overlay(frame, area, app);
    }
}

/// Draw the home hub screen with 4 cards
fn draw_home_screen(frame: &mut Frame, app: &App, area: Rect) {
    // Layout: title bar, main content (4 cards), footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Title bar
            Constraint::Min(0),     // Main content (cards)
            Constraint::Length(3),  // Footer/status
        ])
        .split(area);

    // Title bar
    let title = Paragraph::new(" Casparian Flow - Home Hub ")
        .style(Style::default().fg(Color::Cyan).bold())
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(title, chunks[0]);

    // Draw the 4 cards in a 2x2 grid
    draw_home_cards(frame, app, chunks[1]);

    // Footer with shortcuts
    let footer = Paragraph::new(" [↑↓←→/hjkl] Navigate  [Enter] Select  [1-4] Go to view  [0/H] Home  [q] Quit ")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::TOP));
    frame.render_widget(footer, chunks[2]);
}

/// Draw the 4 home hub cards in a 2x2 grid
fn draw_home_cards(frame: &mut Frame, app: &App, area: Rect) {
    // Split area into 2 rows
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Split each row into 2 columns
    let top_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(rows[0]);
    let bottom_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(rows[1]);

    let areas = [top_row[0], top_row[1], bottom_row[0], bottom_row[1]];

    // Card definitions: (title, description, stats, number)
    // Order matches spec: 1=Discover, 2=Parser Bench, 3=Jobs, 4=Sources
    let cards: [(&str, &str, String, &str); 4] = [
        (
            "Discover",
            "Find and tag files for processing",
            format!(
                "{} files, {} sources",
                app.home.stats.file_count, app.home.stats.source_count
            ),
            "1",
        ),
        (
            "Parser Bench",
            "Develop and test parsers",
            format!(
                "{} parsers, {} paused",
                app.home.stats.parser_count, app.home.stats.paused_parsers
            ),
            "2",
        ),
        (
            "Jobs",
            "Monitor job queue and workers",
            format!(
                "{} running, {} pending, {} failed",
                app.home.stats.running_jobs,
                app.home.stats.pending_jobs,
                app.home.stats.failed_jobs
            ),
            "3",
        ),
        (
            "Sources",
            "Manage data sources",
            format!(
                "{} sources configured",
                app.home.stats.source_count
            ),
            "4",
        ),
    ];

    for (i, card_area) in areas.iter().enumerate() {
        let (title, desc, stats, number) = &cards[i];
        let is_selected = app.home.selected_card == i;

        draw_card(frame, *card_area, title, desc, stats, number, is_selected);
    }
}

/// Draw a single card
fn draw_card(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    description: &str,
    stats: &str,
    number: &str,
    is_selected: bool,
) {
    let border_style = if is_selected {
        Style::default().fg(Color::Cyan).bold()
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let title_style = if is_selected {
        Style::default().fg(Color::Cyan).bold()
    } else {
        Style::default().fg(Color::White)
    };

    let block = Block::default()
        .title(format!(" [{}] {} ", number, title))
        .title_style(title_style)
        .borders(Borders::ALL)
        .border_style(border_style);

    // Calculate inner area for content
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Content layout
    let content_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Spacer
            Constraint::Length(2), // Description
            Constraint::Min(0),    // Stats
        ])
        .split(inner);

    // Description
    let desc_style = Style::default().fg(if is_selected {
        Color::White
    } else {
        Color::Gray
    });
    let desc_para = Paragraph::new(description)
        .style(desc_style)
        .alignment(Alignment::Center);
    frame.render_widget(desc_para, content_chunks[1]);

    // Stats
    let stats_style = Style::default().fg(if is_selected {
        Color::Yellow
    } else {
        Color::DarkGray
    });
    let stats_para = Paragraph::new(stats)
        .style(stats_style)
        .alignment(Alignment::Center);
    frame.render_widget(stats_para, content_chunks[2]);
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
        let hint_line = Paragraph::new("[/]filter [s]scan [j/k]nav [Enter]select [Esc]close")
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
        // Expanded dropdown with optional filter line
        let is_filtering = app.discover.tags_filtering;

        let inner_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .margin(1)
            .split(area);

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
            let hint_line = Paragraph::new("[/]filter [j/k]nav [Enter]select [Esc]close")
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
        let visible_height = inner_chunks[1].height as usize;

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
        frame.render_widget(block, area);
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
    // Format: " Rule Builder - [1] Source: X ▾  [2] Tags: All ▾  | 247 files match | Scan: 12345 files "
    let source_name = app.discover.selected_source()
        .map(|s| s.name.clone())
        .unwrap_or_else(|| "All".to_string());
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

    let header_text = format!(
        " Rule Builder - [1] Source: {} ▾  [2] Tags: All ▾  │ {} files match{}",
        source_name, match_count, scan_info
    );
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
        (" [s] Scan  [Tab] Navigate  [↑↓] Move  [Ctrl+S] Save  [?] Help  [Esc] Close ".to_string(), Style::default().fg(Color::DarkGray))
    };
    let footer = Paragraph::new(footer_text)
        .style(footer_style)
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::TOP));
    frame.render_widget(footer, chunks[2]);
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
            Constraint::Min(0),     // Padding
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
}

/// Draw the right panel of Rule Builder (file results list)
fn draw_rule_builder_right_panel(frame: &mut Frame, builder: &super::extraction::RuleBuilderState, area: Rect, scan_error: Option<&str>) {
    use super::extraction::{RuleBuilderFocus, FileResultsPhase};

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
    let header_text = match builder.file_results_phase {
        FileResultsPhase::Exploration => {
            let folder_count = builder.folder_matches.len();
            let file_count: usize = builder.folder_matches.iter().map(|f| f.count).sum();
            if builder.is_streaming {
                format!(" FOLDERS  {} folders ({} files)  {}", folder_count, file_count, spinner_char(builder.stream_elapsed_ms / 100))
            } else {
                format!(" FOLDERS  {} folders ({} files)  [Enter] drill down", folder_count, file_count)
            }
        }
        FileResultsPhase::ExtractionPreview => {
            let file_count = builder.preview_files.len();
            let warning_count: usize = builder.preview_files.iter().map(|f| f.warnings.len()).sum();
            if warning_count > 0 {
                format!(" PREVIEW  {} files  ⚠ {} warnings  t:test", file_count, warning_count)
            } else {
                format!(" PREVIEW  {} files  ✓ extractions OK  t:test", file_count)
            }
        }
        FileResultsPhase::BacktestResults => {
            let filter_label = builder.result_filter.label();
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
    match builder.file_results_phase {
        FileResultsPhase::Exploration => {
            draw_exploration_phase(frame, builder, file_list_inner, file_list_focused, scan_error);
        }
        FileResultsPhase::ExtractionPreview => {
            draw_extraction_preview_phase(frame, builder, file_list_inner, file_list_focused);
        }
        FileResultsPhase::BacktestResults => {
            draw_backtest_results_phase(frame, builder, file_list_inner, file_list_focused);
        }
    }

    // Phase-specific status bar
    let status = match builder.file_results_phase {
        FileResultsPhase::Exploration => {
            if builder.folder_matches.is_empty() {
                " Type a pattern to find files...".to_string()
            } else {
                let total_files: usize = builder.folder_matches.iter().map(|f| f.count).sum();
                format!(" {} folders  {} files matched", builder.folder_matches.len(), total_files)
            }
        }
        FileResultsPhase::ExtractionPreview => {
            if builder.preview_files.is_empty() {
                " No files match current pattern".to_string()
            } else {
                let ok_count = builder.preview_files.iter().filter(|f| f.warnings.is_empty()).count();
                format!(" {} files  {} OK  {} warnings  Press 't' to run backtest",
                    builder.preview_files.len(), ok_count,
                    builder.preview_files.len() - ok_count)
            }
        }
        FileResultsPhase::BacktestResults => {
            format!(
                " Pass: {}  Fail: {}  Skip: {}",
                builder.backtest.pass_count,
                builder.backtest.fail_count,
                builder.backtest.excluded_count
            )
        }
    };
    let status_para = Paragraph::new(status)
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(status_para, right_chunks[2]);
}

/// Phase 1: Exploration - folder counts + sample filenames
fn draw_exploration_phase(frame: &mut Frame, builder: &super::extraction::RuleBuilderState, area: Rect, focused: bool, scan_error: Option<&str>) {
    let lines: Vec<Line> = if builder.folder_matches.is_empty() {
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
        let start = centered_scroll_offset(builder.selected_file, max_display, builder.folder_matches.len());

        // Calculate column widths for alignment
        // Use width proportions: 60% folder path, 15% count, 25% sample filename
        let available_width = area.width.saturating_sub(6) as usize; // Reserve space for prefix/icon
        let path_width = (available_width * 50 / 100).min(35);
        let count_width = 8;

        builder.folder_matches.iter()
            .enumerate()
            .skip(start)
            .take(max_display)
            .map(|(i, folder)| {
                let is_selected = i == builder.selected_file && focused;
                let is_expanded = builder.expanded_folder_indices.contains(&i);

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
    let lines: Vec<Line> = if builder.preview_files.is_empty() {
        vec![Line::from(Span::styled(
            "  No files with extractions",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        let max_display = area.height.saturating_sub(1) as usize;
        let start = centered_scroll_offset(builder.selected_file, max_display, builder.preview_files.len());

        builder.preview_files.iter()
            .enumerate()
            .skip(start)
            .take(max_display)
            .map(|(i, file)| {
                let is_selected = i == builder.selected_file && focused;
                let has_warnings = !file.warnings.is_empty();

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
                    "{}{} {}  [{}]",
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
    let lines: Vec<Line> = if builder.visible_indices.is_empty() {
        vec![Line::from(Span::styled(
            "  No matching files",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        let max_display = area.height.saturating_sub(1) as usize;
        let start = centered_scroll_offset(builder.selected_file, max_display, builder.visible_indices.len());

        builder.visible_indices.iter()
            .skip(start)
            .take(max_display)
            .enumerate()
            .map(|(i, &idx)| {
                let file = &builder.matched_files[idx];
                let is_selected = i + start == builder.selected_file && focused;
                let prefix = if is_selected { "► " } else { "  " };
                let indicator = file.test_result.indicator();

                let style = if is_selected {
                    Style::default().fg(Color::White).bold()
                } else {
                    Style::default().fg(Color::Gray)
                };

                Line::from(Span::styled(
                    format!("{}{} {}", prefix, indicator, file.relative_path),
                    style,
                ))
            })
            .collect()
    };

    let file_list = Paragraph::new(lines);
    frame.render_widget(file_list, area);
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

/// Draw the AI Sidebar (Chat)
fn draw_sidebar(frame: &mut Frame, app: &App, area: Rect) {
    let main_block = Block::default()
        .borders(Borders::LEFT)
        .title(" AI Assistant ")
        .style(Style::default().fg(Color::Cyan));
    
    frame.render_widget(main_block, area);

    // Inner area for content to respect the border
    let inner_area = area.inner(Margin { vertical: 1, horizontal: 1 });
    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),     // Messages
            Constraint::Length(3),  // Input
        ])
        .split(inner_area);

    draw_messages(frame, app, inner_chunks[0]);
    draw_input(frame, app, inner_chunks[1]);
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
    let footer_text = " [j/k] Navigate  [t] Test  [n] Quick test  [/] Filter  [?] Help  [Esc] Home ";
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
            .style(Style::default().fg(Color::DarkGray))
            .block(Block::default().borders(Borders::ALL).title(title));
        frame.render_widget(widget, area);
        return;
    }

    // Build list items
    let items: Vec<ListItem> = app
        .parser_bench
        .parsers
        .iter()
        .enumerate()
        .map(|(i, parser)| {
            let symbol = parser.health.symbol();
            let version = parser.version.as_deref().unwrap_or("—");
            let name = &parser.name;

            // Format: ● parser_name     v1.0.0
            let line = format!(
                " {} {:<20} {}",
                symbol,
                truncate_end(name, 20),
                version
            );

            let style = if i == app.parser_bench.selected_parser {
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

    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(title));

    let mut state = ListState::default();
    state.select(Some(app.parser_bench.selected_parser));
    frame.render_stateful_widget(list, area, &mut state);
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

/// Draw the Inspect mode screen - browse output tables and run queries
fn draw_inspect_screen(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Title
            Constraint::Min(0),     // Content
            Constraint::Length(3),  // Footer
        ])
        .split(area);

    // Title
    let title = Paragraph::new(" Inspect Mode - Data Browser ")
        .style(Style::default().fg(Color::Cyan).bold())
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(title, chunks[0]);

    // Main content: tables list (left) + detail/query pane (right)
    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(chunks[1]);

    // Tables list
    draw_tables_list(frame, app, content_chunks[0]);

    // Detail/query pane
    draw_table_detail(frame, app, content_chunks[1]);

    // Footer with shortcuts
    let footer_text = if app.inspect.query_focused {
        " [Enter] Execute  [Esc] Exit query  [/] Toggle query "
    } else {
        " [j/k] Navigate  [/] Query  [e] Export  [f] Filter  [Esc] Home "
    };
    let footer = Paragraph::new(footer_text)
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::TOP));
    frame.render_widget(footer, chunks[2]);
}

/// Draw the tables list in Inspect mode
fn draw_tables_list(frame: &mut Frame, app: &App, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();

    if app.inspect.tables.is_empty() {
        lines.push(Line::from(Span::styled(
            "No output tables found.",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Run a parser to generate output.",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for (i, table) in app.inspect.tables.iter().enumerate() {
            let is_selected = i == app.inspect.selected_table;

            let style = if is_selected {
                Style::default().fg(Color::Cyan).bold()
            } else {
                Style::default().fg(Color::White)
            };

            let prefix = if is_selected { "> " } else { "  " };

            // Table name
            lines.push(Line::from(Span::styled(
                format!("{}{}", prefix, table.name),
                style,
            )));

            // Stats line
            let size_kb = table.size_bytes / 1024;
            let stats_text = format!(
                "  {} rows, {} cols, {}KB",
                format_number(table.row_count),
                table.column_count,
                format_number(size_kb as u64)
            );
            lines.push(Line::from(Span::styled(
                stats_text,
                if is_selected {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default().fg(Color::DarkGray)
                },
            )));

            lines.push(Line::from("")); // Spacing
        }
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Tables ")
        .border_style(Style::default().fg(Color::White));

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

/// Draw table detail/query pane in Inspect mode
fn draw_table_detail(frame: &mut Frame, app: &App, area: Rect) {
    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),     // Table info / query result
            Constraint::Length(4), // Query input
        ])
        .split(area);

    // Info or query result
    let info_content = if let Some(ref result) = app.inspect.query_result {
        result.clone()
    } else if let Some(table) = app.inspect.tables.get(app.inspect.selected_table) {
        format!(
            "Table: {}\n\nRows: {}\nColumns: {}\nSize: {} bytes\nLast Updated: {}\n\n{}",
            table.name,
            format_number(table.row_count),
            table.column_count,
            format_number(table.size_bytes),
            table.last_updated.format("%Y-%m-%d %H:%M:%S"),
            "Press / to enter a SQL query."
        )
    } else {
        "Select a table to view details.".to_string()
    };

    let info_block = Block::default()
        .borders(Borders::ALL)
        .title(" Details ")
        .border_style(Style::default().fg(Color::White));

    let info = Paragraph::new(info_content)
        .block(info_block)
        .wrap(Wrap { trim: true });
    frame.render_widget(info, inner_chunks[0]);

    // Query input
    let query_border_style = if app.inspect.query_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let query_block = Block::default()
        .borders(Borders::ALL)
        .title(if app.inspect.query_focused {
            " SQL Query (Enter to run) "
        } else {
            " SQL Query (press / to focus) "
        })
        .border_style(query_border_style);

    let query_text = if app.inspect.query_input.is_empty() && !app.inspect.query_focused {
        "SELECT * FROM table_name LIMIT 10".to_string()
    } else {
        app.inspect.query_input.clone()
    };

    let query_style = if app.inspect.query_focused {
        Style::default().fg(Color::White)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let query = Paragraph::new(query_text)
        .style(query_style)
        .block(query_block);
    frame.render_widget(query, inner_chunks[1]);

    // Show cursor in query input when focused
    if app.inspect.query_focused {
        frame.set_cursor_position(Position::new(
            inner_chunks[1].x + app.inspect.query_input.len() as u16 + 1,
            inner_chunks[1].y + 1,
        ));
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
    let (running, done, failed, total_files, total_bytes) = app.jobs_state.aggregate_stats();
    let status_parts: Vec<String> = vec![
        if running > 0 { format!("↻ {} running", running) } else { String::new() },
        if done > 0 { format!("✓ {} done", done) } else { String::new() },
        if failed > 0 { format!("✗ {} failed", failed) } else { String::new() },
    ].into_iter().filter(|s| !s.is_empty()).collect();

    let status_text = format!(
        "  {}     {}/{} files • {} output",
        status_parts.join("   "),
        total_files,
        app.jobs_state.pipeline.source.count,
        format_size(total_bytes)
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

    // Main content depends on view state
    match app.jobs_state.view_state {
        JobsViewState::JobList => {
            draw_jobs_list(frame, app, job_list_area);
        }
        JobsViewState::DetailPanel => {
            // Split into list and detail
            let content_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(job_list_area);
            draw_jobs_list(frame, app, content_chunks[0]);
            draw_job_detail(frame, app, content_chunks[1]);
        }
        _ => {
            draw_jobs_list(frame, app, job_list_area);
        }
    }

    // Footer with shortcuts (context-sensitive per spec Section 7)
    let footer_text = match app.jobs_state.view_state {
        JobsViewState::DetailPanel => " [Esc] Back  [R] Retry  [l] Logs  [y] Copy path ",
        JobsViewState::MonitoringPanel => " [Esc] Back  [p] Pause  [r] Reset ",
        _ => " [j/k] Navigate  [Enter] Details  [P] Pipeline  [m] Monitor  [f] Filter  [?] Help ",
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
    let jobs = app.jobs_state.filtered_jobs();

    if jobs.is_empty() {
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
        for (i, job) in jobs.iter().enumerate() {
            let is_selected = i == app.jobs_state.selected_index;
            render_job_line(&mut lines, job, is_selected, area.width as usize);
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
        _ => {
            format!("           {} items", job.items_processed)
        }
    };

    lines.push(Line::from(Span::styled(detail_line, Style::default().fg(Color::DarkGray))));

    // Line 3: First failure (for failed jobs only)
    if job.status == JobStatus::Failed && !job.failures.is_empty() {
        let first_failure = &job.failures[0];
        lines.push(Line::from(Span::styled(
            format!("           First failure: {}", truncate_path(&first_failure.file_path, 50)),
            Style::default().fg(Color::Red),
        )));
    }

    lines.push(Line::from("")); // Spacing between jobs
}

/// Draw job detail panel (per spec Section 7)
fn draw_job_detail(frame: &mut Frame, app: &App, area: Rect) {
    let jobs = app.jobs_state.filtered_jobs();

    let content = if let Some(job) = jobs.get(app.jobs_state.selected_index) {
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
            detail.push_str("\n[R] Retry all  [l] Logs");
        } else if job.status == JobStatus::Running {
            detail.push_str("\n[c] Cancel  [l] Logs");
        }

        detail
    } else {
        "Select a job to view details.".to_string()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" JOB DETAILS ")
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


/// Draw chat messages with proper scrolling and line wrapping
fn draw_messages(frame: &mut Frame, app: &App, area: Rect) {
    // Calculate available width for text (minus borders and padding)
    let available_width = area.width.saturating_sub(4) as usize;

    // Build wrapped message lines
    let mut lines: Vec<Line> = Vec::new();

    for msg in &app.chat.messages {
        let (style, prefix) = match msg.role {
            MessageRole::User => (Style::default().fg(Color::Green), "You"),
            MessageRole::Assistant => (Style::default().fg(Color::Cyan), "Claude"),
            MessageRole::System => (Style::default().fg(Color::Yellow), "System"),
            MessageRole::Tool => (Style::default().fg(Color::Magenta), "Tool"),
        };

        // Format timestamp
        let timestamp = msg.timestamp.format("%H:%M:%S").to_string();

        // Header line with role and timestamp
        let header = Line::from(vec![
            Span::styled(format!("{} ", prefix), style.bold()),
            Span::styled(format!("[{}]", timestamp), Style::default().fg(Color::DarkGray)),
        ]);
        lines.push(header);

        // Wrap message content
        let wrapped = wrap_text(&msg.content, available_width);
        for line_text in wrapped {
            lines.push(Line::from(Span::styled(format!("  {}", line_text), style)));
        }

        // Add empty line between messages
        lines.push(Line::from(""));
    }

    // Calculate scroll position
    let total_lines = lines.len();
    let visible_lines = area.height.saturating_sub(2) as usize; // Minus borders

    // Clamp scroll to valid range
    let max_scroll = total_lines.saturating_sub(visible_lines);
    let scroll_offset = app.chat.scroll.min(max_scroll);

    // Get visible slice of lines
    let visible_start = scroll_offset;
    let visible_end = (scroll_offset + visible_lines).min(total_lines);
    let visible_lines_slice: Vec<Line> = lines
        .into_iter()
        .skip(visible_start)
        .take(visible_end - visible_start)
        .collect();

    let messages_block = Block::default()
        .borders(Borders::ALL)
        .title(" Messages ")
        .title_alignment(Alignment::Left)
        .border_style(Style::default().fg(Color::White));

    let messages_paragraph = Paragraph::new(visible_lines_slice)
        .block(messages_block);

    frame.render_widget(messages_paragraph, area);

    // Draw scrollbar if there's content to scroll
    if total_lines > visible_lines as usize {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("^"))
            .end_symbol(Some("v"));

        let mut scrollbar_state = ScrollbarState::new(max_scroll)
            .position(scroll_offset);

        // Scrollbar area inside the block
        let scrollbar_area = Rect {
            x: area.x + area.width - 1,
            y: area.y + 1,
            width: 1,
            height: area.height.saturating_sub(2),
        };

        frame.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
    }
}

/// Wrap text to fit within a given width
fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![text.to_string()];
    }

    let mut result = Vec::new();

    for line in text.lines() {
        if line.is_empty() {
            result.push(String::new());
            continue;
        }

        let mut current_line = String::new();
        let mut current_width = 0;

        for word in line.split_whitespace() {
            let word_width = word.chars().count();

            if current_width + word_width + (if current_width > 0 { 1 } else { 0 }) <= max_width {
                if current_width > 0 {
                    current_line.push(' ');
                    current_width += 1;
                }
                current_line.push_str(word);
                current_width += word_width;
            } else {
                // Word doesn't fit on current line
                if !current_line.is_empty() {
                    result.push(current_line);
                    current_line = String::new();
                    current_width = 0;
                }

                // Handle very long words that exceed max_width
                if word_width > max_width {
                    let mut chars = word.chars().peekable();
                    while chars.peek().is_some() {
                        let chunk: String = chars.by_ref().take(max_width).collect();
                        if chars.peek().is_some() {
                            result.push(chunk);
                        } else {
                            current_line = chunk;
                            current_width = current_line.chars().count();
                        }
                    }
                } else {
                    current_line = word.to_string();
                    current_width = word_width;
                }
            }
        }

        if !current_line.is_empty() {
            result.push(current_line);
        }
    }

    if result.is_empty() {
        result.push(String::new());
    }

    result
}


/// Draw multi-line input box
fn draw_input(frame: &mut Frame, app: &App, area: Rect) {
    let input_style = if app.chat.awaiting_response {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::White)
    };

    // Build title based on state
    let title = if app.chat.awaiting_response {
        " Waiting for response... ".to_string()
    } else if app.chat.browsing_history {
        " [History] (Up/Down to navigate) ".to_string()
    } else if app.chat.input_line_count() > 1 {
        format!(" Line {}/{} | Shift+Enter: new line | Enter: send ",
            app.chat.cursor_line_col().0 + 1,
            app.chat.input_line_count())
    } else {
        " > Type message | Shift+Enter: new line | Enter: send | Up: history ".to_string()
    };

    let input_block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .title_alignment(Alignment::Left)
        .border_style(if app.chat.browsing_history {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        });

    // Calculate visible portion of input for scrolling
    let available_height = area.height.saturating_sub(2) as usize;
    let input_lines: Vec<&str> = app.chat.input.lines().collect();
    let _total_input_lines = input_lines.len().max(1);

    let (cursor_line, cursor_col) = app.chat.cursor_line_col();

    // Ensure cursor line is visible
    let visible_start = if cursor_line >= available_height {
        cursor_line - available_height + 1
    } else {
        0
    };

    let visible_text: String = if app.chat.input.is_empty() {
        String::new()
    } else {
        app.chat.input
            .lines()
            .skip(visible_start)
            .take(available_height)
            .collect::<Vec<_>>()
            .join("\n")
    };

    let input = Paragraph::new(visible_text)
        .style(input_style)
        .block(input_block);

    frame.render_widget(input, area);

    // Show cursor position
    if !app.chat.awaiting_response {
        // Calculate cursor position on screen
        let screen_line = (cursor_line - visible_start) as u16;

        frame.set_cursor_position(Position::new(
            area.x + cursor_col as u16 + 1,
            area.y + screen_line + 1,
        ));
    }
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
                    "  Tab/Shift+Tab  Cycle between fields    1-4       Go to view",
                    "  ↑/↓ arrows     Move between fields     0 / H     Home",
                    "  ←/→ arrows     Switch panels           Esc       Back / Close",
                    "  j/k            Navigate lists (vim)",
                    "  Enter          Expand folder / Save    PATTERN SYNTAX",
                    "                                         ──────────────",
                    "  ACTIONS                                **/*      All files",
                    "  ───────                                *.rs      Rust files",
                    "  1              Open Source dropdown    **/*.csv  CSV files (recursive)",
                    "  2              Open Tags dropdown      <name>    Extract field",
                    "  s              Scan new directory",
                    "  Ctrl+S         Save rule               FOCUS INDICATOR",
                    "  t              Run backtest            ──────────────",
                    "  Ctrl+Space     AI assist               █ cursor   = text input active",
                    "                                         ► pointer  = list navigation",
                    "  IN DROPDOWNS",
                    "  ────────────                           GLOBAL",
                    "  /              Filter list             ──────",
                    "  j/k or ↑/↓    Navigate                 ?         This help",
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
                    "  s              Scan new directory      1-4       Go to view",
                    "  n              Create new rule         0 / H     Home",
                    "  Enter          Open Rule Builder       Esc       Back / Close",
                    "  j/k or ↑/↓    Navigate files",
                    "  /              Filter files            GLOBAL",
                    "                                         ──────",
                    "  IN SOURCES/TAGS DROPDOWN               ?         This help",
                    "  ────────────────────────               r         Refresh view",
                    "  /              Filter list             q         Quit",
                    "  s              Scan (in Sources)",
                    "  j/k or ↑/↓    Navigate",
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
            "  j/k or ↑/↓    Navigate parsers        Esc       Back / Close",
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
            "  j/k or ↑/↓    Navigate jobs           1-4       Go to view",
            "  Enter          View job details        0 / H     Home",
            "  P              View pipeline           Esc       Back / Close",
            "  m              Toggle monitor mode",
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
            "  4              Sources view            Alt+A     AI sidebar",
            "  0 / H          Home",
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

    let about_lines = vec![
        Line::from(format!("  Version:    {}", env!("CARGO_PKG_VERSION"))),
        Line::from(format!("  Database:   ~/.casparian_flow/casparian_flow.sqlite3")),
        Line::from(format!("  Config:     ~/.casparian_flow/config.toml")),
    ];
    let about = Paragraph::new(about_lines).block(about_block);
    frame.render_widget(about, content_chunks[2]);

    // Footer
    let footer_text = if app.settings.editing {
        "[Enter] Save  [Esc] Cancel"
    } else {
        "[↑↓/jk] Navigate  [Tab] Category  [Enter] Edit/Toggle  [Esc] Close"
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
            database: None,
            api_key: None,
            model: "test".into(),
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

        // Card names per spec: Discover, Parser Bench, Jobs, Sources
        assert!(content.contains("Discover"));
        assert!(content.contains("Parser"));  // "Parser Bench"
        assert!(content.contains("Jobs"));
        assert!(content.contains("Sources"));
    }

    #[test]
    fn test_draw_discover_screen() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;
        
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

        assert!(content.contains("FILES"));
        assert!(content.contains("test_file.csv"));
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

    #[test]
    fn test_wrap_text_basic() {
        let text = "hello world this is a test";
        let wrapped = wrap_text(text, 10);
        assert_eq!(wrapped, vec!["hello", "world this", "is a test"]);
    }

    #[test]
    fn test_wrap_text_multiline() {
        let text = "line one\nline two";
        let wrapped = wrap_text(text, 20);
        assert_eq!(wrapped, vec!["line one", "line two"]);
    }

    #[test]
    fn test_wrap_text_long_word() {
        let text = "supercalifragilisticexpialidocious";
        let wrapped = wrap_text(text, 10);
        assert_eq!(wrapped.len(), 4); // Should be split into chunks
    }

    #[test]
    fn test_wrap_text_empty() {
        let wrapped = wrap_text("", 10);
        assert_eq!(wrapped, vec![""]);
    }
}
