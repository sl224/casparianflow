# Execution Plan: Language-Neutral Subprocess Plugins

Status: Working plan (pre‑v1, breaking changes allowed)  
Last updated: 2026-01-21  
Scope: Add a language‑neutral subprocess plugin runtime while keeping Python shim unchanged.

This plan is written so a new Codex session can implement the feature with no prior context.

---

## Context Snapshot (Current Code)

- **Python shim runtime** is implemented in:
  - `crates/casparian_worker/shim/bridge_shim.py` (guest process)
  - `crates/casparian_worker/src/bridge.rs` (host; TCP + framed Arrow IPC)
- **Publish flow** now requires:
  - `casparian.toml` manifest (flat fields: `name`, `version`, `protocol_version`)
  - `outputs` dict literal in plugin source (AST‑extracted via Python)
  - `schema_artifacts_json` compiled to `SchemaDefinition` JSON
  - `artifact_hash = sha256(source + lockfile + manifest_json + schema_artifacts_json)`
  - Implemented in `crates/casparian/src/publish.rs`
- **Deploy command** carries:
  - `manifest_json`, `protocol_version`, `schema_artifacts_json`
  - Updated in `crates/casparian_protocol/src/types.rs`
- **Sentinel deploy** validates manifest + schema artifacts, computes artifact hash, inserts into `cf_plugin_manifest`, and **creates schema contracts** (enforcing “schema change => version bump”).
  - `crates/casparian_sentinel/src/sentinel.rs`
- **DB schema** (pre‑v1, reset allowed) now includes:
  - `cf_plugin_manifest.manifest_json`, `.protocol_version`, `.schema_artifacts_json`
  - `crates/casparian_sentinel/src/db/queue.rs`
- **Idempotency** uses `schema_hash` (blake3 of SchemaDefinition JSON) + output target keys; table name suffix uses schema hash:
  - `crates/casparian_protocol/src/idempotency.rs`

Important constraint (pre‑v1): **No migrations**. Delete `~/.casparian_flow/casparian_flow.duckdb` when schema changes.

---

## Decisions (Locked for This Plan)

1) **Runtime abstraction:** add `PluginRuntime` with `PythonShimRuntime` (existing) + `NativeSubprocessRuntime` (new).
2) **Language‑neutral protocol (native):** plugin executable emits:
   - **stdout:** Arrow IPC streams (binary)
   - **stderr:** NDJSON control frames (UTF‑8)
3) **Manifest‑first publishing:** canonical schema artifacts (JSON) remain the registry + signing unit.
4) **Trust policy:** native binaries run **only if signed** by a trusted key unless explicitly allowed by local dev override.
5) **Schema hash algorithm:**
   - Keep `schema_hash = blake3(canonical_schema_json)` (existing)
   - Use sha256 for **bundle index/signature** (signing only)

---

## Target Architecture

### Runtime kinds
- `python_shim` (current behavior)
- `native_exec` (new; subprocess)

### Native protocol (stderr control frames)
All frames are NDJSON with `type`:

- `hello` (MUST be first):
  ```json
  {"type":"hello","protocol":"0.1","parser_id":"evtx","parser_version":"0.1.0","capabilities":{"multi_output":true}}
  ```

- `output_begin` (required per output; MUST precede stdout stream):
  ```json
  {"type":"output_begin","output":"events","schema_hash":"<blake3>","stream_index":0}
  ```

- `output_end` (required after stream):
  ```json
  {"type":"output_end","output":"events","rows_emitted":12345,"stream_index":0}
  ```

Optional:
- `progress`, `warning`, `error`, `row_error` (row_error is diagnostics-only for v1).

### Arrow streams (stdout)
- One stream per output, sequential only (no interleaving).
- Schema must match stored contract (engine validates).
- If stdout has non‑Arrow bytes → fatal protocol error.

---

## Data Model Changes

### 1) `cf_plugin_manifest` additions (required)
Add columns (pre‑v1 reset allowed):
- `runtime_kind TEXT NOT NULL` (`python_shim|native_exec`)
- `entrypoint TEXT NOT NULL` (path to plugin file or installed executable)
- `platform_os TEXT NULL` (native only)
- `platform_arch TEXT NULL` (native only)
- `signature_verified BOOLEAN DEFAULT false`
- `signer_id TEXT NULL`
- `outputs_json TEXT NOT NULL` (or normalize via outputs table)

**Uniqueness**: current `UNIQUE(plugin_name, version)` is insufficient for native; must include platform+runtime.  
Option A (simpler): `UNIQUE(plugin_name, version, runtime_kind, platform_os, platform_arch)`  
Option B: drop uniqueness and enforce in code.

### 2) `cf_plugin_outputs` (recommended normalized table)
Fields:
- `plugin_name`
- `version`
- `runtime_kind`
- `platform_os`
- `platform_arch`
- `output_name`
- `schema_hash` (blake3 of schema JSON)
- `schema_scope_id` (derive_scope_id)
- `schema_path` (relative in bundle; optional)


---

## Execution Plan (Implementation Steps)

### Phase 1 — Protocol + DB wiring

1) **Extend manifest schema** (decide one format only):
   - Keep flat `casparian.toml` for now, add:
     ```
     runtime_kind = "python_shim" | "native_exec"
     entrypoint = "parser.py:parse" (python) or "bin/evtx_plugin" (native)
     platform_os = "linux" (native)
     platform_arch = "x86_64" (native)
     ```
   - Update parsing in `crates/casparian/src/publish.rs` to include these fields.

2) **Update `cf_plugin_manifest` schema**:
   - `crates/casparian_sentinel/src/db/queue.rs`
   - Add required columns + `require_columns` check
   - Update uniqueness as per above.

3) **Update Sentinel deploy handling**:
   - `crates/casparian_sentinel/src/sentinel.rs`
   - Validate `runtime_kind`, `entrypoint`, and platform for native.
   - Insert fields into `cf_plugin_manifest`.
   - Create/update `cf_plugin_outputs` (if chosen).
   - Continue creating schema contracts from `schema_artifacts_json`.

4) **Extend Dispatch query + payload**:
   - `DispatchQueryResult` must include `runtime_kind`, `entrypoint`, platform, schema info.
   - Extend `casparian_protocol::types::DispatchCommand` with:
     - `runtime_kind`, `entrypoint`, `platform_os`, `platform_arch`
     - make `env_hash`, `source_code` optional for non‑Python
   - Update Sentinel dispatch query + creation of DispatchCommand.

### Phase 2 — Trust + Bundle import

1) **Add trust config parsing**:
   - File: `~/.casparian_flow/config.toml`
   - Example:
     ```toml
     [trust]
     mode = "vault_signed_only" # default
     allowed_signers = ["casparian_root_2026"]

     [trust.keys]
     casparian_root_2026 = "BASE64_ED25519_PUB"
     ```
   - Add a `allow_unsigned_native` (dev override) flag.

2) **Bundle format**
   ```
   bundle_root/
     casparian.toml
     schemas/*.schema.json
     bin/<executable>
     bundle.index.json  (canonical JSON; sha256 over bytes)
     bundle.sig         (ed25519 signature over index)
   ```
   Use sha256 for bundle index/signature (not for schema hash).

3) **CLI commands**
   - `casparian plugin import <bundle_path>`
   - `casparian plugin list`
   - `casparian plugin verify <plugin_id>@<version>`
   - Implement in `crates/casparian/src/cli/plugin.rs` (new module) and wire into CLI.
   - Import flow:
     - verify hashes
     - verify signature
     - copy to `~/.casparian_flow/plugins/<plugin>/<version>/<os>/<arch>/`
     - register in Sentinel DB (`cf_plugin_manifest` + `cf_plugin_outputs`)
     - populate schema contracts from schema artifacts

### Phase 3 — Worker runtime abstraction

1) **Introduce `PluginRuntime` trait**:
   - File: `crates/casparian_worker/src/runtime.rs` (new)
   - Trait:
     ```rust
     trait PluginRuntime {
         fn run_file(&self, ctx: RunContext, input_path: &Path) -> Result<RunOutputs>;
     }
     ```
   - Implement `PythonShimRuntime` as thin wrapper around existing bridge.

2) **Implement `NativeSubprocessRuntime`**:
   - Spawn process using `std::process::Command`.
   - Read **stderr** in a thread, parse NDJSON frames, send via channel.
   - Read **stdout** Arrow IPC streams in main thread.
   - Enforce `hello` timeout.
   - Enforce `output_begin` before any Arrow stream.
   - Validate `schema_hash` in `output_begin` matches stored manifest (blake3).
   - Use existing Rust schema validation/quarantine on each RecordBatch.
   - Fail run on protocol violations.

3) **Wire runtime selection**:
   - In worker job execution (likely `crates/casparian_worker/src/worker.rs`), select runtime by `DispatchCommand.runtime_kind`.
   - For native_exec: enforce trust policy using `signature_verified` + `signer_id`.

### Phase 4 — Tests

1) **Add toy native plugin fixture**:
   - Create minimal Rust or Go plugin that emits:
     - `hello`
     - `output_begin`
     - Arrow IPC stream with 2–3 rows
     - `output_end`
     - exit 0
   - Place under `tests/fixtures/native_plugin_*`.

2) **Integration tests**:
   - Import bundle → register → run → output written to DuckDB.
   - Validate schema enforcement.

3) **Negative tests**:
   - Missing `hello`
   - stdout includes non‑Arrow bytes
   - schema_hash mismatch
   - unsigned bundle in vault_signed_only mode

---

## File‑Level Touch List (Expected)

**Protocol / Types**
- `crates/casparian_protocol/src/types.rs`  
  - Extend `DispatchCommand`
  - Possibly add `runtime_kind` enums

**Sentinel**
- `crates/casparian_sentinel/src/db/queue.rs` (schema changes)
- `crates/casparian_sentinel/src/sentinel.rs` (deploy + dispatch)
- `crates/casparian_sentinel/src/db/models.rs` (PluginManifest struct fields)

**Worker**
- `crates/casparian_worker/src/runtime.rs` (new)
- `crates/casparian_worker/src/worker.rs` (runtime selection)
- `crates/casparian_worker/src/native_runtime.rs` (new)

**CLI**
- `crates/casparian/src/cli/plugin.rs` (new)
- `crates/casparian/src/main.rs` (wire command)

**Docs**
- `docs/schema_rfc.md` (already has future section)
- `docs/execution_plan.md` (optional summary)

---

## Acceptance Criteria

1) Worker can run a native bundle via subprocess and ingest Arrow output into DuckDB, enforcing schema contracts in Rust.
2) `casparian plugin import` installs and registers a signed bundle.
3) Unsigned native bundles are rejected by default; can be allowed only with explicit local override.
4) Protocol violations fail the run deterministically.
5) Existing Python shim behavior unchanged.

---

## Notes / Gotchas

- **Protocol mismatch**: current Python bridge uses TCP + custom frames; do not reuse it for native. Create a separate runtime that uses stdio.
- **Schema hash**: keep `schema_hash` = blake3(canonical schema JSON). Do not mix sha256 here.
- **Uniqueness**: platform‑specific binaries require schema keys to include platform/os/arch or you’ll collide.
- **Pre‑v1 reset**: changing DB schema requires deleting `~/.casparian_flow/casparian_flow.duckdb`.

---

## Progress Log

### 2026-01-21

- [x] Phase 1 / Step 1 — Extend manifest schema + parsing.
  - Log: Added `RuntimeKind` enum and manifest fields (`runtime_kind`, `entrypoint`, `platform_os`, `platform_arch`) with validation and JSON serialization; updated publish integration tests to include the new manifest fields.
  - Files: `crates/casparian/src/publish.rs`, `crates/casparian/tests/publish_integration.rs`

- [x] Phase 1 / Step 2 — Update `cf_plugin_manifest` schema.
  - Log: Expanded `cf_plugin_manifest` table with runtime/platform/signing/outputs columns, updated UNIQUE constraint to include runtime/platform, and extended `require_columns` checks.
  - Files: `crates/casparian_sentinel/src/db/queue.rs`

- [x] Phase 1 / Step 3 — Update Sentinel deploy handling.
  - Log: Parsed `runtime_kind`/`entrypoint`/platform fields from manifest JSON, validated native vs python platform requirements, inserted new runtime/platform/signing/output fields into `cf_plugin_manifest`, and updated manifest model fields.
  - Files: `crates/casparian_sentinel/src/sentinel.rs`, `crates/casparian_sentinel/src/db/models.rs`

- [x] Phase 2 / Step 1 — Add trust config parsing.
  - Log: Implemented trust config parser with strong types and validation (allowed_signers must exist in keys), added default loader and tests, and exposed module from the library.
  - Files: `crates/casparian/src/trust/config.rs`, `crates/casparian/src/trust/mod.rs`, `crates/casparian/src/lib.rs`

- [x] Phase 1 / Step 4 — Extend Dispatch query + payload.
  - Log: Added `RuntimeKind` to protocol, extended `DispatchCommand` with runtime/entrypoint/platform and optional env/source, updated sentinel dispatch query + validation, and adjusted worker/tests to handle optional env/source.
  - Files: `crates/casparian_protocol/src/types.rs`, `crates/casparian_protocol/src/lib.rs`, `crates/casparian_sentinel/src/sentinel.rs`, `crates/casparian_sentinel/tests/integration.rs`, `crates/casparian_worker/src/worker.rs`, `crates/casparian_worker/tests/integration.rs`, `crates/casparian_worker/tests/concurrency_test.rs`

- [x] Phase 2 / Step 2 — Bundle format + CLI commands.
  - Log: Added `plugin` CLI with import/list/verify, bundle index + signature verification, native bundle install path, registry insertion, and schema contract registration. Added Ed25519/base64 deps for signature verification.
  - Files: `crates/casparian/src/cli/plugin.rs`, `crates/casparian/src/cli/mod.rs`, `crates/casparian/src/main.rs`, `Cargo.toml`, `crates/casparian/Cargo.toml`

- [x] Phase 3 / Step 1 — Introduce PluginRuntime trait.
  - Log: Added runtime abstractions (`PluginRuntime`, `RunContext`, `RunOutputs`) and a Python shim runtime wrapper around the existing bridge.
  - Files: `crates/casparian_worker/src/runtime.rs`, `crates/casparian_worker/src/lib.rs`

- [x] Phase 3 / Step 2 — Implement NativeSubprocessRuntime.
  - Log: Added native subprocess runtime with stderr NDJSON control frame parsing, Arrow IPC stdout streaming, hello/output_begin/output_end enforcement, and schema hash checks.
  - Files: `crates/casparian_worker/src/native_runtime.rs`, `crates/casparian_worker/src/lib.rs`, `crates/casparian_worker/src/runtime.rs`

- [x] Phase 3 / Step 3 — Wire runtime selection.
  - Log: Switched worker execution to select Python vs native runtime, resolved native entrypoints from local plugin install path, enforced unsigned-native override via config/env, and added schema hash map for native validation.
  - Files: `crates/casparian_worker/src/worker.rs`, `crates/casparian_worker/Cargo.toml`, `crates/casparian_protocol/src/types.rs`, `crates/casparian_sentinel/src/sentinel.rs`, `crates/casparian_sentinel/tests/integration.rs`, `crates/casparian_worker/tests/integration.rs`, `crates/casparian_worker/tests/concurrency_test.rs`

- [x] Phase 4 / Step 1 — Add toy native plugin fixture.
  - Log: Added a minimal Rust native plugin fixture that emits hello/output_begin/output_end frames and an Arrow IPC stream, plus a matching schema JSON.
  - Files: `tests/fixtures/native_plugin_basic/Cargo.toml`, `tests/fixtures/native_plugin_basic/src/main.rs`, `tests/fixtures/native_plugin_basic/schemas/events.schema.json`

- [x] Phase 4 / Step 2 — Integration tests (bundle import → run).
  - Log: Added an end-to-end native bundle import test (ignored by default) that builds the fixture, signs the bundle, imports via CLI internals, and verifies registry insertion.
  - Files: `crates/casparian/src/cli/plugin.rs`

- [x] Phase 4 / Step 3 — Negative tests.
  - Log: Added native runtime protocol negative tests for missing hello, non-Arrow stdout, and schema hash mismatch; added trust enforcement test for unsigned bundles.
  - Files: `crates/casparian_worker/tests/native_runtime.rs`, `crates/casparian/src/cli/plugin.rs`
