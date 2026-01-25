//! Snapshot helpers for deterministic TUI rendering.

use std::io;

use ratatui::{backend::TestBackend, buffer::Buffer, layout::Rect, style::Color, Terminal};
use serde::Serialize;

use super::app::{App, ShellFocus, TuiMode};
use super::ui;

#[derive(Debug, Clone, Serialize)]
pub struct LayoutRect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl LayoutRect {
    fn from_rect(rect: Rect) -> Self {
        Self {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LayoutNode {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub rect: LayoutRect,
    pub focused: bool,
}

impl LayoutNode {
    fn new(id: &str, title: &str, kind: &str, rect: Rect, focused: bool) -> Self {
        Self {
            id: id.to_string(),
            title: title.to_string(),
            kind: kind.to_string(),
            rect: LayoutRect::from_rect(rect),
            focused,
        }
    }
}

fn mode_label(mode: TuiMode) -> &'static str {
    match mode {
        TuiMode::Home => "Home",
        TuiMode::Discover => "Discover",
        TuiMode::Jobs => "Jobs",
        TuiMode::Sources => "Sources",
        TuiMode::Approvals => "Approvals",
        TuiMode::ParserBench => "Parser Bench",
        TuiMode::Query => "Query",
        TuiMode::Settings => "Settings",
        TuiMode::Sessions => "Sessions",
        TuiMode::Triage => "Triage",
        TuiMode::Catalog => "Catalog",
    }
}

/// Build a lightweight layout tree for LLM/UX inspection.
pub fn layout_tree(app: &App, width: u16, height: u16) -> Vec<LayoutNode> {
    let area = Rect::new(0, 0, width, height);
    let mut nodes = Vec::new();

    nodes.push(LayoutNode::new(
        "screen",
        &format!("Screen ({})", mode_label(app.mode)),
        "root",
        area,
        false,
    ));

    let inspector_visible = !app.inspector_collapsed;
    let shell = ui::shell_layout(area, inspector_visible);

    let rail_focused = app.shell_focus == ShellFocus::Rail;
    let main_focused = app.shell_focus == ShellFocus::Main;

    nodes.push(LayoutNode::new(
        "shell_top",
        "Top Bar",
        "shell",
        shell.top,
        false,
    ));
    nodes.push(LayoutNode::new(
        "shell_rail",
        "Navigation",
        "shell",
        shell.rail,
        rail_focused,
    ));
    nodes.push(LayoutNode::new(
        "shell_main",
        "Main",
        "shell",
        shell.main,
        main_focused,
    ));
    if inspector_visible {
        nodes.push(LayoutNode::new(
            "shell_inspector",
            "Inspector",
            "shell",
            shell.inspector,
            false,
        ));
    }
    nodes.push(LayoutNode::new(
        "shell_bottom",
        "Footer",
        "shell",
        shell.bottom,
        false,
    ));

    if app.jobs_drawer_open {
        let rect = ui::right_drawer_area(area);
        nodes.push(LayoutNode::new(
            "jobs_drawer",
            "Jobs Drawer",
            "overlay",
            rect,
            true,
        ));
    }

    if app.sources_drawer_open {
        let rect = ui::right_drawer_area(area);
        nodes.push(LayoutNode::new(
            "sources_drawer",
            "Sources Drawer",
            "overlay",
            rect,
            true,
        ));
    }

    if app.show_help {
        let rect = ui::help_overlay_area(area);
        nodes.push(LayoutNode::new(
            "help_overlay",
            "Help Overlay",
            "overlay",
            rect,
            true,
        ));
    }

    if app.command_palette.visible {
        let rect = ui::command_palette_area(&app.command_palette, area);
        nodes.push(LayoutNode::new(
            "command_palette",
            "Command Palette",
            "overlay",
            rect,
            true,
        ));
    }

    nodes
}

/// Render the current app state into a Ratatui buffer with a TestBackend.
pub fn render_app_to_buffer(app: &App, width: u16, height: u16) -> io::Result<Buffer> {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend)?;
    terminal.draw(|frame| ui::draw(frame, app))?;
    Ok(terminal.backend().buffer().clone())
}

/// Convert a Buffer into a plain text grid.
pub fn buffer_to_plain_text(buf: &Buffer) -> String {
    let width = buf.area.width;
    let height = buf.area.height;
    let mut out = String::with_capacity((width as usize + 1) * height as usize);

    for y in 0..height {
        for x in 0..width {
            let cell = &buf[(x, y)];
            let symbol = cell.symbol();
            if symbol.is_empty() {
                out.push(' ');
            } else {
                out.push_str(symbol);
            }
        }
        if y + 1 < height {
            out.push('\n');
        }
    }

    out
}

/// Convert a Buffer into a background mask for highlighting focus/selection.
pub fn buffer_to_bg_mask(buf: &Buffer) -> String {
    let width = buf.area.width;
    let height = buf.area.height;
    let mut out = String::with_capacity((width as usize + 1) * height as usize);

    for y in 0..height {
        for x in 0..width {
            let cell = &buf[(x, y)];
            let ch = if cell.bg == Color::Reset {
                ' '
            } else if cell.bg == Color::DarkGray {
                '#'
            } else {
                '*'
            };
            out.push(ch);
        }
        if y + 1 < height {
            out.push('\n');
        }
    }

    out
}

/// Normalize snapshot strings for consistent comparisons.
pub fn normalize_for_snapshot(input: &str) -> String {
    let normalized = input.replace("\r\n", "\n");
    normalized
        .split('\n')
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}
