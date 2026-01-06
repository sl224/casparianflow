# Scout UI Demo

This demo shows the complete Scout file discovery and routing workflow in the Tauri UI.

## Prerequisites

1. Build the UI: `cd ui && bun run tauri build` or run dev mode: `bun run tauri dev`
2. Sample data is at: `ui/test-fixtures/scout/sample_data/`

## Demo Workflow

### Step 1: Open the App
- Launch the Tauri app (Casparian Deck)
- Click the **SCOUT** tab in the navigation

### Step 2: Add a Source
- Click **"+ Add Folder"** in the Sources section
- Navigate to `ui/test-fixtures/scout/sample_data/`
- Select the folder

### Step 3: Scan for Files
- The folder is automatically scanned after selection
- You should see 4 files discovered:
  - `sales_2024_01.csv` (113 bytes)
  - `sales_2024_02.csv` (113 bytes)
  - `inventory.json` (50 bytes)
  - `events.jsonl` (164 bytes)

### Step 4: Add Routes with Live Preview

**Route 1: CSV Sales Data**
- Click **"+ Add Route"**
- Name: `CSV Sales Data`
- Pattern: `*.csv`
- Output Path: `/output/bronze/sales`
- Watch the live preview update as you type the pattern
- You should see: **2 files matched (226 B)**
- Click **"Add Route"**

**Route 2: JSONL Events**
- Click **"+ Add Route"** again
- Name: `JSONL Events`
- Pattern: `*.jsonl`
- Output Path: `/output/bronze/events`
- Live preview shows: **1 file matched (164 B)**
- Click **"Add Route"**

### Step 5: Review Routes
- You should see both routes listed:
  - `*.csv` -> `/output/bronze/sales`
  - `*.jsonl` -> `/output/bronze/events`
- The `inventory.json` file is not matched by any route (unmatched)

### Step 6: Process Files
- Check the PROCESS section shows pending files
- Click **"Process Files"**
- Watch the processing complete
- Status updates to show processed files

### Step 7: Verify Output
- Check the output directories for Parquet files:
  - `/output/bronze/sales/*.parquet`
  - `/output/bronze/events/*.parquet`

## What to Observe

1. **Live Pattern Preview**: As you type a pattern, the matched file count updates in real-time
2. **Aggregates**: File counts and sizes are displayed as aggregates, not individual files
3. **Status Tracking**: File status changes from `pending` -> `processed`
4. **Error Handling**: Invalid patterns show error messages

## Sample Data

```
ui/test-fixtures/scout/sample_data/
├── sales_2024_01.csv     # CSV with sales data
├── sales_2024_02.csv     # CSV with sales data
├── inventory.json        # JSON product data
└── events.jsonl          # JSONL event stream
```

## Troubleshooting

- **No files showing**: Click "Scan" to rescan the folder
- **Process button disabled**: Add at least one route and have pending files
- **Pattern not matching**: Check glob syntax (e.g., `*.csv` not `*.CSV`)
