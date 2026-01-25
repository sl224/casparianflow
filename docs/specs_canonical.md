# Specs Canonical Index

**Status**: canonical
**Last verified against code**: 2026-01-24

Purpose: Single source of truth for which specs are canonical vs reference only.

Spec posture:
- Treat specs as guidance, not law; shipping is driven by PMF and actual behavior.
- When code and spec diverge, prefer code and update the spec quickly.
- Prune or clearly mark sections that are out of date.

**See also:** [docs/index.md](index.md) for complete documentation index.

## Canonical (System of Record, v1)
- STRATEGY.md: Product strategy and vertical priority (DFIR-first).
- docs/v1_scope.md: v1 scope and success metrics.
- docs/schema_rfc.md: Schema contract system (canonical RFC).
- docs/execution_plan.md: Implementation plan (current).
- docs/v1_checklist.md: v1 delivery checklist (status tracking).
- spec.md: High-level product spec (v1 direction).
- docs/product/pricing.md: Pricing system of record (DFIR-first, annual-first).
- docs/product/validated_personas.md: ICP evidence and workflows.
- docs/product/domain_intelligence.md: File format catalog and detection hints.

## Archived (Superseded)
- docs/product/_archive/pricing_v2_refined.md: Finance-first pricing (deprecated).

## Active Docs
- README.md: Repository overview and quickstart.
- docs/trust_guarantees.md: Trust model and security.

## Archived / Superseded
- specs/archive/db.md: Outdated async actor design (superseded by DuckDB)
- specs/archive/db_actor.md: Outdated actor boundary (superseded by DuckDB)
- docs/archive/DUCKDB_MIGRATION_PLAN.md: Historical migration notes
- docs/archive/claude_docs_legacy/: Outdated LLM context files

## Reference (Non-Canonical, Keep for Later)
- strategies/*.md (reference only; see STRATEGY.md for priorities).
- docs/ppg_spec.md (future optional).
- specs/pipelines.md (post-v1 scheduling/pipelines).
- specs/features/* (post-v1 feature docs).
- specs/parsers/* (vertical parser specs post-v1).
- specs/meta/* (internal workflows, not product specs).
