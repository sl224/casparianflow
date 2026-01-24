# Data Model Maintenance Workflow

**Type:** Meta-specification (LLM Process Template)
**Version:** 1.3
**Category:** Analysis workflow (per workflow_manager.md Section 3.3.1)
**Purpose:** Periodic audit and cleanup of Rust data models (structs, enums, type aliases)
**Related:** spec_maintenance_workflow.md (spec corpus maintenance)

---

## 1. Overview

This workflow maintains the health of the **codebase's data models** by identifying unused types, duplicate structures, overly complex models, and opportunities for consolidation.

**Key Difference from Spec Maintenance:**
- **Spec Maintenance:** Documents → alignment with code
- **Data Model Maintenance:** Code → structural health and usage analysis

### 1.1 Design Principles

1. **Usage is Truth** - If a type isn't used, it's dead code
2. **Single Source of Truth** - Duplicate types create inconsistency
3. **Bounded Complexity** - Models should be comprehensible in isolation
4. **Crate Boundaries Matter** - Types should live in the crate that owns them
5. **Prefer Composition** - Smaller, composable types over monolithic ones

### 1.2 When to Run

| Trigger | Reason |
|---------|--------|
| **Quarterly** | Regular hygiene |
| **Before major refactor** | Identify cleanup opportunities |
| **After feature removal** | Find orphaned types |
| **New crate extraction** | Identify types to move |
| **Onboarding** | Understand model landscape |

### 1.3 Scope

This workflow audits:

| Category | Examples |
|----------|----------|
| **Structs** | Data transfer objects, domain models, config types |
| **Enums** | State machines, error types, variants |
| **Type Aliases** | `type Result<T> = std::result::Result<T, Error>` |
| **Newtypes** | `struct UserId(i64)` |

**Excluded:**
- Trait definitions (behavior, not data)
- Impl blocks (methods, not structure)
- Constants and statics

### 1.4 Audit Scoping

Audits can be scoped for partial analysis:

#### Scope Parameters

| Parameter | Syntax | Example |
|-----------|--------|---------|
| `--crate` | Crate name | `--crate casparian_worker` |
| `--pattern` | Type name glob | `--pattern "*Config"` |
| `--phase` | Phase range | `--phase 3-4` (only cross-model and recommendations) |
| `--exclude` | Exclusion glob | `--exclude "test_*"` |
| `--depth` | Nested analysis depth | `--depth 3` (default: 3 levels) |

#### Scope Combinations

**Single Crate Audit:**
```
Scope: crates/casparian_worker/src/**/*.rs
Phases: 1-5 (full workflow)
Output: Worker-specific inventory, usage, recommendations
```

**Type Pattern Audit:**
```
Scope: All crates
Filter: Types matching "*Error"
Phases: 1-5
Output: Error type consistency report
```

**Cross-Model Only (requires prior inventory):**
```
Scope: All crates
Input: Existing model_inventory.md from previous run
Phases: 3-5 (skip inventory and usage analysis)
Output: Fresh cross-model analysis using cached inventory
```

#### Scope Validation

Before starting scoped audit:

1. **Crate scope:** Verify crate exists in `crates/` directory
2. **Pattern scope:** Test pattern matches at least one type via grep
3. **Phase scope:** Validate dependencies:

| Phase | Requires |
|-------|----------|
| 1 (Inventory) | None |
| 2 (Usage) | Phase 1 output or `--inventory-file` |
| 3 (Cross-Model) | Phase 1 output or `--inventory-file` |
| 4 (Recommendations) | Phase 3 output |
| 5 (Execution) | Phase 4 output + user approval |

#### Incremental Audit Support

For large codebases, support incremental audits with merge capability:

**Modification Detection:** Use content hash of type definition (field names + types) to detect changes.

**Merge Algorithm:**
1. Load existing inventory (if `--merge-with` provided)
2. Run scoped audit on target
3. Merge results:
   - New types: Add to inventory
   - Modified types (hash changed): Update entry, mark as MODIFIED
   - Removed types: Mark as DELETED (don't auto-remove)

#### Merge Conflict Resolution

**Conflict Types:**

| Conflict | Definition | Resolution |
|----------|------------|------------|
| **Classification conflict** | Audit A says DEAD, Audit B says ACTIVE | Latest timestamp wins |
| **Metadata conflict** | Different field counts extracted | Re-extract from source |
| **Recommendation conflict** | Audits recommend different actions | Merge to strongest |
| **Deletion conflict** | Audit A deletes, Audit B modifies | Deletion wins |

**Resolution Rule:** Latest timestamp wins with full audit trail. Both entries are logged for traceability.

**Recommendation Merge Matrix:**

| Base Rec | Scoped Rec | Merged Rec | Rationale |
|----------|------------|------------|-----------|
| NONE | REMOVE | REMOVE | New information |
| REMOVE | NONE | REMOVE | Conservative |
| MERGE | SPLIT | NEEDS_REVIEW | Contradictory |
| REDUCE_VIS | MOVE | MOVE + REDUCE_VIS | Combine |

**Audit Trail:** All conflicts logged to `merge_conflicts.md` with timestamps, sources, and resolution applied. Human override available.

#### Scoped Output Structure

```
specs/meta/maintenance/data-models/
├── 2026-01-14/                   # Full audit (base pattern)
├── 2026-01-14-worker-only/       # Scoped audit (suffix extension)
│   ├── model_inventory.md        # Only casparian_worker types
│   ├── scope.json                # {"crate": "casparian_worker"}
│   └── ...
└── 2026-01-14-errors/            # Pattern audit
    ├── model_inventory.md        # Only *Error types
    ├── scope.json                # {"pattern": "*Error"}
    └── ...
```

---

## 2. Execution Model

### 2.1 Single-Instance Architecture

```
User initiates maintenance
        │
        ▼
┌───────────────────────────────────────────────────────────────┐
│                    MAINTENANCE AGENT                          │
│           (Single Claude instance, interactive)               │
│                                                               │
│  Phase 1 ──► Phase 2 ──► Phase 3 ──► Phase 4 ──► Phase 5     │
│  Inventory   Usage      Cross-Model  Recommend    Execute     │
└───────────────────────────────────────────────────────────────┘
        │
        ▼
  ┌─────────────┐
  │ USER REVIEW │  Review recommendations via AskUserQuestion
  └─────────────┘
        │
        ▼ (User approves)
┌───────────────────────────────────────────────────────────────┐
│                    MAINTENANCE AGENT                          │
│                     Phase 5: Execute                          │
└───────────────────────────────────────────────────────────────┘
```

### 2.2 Output Files

| Phase | File | Content |
|-------|------|---------|
| 1 | `model_inventory.md` | All types with metadata |
| 1 | `model_graph.json` | Type relationships (contains, references) |
| 2 | `usage_report.md` | Per-type usage analysis |
| 3 | `cross_model_report.md` | Duplicates, bloat, consolidation opportunities |
| 4 | `recommendations.md` | Prioritized action items |
| 5 | `execution_log.md` | Changes made |

---

## 3. Phase 1: Model Inventory

### 3.1 Scan Locations

```
crates/*/src/**/*.rs    # All Rust source files
```

### 3.2 Type Detection Patterns

**Structs:**
```rust
// Named struct
pub struct SourceConfig { ... }

// Tuple struct / Newtype
pub struct UserId(pub i64);

// Unit struct
pub struct Marker;
```

**Enums:**
```rust
pub enum JobStatus {
    Pending,
    Running,
    Complete,
    Failed(String),
}
```

**Type Aliases:**
```rust
pub type Result<T> = std::result::Result<T, CasparianError>;
type FileMap = HashMap<PathBuf, FileMetadata>;
```

### 3.2.1 Type Extraction Method (LLM Execution)

Since this workflow is executed by Claude (not programmatic tools), use these search strategies:

**Step 1: Glob for Source Files**
```
Pattern: crates/*/src/**/*.rs
Tool: Use Glob tool to get list of all Rust files
```

**Step 2: Grep for Type Definitions**

Use these exact grep patterns to find types:

| Type | Grep Pattern | Notes |
|------|--------------|-------|
| **Public Struct** | `^pub struct \w+` | Matches line start |
| **Private Struct** | `^struct \w+` | Less common |
| **Public Enum** | `^pub enum \w+` | Matches line start |
| **Private Enum** | `^enum \w+` | Less common |
| **Public Type Alias** | `^pub type \w+` | Module-level only |
| **Private Type Alias** | `^type \w+ =` | Inside impl blocks too |

**Step 3: Read Each File for Metadata Extraction**

For each file with matches, read the file and extract:

1. **Type Name**: The identifier after `struct`/`enum`/`type`
2. **Line Number**: From grep output
3. **Visibility**: `pub`, `pub(crate)`, `pub(super)`, or private
4. **Derives**: Look for `#[derive(...)]` on preceding lines (within 5 lines)
5. **Attributes**: Look for `#[...]` attributes (serde, cfg, allow, etc.)
6. **Field Count**: For structs, count fields in `{ }` block
7. **Variant Count**: For enums, count variants in `{ }` block
8. **Doc Comment**: Check for `///` or `//!` preceding the definition

**Example Extraction Flow:**

```
1. Glob("crates/*/src/**/*.rs") -> ["crates/casparian/src/scout/types.rs", ...]

2. For "types.rs":
   Grep("^pub struct") -> ["pub struct Source {" at line 17, ...]
   Grep("^pub enum") -> ["pub enum FileStatus {" at line 91, ...]

3. Read("types.rs"):
   - Line 142: "pub struct ScannedFile"
   - Lines 138-141: `#[derive(Debug, Clone, Serialize, Deserialize)]` + `#[serde(...)]`
   - Lines 143-159: Field definitions (count = 17)
   - Has `///` doc comment
```

**Handling Complex Cases:**

| Case | Strategy |
|------|----------|
| Multiline derives | Scan up to 10 lines above definition |
| Nested types | Grep with context (`-A 50`) to capture full body |
| Generic types | Include `<...>` in name: `struct Foo<T>` |
| Conditional compilation | Note `#[cfg(...)]` attribute |
| Macro-generated types | Skip or mark as "MACRO_GENERATED" |

**Trade-offs:**
- **Pros:** Simple grep patterns work for 95%+ of types; no need for full Rust parser
- **Cons:** May miss macro-generated types; conditional compilation types may be missed
- **Mitigation:** Add "PARSE_WARNING" category for uncertain extractions

#### Macro-Generated Type Detection

Types generated by macros require special handling since grep won't find explicit `struct`/`enum` definitions.

**Detection Strategy:**

| Macro Pattern | Detection Method | Classification |
|---------------|------------------|----------------|
| `#[derive(...)]` | Standard grep works | NORMAL - derive generates impls, not types |
| `diesel::table!` | Grep for `table!` macro | MACRO_GENERATED_TABLE |
| `bitflags!` | Grep for `bitflags!` | MACRO_GENERATED_FLAGS |
| `thiserror::Error` | Standard grep + derive | NORMAL - type is explicit |
| Custom proc macros | Grep for `#[proc_macro_name]` | NEEDS_MANUAL_REVIEW |

**Catalog of Known Macros:**

| Macro | Generates | Name Extraction |
|-------|-----------|-----------------|
| `diesel::table!` | Table type + columns | First identifier in block |
| `bitflags!` | Struct with flags | `struct Name` inside macro |
| `lazy_static!` | Static variable | `static ref NAME` inside |
| `derive(Builder)` | Builder struct | `{OriginalType}Builder` |

**Note:** This catalog is project-specific. Extend based on dependencies used.

**Skip List (generates impls, not types):**
- `#[derive(Debug, Clone, Serialize, ...)]` - Standard derives
- `#[async_trait]` - Only modifies trait
- `#[test]`, `#[tokio::test]` - Test annotations

### 3.3 Metadata Extraction

For each type, extract:

| Field | Source | Example |
|-------|--------|---------|
| `name` | Type identifier | `SourceConfig` |
| `kind` | struct/enum/alias | `struct` |
| `crate` | Parent crate | `casparian_scout` |
| `file` | File path | `src/scout/types.rs` |
| `line` | Line number | `45` |
| `visibility` | pub/pub(crate)/private | `pub` |
| `derives` | Derive macros | `[Debug, Clone, Serialize]` |
| `field_count` | Number of fields | `8` |
| `variant_count` | Enum variants | `4` |
| `doc_comment` | Has documentation? | `true` |
| `attributes` | Other attrs | `[serde(rename_all = "camelCase")]` |

#### 3.3.1 Generic Parameter Extraction

For types with generic parameters, extract and represent as follows:

**Extraction Regex:**
```
Pattern: ^pub\s+(struct|enum)\s+(\w+)<([^>]+)>
Capture: $1 = kind, $2 = name, $3 = generic parameters string
```

**Parameter Parsing:**

| Pattern | Interpretation | Representation |
|---------|----------------|----------------|
| `T` | Unbounded type param | `TypeParam("T", [])` |
| `T: Clone` | Single trait bound | `TypeParam("T", ["Clone"])` |
| `T: Clone + Debug` | Multiple bounds | `TypeParam("T", ["Clone", "Debug"])` |
| `'a` | Lifetime parameter | `Lifetime("'a")` |
| `const N: usize` | Const generic | `ConstParam("N", "usize")` |

**Inventory Representation (JSON):**
```json
{
  "name": "Container",
  "generics": {
    "type_params": [{"name": "T", "bounds": ["Clone"]}],
    "lifetimes": [{"name": "'a"}],
    "const_params": [{"name": "N", "type": "usize"}]
  }
}
```

**Simplification for Analysis:**
- Ignore lifetimes for structural similarity (not relevant to duplication)
- Treat generic params by name only (not bounds)
- `Foo<T: Clone>` and `Foo<T: Debug>` are structurally equivalent

**Edge Cases:**
- Multi-line `where` clauses: Include bounds in representation
- Higher-ranked trait bounds (`for<'a>`): Simplify to "HRTB present" flag

### 3.4 Inventory Output

```markdown
## Model Inventory

### By Crate

#### casparian (main binary)

| Type | Kind | File | Fields | Visibility | Derives |
|------|------|------|--------|------------|---------|
| `SourceConfig` | struct | scout/types.rs:45 | 8 | pub | Debug, Clone, Serialize |
| `ScanResult` | struct | scout/scanner.rs:23 | 5 | pub | Debug |
| `TaggingRule` | struct | scout/tagger.rs:12 | 6 | pub | Debug, Clone |
| `JobStatus` | enum | cli/jobs.rs:8 | 4 | pub | Debug, Clone, PartialEq |

#### casparian_worker

| Type | Kind | File | Fields | Visibility | Derives |
|------|------|------|--------|------------|---------|
| `WorkerConfig` | struct | lib.rs:15 | 5 | pub | Debug, Clone |
| `InferenceResult` | struct | type_inference/mod.rs:30 | 3 | pub | Debug |

### Summary

| Metric | Count |
|--------|-------|
| Total types | 87 |
| Structs | 62 |
| Enums | 18 |
| Type aliases | 7 |
| Public types | 54 |
| Private types | 33 |
```

### 3.5 Relationship Graph

Build a graph of type relationships:

```json
{
  "casparian::scout::SourceConfig": {
    "contains": ["casparian::scout::SourceId", "std::path::PathBuf"],
    "referenced_by": ["casparian::scout::Scanner", "casparian::cli::scan::ScanCommand"],
    "derives": ["Debug", "Clone", "Serialize", "Deserialize"],
    "crate": "casparian",
    "file": "src/scout/types.rs",
    "line": 45
  }
}
```

**Relationship types:**
- `contains`: Fields of this type
- `referenced_by`: Types that have fields of this type
- `implements`: Traits implemented (for context)

---

## 4. Phase 2: Usage Analysis

### 4.1 Usage Categories

For each type, determine:

| Category | Definition | Action |
|----------|------------|--------|
| **ACTIVE** | Used in runtime code paths | None |
| **TEST_ONLY** | Only used in `#[cfg(test)]` | Consider extracting to test utils |
| **DEAD** | No references found | **Remove** |
| **INTERNAL_ONLY** | Used only within defining module | Consider making private |
| **OVER_EXPORTED** | `pub` but only used in crate | Make `pub(crate)` |

### 4.2 Usage Detection Algorithm

Execute these grep patterns in order to classify each type:

**Step 1: Find All References**

For type `TypeName` in crate `crate_name`:

```
# Pattern 1: Direct use imports
Grep: "use .*TypeName"
Result: List of files importing this type

# Pattern 2: Qualified path usage
Grep: "crate_name::.*TypeName"
Result: External crate references

# Pattern 3: General usage (word-boundary aware)
Grep: "\bTypeName\b"
Result: All occurrences (filter out definitions)

# Pattern 4: impl blocks
Grep: "impl.*TypeName"
Result: Has methods or trait implementations

# Pattern 5: Pattern matching
Grep: "TypeName::\w+|TypeName \{"
Result: Enum variant or struct destructuring
```

**Step 2: Classify by Context**

For each reference found, classify by file context:

| Context | Detection Method | Classification |
|---------|------------------|----------------|
| `#[cfg(test)]` block | Check if reference is inside `mod tests` | TEST_ONLY |
| Same module only | All refs in same file as definition | INTERNAL_ONLY |
| Same crate only | Refs only in files under same `crates/xxx/` | OVER_EXPORTED |
| Cross-crate | Refs in different `crates/yyy/` | ACTIVE |
| No references | Zero grep matches (except definition) | DEAD |

**Step 3: Classification Algorithm**

```
function classify_type(type_name, definition_file, crate_name):
    refs = []

    # Search within crate
    internal_refs = Grep("\bTypeName\b", path=f"crates/{crate_name}/src/")

    # Search in other crates
    for other_crate in all_crates:
        if other_crate != crate_name:
            external_refs = Grep("\bTypeName\b", path=f"crates/{other_crate}/src/")
            refs.extend(external_refs)

    # Filter out the definition itself
    refs = [r for r in refs if not is_definition_line(r)]

    # Check for test-only
    test_refs = [r for r in refs if is_in_test_module(r)]
    non_test_refs = [r for r in refs if not is_in_test_module(r)]

    if len(refs) == 0:
        return "DEAD"
    elif len(non_test_refs) == 0:
        return "TEST_ONLY"
    elif all_refs_in_same_file(non_test_refs, definition_file):
        return "INTERNAL_ONLY"
    elif all_refs_in_same_crate(non_test_refs, crate_name) and visibility == "pub":
        return "OVER_EXPORTED"
    else:
        return "ACTIVE"
```

**Step 4: Handling Special Cases**

| Case | Pattern | Classification |
|------|---------|----------------|
| `#[allow(dead_code)]` on **type** | Grep for attribute directly above `struct`/`enum` | ACKNOWLEDGED_DEAD |
| `#[allow(dead_code)]` on **field** | Attribute on field, not type | Type may still be ACTIVE |
| Re-exported type | `pub use types::TypeName` | Count re-export as usage |
| Generic parameter | `Vec<TypeName>` | ACTIVE (used in generic) |
| Derive-generated | `#[derive(Clone)]` | Skip (compiler-generated) |

**IMPORTANT:** Distinguish type-level vs field-level `#[allow(dead_code)]`:
- Type-level: `#[allow(dead_code)] pub struct Foo` → Type itself is acknowledged dead
- Field-level: `#[allow(dead_code)] pub field: T` → Only the field is dead, type may be active
- Comment mentioning "fields" → Developer acknowledging field-level dead code, NOT type-level

#### Trait Object and Impl Trait Detection

Standard grep for `\bTraitName\b` will miss trait objects. Add these patterns for traits:

```
# Pattern 6: dyn trait objects
Grep: "dyn\s+TraitName"
Result: Trait object usage (Box<dyn T>, &dyn T, Arc<dyn T>)

# Pattern 7: impl Trait in return position
Grep: "impl\s+TraitName"
Result: Existential type usage

# Pattern 8: Trait bounds
Grep: ":\s*TraitName\b|:\s*\w+\s*\+\s*TraitName"
Result: Used as constraint
```

**Trait Usage Categories:**

| Usage Type | Pattern | Weight |
|------------|---------|--------|
| Direct impl | `impl TraitName for Type` | HIGH |
| Trait object | `dyn TraitName` | HIGH |
| Generic bound | `T: TraitName` | MEDIUM |
| impl return | `-> impl TraitName` | MEDIUM |

#### Macro-Expanded Usage Detection

Types used inside macro invocations may not be visible to simple grep.

**Strategy:** Standard `\bTypeName\b` catches 80%+ of macro usages (types in `vec![]`, etc.). For framework-specific macros:

| Macro | Detection Pattern |
|-------|------------------|
| `sqlx::query_as!` | `query_as!\s*\(\s*TypeName` |
| `vec![]`, `hashmap!{}` | Standard grep catches these |
| Custom project macros | Add project-specific patterns |

**Classification Adjustment:**

```
if len(non_macro_refs) == 0 and len(macro_refs) > 0:
    return "MACRO_USAGE_ONLY"  # Needs human verification
```

**New Usage Category:**

| Category | Definition | Action |
|----------|------------|--------|
| **MACRO_USAGE_ONLY** | Only found inside macro invocations | Manual review required |

**Note:** Exclude doc comments (`///`, `//!`) from detection to avoid false positives.

**Example Classifications:**

```markdown
# ACTIVE - FileStatus
Definition: crates/casparian/src/scout/types.rs:91
References found: 30+ locations (imports, field types, function signatures)
Classification: ACTIVE

# DEAD - ProcessedEntry (hypothetical)
Definition: crates/casparian/src/scout/types.rs:254
References found: Only definition line
Classification: DEAD

# OVER_EXPORTED - UpsertResult
Definition: crates/casparian/src/scout/types.rs:241
Visibility: pub
References: Only in crates/casparian/src/scout/db.rs (same crate)
Classification: OVER_EXPORTED (pub but only used internally)
```

### 4.3 Usage Report Format

```markdown
## Usage Analysis

### DEAD Types (Immediate Removal Candidates)

| Type | Crate | Last Modified | Recommendation |
|------|-------|---------------|----------------|
| `OldConfig` | casparian | 2025-10-15 | Remove - no usages found |
| `DeprecatedResult` | casparian_worker | 2025-09-01 | Remove - commented "TODO: remove" |

### TEST_ONLY Types (Review Needed)

| Type | Crate | Used In | Recommendation |
|------|-------|---------|----------------|
| `MockSource` | casparian | tests/scanner_tests.rs | OK - test utility |
| `TestConfig` | casparian_worker | tests/*.rs | OK - test utility |

### OVER_EXPORTED Types (Visibility Reduction)

| Type | Crate | Current | Used In | Recommendation |
|------|-------|---------|---------|----------------|
| `InternalState` | casparian | pub | same crate only | Make `pub(crate)` |
| `HelperStruct` | casparian_schema | pub | same module only | Make private |

### ACTIVE Types (62)
[Summary only - no action needed]
```

---

## 5. Phase 3: Cross-Model Analysis

### 5.1 Duplicate Detection

Identify types with similar field sets:

**Detection algorithm:**
```
1. Extract field names and types for each struct
2. Calculate Jaccard similarity: |A ∩ B| / |A ∪ B|
3. Flag pairs with similarity > 70%
```

**Example:**
```rust
// casparian/src/scout/types.rs
pub struct SourceConfig {
    pub id: i64,
    pub path: PathBuf,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

// casparian/src/cli/scan.rs
pub struct ScanConfig {
    pub source_id: i64,       // Same as `id`
    pub source_path: PathBuf, // Same as `path`
    pub source_name: String,  // Same as `name`
    pub scan_time: DateTime<Utc>,
}
```

**Similarity: 75%** (3/4 fields semantically equivalent)

**Output:**
```markdown
### Duplicate Detection

#### DUP-001: SourceConfig <-> ScanConfig (75% similar)

| SourceConfig | ScanConfig | Match |
|--------------|------------|-------|
| id: i64 | source_id: i64 | Semantic |
| path: PathBuf | source_path: PathBuf | Semantic |
| name: String | source_name: String | Semantic |
| created_at: DateTime | scan_time: DateTime | Different purpose |

**Recommendation:** Extract common `SourceIdentity` type
```

### 5.1.1 Advanced Duplicate Detection

#### Same-Name-Different-Crate Handling

When types share the same name across crates, apply these heuristics:

| Signal | Interpretation | Action |
|--------|----------------|--------|
| Visibility differs (pub vs private) | Private is local-only helper | Flag as INTENTIONAL_VARIANT |
| Field count differs by >50% | Different purposes | Flag as INTENTIONAL_VARIANT |
| No cross-crate imports | Not meant to be shared | Flag as INTENTIONAL_VARIANT |
| One imports from the other | Wrapper/adapter pattern | Flag for REVIEW |
| Nearly identical fields | Unintended duplication | Flag as PROBABLE_DUPLICATE |

**Note:** Private types (`struct Foo` without `pub`) are automatically classified as INTENTIONAL_VARIANT for same-name detection.

**Detection Algorithm:**
```
for each pair (TypeA in CrateX, TypeB in CrateY) where name(A) == name(B):
    if visibility(A) == private OR visibility(B) == private:
        classify as INTENTIONAL_VARIANT
    elif visibility(A) != visibility(B):
        classify as INTENTIONAL_VARIANT
    elif field_similarity(A, B) > 0.9:  # Using Jaccard on field NAMES only
        classify as PROBABLE_DUPLICATE
    elif one_imports_other(A, B):
        classify as WRAPPER_PATTERN
    else:
        classify as DISTINCT_TYPES
```

**Example from codebase:**
```
ScannedFile exists in 2 locations:
- casparian::scout::types::ScannedFile (17 fields, pub) → CANONICAL
- casparian::cli::tag::ScannedFile (7 fields, private) → INTENTIONAL_VARIANT
```

#### Generic Type Handling

Generic types require structural comparison:

**Step 1: Extract type structure**
```
Vec<T>           -> Container(Vec, [TypeParam(T)])
HashMap<K, V>    -> Container(HashMap, [TypeParam(K), TypeParam(V)])
Option<Vec<T>>   -> Container(Option, [Container(Vec, [TypeParam(T)])])
```

**Step 2: Compare structures**
- Two `Vec<T>` types are equivalent if T types are equivalent
- `Vec<String>` != `Vec<i64>` (different inner type)
- `Vec<MyStruct>` matches if MyStruct definitions match

**Step 3: Handle type parameters**

| Scenario | Detection | Classification |
|----------|-----------|----------------|
| Same container, same inner type | Exact match | DUPLICATE |
| Same container, different inner type | No match | DISTINCT |
| Same container, user-defined inner type | Compare inner definitions | RECURSIVE_CHECK |

#### Nested Type Analysis

For structs containing other user-defined types, build containment graph and limit depth to `--depth` parameter (default: 3).

**Algorithm:**
1. Build containment graph: `QuickScanResult -> ScannedFile`
2. When analyzing QuickScanResult, also resolve ScannedFile
3. Two "container" types are duplicates if:
   - Their direct fields match (Jaccard > 0.7 on field **names**)
   - Their nested types also match (recursive check, up to depth limit)

#### Jaccard Similarity Clarification

**IMPORTANT:** Jaccard similarity for duplicate detection uses field **names** only, not full type signatures:

```
Jaccard(A, B) = |field_names(A) ∩ field_names(B)| / |field_names(A) ∪ field_names(B)|
```

This allows detecting semantic duplicates even when field types differ slightly (e.g., `i32` vs `i64`).

#### Semantic Duplicate Detection (Different Names)

Types with different names but identical structure represent potential consolidation opportunities.

**Detection Method: Structural Fingerprinting**

```
function compute_structural_fingerprint(type_def):
    fields = extract_fields(type_def)
    sorted_fields = sort(fields, by=field_name)
    fingerprint = ",".join(f"{f.name}:{normalize_type(f.type)}" for f in sorted_fields)
    return hash(fingerprint)
```

**Type Normalization:**

| Original | Normalized | Rationale |
|----------|------------|-----------|
| `i32`, `i64` | `integer` | Numeric precision variance |
| `String`, `&str` | `string` | String representation variance |
| `Vec<T>`, `[T]` | `list<T>` | Collection variance |
| `PathBuf`, `&Path` | `path` | Path representation variance |
| `DateTime<Utc>` | `datetime` | Time type variance |

**Similarity Tiers:**

| Tier | Definition | Example |
|------|------------|---------|
| **EXACT_DUPLICATE** | Same fields, same types | `FileEntry` = `FileRecord` |
| **NORMALIZED_DUPLICATE** | Same fields, normalized types match | `id: i32` vs `id: i64` |
| **STRUCTURAL_DUPLICATE** | Same field count, 90%+ name overlap | Field renames only |

**Exclusions:**
- Private types (module-local helpers are OK to duplicate)
- Test fixtures (test types may mirror production intentionally)
- Types with `#[repr(C)]` or FFI markers (binary compatibility)

### 5.2 Bloat Detection

Identify overly complex types:

| Threshold | Severity | Action |
|-----------|----------|--------|
| **> 8 fields** | MEDIUM | Consider splitting |
| **> 15 fields** | HIGH | Must split |
| **> 5 enum variants with data** | MEDIUM | Consider separate types |
| **> 10 enum variants** | HIGH | Consider splitting |
| **Nested depth > 3** | MEDIUM | Flatten or extract |

**Output:**
```markdown
### Bloat Detection

#### BLOAT-001: JobRunResult (16 fields)

```rust
pub struct JobRunResult {
    pub job_id: Uuid,
    pub parser_id: Uuid,
    pub parser_name: String,
    pub parser_version: String,
    pub input_path: PathBuf,
    pub input_hash: String,
    pub output_path: PathBuf,
    pub output_format: String,
    pub rows_processed: u64,
    pub rows_failed: u64,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub duration_ms: u64,
    pub status: JobStatus,
    pub error_message: Option<String>,
    pub lineage_columns: Vec<String>,
}
```

**Analysis:**
- 16 fields exceeds recommended max of 8
- Natural groupings exist:
  - Parser info: parser_id, parser_name, parser_version
  - Input info: input_path, input_hash
  - Output info: output_path, output_format, rows_processed, rows_failed
  - Timing info: start_time, end_time, duration_ms
  - Status info: status, error_message

**Recommendation:** Split into:
- `ParserInfo` (3 fields)
- `InputInfo` (2 fields)
- `OutputInfo` (4 fields)
- `JobTiming` (3 fields)
- `JobRunResult` (4 fields: job_id, parser: ParserInfo, input: InputInfo, ...)
```

### 5.3 Consolidation Opportunities

Identify types that could share a common base:

```markdown
### Consolidation Opportunities

#### CONSOLIDATE-001: Config types

These types share common patterns:
- `SourceConfig` (casparian::scout)
- `WorkerConfig` (casparian_worker)
- `SentinelConfig` (casparian_sentinel)

Common fields:
- `name: String` (in all 3)
- `enabled: bool` (in 2/3)
- `timeout_ms: u64` (in 2/3)

**Recommendation:** Extract `BaseConfig` trait or struct
```

### 5.4 Crate Boundary Analysis

Identify types in wrong crate:

```markdown
### Crate Boundary Issues

#### BOUNDARY-001: ScoutError in casparian_worker

`casparian_worker::ScoutError` - should live in `casparian::scout`

**Evidence:**
- Name references "Scout"
- Used to wrap scout operations
- Creates coupling from worker → scout

**Recommendation:** Move to `casparian::scout::error`
```

### 5.4.1 Crate Ownership Rules

#### Determining Correct Crate

Use these rules in order of precedence:

| Rule | Priority | Example |
|------|----------|---------|
| **Name prefix match** | 1 | `ScoutError` belongs in `casparian::scout` module |
| **Dependency direction** | 2 | If crate A depends on crate B, B's types shouldn't reference A |
| **Usage concentration** | 3 | Type used 80%+ in one crate should live there |
| **Abstraction level** | 4 | Low-level (protocol) types in protocol crate; high-level (UI) in main |

**Note:** In this codebase, `scout` is a **module** within the `casparian` binary crate, not a separate crate (see Cargo.toml comment: "casparian_scout has been inlined").

#### Ownership Decision Tree

```
Is the type name prefixed with a crate/module name?
├─ YES: Type belongs in that crate/module
│       e.g., "ScoutError" → casparian::scout module
│       e.g., "WorkerError" → casparian_worker crate
│
└─ NO: Check dependency direction
       ├─ Type only references downstream deps? → OK where it is
       │  e.g., Schema in casparian_schema referencing DataType from casparian_protocol
       │
       └─ Type references upstream deps? → WRONG CRATE
          e.g., Type in casparian_protocol referencing casparian_schema → VIOLATION
```

#### Signal-Based Detection

**WRONG_CRATE signals:**

| Signal | Pattern | Example |
|--------|---------|---------|
| Name mismatch | Type name contains other crate's domain | `WorkerConfig` in `casparian_schema` |
| Inverse dependency | Type imports from dependent crate | Protocol type using Schema type |
| Usage imbalance | >90% usage in different crate | Type defined elsewhere but only used in deprecated tooling |
| Re-export chain | `pub use other_crate::Type` spanning 3+ crates | Type defined in A, re-exported through B and C |

**OK_WHERE_IT_IS signals:**

| Signal | Pattern | Example |
|--------|---------|---------|
| Shared protocol type | Used by 3+ crates equally | `DataType` in protocol crate |
| Binary-specific | Only used in main binary | CLI types in casparian |
| Test fixture | Only used in tests | `MockProvider` in crate's test utils |

#### Concrete Rules for This Codebase

| Type Pattern | Correct Location | Rationale |
|--------------|------------------|-----------|
| `*Error` for module X | Same module/crate as X | Errors are part of module API |
| `*Config` for feature X | Same crate as X | Config couples to implementation |
| `*Result` for operation X | Same crate as X | Results are operation outputs |
| Protocol types (`OpCode`, `Message`) | `casparian_protocol` | Shared communication layer |
| Schema types (`LockedSchema`, `Contract`) | `casparian_schema` | Dedicated schema crate |
| Scout types (`ScannedFile`, `Source`) | `casparian::scout` module | Scout is module in main binary |

### 5.5 Naming Consistency

Flag inconsistent naming:

| Pattern | Examples | Issue |
|---------|----------|-------|
| Mixed suffixes | `JobResult`, `ScanResponse`, `ParseOutput` | Should be consistent |
| Abbreviations | `Cfg` vs `Config` | Pick one |
| Crate prefix | `CasparianError` in casparian crate | Redundant |

---

## 6. Phase 4: Recommendations

### 6.1 Recommendation Types

| Type | Priority | Action |
|------|----------|--------|
| **REMOVE** | HIGH | Delete dead type |
| **MERGE** | MEDIUM | Combine duplicate types |
| **SPLIT** | MEDIUM | Break up bloated type |
| **MOVE** | LOW | Relocate to correct crate |
| **RENAME** | LOW | Fix naming inconsistency |
| **REDUCE_VISIBILITY** | LOW | pub → pub(crate) |
| **EXTRACT** | MEDIUM | Extract common base type |
| **DOCUMENT** | LOW | Add missing doc comments |

### 6.2 Recommendation Format

```markdown
## Recommendations

### High Priority (3)

#### REC-001: Remove `OldConfig`
- **Type:** REMOVE
- **Location:** casparian/src/config.rs:45
- **Reason:** DEAD - no usages found
- **Effort:** Trivial
- **Risk:** None
- **Action:** Delete struct and impl blocks

#### REC-002: Merge `SourceConfig` and `ScanConfig`
- **Type:** MERGE
- **Location:** casparian/src/scout/*.rs
- **Reason:** 75% field overlap
- **Effort:** Small
- **Risk:** Low - update 5 call sites
- **Action:**
  1. Create `SourceIdentity` with shared fields
  2. Update both types to contain `SourceIdentity`
  3. Update call sites

### Medium Priority (5)

#### REC-003: Split `JobRunResult`
- **Type:** SPLIT
- **Location:** casparian/src/runner/mod.rs:120
- **Reason:** 16 fields (bloat)
- **Effort:** Medium
- **Risk:** Medium - used in 12 locations
- **Action:**
  1. Extract `ParserInfo`, `InputInfo`, `OutputInfo`, `JobTiming`
  2. Update `JobRunResult` to contain these
  3. Update serialization
  4. Update all usages

### Low Priority (8)
...
```

### 6.3 Effort Estimation

| Effort | Definition |
|--------|------------|
| **Trivial** | Delete unused code, rename |
| **Small** | Change 1-5 call sites |
| **Medium** | Restructure type, change 5-20 call sites |
| **Large** | Major refactor, change 20+ call sites |

---

## 7. Phase 5: Execution

### 7.1 User Approval

Before executing, present summary and get approval.

### 7.2 Execution Order

1. **Removes first** - Clean up dead code
2. **Visibility reductions** - Safe, no behavior change
3. **Extractions** - Create new common types
4. **Merges** - Consolidate after extractions ready
5. **Splits** - Break up bloated types
6. **Moves** - Relocate after structure stabilized
7. **Renames** - Last (cosmetic)

### 7.3 Per-Change Protocol

For each change:
1. **Create branch** - `model-cleanup/REC-001-remove-old-config`
2. **Make change** - Edit files
3. **Run `cargo check`** - Verify compiles
4. **Run `cargo test`** - Verify tests pass
5. **Commit** - With descriptive message
6. **Continue or rollback** - Based on test results

### 7.3.1 Git Branching Strategy

**Note:** This introduces a new branch prefix `model-cleanup/*` which differs from the existing `feat/*` convention. This is intentional - model cleanup is a distinct operation type.

#### Recommended Approach: Single Cleanup Branch with Atomic Commits

```
main
  │
  └── model-cleanup/2026-01-14
        │
        ├── [commit 1] Remove dead type: OldConfig
        │   Files: src/config.rs
        │   Verified: cargo check ✓, cargo test ✓
        │
        ├── [commit 2] Reduce visibility: InternalState pub → pub(crate)
        │   Files: src/state.rs
        │   Verified: cargo check ✓, cargo test ✓
        │
        └── [commit 3] Merge SourceConfig and ScanConfig
            Files: src/scout/types.rs, src/cli/scan.rs (+ 5 more)
            Verified: cargo check ✓, cargo test ✓
```

**Rationale:**
- Single branch is simpler to manage than N branches
- Atomic commits enable precise rollback (`git revert <commit>`)
- Matches observed codebase pattern of thematic branches

#### Commit Message Format

```
model-cleanup: <ACTION> <TypeName>

<Description of change>

Changes:
- <file1>: <what changed>
- <file2>: <what changed>

Verification:
- cargo check: PASS
- cargo test: PASS
- Tests affected: <list or "none">

Recommendation-ID: REC-XXX
```

#### Rollback Protocol

**Scenario 1: Commit fails verification (before push)**
```bash
# Revert uncommitted changes
git checkout -- .

# Log failure
echo "REC-XXX: Failed - <reason>" >> execution_log.md

# Continue to next recommendation
```

**Scenario 2: Later commit depends on failed commit**
```bash
# Skip dependent commits
# Log all skipped recommendations
echo "REC-YYY: Skipped - depends on failed REC-XXX" >> execution_log.md
```

**Scenario 3: Need to undo after push (discovered issue later)**
```bash
# Create revert commit
git revert <commit-sha>

# Update execution_log.md
echo "REC-XXX: Reverted - <reason>" >> execution_log.md
```

**Scenario 4: Complete rollback of entire cleanup**
```bash
# If branch not merged: delete branch
git branch -D model-cleanup/2026-01-14

# If branch merged: revert all commits (reverse order)
git revert HEAD~4..HEAD  # Revert last 4 commits
```

**Scenario 5: Unrelated test failure (flaky test or pre-existing regression)**
```bash
# Identify if failure is related to change
# If tests failing are NOT in changed files:
echo "REC-XXX: Completed with warning - unrelated test failure in <test_name>" >> execution_log.md

# Continue to next recommendation (don't block on unrelated failures)
# Log for follow-up investigation
```

#### When to Use Branch-Per-Change

Only use separate branches when:
1. Change is experimental and may not be approved
2. Change requires extended review period
3. Change depends on external factors (upstream crate update)

### 7.4 Rollback Strategy

If change breaks compilation:
1. Revert uncommitted changes
2. Log failure reason
3. Skip recommendation, continue to next
4. Report skipped recommendations in summary

### 7.5 Final Verification

After ALL recommendations are executed, perform comprehensive verification:

#### 7.5.1 Build Verification

```bash
# Full release build (catches optimization-only issues)
cargo build --release

# Check all targets compile
cargo check --all-targets
```

#### 7.5.2 Test Suite

```bash
# Run full test suite
cargo test

# Run integration/E2E tests if present
cargo test --test '*'

# Run specific crate tests for modified crates
cargo test -p <modified_crate_1> -p <modified_crate_2>
```

#### 7.5.3 Verification Checklist

| Check | Command | Required |
|-------|---------|----------|
| Compilation | `cargo check` | MUST PASS |
| Release build | `cargo build --release` | MUST PASS |
| Unit tests | `cargo test --lib` | MUST PASS |
| Integration tests | `cargo test --test '*'` | SHOULD PASS |
| Clippy | `cargo clippy` | SHOULD PASS (warnings OK) |
| Doc tests | `cargo test --doc` | SHOULD PASS |

#### 7.5.4 Verification Outcomes

| Result | Action |
|--------|--------|
| All MUST checks pass | Proceed to commit/PR |
| MUST check fails | Rollback changes, investigate |
| SHOULD check fails (unrelated) | Document in execution_log.md, proceed |
| SHOULD check fails (related) | Fix issue or rollback specific change |

#### 7.5.5 Documentation Update

After successful verification, update execution_log.md:

```markdown
## Final Verification

**Date:** YYYY-MM-DD
**Verified By:** LLM/User

### Build Results
- cargo check: PASS
- cargo build --release: PASS
- cargo test: PASS (X passed, Y skipped)

### Summary
- Recommendations executed: N
- Recommendations skipped: M
- Total types removed: X
- Total types modified: Y
- Lines removed: ~Z
```

---

## 8. Error Handling

### 8.1 Error Types

| Error | Severity | Action |
|-------|----------|--------|
| **PARSE_ERROR** | LOW | Skip file, log warning |
| **USAGE_AMBIGUOUS** | MEDIUM | Mark as NEEDS_REVIEW |
| **CIRCULAR_DEPENDENCY** | HIGH | Report cycle, skip affected types |
| **COMPILE_FAILURE** | HIGH | Rollback change |

### 8.2 Graceful Degradation

One unparseable file should not block analysis of others.

```markdown
### Parse Errors

| File | Error | Impact |
|------|-------|--------|
| src/legacy.rs | Syntax error line 45 | Skipped 3 types |
| src/generated.rs | Macro expansion failed | Skipped 1 type |

**Inventory Completion:** 95% (82/87 types analyzed)
```

---

## 9. Decision Checkpoints

### 9.1 Mandatory User Confirmation

| Trigger | Action |
|---------|--------|
| REMOVE any type | Show usages (should be 0), confirm |
| MERGE types | Show both types, confirm target |
| SPLIT type | Show proposed structure, confirm |
| MOVE across crates | Show dependency impact, confirm |

### 9.2 Auto-Approve (No Confirmation)

| Action | Reason |
|--------|--------|
| REDUCE_VISIBILITY | Safe, doesn't break external API |
| DOCUMENT | Adding docs never breaks code |
| RENAME (private types) | No external impact |

---

## 10. Output Artifacts

### 10.1 Session Folder Structure

```
specs/meta/maintenance/data-models/
├── YYYY-MM-DD/
│   ├── model_inventory.md        # Phase 1 output
│   ├── model_graph.json          # Phase 1 output
│   ├── usage_report.md           # Phase 2 output
│   ├── cross_model_report.md     # Phase 3 output
│   ├── recommendations.md        # Phase 4 output
│   ├── execution_log.md          # Phase 5 output
│   ├── summary.md                # Final summary
│   └── decisions.md              # User decisions
```

### 10.2 Summary Format

```markdown
## Data Model Maintenance Summary - 2026-01-14

### Model Health

| Metric | Before | After |
|--------|--------|-------|
| Total types | 87 | 82 |
| Dead types | 5 | 0 |
| Bloated types | 3 | 0 |
| Over-exported | 8 | 2 |
| Duplicate pairs | 2 | 0 |

### Actions Taken

| Action | Count |
|--------|-------|
| Removed | 5 |
| Merged | 2 |
| Split | 2 |
| Visibility reduced | 6 |

### Remaining Issues

- `JobRunResult` split deferred (large refactor)
- `ConfigTrait` extraction needs design

### Next Maintenance

Recommended: 2026-04-14 (quarterly)
```

---

## 11. Metrics

### 11.1 Model Health Score

```
health_score = (
    (1 - dead_types / total_types) * 0.3 +
    (1 - bloated_types / total_types) * 0.2 +
    (1 - duplicate_pairs / possible_pairs) * 0.2 +
    (documented_types / total_types) * 0.15 +
    (correctly_scoped / total_types) * 0.15
) * 100
```

### 11.2 Tracking Over Time

```markdown
## Health History

| Date | Score | Types | Dead | Bloated | Actions |
|------|-------|-------|------|---------|---------|
| 2026-01-14 | 92% | 82 | 0 | 0 | Full maintenance |
| 2025-10-15 | 78% | 87 | 5 | 3 | Initial audit |
```

---

## 12. Integration with Development

### 12.1 Pre-Commit Check (Future)

```bash
# Conceptual - could be a lint
cargo casparian model-check

Model Health: 85%
- 2 new types without documentation
- 1 type with >10 fields

Continue with commit? (y/n)
```

### 12.2 CI Integration (Future)

```yaml
# In CI pipeline
- name: Model health check
  run: cargo casparian model-audit --fail-on-dead
```

---

## 13. Comparison to Spec Maintenance

| Aspect | Spec Maintenance | Data Model Maintenance |
|--------|------------------|------------------------|
| **Subject** | Markdown documents | Rust types |
| **Phase 2** | Code alignment | Usage analysis |
| **Overlap** | Text similarity | Field similarity |
| **Bloat** | Line count | Field count |
| **Dead** | Orphan (no refs) | Dead (no usages) |
| **Output** | Updated specs | Refactored code |
| **Validation** | Manual review | `cargo check` + `cargo test` |

---

## 14. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-14 | 1.0 | Initial specification |
| 2026-01-14 | 1.1 | Spec refinement Round 1: Added Section 3.2.1 (Type Extraction Method with concrete grep patterns); Expanded Section 4.2 (Usage Detection Algorithm with classification logic); Added `#[allow(dead_code)]` disambiguation rules |
| 2026-01-14 | 1.2 | Spec refinement Round 2: Added Section 1.4 (Audit Scoping with parameters, incremental support); Added Section 5.1.1 (Advanced Duplicate Detection with same-name handling, generic types, nested analysis); Added Section 5.4.1 (Crate Ownership Rules with decision tree); Added Section 7.3.1 (Git Branching Strategy with rollback protocol) |
| 2026-01-14 | 1.3 | Spec refinement Round 3: Added macro-generated type detection (Section 3.2.1); Added generic parameter extraction (Section 3.3.1); Added trait object and impl trait detection (Section 4.2); Added macro-expanded usage detection with MACRO_USAGE_ONLY category; Added semantic duplicate detection with structural fingerprinting; Expanded merge conflict resolution algorithm |
