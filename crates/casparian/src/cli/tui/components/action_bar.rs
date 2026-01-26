use std::borrow::Cow;

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionHint {
    pub key: Cow<'static, str>,
    pub label: Cow<'static, str>,
    pub enabled: bool,
    pub priority: u8,
}

impl ActionHint {
    pub fn new(key: impl Into<Cow<'static, str>>, label: impl Into<Cow<'static, str>>, priority: u8) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            enabled: true,
            priority,
        }
    }

    pub fn disabled(key: impl Into<Cow<'static, str>>, label: impl Into<Cow<'static, str>>, priority: u8) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            enabled: false,
            priority,
        }
    }

    fn key_width(&self) -> usize {
        self.key.chars().count()
    }
}

const GAP_WIDTH: usize = 2;
const MORE_INDICATOR: &str = "(? more)";

pub fn render_action_bar(frame: &mut Frame, area: Rect, hints: &[ActionHint], style: Style) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let block = Block::default().borders(Borders::TOP);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let max_lines = inner.height.min(2) as usize;
    let layout = layout_hints(hints, inner.width as usize, max_lines);
    let lines = build_lines(&layout, hints, style);

    let paragraph = Paragraph::new(lines).alignment(Alignment::Center);
    frame.render_widget(paragraph, inner);
}

pub fn render_action_bar_message(frame: &mut Frame, area: Rect, message: &str, style: Style) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let block = Block::default().borders(Borders::TOP);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let paragraph = Paragraph::new(message)
        .style(style)
        .alignment(Alignment::Center)
        .wrap(ratatui::widgets::Wrap { trim: true });
    frame.render_widget(paragraph, inner);
}

pub fn format_help_lines(hints: &[ActionHint]) -> Vec<String> {
    let key_width = hints
        .iter()
        .map(|hint| hint.key_width())
        .max()
        .unwrap_or(0);
    hints
        .iter()
        .map(|hint| {
            let padding = key_width.saturating_sub(hint.key_width());
            format!("  {}{} {}", hint.key, " ".repeat(padding), hint.label)
        })
        .collect()
}

#[derive(Clone, Debug)]
struct HintLayout {
    lines: Vec<Vec<LayoutItem>>,
    dropped: bool,
}

#[derive(Clone, Debug)]
enum LayoutItem {
    Hint(usize),
    More,
}

fn layout_hints(hints: &[ActionHint], width: usize, max_lines: usize) -> HintLayout {
    let mut active: Vec<usize> = (0..hints.len()).collect();
    let mut dropped_any = false;

    loop {
        let mut items: Vec<LayoutItem> = active.iter().copied().map(LayoutItem::Hint).collect();
        if dropped_any {
            items.push(LayoutItem::More);
        }

        if let Some(lines) = pack_items(&items, hints, width, max_lines) {
            return HintLayout {
                lines,
                dropped: dropped_any,
            };
        }

        if active.is_empty() {
            return HintLayout {
                lines: Vec::new(),
                dropped: dropped_any,
            };
        }

        let drop_pos = active
            .iter()
            .enumerate()
            .min_by_key(|(_, idx)| (hints[**idx].priority, std::cmp::Reverse(*idx)))
            .map(|(pos, _)| pos)
            .unwrap_or(0);
        active.remove(drop_pos);
        dropped_any = true;
    }
}

fn pack_items(
    items: &[LayoutItem],
    hints: &[ActionHint],
    width: usize,
    max_lines: usize,
) -> Option<Vec<Vec<LayoutItem>>> {
    let mut lines: Vec<Vec<LayoutItem>> = vec![Vec::new()];
    let mut line_width = 0usize;

    for item in items {
        let item_width = item_width(item, hints);
        let gap = if line_width == 0 { 0 } else { GAP_WIDTH };

        if line_width + gap + item_width <= width {
            if gap > 0 {
                line_width += gap;
            }
            line_width += item_width;
            lines.last_mut().expect("line exists").push(item.clone());
            continue;
        }

        if lines.len() >= max_lines {
            return None;
        }

        lines.push(vec![item.clone()]);
        line_width = item_width;
    }

    Some(lines)
}

fn item_width(item: &LayoutItem, hints: &[ActionHint]) -> usize {
    match item {
        LayoutItem::Hint(idx) => hint_width(&hints[*idx]),
        LayoutItem::More => MORE_INDICATOR.chars().count(),
    }
}

fn hint_width(hint: &ActionHint) -> usize {
    let key_width = hint.key_width();
    let label_width = hint.label.chars().count();
    if label_width == 0 {
        key_width + 2
    } else {
        key_width + 2 + 1 + label_width
    }
}

fn build_lines(layout: &HintLayout, hints: &[ActionHint], style: Style) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    for line_items in &layout.lines {
        let mut spans = Vec::new();
        let mut first = true;
        for item in line_items {
            if !first {
                spans.push(Span::raw("  "));
            }
            first = false;

            match item {
                LayoutItem::Hint(idx) => {
                    let hint = &hints[*idx];
                    spans.extend(render_hint_spans(hint, style));
                }
                LayoutItem::More => {
                    spans.push(Span::styled(
                        MORE_INDICATOR,
                        Style::default().fg(Color::DarkGray),
                    ));
                }
            }
        }
        lines.push(Line::from(spans));
    }

    lines
}

fn render_hint_spans(hint: &ActionHint, base: Style) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let key_style = if hint.enabled {
        base.fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let label_style = if hint.enabled {
        base
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let key_text = format!("[{}]", hint.key);
    spans.push(Span::styled(key_text, key_style));
    if !hint.label.is_empty() {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(hint.label.clone(), label_style));
    }

    spans
}
