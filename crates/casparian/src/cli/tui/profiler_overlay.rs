//! Profiler overlay for TUI
//!
//! Renders performance data as a floating overlay when F12 is pressed.
//! Shows frame timing, budget utilization, and zone breakdown.

use casparian_profiler::Profiler;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Gauge, Paragraph, Row, Sparkline, Table},
    Frame,
};

/// Render the profiler overlay
pub fn render(frame: &mut Frame, profiler: &Profiler) {
    let area = frame.area();

    // Overlay takes bottom-right quadrant, 60 chars wide, 20 lines tall
    let overlay_width = 64.min(area.width.saturating_sub(4));
    let overlay_height = 22.min(area.height.saturating_sub(2));

    let overlay_area = Rect {
        x: area.width.saturating_sub(overlay_width + 2),
        y: area.height.saturating_sub(overlay_height + 1),
        width: overlay_width,
        height: overlay_height,
    };

    // Clear the area behind the overlay
    frame.render_widget(Clear, overlay_area);

    // Main block
    let block = Block::default()
        .title(" Profiler [F12] ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .style(Style::default().fg(Color::Cyan));

    let inner = block.inner(overlay_area);
    frame.render_widget(block, overlay_area);

    // Split into sections
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Header with frame info
            Constraint::Length(3), // Sparkline
            Constraint::Min(4),    // Zone table
        ])
        .split(inner);

    // Header: Frame count, budget bar, timing
    render_header(frame, profiler, chunks[0]);

    // Sparkline: Last 30 frame times
    render_sparkline(frame, profiler, chunks[1]);

    // Zone table
    render_zones(frame, profiler, chunks[2]);
}

fn render_header(frame: &mut Frame, profiler: &Profiler, area: Rect) {
    let budget_ms = profiler.budget_ms();
    let utilization = profiler.budget_utilization();
    let last_time = profiler.last_frame_time().unwrap_or(0.0);
    let frame_count = profiler.frame_count();

    // Color based on utilization
    let bar_color = if utilization > 1.0 {
        Color::Red
    } else if utilization > 0.8 {
        Color::Yellow
    } else {
        Color::Green
    };

    // Frame: N  Budget: XX% ████░░  XXXms / 250ms
    let percent = (utilization * 100.0).min(100.0) as u16;

    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(bar_color))
        .percent(percent)
        .label(format!(
            "Frame: {}  {:.0}ms / {}ms ({:.0}%)",
            frame_count,
            last_time,
            budget_ms,
            utilization * 100.0
        ));

    frame.render_widget(gauge, area);
}

fn render_sparkline(frame: &mut Frame, profiler: &Profiler, area: Rect) {
    let history = profiler.frame_history(30);
    let avg = profiler.avg_frame_time(30);

    // Convert to u64 for sparkline (scale to fit)
    let max_val = history.iter().cloned().fold(0.0_f64, f64::max);
    let min_val = history.iter().cloned().fold(f64::MAX, f64::min);

    let data: Vec<u64> = history
        .iter()
        .rev() // Reverse so oldest is first (left side)
        .map(|&v| {
            // Scale to 0-100 range
            if max_val == min_val {
                50
            } else {
                ((v - min_val) / (max_val - min_val) * 100.0) as u64
            }
        })
        .collect();

    let sparkline = Sparkline::default()
        .data(&data)
        .style(Style::default().fg(Color::Cyan));

    // Stats line
    let stats = Paragraph::new(Line::from(vec![
        Span::raw("avg: "),
        Span::styled(format!("{:.0}ms", avg), Style::default().fg(Color::White)),
        Span::raw("  max: "),
        Span::styled(
            format!("{:.0}ms", max_val),
            Style::default().fg(if max_val > profiler.budget_ms() as f64 {
                Color::Red
            } else {
                Color::White
            }),
        ),
        Span::raw("  min: "),
        Span::styled(
            format!("{:.0}ms", min_val),
            Style::default().fg(Color::White),
        ),
    ]));

    // Split area for sparkline and stats
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    frame.render_widget(sparkline, chunks[0]);
    frame.render_widget(stats, chunks[1]);
}

fn render_zones(frame: &mut Frame, profiler: &Profiler, area: Rect) {
    let zones = profiler.last_frame_zones();
    let last_frame_total = profiler.last_frame_time().unwrap_or(1.0);

    // Build rows with hierarchy indentation
    let rows: Vec<Row> = zones
        .iter()
        .map(|(name, ms, _calls)| {
            let percent = (ms / last_frame_total * 100.0).min(100.0);

            // Indent based on dot count (hierarchy)
            let indent = name.matches('.').count();
            let display_name = if indent > 1 {
                format!(
                    "{}{}",
                    "  ".repeat(indent - 1),
                    name.rsplit('.').next().unwrap_or(name)
                )
            } else {
                (*name).to_string()
            };

            // Mini bar: █ characters proportional to percent
            let bar_width = (percent / 10.0) as usize;
            let bar = "█".repeat(bar_width.min(10));

            // Color based on percentage
            let time_color = if percent > 50.0 {
                Color::Red
            } else if percent > 25.0 {
                Color::Yellow
            } else {
                Color::White
            };

            Row::new(vec![
                display_name,
                format!("{:>6.1}ms", ms),
                format!("{:>5.1}%", percent),
                bar,
            ])
            .style(Style::default().fg(time_color))
        })
        .collect();

    let header = Row::new(vec!["Zone", "Time", "%", ""]).style(
        Style::default()
            .add_modifier(Modifier::BOLD)
            .fg(Color::Cyan),
    );

    let widths = [
        Constraint::Min(20),
        Constraint::Length(8),
        Constraint::Length(6),
        Constraint::Length(10),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::TOP));

    frame.render_widget(table, area);
}
