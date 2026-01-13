//! UI rendering for the TUI

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
};

use crate::cli::output::format_number;
use super::app::{App, DiscoverFocus, DiscoverViewState, JobStatus, MessageRole, ParserBenchView, ParserHealth, TuiMode};

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
        TuiMode::ParserBench => draw_parser_bench_screen(frame, app, main_area),
        TuiMode::Inspect => draw_inspect_screen(frame, app, main_area),
        TuiMode::Jobs => draw_jobs_screen(frame, app, main_area),
    }

    // Draw Sidebar
    if let Some(area) = sidebar_area {
        draw_sidebar(frame, app, area);
    }

    // Draw Help Overlay (on top of everything)
    if app.show_help {
        draw_help_overlay(frame, area);
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
    let (title_text, title_style) = match app.discover.view_state {
        DiscoverViewState::RuleCreation => (
            format!(" Save filter '{}' as rule - Tag: {}_ ", app.discover.filter, app.discover.rule_tag_input),
            Style::default().fg(Color::Blue).bold(),
        ),
        DiscoverViewState::BulkTagging => {
            let checkbox = if app.discover.bulk_tag_save_as_rule { "[x]" } else { "[ ]" };
            (
                format!(" Tag {} files: {}_ {} Save as rule ", filtered_count, app.discover.bulk_tag_input, checkbox),
                Style::default().fg(Color::Yellow).bold(),
            )
        }
        DiscoverViewState::CreatingSource => {
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
        }
        DiscoverViewState::Tagging => {
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
        }
        DiscoverViewState::EnteringPath => (
            " Discover - Adding Source ".to_string(),
            Style::default().fg(Color::Cyan).bold(),
        ),
        DiscoverViewState::Filtering => (
            format!(" Filter: {}_ ({} of {}) ", app.discover.filter, filtered_count, total_files),
            Style::default().fg(Color::Yellow).bold(),
        ),
        _ => {
            if let Some(ref err) = app.discover.scan_error {
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
            }
        }
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

    // File List or Glob Explorer
    if app.discover.glob_explorer.is_some() {
        // Glob Explorer is active - show hierarchical folder view
        draw_glob_explorer(frame, app, main_chunks[0]);
    } else {
        // Normal file list
        draw_file_list(frame, app, &filtered, main_chunks[0]);
    }

    // Preview Pane (if open, only for normal file list)
    if app.discover.preview_open && app.discover.glob_explorer.is_none() {
        draw_file_preview(frame, app, &filtered, main_chunks[1]);
    }

    // Footer - context-aware based on focus and state
    let (footer_text, footer_style) = get_discover_footer(app, filtered_count);
    let footer = Paragraph::new(footer_text)
        .style(footer_style)
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::TOP));
    frame.render_widget(footer, chunks[2]);

    // Dialogs (overlays) - render based on view_state
    match app.discover.view_state {
        DiscoverViewState::RulesManager => draw_rules_manager_dialog(frame, app, area),
        DiscoverViewState::RuleCreation => draw_rule_creation_dialog(frame, app, area),
        // Sources Manager dialogs (spec v1.7)
        DiscoverViewState::SourcesManager => draw_sources_manager_dialog(frame, app, area),
        DiscoverViewState::SourceEdit => draw_source_edit_dialog(frame, app, area),
        DiscoverViewState::SourceDeleteConfirm => draw_source_delete_confirm_dialog(frame, app, area),
        // Scanning progress dialog
        DiscoverViewState::Scanning => draw_scanning_dialog(frame, app, area),
        // Add Source path input dialog
        DiscoverViewState::EnteringPath => draw_add_source_dialog(frame, app, area),
        _ => {}
    }
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

    // Dialog/input-specific footers based on view state
    match app.discover.view_state {
        DiscoverViewState::RuleCreation => {
            return (" [Enter] Save rule  [Esc] Cancel ".to_string(), Style::default().fg(Color::DarkGray));
        }
        DiscoverViewState::BulkTagging => {
            return (" [Enter] Apply tag  [Space] Toggle rule  [Esc] Cancel ".to_string(), Style::default().fg(Color::DarkGray));
        }
        DiscoverViewState::CreatingSource => {
            return (" [Enter] Create source  [Esc] Cancel ".to_string(), Style::default().fg(Color::DarkGray));
        }
        DiscoverViewState::Tagging => {
            return (" [Enter] Apply tag  [Tab] Autocomplete  [Esc] Cancel ".to_string(), Style::default().fg(Color::DarkGray));
        }
        DiscoverViewState::EnteringPath => {
            return (" [Enter] Scan  [Esc] Cancel ".to_string(), Style::default().fg(Color::DarkGray));
        }
        DiscoverViewState::Scanning => {
            return (" Scanning... [0] Home  [4] Jobs (scan continues)  [Esc] Cancel ".to_string(), Style::default().fg(Color::Yellow));
        }
        DiscoverViewState::Filtering => {
            return (
                format!(" [Enter] Done  [t] Tag {} files  [R] Save as rule  [Esc] Clear ", filtered_count),
                Style::default().fg(Color::DarkGray),
            );
        }
        // Sources Manager dialogs (spec v1.7)
        DiscoverViewState::SourcesManager => {
            return (" [n] New  [e] Edit  [d] Delete  [r] Rescan  [Esc] Close ".to_string(), Style::default().fg(Color::DarkGray));
        }
        DiscoverViewState::SourceEdit => {
            return (" [Enter] Save  [Esc] Cancel ".to_string(), Style::default().fg(Color::DarkGray));
        }
        DiscoverViewState::SourceDeleteConfirm => {
            return (" [Enter/y] Confirm delete  [Esc/n] Cancel ".to_string(), Style::default().fg(Color::Red));
        }
        _ => {}
    }

    // Focus-based footers with new navigation model
    match app.discover.focus {
        DiscoverFocus::Files => {
            // Glob Explorer mode has its own footer
            if let Some(ref explorer) = app.discover.glob_explorer {
                if explorer.pattern_editing {
                    return (
                        " [Enter] Done  [Esc] Cancel  │ Type glob pattern (e.g., *.csv) ".to_string(),
                        Style::default().fg(Color::Yellow),
                    );
                }
                return (
                    " [j/k] Navigate  [Enter] Drill in  [Backspace] Back  [/] Filter  [g/Esc] Exit ".to_string(),
                    Style::default().fg(Color::Cyan),
                );
            }
            // Normal file list mode
            if !app.discover.filter.is_empty() {
                (
                    format!(" [t] Tag {} files  [Esc] Clear  │ [R] Rules [M] Sources │ 1:Source 2:Tags ", filtered_count),
                    Style::default().fg(Color::DarkGray),
                )
            } else {
                (" [g] Explorer  [/] Filter  [t] Tag  [s] Scan  │ [R] Rules [M] Sources │ 1:Source 2:Tags ".to_string(), Style::default().fg(Color::DarkGray))
            }
        }
        DiscoverFocus::Sources => {
            (" [Enter] Select  [↑↓] Navigate  │ [R] Rules [M] Sources │ [Esc] Cancel ".to_string(), Style::default().fg(Color::DarkGray))
        }
        DiscoverFocus::Tags => {
            (" [Enter] Select  [↑↓] Navigate  │ [R] Rules [M] Sources │ [Esc] All files ".to_string(), Style::default().fg(Color::DarkGray))
        }
    }
}

/// Draw the Discover mode sidebar with Sources and Tags sections (scrollable)
fn draw_discover_sidebar(frame: &mut Frame, app: &App, area: Rect) {
    // Dynamic layout based on view state
    let sources_open = app.discover.view_state == DiscoverViewState::SourcesDropdown;
    let tags_open = app.discover.view_state == DiscoverViewState::TagsDropdown;

    // Calculate heights: collapsed = 3 lines (1 content + 2 border), expanded = remaining space
    let (sources_constraint, tags_constraint) = match (sources_open, tags_open) {
        (true, false) => (Constraint::Min(10), Constraint::Length(3)),
        (false, true) => (Constraint::Length(3), Constraint::Min(10)),
        (true, true) => (Constraint::Percentage(50), Constraint::Percentage(50)),
        (false, false) => (Constraint::Length(3), Constraint::Length(3)),
    };

    let v_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([sources_constraint, tags_constraint])
        .split(area);

    // === SOURCES DROPDOWN ===
    draw_sources_dropdown(frame, app, v_chunks[0]);

    // === TAGS DROPDOWN ===
    draw_tags_dropdown(frame, app, v_chunks[1]);
}

/// Draw the Sources dropdown (collapsed or expanded)
fn draw_sources_dropdown(frame: &mut Frame, app: &App, area: Rect) {
    let is_open = app.discover.view_state == DiscoverViewState::SourcesDropdown;
    let is_focused = app.discover.focus == DiscoverFocus::Sources;

    let style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    if is_open {
        // Expanded dropdown with optional filter line
        let is_filtering = app.discover.sources_filtering;

        let inner_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .margin(1)
            .split(area);

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
            let hint_line = Paragraph::new("[/]filter [s]scan [j/k]nav")
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
        let visible_height = inner_chunks[1].height as usize;

        if filtered.is_empty() {
            lines.push(Line::from(Span::styled(
                "No matches",
                Style::default().fg(Color::DarkGray).italic(),
            )));
        } else {
            // Find scroll offset based on preview position
            let preview_idx = app.discover.preview_source.unwrap_or_else(|| app.discover.selected_source_index());
            let preview_pos = filtered.iter().position(|(i, _)| *i == preview_idx).unwrap_or(0);
            let scroll_offset = if preview_pos >= visible_height {
                preview_pos - visible_height + 1
            } else {
                0
            };

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
            .border_style(style)
            .title(Span::styled(" [1] Source ▲ ", style));
        frame.render_widget(block, area);
    } else {
        // Collapsed: show selected source
        let selected_text = if app.discover.sources.is_empty() {
            "No sources (press 's')".to_string()
        } else if let Some(source) = app.discover.sources.get(app.discover.selected_source_index()) {
            format!("{} ({})", source.name, source.file_count)
        } else {
            "Select source...".to_string()
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(style)
            .title(Span::styled(" [1] Source ", style));

        let content = Paragraph::new(selected_text)
            .style(if is_focused { Style::default().fg(Color::Cyan) } else { Style::default().fg(Color::Gray) })
            .block(block);

        frame.render_widget(content, area);
    }
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
            // Show keybinding hints
            let hint_line = Paragraph::new("[/]filter [j/k]nav")
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
            // Find scroll offset based on preview position
            let scroll_offset = if let Some(preview_idx) = app.discover.preview_tag {
                let preview_pos = filtered.iter().position(|(i, _)| *i == preview_idx).unwrap_or(0);
                if preview_pos >= visible_height {
                    preview_pos + 1 - visible_height
                } else {
                    0
                }
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

        // Border with title
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(style)
            .title(Span::styled(" [2] Tags ▲ ", style));
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

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(style)
            .title(Span::styled(" [2] Tags ", style));

        let content = Paragraph::new(selected_text)
            .style(if is_focused { Style::default().fg(Color::Cyan) } else { Style::default().fg(Color::Gray) })
            .block(block);

        frame.render_widget(content, area);
    }
}

/// Draw the Rules Manager dialog as an overlay
fn draw_rules_manager_dialog(frame: &mut Frame, app: &App, area: Rect) {
    use ratatui::widgets::Clear;

    // Center the dialog
    let dialog_width = area.width.min(60);
    let dialog_height = area.height.min(20);
    let x = (area.width.saturating_sub(dialog_width)) / 2;
    let y = (area.height.saturating_sub(dialog_height)) / 2;

    let dialog_area = Rect::new(
        area.x + x,
        area.y + y,
        dialog_width,
        dialog_height,
    );

    // Clear the area behind the dialog
    frame.render_widget(Clear, dialog_area);

    // Dialog block
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
    use ratatui::widgets::Clear;

    // Center the dialog
    let dialog_width = area.width.min(65);
    let dialog_height = area.height.min(20);
    let x = (area.width.saturating_sub(dialog_width)) / 2;
    let y = (area.height.saturating_sub(dialog_height)) / 2;

    let dialog_area = Rect::new(
        area.x + x,
        area.y + y,
        dialog_width,
        dialog_height,
    );

    // Clear the area behind the dialog
    frame.render_widget(Clear, dialog_area);

    // Dialog block
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
    use ratatui::widgets::Clear;

    // Center a smaller dialog
    let dialog_width = area.width.min(50);
    let dialog_height = 8;
    let x = (area.width.saturating_sub(dialog_width)) / 2;
    let y = (area.height.saturating_sub(dialog_height)) / 2;

    let dialog_area = Rect::new(
        area.x + x,
        area.y + y,
        dialog_width,
        dialog_height,
    );

    frame.render_widget(Clear, dialog_area);

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
    use ratatui::widgets::Clear;

    // Center a smaller dialog
    let dialog_width = area.width.min(55);
    let dialog_height = 10;
    let x = (area.width.saturating_sub(dialog_width)) / 2;
    let y = (area.height.saturating_sub(dialog_height)) / 2;

    let dialog_area = Rect::new(
        area.x + x,
        area.y + y,
        dialog_width,
        dialog_height,
    );

    frame.render_widget(Clear, dialog_area);

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
    use ratatui::widgets::Clear;

    // Calculate height based on content
    let suggestions_count = app.discover.path_suggestions.len().min(8);
    let has_suggestions = suggestions_count > 0;
    let has_error = app.discover.scan_error.is_some();

    // Base height + suggestions + error
    let dialog_height = 9
        + if has_suggestions { suggestions_count as u16 + 2 } else { 0 }
        + if has_error { 2 } else { 0 };

    // Center a dialog
    let dialog_width = area.width.min(70);
    let x = (area.width.saturating_sub(dialog_width)) / 2;
    let y = (area.height.saturating_sub(dialog_height)) / 2;

    let dialog_area = Rect::new(
        area.x + x,
        area.y + y,
        dialog_width,
        dialog_height,
    );

    frame.render_widget(Clear, dialog_area);

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
    use ratatui::widgets::Clear;

    // Center a dialog
    let dialog_width = area.width.min(60);
    let dialog_height = 9;
    let x = (area.width.saturating_sub(dialog_width)) / 2;
    let y = (area.height.saturating_sub(dialog_height)) / 2;

    let dialog_area = Rect::new(
        area.x + x,
        area.y + y,
        dialog_width,
        dialog_height,
    );

    // Clear the entire dialog area first
    frame.render_widget(Clear, dialog_area);

    // Animated spinner for title
    let spinner_chars = ["-", "\\", "|", "/"];
    let spinner = spinner_chars[(app.tick_count / 2) as usize % 4];

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
    let files = progress.map(|p| p.files_found).unwrap_or(0);
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
    let line2 = format!("  {} files | {} dirs | {}", files, dirs, time_str);
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

/// Draw the Rule Creation dialog as an overlay (two-field version with live preview)
fn draw_rule_creation_dialog(frame: &mut Frame, app: &App, area: Rect) {
    use ratatui::widgets::Clear;
    use super::app::RuleDialogFocus;

    // Center the dialog
    let dialog_width = area.width.min(70);
    let dialog_height = area.height.min(18);
    let x = (area.width.saturating_sub(dialog_width)) / 2;
    let y = (area.height.saturating_sub(dialog_height)) / 2;

    let dialog_area = Rect::new(
        area.x + x,
        area.y + y,
        dialog_width,
        dialog_height,
    );

    // Clear the area behind the dialog
    frame.render_widget(Clear, dialog_area);

    // Dialog block
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Span::styled(" New Tagging Rule ", Style::default().fg(Color::Cyan).bold()));

    let inner_area = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    // Split inner area: fields + preview + footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Pattern field
            Constraint::Length(3),  // Tag field
            Constraint::Length(1),  // Separator
            Constraint::Min(1),     // Preview
            Constraint::Length(2),  // Footer
        ])
        .split(inner_area);

    // Pattern field
    let pattern_focused = app.discover.rule_dialog_focus == RuleDialogFocus::Pattern;
    let pattern_style = if pattern_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let pattern_block = Block::default()
        .borders(Borders::ALL)
        .border_style(pattern_style)
        .title(Span::styled(" Pattern ", pattern_style));

    let pattern_text = if pattern_focused {
        format!("{}█", app.discover.rule_pattern_input)
    } else {
        app.discover.rule_pattern_input.clone()
    };
    let pattern_para = Paragraph::new(pattern_text)
        .style(Style::default().fg(Color::White))
        .block(pattern_block);
    frame.render_widget(pattern_para, chunks[0]);

    // Tag field
    let tag_focused = app.discover.rule_dialog_focus == RuleDialogFocus::Tag;
    let tag_style = if tag_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let tag_block = Block::default()
        .borders(Borders::ALL)
        .border_style(tag_style)
        .title(Span::styled(" Tag ", tag_style));

    let tag_text = if tag_focused {
        format!("{}█", app.discover.rule_tag_input)
    } else {
        app.discover.rule_tag_input.clone()
    };
    let tag_para = Paragraph::new(tag_text)
        .style(Style::default().fg(Color::White))
        .block(tag_block);
    frame.render_widget(tag_para, chunks[1]);

    // Separator
    let sep = Paragraph::new("─".repeat(inner_area.width.saturating_sub(2) as usize))
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(sep, chunks[2]);

    // Preview section
    let preview_count = app.discover.rule_preview_count;
    let preview_title = if preview_count > 0 {
        format!(" Preview ({} files match) ", preview_count)
    } else if app.discover.rule_pattern_input.is_empty() {
        " Preview (enter a pattern) ".to_string()
    } else {
        " Preview (no matches) ".to_string()
    };

    let mut preview_lines: Vec<Line> = Vec::new();
    if app.discover.rule_preview_files.is_empty() && !app.discover.rule_pattern_input.is_empty() {
        preview_lines.push(Line::from(Span::styled(
            "  No files match this pattern",
            Style::default().fg(Color::DarkGray).italic(),
        )));
    } else {
        for (i, path) in app.discover.rule_preview_files.iter().take(5).enumerate() {
            let display_path = truncate_path_start(path, 50);
            preview_lines.push(Line::from(Span::styled(
                format!("  {}", display_path),
                Style::default().fg(Color::Gray),
            )));
            if i == 4 && preview_count > 5 {
                preview_lines.push(Line::from(Span::styled(
                    format!("  ... and {} more", preview_count - 5),
                    Style::default().fg(Color::DarkGray).italic(),
                )));
            }
        }
    }

    let preview_block = Block::default()
        .borders(Borders::NONE)
        .title(Span::styled(preview_title, Style::default().fg(Color::Yellow)));
    let preview_para = Paragraph::new(preview_lines).block(preview_block);
    frame.render_widget(preview_para, chunks[3]);

    // Footer with keybindings
    let footer = Paragraph::new(Line::from(vec![
        Span::styled(" [Tab]", Style::default().fg(Color::Cyan)),
        Span::raw(" Switch field  "),
        Span::styled("[Enter]", Style::default().fg(Color::Cyan)),
        Span::raw(" Create  "),
        Span::styled("[Esc]", Style::default().fg(Color::Cyan)),
        Span::raw(" Cancel"),
    ]))
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(footer, chunks[4]);
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
        // VIRTUAL SCROLLING: Only render visible rows for performance with large file lists
        // Calculate visible height (area height minus 1 for title line)
        let visible_rows = area.height.saturating_sub(1) as usize;
        let total_files = filtered_files.len();
        let selected = app.discover.selected;

        // Calculate scroll offset to keep selected item visible
        // Try to keep selected item roughly centered, but don't scroll past boundaries
        let scroll_offset = if visible_rows >= total_files {
            0 // All files fit, no scrolling needed
        } else if selected < visible_rows / 2 {
            0 // Near start, show from beginning
        } else if selected > total_files.saturating_sub(visible_rows / 2) {
            total_files.saturating_sub(visible_rows) // Near end, show last visible_rows
        } else {
            selected.saturating_sub(visible_rows / 2) // Center the selection
        };

        let end_idx = (scroll_offset + visible_rows).min(total_files);

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

        // Only iterate over visible files
        for (visible_idx, file) in filtered_files[scroll_offset..end_idx].iter().enumerate() {
            let actual_idx = scroll_offset + visible_idx;
            let is_selected = actual_idx == selected;
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
    let selected = app.discover.selected;

    // Show position indicator for large lists
    let title = if file_count > 100 {
        format!(" [3] FILES ({}/{}) ", selected + 1, format_count(file_count))
    } else {
        format!(" [3] FILES ({}) ", file_count)
    };
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

/// Format large counts with K/M suffixes for readability
fn format_count(count: usize) -> String {
    if count >= 1_000_000 {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    } else if count >= 1_000 {
        format!("{:.1}K", count as f64 / 1_000.0)
    } else {
        count.to_string()
    }
}

/// Draw the Glob Explorer (hierarchical folder view)
fn draw_glob_explorer(frame: &mut Frame, app: &App, area: Rect) {
    use super::app::GlobFileCount;

    let explorer = match &app.discover.glob_explorer {
        Some(e) => e,
        None => return,
    };

    let is_focused = app.discover.focus == DiscoverFocus::Files;

    // Split into three sections: Pattern, Folders, Preview
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Pattern input
            Constraint::Min(5),     // Folders list
            Constraint::Length(6),  // Preview files
        ])
        .split(area);

    // --- Pattern section ---
    let pattern_display = if explorer.pattern.is_empty() {
        "<no filter>".to_string()
    } else {
        explorer.pattern.clone()
    };
    let total_count = match &explorer.total_count {
        GlobFileCount::Exact(n) => format!("{} files", format_count(*n)),
        GlobFileCount::Estimated(n) => format!("~{} files", format_count(*n)),
    };
    let pattern_line = format!(
        "Pattern: {}{}  [{}]",
        pattern_display,
        if !explorer.current_prefix.is_empty() {
            format!("  (in {})", explorer.current_prefix)
        } else {
            String::new()
        },
        total_count
    );
    let pattern_style = if explorer.pattern_editing {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::Cyan)
    };
    let pattern_widget = Paragraph::new(Line::from(Span::styled(pattern_line, pattern_style)))
        .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(pattern_widget, chunks[0]);

    // --- Folders section ---
    let mut folder_lines: Vec<Line> = Vec::new();

    if explorer.folders.is_empty() {
        folder_lines.push(Line::from(""));
        folder_lines.push(Line::from(Span::styled(
            "  No folders found",
            Style::default().fg(Color::DarkGray),
        )));
        if !explorer.current_prefix.is_empty() {
            folder_lines.push(Line::from(Span::styled(
                "  Press [Backspace] to go back",
                Style::default().fg(Color::Cyan),
            )));
        }
    } else {
        // Virtual scrolling for folders
        let visible_rows = chunks[1].height.saturating_sub(1) as usize;
        let total_folders = explorer.folders.len();
        let selected = explorer.selected_folder;

        let scroll_offset = if visible_rows >= total_folders {
            0
        } else if selected < visible_rows / 2 {
            0
        } else if selected > total_folders.saturating_sub(visible_rows / 2) {
            total_folders.saturating_sub(visible_rows)
        } else {
            selected.saturating_sub(visible_rows / 2)
        };

        let end_idx = (scroll_offset + visible_rows).min(total_folders);

        for (visible_idx, folder) in explorer.folders[scroll_offset..end_idx].iter().enumerate() {
            let actual_idx = scroll_offset + visible_idx;
            let is_selected = actual_idx == selected;

            let style = if is_selected && is_focused {
                Style::default().fg(Color::White).bold().bg(Color::DarkGray)
            } else if is_selected {
                Style::default().fg(Color::Cyan).bg(Color::Black)
            } else {
                Style::default().fg(Color::Gray)
            };

            let icon = if folder.is_file { "📄" } else { "📁" };
            let arrow = if folder.is_file { " " } else { ">" };
            let count_str = format_count(folder.file_count);

            let content = format!(
                " {} {:<40} {:>8} files {}",
                icon,
                truncate_string(&folder.name, 40),
                count_str,
                arrow
            );
            folder_lines.push(Line::from(Span::styled(content, style)));
        }
    }

    let folders_title = format!(" FOLDERS ({}) ", explorer.folders.len());
    let folders_title_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let folders_block = Block::default()
        .borders(Borders::NONE)
        .title(Span::styled(folders_title, folders_title_style));
    let folders_widget = Paragraph::new(folder_lines).block(folders_block);
    frame.render_widget(folders_widget, chunks[1]);

    // --- Preview section ---
    let mut preview_lines: Vec<Line> = Vec::new();

    if explorer.preview_files.is_empty() {
        preview_lines.push(Line::from(Span::styled(
            "  No preview files",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for file in explorer.preview_files.iter().take(5) {
            let size_str = format_size(file.size);
            let content = format!("  {} {:>8}", truncate_path_start(&file.rel_path, 50), size_str);
            preview_lines.push(Line::from(Span::styled(
                content,
                Style::default().fg(Color::DarkGray),
            )));
        }
    }

    let preview_block = Block::default()
        .borders(Borders::TOP)
        .title(Span::styled(" PREVIEW ", Style::default().fg(Color::DarkGray)));
    let preview_widget = Paragraph::new(preview_lines).block(preview_block);
    frame.render_widget(preview_widget, chunks[2]);
}

/// Truncate a string to max length (with ellipsis)
fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else if max_len <= 3 {
        "...".to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
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
    let footer = Paragraph::new(" [Esc/Alt+H] Home  [Alt+D] Discover  [Alt+P] Parser Bench  [Alt+I] Inspect  [Alt+J] Jobs ")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::TOP));
    frame.render_widget(footer, chunks[2]);
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
    let footer_text = match app.parser_bench.view {
        ParserBenchView::ParserList => {
            " [j/k] Navigate  [t] Test  [n] Quick test  [/] Filter  [?] Help  [Esc] Home "
        }
        ParserBenchView::ResultView => {
            " [r] Re-run  [f] Different file  [Esc] Back "
        }
        _ => " [Esc] Back ",
    };
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
        Some(JobStatus::Cancelled) => " (Cancelled only)",
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
                JobStatus::Cancelled => Color::DarkGray,
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

/// Draw the help overlay (per spec Section 3.1)
fn draw_help_overlay(frame: &mut Frame, area: Rect) {
    use ratatui::widgets::Clear;

    // Center the help panel
    let help_width = 60.min(area.width.saturating_sub(4));
    let help_height = 20.min(area.height.saturating_sub(4));
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

    // Help content
    let help_text = vec![
        "",
        "  NAVIGATION",
        "  ──────────",
        "  1         Discover view",
        "  2         Parser Bench view",
        "  3         Jobs view",
        "  4         Sources view",
        "  0 / H     Home",
        "  Esc       Back / Close dialog",
        "",
        "  GLOBAL ACTIONS",
        "  ──────────────",
        "  ?         Toggle this help",
        "  r         Refresh current view",
        "  q         Quit application",
        "  Alt+A     Toggle AI sidebar",
        "",
        "  Press ? or Esc to close",
    ];

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
