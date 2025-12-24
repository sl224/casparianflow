# Debug Summary: PyArrow "cannot import name Queue" Issue

## Status: RESOLVED

**Root Cause**: `src/casparian_flow/engine/queue.py` was shadowing Python's standard library `queue` module. When pyarrow tried to `from queue import Queue`, it found our local `queue.py` instead.

**Fix**: Renamed `queue.py` to `job_queue.py` and updated all imports.

---

## Problem Statement

When running the Casparian Flow E2E test, the Python guest process fails with:
```
ImportError: cannot import name Queue
```

The error occurs in pyarrow during initialization:
```
File "pyarrow/io.pxi", line 33, in init pyarrow.lib
ImportError: cannot import name Queue
```

## Key Finding: File Location Matters

**The exact same code works when saved to /tmp but fails when run from the project directory.**

### Proof:

1. **Works** (saved to /tmp):
```bash
/Users/shan/.casparian_flow/venvs/test_env_hash_123/bin/python /tmp/test_bridge_debug.py
# Result: SUCCESS - pyarrow imports fine, publishes 3 rows
```

2. **Fails** (original location):
```bash
/Users/shan/.casparian_flow/venvs/test_env_hash_123/bin/python /Users/shan/workspace/casparianflow/src/casparian_flow/engine/bridge_shim.py
# Result: FAILURE - "cannot import name Queue"
```

Both files are functionally identical - the debug version is a faithful recreation of bridge_shim.py.

## Environment

- **Python**: 3.13.9 (uv-managed at `~/.local/share/uv/python/cpython-3.13.9-macos-aarch64-none`)
- **PyArrow**: 22.0.0
- **Platform**: macOS Darwin 25.1.0, Apple Silicon (aarch64)
- **Virtual Environment**:
  - Location: `/Users/shan/workspace/casparianflow/.venv`
  - Symlinked to: `/Users/shan/.casparian_flow/venvs/test_env_hash_123`

## What Works vs What Fails

### Works (all of these succeed):

1. Direct import: `python -c "import pyarrow"`
2. Import in subprocess: `subprocess.run([python, "-c", "import pyarrow"])`
3. Socket connect then import
4. exec() plugin code then import
5. Full bridge flow with socket + exec + import (when script is in /tmp)
6. Running with `env -i PATH=... HOME=... python script.py`

### Fails (only this specific case):

Running `bridge_shim.py` from its project location as a subprocess:
```bash
/path/to/python /Users/shan/workspace/casparianflow/src/casparian_flow/engine/bridge_shim.py
```

When run from:
- Shell script subprocess: FAILS
- Python subprocess.run(): FAILS
- Rust std::process::Command: FAILS

## Relevant Files

### `/Users/shan/workspace/casparianflow/src/casparian_flow/engine/bridge_shim.py`
- The guest process shim for bridge mode
- Receives plugin code via environment variable
- Connects to Unix socket
- Imports pyarrow INSIDE the `publish()` method (not at module level)
- Line 132 is where pyarrow import fails

### `/Users/shan/workspace/casparianflow/crates/casparian_worker/src/bridge.rs`
- Rust worker that spawns the Python guest process
- Uses `std::process::Command`
- Currently configured to use `uv run --frozen --python <interpreter> <shim>`
- No `env_clear()` - inherits full environment

### Venv Structure
```
/Users/shan/.casparian_flow/venvs/test_env_hash_123 -> /Users/shan/workspace/casparianflow/.venv
└── bin/
    └── python -> ~/.local/share/uv/python/cpython-3.13.9-macos-aarch64-none/bin/python3.13
└── lib/python3.13/site-packages/
    └── pyarrow/ (version 22.0.0)
```

## Root Cause (Confirmed)

**Module shadowing**: `src/casparian_flow/engine/queue.py` shadowed Python's standard library `queue` module.

When bridge_shim.py ran from the project directory:
1. Python added `src/casparian_flow/engine` to `sys.path`
2. pyarrow tried `from queue import Queue` (standard library)
3. Python found our local `queue.py` first
4. Our `queue.py` has `JobQueue` class, not `Queue`
5. Import failed

This explains why:
- Running from `/tmp` worked: no local `queue.py` in search path
- Running from project directory failed: local `queue.py` took precedence

## Resolution

Renamed `src/casparian_flow/engine/queue.py` to `job_queue.py` and updated imports in:
- `sentinel.py`
- `api.py`
- `app.py`

## Original Suspected Causes (for reference)

```bash
cd /Users/shan/workspace/casparianflow

# This should work
/Users/shan/.casparian_flow/venvs/test_env_hash_123/bin/python -c "import pyarrow; print('OK')"

# This fails when run as subprocess with socket context
python3 << 'DRIVER'
import socket, os, sys, struct, subprocess, base64, time, threading

sock_path = "/tmp/repro.sock"
try: os.unlink(sock_path)
except: pass

def listener():
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.bind(sock_path)
    s.listen(1)
    s.settimeout(10)
    conn, _ = s.accept()
    header = conn.recv(4)
    length = struct.unpack("!I", header)[0]
    if length == 0xFFFFFFFF:
        err_len = struct.unpack("!I", conn.recv(4))[0]
        err_msg = conn.recv(err_len).decode()
        print(f"ERROR: {err_msg}")
    conn.close()
    s.close()

thread = threading.Thread(target=listener)
thread.start()
time.sleep(0.1)

plugin = 'class Handler:\n    def configure(self, ctx, cfg): self.ctx = ctx\n    def execute(self, path):\n        import pandas as pd\n        yield pd.DataFrame({"x": [1,2,3]})'

env = os.environ.copy()
env["BRIDGE_SOCKET"] = sock_path
env["BRIDGE_PLUGIN_CODE"] = base64.b64encode(plugin.encode()).decode()
env["BRIDGE_FILE_PATH"] = "/tmp/test.csv"
env["BRIDGE_JOB_ID"] = "999"
env["BRIDGE_FILE_VERSION_ID"] = "1"

proc = subprocess.run(
    ["/Users/shan/.casparian_flow/venvs/test_env_hash_123/bin/python",
     "src/casparian_flow/engine/bridge_shim.py"],
    env=env, capture_output=True, text=True, timeout=10
)
print("stderr:", proc.stderr)
thread.join(timeout=3)
DRIVER
```

## Lessons Learned

1. **Never name Python files after standard library modules** (queue, logging, socket, etc.)
2. **Module shadowing can cause confusing errors** - the error message "cannot import name Queue" didn't directly point to our `queue.py`
3. **File location matters for debugging** - running from /tmp vs project directory revealed the path difference
