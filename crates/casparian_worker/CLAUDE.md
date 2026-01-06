# Claude Code Instructions for casparian_worker

## Quick Reference

```bash
cargo test -p casparian_worker              # All tests
cargo test -p casparian_worker --test e2e_type_inference  # E2E tests
```

---

## Overview

`casparian_worker` is the **Worker crate** containing:
1. **Type Inference Engine** - Constraint-based type detection
2. **Bridge Mode Execution** - Host/Guest privilege separation
3. **Virtual Environment Management** - UV-based venv lifecycle

---

## Type Inference Engine

### The Philosophy

**Elimination, not voting.**

Traditional type inference: "70% of values look like dates, so it's a date."

Our approach: "This value CANNOT be a month (31 > 12), so DD/MM/YY is PROVEN."

### Key Insight

One constraining value can resolve ambiguity with certainty:

```
Values: ["15/06/24", "31/05/24", "01/12/24"]

- "15/06/24": Could be DD/MM/YY or MM/DD/YY (15 ≤ 12, 06 ≤ 12)
- "31/05/24": PROVES DD/MM/YY because 31 > 12 (cannot be month)
- After seeing "31/05/24", format is resolved with CERTAINTY
```

---

## Type Inference API

### ConstraintSolver

```rust
use casparian_worker::type_inference::{ConstraintSolver, DataType};

let mut solver = ConstraintSolver::new("amount");

// Add values - each eliminates impossible types
solver.add_value("100");    // Could be Integer, Float, String
solver.add_value("200");    // Still could be Integer, Float, String
solver.add_value("150.50"); // Eliminates Integer (has decimal)

// Get remaining possibilities
let types = solver.possible_types();
assert!(!types.contains(&DataType::Integer));
assert!(types.contains(&DataType::Float));

// Get the inferred type
let result = solver.infer();
assert_eq!(result.data_type, DataType::Float);
```

### DataType

```rust
pub enum DataType {
    Null,      // Empty/null value
    Boolean,   // true/false, yes/no, 1/0
    Integer,   // 64-bit signed
    Float,     // 64-bit floating point
    Date,      // Date only (no time)
    DateTime,  // Date + time
    Time,      // Time only
    Duration,  // Interval/duration
    String,    // UTF-8 fallback
}
```

### Elimination Evidence

Track WHY a type was eliminated:

```rust
let evidence = solver.elimination_evidence();
for e in evidence {
    println!("Eliminated {:?} because: {} (value: '{}')",
        e.eliminated_type,
        e.reason,
        e.constraining_value
    );
}
// Output: Eliminated Integer because: Contains decimal point (value: '150.50')
```

### TypeInferenceResult

```rust
pub struct TypeInferenceResult {
    pub column_name: String,
    pub data_type: DataType,
    pub nullable: bool,         // Had null/empty values?
    pub format: Option<String>, // For dates/times
    pub confidence: f64,        // 1.0 = certain, <1.0 = ambiguous
    pub sample_values: Vec<String>,
    pub elimination_evidence: Vec<EliminationEvidence>,
}
```

---

## Streaming Type Inference

For large files, infer types row-by-row:

```rust
use casparian_worker::type_inference::{infer_types_streaming, StreamingConfig};

let config = StreamingConfig {
    early_termination: true,  // Stop when all types resolved
    max_rows: 10000,          // Limit for very large files
    batch_size: 100,          // Rows per batch
};

let columns = &["id", "amount", "date"];
let rows = csv_reader.records();  // Iterator of rows

let results = infer_types_streaming(
    columns,
    rows.map(|r| r.iter().map(|s| s.as_str()).collect::<Vec<_>>()),
    config,
)?;

for (col, result) in results {
    println!("{}: {:?}", col, result.data_type);
}
```

---

## Date Format Detection

### Supported Formats

```rust
pub const DATE_FORMATS: &[(&str, &str)] = &[
    // ISO
    ("%Y-%m-%d", "YYYY-MM-DD"),
    ("%Y/%m/%d", "YYYY/MM/DD"),

    // US
    ("%m/%d/%Y", "MM/DD/YYYY"),
    ("%m-%d-%Y", "MM-DD-YYYY"),

    // EU
    ("%d/%m/%Y", "DD/MM/YYYY"),
    ("%d-%m-%Y", "DD-MM-YYYY"),

    // Short year
    ("%d/%m/%y", "DD/MM/YY"),
    ("%m/%d/%y", "MM/DD/YY"),
    // ... more
];
```

### Format Disambiguation

```rust
// Ambiguous: "01/02/2024" could be Jan 2 or Feb 1
let mut solver = ConstraintSolver::new("date");
solver.add_value("01/02/2024");  // Ambiguous

// Disambiguating: "31/01/2024" proves DD/MM/YYYY
solver.add_value("31/01/2024");  // Day=31 > 12, so DD/MM/YYYY

let result = solver.infer();
assert_eq!(result.format, Some("%d/%m/%Y".to_string()));
```

---

## Bridge Mode Execution

### Architecture

```
Worker (Host)  <──AF_UNIX──>  Guest Process (isolated venv)
     │                              │
     │ - Credentials              │ - Plugin code only
     │ - Heavy drivers            │ - Minimal deps (pandas, pyarrow)
     │ - Sink writers             │ - No secrets
     ▼                              ▼
  Write to DB/Parquet         Stream Arrow IPC batches
```

### Why Bridge Mode?

1. **Security**: Guest can't access credentials
2. **Isolation**: Plugin crashes don't affect host
3. **Dependency management**: Each plugin has its own venv
4. **Reproducibility**: Lockfiles ensure same deps

### BridgeShim (Guest)

The guest runs in an isolated subprocess:

```python
# bridge_shim.py (simplified)
import sys
import socket
import pyarrow as pa

def main():
    # Get plugin code from environment
    plugin_code = os.environ["CASPARIAN_PLUGIN_CODE"]
    file_path = os.environ["CASPARIAN_FILE_PATH"]
    socket_path = os.environ["CASPARIAN_SOCKET"]

    # Execute plugin
    exec(plugin_code)
    result = Handler().execute(file_path)

    # Stream results via Arrow IPC
    with socket.socket(socket.AF_UNIX) as sock:
        sock.connect(socket_path)
        writer = pa.ipc.new_stream(sock, result.schema)
        for batch in result.to_batches():
            writer.write_batch(batch)
```

---

## Virtual Environment Management

### UV for Speed

All venvs are managed by [uv](https://github.com/astral-sh/uv):

```rust
use casparian_worker::venv_manager::VenvManager;

let manager = VenvManager::new("~/.casparian_flow/venvs")?;

// Ensure venv exists for a lockfile
let venv_path = manager.ensure(&env_hash, &lockfile_content)?;

// Execute in venv
manager.run_in_venv(&venv_path, "python", &["-c", "print('hello')"])?;
```

### Content-Addressable Storage

Venvs are stored by hash of their lockfile:

```
~/.casparian_flow/venvs/
├── a1b2c3d4e5f6.../  # venv for lockfile hash a1b2c3d4e5f6
│   ├── bin/
│   ├── lib/
│   └── pyvenv.cfg
├── f6e5d4c3b2a1.../  # different lockfile
└── ...
```

### LRU Eviction

Old venvs are cleaned up:

```rust
manager.cleanup(
    max_venvs: 20,        // Keep at most 20 venvs
    max_age_days: 30,     // Delete if unused for 30 days
)?;
```

---

## Worker Configuration

```rust
use casparian_worker::{Worker, WorkerConfig};

let config = WorkerConfig {
    sentinel_address: "tcp://127.0.0.1:5555".to_string(),
    output_dir: PathBuf::from("./output"),
    worker_id: None,  // Auto-generated if not provided
    venv_root: PathBuf::from("~/.casparian_flow/venvs"),
};

let worker = Worker::new(config)?;
worker.run()?;  // Blocks, processing jobs
```

---

## Common Tasks

### Add a New Data Type

1. Add variant to `DataType`:
```rust
pub enum DataType {
    // ... existing
    Currency,  // Money with currency symbol
}
```

2. Implement detection:
```rust
impl DataType {
    pub fn can_be_currency(value: &str) -> bool {
        // Check for $, €, £, etc.
        let trimmed = value.trim();
        let has_symbol = ["$", "€", "£", "¥"].iter()
            .any(|s| trimmed.starts_with(s) || trimmed.ends_with(s));

        if has_symbol {
            // Remove symbol, check if remainder is numeric
            let numeric_part = trimmed.trim_matches(|c| "$€£¥ ".contains(c));
            numeric_part.parse::<f64>().is_ok()
        } else {
            false
        }
    }
}
```

3. Add to elimination logic in `ConstraintSolver`

### Debug Type Inference

```rust
let mut solver = ConstraintSolver::new("problematic_column");

for value in values {
    solver.add_value(value);

    // Debug: print remaining possibilities after each value
    let remaining = solver.possible_types();
    tracing::debug!("After '{}': {:?}", value, remaining);
}

let result = solver.infer();
tracing::info!("Inferred: {:?} (confidence: {})", result.data_type, result.confidence);

// Print elimination evidence
for e in result.elimination_evidence {
    tracing::info!("Eliminated {:?}: {} (value: '{}')",
        e.eliminated_type, e.reason, e.constraining_value);
}
```

### Test Type Inference

```rust
#[test]
fn test_decimal_eliminates_integer() {
    let mut solver = ConstraintSolver::new("price");

    solver.add_value("100");
    solver.add_value("200");
    assert!(solver.possible_types().contains(&DataType::Integer));

    solver.add_value("150.50");
    assert!(!solver.possible_types().contains(&DataType::Integer));
    assert!(solver.possible_types().contains(&DataType::Float));
}

#[test]
fn test_date_format_disambiguation() {
    let mut solver = ConstraintSolver::new("date");

    solver.add_value("01/02/2024");  // Ambiguous
    assert!(solver.possible_date_formats().len() > 1);

    solver.add_value("31/05/2024");  // Proves DD/MM/YYYY
    assert_eq!(solver.possible_date_formats(), vec!["%d/%m/%Y"]);
}
```

---

## File Structure

```
casparian_worker/
├── CLAUDE.md            # This file
├── Cargo.toml
├── shim/
│   ├── bridge_shim.py   # Guest process Python code
│   └── casparian_types.py  # Shared types
├── src/
│   ├── lib.rs           # Crate root
│   ├── worker.rs        # Worker implementation
│   ├── bridge.rs        # Host/Guest communication
│   ├── venv_manager.rs  # UV-based venv management
│   ├── analyzer.rs      # File analysis
│   ├── shredder.rs      # Legacy shredder
│   ├── metrics.rs       # Worker metrics
│   └── type_inference/
│       ├── mod.rs       # Module root
│       ├── constraints.rs  # Constraint types
│       ├── solver.rs    # ConstraintSolver
│       ├── date_formats.rs  # Date detection
│       └── streaming.rs # Streaming inference
└── tests/
    └── e2e_type_inference.rs  # E2E tests (25 tests)
```

---

## Key Principles

1. **Elimination over voting** - Certainty when possible
2. **Evidence tracking** - Know WHY a type was inferred
3. **Early termination** - Stop when types are resolved
4. **Bridge isolation** - Guest has no secrets
5. **Reproducible environments** - Lockfiles are law
