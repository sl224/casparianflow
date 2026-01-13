# Extraction API Refinement - Status

**Started:** 2026-01-12
**Source Specs:** `extraction_rules.md` (1343 lines), `semantic_path_mapping.md` (1681 lines)
**Goal:** Sharp, simple API delivering 80% value with 20% complexity
**Target:** <1000 lines combined, <5 min to first extraction

---

## Gap Inventory

### Complexity Gaps (CRITICAL - Address First)
- [ ] **GAP-SIMPLE-001**: Two specs (3024 lines) for one user goal - extract metadata from paths
- [ ] **GAP-SIMPLE-002**: Semantic vocabulary (9 primitives, variants, parameters) may be premature abstraction
- [ ] **GAP-SIMPLE-003**: No single-line rule syntax for simple cases
- [ ] **GAP-SIMPLE-004**: Full YAML schema requires 8+ fields when user just wants pattern + field

### API Surface Gaps (HIGH)
- [ ] **GAP-API-001**: Minimal CLI undefined - what's `casparian rules create` look like?
- [ ] **GAP-API-002**: Example-first authoring (`from-example`) not fully specified
- [ ] **GAP-API-003**: Progressive disclosure tiers (Auto/Templates/Full) not implemented
- [ ] **GAP-API-004**: No quick "what did this extract?" command for debugging

### User Journey Gaps (HIGH)
- [ ] **GAP-UX-001**: Onboarding flow undefined - how does new user get to first extraction?
- [ ] **GAP-UX-002**: Coverage report UI exists in spec but TUI wireframes missing
- [ ] **GAP-UX-003**: Near-miss surfacing detection exists but presentation unclear
- [ ] **GAP-UX-004**: Rule testing feedback loop - how does user iterate?

### Semantic Layer Gaps (MEDIUM - May Be Cut)
- [ ] **GAP-SEM-001**: Is vocabulary system necessary or can patterns be inferred algorithmically?
- [ ] **GAP-SEM-002**: Partial recognition repair flow adds complexity - is it worth it?
- [ ] **GAP-SEM-003**: Link desync tracking (LINKED/MODIFIED/DETACHED) - needed?
- [ ] **GAP-SEM-004**: Equivalence classes - useful or over-engineering?

### AI Integration Gaps (MEDIUM)
- [ ] **GAP-AI-001**: AI boundary unclear - what exactly should AI do vs algorithm?
- [ ] **GAP-AI-002**: AI proposal workflow needs simplification
- [ ] **GAP-AI-003**: No fallback when AI unavailable - should feel seamless

### Template Gaps (HIGH - Quick Win)
- [ ] **GAP-TPL-001**: Defense template defined but not battle-tested
- [ ] **GAP-TPL-002**: Healthcare template assumes HL7 structure - validate
- [ ] **GAP-TPL-003**: Finance template for FIX logs - needs real examples
- [ ] **GAP-TPL-004**: Legal eDiscovery template missing entirely

### Data Model Gaps (MEDIUM)
- [ ] **GAP-DATA-001**: Rule versioning not specified - how do rules evolve?
- [ ] **GAP-DATA-002**: Multi-source rules - can one rule apply to multiple sources?
- [ ] **GAP-DATA-003**: Extraction history/audit trail undefined

---

## Issues

- [ ] **ISSUE-SCOPE-001** [CRITICAL]: Two specs may need to merge into one
- [ ] **ISSUE-SCOPE-002** [HIGH]: Semantic layer may be cut entirely for v1
- [ ] **ISSUE-SCOPE-003** [MEDIUM]: Current specs optimized for completeness, not usability

---

## Open Questions

- [ ] **OQ-001**: Should semantic_path_mapping.md be deferred to v2?
- [ ] **OQ-002**: Can we ship extraction_rules.md alone and add semantics later?
- [ ] **OQ-003**: What's the minimal viable template library (2 verticals? 4?)
- [ ] **OQ-004**: Is DFA-based multi-pattern matching (globset) necessary for v1?
- [ ] **OQ-005**: Should rules be source-scoped or global?

---

## Round Progress

| Round | Gaps In | Resolved | New | Gaps Out | Net | State |
|-------|---------|----------|-----|----------|-----|-------|
| 0     | 24      | -        | -   | 24       | -   | INITIAL |
| 1     | 24      | 0        | 0   | 24       | 0   | NEEDS_REVISION |
| 2     | 24      | 4        | 3   | 23       | +1  | **APPROVED** |

### Round 2 Resolution
- GAP-SIMPLE-001: RESOLVED (unified two-tier API)
- GAP-SIMPLE-002: RESOLVED (semantic layer optional)
- GAP-API-001: RESOLVED (minimal CLI defined)
- GAP-UX-001: PARTIALLY (confidence UI specified)

### New Gaps from Round 2
- GAP-TPL-002: Template coverage validation
- GAP-EQUIV-001: Equivalence class conflict resolution
- GAP-CONF-001: Confidence threshold configuration

---

## Priority Order

**Foundations (Round 1-3):**
1. GAP-SIMPLE-001 - Spec consolidation decision
2. GAP-SIMPLE-002 - Semantic layer verdict
3. GAP-API-001 - Minimal CLI definition

**Core API (Round 4-6):**
4. GAP-API-002 - Example-first authoring
5. GAP-UX-001 - Onboarding flow
6. GAP-TPL-001-004 - Template validation

**Polish (Round 7+):**
7. Remaining UX gaps
8. Data model gaps
9. AI integration refinement

---

## Meta-Observations

This refinement is about **subtraction, not addition**. The specs are comprehensive but potentially over-engineered. Success means:
- Fewer lines of spec
- Fewer concepts to learn
- Faster time to value
- Same or better outcomes for users
