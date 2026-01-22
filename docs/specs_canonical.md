# Specs Canonical Index

Status: Working Draft (directional, not binding)
Purpose: Single source of truth for which specs are canonical vs reference only.

Spec posture:
- Treat specs as guidance, not law; shipping is driven by PMF and actual behavior.
- When code and spec diverge, prefer code and update the spec quickly.
- Prune or clearly mark sections that are out of date.

## Canonical (System of Record, v1)
- STRATEGY.md: Product strategy and vertical priority.
- docs/v1_scope.md: v1 scope and success metrics.
- docs/schema_rfc.md: Schema contract system (canonical RFC).
- docs/execution_plan.md: Implementation plan (current).
- docs/v1_checklist.md: v1 delivery checklist (status tracking).
- spec.md: High-level product spec (v1 direction).
- specs/db.md: DB abstraction and job events (WIP, active).
- specs/db_actor.md: DB actor boundary (WIP, must align with DuckDB-first).
- docs/product/pricing_v2_refined.md: Pricing system of record.
- docs/product/validated_personas.md: ICP evidence and workflows.
- docs/product/domain_intelligence.md: File format catalog and detection hints.

## Active Docs
- README.md: Repository overview and quickstart.

## Archived / Superseded
- specs/parser_schema_contract_rfc.md -> docs/schema_rfc.md
- docs/DUCKDB_MIGRATION_PLAN.md (historical migration notes; not current plan)

## Reference (Non-Canonical, Keep for Later)
- strategies/*.md (reference only; see STRATEGY.md for priorities).
- docs/ppg_spec.md (future optional).
- specs/pipelines.md (post-v1 scheduling/pipelines).
- specs/features/* (post-v1 feature docs).
- specs/parsers/* (vertical parser specs post-v1).
- specs/meta/* (internal workflows, not product specs).
