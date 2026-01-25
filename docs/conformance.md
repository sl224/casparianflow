# Conformance Suite

Casparian's conformance suite is a small, auditable set of tests that define
what the product guarantees. Each test name is prefixed so it can be run by
tier or by substring.

## Tiers

- **Tier 0 (Smoke / Contract Surface)**
  - Runtime: seconds
  - Always-on, fast confidence that the CLI and JSON surface are usable.
- **Tier 1 (Integrity Guarantees)**
  - Runtime: minutes
  - Always-on in CI; validates data fidelity, lineage, atomicity, and identity.
- **Tier 2 (Stress / Scale / Regression)**
  - Runtime: variable
  - Ignored by default; run manually or nightly.

## Naming

All conformance tests follow the prefix:

- `conf_t0_*`
- `conf_t1_*`
- `conf_t2_*`

This allows running a targeted tier with `cargo test` substring matching.

## How to run

```bash
# Tier 0
./scripts/conformance.sh t0

# Tier 1
./scripts/conformance.sh t1

# Tier 2 (ignored tests)
./scripts/conformance.sh t2
```

## Tier 0 tests

- `conf_t0_source_json_and_sync` (crates/casparian/tests/cli_source_json.rs)
  - Proves: source add/list/show JSON contract + sync path.
- `conf_t0_config_json_paths` (crates/casparian/tests/conformance_t0_config.rs)
  - Proves: `casparian config --json` shape + CASPARIAN_HOME path resolution.

## Tier 1 tests

- `conf_t1_parquet_decimal_timestamp_tz_roundtrip` (crates/casparian_sinks/src/lib.rs)
  - Proves: Parquet sink preserves decimal + timestamp with tz values.
- `conf_t1_duckdb_decimal_timestamptz_roundtrip_values` (crates/casparian_sinks_duckdb/src/lib.rs)
  - Proves: DuckDB sink preserves decimal + timestamp with tz values.
- `conf_t1_lineage_injection_appends_columns` (crates/casparian_sinks/src/lib.rs)
  - Proves: lineage columns are appended with correct types.
- `conf_t1_sink_commit_guard_aborts_without_outputs` (crates/casparian_sinks/src/lib.rs)
  - Proves: atomic commit guard prevents partial outputs.
- `conf_t1_worker_rejects_reserved_lineage_columns` (crates/casparian_worker/src/worker.rs)
  - Proves: parsers cannot spoof runtime lineage columns.
- `conf_t1_quarantine_writes_outputs_and_quarantine` (crates/casparian_worker/src/worker.rs)
  - Proves: schema enforcement produces outputs + quarantine artifacts.
- `conf_t1_rename_preserves_file_id_and_tags` (crates/casparian/tests/conformance_t1_rename.rs)
  - Proves: rename preserves file identity and tags.

## Tier 2 tests

- Currently empty; use `#[ignore]` for stress/perf regression tests when added.
