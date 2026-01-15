# TUI Style Guide

**Status:** Active
**Parent:** spec.md
**Version:** 1.0
**Date:** January 14, 2026

---

## 1. Overview

This document defines the visual design system for Casparian Flow's Terminal User Interface. The style prioritizes clarity, accessibility, and visual hierarchy in terminal environments with limited color support.

**Design Philosophy:**
- Information density over decoration
- Clear visual hierarchy via color and borders
- Consistent focus indicators across all modes
- Terminal-safe color palette (16-color compatible)
- Unicode symbols for status, ASCII fallbacks available

---

## 2. Color Palette

### 2.1 Semantic Colors

| Role | Color | Hex (approx) | Usage |
|------|-------|--------------|-------|
| **Primary/Focus** | `Cyan` | #00FFFF | Focused elements, active selections, primary actions |
| **Success** | `Green` | #00FF00 | Completed jobs, healthy parsers, confirmations |
| **Warning** | `Yellow` | #FFFF00 | Filtering mode, warnings, pending states |
| **Error** | `Red` | #FF0000 | Failed jobs, errors, destructive confirmations |
| **Info** | `Blue` | #0000FF | Running jobs, informational dialogs |
| **Accent** | `Magenta` | #FF00FF | Tool messages, tagging operations |
| **Muted** | `DarkGray` | #555555 | Unfocused elements, hints, secondary text |
| **Text** | `White` | #FFFFFF | Primary text, selected items |
| **Dim Text** | `Gray` | #AAAAAA | Secondary text, descriptions |

### 2.2 Context-Specific Colors

```
Title Bars:           Cyan + Bold
Footers/Hints:        DarkGray
Selected Item (bg):   DarkGray background, White text
Preview Item:         White + Bold (► prefix)
Cursor (input):       Yellow + Bold (│ character)
Disabled:             DarkGray
```

### 2.3 Status Colors by State

| State | Color | Example |
|-------|-------|---------|
| Focused | `Cyan` | Active panel borders |
| Active/Editing | `Yellow` | Filter mode, input active |
| Error/Critical | `Red` | Failed jobs, delete confirmations |
| Success | `Green` | Completed, healthy |
| In Progress | `Blue` | Running jobs |
| Inactive | `DarkGray` | Unfocused panels |

---

## 3. Typography

### 3.1 Text Styles

| Style | Ratatui | Usage |
|-------|---------|-------|
| **Title** | `.fg(Color::Cyan).bold()` | Screen titles, panel headers |
| **Subtitle** | `.fg(Color::DarkGray)` | Section hints, keybinding labels |
| **Body** | `.fg(Color::White)` | Primary content |
| **Muted** | `.fg(Color::Gray)` | Secondary content, descriptions |
| **Dim** | `.fg(Color::DarkGray)` | Hints, disabled text |
| **Emphasis** | `.fg(Color::White).bold()` | Selected items, important text |
| **Italic** | `.fg(Color::DarkGray).italic()` | Empty state messages, placeholders |

### 3.2 Text Conventions

```rust
// Titles - Cyan + Bold, centered or left-aligned
.style(Style::default().fg(Color::Cyan).bold())

// Selected item - White + Bold with bg highlight
Style::default().fg(Color::White).bold().bg(Color::DarkGray)

// Error messages - Red, plain
Style::default().fg(Color::Red)

// Success messages - Green, plain
Style::default().fg(Color::Green)

// Input cursor - Yellow + Bold (│ character)
Span::styled("│", Style::default().fg(Color::Yellow).bold())
```

---

## 4. Borders & Containers

### 4.1 Border Types

| State | Border Type | Color |
|-------|-------------|-------|
| **Focused** | `Double` | `Cyan` |
| **Unfocused** | `Rounded` | `DarkGray` |
| **Editing/Active** | `Double` | `Yellow` |
| **Error** | `Plain` | `Red` |

### 4.2 Border Hierarchy

```
Level 1 (Screen):     No borders (full screen)
Level 2 (Panels):     Rounded borders (unfocused) or Double (focused)
Level 3 (Sections):   TOP or BOTTOM borders only
Level 4 (Dialogs):    ALL borders, Double when modal
```

### 4.3 Implementation Pattern

```rust
// Focused panel
let (border_style, border_type) = if is_focused {
    (Style::default().fg(Color::Cyan), BorderType::Double)
} else {
    (Style::default().fg(Color::DarkGray), BorderType::Rounded)
};

let block = Block::default()
    .borders(Borders::ALL)
    .border_style(border_style)
    .border_type(border_type)
    .title(Span::styled(" Title ", title_style));
```

### 4.4 Separator Borders

```rust
// Title bar separator
Block::default().borders(Borders::BOTTOM)

// Footer separator
Block::default().borders(Borders::TOP)

// Left panel divider
Block::default().borders(Borders::LEFT)
```

---

## 5. Status Symbols

### 5.1 Job Status

| Status | Symbol | Color |
|--------|--------|-------|
| Pending | `○` | Yellow |
| Running | `↻` | Blue |
| Completed | `✓` | Green |
| Failed | `✗` | Red |
| Cancelled | `⊘` | DarkGray |

### 5.2 Parser Health

| Health | Symbol | Color |
|--------|--------|-------|
| Healthy | `●` | Green |
| Warning | `⚠` | Yellow |
| Paused | `⏸` | Red |
| Unknown | `○` | Gray |
| Broken Link | `✗` | Red |

### 5.3 Navigation & UI

| Element | Symbol | Usage |
|---------|--------|-------|
| Selection | `▸` or `►` | List item prefix |
| Dropdown Open | `▲` | Expanded dropdown |
| Dropdown Closed | `▾` | Collapsed dropdown |
| Folder | `📁` | Directory icon |
| File | `📄` | File icon |
| Arrow Right | `>` or `→` | Drill-down indicator |
| Arrow Flow | `────▶` | Pipeline flow |

### 5.4 Spinner Animation

```rust
// Braille spinner (preferred)
const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

// ASCII fallback
const SPINNER_ASCII: [char; 4] = ['-', '\\', '|', '/'];

pub fn spinner_char(tick: u64) -> char {
    SPINNER[(tick / 2) as usize % SPINNER.len()]
}
```

### 5.5 Animation Timing

The TUI event loop runs with a **tick rate of 250ms** (4 ticks per second). Animation frame selection is controlled by dividing the tick counter.

| Parameter | Value | Source |
|-----------|-------|--------|
| **Tick Rate** | 250ms | `mod.rs:66` |
| **Spinner Frame Divisor** | `/2` | `ui.rs:21` |
| **Effective Frame Rate** | 2 fps | Every 500ms |

**Frame Rate Calculation:**
```
Effective frame rate = 1000ms / (tick_rate_ms * divisor)
                     = 1000 / (250 * 2)
                     = 2 frames per second
```

**Animation Durations:**

| Animation | Divisor | Frames | Duration per Frame | Full Cycle |
|-----------|---------|--------|-------------------|------------|
| Braille spinner | `/2` | 10 | 500ms | 5.0s |
| ASCII spinner | `/2` | 4 | 500ms | 2.0s |
| Dots animation | `/4` | 4 | 1000ms | 4.0s |

**Implementation Reference:**

```rust
// Tick rate (mod.rs:66)
let mut events = EventHandler::new(std::time::Duration::from_millis(250));

// Tick counter increment (app.rs)
self.tick_count = self.tick_count.wrapping_add(1);

// Frame selection (ui.rs:21)
SPINNER[(tick / 2) as usize % SPINNER.len()]
```

**Design Rationale:**
- 250ms tick rate balances responsiveness with CPU efficiency
- `/2` divisor slows animations to comfortable viewing speed (2 fps)
- Braille spinner's 10 frames provide smooth rotation at 2 fps

### 5.6 Animation Speed Hierarchy

The TUI uses **two animation speeds** to create visual hierarchy:

| Speed | Divisor | FPS | Use Case |
|-------|---------|-----|----------|
| **Fast** | `/2` | 2 fps | Primary animations (spinners in titles) |
| **Slow** | `/4` | 1 fps | Secondary animations (dots in hint lines) |

**Rationale:** Secondary animations (dots) run slower to avoid competing visually with primary animations (spinners). When a dialog shows both a title spinner and hint dots, the slower dots provide subtle feedback without distraction.

```
┌─ [|] Scanning ─────────────────────────────┐  <- Fast spinner (2 fps)
│ scanning...                                 │  <- Slow dots (1 fps)
└─────────────────────────────────────────────┘
```

---

## 6. Focus Indicators

### 6.1 Visual Hierarchy

```
Focused:
┌═══════════════════════════════════════╗  <- Double border, Cyan
║ FOCUSED PANEL                          ║
╚═══════════════════════════════════════╝

Unfocused:
╭───────────────────────────────────────╮  <- Rounded border, DarkGray
│ UNFOCUSED PANEL                        │
╰───────────────────────────────────────╯
```

### 6.2 Selection Highlighting

```rust
// Selected item in list
let style = if is_selected && is_focused {
    Style::default().fg(Color::White).bold().bg(Color::DarkGray)
} else if is_selected {
    Style::default().fg(Color::Cyan).bg(Color::Black)
} else {
    Style::default().fg(Color::Gray)
};
```

### 6.3 Preview vs Selection

| State | Style |
|-------|-------|
| Preview (hovering) | `► ` prefix + `White.bold()` |
| Selected (committed) | `Cyan` text |
| Both | `► ` prefix + `Cyan.bold()` |

---

## 7. Layout Patterns

### 7.1 Standard Screen Structure

```
┌─────────────────────────────────────────────────┐
│ TITLE BAR                            [3 lines]  │ <- Cyan text, BOTTOM border
├─────────────────────────────────────────────────┤
│                                                 │
│ MAIN CONTENT                         [Min(0)]  │ <- Flexible height
│                                                 │
├─────────────────────────────────────────────────┤
│ FOOTER / STATUS BAR                  [3 lines]  │ <- DarkGray text, TOP border
└─────────────────────────────────────────────────┘
```

### 7.2 Two-Panel Layout

```
┌────────────────┬────────────────────────────────┐
│                │                                │
│  LEFT PANEL    │       RIGHT PANEL              │
│  (35% or Min)  │       (65% or rest)            │
│                │                                │
└────────────────┴────────────────────────────────┘
```

### 7.3 Sidebar Pattern

```
┌─────────────────────────────────────┬───────────┐
│                                     │           │
│  MAIN CONTENT (70%)                 │  SIDEBAR  │
│                                     │  (30%)    │
│                                     │           │
└─────────────────────────────────────┴───────────┘
```

### 7.4 Dialog/Modal Pattern

```rust
pub fn centered_dialog_area(area: Rect, max_width: u16, max_height: u16) -> Rect {
    let width = area.width.min(max_width);
    let height = area.height.min(max_height);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width, height)
}

// Usage: Clear background, then render dialog
frame.render_widget(Clear, dialog_area);
```

---

## 8. Component Styles

### 8.1 Title Bar

```rust
let title = Paragraph::new(" Screen Name ")
    .style(Style::default().fg(Color::Cyan).bold())
    .alignment(Alignment::Center)
    .block(Block::default().borders(Borders::BOTTOM));
```

### 8.2 Footer/Status Bar

```rust
// Normal hints
let footer = Paragraph::new(" [key] Action  [key] Action ")
    .style(Style::default().fg(Color::DarkGray))
    .alignment(Alignment::Center)
    .block(Block::default().borders(Borders::TOP));

// Status message (success)
Style::default().fg(Color::Green)

// Status message (error)
Style::default().fg(Color::Red)
```

### 8.3 Dropdown (Collapsed)

```rust
// Format: "[N] name ▾ count"
Line::from(vec![
    Span::styled("[1] ", Style::default().fg(Color::DarkGray)),
    Span::styled(&name, Style::default().fg(Color::White)),
    Span::styled(" ▾ ", Style::default().fg(Color::DarkGray)),
    Span::styled(count, Style::default().fg(Color::DarkGray)),
])
```

### 8.4 Dropdown (Expanded)

```rust
// Double borders when focused and open
let block = Block::default()
    .borders(Borders::ALL)
    .border_style(Style::default().fg(Color::Cyan))
    .border_type(BorderType::Double)
    .title(Span::styled(" [1] Source ▲ ", Style::default().fg(Color::Cyan).bold()));
```

### 8.5 Input Field

```rust
// Active input with cursor (glob pattern style)
let mut spans = vec![
    Span::styled(before_cursor, Style::default().fg(Color::Yellow).bold()),
    Span::styled("│", Style::default().fg(Color::White).bold()),  // Cursor
    Span::styled(after_cursor, Style::default().fg(Color::Yellow).bold()),
];

// Dialog input style (uses block cursor)
let filter_line = Paragraph::new(vec![
    Line::from(vec![
        Span::styled(text, Style::default().fg(Color::Yellow)),
        Span::styled("█", Style::default().fg(Color::Yellow)),  // Block cursor
    ])
]);

// Input container - Double borders when editing
let border_type = if is_editing { BorderType::Double } else { BorderType::Rounded };
let border_color = if is_editing { Color::Yellow } else { Color::Cyan };
```

#### Cursor Style Selection

The TUI uses **two distinct cursor styles** based on input context:

| Cursor | Character | Context | Position |
|--------|-----------|---------|----------|
| **Block** | `█` | Dialog form inputs | Appended to text end |
| **Pipe** | `│` | Inline filter/search | Inserted at cursor position |

**Block Cursor (█) - Dialog Forms:**
- Used in modal dialogs with explicit field labels
- Input field is primary focus element
- Cursor appended to visible text end
- Examples: Source name edit, rule pattern/tag fields

**Pipe Cursor (│) - Inline Inputs:**
- Used in main panels for filter/search
- Shows exact cursor position within text
- Examples: File explorer glob filter

```rust
// Dialog input (block cursor)
let input_text = format!("{}█", input_value);

// Inline filter (pipe cursor)
pattern_spans.push(Span::styled(before_cursor, style));
pattern_spans.push(Span::styled("│", Style::default().fg(Color::White).bold()));
pattern_spans.push(Span::styled(after_cursor, style));
```

### 8.6 Progress Bar

```rust
fn render_progress_bar(percent: u8, width: usize) -> String {
    let filled = (percent as usize * width) / 100;
    let empty = width - filled;
    format!("[{}{}]", "█".repeat(filled), "░".repeat(empty))
}
```

### 8.7 Sparkline (Text-based)

```rust
fn render_sparkline(data: &VecDeque<u32>) -> String {
    // Actual codebase array - uses space for zero level
    const BLOCKS: [char; 8] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇'];

    let max_val = data.iter().max().copied().unwrap_or(1).max(1);
    data.iter().map(|&v| {
        let idx = ((v as f64 / max_val as f64) * 7.0) as usize;
        BLOCKS[idx.min(7)]
    }).collect()
}
```

> **Note:** Uses space for zero/min values to create visual baseline.

### 8.8 List Widget Patterns

The codebase uses two patterns for rendering selectable lists:

#### Pattern A: Paragraph-Based Lists (Preferred)

Used when: Custom line formatting needed, multi-line items, or complex styling per item.

```rust
let mut lines: Vec<Line> = Vec::new();

for (i, item) in items.iter().enumerate() {
    let is_selected = i == app.state.selected_index;

    // Selection prefix (arrow indicator)
    let prefix = if is_selected { "► " } else { "  " };

    // Selection styling with focus awareness
    let style = if is_selected && is_focused {
        Style::default().fg(Color::White).bold().bg(Color::DarkGray)
    } else if is_selected {
        Style::default().fg(Color::Cyan).bg(Color::Black)
    } else {
        Style::default().fg(Color::Gray)
    };

    lines.push(Line::from(Span::styled(
        format!("{}{}", prefix, item.name),
        style,
    )));
}

let list = Paragraph::new(lines).block(block);
frame.render_widget(list, area);
```

#### Pattern B: Ratatui List Widget

Used when: Simple, uniform items where `ListItem` styling suffices.

```rust
use ratatui::widgets::{List, ListItem, ListState};

let items: Vec<ListItem> = data
    .iter()
    .enumerate()
    .map(|(i, item)| {
        let style = if i == selected_index {
            Style::default().bg(Color::DarkGray).fg(Color::White)
        } else {
            Style::default().fg(Color::Gray)
        };
        ListItem::new(format!(" {}", item.name)).style(style)
    })
    .collect();

let list = List::new(items)
    .block(Block::default().borders(Borders::ALL).title(" Items "));

// Stateful rendering with ListState
let mut state = ListState::default();
state.select(Some(selected_index));
frame.render_stateful_widget(list, area, &mut state);
```

### 8.9 Selection Highlighting Patterns

#### Three-State Selection (Focused + Selected)

| State | Style | Example Use |
|-------|-------|-------------|
| Selected + Focused | `fg(White).bold().bg(DarkGray)` | Active panel, current item |
| Selected + Unfocused | `fg(Cyan).bg(Black)` | Inactive panel, current item |
| Unselected | `fg(Gray)` | All other items |

```rust
let style = if is_selected && is_focused {
    Style::default().fg(Color::White).bold().bg(Color::DarkGray)
} else if is_selected {
    Style::default().fg(Color::Cyan).bg(Color::Black)
} else {
    Style::default().fg(Color::Gray)
};
```

#### Two-State Selection (Simple)

| State | Style | Example Use |
|-------|-------|-------------|
| Selected | `fg(Cyan).bold()` | Single-panel lists |
| Unselected | `fg(White)` or `fg(Gray)` | Other items |

### 8.10 Selection Prefix Indicator

| Symbol | Usage |
|--------|-------|
| `► ` (filled arrow) | Selected/Preview item |
| `  ` (two spaces) | Unselected items |

**Alignment Rule:** Always use 2-character prefix for column alignment:
```rust
let prefix = if is_selected { "► " } else { "  " };
```

### 8.11 ListState Management

**State Location:** Selection indices live in `App` state structs, NOT in ratatui `ListState`:

```rust
// In app.rs
pub struct DiscoverState {
    pub selected_rule: usize,
    pub selected_source_id: Option<SourceId>,
    // ...
}

pub struct JobsState {
    pub selected_index: usize,
    // ...
}
```

**ListState Usage:** Only create `ListState` at render time:

```rust
// At render time (ui.rs)
let mut state = ListState::default();
state.select(Some(app.parser_bench.selected_parser));
frame.render_stateful_widget(list, area, &mut state);
```

**Why This Pattern:**
- State persists across re-renders without special handling
- Selection logic lives with app state, not widget state
- Simpler serialization/persistence if needed

### 8.12 Virtual Scrolling for Lists

For lists exceeding visible height, implement virtual scrolling:

```rust
// Calculate scroll offset to keep selection centered
fn centered_scroll_offset(selected: usize, visible_height: usize, total: usize) -> usize {
    if total <= visible_height {
        0
    } else if selected < visible_height / 2 {
        0
    } else if selected > total - visible_height / 2 {
        total - visible_height
    } else {
        selected - visible_height / 2
    }
}

// Render only visible items
let scroll_offset = centered_scroll_offset(selected, visible_rows, total_items);
let end_idx = (scroll_offset + visible_rows).min(total_items);

for (visible_idx, item) in items[scroll_offset..end_idx].iter().enumerate() {
    let actual_idx = scroll_offset + visible_idx;
    let is_selected = actual_idx == selected;
    // ... render item
}
```

**Position Indicator:** For large lists (>100 items), show position in title:

```rust
let title = if item_count > 100 {
    format!(" FILES ({}/{}) ", selected + 1, format_count(item_count))
} else {
    format!(" FILES ({}) ", item_count)
};
```

### 8.13 Scrollbars

#### When to Show Scrollbars

| Context | Scrollbar | Indicator Alternative |
|---------|-----------|----------------------|
| **Chat/Messages** | Yes - always when content exceeds viewport | N/A |
| **File Lists** | No - use title position indicator | "(15/1.2K)" in title |
| **Dropdown Lists** | No - use virtual scrolling only | Visual scroll behavior |
| **Job Lists** | No - use virtual scrolling only | Visual scroll behavior |
| **Preview Panels** | Optional - for multi-page content | Wrap or truncate |

**Rationale:** Scrollbars are reserved for continuous text content (messages, logs, output) where position tracking aids comprehension. Discrete item lists use title indicators and virtual scrolling for cleaner appearance.

#### Scrollbar Styling

| Property | Value | Notes |
|----------|-------|-------|
| **Orientation** | `VerticalRight` | Right edge of content area |
| **Track Style** | Default (inherit) | No explicit track styling |
| **Thumb Style** | Default (inherit) | No explicit thumb styling |
| **Begin Symbol** | `"^"` | ASCII up arrow |
| **End Symbol** | `"v"` | ASCII down arrow |
| **Track Symbol** | Default | Uses ratatui default `│` |
| **Thumb Symbol** | Default | Uses ratatui default `█` |

#### Scrollbar Position

Position scrollbar INSIDE the block border:

```rust
let scrollbar_area = Rect {
    x: area.x + area.width - 1,  // Rightmost column
    y: area.y + 1,               // Below top border
    width: 1,
    height: area.height.saturating_sub(2),  // Minus top/bottom borders
};
```

#### Scrollbar Implementation

```rust
use ratatui::widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState};

// Only render when needed
if total_items > visible_items {
    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(Some("^"))
        .end_symbol(Some("v"));

    let mut scrollbar_state = ScrollbarState::new(max_scroll)
        .position(current_scroll);

    let scrollbar_area = Rect {
        x: area.x + area.width - 1,
        y: area.y + 1,
        width: 1,
        height: area.height.saturating_sub(2),
    };

    frame.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
}
```

### 8.14 Checkbox/Toggle Indicators

Used for boolean options in dialogs, input forms, and modal confirmations.

**Format:**
```
[x] Option enabled
[ ] Option disabled
```

**Width:** Always 3 characters for column alignment

**Implementation:**
```rust
let checkbox = if app.state.option_enabled { "[x]" } else { "[ ]" };
let style = if is_focused {
    Style::default().fg(Color::Cyan).bold()
} else {
    Style::default().fg(Color::White)
};

lines.push(Line::from(Span::styled(
    format!("  {} Enable this option", checkbox),
    style,
)));
```

**Usage Contexts:**
- Bulk tag dialog: `[x] Save as rule`
- Rule creation: `[x] Enable rule`, `[ ] Run extraction job immediately`

**Navigation:**
- `↑/↓` or `j/k` to move between options
- `Space` to toggle checked state

**Note:** Multi-select lists (checkboxes per item) are not implemented. Bulk operations use filter-based selection instead.

---

## 9. Chat/Message Styles

### 9.1 Message Role Colors

| Role | Color | Prefix |
|------|-------|--------|
| User | Green | "You" |
| Assistant | Cyan | "Claude" |
| System | Yellow | "System" |
| Tool | Magenta | "Tool" |

### 9.2 Message Format

```rust
let (style, prefix) = match msg.role {
    MessageRole::User => (Style::default().fg(Color::Green), "You"),
    MessageRole::Assistant => (Style::default().fg(Color::Cyan), "Claude"),
    MessageRole::System => (Style::default().fg(Color::Yellow), "System"),
    MessageRole::Tool => (Style::default().fg(Color::Magenta), "Tool"),
};

// Format: "prefix [timestamp]: content"
```

---

## 10. Keybinding Display

### 10.1 Footer Format

```
" [key] Action  [key] Action  │  [key] Action "
     ^               ^        ^       ^
   bracket       action   separator  more actions
```

### 10.2 Style Conventions

```rust
// Standard footer
" [j/k] Navigate  [Enter] Select  [Esc] Back "

// Context divider
" ... │ [R] Rules [M] Sources │ 1:Source 2:Tags "

// Mode indicator
" Filter: pattern_ (X of Y) "  // Yellow, Bold
```

### 10.3 Common Keybindings

| Key | Action | Context |
|-----|--------|---------|
| `j/k` or `↑/↓` | Navigate up/down | Lists |
| `h/l` or `←/→` | Navigate left/right | Grids, drill-down |
| `Enter` | Confirm/Select | Everywhere |
| `Esc` | Cancel/Back | Everywhere |
| `/` | Filter/Search | Lists |
| `?` | Help | Everywhere |
| `q` | Quit | Top-level |
| `0` or `H` | Home | Everywhere |
| `1-4` | Quick nav to mode | Home |

---

## 11. Empty States

### 11.1 Pattern

```rust
// Centered, italic, gray text
let empty_msg = Paragraph::new("  No items found")
    .style(Style::default().fg(Color::DarkGray).italic());

// With action hint
let empty_msg = vec![
    "",
    "  No parsers found.",
    "",
    "  Add parsers to:",
    "    ~/.casparian_flow/parsers/",
    "",
    "  [n] Quick test any .py file",
];
```

### 11.2 Standard Messages

| Context | Message |
|---------|---------|
| Empty list | "No items found" |
| No matches (filter) | "No matches" + "[/] Clear filter" |
| No selection | "Select an item to view details" |
| Loading | "⠋ Loading..." (animated) |

---

## 12. Truncation

### 12.1 Path Truncation (Start)

```rust
/// Show END of path (most relevant)
/// "/very/long/path/to/file.py" -> ".../path/to/file.py"
fn truncate_path_start(path: &str, max_width: usize) -> String {
    if path.chars().count() <= max_width {
        return path.to_string();
    }
    let suffix: String = path.chars().rev().take(max_width - 4)
        .collect::<String>().chars().rev().collect();
    format!(".../{}", suffix)
}
```

### 12.2 Path Truncation (End)

```rust
/// Show START of path
/// "/very/long/path/to/file.py" -> "/very/long/pat..."
fn truncate_end(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        format!("{}...", s.chars().take(max_len - 3).collect::<String>())
    }
}
```

### 12.3 Tags Truncation

```rust
/// "[tag1, tag2, tag3]" -> "[tag1, ta...]"
fn truncate_tags(tags: &[String], max_width: usize) -> String
```

### 12.4 Count Formatting

```rust
/// 1500 -> "1.5K", 2500000 -> "2.5M"
pub fn format_count(count: usize) -> String {
    if count >= 1_000_000 {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    } else if count >= 1_000 {
        format!("{:.1}K", count as f64 / 1_000.0)
    } else {
        count.to_string()
    }
}
```

---

## 13. Accessibility

### 13.1 Color Independence

- Never rely solely on color to convey information
- Use symbols (✓, ✗, ●) alongside color
- Provide text labels for all states

### 13.2 ASCII Fallbacks

| Unicode | ASCII | Usage |
|---------|-------|-------|
| `●` | `*` | Health indicator |
| `✓` | `[x]` | Completed |
| `✗` | `[!]` | Failed |
| `▸` | `>` | Selection arrow |
| `▾` | `v` | Dropdown closed |
| `▲` | `^` | Dropdown open |
| `█░` | `#-` | Progress bar |

### 13.3 Minimum Contrast

- Primary text (White) on background
- Focused elements (Cyan) clearly distinguished
- Error states (Red) unmistakable
- Never use light colors on light backgrounds

### 13.4 Unicode Detection and Fallback Strategy

#### Current Implementation (v1.0)

The TUI currently uses **Unicode-first with no automatic detection**. Modern terminals (macOS Terminal, iTerm2, Windows Terminal, most Linux terminals) support Unicode by default.

**Rationale:**
- Target users are developers with modern terminal setups
- Unicode symbols provide better visual density and clarity
- Complexity of reliable cross-platform detection outweighs benefits

#### Symbol Mapping

| Category | Unicode | ASCII Fallback | Implementation Status |
|----------|---------|----------------|----------------------|
| **Spinner (loading)** | `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏` | `-\|/` | Both available via `spinner_char()` / `spinner_ascii()` |
| **Job Status** | `○↻✓✗⊘` | `[ ][>][x][!][~]` | Unicode only |
| **Parser Health** | `●⚠⏸○✗` | `*!P?X` | Unicode only |
| **Dropdown** | `▲▾` | `^v` | Unicode only |
| **Selection** | `▸►` | `>` | Unicode only |
| **Progress Bar** | `█░` | `#-` | Unicode only |
| **Sparkline** | `▁▂▃▄▅▆▇` | N/A | Unicode only |

#### When ASCII Spinner Is Used

The codebase uses `spinner_ascii()` specifically for **modal dialogs** (Scanning, Analyzing) where the spinner appears in the title bar, for visual consistency with the bracket-style title format `[X] Title`.

**Usage pattern:**
```rust
// Modal dialog titles use ASCII spinner
let spinner = spinner_ascii(app.tick_count);  // Returns: - \ | /
format!(" [{}] Scanning ", spinner)           // [|] Scanning

// Content area loading uses Unicode spinner
let spinner = spinner_char(app.tick_count);   // Returns: ⠋ ⠙ ⠹ etc.
format!("{} Loading folder hierarchy...", spinner)
```

#### Future Enhancement Path

If ASCII fallback mode is needed:

1. **Add CLI flag**: `--ascii` to TuiArgs struct
2. **Add environment check**: `$CASPARIAN_ASCII=1` or detect `$TERM=dumb`
3. **Implement symbol trait**: Create `SymbolSet` trait with `UnicodeSymbols` and `AsciiSymbols` implementations
4. **Store in App state**: `app.symbols: Box<dyn SymbolSet>`

**Decision:** Not implementing automatic detection for v1.0 - will add `--ascii` flag if user feedback indicates need.

---

## 14. Implementation Checklist

When creating new UI components:

- [ ] Define focused and unfocused border styles
- [ ] Use Double borders for focused state
- [ ] Use Rounded borders for unfocused state
- [ ] Apply Cyan for primary focus color
- [ ] Apply Yellow for active input/editing
- [ ] Apply DarkGray for muted/hint text
- [ ] Include status symbols with color
- [ ] Provide keyboard navigation hints in footer
- [ ] Handle empty states with helpful text
- [ ] Truncate long text appropriately
- [ ] Test with 80x24 minimum terminal size

---

## 15. Quick Reference

```rust
// Focus colors
FOCUSED:    Color::Cyan + BorderType::Double
UNFOCUSED:  Color::DarkGray + BorderType::Rounded
EDITING:    Color::Yellow + BorderType::Double

// Status colors
SUCCESS:    Color::Green
WARNING:    Color::Yellow
ERROR:      Color::Red
INFO:       Color::Blue
RUNNING:    Color::Blue

// Text colors
PRIMARY:    Color::White
SECONDARY:  Color::Gray
HINT:       Color::DarkGray
SELECTED:   Color::White + Bold + bg(DarkGray)

// Message roles
USER:       Color::Green
ASSISTANT:  Color::Cyan
SYSTEM:     Color::Yellow
TOOL:       Color::Magenta
```

---

## 16. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-14 | 1.0 | Initial specification from codebase analysis |
| 2026-01-14 | 1.1 | Added Section 5.5 Animation Timing (GAP-TIMING-001) |
| 2026-01-14 | 1.2 | Added Sections 8.8-8.13 (List patterns, Scrollbars), Section 13.4 (Unicode fallback) |
| 2026-01-14 | 1.3 | Added Section 5.6 (Animation hierarchy), Section 8.5 cursor guidance, Section 8.14 (Checkbox/Toggle) |
