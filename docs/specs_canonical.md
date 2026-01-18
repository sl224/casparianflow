# Specs Canonical Index

Status: Working Draft (directional, not binding)
Purpose: Single source of truth for which specs are canonical vs reference only.

Spec posture:
- Treat specs as guidance, not law; shipping is driven by PMF and actual behavior.
- When code and spec diverge, prefer code and update the spec quickly.
- Prune or clearly mark sections that are out of date.

## Canonical (System of Record)
- STRATEGY.md: Product strategy and vertical priority.
- docs/v1_scope.md: v1 scope and success metrics.
- docs/schema_rfc.md: Schema contract system (canonical RFC).
- docs/execution_plan.md: Implementation plan (current).
- specs/db.md: DB abstraction and job events (WIP, active).
- specs/db_actor.md: DB actor boundary (WIP, must align with DuckDB-first).
- specs/pipelines.md: Pipeline orchestration (WIP, active).
- docs/ppg_spec.md: Parameterizable Parser Generator (future, optional).
- docs/product/pricing_v2_refined.md: Pricing system of record.
- docs/product/validated_personas.md: ICP evidence and workflows.
- docs/product/domain_intelligence.md: File format catalog and detection hints.

## Active but Needs Alignment
- spec.md: High-level product spec; refresh to match v1 scope.
- README.md: Overview; update after v1 scope is finalized.
- docs/DUCKDB_MIGRATION_PLAN.md: Align with single-DB policy.

## Deprecated / Superseded
- specs/parser_schema_contract_rfc.md -> docs/schema_rfc.md

## Reference (Non-Canonical, Keep for Later)
- strategies/*.md (except strategies/finance.md for v1 GTM context).
- specs/features/* (post-v1 feature docs).
- specs/parsers/* (vertical parser specs post-v1).
- specs/meta/* (internal workflows, not product specs).
