# Engineer Resolution: GAP-PIE-001

**Gap**: Path Intelligence phases have no success criteria
**Section**: 3.5.10 Implementation Phases
**Status**: Resolved

---

## Summary

The Path Intelligence Engine (Section 3.5.10) lists six implementation phases with time estimates but lacks:
1. Success criteria for each phase
2. Metrics to measure effectiveness
3. Gate criteria to proceed to next phase
4. Rollback criteria if phase fails

This resolution provides concrete, measurable criteria for each phase.

---

## Resolution: Phase Success Criteria

### Phase 1: Embedding Clustering (3-4 days)

**Objective**: Cluster file paths by semantic similarity using sentence embeddings + HDBSCAN.

**Success Criteria**:

| Metric | Target | Measurement |
|--------|--------|-------------|
| **Cluster Purity** | >= 85% | Paths in same cluster should match same glob pattern (validated against 3 test datasets) |
| **Clustering Latency** | < 500ms for 1000 paths | p95 latency on CPU (no GPU required) |
| **Noise Ratio** | <= 30% unclustered | At most 30% of paths labeled as noise (-1) by HDBSCAN |
| **Model Loading** | < 3s cold start | Time to load `all-MiniLM-L6-v2` on first use |

**Test Datasets**:
1. `demo/clustering/mixed_500/` - 500 files, 5 expected clusters
2. `demo/clustering/noisy_200/` - 200 files with intentionally messy naming
3. `demo/clustering/cross_source_100/` - 100 files from 3 different naming conventions

**Gate Criteria** (all must pass):
- [ ] Cluster purity >= 85% on test dataset 1
- [ ] Cluster purity >= 70% on test dataset 2 (noisy data allowed lower threshold)
- [ ] p95 latency < 500ms
- [ ] Unit tests for `cluster_paths()` pass
- [ ] Integration test: TUI displays clusters correctly

**Rollback Criteria**:
- If cluster purity < 70% on dataset 1 after 2 days of tuning: Fall back to algorithmic-only inference
- If latency > 2s consistently: Investigate alternative embedding models or reduce embedding dimensions

---

### Phase 2: Field Naming with Phi-3.5 (3-4 days)

**Objective**: Use local LLM to propose semantic field names from path segments.

**Success Criteria**:

| Metric | Target | Measurement |
|--------|--------|-------------|
| **Field Name Accuracy** | >= 75% | Proposed field names match user-expected names (human evaluation on 50 paths) |
| **Semantic Correctness** | >= 90% | Field names are semantically valid (snake_case, no typos, domain-appropriate) |
| **LLM Latency** | < 2s per path | Time from prompt to parsed JSON response |
| **Graceful Degradation** | 100% | System works if LLM unavailable (falls back to `segment_N` naming) |

**Evaluation Protocol**:
1. Create 50 representative paths with human-labeled "expected" field names
2. Run Path Intelligence on each path
3. Score: exact match = 1.0, semantic equivalent = 0.8, wrong but valid = 0.3, invalid = 0

**Gate Criteria** (all must pass):
- [ ] Field name accuracy >= 75% on evaluation set
- [ ] Semantic correctness >= 90%
- [ ] LLM graceful degradation works (test with Ollama stopped)
- [ ] Prompt template produces valid JSON >= 95% of the time
- [ ] TUI shows proposed field names with confidence scores

**Rollback Criteria**:
- If accuracy < 60% after prompt tuning: Disable LLM field naming, use prefix-based heuristics only
- If Phi-3.5 unavailable on target hardware: Document minimum requirements, gate on Qwen 2.5 3B instead

---

### Phase 3: Cross-Source Equivalence (2-3 days)

**Objective**: Detect semantically equivalent file structures across different sources.

**Success Criteria**:

| Metric | Target | Measurement |
|--------|--------|-------------|
| **Equivalence Precision** | >= 85% | Proposed equivalences are actually equivalent (no false positives) |
| **Equivalence Recall** | >= 70% | System finds most true equivalences (acceptable false negatives) |
| **Similarity Threshold Tuning** | Documented | Optimal threshold for `equivalence_threshold` config documented |
| **Unified Schema Generation** | Works | System generates unified extraction schema for equivalent sources |

**Test Scenarios**:
1. Mission data: `/data/mission_042/` vs `/archive/msn-42/` vs `/backup/MISSION.042/`
2. Client data: `/clients/ACME/` vs `/client_data/acme_corp/` vs `/cust/acme/`
3. Negative case: `/logs/app/` should NOT match `/data/customers/` despite both having dates

**Gate Criteria** (all must pass):
- [ ] Precision >= 85% on test scenarios
- [ ] Recall >= 70% on test scenarios
- [ ] False positive rate on negative cases = 0%
- [ ] CLI command `casparian sources --find-equivalents` works
- [ ] User can confirm/reject proposed equivalences in TUI

**Rollback Criteria**:
- If precision < 70%: Raise default `equivalence_threshold` to 0.85
- If too many false positives: Add manual confirmation requirement (no auto-merge)

---

### Phase 4: Single-File Proposals (2-3 days)

**Objective**: Propose extraction fields from a single example file (no 3+ file requirement).

**Success Criteria**:

| Metric | Target | Measurement |
|--------|--------|-------------|
| **Proposal Quality** | >= 70% useful | Proposed fields are accepted by users >= 70% of the time |
| **Confidence Calibration** | r >= 0.6 | Confidence scores correlate with actual acceptance rate |
| **Bootstrap Success** | >= 80% | Users successfully create rules from single-file proposals 80% of time |
| **Edit Rate** | <= 50% | Users edit <= 50% of proposed fields (rest accepted as-is) |

**Confidence Calibration Protocol**:
1. Collect 100 single-file proposals with confidence scores
2. Track which proposals were accepted vs rejected
3. Calculate correlation (Pearson r) between confidence and acceptance

**Gate Criteria** (all must pass):
- [ ] Proposal quality >= 70% (measured via user study or internal testing)
- [ ] Confidence scores show meaningful differentiation (high-confidence accepted more)
- [ ] CLI `casparian extract <single_file>` works end-to-end
- [ ] TUI shows confidence bars and allows accept/edit/reject

**Rollback Criteria**:
- If proposal quality < 50%: Require "More examples" flow by default
- If confidence uncorrelated: Remove confidence display (show all proposals equally)

---

### Phase 5: Training Data Flywheel (1-2 weeks)

**Objective**: Capture user-approved rules as training data for future model improvements.

**Success Criteria**:

| Metric | Target | Measurement |
|--------|--------|-------------|
| **Data Capture Rate** | >= 95% | Approved rules are stored in training format 95%+ of time |
| **Data Quality** | >= 90% usable | Captured examples are valid training examples (no corruption) |
| **Privacy Compliance** | 100% | No raw paths stored; only sanitized paths and field mappings |
| **Retrieval Works** | < 100ms | Similar past rules retrieved in < 100ms for 10K stored rules |

**Storage Schema Validation**:
- Training examples include: sanitized paths, approved fields, field mappings
- Schema versioned for future migrations
- Export format compatible with sentence-transformers fine-tuning

**Gate Criteria** (all must pass):
- [ ] Training data capture works for all wizard types
- [ ] Sanitization applied before storage (privacy audit pass)
- [ ] Retrieval query returns relevant past rules
- [ ] Storage scales to 10K+ rules without degradation
- [ ] User can view/delete their training data contributions

**Rollback Criteria**:
- If privacy audit fails: Disable training data capture until fixed
- If storage performance degrades: Add pagination, limit retrieval to top-K

---

### Phase 6: Fine-Tuned Embeddings (2-3 weeks)

**Objective**: Train custom embedding adapter using collected training data.

**Prerequisites**: Phases 1-5 complete AND >= 500 training examples collected

**Success Criteria**:

| Metric | Target | Measurement |
|--------|--------|-------------|
| **Clustering Improvement** | >= 10% relative | Fine-tuned model improves cluster purity by >= 10% over baseline |
| **Field Naming Improvement** | >= 10% relative | Fine-tuned model improves field name accuracy by >= 10% |
| **No Regression** | < 5% | Performance on standard benchmarks does not regress > 5% |
| **Model Size** | < 50MB adapter | Adapter weights add < 50MB to base model |

**A/B Testing Protocol**:
1. Split test set 50/50
2. Run baseline model (Phase 1) on group A
3. Run fine-tuned model on group B
4. Compare metrics, require statistical significance (p < 0.05)

**Gate Criteria** (all must pass):
- [ ] >= 500 training examples available
- [ ] A/B test shows >= 10% improvement with p < 0.05
- [ ] No regression on standard sentence similarity benchmarks
- [ ] Model loads within existing latency budget (< 3s cold start)
- [ ] Rollback to base model possible via config flag

**Rollback Criteria**:
- If improvement < 5%: Keep base model, document learnings, retry with more data
- If regression detected post-deploy: Automatic rollback to base model via config
- If training diverges: Stop training, use last checkpoint, document hyperparameter bounds

---

## Phase Progression Decision Tree

```
Phase N Complete?
       │
       ├─ All gate criteria pass?
       │         │
       │         ├─ YES → Proceed to Phase N+1
       │         │
       │         └─ NO → Apply rollback criteria
       │                        │
       │                        ├─ Rollback succeeds → Document, proceed with reduced scope
       │                        │
       │                        └─ Rollback fails → Stop, escalate for design review
       │
       └─ Time estimate exceeded by 2x?
                  │
                  ├─ YES → Force checkpoint review
                  │              │
                  │              ├─ Progress > 70% → Extend 50%, continue
                  │              │
                  │              └─ Progress <= 70% → Evaluate pivot or cancel
                  │
                  └─ NO → Continue
```

---

## Global Metrics Dashboard

Track across all phases:

| Metric | Description | Alert Threshold |
|--------|-------------|-----------------|
| **User Acceptance Rate** | % of AI proposals accepted without edits | < 50% |
| **Time to First Value** | Time from scan to first approved rule | > 5 minutes |
| **Correction Overhead** | Time spent correcting AI proposals | > 30% of total time |
| **System Availability** | Uptime of Path Intelligence features | < 99% |
| **Privacy Violations** | Unsanitized paths sent to external services | > 0 (critical alert) |

---

## Revision History

| Date | Version | Author | Changes |
|------|---------|--------|---------|
| 2026-01-13 | 1.0 | Engineer | Initial resolution for GAP-PIE-001 |
