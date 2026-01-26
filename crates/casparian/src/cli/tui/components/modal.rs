use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear},
};

#[derive(Clone, Copy, Debug)]
pub struct ModalLayout {
    pub area: Rect,
    pub inner: Rect,
    pub header: Rect,
    pub body: Rect,
    pub footer: Rect,
}

pub fn render_scrim(frame: &mut Frame, area: Rect, top_bar: Rect) {
    let scrim_area = Rect::new(
        area.x,
        top_bar.y + top_bar.height,
        area.width,
        area.height.saturating_sub(top_bar.height),
    );
    frame.render_widget(Clear, scrim_area);
}

pub fn render_modal(
    frame: &mut Frame,
    area: Rect,
    max_width: u16,
    max_height: u16,
    header_height: u16,
    footer_height: u16,
    title: &str,
    border_style: Style,
) -> ModalLayout {
    let dialog = centered_area(area, max_width, max_height);
    frame.render_widget(Clear, dialog);

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);
    let inner = block.inner(dialog);
    frame.render_widget(block, dialog);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_height),
            Constraint::Min(0),
            Constraint::Length(footer_height),
        ])
        .split(inner);

    ModalLayout {
        area: dialog,
        inner,
        header: chunks[0],
        body: chunks[1],
        footer: chunks[2],
    }
}

pub fn centered_area(area: Rect, max_width: u16, max_height: u16) -> Rect {
    let width = area.width.min(max_width);
    let height = area.height.min(max_height);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width, height)
}
