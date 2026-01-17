# Settings - TUI View Spec

**Status:** Partial implementation
**Parent:** specs/tui.md (Master TUI Spec)
**Version:** 1.1
**Related:** specs/tui_style_guide.md
**Last Updated:** 2026-01-14

> **Note:** For global keybindings, layout patterns, and common UI elements,
> see the master TUI spec at `specs/tui.md`.

---

## 1. Overview

The **Settings** view manages runtime configuration in-memory. It is accessible
by pressing `,` from any mode. Persistence to `config.toml` is not implemented.

---

## 2. Layout (Current)

```
┌─ Settings ─────────────────────────────────────────────────────────────┐
│ General                                                            │
│  Default source path:  ~/data                                     │
│  Auto-scan on startup:  Yes                                       │
│  Confirm destructive:    Yes                                      │
│                                                                    │
│ Display                                                            │
│  Theme:               Dark                                        │
│  Unicode symbols:     Yes                                         │
│  Show hidden files:   No                                          │
│                                                                    │
│ About                                                              │
│  Version:    <app version>                                        │
│  Database:   <db path>                                            │
│  Config:     <config path>                                        │
│                                                                    │
│ [↑/↓] Navigate  [Enter] Edit/Toggle  [Tab] Category  [Esc] Back    │
└────────────────────────────────────────────────────────────────────┘
```

---

## 3. Keybindings (Current)

| Key | Action | Context |
|-----|--------|---------|
| `,` | Open Settings | Global |
| `Esc` | Back to previous mode | Settings |
| `↑/↓` | Move selection | Settings |
| `Enter` | Edit/Toggle | Settings |
| `Tab` / `Shift+Tab` | Switch category | Settings |

---

## 4. Notes / Planned

- Settings are not persisted to disk.
- No confirmation toast after changes.
- About section is read-only.

---

## 5. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-14 | 1.1 | Updated to match current Settings implementation |
