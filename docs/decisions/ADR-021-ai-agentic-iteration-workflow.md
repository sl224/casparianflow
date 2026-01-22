# ADR-021: AI Agentic Iteration Workflow

Status: Accepted (v1)
Date: 2026-01-21
Owners: Platform + AI Infrastructure

> **Note:** This ADR is now part of v1 scope. MCP integration enables AI-assisted
> workflows while preserving human approval gates for write operations.

## Context

Casparian Flow supports AI-assisted parser development where an AI agent proposes
parsers and schema definitions iteratively. The current architecture requires
AST-extractable schema-as-code for publishing (see `docs/schema_rfc.md`), which
is correct for the Vault/Registry integrity model.

However, during AI **iteration** (not publishing), the AST-only extraction path
creates friction:

1. **Speed**: AST parsing adds latency to every iteration loop.
2. **Flexibility**: The agent needs to propose and test schema variations rapidly.
3. **Learning**: The agent needs rich, machine-readable error context to converge.

This ADR introduces a **dual-path architecture** that preserves publish-time
integrity while enabling fast agentic iteration.

## Decision

Split the schema contract workflow into two paths:

### 1. Ephemeral Schema Contracts (Iteration Path)

During AI iteration and backtest loops, allow schema definitions to be supplied
as **JSON directly** without AST extraction:

```
Agent proposes:
  - Parser code
  - Schema definition (JSON dict)
        ↓
Engine validates:
  - Canonicalizes JSON (sorted keys, stable field order)
  - Computes schema_hash = sha256(canonical_bytes)
  - Creates EphemeralSchemaContract (in-memory + optional cache)
        ↓
Backtest runs:
  - Uses ephemeral contract for validation
  - Emits rich violation context (see ADR-021 Section: ViolationContext)
  - Streams progress events (see specs/jobs_progress.md)
        ↓
Agent learns from feedback and iterates
```

**Key constraints for ephemeral contracts:**
- **NOT** written to `schema_contracts` table (system of record).
- **NOT** published to Registry/Vault.
- **STILL** canonicalized and hashed for reproducibility within the session.
- **STILL** validated by Rust/Arrow layer (no "silent accept").

### 2. Published Schema Contracts (Vault Path)

When iteration quality is acceptable, the agent (or user) promotes the schema:

```
casparian ai promote-schema --run <id> --output schema.py
```

This command:
1. Reads the canonical `SchemaDefinition` from the ephemeral contract.
2. Generates AST-extractable schema-as-code (Python dataclass/dict literals).
3. Outputs a `.py` file ready for inclusion in the parser.

At publish time:
- AST extraction validates the schema is truly literal.
- Gatekeeper, determinism checks, and fixture suite run.
- Schema hashes are stored in `schema_contracts` table.
- Bundle signing occurs (Vault tier).

## Data Structures

### EphemeralSchemaContract

```rust
/// Temporary contract used during AI iteration.
/// Not persisted to the system-of-record table.
pub struct EphemeralSchemaContract {
    /// Output name this contract applies to
    pub output_name: String,

    /// The schema definition (already parsed and validated)
    pub schema_definition: SchemaDefinition,

    /// SHA-256 of canonicalized schema JSON
    pub schema_hash: String,

    /// Source of this contract
    pub source: EphemeralSource,

    /// When this ephemeral contract was created
    pub created_at: DateTime<Utc>,

    /// Optional run ID for correlation
    pub run_id: Option<String>,
}

pub enum EphemeralSource {
    /// From AI iteration loop
    AiIteration,
    /// From CLI preview
    CliPreview,
    /// From TUI test
    TuiTest,
}
```

### Schema Definition (JSON Input)

The agent supplies a schema definition as JSON:

```json
{
  "output_name": "trades",
  "mode": "strict",
  "columns": [
    {
      "name": "order_id",
      "type": {"kind": "string"},
      "nullable": false
    },
    {
      "name": "price",
      "type": {"kind": "decimal", "precision": 18, "scale": 8},
      "nullable": false
    }
  ]
}
```

### Canonicalization

To ensure reproducibility, schema JSON is canonicalized before hashing:

```rust
fn canonicalize_schema(schema: &SchemaDefinition) -> Vec<u8> {
    // 1. Serialize with sorted keys
    // 2. No whitespace
    // 3. Deterministic field ordering
    serde_json::to_vec(&schema).expect("schema is always serializable")
}

fn compute_schema_hash(schema: &SchemaDefinition) -> String {
    let canonical = canonicalize_schema(schema);
    let hash = sha2::Sha256::digest(&canonical);
    hex::encode(hash)
}
```

## Storage

### Ephemeral Contracts (During Iteration)

**In-memory**: Primary storage during active backtest session.

**Optional file cache** (for debugging/audit):
```
~/.casparian_flow/ai/contracts/
├── {run_id}/
│   ├── ephemeral_contract.json    # The schema definition
│   ├── schema_hash.txt            # The computed hash
│   └── metadata.json              # run_id, timestamps, source
```

This cache is:
- **Optional** (can be disabled for performance).
- **Not** the system of record.
- **Useful** for debugging failed iterations.

### Published Contracts (After Promotion)

Written to `schema_contracts` table per existing `docs/schema_rfc.md`:
- `scope_id` derived from `parser_id + version + output_name`.
- `logic_hash` stored as advisory metadata.
- Full audit trail preserved.

## CLI Commands

### New Commands for AI Iteration

```bash
# Run backtest with ephemeral schema (JSON input)
casparian ai backtest parser.py \
  --schema ephemeral_schema.json \
  --input-dir ./samples/ \
  --follow                        # Stream progress events

# Promote ephemeral schema to schema-as-code
casparian ai promote-schema \
  --run <run_id> \
  --output parser_schema.py

# Or promote from a JSON file directly
casparian ai promote-schema \
  --from ephemeral_schema.json \
  --output parser_schema.py
```

### Integration with Existing Commands

```bash
# Existing publish command unchanged - requires AST-extractable schema
casparian publish parser.py --version 1.0.0
# Fails if schema not AST-extractable

# Existing run command can use ephemeral schema in dev mode
casparian run parser.py input.csv --dev --schema schema.json
```

## Workflow: AI Agentic Loop

### Phase A: Fast Iteration Loop

```
┌─────────────────────────────────────────────────────────────────┐
│                    AI ITERATION LOOP                             │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  1. Agent proposes:                                              │
│     • Parser code (Python)                                       │
│     • Schema definition (JSON)                                   │
│                                                                  │
│  2. Engine runs backtest:                                        │
│     • Creates EphemeralSchemaContract                            │
│     • Validates against schema                                   │
│     • Streams progress events                                    │
│     • Emits rich ViolationContext on errors                      │
│                                                                  │
│  3. Agent receives feedback:                                     │
│     • Progress: files_processed, rows_emitted, quarantine_pct    │
│     • Violations: type, samples, distributions, suggestions      │
│                                                                  │
│  4. Agent updates parser/schema and repeats                      │
│                                                                  │
│  Loop until: quality threshold met                               │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Phase B: Promotion to Publishable Artifact

```
┌─────────────────────────────────────────────────────────────────┐
│                    PROMOTION PATH                                │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  1. Quality threshold met:                                       │
│     • Pass rate > 95%                                            │
│     • Quarantine % < threshold                                   │
│                                                                  │
│  2. Promote schema:                                              │
│     $ casparian ai promote-schema --run <id> --output schema.py │
│                                                                  │
│  3. Generated schema-as-code:                                    │
│     outputs = {                                                  │
│         "trades": {                                              │
│             "mode": "strict",                                    │
│             "columns": [                                         │
│                 {"name": "order_id", "type": {"kind": "string"}, │
│                  "nullable": False},                             │
│                 ...                                              │
│             ]                                                    │
│         }                                                        │
│     }                                                            │
│                                                                  │
│  4. Publish with full checks:                                    │
│     $ casparian publish parser.py --version 1.0.0               │
│     • Gatekeeper                                                 │
│     • Determinism checks                                         │
│     • Fixture suite                                              │
│     • Vault signing                                              │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## Rationale

### Why Not Just Use AST Extraction Always?

| Concern | AST-Only | Ephemeral + Promote |
|---------|----------|---------------------|
| Iteration speed | Slow (parse → extract → validate) | Fast (JSON → validate) |
| Agent flexibility | Limited to literals | Full JSON expressiveness |
| Error feedback | Limited | Rich ViolationContext |
| Publish integrity | ✓ | ✓ (after promotion) |
| Vault compatibility | ✓ | ✓ (after promotion) |

### Why Not Just Use JSON Sidecars for Everything?

From `docs/schema_rfc.md`:
> "Hand-authored JSON sidecars are not supported for publish."

The ephemeral path is for **iteration only**. Publish requires schema-as-code
for:
- AST-verifiable integrity (no code execution at publish time).
- Reproducibility (source file is the single source of truth).
- Vault signing (canonical artifact derived from AST).

### Why Canonicalize Ephemeral Schemas?

Even during iteration:
- Reproducibility: same schema → same hash → same validation behavior.
- Debugging: can compare hashes across iterations.
- Correlation: attach schema_hash to backtest artifacts and diffs.

## Scope

### In Scope
- Ephemeral schema contract data structure and storage.
- `casparian ai backtest` command with ephemeral schema input.
- `casparian ai promote-schema` command for codegen.
- Progress streaming for AI backtest loops (see `specs/jobs_progress.md`).
- ViolationContext for machine-readable error feedback (see Section below).

### Out of Scope
- Changes to existing publish workflow.
- Changes to `schema_contracts` table schema.
- MCP/external AI service integration (this ADR covers the local execution path).

## Related Documents

- `docs/schema_rfc.md`: Master schema contract RFC (publish path).
- `specs/jobs_progress.md`: Progress streaming specification.
- `ADR-018-worker-cap-simplifications.md`: Worker constraints apply to AI backtests.

## Implementation Priority

1. **EphemeralSchemaContract struct** - Foundation for iteration.
2. **Schema canonicalization** - Reproducibility.
3. **CLI commands** - `ai backtest`, `ai promote-schema`.
4. **Progress streaming** - Agentic feedback.
5. **ViolationContext** - Machine-readable errors.

## Risks and Mitigations

**Risk**: Ephemeral contracts bypass Rust enforcement (silent accept).
**Mitigation**: Ephemeral contracts MUST go through the same Rust/Arrow validation
path as published contracts. The only difference is storage location.

**Risk**: Agents try to publish without promotion.
**Mitigation**: Publish command validates AST-extractability. Non-literal schemas
fail with a clear error pointing to `promote-schema`.

**Risk**: Ephemeral contract cache grows unbounded.
**Mitigation**: Add TTL-based cleanup (e.g., 7 days). Cache is optional.

## Success Criteria

- AI iteration loop runs at <500ms per backtest (small sample).
- Promoted schemas are indistinguishable from hand-written schema-as-code.
- No changes to existing publish workflow required.
- ViolationContext enables agent convergence in <10 iterations (typical cases).
