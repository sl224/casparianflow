# ADR-019: Enable Per-Output Sink Routing (Fix Sentinel Truncation)

Status: Accepted  
Date: 2026-01-18  
Owners: Platform + Product  

## Context
Many parsers emit multiple output models from a single input file (HL7, FIX,
PST, manufacturing exports). It is common to want different destinations for
different outputs (e.g., raw messages to Parquet, summary tables to DuckDB, or
some outputs to a different path).

The worker already supports per-output sink selection via `SinkConfig.topic`
matching the output name:
- `find_sink_config(cmd, output_name)` selects the sink with `topic == output_name`
  and falls back to the first sink if no match exists.

However, the Sentinel currently truncates sink configs to a single entry and
overwrites the topic to `"output"`. This makes per-output routing impossible
even if multiple sinks are configured in `cf_topic_config`.

## Problem
Multi-sink configuration is effectively ignored:
- Multiple sink rows in `cf_topic_config` are truncated to the first entry.
- The selected sink topic is forced to `"output"`, so output name matching
  never occurs.

Result: all outputs are written to the same sink, regardless of configuration.

## Decision
Enable per-output sink routing by passing all sink configs to the worker and
preserving each config's `topic` as the output name.

## Proposed Changes
1) **Sentinel**: stop truncating sink configs and stop overwriting `topic`.
   - Pass all configured `SinkConfig` entries for the plugin.
   - If no sinks exist, add a single default sink as today.
   - Enforce uniqueness on `(plugin_name, topic_name)` to avoid ambiguous routing.

2) **Worker**: keep current routing behavior.
   - Use `SinkConfig.topic` to match output name.
   - If no match, use an explicit default sink (`topic="*"`).
   - If there are multiple sinks and no explicit default, fail the job with a clear error.
   - Single-sink configs still apply to all outputs.
   - Optionally warn when output has no matching sink (observability only).

3) **Docs**: clarify that `cf_topic_config.topic_name` must match output
   names emitted by the parser (not file tags).

## Workflow Example
```
Plugin outputs: hl7_messages, hl7_patients, hl7_observations

cf_topic_config:
  (hl7_parser, hl7_messages)     -> parquet:///data/raw/
  (hl7_parser, hl7_patients)     -> duckdb:///data/hl7.db
  (hl7_parser, hl7_observations) -> parquet:///data/clinical/
```
Result: each output routes to its configured sink.

### Alternative (Single Sink Per Job)
If we choose a single sink per job, all outputs go to the same destination:

```
parquet://./output/
  -> hl7_messages_{job}.parquet
  -> hl7_patients_{job}.parquet
  -> hl7_observations_{job}.parquet
```

In that model, topic-based sink routing can be removed entirely; tags/topics
are still used for input selection, not output routing.

## Why This Aligns with Customer Needs
- Multi-model files are common in every target vertical.
- Regulated teams often separate raw vs derived outputs.
- Keeping per-output routing avoids needing a second export job just to split
  destinations.

## Scope
- No change to multi-output generation or schema contracts.
- No change to sink writers; only routing and config handling.

## Code Touchpoints
- `crates/casparian_sentinel/src/sentinel.rs`  
  - Remove `sinks.truncate(1)` and `first.topic = "output"` normalization.
- `crates/casparian_worker/src/worker.rs`  
  - Keep `find_sink_config` as-is; optionally add a warning on no match.
- `docs/specs` or CLI docs (if any) describing sink config semantics.

## Validation
- Add a test where a parser emits two outputs and each routes to a different
  sink URI.
- Ensure single-sink configs still behave as before.
