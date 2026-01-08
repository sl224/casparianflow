# Parallel Execution Plan Template

**Goal:** [One sentence describing what this parallel execution achieves]

**Philosophy:** [Core principle: data-oriented, foundation-first, etc.]

---

## COMPACTION-SAFE ORCHESTRATION

**CRITICAL:** This plan uses parallel agents. To survive conversation compaction:

1. **Before spawning workers:** Create checkpoint file in archive/
2. **After each phase:** Update checkpoint with progress
3. **On resume:** Read checkpoint, continue from current_phase
4. **On completion:** Mark checkpoint as COMPLETED

---

## CURRENT STATE ANALYSIS

### What EXISTS (Ready to Build On)

| Component | Location | Status | Notes |
|-----------|----------|--------|-------|
| Example | `path/to/file` | Working | Description |

### What DOESN'T EXIST (Must Build)

| Component | Priority | Complexity | Notes |
|-----------|----------|------------|-------|
| Feature A | P0 | High | Critical path |
| Feature B | P1 | Medium | Depends on A |

---

## FILE STRUCTURE (Target)

```
crates/
├── existing_crate/
│   └── src/
│       └── file.rs          # W1 MODIFIES: description
│
└── new_crate/               # W2 CREATES: New crate
    └── src/
        └── lib.rs           # W2: Main implementation
```

---

## FILE OWNERSHIP MATRIX

Prevents merge conflicts by assigning clear ownership.

| File | W1 | W2 | W3 | Notes |
|------|----|----|-----|-------|
| crate_a/src/lib.rs | PRIMARY | - | READ | W1 owns this file |
| crate_a/src/new.rs | - | PRIMARY | - | W2 creates this |
| crate_b/src/lib.rs | - | MODIFY | PRIMARY | W2 adds import, W3 owns |

**Legend:**
- PRIMARY: Creates or owns the file
- MODIFY: Makes specific changes (describe in Notes)
- READ: May read but not modify
- `-`: No interaction

---

## WORKER DEFINITIONS

### Worker 1: [Name] (Foundation)

**Worktree:** `../cf-w1`
**Branch:** `feat/worker-1-name`
**Files Owned:** List files this worker creates/modifies

**Task:**
1. Step one
2. Step two
3. Step three

**Validation:**
```bash
cd ../cf-w1 && cargo check && cargo test -p package_name
```

**Done When:**
- [ ] Criteria 1
- [ ] Criteria 2

---

### Worker 2: [Name]

**Worktree:** `../cf-w2`
**Branch:** `feat/worker-2-name`
**Depends On:** W1 (if applicable)
**Files Owned:** List files

**Task:**
1. Step one
2. Step two

**Validation:**
```bash
cd ../cf-w2 && cargo check && cargo test -p package_name
```

**Done When:**
- [ ] Criteria 1

---

## ORCHESTRATOR PROTOCOL

### Phase 1: Setup Worktrees

```bash
cd /path/to/repo
git worktree add ../cf-w1 -b feat/worker-1-name
git worktree add ../cf-w2 -b feat/worker-2-name
# ... for each worker
```

### Phase 2: Spawn Workers (Parallel)

Spawn all workers in a **single message** using Task tool with:
- `run_in_background: true`
- `subagent_type: "general-purpose"`

Each agent receives:
1. Their worker definition from this document
2. Instruction to work in their worktree directory
3. Instruction to commit with message `[W#] description`

### Phase 3: Monitor Progress

Use `TaskOutput` with `block: false` every 30-60 seconds.

Track status:
- W1: pending → running → validating → done/failed
- W2: pending → running → validating → done/failed

### Phase 4: Validate Each Worker

When worker reports done:
```bash
cd ../cf-w# && cargo check && cargo test
```

If validation fails: respawn agent with error context.

### Phase 5: Merge (Ordered)

Only after ALL workers pass validation:

```bash
cd /path/to/repo

# 1. Foundation workers first
git merge feat/worker-1-name --no-edit
cargo check

# 2. Dependent workers in order
git merge feat/worker-2-name --no-edit
# If conflict: [describe resolution strategy]
cargo check

# 3. Integration worker last
git merge feat/worker-N-name --no-edit
cargo check && cargo test
```

### Phase 6: Cleanup

```bash
git worktree remove ../cf-w1
git worktree remove ../cf-w2
git branch -d feat/worker-1-name feat/worker-2-name
```

---

## CHECKPOINT TEMPLATE

Save to `archive/CHECKPOINT_[feature].md`:

```markdown
# Checkpoint: [Feature Name]

**Created:** [timestamp]
**Last Updated:** [timestamp]
**Status:** IN_PROGRESS | BLOCKED | COMPLETED

## Worker Status

| Worker | Status | Last Update | Notes |
|--------|--------|-------------|-------|
| W1 | done | timestamp | Merged |
| W2 | running | timestamp | 80% complete |
| W3 | blocked | timestamp | Waiting on W1 |

## Current Phase

Phase 3: Monitoring

## Blockers

- None / Description of blocker

## Next Actions

1. Wait for W2 to complete
2. Validate W2
3. Begin merge sequence
```

---

## ANTI-PATTERNS

1. **Don't share files between workers** - Use ownership matrix
2. **Don't skip validation** - One broken worker breaks the merge
3. **Don't merge out of order** - Dependencies matter
4. **Don't forget checkpoints** - Compaction will lose context
5. **Don't spawn sequentially** - Spawn all workers in one message
