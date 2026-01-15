# Settings - TUI View Spec

**Status:** Draft
**Parent:** specs/tui.md (Master TUI Spec)
**Version:** 1.0
**Related:** specs/tui_style_guide.md
**Last Updated:** 2026-01-14

> **Note:** For global keybindings, layout patterns, and common UI elements,
> see the master TUI spec at `specs/tui.md`.

---

## 1. Overview

The **Settings** view manages application configuration and preferences. Users access this view by pressing `,` from any mode.

### 1.1 Design Philosophy

- **Discoverable**: All configurable options visible in one place
- **Non-destructive**: Changes can be previewed before applying
- **Keyboard-driven**: All settings accessible without mouse
- **Persistent**: Settings saved to `~/.casparian_flow/config.toml`

### 1.2 Core Entities

```
~/.casparian_flow/
├── config.toml          # User preferences
└── casparian_flow.sqlite3  # Application data (not settings)
```

---

## 2. Layout

```
┌─ Settings ─────────────────────────────────────────────────────────┐
│                                                                    │
│  ┌─ General ────────────────────────────────────────────────────┐  │
│  │  Default source path:  ~/data                          [Edit]│  │
│  │  Auto-scan on startup: Yes                           [Toggle]│  │
│  │  Confirm destructive:  Yes                           [Toggle]│  │
│  └──────────────────────────────────────────────────────────────┘  │
│                                                                    │
│  ┌─ Display ────────────────────────────────────────────────────┐  │
│  │  Theme:               Dark                           [Cycle] │  │
│  │  Unicode symbols:     Yes                           [Toggle] │  │
│  │  Show hidden files:   No                            [Toggle] │  │
│  └──────────────────────────────────────────────────────────────┘  │
│                                                                    │
│  ┌─ About ──────────────────────────────────────────────────────┐  │
│  │  Version:    0.1.0                                           │  │
│  │  Database:   ~/.casparian_flow/casparian_flow.sqlite3       │  │
│  │  Config:     ~/.casparian_flow/config.toml                  │  │
│  └──────────────────────────────────────────────────────────────┘  │
│                                                                    │
│ [↑↓] Navigate  [Enter] Edit/Toggle  [Esc] Close  [?] Help         │
└────────────────────────────────────────────────────────────────────┘
```

---

## 3. Settings Categories

### 3.1 General

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `default_source_path` | Path | `~/data` | Default directory for new sources |
| `auto_scan_on_startup` | Bool | `true` | Scan existing sources on app start |
| `confirm_destructive` | Bool | `true` | Confirm before delete operations |

### 3.2 Display

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `theme` | Enum | `Dark` | Color theme (Dark, Light, System) |
| `unicode_symbols` | Bool | `true` | Use Unicode symbols (fallback to ASCII) |
| `show_hidden_files` | Bool | `false` | Include dotfiles in file lists |

### 3.3 About (Read-Only)

| Field | Description |
|-------|-------------|
| Version | Application version |
| Database | Path to SQLite database |
| Config | Path to config file |

---

## 4. Keybindings

| Key | Action | Context |
|-----|--------|---------|
| `,` | Open Settings | Any view |
| `Esc` | Close Settings | Settings view |
| `↑`/`k` | Previous setting | Settings view |
| `↓`/`j` | Next setting | Settings view |
| `Enter` | Edit/Toggle | Editable setting |
| `Tab` | Next category | Settings view |
| `Shift+Tab` | Previous category | Settings view |

---

## 5. State Machine

```
Any View ──(,)──> Settings ──(Esc)──> Previous View
                     │
                     │ Enter on editable
                     v
                  Editing ──(Enter/Esc)──> Settings
```

---

## 6. Config File Format

```toml
# ~/.casparian_flow/config.toml

[general]
default_source_path = "~/data"
auto_scan_on_startup = true
confirm_destructive = true

[display]
theme = "dark"
unicode_symbols = true
show_hidden_files = false
```

---

## 7. Implementation Notes

### 7.1 Config Loading

1. On startup, check for `~/.casparian_flow/config.toml`
2. If missing, create with defaults
3. Parse and validate settings
4. Apply to application state

### 7.2 Config Saving

1. On setting change, update in-memory state
2. Write to `config.toml` immediately (no "Save" button)
3. Show brief confirmation toast

---

## 8. Implementation Phases

- [ ] Phase 1: Read-only About section
- [ ] Phase 2: Display settings (theme, unicode)
- [ ] Phase 3: General settings
- [ ] Phase 4: Config file persistence

---

## 9. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-14 | 1.0 | Initial specification (created by spec_maintenance_workflow) |
