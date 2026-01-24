# Manual Testing Guide

## Quick Start

**Build the Rust binary:**
```bash
cargo build --release
```

**Start the system:**
```bash
./target/release/casparian start
```

**UI is running at: http://localhost:5000**

---

## Testing Rust Components

### TEST: Publish Command (Azure Authentication)

1. **Set Azure credentials** (if testing enterprise mode):
   ```bash
   export AZURE_TENANT_ID="your-tenant-id"
   export AZURE_CLIENT_ID="your-client-id"
   export AZURE_CLIENT_SECRET="your-secret"  # Optional for confidential clients
   ```

2. **Publish a plugin:**
   ```bash
   ./target/release/casparian publish my_plugin.py --version 1.0.0
   ```

3. **Verify:**
   - Device code flow prompts appear if using Azure
   - Plugin is signed with Ed25519
   - Artifact is deployed to Sentinel
   - Check logs for "Successfully published"

### TEST: Sentinel Server

1. **Start unified process:**
   ```bash
   ./target/release/casparian start
   ```

2. **Verify:**
   - Server binds to configured port (default: 5555)
   - Control plane runtime starts
   - Data plane runtime starts
   - Check logs for "Sentinel listening on tcp://..."

### TEST: Azure Integration (Real)

Run real Azure AD tests (requires credentials):
```bash
cargo test -p cf_security --test test_azure_real -- --ignored --nocapture
```

**Expected:**
- OpenID configuration fetched successfully
- Device code flow prompts for browser authentication
- Access token received after authentication
- Token type is "Bearer"

---

## Testing File Import Feature

## Test Environment Setup ✅

### Test Files Created
Located in: `test_files/` (relative to project root)

```
test_files/
├── data/
│   ├── sales_2024.csv      (5 rows of sales data)
│   └── inventory.csv        (4 rows of inventory data)
└── documents/
    ├── report.txt           (Text report)
    └── config.json          (JSON configuration)
```

### Plugins Configured

1. **sales_analyzer**
   - Subscriptions: `csv`, `sales`, `data`
   - Processes CSV files and generates analysis

2. **text_processor**
   - Subscriptions: `txt`, `doc`, `report`
   - Processes text files and extracts metadata

### Routing Rules

- `*.csv` → `csv` tag
- `sales*.csv` → `sales` tag (higher priority)
- `*.txt` → `txt` tag
- `*report*` → `report` tag
- `data/*` → `data` tag

---

## Step-by-Step Testing Instructions

### TEST 1: Browse and Import a CSV File (Sales Data)

1. **Open the Import Page**
   - Go to: http://localhost:5000/import
   - You should see the Import Files page

2. **Select Source Root**
   - In the dropdown, select the source root containing "test_files"
   - The ID should be 3
   - Click on it to load the file browser

3. **Navigate to Data Folder**
   - You should see two folders: `data` and `documents`
   - Click on the `data` folder name to browse into it

4. **View Available Files**
   - You should see:
     - ✅ `sales_2024.csv` (with a checkbox, not disabled)
     - ✅ `inventory.csv` (with a checkbox, not disabled)
   - Both should be selectable (NEW files)

5. **Select sales_2024.csv**
   - Check the checkbox next to `sales_2024.csv`

6. **Add Manual Tags**
   - In the "Tags" input field, type: `test,Q1,sales_data`

7. **Select Plugin Manually**
   - Check the box next to `sales_analyzer`

8. **Import the File**
   - Click the "📥 Import Selected Files" button
   - Wait for the success message

9. **Verify Success**
   - You should see: "✅ Successfully imported 1 file(s)"
   - The filename and ID should be displayed
   - Click "View in Inventory →" to see the imported file

**Expected Results:**
- File copied to `data/managed/` with timestamp prefix
- FileLocation created
- FileVersion created with tags: `csv,data,sales,test,Q1,sales_data`
- Manual tags: `test`, `Q1`, `sales_data`
- Jobs created:
  - `sales_analyzer` (manually selected)
  - No duplicate jobs (auto-routing skipped because already manually selected)

---

### TEST 2: Import a Text File (Auto-Routing Only)

1. **Go Back to Import Page**
   - Navigate to: http://localhost:5000/import
   - Select the test_files source root again

2. **Browse to Documents Folder**
   - Click on `documents` folder

3. **View Available Files**
   - You should see:
     - ✅ `report.txt`
     - ✅ `config.json`

4. **Select report.txt**
   - Check the checkbox next to `report.txt`

5. **Add Tags (No Manual Plugin Selection)**
   - Tags: `Q1,final`
   - **Do NOT select any plugins** (testing auto-routing only)

6. **Import the File**
   - Click "📥 Import Selected Files"

7. **Verify Success**
   - Check the success message
   - Click "View in Inventory →"

**Expected Results:**
- File copied to `data/managed/`
- Tags: `txt,report,Q1,final` (txt and report from auto-routing)
- Manual tags: `Q1`, `final`
- Job created:
  - `text_processor` (auto-matched via `txt` and `report` tags)

---

### TEST 3: Import Multiple Files with Mixed Configuration

1. **Return to Import Page**
   - http://localhost:5000/import
   - Select test_files source root

2. **Navigate to Data Folder**
   - Browse into `data/`

3. **Select Multiple Files**
   - Check both:
     - ✅ `sales_2024.csv`
     - ✅ `inventory.csv`
   - Note: sales_2024.csv might show as disabled if already imported (that's correct!)

4. **If sales_2024.csv is Disabled**
   - This means it's already in inventory (from TEST 1)
   - Just select `inventory.csv`

5. **Configure Import**
   - Tags: `inventory,stock,test`
   - Plugins: Check `sales_analyzer` AND `text_processor`

6. **Import**
   - Click "📥 Import Selected Files"

**Expected Results:**
- File(s) imported successfully
- Each file gets:
  - Manual tags: `inventory`, `stock`, `test`
  - Auto tags from routing rules
  - Jobs for BOTH manually selected plugins
  - Additional jobs from auto-routing (if tags match)

---

### TEST 4: Verify Database Entries

1. **Go to Inventory Page**
   - Click "📁 Inventory" in the nav menu
   - Or visit: http://localhost:5000/inventory

2. **Check Imported Files**
   - You should see all imported files
   - Each with an ID, filename, and tags
   - Files should show manual tags you added

3. **Go to Operations Page**
   - Click "⚙️ Operations" in the nav menu
   - Or visit: http://localhost:5000/operations

4. **Check Available Plugins**
   - In the plugin dropdown, you should see:
     - `sales_analyzer`
     - `text_processor`

---

### TEST 5: Browse Behavior Checks

1. **Return to Import Page**
   - http://localhost:5000/import

2. **Navigate Through Folders**
   - Click into `data/` folder
   - Click "⬆️ Parent Directory" to go back to root
   - Click into `documents/` folder
   - Verify navigation works smoothly

3. **Check File States**
   - Files you already imported should show "[In Inventory]"
   - Checkbox should be disabled
   - Files NOT imported should be selectable

4. **Test Empty Selection**
   - Don't select any files
   - Click "📥 Import Selected Files"
   - Should see: "⚠️ No files selected"

---

## Verification Checklist

After completing the tests, verify:

- [ ] Files appear in Inventory page
- [ ] Tags are displayed correctly
- [ ] Managed directory contains files: `ls data/managed/`
- [ ] Files have timestamp prefixes
- [ ] Previously imported files show as disabled
- [ ] Navigation between folders works
- [ ] Success messages appear after import
- [ ] Error handling works (empty selection, etc.)

---

## Checking Results in Database

Run this command to see what was created:

```bash
uv run python -c "
from sqlalchemy.orm import Session
from casparian_flow.db.access import get_engine
from casparian_flow.db.models import FileLocation, ProcessingJob
from casparian_flow.config import settings

engine = get_engine(settings.database)
db = Session(engine)

print('\n=== IMPORTED FILES ===')
files = db.query(FileLocation).all()
for f in files:
    print(f'ID {f.id}: {f.filename} (SourceRoot: {f.source_root_id})')

print('\n=== QUEUED JOBS ===')
jobs = db.query(ProcessingJob).all()
for j in jobs:
    print(f'Job {j.id}: {j.plugin_name} (Status: {j.status.value}, File: {j.file_id})')

db.close()
"
```

---

## Troubleshooting

### UI Not Loading
- Check server is running on port 5000
- Visit: http://localhost:5000
- Check console for errors

### Files Not Showing
- Verify SourceRoot path is correct
- Check file permissions
- Try refreshing the page

### Import Fails
- Check server logs in terminal
- Verify `data/managed/` directory exists
- Check file permissions

### No Jobs Created
- Verify plugins are configured
- Check routing rules match file names
- Review plugin subscription tags

---

## Clean Up After Testing

To reset the test environment:

```bash
uv run python scripts/setup_manual_test.py
```

This will:
- Delete test jobs
- Remove imported files from managed directory
- Keep plugins and routing rules for next test

---

## Expected File Flow

```
Source File (test_files/data/sales_2024.csv)
    ↓
[Import UI] Select file + tags + plugins
    ↓
ImportService.import_files()
    ↓
Copy to: data/managed/20241222_HHMMSS_XXXXXX_sales_2024.csv
    ↓
Database:
  - FileLocation (managed source root)
  - FileVersion (with hash)
  - FileTag (manual tags)
  - ProcessingJob (manual + auto plugins)
    ↓
[Inventory Page] View imported file
[Operations Page] See queued jobs
```

---

## Success Criteria

You've successfully tested the feature if:
1. ✅ You can browse folders in the UI
2. ✅ Files are selectable/disabled correctly
3. ✅ Import creates database entries
4. ✅ Files appear in managed directory
5. ✅ Tags are applied correctly
6. ✅ Jobs are created for selected plugins
7. ✅ Auto-routing creates additional jobs
8. ✅ Previously imported files are disabled

**Happy Testing!** 🎉
