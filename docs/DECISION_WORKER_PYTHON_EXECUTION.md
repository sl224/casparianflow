# Decision: How Should the Rust Worker Execute Python Plugins?

## Context

Casparian Flow is a data processing system with:
- **Sentinel** (Rust): Control plane that dispatches jobs from a queue
- **Worker** (Rust): Executes Python plugins via a "bridge" (Unix socket IPC + Arrow)
- **Plugins**: Python code that processes files and yields Arrow batches

The worker spawns a Python subprocess to run `bridge_shim.py`, which loads and executes plugin code. Plugins have dependencies (minimally `pyarrow`, potentially others).

## Current Implementation

```
1. Sentinel sends DISPATCH with: plugin_name, source_code, env_hash, artifact_hash, file_path
2. Worker constructs path: ~/.casparian_flow/venvs/{env_hash}/bin/python
3. Worker spawns: {interpreter} bridge_shim.py (with env vars for socket, code, etc.)
4. Python plugin runs, sends Arrow batches over Unix socket
5. Worker collects results, sends CONCLUDE to Sentinel
```

The `env_hash` is stored in the database alongside the plugin manifest and is
the lockfile hash. The `artifact_hash` ties source + lockfile for auditability.
The assumption is that some external process has pre-created virtual
environments at the expected paths.

## Decision (v1, ADR-018)

- Environments are provisioned out-of-band; Sentinel and Worker do not create envs.
- Workers fail fast with a permanent error if `env_hash` is missing locally.
- The worker pool is homogeneous; every worker must have the same plugin envs installed.

## The Problem

For E2E testing, we need a Python interpreter with `pyarrow` installed. The project uses `uv` for Python dependency management, which creates `.venv/` in the project root.

Attempted solutions:
1. Symlink `~/.casparian_flow/venvs/test_env_hash_123/bin/python` → system python3
   - **Failed**: System Python doesn't have pyarrow

2. Symlink to `.venv/bin/python`
   - **Failed**: Python venvs don't work via binary symlink alone; site-packages aren't found

This raises a deeper question: is the current venv-path model the right approach?

## Options

**Note:** Options B-D are retained for future evaluation but are out of scope for v1.

### Option A: Keep Current Model (venv paths)

**How it works:**
- Worker expects pre-provisioned venvs at `~/.casparian_flow/venvs/{env_hash}/`
- External tooling (installer, admin, or separate service) creates these venvs
- env_hash is a content hash of dependencies, ensuring reproducibility

**For E2E test fix:**
```bash
# Symlink entire venv directory, not just binary
ln -s "$PROJECT_ROOT/.venv" "$HOME/.casparian_flow/venvs/test_env_hash_123"
```

**Pros:**
- Minimal code changes
- Clear separation: worker executes, something else provisions
- Works with any Python environment manager

**Cons:**
- Requires external venv management
- Path conventions are implicit contracts
- Testing requires mocking filesystem structure

---

### Option B: Integrate uv Directly

**How it works:**
- Worker shells out to `uv run` instead of directly spawning Python
- Command: `uv run --frozen --project {manifest_path} bridge_shim.py`
- uv handles environment resolution, caching, and execution

**Changes required:**
- Worker calls `uv run` instead of `{venv}/bin/python`
- env_hash becomes a lockfile hash (or we pass a manifest/lockfile path)
- uv is optional on worker machines (fallback to direct interpreter spawn if missing)

**Pros:**
- uv is Rust-native, extremely fast (~100ms cold start)
- Automatic dependency resolution and caching
- No manual venv provisioning
- Single tool for dev and production

**Cons:**
- uv dependency for best macOS compatibility (direct spawn can be less reliable)
- Changes execution model
- Less control over exact Python binary used
- uv is relatively new (though backed by Astral, well-maintained)

---

### Option C: Hybrid - uv for Provisioning, Direct Execution

**How it works:**
- Use uv to create/sync venvs at known paths (ahead of time or on-demand)
- Worker still executes `{venv}/bin/python` directly
- Best of both: fast provisioning via uv, simple execution model

**Example provisioning:**
```bash
uv venv ~/.casparian_flow/venvs/{env_hash}
uv pip install -r requirements.txt --python ~/.casparian_flow/venvs/{env_hash}
```

**Pros:**
- Keeps worker execution simple
- Fast venv creation via uv
- Works if uv isn't available at runtime (venvs pre-created)

**Cons:**
- Still need venv management logic somewhere
- Two-step process (provision, then execute)

---

### Option D: Embedded Python (PyO3)

**How it works:**
- Embed Python interpreter directly in Rust worker via PyO3
- No subprocess spawning
- Direct memory sharing possible

**Pros:**
- No subprocess overhead
- Tighter integration
- Could share Arrow memory directly

**Cons:**
- Major architectural change
- GIL considerations for parallelism
- Build complexity (linking Python)
- Loses privilege separation (plugins run in-process)

---

## Evaluation Criteria

1. **Simplicity**: How much code/infrastructure is needed?
2. **Reliability**: What can go wrong? How debuggable?
3. **Performance**: Startup latency, memory overhead
4. **Operability**: What does deployment look like?
5. **Flexibility**: Can users bring their own Python/environments?
6. **Security**: Privilege separation between host and guest code

## Current State

- Project uses `uv` for development
- Worker code assumes venv paths exist and fails if missing
- No venv provisioning logic implemented (by design for v1)
- E2E test requires a pre-provisioned env matching the test env_hash

## Question for Evaluation

Given the constraints and goals of a data processing system that runs user-provided Python plugins:

1. Which option best balances simplicity, reliability, and operability?
2. Is the dependency on `uv` acceptable, or should the system be Python-tooling-agnostic?
3. For the immediate E2E test, what's the pragmatic fix vs. the architecturally correct one?
