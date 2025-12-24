# Debugging: PyArrow Import Failure in Rust Bridge Subprocess

## Problem Summary

When the Rust worker binary spawns a Python subprocess to run `bridge_shim.py`, pyarrow fails to import with:
```
ImportError: cannot import name Queue
```

The exact same Python code works when spawned from a Python subprocess test.

## Environment

- **macOS** (Darwin 25.1.0, aarch64)
- **Python 3.13.9** (uv-managed)
- **PyArrow 22.0.0**
- **Rust** (using `std::process::Command`)

## The Symptom

```
File "pyarrow/io.pxi", line 33, in init pyarrow.lib
ImportError: cannot import name Queue
```

This is a Python 3.13 compatibility issue with pyarrow's Cython extension trying to import `Queue` from `multiprocessing`.

## What Works

1. **Direct Python invocation** from shell:
   ```bash
   VIRTUAL_ENV=/path/to/.venv python -c "import pyarrow"  # Works
   ```

2. **Python subprocess** spawning Python:
   ```python
   subprocess.run([python_path, "-c", "import pyarrow"], env=minimal_env)  # Works
   ```

3. **Bridge shim** via Python subprocess:
   ```python
   subprocess.run([python_path, "bridge_shim.py"], env=minimal_env)  # Works
   ```

## What Fails

**Rust `Command::spawn()`** running the exact same command:
```rust
Command::new(python_path)
    .arg(shim_path)
    .env_clear()  // Clear all inherited env
    .env("PATH", ...)
    .env("HOME", ...)
    .env("VIRTUAL_ENV", real_venv_path)
    .spawn()
```

## Key Files

### `/Users/shan/workspace/casparianflow/crates/casparian_worker/src/bridge.rs`
The Rust code that spawns Python. Current implementation uses `env_clear()` to prevent environment pollution, but still fails.

Key function: `spawn_guest()` (lines 85-130)

### `/Users/shan/workspace/casparianflow/src/casparian_flow/engine/bridge_shim.py`
The Python shim that gets executed. It:
1. Decodes plugin source code from base64
2. Connects to Unix socket
3. `exec()` the plugin code in a namespace
4. Plugin imports pyarrow - **this is where it fails**

### `/Users/shan/workspace/casparianflow/tests/e2e/debug_bridge_env.py`
Diagnostic script that tests all Python subprocess combinations - **all pass**.

## Venv Configuration

The worker uses a symlinked venv:
```
~/.casparian_flow/venvs/test_env_hash_123 -> /path/to/project/.venv
```

The `.venv` is managed by `uv` and contains pyarrow.

## What I've Tried

1. **Setting VIRTUAL_ENV** - Added to env, resolved to real .venv path ❌
2. **env_clear()** - Cleared all inherited vars, added only essentials ❌
3. **Restructuring Command chain** - Ensured env_clear() called first ❌
4. **Checking problematic vars** - No PYTHONPATH, PYTHONHOME, etc. found

## Observations

1. When I accidentally removed `.arg(shim_path)`, Python started but hung waiting for input (no script to run). Importantly, **it got past pyarrow** in bridge_shim.py's imports because the job status was RUNNING, not FAILED.

2. Adding `.arg(shim_path)` back causes the failure to return immediately.

3. The Rust `Command` inherits environment by default. Even with `env_clear()`, something is different about how Rust spawns vs Python spawns.

## Suspected Issues

1. **spawn_blocking context** - The Rust code runs inside `tokio::task::spawn_blocking()`. Could this affect subprocess environment?

2. **Something about `.arg()` interaction** - Why does removing the script arg make it work?

3. **Rust Command vs Python subprocess** - There must be something fundamentally different in how they set up the child process.

4. **macOS-specific** - Could be related to `posix_spawn` or `fork+exec` differences.

## Reproduction Steps

```bash
cd /Users/shan/workspace/casparianflow/tests/e2e

# This passes (Python spawning Python):
python3 debug_bridge_env.py

# This fails (Rust spawning Python):
./run_e2e_test.sh
```

## Key Code Paths

### Rust spawn (bridge.rs):
```rust
let mut cmd = Command::new(&config.interpreter_path);
cmd.env_clear();

cmd.arg(&config.shim_path)
    .env("PATH", std::env::var("PATH").unwrap_or_default())
    .env("HOME", std::env::var("HOME").unwrap_or_default())
    .env("VIRTUAL_ENV", venv_root)
    .env("BRIDGE_SOCKET", socket_path)
    // ... other BRIDGE_* vars
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());

let child = cmd.spawn()?;
```

### Python spawn (debug_bridge_env.py):
```python
env = {
    'PATH': os.environ.get('PATH', ''),
    'HOME': os.environ.get('HOME', ''),
    'VIRTUAL_ENV': str(REAL_VENV),
    'BRIDGE_SOCKET': '/tmp/nonexistent.sock',
    # ... other BRIDGE_* vars
}

subprocess.run([str(python_path), str(BRIDGE_SHIM)], env=env)  # Works!
```

## Potential Solution: Use `uv run`

Testing shows `uv run` works correctly even with minimal environment:
```bash
env -i PATH="$PATH" HOME="$HOME" uv run python -c "import pyarrow"  # Works!
```

Instead of calling the venv Python directly, the worker could use:
```rust
Command::new("uv")
    .args(["run", "--frozen", "python", "bridge_shim.py"])
    // env vars...
```

This would delegate environment setup to `uv`, which handles it correctly.

## Questions for Diagnosis

1. What does Rust's `Command::spawn()` do differently from Python's `subprocess.run()` on macOS?

2. Could `tokio::spawn_blocking()` affect the subprocess environment?

3. Is there something about argument passing (`.arg()`) that affects environment inheritance even after `env_clear()`?

4. Is this a pyarrow/Cython issue with how it resolves imports in certain process contexts?

5. Is there a subtle difference in how `posix_spawn` (used by Rust) vs `fork+exec` (used by Python) handles environment on macOS?

## Files to Review

- `crates/casparian_worker/src/bridge.rs` - spawn_guest function
- `src/casparian_flow/engine/bridge_shim.py` - Python shim
- `tests/e2e/debug_bridge_env.py` - diagnostic that passes
- `tests/e2e/run_e2e_test.sh` - E2E test that fails
