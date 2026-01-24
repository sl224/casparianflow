# Test Fixture Plugins

This directory contains minimal plugins designed for testing various scenarios in a controlled, deterministic way.

## fixture_plugin.py

A minimal Python plugin that generates deterministic output with configurable behavior.

### Output Schema

| Column | Type | Description |
|--------|------|-------------|
| `id` | int64 | Sequential row ID (0 to rows-1) |
| `value` | string | String value "value_{id}" |

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `CF_FIXTURE_MODE` | `normal` | Operating mode (see below) |
| `CF_FIXTURE_ROWS` | `10` | Number of rows to generate |
| `CF_FIXTURE_SLEEP_SECS` | `10` | Sleep duration in `slow` mode |
| `CF_FIXTURE_ERROR_MSG` | `Fixture error` | Error message in `error` mode |

### Modes

#### `normal` (default)
Generates deterministic output immediately. Use for basic execution tests.

```bash
CF_FIXTURE_MODE=normal casparian run fixture_plugin.py input.txt
```

#### `slow`
Sleeps for `CF_FIXTURE_SLEEP_SECS` before generating output. Use for timeout and cancellation tests.

```bash
CF_FIXTURE_MODE=slow CF_FIXTURE_SLEEP_SECS=5 casparian run fixture_plugin.py input.txt
```

#### `collision`
Adds a reserved `_cf_job_id` column to the output. Use to test lineage collision detection (the worker should reject output that overwrites reserved columns).

```bash
CF_FIXTURE_MODE=collision casparian run fixture_plugin.py input.txt
```

#### `error`
Raises a `RuntimeError` with the message from `CF_FIXTURE_ERROR_MSG`. Use to test error handling paths.

```bash
CF_FIXTURE_MODE=error CF_FIXTURE_ERROR_MSG="Test error" casparian run fixture_plugin.py input.txt
```

### Usage in Tests

```rust
use casparian_worker::bridge::{execute_bridge, materialize_bridge_shim, BridgeConfig};

// Set environment for fixture mode
std::env::set_var("CF_FIXTURE_MODE", "normal");
std::env::set_var("CF_FIXTURE_ROWS", "10");

let config = BridgeConfig {
    source_code: include_str!("../tests/fixtures/plugins/fixture_plugin.py").to_string(),
    // ... other config
};

let result = execute_bridge(config)?;
```

### Key Invariants

1. **Deterministic output** - Same mode and rows always produces same output
2. **No external dependencies** - Only uses pyarrow (provided by bridge)
3. **Controllable behavior** - All behavior controlled via environment variables
4. **Fast by default** - Normal mode completes immediately for quick tests
