# Casparian Flow - Tauri UI

UI designs for the Casparian Flow desktop application built with Tauri.

## Design Source

All UI designs are located in `/designs/casparian-flow.pen` and can be edited using the Pencil design tool.

## Screens

### Core Screens

| Screen | Description | Node ID |
|--------|-------------|---------|
| Home | Dashboard with stats, ready outputs, active runs, quick actions | `qHBvJ` |
| Discover | File discovery with filtering and tagging | `JUlGC` |
| Parsers | Parser registry with details panel | `ab4Gm` |
| Jobs | Job queue monitoring with status filters | `yl6Pt` |
| Settings | Application configuration | `DpE4V` |

### MCP Control Plane Screens

| Screen | Description | Node ID |
|--------|-------------|---------|
| Approvals | Review and approve pending MCP operations | `IGqpB` |
| Query Console | Run SQL queries on output data | `HaS7o` |

## Design System

### Colors (Design Tokens)

- `$--background` - Main background
- `$--foreground` - Primary text
- `$--muted-foreground` - Secondary text
- `$--primary` - Primary accent (orange)
- `$--primary-foreground` - Text on primary
- `$--destructive` - Error/danger states
- `$--color-success-foreground` - Success states
- `$--color-warning-foreground` - Warning states
- `$--border` - Border color
- `$--muted` - Muted background (table headers, code areas)
- `$--sidebar` - Sidebar background
- `$--sidebar-foreground` - Sidebar text
- `$--sidebar-accent` - Sidebar active item background
- `$--sidebar-accent-foreground` - Sidebar active item text

### Typography

- **Primary Font**: Geist (UI text)
- **Monospace Font**: JetBrains Mono (code, data)
- **Icon Font**: Material Symbols Sharp

### Layout

- Screen size: 1440x900
- Sidebar width: 280px
- Content padding: 24px vertical, 32px horizontal
- Card border radius: 8px
- Button border radius: 6px

## Folder Structure

```
tauri-ui/
├── README.md          # This file
├── screens/           # Screen-specific components
├── components/        # Shared UI components
└── assets/            # Static assets (icons, images)
```

## Development

### Exporting from Pencil

1. Open `/designs/casparian-flow.pen` in Pencil
2. Select the screen frame by node ID
3. Export as desired format (SVG, PNG, or code)

### Implementing in Tauri

The designs follow a consistent pattern:
1. Left sidebar with navigation
2. Main content area with header + content
3. Stats row for dashboards (optional)
4. Data tables with headers and row actions

Recommended tech stack for implementation:
- **Frontend**: React + TypeScript
- **Styling**: Tailwind CSS
- **Icons**: Material Symbols
- **State**: Zustand or Redux
