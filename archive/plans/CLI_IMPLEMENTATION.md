# Casparian Flow CLI - Implementation Plan

**Last Updated:** January 2025
**Purpose:** World-class CLI that mirrors UI functionality with Jon Blow design principles.

---

## Design Philosophy

### Core Principles

1. **Verb-First Commands** - Action before noun: `casparian scan` not `casparian folder scan`
2. **Fast Feedback** - <1s for typical operations, streaming output for long ops
3. **Helpful Errors** - Every error includes what went wrong AND how to fix it
4. **Type-Preserving Interchange** - Binary formats (Arrow IPC), not text streams
5. **No Hidden State** - No project files, no server (unless explicitly started)
6. **Discoverable** - Help system teaches the tool organically

### Anti-Patterns to Avoid

- No interactive wizards (use flags instead)
- No "press enter to continue"
- No spinners without information
- No silent failures
- No config files required for basic usage
- **No text-based piping for data** (use Arrow IPC or file-based interchange)

### Why Not Unix Pipes?

Unix text pipes are fundamentally broken for data tools:

| Problem | Example |
|---------|---------|
| **Type Erasure** | `"123"` vs `123` vs `123.0` all become the same string |
| **Re-parsing Overhead** | Every tool parses → transforms → re-serializes |
| **Schema-less** | Downstream has no idea what structure to expect |
| **No Error Propagation** | Errors get lost or mixed with data |
| **Lossy Encoding** | Newlines in data? Good luck. |

**JSON is not the answer** - it's just "structured text" with most of the same problems.

### The Right Approach: Typed Binary Interchange

```bash
# BAD (Unix way - lossy, slow, fragile)
casparian scan ~/data --json | jq '.files[] | select(.size > 1000000)'

# GOOD (typed binary - fast, queryable)
casparian scan ~/data -o scan.arrow
casparian query scan.arrow "SELECT * FROM files WHERE size > 1MB"

# BEST (implicit result chaining)
casparian scan ~/data
casparian filter "size > 1MB"   # operates on last result (Arrow IPC)
```

**Output Modes:**

| Context | Format | Reason |
|---------|--------|--------|
| Terminal (human) | Pretty tables | Readable, truncated, colored |
| Piped to casparian | Arrow IPC | Zero-copy, typed, streamable |
| `-o file.arrow` | Arrow IPC file | Explicit typed output |
| `-o file.parquet` | Parquet | Compressed columnar storage |
| `-o file.sqlite` | SQLite | Ad-hoc SQL queries |

### Result Chaining (Replacing Pipes)

Instead of piping text, casparian maintains a **result cache** with typed Arrow data:

```
~/.casparian/
├── results/
│   ├── last.arrow              # Most recent result (any command)
│   ├── scan_20250105_143022.arrow
│   ├── filter_20250105_143025.arrow
│   └── query_20250105_143030.arrow
└── config.toml
```

**Commands operate on `last.arrow` by default:**

```bash
$ casparian scan ~/data
Found 1,247 files (156 MB total)
→ Result: ~/.casparian/results/last.arrow

$ casparian filter "type = 'csv' AND size > 1MB"
Filtered: 23 of 1,247 files match
→ Result: ~/.casparian/results/last.arrow (overwritten)

$ casparian query "SELECT type, COUNT(*) as n, SUM(size) as bytes FROM _ GROUP BY type"
┌─────────┬───────┬─────────────┐
│ type    │ n     │ bytes       │
├─────────┼───────┼─────────────┤
│ csv     │ 847   │ 93567232    │
│ json    │ 234   │ 47298560    │
└─────────┴───────┴─────────────┘
```

**Explicit chaining with `-i` (input):**

```bash
# Use specific input instead of last result
casparian filter -i scan.arrow "size > 100KB" -o filtered.arrow
casparian query -i filtered.arrow "SELECT * LIMIT 10"
```

**Why this is better than pipes:**

| Unix Pipes | Casparian Chaining |
|------------|-------------------|
| Text re-parsed at each step | Arrow zero-copy reads |
| Types lost | Types preserved end-to-end |
| Errors mixed with data | Proper error propagation |
| No random access | Seek to any offset |
| Memory-inefficient | Memory-mapped files |

---

## Command Hierarchy

```
casparian
├── scan        # Discover files in a path
├── preview     # Preview file contents/structure
├── generate    # AI-generate a parser
├── test        # Test a parser against files
├── fix         # AI-fix parser errors
├── run         # Process files at scale
├── query       # SQL against output
├── publish     # Deploy parser as plugin
├── config      # Settings and state
└── help        # World-class help system
```

---

## Phase Roadmap

### Phase 1: See Your Data (Foundation)

**Commands:** `scan`, `preview`

**Value:** User can see what they have before doing anything.

```bash
# Discover files (human-readable output)
casparian scan ~/data
casparian scan ~/data --type csv --recursive
casparian scan ~/data --stats

# Save to typed format for downstream tools
casparian scan ~/data -o scan.arrow
casparian scan ~/data -o scan.parquet

# Preview a file
casparian preview ~/data/sales.csv
casparian preview ~/data/sales.csv --rows 50
casparian preview ~/data/sales.csv --schema

# Inspect binary files
casparian preview ~/data/mystery.bin --magic
casparian preview ~/data/mystery.bin --hex --annotate
```

### Phase 2: Generate Parsers (AI Core)

**Commands:** `generate`, `test`, `fix`

**Value:** AI creates parser, user validates output.

```bash
# Generate parser from file
casparian generate ~/data/sales.csv
casparian generate ~/data/sales.csv --name sales_parser

# Test parser
casparian test parser.py --file ~/data/sales.csv
casparian test parser.py --dir ~/data/sales/

# AI-fix errors
casparian fix parser.py --error "KeyError: 'amount'"
```

### Phase 3: Run at Scale (Processing)

**Commands:** `run`

**Value:** Process thousands of files efficiently.

```bash
# Process files
casparian run parser.py --dir ~/data/
casparian run parser.py --tag sales_data
casparian run parser.py --dir ~/data/ --output ~/output/ --format parquet

# Watch mode
casparian run parser.py --watch ~/data/
```

### Phase 4: Query Output (Analysis)

**Commands:** `query`

**Value:** SQL against Parquet/SQLite output.

```bash
# Query output
casparian query "SELECT * FROM sales LIMIT 10"
casparian query "SELECT date, SUM(amount) FROM sales GROUP BY date"
casparian query --file ~/output/sales.parquet "SELECT * LIMIT 5"
```

### Phase 5: Configuration (Polish)

**Commands:** `config`, `publish`

**Value:** Persistent settings, plugin deployment.

```bash
# Config
casparian config set api_key sk-...
casparian config get api_key
casparian config list

# Publish parser as plugin
casparian publish parser.py --name "Sales Parser" --tag sales_data
```

---

## World-Class Help System

### Design Goals

1. **Layered Detail** - Brief by default, verbose with `--help`
2. **Examples First** - Show what to do, not just what's possible
3. **Contextual** - Help adapts to what you're trying to do
4. **No Walls of Text** - Scannable, not readable

### Help Hierarchy

```bash
# Level 0: Command list
casparian
casparian --help

# Level 1: Command overview
casparian scan --help

# Level 2: Deep dive
casparian scan --help-full
casparian help scan
```

### Example Help Output

```
$ casparian

Casparian Flow - Transform messy files into queryable data

USAGE:
    casparian <command> [options]

COMMANDS:
    scan        Discover files in a directory
    preview     Preview file contents and structure
    generate    AI-generate a parser from a sample file
    test        Test a parser against files
    run         Process files at scale

QUICK START:
    $ casparian scan ~/data
    $ casparian preview ~/data/sales.csv
    $ casparian generate ~/data/sales.csv

Run 'casparian <command> --help' for command details.
```

```
$ casparian scan --help

Discover files in a directory

USAGE:
    casparian scan <path> [options]

EXAMPLES:
    casparian scan ~/data                      # All files
    casparian scan ~/data --type csv           # Only CSVs
    casparian scan ~/data --recursive --json   # Deep scan, JSON output

OPTIONS:
    -t, --type <ext>     Filter by extension (csv, json, log, parquet)
    -r, --recursive      Include subdirectories
    -d, --depth <n>      Max recursion depth (default: unlimited)
    --json               Output as JSON (for piping)
    --stats              Show aggregate statistics

OUTPUT COLUMNS:
    PATH       Relative path from scan root
    SIZE       Human-readable file size
    MODIFIED   Last modification time
    TYPE       Detected file type

SEE ALSO:
    casparian preview    Preview a specific file
    casparian generate   Generate parser from file
```

### Error Messages

Every error follows this pattern:

```
ERROR: <what went wrong>

<context if helpful>

TRY:
    <specific command to fix it>
```

Examples:

```
$ casparian scan /nonexistent

ERROR: Directory not found: /nonexistent

TRY:
    casparian scan ~/data     # Use an existing directory
    ls -la /                  # List root to find your data
```

```
$ casparian test parser.py --file data.csv

ERROR: Parser failed on line 15: KeyError: 'amount'

The parser expected a column 'amount' but the file has: date, value, category

TRY:
    casparian preview data.csv --schema   # See actual columns
    casparian fix parser.py --error "KeyError: 'amount'"   # AI-fix
```

```
$ casparian generate data.csv

ERROR: No API key configured

AI generation requires an Anthropic API key.

TRY:
    casparian config set api_key sk-ant-...   # Set your key
    casparian generate data.csv --key sk-...  # Or pass it directly
```

---

## Output Formats

### Default: Human-Readable

```
$ casparian scan ~/data

Found 47 files in ~/data

PATH                    SIZE      MODIFIED         TYPE
sales_2024.csv          2.3 MB    2024-12-15       csv
transactions.json       890 KB    2024-12-14       json
access.log              15 MB     2024-12-16       log
...

Summary: 47 files, 156 MB total
  CSV: 23 files (89 MB)
  JSON: 15 files (45 MB)
  LOG: 9 files (22 MB)
```

### JSON: Machine-Readable

```
$ casparian scan ~/data --json

{
  "path": "/Users/shan/data",
  "files": [
    {"path": "sales_2024.csv", "size": 2412544, "modified": "2024-12-15T10:30:00Z", "type": "csv"},
    {"path": "transactions.json", "size": 911360, "modified": "2024-12-14T14:22:00Z", "type": "json"}
  ],
  "summary": {
    "total_files": 47,
    "total_bytes": 163577856,
    "by_type": {"csv": 23, "json": 15, "log": 9}
  }
}
```

### Composability Examples

```bash
# Count CSV files
casparian scan ~/data --json | jq '.files | map(select(.type == "csv")) | length'

# Find large files
casparian scan ~/data --json | jq '.files | map(select(.size > 10000000))'

# Process with other tools
casparian preview data.csv --json | jq '.rows' | head -100

# Pipe to run
casparian scan ~/data --type csv --json | jq -r '.files[].path' | xargs -I{} casparian test parser.py --file {}
```

---

## Progress and Streaming

### Long Operations

For operations >2s, stream progress:

```
$ casparian run parser.py --dir ~/data/

Processing 1,247 files...

[=====>                    ] 23% (287/1,247) sales_2024_03.csv
  Elapsed: 45s | ETA: 2m 30s | Rate: 6.4 files/s

Errors (3):
  ✗ corrupt_file.csv - Invalid UTF-8 at byte 1024
  ✗ empty.csv - No data rows
  ✗ weird_format.csv - Schema mismatch (expected 5 cols, got 7)
```

### Streaming Output

```
$ casparian test parser.py --file huge.csv --stream

Row 1: {"date": "2024-01-01", "amount": 150.00, "category": "sales"}
Row 2: {"date": "2024-01-02", "amount": 275.50, "category": "refund"}
Row 3: {"date": "2024-01-03", "amount": 89.99, "category": "sales"}
...
^C (Ctrl+C to stop)
```

---

## Implementation Details

### Binary Structure

```
crates/casparian_cli/
├── Cargo.toml
├── src/
│   ├── main.rs           # Entry point, arg parsing
│   ├── commands/
│   │   ├── mod.rs
│   │   ├── scan.rs       # Phase 1
│   │   ├── preview.rs    # Phase 1
│   │   ├── generate.rs   # Phase 2
│   │   ├── test.rs       # Phase 2
│   │   ├── fix.rs        # Phase 2
│   │   ├── run.rs        # Phase 3
│   │   ├── query.rs      # Phase 4
│   │   ├── config.rs     # Phase 5
│   │   └── publish.rs    # Phase 5
│   ├── output/
│   │   ├── mod.rs
│   │   ├── table.rs      # Human-readable tables
│   │   ├── json.rs       # JSON output
│   │   └── progress.rs   # Progress bars/streaming
│   └── error.rs          # Helpful error formatting
```

### Dependencies

```toml
[dependencies]
clap = { version = "4", features = ["derive"] }  # Arg parsing
serde_json = "1"                                  # JSON output
indicatif = "0.17"                               # Progress bars
comfy-table = "7"                                # Table formatting
tokio = { version = "1", features = ["full"] }   # Async runtime
```

### Shared Code with UI

The CLI should reuse core logic from existing crates:

```rust
// CLI command
use casparian_scout::{Scanner, ScanOptions};
use casparian_worker::{Bridge, ParserRunner};

pub fn scan(path: &Path, opts: ScanOptions) -> Result<ScanResult> {
    // Same logic as Tauri command, different output
    let scanner = Scanner::new(path)?;
    scanner.scan(opts)
}
```

---

## Testing Strategy

### Unit Tests

```rust
#[test]
fn test_scan_empty_dir() {
    let result = scan(temp_dir(), ScanOptions::default());
    assert!(result.files.is_empty());
}

#[test]
fn test_helpful_error_missing_dir() {
    let err = scan(Path::new("/nonexistent"), ScanOptions::default()).unwrap_err();
    assert!(err.to_string().contains("TRY:"));
}
```

### Integration Tests

```bash
# tests/cli_integration.sh

# Test scan
output=$(casparian scan ./test_data)
[[ "$output" == *"Found"* ]] || exit 1

# Test JSON output
output=$(casparian scan ./test_data --json)
echo "$output" | jq . || exit 1

# Test error handling
output=$(casparian scan /nonexistent 2>&1)
[[ "$output" == *"TRY:"* ]] || exit 1
```

### Snapshot Tests

```rust
#[test]
fn test_help_output() {
    let output = Command::new("casparian")
        .arg("--help")
        .output()
        .unwrap();

    insta::assert_snapshot!(String::from_utf8_lossy(&output.stdout));
}
```

---

## Flag Definitions (World-Class Specification)

### `scan` Command - Complete Flag Reference

```
casparian scan <path> [options]

REQUIRED:
    <path>                  Directory to scan

OUTPUT MODE (mutually exclusive - pick one):
    (default)               Human-readable table to stdout (TTY only)
    -o, --output <file>     Write to typed file. Format from extension:
                              .arrow   → Arrow IPC (fast, typed, streamable)
                              .parquet → Parquet (compressed columnar)
                              .sqlite  → SQLite database (queryable)
                              .csv     → CSV (legacy compat, LOSSY - warns)
    --stats                 Statistics only (counts by type, total size)
    --paths                 One absolute path per line (for xargs)
    --count                 Just the file count (integer to stdout)

FILTERS:
    -t, --type <ext>        Filter by extension. Repeatable or comma-separated:
                              -t csv -t json    (two filters)
                              -t csv,json,parquet (comma-separated)
                            Case-insensitive. Matches without dot.

    -r, --recursive         Include subdirectories (default: current dir only)

    -d, --depth <n>         Max recursion depth. Requires -r.
                              --depth 1 = immediate children only
                              --depth 2 = children and grandchildren
                            Default when -r: unlimited

    --min-size <size>       Minimum file size (inclusive)
                            Format: <number><unit>
                              Units: B, KB, MB, GB (case-insensitive)
                              Examples: 100KB, 10MB, 1.5GB, 1024B
                            Files smaller than this are excluded.

    --max-size <size>       Maximum file size (inclusive). Same format.

    --newer <datetime>      Files modified after this time.
                            Formats accepted:
                              ISO 8601: 2024-01-15, 2024-01-15T10:30:00
                              Relative: "1 hour ago", "yesterday", "last week"

    --older <datetime>      Files modified before this time. Same formats.

    --name <pattern>        Filename glob pattern (not path).
                              --name "sales_*.csv"
                              --name "2024-??-??.log"

    --exclude <pattern>     Exclude files matching pattern. Repeatable.
                              --exclude "*.tmp" --exclude ".DS_Store"

BEHAVIOR:
    --follow-symlinks       Follow symbolic links (default: skip)
    --include-hidden        Include dotfiles/dotdirs (default: skip)
    --no-ignore             Don't respect .gitignore (default: respects it)

PERFORMANCE:
    --parallel <n>          Parallel directory walking threads (default: auto)
    --limit <n>             Stop after finding n files

EXIT CODES:
    0   Success
    1   No files found (with filters)
    2   Path does not exist
    3   Permission denied
    4   Invalid arguments
```

### `preview` Command - Complete Flag Reference

```
casparian preview <file> [options]

REQUIRED:
    <file>                  File to preview

OUTPUT MODE (mutually exclusive):
    (default)               Human-readable: schema + data table
    --schema                Schema only, no data rows
    --magic                 Magic bytes + detected format only
    --hex                   Hex dump with ASCII sidebar
    -o, --output <file>     Write parsed data to file (format from extension)

DATA SELECTION:
    -n, --rows <n>          Rows to show (default: 20)
    --offset <n>            Skip first n rows
    --sample <n>            Random sample of n rows (not first n)
    --all                   All rows (WARNING: may be huge)
    --columns <list>        Only these columns: --columns "id,name,amount"

FORMAT HINTS (when auto-detection fails):
    --format <fmt>          Force format interpretation:
                              csv, tsv, json, ndjson, jsonl,
                              parquet, arrow, avro, orc,
                              log, text, binary

    --delimiter <char>      Force CSV/TSV delimiter:
                              comma, tab, pipe, semicolon, space
                              Or literal: --delimiter "|"

    --quote <char>          CSV quote character (default: ")

    --encoding <enc>        Force text encoding:
                              utf8, utf16, utf16le, utf16be,
                              latin1, ascii, cp1252
                            Default: auto-detect

    --no-header             CSV/TSV has no header row (use col_0, col_1, ...)
    --header-row <n>        Header is on row n (0-indexed), skip rows before

BINARY INSPECTION:
    --hex                   Show hex dump instead of parsed data
    --hex-offset <n>        Start hex dump at byte offset (default: 0)
    --hex-length <n>        Bytes to show (default: 256)
    --hex-width <n>         Bytes per row (default: 16)
    --hex-annotate          Add structure annotations (detect patterns)

TYPE INFERENCE:
    --infer-rows <n>        Rows to sample for type inference (default: 1000)
    --strict-types          Don't fall back to string on mixed types
    --date-formats <list>   Additional date formats to recognize:
                              --date-formats "%d/%m/%Y,%m-%d-%Y"

EXIT CODES:
    0   Success
    1   File not found
    2   Permission denied
    3   Unknown format (use --format to force)
    4   Parse error
    5   Invalid arguments
```

---

## Binary File Handling (World-Class Approach)

Binary files aren't just "raw bytes" - they have **structure**. Show the structure.

### Recognized Format Detection

Use magic bytes, not file extensions:

| Magic Bytes | Format | Action |
|-------------|--------|--------|
| `50 41 52 31` | Parquet | Show schema, row groups, sample data |
| `41 52 52 4F 57 31` | Arrow IPC | Show schema, record batches |
| `53 51 4C 69 74 65` | SQLite | Show tables, row counts |
| `89 50 4E 47` | PNG | Show dimensions, color depth |
| `FF D8 FF` | JPEG | Show dimensions, EXIF |
| `50 4B 03 04` | ZIP/XLSX/DOCX | List contents |
| `7B` (+ structure) | JSON | Parse and preview |
| `{` per line | NDJSON | Stream preview |
| BOM or text heuristics | Text/CSV | Parse with delimiter detection |

### Preview Output by Format Type

**Columnar Data (Parquet, Arrow, ORC):**
```
File: sales_2024.parquet
Format: Apache Parquet v2.6
Size: 45.2 MB (compressed) → ~120 MB (uncompressed)
Compression: Snappy
Created: polars-0.20.0

STRUCTURE:
  Row groups: 12
  Total rows: 1,234,567

SCHEMA:
  id        : INT64 (not null)        ← primary key candidate
  name      : STRING (nullable)       ← 0.3% nulls
  amount    : FLOAT64 (nullable)      ← range: -1000.00 to 99999.99
  timestamp : TIMESTAMP[us, UTC]      ← 2024-01-01 to 2024-12-31

DATA (rows 0-19 of 1,234,567):
┌──────────┬─────────────┬──────────┬─────────────────────┐
│ id       │ name        │ amount   │ timestamp           │
│ int64    │ string      │ float64  │ timestamp[us, UTC]  │
╞══════════╪═════════════╪══════════╪═════════════════════╡
│ 1        │ Alice       │ 150.50   │ 2024-01-15 10:30:00 │
│ 2        │ Bob         │ 275.00   │ 2024-01-15 11:45:00 │
│ 3        │ ∅           │ ∅        │ 2024-01-15 12:00:00 │
└──────────┴─────────────┴──────────┴─────────────────────┘
```

**Database (SQLite):**
```
File: app.db
Format: SQLite 3.x
Size: 12.4 MB
Page size: 4096

TABLES:
  users        : 45,231 rows, 12 columns
  orders       : 234,567 rows, 8 columns
  products     : 1,234 rows, 15 columns
  order_items  : 891,234 rows, 5 columns

SCHEMA (users):
  id         : INTEGER PRIMARY KEY
  email      : TEXT NOT NULL UNIQUE
  name       : TEXT
  created_at : TEXT (ISO 8601 dates detected)

Use: casparian preview app.db --table orders
```

**Image (PNG, JPEG):**
```
File: screenshot.png
Format: PNG (Portable Network Graphics)
Size: 2.3 MB

PROPERTIES:
  Dimensions: 1920 × 1080 px
  Bit depth: 8
  Color type: RGBA (with alpha)
  Compression: zlib deflate
  Interlaced: No

This is an image file, not tabular data.
Use --hex for raw bytes, or open in an image viewer.
```

**Unknown Binary:**
```
File: mystery.bin
Format: Unknown
Size: 8.4 KB

MAGIC BYTES: 00 00 00 01 00 00 00 48 65 6C 6C 6F

ANALYSIS:
  Null bytes: 12%
  Printable ASCII: 34%
  High bytes (>127): 54%
  Entropy: 4.2 bits/byte (low - likely structured, not compressed)

DETECTED PATTERNS:
  • Fixed-size records: 64 bytes each (130 records)
  • Little-endian integers at offsets 0, 4, 8
  • Null-terminated strings at offset 16

Use --hex --annotate for detailed structure view.
```

### Hex Dump with Annotations

```
$ casparian preview mystery.bin --hex --annotate

File: mystery.bin (8,320 bytes = 130 × 64-byte records)

RECORD 0 (offset 0x0000):
┌─────────┬────────────────────────────────────────────────┬──────────────────┐
│ OFFSET  │ 00 01 02 03 04 05 06 07  08 09 0A 0B 0C 0D 0E 0F │ ASCII            │
├─────────┼────────────────────────────────────────────────┼──────────────────┤
│ 0x0000  │ 01 00 00 00 E8 03 00 00  00 00 00 00 00 00 00 00 │ ................│
│         │ └─ id: 1    └─ value: 1000                       │                  │
│ 0x0010  │ 48 65 6C 6C 6F 20 57 6F  72 6C 64 00 00 00 00 00 │ Hello World.....│
│         │ └─ name[32]: "Hello World"                       │                  │
│ 0x0020  │ 00 00 00 00 00 00 00 00  00 00 00 00 00 00 00 00 │ ................│
│ 0x0030  │ 00 00 C8 42 00 00 00 00  00 00 00 00 00 00 00 00 │ ...B............│
│         │ └─ float32: 100.0                                │                  │
└─────────┴────────────────────────────────────────────────┴──────────────────┘

INFERRED RECORD STRUCTURE (64 bytes):
  struct Record {
      id: u32,          // offset 0
      value: u32,       // offset 4
      _pad1: [u8; 8],   // offset 8
      name: [u8; 32],   // offset 16 (null-terminated string)
      _pad2: [u8; 12],  // offset 48
      amount: f32,      // offset 60
  }
```

---

## Phase 1 Detailed Design

### `scan` Command

**Purpose:** Discover files in a directory.

**Implementation:**

```rust
// src/commands/scan.rs

use clap::Args;
use std::path::PathBuf;

#[derive(Args)]
pub struct ScanArgs {
    /// Directory to scan
    path: PathBuf,

    /// Filter by file extension
    #[arg(short = 't', long = "type")]
    types: Vec<String>,

    /// Include subdirectories
    #[arg(short, long)]
    recursive: bool,

    /// Max recursion depth
    #[arg(short, long)]
    depth: Option<usize>,

    /// Output as JSON
    #[arg(long)]
    json: bool,

    /// Show only statistics
    #[arg(long)]
    stats: bool,

    /// Quiet mode (just file paths)
    #[arg(short, long)]
    quiet: bool,
}

pub fn execute(args: ScanArgs) -> Result<()> {
    // Validate path exists
    if !args.path.exists() {
        return Err(HelpfulError::new(
            format!("Directory not found: {}", args.path.display()),
            vec![
                format!("casparian scan ~/data     # Use an existing directory"),
                format!("ls -la {}                 # Check what exists", args.path.parent().unwrap_or(&args.path).display()),
            ],
        ));
    }

    if !args.path.is_dir() {
        return Err(HelpfulError::new(
            format!("Not a directory: {}", args.path.display()),
            vec![
                format!("casparian preview {}      # Preview this file instead", args.path.display()),
                format!("casparian scan {}         # Scan the parent directory", args.path.parent().unwrap_or(&args.path).display()),
            ],
        ));
    }

    // Perform scan
    let options = ScanOptions {
        types: args.types,
        recursive: args.recursive,
        max_depth: args.depth,
        ..Default::default()
    };

    let result = Scanner::new(&args.path)?.scan(options)?;

    // Output
    if args.json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else if args.stats {
        print_stats(&result);
    } else if args.quiet {
        for file in &result.files {
            println!("{}", file.path.display());
        }
    } else {
        print_table(&result);
    }

    Ok(())
}
```

### `preview` Command

**Purpose:** Preview file contents and infer structure.

**Arguments:**
```
USAGE:
    casparian preview <file> [options]

ARGS:
    <file>    File to preview (required)

OPTIONS:
    -n, --rows <n>       Number of rows to show (default: 20)
    --schema             Show inferred schema only
    --raw                Show raw file contents (no parsing)
    --head <n>           Show first n bytes (for binary/large files)
    --encoding <enc>     Force encoding (utf-8, latin-1, etc.)
    --delimiter <char>   Force CSV delimiter
    --json               Output as JSON
```

**Implementation:**

```rust
// src/commands/preview.rs

#[derive(Args)]
pub struct PreviewArgs {
    /// File to preview
    file: PathBuf,

    /// Number of rows to show
    #[arg(short = 'n', long, default_value = "20")]
    rows: usize,

    /// Show inferred schema only
    #[arg(long)]
    schema: bool,

    /// Show raw contents (no parsing)
    #[arg(long)]
    raw: bool,

    /// Output as JSON
    #[arg(long)]
    json: bool,
}

pub fn execute(args: PreviewArgs) -> Result<()> {
    if !args.file.exists() {
        return Err(HelpfulError::new(
            format!("File not found: {}", args.file.display()),
            vec![
                format!("casparian scan {}  # Find files in this directory",
                    args.file.parent().unwrap_or(Path::new(".")).display()),
            ],
        ));
    }

    // Detect file type
    let file_type = detect_type(&args.file)?;

    if args.raw {
        // Raw mode: just dump bytes
        let content = fs::read_to_string(&args.file)
            .map_err(|e| HelpfulError::new(
                format!("Cannot read file: {}", e),
                vec![format!("casparian preview {} --head 1000  # Try binary preview", args.file.display())],
            ))?;
        println!("{}", &content[..content.len().min(10000)]);
        return Ok(());
    }

    // Parse based on type
    let preview = match file_type {
        FileType::Csv => preview_csv(&args.file, args.rows)?,
        FileType::Json => preview_json(&args.file, args.rows)?,
        FileType::Log => preview_log(&args.file, args.rows)?,
        FileType::Parquet => preview_parquet(&args.file, args.rows)?,
        FileType::Unknown => {
            return Err(HelpfulError::new(
                format!("Unknown file type: {}", args.file.display()),
                vec![
                    format!("casparian preview {} --raw  # View raw contents", args.file.display()),
                ],
            ));
        }
    };

    // Output
    if args.json {
        println!("{}", serde_json::to_string_pretty(&preview)?);
    } else if args.schema {
        print_schema(&preview.schema);
    } else {
        print_preview_table(&preview);
    }

    Ok(())
}
```

### Output Formatting

```rust
// src/output/table.rs

use comfy_table::{Table, Row, Cell, Color};

pub fn print_scan_table(result: &ScanResult) {
    println!("\nFound {} files in {}\n", result.files.len(), result.path.display());

    let mut table = Table::new();
    table.set_header(vec!["PATH", "SIZE", "MODIFIED", "TYPE"]);

    for file in &result.files {
        table.add_row(vec![
            &file.rel_path,
            &format_size(file.size),
            &format_time(file.modified),
            &file.file_type,
        ]);
    }

    println!("{table}");

    // Summary
    println!("\nSummary: {} files, {} total", result.files.len(), format_size(result.total_size));
    for (typ, count) in &result.by_type {
        println!("  {}: {} files", typ, count);
    }
}

pub fn print_preview_table(preview: &FilePreview) {
    // Schema header
    println!("\nFile: {}", preview.path.display());
    println!("Type: {} | Rows: {} | Size: {}\n",
        preview.file_type,
        preview.total_rows.map_or("?".to_string(), |n| n.to_string()),
        format_size(preview.size)
    );

    // Schema
    println!("SCHEMA:");
    for col in &preview.schema.columns {
        println!("  {} : {} {}",
            col.name,
            col.dtype,
            if col.nullable { "(nullable)" } else { "" }
        );
    }
    println!();

    // Data table
    let mut table = Table::new();
    table.set_header(preview.schema.columns.iter().map(|c| &c.name));

    for row in &preview.rows {
        table.add_row(row.values.iter().map(|v| format_value(v)));
    }

    println!("{table}");

    if let Some(total) = preview.total_rows {
        if total > preview.rows.len() {
            println!("\n... and {} more rows", total - preview.rows.len());
        }
    }
}
```

### Error Formatting

```rust
// src/error.rs

pub struct HelpfulError {
    message: String,
    suggestions: Vec<String>,
}

impl HelpfulError {
    pub fn new(message: impl Into<String>, suggestions: Vec<String>) -> Self {
        Self {
            message: message.into(),
            suggestions,
        }
    }
}

impl std::fmt::Display for HelpfulError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "ERROR: {}", self.message)?;
        if !self.suggestions.is_empty() {
            writeln!(f)?;
            writeln!(f, "TRY:")?;
            for suggestion in &self.suggestions {
                writeln!(f, "    {}", suggestion)?;
            }
        }
        Ok(())
    }
}
```

---

## Migration from UI Commands

The CLI should reuse the same core logic as Tauri commands. Here's the mapping:

| CLI Command | Tauri Command | Shared Logic |
|-------------|---------------|--------------|
| `scan` | `scout_scan_source` | `casparian_scout::Scanner` |
| `preview` | `parser_lab_validate_parser` (partial) | Type detection, schema inference |
| `generate` | `parser_lab_generate_parser` (not yet) | AI client, prompt templates |
| `test` | `parser_lab_validate_parser` | `casparian_worker::Bridge` |
| `run` | `process_queue` | `casparian_worker::Bridge` |
| `query` | (new) | DuckDB/SQLite query engine |
| `publish` | `deploy_plugin` | Plugin signing, DB insert |
| `config` | (new) | Config file management |

---

## Milestones

### v0.1.0 - Foundation

- [ ] `casparian scan` with all options
- [ ] `casparian preview` for CSV, JSON
- [ ] Human-readable and JSON output
- [ ] Helpful error messages
- [ ] Help system (--help for all commands)

### v0.2.0 - AI Integration

- [ ] `casparian generate` with Anthropic API
- [ ] `casparian test` with Bridge mode
- [ ] `casparian fix` for error recovery

### v0.3.0 - Scale

- [ ] `casparian run` with progress
- [ ] Parallel file processing
- [ ] Watch mode

### v0.4.0 - Query

- [ ] `casparian query` with DuckDB
- [ ] Multi-file queries
- [ ] Output to various formats

### v0.5.0 - Polish

- [ ] `casparian config` for all settings
- [ ] `casparian publish` for plugin deployment
- [ ] Shell completions (bash, zsh, fish)
- [ ] Man pages

---

## Appendix: Jon Blow Principles Applied

| Principle | Application |
|-----------|-------------|
| **Direct** | No wizards, no prompts. Flags, not interactive menus. |
| **Fast** | <1s startup. Stream output for long operations. |
| **Honest** | Show real progress, real errors, real data. |
| **Composable** | JSON output, exit codes, stdin/stdout. |
| **Helpful** | Every error has a fix. Help teaches, not just documents. |
| **No Magic** | Explicit flags, not auto-detection that surprises. |

---

## Phase 1 Implementation Spec

### Overview

Phase 1 adds two new subcommands to the existing `casparian` binary:
- `scan` - Standalone filesystem discovery (no database required)
- `preview` - File content preview with schema inference

These integrate with the existing command structure in `main.rs`.

### Code Structure

```
crates/casparian/src/
├── main.rs                    # Add Scan and Preview to Commands enum
├── commands/
│   ├── mod.rs                 # Export new commands
│   ├── scan.rs                # Scan implementation
│   └── preview.rs             # Preview implementation
├── output/
│   ├── mod.rs                 # Output formatting
│   ├── table.rs               # Human-readable tables
│   └── json.rs                # JSON output
└── error.rs                   # Helpful error types
```

### Adding to main.rs

```rust
// Add to Commands enum
#[derive(Subcommand, Debug)]
enum Commands {
    // ... existing commands ...

    /// Discover files in a directory (no database required)
    Scan {
        /// Directory to scan
        path: std::path::PathBuf,

        /// Filter by file extension (can repeat: --type csv --type json)
        #[arg(short = 't', long = "type")]
        types: Vec<String>,

        /// Include subdirectories
        #[arg(short, long)]
        recursive: bool,

        /// Max recursion depth
        #[arg(short, long)]
        depth: Option<usize>,

        /// Minimum file size (e.g., 1KB, 10MB)
        #[arg(long)]
        min_size: Option<String>,

        /// Maximum file size
        #[arg(long)]
        max_size: Option<String>,

        /// Output as JSON (for piping)
        #[arg(long)]
        json: bool,

        /// Show only statistics
        #[arg(long)]
        stats: bool,

        /// Quiet mode (just file paths, one per line)
        #[arg(short, long)]
        quiet: bool,
    },

    /// Preview file contents and infer schema
    Preview {
        /// File to preview
        file: std::path::PathBuf,

        /// Number of rows to show (default: 20)
        #[arg(short = 'n', long, default_value = "20")]
        rows: usize,

        /// Show inferred schema only
        #[arg(long)]
        schema: bool,

        /// Show raw contents (no parsing)
        #[arg(long)]
        raw: bool,

        /// First N bytes for binary/large files
        #[arg(long)]
        head: Option<usize>,

        /// Force CSV delimiter
        #[arg(long)]
        delimiter: Option<char>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}
```

### scan.rs Implementation

```rust
//! Standalone filesystem scanner (no database required)
//!
//! Unlike scout's Scanner which uses SQLite for state, this is a
//! pure filesystem walk that outputs directly to stdout.

use crate::output::{JsonOutput, TableOutput};
use anyhow::{Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Scan result for a single file
#[derive(Debug, Serialize)]
pub struct FileEntry {
    pub path: String,
    pub rel_path: String,
    pub size: u64,
    pub modified: Option<String>,
    pub file_type: String,
}

/// Aggregate scan result
#[derive(Debug, Serialize)]
pub struct ScanResult {
    pub root: String,
    pub files: Vec<FileEntry>,
    pub summary: ScanSummary,
}

#[derive(Debug, Serialize)]
pub struct ScanSummary {
    pub total_files: usize,
    pub total_bytes: u64,
    pub by_type: std::collections::HashMap<String, TypeSummary>,
}

#[derive(Debug, Serialize)]
pub struct TypeSummary {
    pub count: usize,
    pub bytes: u64,
}

/// Scan options from CLI args
pub struct ScanOptions {
    pub types: Vec<String>,
    pub recursive: bool,
    pub max_depth: Option<usize>,
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
}

/// Execute the scan command
pub fn execute(
    path: PathBuf,
    types: Vec<String>,
    recursive: bool,
    depth: Option<usize>,
    min_size: Option<String>,
    max_size: Option<String>,
    json: bool,
    stats: bool,
    quiet: bool,
) -> Result<()> {
    // Validate path exists
    if !path.exists() {
        return Err(HelpfulError::path_not_found(&path).into());
    }

    if !path.is_dir() {
        return Err(HelpfulError::not_a_directory(&path).into());
    }

    // Parse size filters
    let min_bytes = min_size.map(|s| parse_size(&s)).transpose()?;
    let max_bytes = max_size.map(|s| parse_size(&s)).transpose()?;

    // Build walker
    let mut walker = WalkDir::new(&path);

    if !recursive {
        walker = walker.max_depth(1);
    } else if let Some(d) = depth {
        walker = walker.max_depth(d);
    }

    // Collect files
    let mut files = Vec::new();
    let mut by_type: std::collections::HashMap<String, TypeSummary> =
        std::collections::HashMap::new();
    let mut total_bytes = 0u64;

    for entry in walker.into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }

        let meta = entry.metadata().ok();
        let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);

        // Apply size filters
        if let Some(min) = min_bytes {
            if size < min { continue; }
        }
        if let Some(max) = max_bytes {
            if size > max { continue; }
        }

        // Detect file type
        let file_type = detect_type(entry.path());

        // Apply type filter
        if !types.is_empty() && !types.contains(&file_type) {
            continue;
        }

        let rel_path = entry.path()
            .strip_prefix(&path)
            .unwrap_or(entry.path())
            .to_string_lossy()
            .to_string();

        let modified = meta.as_ref()
            .and_then(|m| m.modified().ok())
            .map(|t| format_time(t));

        // Update by_type stats
        by_type.entry(file_type.clone())
            .or_insert(TypeSummary { count: 0, bytes: 0 })
            .count += 1;
        by_type.get_mut(&file_type).unwrap().bytes += size;
        total_bytes += size;

        files.push(FileEntry {
            path: entry.path().to_string_lossy().to_string(),
            rel_path,
            size,
            modified,
            file_type,
        });
    }

    let result = ScanResult {
        root: path.to_string_lossy().to_string(),
        summary: ScanSummary {
            total_files: files.len(),
            total_bytes,
            by_type,
        },
        files,
    };

    // Output
    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else if stats {
        print_stats(&result);
    } else if quiet {
        for file in &result.files {
            println!("{}", file.path);
        }
    } else {
        print_table(&result);
    }

    Ok(())
}

/// Detect file type from extension
fn detect_type(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Parse human-readable size (1KB, 10MB, etc.)
fn parse_size(s: &str) -> Result<u64> {
    let s = s.trim().to_uppercase();
    let (num, multiplier) = if s.ends_with("GB") {
        (&s[..s.len()-2], 1024 * 1024 * 1024)
    } else if s.ends_with("MB") {
        (&s[..s.len()-2], 1024 * 1024)
    } else if s.ends_with("KB") {
        (&s[..s.len()-2], 1024)
    } else if s.ends_with("B") {
        (&s[..s.len()-1], 1)
    } else {
        (s.as_str(), 1)
    };

    let num: u64 = num.trim().parse()
        .context("Invalid size number")?;

    Ok(num * multiplier)
}

/// Print human-readable table
fn print_table(result: &ScanResult) {
    use comfy_table::{Table, presets::UTF8_FULL};

    println!();
    println!("Found {} files in {}", result.files.len(), result.root);
    println!();

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec!["PATH", "SIZE", "MODIFIED", "TYPE"]);

    for file in &result.files {
        table.add_row(vec![
            &file.rel_path,
            &format_size(file.size),
            file.modified.as_deref().unwrap_or("-"),
            &file.file_type,
        ]);
    }

    println!("{table}");
    println!();

    // Summary
    println!("Summary: {} files, {}",
        result.summary.total_files,
        format_size(result.summary.total_bytes));

    for (typ, stats) in &result.summary.by_type {
        println!("  {}: {} files ({})", typ, stats.count, format_size(stats.bytes));
    }
}

fn print_stats(result: &ScanResult) {
    println!("Files: {}", result.summary.total_files);
    println!("Total: {}", format_size(result.summary.total_bytes));
    println!();
    for (typ, stats) in &result.summary.by_type {
        println!("{}: {} ({:.1}%)",
            typ,
            stats.count,
            100.0 * stats.bytes as f64 / result.summary.total_bytes as f64);
    }
}

fn format_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

fn format_time(time: std::time::SystemTime) -> String {
    use std::time::UNIX_EPOCH;
    let secs = time.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    // Simple ISO-like format without chrono dependency
    let datetime = chrono::DateTime::<chrono::Utc>::from_timestamp(secs as i64, 0)
        .unwrap_or_default();
    datetime.format("%Y-%m-%d %H:%M").to_string()
}
```

### preview.rs Implementation

```rust
//! File preview with schema inference
//!
//! Supports: CSV, JSON, NDJSON, Parquet, and raw text

use anyhow::{Context, Result};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Serialize)]
pub struct PreviewResult {
    pub path: String,
    pub file_type: String,
    pub size: u64,
    pub total_rows: Option<usize>,
    pub schema: Schema,
    pub rows: Vec<Row>,
}

#[derive(Debug, Serialize)]
pub struct Schema {
    pub columns: Vec<Column>,
}

#[derive(Debug, Serialize)]
pub struct Column {
    pub name: String,
    pub dtype: String,
    pub nullable: bool,
    pub sample_values: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Row {
    pub values: Vec<serde_json::Value>,
}

pub fn execute(
    file: PathBuf,
    rows: usize,
    schema_only: bool,
    raw: bool,
    head: Option<usize>,
    delimiter: Option<char>,
    json_output: bool,
) -> Result<()> {
    // Validate file exists
    if !file.exists() {
        return Err(HelpfulError::file_not_found(&file).into());
    }

    if !file.is_file() {
        return Err(HelpfulError::not_a_file(&file).into());
    }

    // Raw mode - just dump content
    if raw {
        let content = std::fs::read_to_string(&file)
            .or_else(|_| {
                // Binary file - show hex
                let bytes = std::fs::read(&file)?;
                let limit = head.unwrap_or(1000);
                Ok(format!("(binary file, showing first {} bytes as hex)\n{}",
                    limit,
                    hex_dump(&bytes[..bytes.len().min(limit)])))
            })?;

        let limit = head.unwrap_or(10000);
        println!("{}", &content[..content.len().min(limit)]);
        return Ok(());
    }

    // Detect file type
    let file_type = detect_file_type(&file);

    // Preview based on type
    let result = match file_type.as_str() {
        "csv" | "tsv" => preview_csv(&file, rows, delimiter)?,
        "json" => preview_json(&file, rows)?,
        "ndjson" | "jsonl" => preview_ndjson(&file, rows)?,
        "parquet" => preview_parquet(&file, rows)?,
        "log" | "txt" => preview_text(&file, rows)?,
        _ => {
            return Err(HelpfulError::unknown_file_type(&file).into());
        }
    };

    // Output
    if json_output {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else if schema_only {
        print_schema(&result);
    } else {
        print_preview(&result);
    }

    Ok(())
}

fn detect_file_type(path: &PathBuf) -> String {
    let ext = path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());

    match ext.as_deref() {
        Some("csv") => "csv".to_string(),
        Some("tsv") => "tsv".to_string(),
        Some("json") => {
            // Check if NDJSON by reading first few lines
            if let Ok(content) = std::fs::read_to_string(path) {
                let lines: Vec<&str> = content.lines().take(3).collect();
                if lines.len() > 1 && lines.iter().all(|l| l.trim().starts_with('{')) {
                    return "ndjson".to_string();
                }
            }
            "json".to_string()
        }
        Some("jsonl" | "ndjson") => "ndjson".to_string(),
        Some("parquet") => "parquet".to_string(),
        Some("log") => "log".to_string(),
        Some("txt") => "txt".to_string(),
        _ => "unknown".to_string(),
    }
}

fn preview_csv(path: &PathBuf, max_rows: usize, delimiter: Option<char>) -> Result<PreviewResult> {
    let file = std::fs::File::open(path)?;
    let meta = file.metadata()?;

    let delim = delimiter.unwrap_or(',') as u8;
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delim)
        .flexible(true)
        .from_reader(file);

    // Get headers
    let headers: Vec<String> = reader.headers()?
        .iter()
        .map(|h| h.to_string())
        .collect();

    // Sample rows for type inference
    let mut sample_values: Vec<Vec<String>> = vec![vec![]; headers.len()];
    let mut rows = Vec::new();
    let mut total_rows = 0;

    for result in reader.records() {
        let record = result?;
        total_rows += 1;

        if rows.len() < max_rows {
            let values: Vec<serde_json::Value> = record.iter()
                .map(|v| serde_json::Value::String(v.to_string()))
                .collect();
            rows.push(Row { values });

            // Collect samples for type inference
            for (i, val) in record.iter().enumerate() {
                if i < sample_values.len() && sample_values[i].len() < 10 {
                    sample_values[i].push(val.to_string());
                }
            }
        }
    }

    // Infer column types
    let columns: Vec<Column> = headers.iter().enumerate()
        .map(|(i, name)| {
            let samples = sample_values.get(i).cloned().unwrap_or_default();
            let dtype = infer_type(&samples);
            let nullable = samples.iter().any(|v| v.is_empty());

            Column {
                name: name.clone(),
                dtype,
                nullable,
                sample_values: samples.into_iter().take(3).collect(),
            }
        })
        .collect();

    Ok(PreviewResult {
        path: path.to_string_lossy().to_string(),
        file_type: "csv".to_string(),
        size: meta.len(),
        total_rows: Some(total_rows),
        schema: Schema { columns },
        rows,
    })
}

fn infer_type(samples: &[String]) -> String {
    let non_empty: Vec<&str> = samples.iter()
        .filter(|s| !s.is_empty())
        .map(|s| s.as_str())
        .collect();

    if non_empty.is_empty() {
        return "string".to_string();
    }

    // Try integer
    if non_empty.iter().all(|s| s.parse::<i64>().is_ok()) {
        return "integer".to_string();
    }

    // Try float
    if non_empty.iter().all(|s| s.parse::<f64>().is_ok()) {
        return "float".to_string();
    }

    // Try boolean
    let bools = ["true", "false", "yes", "no", "1", "0"];
    if non_empty.iter().all(|s| bools.contains(&s.to_lowercase().as_str())) {
        return "boolean".to_string();
    }

    // Try date/datetime patterns
    if non_empty.iter().all(|s| looks_like_date(s)) {
        return "date".to_string();
    }

    "string".to_string()
}

fn looks_like_date(s: &str) -> bool {
    // Simple pattern matching for common date formats
    let patterns = [
        r"^\d{4}-\d{2}-\d{2}",           // 2024-01-15
        r"^\d{2}/\d{2}/\d{4}",           // 01/15/2024
        r"^\d{4}/\d{2}/\d{2}",           // 2024/01/15
        r"^\d{2}-\d{2}-\d{4}",           // 15-01-2024
    ];

    patterns.iter().any(|p| {
        regex::Regex::new(p).map(|r| r.is_match(s)).unwrap_or(false)
    })
}

fn preview_json(path: &PathBuf, max_rows: usize) -> Result<PreviewResult> {
    let content = std::fs::read_to_string(path)?;
    let meta = std::fs::metadata(path)?;
    let value: serde_json::Value = serde_json::from_str(&content)?;

    // If array, treat as rows
    if let serde_json::Value::Array(arr) = &value {
        let rows: Vec<Row> = arr.iter()
            .take(max_rows)
            .map(|v| Row { values: vec![v.clone()] })
            .collect();

        // Infer schema from first object
        let columns = if let Some(serde_json::Value::Object(obj)) = arr.first() {
            obj.iter()
                .map(|(k, v)| Column {
                    name: k.clone(),
                    dtype: json_type_name(v),
                    nullable: true,
                    sample_values: vec![v.to_string()],
                })
                .collect()
        } else {
            vec![]
        };

        return Ok(PreviewResult {
            path: path.to_string_lossy().to_string(),
            file_type: "json".to_string(),
            size: meta.len(),
            total_rows: Some(arr.len()),
            schema: Schema { columns },
            rows,
        });
    }

    // Single object
    Ok(PreviewResult {
        path: path.to_string_lossy().to_string(),
        file_type: "json".to_string(),
        size: meta.len(),
        total_rows: Some(1),
        schema: Schema { columns: vec![] },
        rows: vec![Row { values: vec![value] }],
    })
}

fn preview_ndjson(path: &PathBuf, max_rows: usize) -> Result<PreviewResult> {
    let file = std::fs::File::open(path)?;
    let meta = file.metadata()?;
    let reader = std::io::BufReader::new(file);

    use std::io::BufRead;

    let mut rows = Vec::new();
    let mut columns_inferred = false;
    let mut columns = Vec::new();

    for line in reader.lines().take(max_rows) {
        let line = line?;
        if line.trim().is_empty() { continue; }

        let value: serde_json::Value = serde_json::from_str(&line)?;

        // Infer columns from first object
        if !columns_inferred {
            if let serde_json::Value::Object(obj) = &value {
                columns = obj.iter()
                    .map(|(k, v)| Column {
                        name: k.clone(),
                        dtype: json_type_name(v),
                        nullable: true,
                        sample_values: vec![v.to_string()],
                    })
                    .collect();
            }
            columns_inferred = true;
        }

        rows.push(Row { values: vec![value] });
    }

    Ok(PreviewResult {
        path: path.to_string_lossy().to_string(),
        file_type: "ndjson".to_string(),
        size: meta.len(),
        total_rows: None, // Would need full scan
        schema: Schema { columns },
        rows,
    })
}

fn preview_parquet(path: &PathBuf, max_rows: usize) -> Result<PreviewResult> {
    use parquet::file::reader::{FileReader, SerializedFileReader};
    use parquet::record::Row as ParquetRow;

    let file = std::fs::File::open(path)?;
    let meta = file.metadata()?;
    let reader = SerializedFileReader::new(file)?;

    let parquet_meta = reader.metadata();
    let schema = parquet_meta.file_metadata().schema();
    let total_rows = parquet_meta.file_metadata().num_rows() as usize;

    // Extract columns from schema
    let columns: Vec<Column> = schema.get_fields().iter()
        .map(|f| Column {
            name: f.name().to_string(),
            dtype: format!("{:?}", f.get_basic_info().logical_type()),
            nullable: !f.is_optional(),
            sample_values: vec![],
        })
        .collect();

    // Read rows
    let mut rows = Vec::new();
    let row_iter = reader.get_row_iter(None)?;

    for row in row_iter.take(max_rows) {
        let parquet_row = row?;
        let values: Vec<serde_json::Value> = (0..parquet_row.len())
            .map(|i| {
                // Convert parquet value to JSON
                let val = parquet_row.get_string(i)
                    .unwrap_or_else(|_| "null".to_string());
                serde_json::Value::String(val)
            })
            .collect();
        rows.push(Row { values });
    }

    Ok(PreviewResult {
        path: path.to_string_lossy().to_string(),
        file_type: "parquet".to_string(),
        size: meta.len(),
        total_rows: Some(total_rows),
        schema: Schema { columns },
        rows,
    })
}

fn preview_text(path: &PathBuf, max_rows: usize) -> Result<PreviewResult> {
    let file = std::fs::File::open(path)?;
    let meta = file.metadata()?;
    let reader = std::io::BufReader::new(file);

    use std::io::BufRead;

    let rows: Vec<Row> = reader.lines()
        .take(max_rows)
        .filter_map(|l| l.ok())
        .map(|line| Row {
            values: vec![serde_json::Value::String(line)]
        })
        .collect();

    Ok(PreviewResult {
        path: path.to_string_lossy().to_string(),
        file_type: "text".to_string(),
        size: meta.len(),
        total_rows: None,
        schema: Schema {
            columns: vec![Column {
                name: "line".to_string(),
                dtype: "string".to_string(),
                nullable: false,
                sample_values: vec![],
            }]
        },
        rows,
    })
}

fn json_type_name(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(_) => "boolean".to_string(),
        serde_json::Value::Number(n) => {
            if n.is_i64() { "integer".to_string() }
            else { "float".to_string() }
        }
        serde_json::Value::String(_) => "string".to_string(),
        serde_json::Value::Array(_) => "array".to_string(),
        serde_json::Value::Object(_) => "object".to_string(),
    }
}

fn hex_dump(bytes: &[u8]) -> String {
    bytes.chunks(16)
        .map(|chunk| {
            let hex: Vec<String> = chunk.iter()
                .map(|b| format!("{:02x}", b))
                .collect();
            let ascii: String = chunk.iter()
                .map(|&b| if b.is_ascii_graphic() || b == b' ' { b as char } else { '.' })
                .collect();
            format!("{:<48} {}", hex.join(" "), ascii)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn print_schema(result: &PreviewResult) {
    println!();
    println!("File: {}", result.path);
    println!("Type: {} | Rows: {} | Size: {}",
        result.file_type,
        result.total_rows.map_or("?".to_string(), |n| n.to_string()),
        format_size(result.size));
    println!();
    println!("SCHEMA:");
    for col in &result.schema.columns {
        println!("  {} : {} {}",
            col.name,
            col.dtype,
            if col.nullable { "(nullable)" } else { "" });
    }
}

fn print_preview(result: &PreviewResult) {
    print_schema(result);

    if result.rows.is_empty() {
        println!("\n(no data rows)");
        return;
    }

    println!();

    use comfy_table::{Table, presets::UTF8_FULL};
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);

    // Header from schema
    let headers: Vec<&str> = result.schema.columns.iter()
        .map(|c| c.name.as_str())
        .collect();

    if !headers.is_empty() {
        table.set_header(headers);
    }

    // Rows
    for row in &result.rows {
        let cells: Vec<String> = row.values.iter()
            .map(|v| match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .collect();
        table.add_row(cells);
    }

    println!("{table}");

    if let Some(total) = result.total_rows {
        if total > result.rows.len() {
            println!("\n... and {} more rows", total - result.rows.len());
        }
    }
}

fn format_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}
```

### error.rs - Helpful Errors

```rust
//! Helpful error messages with suggestions
//!
//! Every error includes:
//! 1. What went wrong
//! 2. Context (if relevant)
//! 3. Suggestions to fix

use std::path::Path;

#[derive(Debug)]
pub struct HelpfulError {
    pub message: String,
    pub context: Option<String>,
    pub suggestions: Vec<String>,
}

impl HelpfulError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            context: None,
            suggestions: vec![],
        }
    }

    pub fn with_context(mut self, ctx: impl Into<String>) -> Self {
        self.context = Some(ctx.into());
        self
    }

    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestions.push(suggestion.into());
        self
    }

    pub fn path_not_found(path: &Path) -> Self {
        let parent = path.parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string());

        Self::new(format!("Directory not found: {}", path.display()))
            .with_suggestion(format!("casparian scan {}     # Scan parent directory", parent))
            .with_suggestion(format!("ls -la {}             # Check what exists", parent))
    }

    pub fn not_a_directory(path: &Path) -> Self {
        let parent = path.parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string());

        Self::new(format!("Not a directory: {}", path.display()))
            .with_suggestion(format!("casparian preview {}  # Preview this file instead", path.display()))
            .with_suggestion(format!("casparian scan {}     # Scan the parent directory", parent))
    }

    pub fn file_not_found(path: &Path) -> Self {
        let parent = path.parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string());

        Self::new(format!("File not found: {}", path.display()))
            .with_suggestion(format!("casparian scan {}     # Find files in this directory", parent))
    }

    pub fn not_a_file(path: &Path) -> Self {
        Self::new(format!("Not a file: {}", path.display()))
            .with_suggestion(format!("casparian scan {}     # Scan this directory", path.display()))
    }

    pub fn unknown_file_type(path: &Path) -> Self {
        Self::new(format!("Unknown file type: {}", path.display()))
            .with_suggestion(format!("casparian preview {} --raw    # View raw contents", path.display()))
            .with_suggestion("Supported types: csv, json, ndjson, parquet, log, txt".to_string())
    }
}

impl std::fmt::Display for HelpfulError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "ERROR: {}", self.message)?;

        if let Some(ctx) = &self.context {
            writeln!(f)?;
            writeln!(f, "{}", ctx)?;
        }

        if !self.suggestions.is_empty() {
            writeln!(f)?;
            writeln!(f, "TRY:")?;
            for suggestion in &self.suggestions {
                writeln!(f, "    {}", suggestion)?;
            }
        }

        Ok(())
    }
}

impl std::error::Error for HelpfulError {}
```

### Dependencies to Add

Add to `crates/casparian/Cargo.toml`:

```toml
# Table formatting
comfy-table = "7"

# Regex for date detection
regex = "1"

# Chrono for time formatting (already in workspace)
chrono = { version = "0.4", features = ["serde"] }
```

### Integration in main.rs

```rust
// Add at top of main.rs
mod commands;
mod output;
mod error;

use commands::{scan, preview};

// In the match statement in main():
Commands::Scan { path, types, recursive, depth, min_size, max_size, json, stats, quiet } => {
    scan::execute(path, types, recursive, depth, min_size, max_size, json, stats, quiet)
}
Commands::Preview { file, rows, schema, raw, head, delimiter, json } => {
    preview::execute(file, rows, schema, raw, head, delimiter, json)
}
```

### Example Session

```bash
$ casparian scan ~/data

Found 23 files in /Users/me/data

┌──────────────────────┬─────────┬──────────────────┬──────┐
│ PATH                 │ SIZE    │ MODIFIED         │ TYPE │
├──────────────────────┼─────────┼──────────────────┼──────┤
│ sales_2024.csv       │ 2.3 MB  │ 2024-12-15 10:30 │ csv  │
│ transactions.json    │ 890 KB  │ 2024-12-14 14:22 │ json │
│ access.log           │ 15 MB   │ 2024-12-16 08:45 │ log  │
│ users.parquet        │ 456 KB  │ 2024-12-10 16:00 │ parquet │
└──────────────────────┴─────────┴──────────────────┴──────┘

Summary: 23 files, 18.6 MB
  csv: 12 files (8.5 MB)
  json: 5 files (2.1 MB)
  log: 4 files (7.2 MB)
  parquet: 2 files (0.8 MB)

$ casparian preview ~/data/sales_2024.csv

File: /Users/me/data/sales_2024.csv
Type: csv | Rows: 45,231 | Size: 2.3 MB

SCHEMA:
  date : date
  product_id : string
  quantity : integer
  amount : float (nullable)
  customer_id : string

┌────────────┬────────────┬──────────┬────────┬─────────────┐
│ date       │ product_id │ quantity │ amount │ customer_id │
├────────────┼────────────┼──────────┼────────┼─────────────┤
│ 2024-01-01 │ SKU-001    │ 5        │ 149.95 │ CUST-42     │
│ 2024-01-01 │ SKU-002    │ 2        │ 59.98  │ CUST-108    │
│ 2024-01-02 │ SKU-001    │ 3        │ 89.97  │ CUST-42     │
│ ...        │ ...        │ ...      │ ...    │ ...         │
└────────────┴────────────┴──────────┴────────┴─────────────┘

... and 45,211 more rows

$ casparian scan /nonexistent

ERROR: Directory not found: /nonexistent

TRY:
    casparian scan /     # Scan parent directory
    ls -la /             # Check what exists
```

---

## Changelog

| Date | Change |
|------|--------|
| 2025-01 | Initial CLI implementation plan |
| 2025-01 | Added Phase 1 detailed implementation spec |
