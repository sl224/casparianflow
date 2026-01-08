//! UI rendering for the TUI

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
};

use crate::cli::output::format_number;
use super::app::{App, DiscoverFocus, JobStatus, MessageRole, TuiMode};

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

/// Truncate tags to fit within max_width.
/// Example: ["sales", "data", "archive"] -> "[sales, da...]"
fn truncate_tags(tags: &[String], max_width: usize) -> String {
    if tags.is_empty() {
        return String::new();
    }

    let joined = tags.join(", ");
    if joined.chars().count() + 2 <= max_width {
        // +2 for []
        format!("[{}]", joined)
    } else if max_width <= 5 {
        // Not enough room for anything meaningful
        "[...]".to_string()
    } else {
        // Truncate and add ellipsis: [tag1, t...]
        let content_width = max_width - 5; // Room for "[" and "...]"
        let truncated: String = joined.chars().take(content_width).collect();
        format!("[{}...]", truncated)
    }
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
        TuiMode::Process => draw_placeholder_screen(frame, main_area, "Process", "Run parsers on discovered files", "Alt+P"),
        TuiMode::Inspect => draw_inspect_screen(frame, app, main_area),
        TuiMode::Jobs => draw_jobs_screen(frame, app, main_area),
    }

    // Draw Sidebar
    if let Some(area) = sidebar_area {
        draw_sidebar(frame, app, area);
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
    let footer = Paragraph::new(" [Arrow keys/hjkl] Navigate  [Enter] Select  [1-4] Quick select  [Ctrl+C] Quit ")
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

    // Card definitions: (title, description, stats, shortcut, number)
    let cards: [(&str, &str, String, &str, &str); 4] = [
        (
            "Discover",
            "Find and tag files for processing",
            format!(
                "{} files, {} sources",
                app.home.stats.file_count, app.home.stats.source_count
            ),
            "Alt+D",
            "1",
        ),
        (
            "Process",
            "Run parsers on discovered files",
            format!(
                "{} parsers, {} paused",
                app.home.stats.parser_count, app.home.stats.paused_parsers
            ),
            "Alt+P",
            "2",
        ),
        (
            "Inspect",
            "View and query output data",
            "Query tables and view results".to_string(),
            "Alt+I",
            "3",
        ),
        (
            "Jobs",
            "Manage job queue and workers",
            format!(
                "{} running, {} pending, {} failed",
                app.home.stats.running_jobs,
                app.home.stats.pending_jobs,
                app.home.stats.failed_jobs
            ),
            "Alt+J",
            "4",
        ),
    ];

    for (i, card_area) in areas.iter().enumerate() {
        let (title, desc, stats, shortcut, number) = &cards[i];
        let is_selected = app.home.selected_card == i;

        draw_card(frame, *card_area, title, desc, stats, shortcut, number, is_selected);
    }
}

/// Draw a single card
fn draw_card(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    description: &str,
    stats: &str,
    shortcut: &str,
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
        .title(format!(" [{}] {} [{}] ", number, title, shortcut))
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
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title/Filter/Scan Input
            Constraint::Min(0),    // Main content (Sidebar + Files)
            Constraint::Length(3), // Footer/Status
        ])
        .split(area);

    // Get filtered files using proper glob matching from App
    let total_files = app.discover.files.len();
    let filtered = app.filtered_files();
    let filtered_count = filtered.len();

    // Title / Filter / Scan Input / Tag Input / Create Source / Bulk Tag / Rule Creation Bar
    let (title_text, title_style) = if app.discover.is_creating_rule {
        (
            format!(" Save filter '{}' as rule - Tag: {}_ ", app.discover.filter, app.discover.rule_tag_input),
            Style::default().fg(Color::Blue).bold(),
        )
    } else if app.discover.is_bulk_tagging {
        let checkbox = if app.discover.bulk_tag_save_as_rule { "[x]" } else { "[ ]" };
        (
            format!(" Tag {} files: {}_ {} Save as rule ", filtered_count, app.discover.bulk_tag_input, checkbox),
            Style::default().fg(Color::Yellow).bold(),
        )
    } else if app.discover.is_creating_source {
        let dir_name = app.discover.pending_source_path.as_ref()
            .map(|p| std::path::Path::new(p)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| p.clone()))
            .unwrap_or_default();
        (
            format!(" Create source from '{}' - Name: {}_ ", dir_name, app.discover.source_name_input),
            Style::default().fg(Color::Blue).bold(),
        )
    } else if app.discover.is_tagging {
        let file_name = app.discover.files
            .get(app.discover.selected)
            .map(|f| std::path::Path::new(&f.path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| f.path.clone()))
            .unwrap_or_default();
        (
            format!(" Tag '{}': {}_ ", file_name, app.discover.tag_input),
            Style::default().fg(Color::Magenta).bold(),
        )
    } else if app.discover.is_entering_path {
        (
            format!(" Scan folder: {}_ ", app.discover.scan_path_input),
            Style::default().fg(Color::Green).bold(),
        )
    } else if app.discover.is_filtering {
        (
            format!(" Filter: {}_ ({} of {}) ", app.discover.filter, filtered_count, total_files),
            Style::default().fg(Color::Yellow).bold(),
        )
    } else if let Some(ref err) = app.discover.scan_error {
        (
            format!("  {}", err),
            Style::default().fg(Color::Red),
        )
    } else if !app.discover.filter.is_empty() {
        (
            format!(" Discover - {} of {} files (filter: '{}') ", filtered_count, total_files, app.discover.filter),
            Style::default().fg(Color::Cyan).bold(),
        )
    } else {
        (
            format!(" Discover - {} files ", total_files),
            Style::default().fg(Color::Cyan).bold(),
        )
    };

    let title = Paragraph::new(title_text)
        .style(title_style)
        .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(title, chunks[0]);

    // Main content: Sidebar (20%) | Files (80%)
    let h_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(18),        // Sidebar min width
            Constraint::Percentage(80), // Files area
        ])
        .split(chunks[1]);

    // Draw Sidebar (Sources + Rules)
    draw_discover_sidebar(frame, app, h_chunks[0]);

    // Files area: File List vs Preview split
    let files_area = h_chunks[1];
    let main_chunks = if app.discover.preview_open {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(files_area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(100)])
            .split(files_area)
    };

    // File List - pass filtered files
    draw_file_list(frame, app, &filtered, main_chunks[0]);

    // Preview Pane (if open)
    if app.discover.preview_open {
        draw_file_preview(frame, app, &filtered, main_chunks[1]);
    }

    // Footer - context-aware based on focus and state
    let (footer_text, footer_style) = get_discover_footer(app, filtered_count);
    let footer = Paragraph::new(footer_text)
        .style(footer_style)
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::TOP));
    frame.render_widget(footer, chunks[2]);
}

/// Get contextual footer text and style for Discover mode
fn get_discover_footer(app: &App, filtered_count: usize) -> (String, Style) {
    // Status message takes priority
    if let Some((ref msg, is_error)) = app.discover.status_message {
        return (
            format!(" {} ", msg),
            if is_error {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::Green)
            },
        );
    }

    // Dialog-specific footers
    if app.discover.is_creating_rule {
        return (" [Enter] Save rule  [Esc] Cancel ".to_string(), Style::default().fg(Color::DarkGray));
    }
    if app.discover.is_bulk_tagging {
        return (" [Enter] Apply tag  [Space] Toggle rule  [Esc] Cancel ".to_string(), Style::default().fg(Color::DarkGray));
    }
    if app.discover.is_creating_source {
        return (" [Enter] Create source  [Esc] Cancel ".to_string(), Style::default().fg(Color::DarkGray));
    }
    if app.discover.is_tagging {
        return (" [Enter] Apply tag  [Tab] Autocomplete  [Esc] Cancel ".to_string(), Style::default().fg(Color::DarkGray));
    }
    if app.discover.is_entering_path {
        return (" [Enter] Scan  [Esc] Cancel ".to_string(), Style::default().fg(Color::DarkGray));
    }
    if app.discover.is_filtering {
        return (
            format!(" [Enter] Done  [t] Tag {} files  [R] Save as rule  [Esc] Clear ", filtered_count),
            Style::default().fg(Color::DarkGray),
        );
    }

    // Focus-based footers with new navigation model
    match app.discover.focus {
        DiscoverFocus::Files => {
            if !app.discover.filter.is_empty() {
                (
                    format!(" [t] Tag {} files  [R] Rule  [/] Edit  [Esc] Clear  │ 1:Src 2:Rules ", filtered_count),
                    Style::default().fg(Color::DarkGray),
                )
            } else {
                (" [/] Filter  [t] Tag  [Tab] Preview  [s] Scan  │ 1:Sources 2:Rules ".to_string(), Style::default().fg(Color::DarkGray))
            }
        }
        DiscoverFocus::Sources => {
            (" [Tab] Select→Files  [Enter] Load  [n] New  [d] Delete  │ 2:Rules 3:Files ".to_string(), Style::default().fg(Color::DarkGray))
        }
        DiscoverFocus::Rules => {
            (" [Tab] Filter→Files  [Enter] Edit  [n] New  [d] Delete  │ 1:Sources 3:Files ".to_string(), Style::default().fg(Color::DarkGray))
        }
    }
}

/// Draw the Discover mode sidebar with Sources and Rules sections (scrollable)
fn draw_discover_sidebar(frame: &mut Frame, app: &App, area: Rect) {
    // Dynamic layout based on dropdown state
    let sources_open = app.discover.sources_dropdown_open;
    let rules_open = app.discover.rules_dropdown_open;

    // Calculate heights: collapsed = 3 lines (1 content + 2 border), expanded = remaining space
    let (sources_constraint, rules_constraint) = match (sources_open, rules_open) {
        (true, false) => (Constraint::Min(10), Constraint::Length(3)),
        (false, true) => (Constraint::Length(3), Constraint::Min(10)),
        (true, true) => (Constraint::Percentage(50), Constraint::Percentage(50)),
        (false, false) => (Constraint::Length(3), Constraint::Length(3)),
    };

    let v_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([sources_constraint, rules_constraint])
        .split(area);

    // === SOURCES DROPDOWN ===
    draw_sources_dropdown(frame, app, v_chunks[0]);

    // === RULES DROPDOWN ===
    draw_rules_dropdown(frame, app, v_chunks[1]);
}

/// Draw the Sources dropdown (collapsed or expanded)
fn draw_sources_dropdown(frame: &mut Frame, app: &App, area: Rect) {
    let is_open = app.discover.sources_dropdown_open;
    let is_focused = app.discover.focus == DiscoverFocus::Sources;

    let style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    if is_open {
        // Expanded: filter input + list
        let inner_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .margin(1)
            .split(area);

        // Filter input line
        let filter_text = format!("Filter: {}█", app.discover.sources_filter);
        let filter_line = Paragraph::new(filter_text)
            .style(Style::default().fg(Color::Yellow));
        frame.render_widget(filter_line, inner_chunks[0]);

        // Filtered list
        let filtered: Vec<_> = app.discover.sources.iter()
            .enumerate()
            .filter(|(_, s)| {
                app.discover.sources_filter.is_empty()
                    || s.name.to_lowercase().contains(&app.discover.sources_filter.to_lowercase())
            })
            .collect();

        let mut lines: Vec<Line> = Vec::new();
        let visible_height = inner_chunks[1].height as usize;

        if filtered.is_empty() {
            lines.push(Line::from(Span::styled(
                "No matches",
                Style::default().fg(Color::DarkGray).italic(),
            )));
        } else {
            // Find scroll offset based on preview position
            let preview_idx = app.discover.preview_source.unwrap_or(app.discover.selected_source);
            let preview_pos = filtered.iter().position(|(i, _)| *i == preview_idx).unwrap_or(0);
            let scroll_offset = if preview_pos >= visible_height {
                preview_pos - visible_height + 1
            } else {
                0
            };

            for (i, source) in filtered.iter().skip(scroll_offset).take(visible_height) {
                let is_preview = app.discover.preview_source == Some(*i);
                let is_selected = *i == app.discover.selected_source;
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
            .border_style(style)
            .title(Span::styled(" [1] SOURCE ▲ ", style));
        frame.render_widget(block, area);
    } else {
        // Collapsed: show selected source
        let selected_text = if app.discover.sources.is_empty() {
            "▼ No sources (press 's')".to_string()
        } else if let Some(source) = app.discover.sources.get(app.discover.selected_source) {
            format!("▼ {} ({})", source.name, source.file_count)
        } else {
            "▼ Select source".to_string()
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(style)
            .title(Span::styled(" [1] ", style));

        let content = Paragraph::new(selected_text)
            .style(if is_focused { Style::default().fg(Color::Cyan) } else { Style::default().fg(Color::Gray) })
            .block(block);

        frame.render_widget(content, area);
    }
}

/// Draw the Rules dropdown (collapsed or expanded)
fn draw_rules_dropdown(frame: &mut Frame, app: &App, area: Rect) {
    let is_open = app.discover.rules_dropdown_open;
    let is_focused = app.discover.focus == DiscoverFocus::Rules;

    let style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    if is_open {
        // Expanded: filter input + list
        let inner_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .margin(1)
            .split(area);

        // Filter input line
        let filter_text = format!("Filter: {}█", app.discover.rules_filter);
        let filter_line = Paragraph::new(filter_text)
            .style(Style::default().fg(Color::Yellow));
        frame.render_widget(filter_line, inner_chunks[0]);

        // Filtered list
        let filtered: Vec<_> = app.discover.rules.iter()
            .enumerate()
            .filter(|(_, r)| {
                app.discover.rules_filter.is_empty()
                    || r.pattern.to_lowercase().contains(&app.discover.rules_filter.to_lowercase())
                    || r.tag.to_lowercase().contains(&app.discover.rules_filter.to_lowercase())
            })
            .collect();

        let mut lines: Vec<Line> = Vec::new();
        let visible_height = inner_chunks[1].height as usize;

        // "All files" option at top
        let no_rule_selected = app.discover.preview_rule.is_none();
        let all_files_style = if no_rule_selected {
            Style::default().fg(Color::White).bold()
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let all_prefix = if no_rule_selected { "► " } else { "  " };
        lines.push(Line::from(Span::styled(
            format!("{}(All files)", all_prefix),
            all_files_style,
        )));

        if filtered.is_empty() && !app.discover.rules.is_empty() {
            lines.push(Line::from(Span::styled(
                "  No matches",
                Style::default().fg(Color::DarkGray).italic(),
            )));
        } else {
            // Find scroll offset based on preview position
            let scroll_offset = if let Some(preview_idx) = app.discover.preview_rule {
                let preview_pos = filtered.iter().position(|(i, _)| *i == preview_idx).unwrap_or(0);
                if preview_pos + 1 >= visible_height { // +1 for "All files" row
                    preview_pos + 2 - visible_height
                } else {
                    0
                }
            } else {
                0
            };

            for (i, rule) in filtered.iter().skip(scroll_offset).take(visible_height.saturating_sub(1)) {
                let is_preview = app.discover.preview_rule == Some(*i);
                let is_selected = app.discover.selected_rule == Some(*i);
                let prefix = if is_preview { "► " } else { "  " };

                let item_style = if is_preview {
                    Style::default().fg(Color::White).bold()
                } else if is_selected {
                    Style::default().fg(Color::Magenta)
                } else {
                    Style::default().fg(Color::Gray)
                };

                let text = format!("{}{} → {}", prefix, rule.pattern, rule.tag);
                lines.push(Line::from(Span::styled(text, item_style)));
            }
        }

        let list = Paragraph::new(lines);
        frame.render_widget(list, inner_chunks[1]);

        // Border with title
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(style)
            .title(Span::styled(" [2] RULE ▲ ", style));
        frame.render_widget(block, area);
    } else {
        // Collapsed: show selected rule or "All files"
        let selected_text = if let Some(rule_idx) = app.discover.selected_rule {
            if let Some(rule) = app.discover.rules.get(rule_idx) {
                format!("▼ {} → {}", rule.pattern, rule.tag)
            } else {
                "▼ All files".to_string()
            }
        } else {
            "▼ All files".to_string()
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(style)
            .title(Span::styled(" [2] ", style));

        let content = Paragraph::new(selected_text)
            .style(if is_focused { Style::default().fg(Color::Cyan) } else { Style::default().fg(Color::Gray) })
            .block(block);

        frame.render_widget(content, area);
    }
}

fn draw_file_list(frame: &mut Frame, app: &App, filtered_files: &[&super::app::FileInfo], area: Rect) {
    let mut lines: Vec<Line> = Vec::new();

    // Check if file list is focused
    let is_focused = app.discover.focus == DiscoverFocus::Files;

    if filtered_files.is_empty() {
        // Helpful empty state
        lines.push(Line::from(""));
        if app.discover.files.is_empty() {
            lines.push(Line::from(Span::styled(
                "  No files loaded",
                Style::default().fg(Color::DarkGray),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  Press [s] to scan a folder",
                Style::default().fg(Color::Cyan),
            )));
            lines.push(Line::from(Span::styled(
                "  Press [r] to load from Scout DB",
                Style::default().fg(Color::Cyan),
            )));
        } else {
            lines.push(Line::from(Span::styled(
                "  No files match the current filter",
                Style::default().fg(Color::DarkGray),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  Press [Esc] to clear filter",
                Style::default().fg(Color::Cyan),
            )));
        }
    } else {
        // Calculate column widths:
        // Icon: 2 chars (emoji + space)
        // Size: 8 chars (fixed)
        // Tags: 12 chars max
        // Path: remaining space
        let size_width = 8;
        let tag_width = 12;
        let icon_width = 3; // icon + space
        let padding = 4; // some breathing room
        let path_width = area.width.saturating_sub((size_width + tag_width + icon_width + padding) as u16) as usize;

        for (i, file) in filtered_files.iter().enumerate() {
            let is_selected = i == app.discover.selected;
            let style = if is_selected && is_focused {
                Style::default().fg(Color::White).bold().bg(Color::DarkGray)
            } else if is_selected {
                Style::default().fg(Color::Cyan).bg(Color::Black)
            } else {
                Style::default().fg(Color::Gray)
            };

            let icon = if file.is_dir { "📁" } else { "📄" };

            // Use rel_path for display if available, otherwise use path
            let display_path_source = if !file.rel_path.is_empty() {
                &file.rel_path
            } else {
                &file.path
            };

            // Truncate path from START (show end of path - the most relevant part)
            let display_path = truncate_path_start(display_path_source, path_width);

            // Format size (fixed 8 chars)
            let size_str = format_size(file.size);

            // Truncate tags (max 12 chars)
            let tags_str = truncate_tags(&file.tags, tag_width);

            // Build line with fixed columns
            let content = format!(
                "{} {:<path_w$} {:>8} {:>tag_w$}",
                icon,
                display_path,
                size_str,
                tags_str,
                path_w = path_width,
                tag_w = tag_width
            );
            lines.push(Line::from(Span::styled(content, style)));
        }
    }

    let file_count = filtered_files.len();
    let title = format!(" [3] FILES ({}) ", file_count);
    let title_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .borders(Borders::NONE)
        .title(Span::styled(title, title_style));

    let list = Paragraph::new(lines).block(block);
    frame.render_widget(list, area);
}

fn draw_file_preview(frame: &mut Frame, app: &App, filtered_files: &[&super::app::FileInfo], area: Rect) {
    let content = if let Some(file) = filtered_files.get(app.discover.selected) {
         format!(
            "Path: {}\nSize: {}\nModified: {}\n\n[Preview functionality coming soon]",
            file.path,
            file.size,
            file.modified.format("%Y-%m-%d %H:%M:%S")
        )
    } else {
        "Select a file to preview".to_string()
    };

    let block = Block::default()
        .borders(Borders::LEFT)
        .title(" Preview ");
    
    let preview = Paragraph::new(content).block(block);
    frame.render_widget(preview, area);
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

/// Draw a placeholder screen for modes not yet implemented
fn draw_placeholder_screen(frame: &mut Frame, area: Rect, title: &str, description: &str, shortcut: &str) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Title
            Constraint::Min(0),     // Content
            Constraint::Length(3),  // Footer
        ])
        .split(area);

    // Title
    let title_widget = Paragraph::new(format!(" {} Mode ", title))
        .style(Style::default().fg(Color::Cyan).bold())
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(title_widget, chunks[0]);

    // Content
    let content = format!(
        "\n\n{}\n\n\n(Coming Soon)\n\nThis mode will allow you to {}.\n\nPress Esc or Alt+H to return to Home.",
        description,
        description.to_lowercase()
    );
    let content_widget = Paragraph::new(content)
        .style(Style::default().fg(Color::Gray))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).title(format!(" {} [{}] ", title, shortcut)));
    frame.render_widget(content_widget, chunks[1]);

    // Footer
    let footer = Paragraph::new(" [Esc/Alt+H] Home  [Alt+D] Discover  [Alt+P] Process  [Alt+I] Inspect  [Alt+J] Jobs ")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::TOP));
    frame.render_widget(footer, chunks[2]);
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

/// Draw the Jobs mode screen - view and manage job queue
fn draw_jobs_screen(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Title
            Constraint::Min(0),     // Content
            Constraint::Length(3),  // Footer
        ])
        .split(area);

    // Title with filter info
    let filter_text = match app.jobs_state.status_filter {
        Some(JobStatus::Pending) => " (Pending only)",
        Some(JobStatus::Running) => " (Running only)",
        Some(JobStatus::Completed) => " (Completed only)",
        Some(JobStatus::Failed) => " (Failed only)",
        None => "",
    };

    let title = Paragraph::new(format!(" Jobs Mode - Queue Manager{} ", filter_text))
        .style(Style::default().fg(Color::Cyan).bold())
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(title, chunks[0]);

    // Main content: job list (left) + detail pane (right)
    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    // Jobs list
    draw_jobs_list(frame, app, content_chunks[0]);

    // Job detail pane
    draw_job_detail(frame, app, content_chunks[1]);

    // Footer with shortcuts
    let footer = Paragraph::new(" [j/k] Navigate  [r] Retry  [c] Cancel  [0-4] Filter  [Esc] Home ")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::TOP));
    frame.render_widget(footer, chunks[2]);
}

/// Draw the jobs list in Jobs mode
fn draw_jobs_list(frame: &mut Frame, app: &App, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();
    let jobs = app.jobs_state.filtered_jobs();

    if jobs.is_empty() {
        lines.push(Line::from(Span::styled(
            "No jobs found.",
            Style::default().fg(Color::DarkGray),
        )));
        if app.jobs_state.status_filter.is_some() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Press 0 to clear filter.",
                Style::default().fg(Color::DarkGray),
            )));
        }
    } else {
        for (i, job) in jobs.iter().enumerate() {
            let is_selected = i == app.jobs_state.selected_index;

            let status_symbol = job.status.symbol();
            let status_color = match job.status {
                JobStatus::Pending => Color::Yellow,
                JobStatus::Running => Color::Blue,
                JobStatus::Completed => Color::Green,
                JobStatus::Failed => Color::Red,
            };

            let line_style = if is_selected {
                Style::default().bold()
            } else {
                Style::default()
            };

            let prefix = if is_selected { "> " } else { "  " };

            // Job ID and status
            lines.push(Line::from(vec![
                Span::styled(prefix, line_style),
                Span::styled(status_symbol, Style::default().fg(status_color)),
                Span::styled(
                    format!(" #{} - {}", job.id, job.parser_name),
                    if is_selected {
                        Style::default().fg(Color::Cyan).bold()
                    } else {
                        Style::default().fg(Color::White)
                    },
                ),
            ]));

            // File path (truncated) - use chars to handle UTF-8 safely
            let path = {
                let char_count = job.file_path.chars().count();
                if char_count > 40 {
                    let suffix: String = job.file_path.chars().skip(char_count - 37).collect();
                    format!("...{}", suffix)
                } else {
                    job.file_path.clone()
                }
            };

            lines.push(Line::from(Span::styled(
                format!("    {}", path),
                Style::default().fg(Color::DarkGray),
            )));

            lines.push(Line::from("")); // Spacing
        }
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" Jobs ({}) ", jobs.len()))
        .border_style(Style::default().fg(Color::White));

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

/// Draw job detail pane in Jobs mode
fn draw_job_detail(frame: &mut Frame, app: &App, area: Rect) {
    let jobs = app.jobs_state.filtered_jobs();

    let content = if let Some(job) = jobs.get(app.jobs_state.selected_index) {
        let mut detail = format!(
            "Job ID: {}\nStatus: {}\nParser: {}\nRetries: {}\nCreated: {}\n\nFile:\n{}\n",
            job.id,
            job.status.as_str(),
            job.parser_name,
            job.retry_count,
            job.created_at.format("%Y-%m-%d %H:%M:%S"),
            job.file_path
        );

        if let Some(ref error) = job.error_message {
            detail.push_str(&format!("\nError:\n{}", error));
        }

        if job.status == JobStatus::Failed {
            detail.push_str("\n\nPress 'r' to retry this job.");
        } else if job.status == JobStatus::Running {
            detail.push_str("\n\nPress 'c' to cancel this job.");
        }

        detail
    } else {
        "Select a job to view details.".to_string()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Details ")
        .border_style(Style::default().fg(Color::White));

    let paragraph = Paragraph::new(content)
        .block(block)
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
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


#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::tui::app::{JobInfo, JobsState};
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

    /// Test that UTF-8 paths are truncated safely without panicking
    #[test]
    fn test_utf8_path_truncation() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut app = App::new(test_args());
        app.mode = TuiMode::Jobs;

        // Create a job with a long UTF-8 path containing multi-byte characters
        // This would panic with byte-based truncation
        let utf8_path = "/data/文件夹/数据/很长的路径名称/包含中文字符/测试文件.csv";
        app.jobs_state = JobsState {
            jobs: vec![JobInfo {
                id: 1,
                file_path: utf8_path.to_string(),
                parser_name: "test_parser".into(),
                status: JobStatus::Running,
                retry_count: 0,
                error_message: None,
                created_at: Local::now(),
            }],
            selected_index: 0,
            status_filter: None,
        };

        // This should not panic - the bug was byte-based truncation on UTF-8
        let result = terminal.draw(|f| draw(f, &app));
        assert!(result.is_ok(), "Rendering UTF-8 path should not panic");
    }

    /// Test with emoji in path (4-byte UTF-8 characters)
    #[test]
    fn test_emoji_path_truncation() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut app = App::new(test_args());
        app.mode = TuiMode::Jobs;

        // Emoji are 4-byte UTF-8 sequences
        let emoji_path = "/data/📁/documents/📊/reports/📈/analysis/🔬/results/data.csv";
        app.jobs_state = JobsState {
            jobs: vec![JobInfo {
                id: 1,
                file_path: emoji_path.to_string(),
                parser_name: "test_parser".into(),
                status: JobStatus::Pending,
                retry_count: 0,
                error_message: None,
                created_at: Local::now(),
            }],
            selected_index: 0,
            status_filter: None,
        };

        let result = terminal.draw(|f| draw(f, &app));
        assert!(result.is_ok(), "Rendering emoji path should not panic");
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

        assert!(content.contains("Discover"));
        assert!(content.contains("Process"));
        assert!(content.contains("Inspect"));
        assert!(content.contains("Jobs"));
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
    fn test_draw_placeholder_modes() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        // Test Process mode placeholder
        let mut app = App::new(test_args());
        app.mode = TuiMode::Process;
        terminal.draw(|f| draw(f, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = buffer
            .content
            .iter()
            .map(|cell| cell.symbol().chars().next().unwrap_or(' '))
            .collect();

        assert!(content.contains("Process"));
        assert!(content.contains("Coming Soon"));
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
