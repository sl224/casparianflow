//! UI rendering for the TUI

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Tabs, Wrap},
};

use super::app::{App, MessageRole, View};

/// Draw the entire UI
pub fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header with tabs
            Constraint::Min(0),    // Main content
            Constraint::Length(5), // Input box (taller for multi-line)
        ])
        .split(frame.area());

    draw_header(frame, app, chunks[0]);

    match app.view {
        View::Chat => draw_chat(frame, app, chunks[1], chunks[2]),
        View::Monitor => draw_monitor(frame, app, chunks[1], chunks[2]),
        View::Help => draw_help(frame, chunks[1], chunks[2]),
    }
}

/// Draw header with tabs
fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let titles = vec!["[F1] Chat", "[F2] Monitor", "[F3] Help"];
    let selected = match app.view {
        View::Chat => 0,
        View::Monitor => 1,
        View::Help => 2,
    };

    let tabs = Tabs::new(titles)
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .title(" Casparian TUI ")
                .title_alignment(Alignment::Left),
        )
        .select(selected)
        .style(Style::default().fg(Color::White))
        .highlight_style(Style::default().fg(Color::Cyan).bold());

    frame.render_widget(tabs, area);
}

/// Draw chat view
fn draw_chat(frame: &mut Frame, app: &App, main_area: Rect, input_area: Rect) {
    // Split main area into chat and context panes
    let chat_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
        .split(main_area);

    // Chat messages
    draw_messages(frame, app, chat_chunks[0]);

    // Context pane (workflow metadata)
    draw_context(frame, app, chat_chunks[1]);

    // Input box
    draw_input(frame, app, input_area);
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

/// Draw context pane with workflow metadata and recent tools
fn draw_context(frame: &mut Frame, app: &App, area: Rect) {
    let mut content = String::new();

    // Show workflow info if available
    if let Some(ref workflow) = app.chat.workflow {
        content.push_str(&format!(
            "Phase: {:?}\nNeeds Approval: {}\n\nNext Actions:\n{}\n\n",
            workflow.phase,
            workflow.needs_approval,
            workflow
                .next_actions
                .iter()
                .map(|a| format!("  > {}", a.tool_name))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    // Show recent tools used
    if !app.chat.recent_tools.is_empty() {
        content.push_str("Recent Tools:\n");
        for tool in &app.chat.recent_tools {
            content.push_str(&format!("  > {}\n", tool));
        }
    } else if content.is_empty() {
        content.push_str("No active workflow.\n\nStart by typing a message like:\n  'scan /path/to/data'\n  'help me process sensor files'");
    }

    // Show status indicator
    if app.chat.awaiting_response {
        content.push_str("\n\n[Processing...]");
    }

    let context = Paragraph::new(content)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Context ")
                .title_alignment(Alignment::Left),
        )
        .wrap(Wrap { trim: true });

    frame.render_widget(context, area);
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

/// Draw monitor view
fn draw_monitor(frame: &mut Frame, app: &App, main_area: Rect, status_area: Rect) {
    // Split into jobs and workers sections
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(main_area);

    // Jobs table placeholder
    let jobs_content = if app.monitor.job_count == 0 {
        "No jobs found.\n\nJobs will appear here when pipelines are running."
    } else {
        "Jobs will be listed here..."
    };

    let jobs = Paragraph::new(jobs_content)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Jobs ")
                .title_alignment(Alignment::Left),
        )
        .wrap(Wrap { trim: true });

    frame.render_widget(jobs, chunks[0]);

    // Workers table placeholder
    let workers = Paragraph::new("Workers will be listed here...")
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Workers ")
                .title_alignment(Alignment::Left),
        )
        .wrap(Wrap { trim: true });

    frame.render_widget(workers, chunks[1]);

    // Status bar
    let status = Paragraph::new("[j/k] Select  [Enter] Details  [r] Retry  [c] Cancel  [q] Quit")
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Center);

    frame.render_widget(status, status_area);
}

/// Draw help view
fn draw_help(frame: &mut Frame, main_area: Rect, status_area: Rect) {
    let help_text = r#"
CASPARIAN TUI - Keyboard Shortcuts

GLOBAL:
  F1          Switch to Chat view
  F2          Switch to Monitor view
  F3          Show this help
  Ctrl+C      Quit

CHAT VIEW:
  Enter       Send message
  Shift+Enter Insert newline (multi-line input)
  Esc         Clear input
  Up          Browse input history (single-line) / Move cursor (multi-line)
  Down        Browse input history / Move cursor
  Ctrl+Up     Scroll messages up
  Ctrl+Down   Scroll messages down
  PageUp      Scroll messages up (fast)
  PageDown    Scroll messages down (fast)
  Home        Move to start of line
  End         Move to end of line

MONITOR VIEW:
  j/k         Select job (down/up)
  Enter       View job details
  r           Retry failed job
  c           Cancel running job

COMMANDS:
  In chat, you can ask Claude to:
  - 'scan /path/to/data' - Discover files
  - 'preview these files' - See file content
  - 'create a schema' - Define data structure
  - 'generate a parser' - Create parser code
  - 'run the pipeline' - Process files

The workflow guides you through:
  scan -> preview -> schema -> approve -> parse -> query
"#;

    let help = Paragraph::new(help_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Help ")
                .title_alignment(Alignment::Left),
        )
        .wrap(Wrap { trim: true });

    frame.render_widget(help, main_area);

    let status = Paragraph::new("Press q or F1/F2 to return")
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Center);

    frame.render_widget(status, status_area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::tui::TuiArgs;
    use ratatui::backend::TestBackend;

    fn test_args() -> TuiArgs {
        TuiArgs {
            database: None,
            api_key: None,
            model: "test".into(),
        }
    }

    #[test]
    fn test_draw_chat_view() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let app = App::new(test_args());

        terminal.draw(|f| draw(f, &app)).unwrap();

        // Check that key elements are rendered
        let buffer = terminal.backend().buffer();
        let content: String = buffer
            .content
            .iter()
            .map(|cell| cell.symbol().chars().next().unwrap_or(' '))
            .collect();

        assert!(content.contains("Chat"));
        assert!(content.contains("Monitor"));
        assert!(content.contains("Messages"));
    }

    #[test]
    fn test_draw_monitor_view() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut app = App::new(test_args());
        app.view = View::Monitor;

        terminal.draw(|f| draw(f, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = buffer
            .content
            .iter()
            .map(|cell| cell.symbol().chars().next().unwrap_or(' '))
            .collect();

        assert!(content.contains("Jobs"));
        assert!(content.contains("Workers"));
    }

    #[test]
    fn test_draw_help_view() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut app = App::new(test_args());
        app.view = View::Help;

        terminal.draw(|f| draw(f, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = buffer
            .content
            .iter()
            .map(|cell| cell.symbol().chars().next().unwrap_or(' '))
            .collect();

        assert!(content.contains("Help"));
        assert!(content.contains("GLOBAL"));
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
