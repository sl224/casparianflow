use ratatui::layout::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewportClass {
    Narrow,
    Short,
    Normal,
}

pub fn viewport_class(area: Rect) -> ViewportClass {
    let narrow = area.width < 100;
    let short = area.height < 28;

    if narrow {
        ViewportClass::Narrow
    } else if short {
        ViewportClass::Short
    } else {
        ViewportClass::Normal
    }
}
