# Casparian CLI - Parallel Execution Plan

**Goal:** Build the CLI with parallel Claude agents, each owning specific commands.

**Philosophy:** W1 creates the skeleton (all commands as stubs). Other workers implement their commands without touching shared files.

---

## COMPACTION-SAFE ORCHESTRATION

**CRITICAL:** This plan uses parallel agents that may take longer than a single conversation context.
To survive conversation compaction, the orchestrator MUST:

1. **Before spawning workers:** Create `ORCHESTRATION_CHECKPOINT.md` with initial state
2. **After each phase:** Update the checkpoint file with progress
3. **On conversation resume:** Read checkpoint to determine current phase
4. **On completion:** Delete or mark checkpoint as COMPLETED

Checkpoint file location: `ORCHESTRATION_CHECKPOINT.md` in repo root.

Template:
```markdown
# Orchestration Checkpoint
plan: CLI_PARALLEL_PLAN
status: IN_PROGRESS
current_phase: PHASE_2_SPAWN_WORKERS
workers:
  W1: { status: RUNNING, branch: feat/cli-core }
  ...
next_action: Poll TaskOutput for W1-W5 completion, then proceed to PHASE_3_MERGE
```

---

## ORCHESTRATOR PROTOCOL (Main Claude)

### Phase 1: Setup
```bash
cd /Users/shan/workspace/casparianflow
git worktree add ../cf-cli-w1 -b feat/cli-core
git worktree add ../cf-cli-w2 -b feat/cli-tag
git worktree add ../cf-cli-w3 -b feat/cli-parser
git worktree add ../cf-cli-w4 -b feat/cli-jobs
git worktree add ../cf-cli-w5 -b feat/cli-resources
```

### Phase 2: Spawn Workers
Spawn workers with `run_in_background: true`, `subagent_type: "general-purpose"`.

**W1 MUST complete before spawning W2-W5** (they depend on the skeleton).

### Phase 3: Merge Order
```bash
# 1. W1 first - creates skeleton
git merge feat/cli-core

# 2-5. Others can merge in any order (no file conflicts)
git merge feat/cli-tag
git merge feat/cli-parser
git merge feat/cli-jobs
git merge feat/cli-resources

# 6. Final verification
cargo build -p casparian
cargo test -p casparian
```

---

## File Structure

```
crates/casparian/src/
├── main.rs                 # W1 ONLY - command enum + dispatch
├── cli/
│   ├── mod.rs              # W1 ONLY - exports
│   ├── error.rs            # W1 ONLY - HelpfulError type
│   ├── output.rs           # W1 ONLY - table formatting, colors
│   │
│   ├── scan.rs             # W1 - scan command implementation
│   ├── tag.rs              # W2 - tag command
│   ├── files.rs            # W2 - files command
│   ├── parser.rs           # W3 - parser subcommands
│   ├── jobs.rs             # W4 - jobs listing
│   ├── job.rs              # W4 - job show/retry/cancel
│   ├── worker.rs           # W4 - worker status/start/stop
│   ├── source.rs           # W5 - source add/rm/ls
│   ├── rule.rs             # W5 - rule add/rm/ls
│   └── topic.rs            # W5 - topic ls/show/create/rm
```

---

## File Ownership Matrix

| File | W1 | W2 | W3 | W4 | W5 | Notes |
|------|----|----|----|----|-----|-------|
| main.rs | PRIMARY | - | - | - | - | Command enum, dispatch |
| cli/mod.rs | PRIMARY | - | - | - | - | Module exports |
| cli/error.rs | PRIMARY | - | - | - | - | HelpfulError |
| cli/output.rs | PRIMARY | - | - | - | - | Tables, formatting |
| cli/scan.rs | PRIMARY | - | - | - | - | Scan implementation |
| cli/tag.rs | - | PRIMARY | - | - | - | Tag implementation |
| cli/files.rs | - | PRIMARY | - | - | - | Files implementation |
| cli/parser.rs | - | - | PRIMARY | - | - | Parser subcommands |
| cli/jobs.rs | - | - | - | PRIMARY | - | Jobs listing |
| cli/job.rs | - | - | - | PRIMARY | - | Job operations |
| cli/worker.rs | - | - | - | PRIMARY | - | Worker control |
| cli/source.rs | - | - | - | - | PRIMARY | Source CRUD |
| cli/rule.rs | - | - | - | - | PRIMARY | Rule CRUD |
| cli/topic.rs | - | - | - | - | PRIMARY | Topic CRUD |

**Zero file conflicts by design.**

---

## W1: Core + Scan (MUST RUN FIRST)

**Branch:** `feat/cli-core`
**Directory:** `../cf-cli-w1`

### Deliverables

1. **main.rs changes:**
   - Add `mod cli;`
   - Add full `Commands` enum with ALL commands (including placeholders)
   - Dispatch to `cli::*::execute()` functions

2. **cli/mod.rs:**
   - Export all submodules
   - Common types

3. **cli/error.rs:**
   - `HelpfulError` struct with suggestions
   - Error display formatting

4. **cli/output.rs:**
   - `print_table()` using comfy-table
   - `format_size()`, `format_time()`
   - Color support (detect TTY)

5. **cli/scan.rs:**
   - Full `scan` command implementation
   - `--status` flag for last scan info
   - `--dry-run` flag

### Command Skeleton in main.rs

```rust
#[derive(Subcommand, Debug)]
enum Commands {
    // === W1: Core ===
    /// Discover files in sources (metadata to SQLite)
    Scan {
        #[arg(long)]
        status: bool,
        #[arg(long)]
        dry_run: bool,
    },

    // === W2: Tagging ===
    /// Apply rules to tag files, or manually tag a file
    Tag {
        /// Manual tag: file path
        path: Option<PathBuf>,
        /// Manual tag: topic name
        topic: Option<String>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        no_queue: bool,
    },
    /// Remove tag from file
    Untag {
        path: PathBuf,
    },
    /// List files with filters
    Files {
        #[arg(long)]
        topic: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        untagged: bool,
        #[arg(long, default_value = "50")]
        limit: usize,
    },

    // === W3: Parser ===
    /// Parser development commands
    Parser {
        #[command(subcommand)]
        action: ParserAction,
    },

    // === W4: Jobs ===
    /// List jobs in queue
    Jobs {
        #[arg(long)]
        topic: Option<String>,
        #[arg(long)]
        pending: bool,
        #[arg(long)]
        running: bool,
        #[arg(long)]
        failed: bool,
        #[arg(long)]
        done: bool,
        #[arg(long, default_value = "50")]
        limit: usize,
    },
    /// Job operations
    Job {
        #[command(subcommand)]
        action: JobAction,
    },
    /// Worker control
    Worker {
        #[command(subcommand)]
        action: WorkerAction,
    },

    // === W5: Resources ===
    /// Source folder management
    Source {
        #[command(subcommand)]
        action: SourceAction,
    },
    /// Tagging rule management
    Rule {
        #[command(subcommand)]
        action: RuleAction,
    },
    /// Topic management
    Topic {
        #[command(subcommand)]
        action: TopicAction,
    },

    // === Existing commands (keep these) ===
    Start { ... },
    Sentinel { ... },
    Worker { ... },
    // etc.
}

#[derive(Subcommand, Debug)]
enum ParserAction {
    Ls,
    Show { name: String },
    Test { file: PathBuf, #[arg(long)] input: PathBuf },
    Publish { file: PathBuf, #[arg(long)] topic: String },
    Unpublish { name: String },
    Backtest { name: String, #[arg(long)] limit: Option<usize> },
}

#[derive(Subcommand, Debug)]
enum JobAction {
    Show { id: i64 },
    Retry {
        id: Option<i64>,
        #[arg(long)]
        all_failed: bool,
        #[arg(long)]
        topic: Option<String>,
    },
    Cancel { id: i64 },
}

#[derive(Subcommand, Debug)]
enum WorkerAction {
    Status,
    Start { #[arg(long)] daemon: bool },
    Stop,
    Restart,
}

#[derive(Subcommand, Debug)]
enum SourceAction {
    Add { path: PathBuf, #[arg(long)] name: Option<String> },
    Rm { path_or_name: String },
    Ls,
}

#[derive(Subcommand, Debug)]
enum RuleAction {
    Add { pattern: String, #[arg(long)] topic: String, #[arg(long)] priority: Option<i32> },
    Rm { pattern: String },
    Ls,
}

#[derive(Subcommand, Debug)]
enum TopicAction {
    Ls,
    Show { topic: String },
    Create { topic: String },
    Rm { topic: String },
}
```

### Dispatch in main.rs

```rust
fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Scan { status, dry_run } => cli::scan::execute(status, dry_run),
        Commands::Tag { path, topic, dry_run, no_queue } => cli::tag::execute(path, topic, dry_run, no_queue),
        Commands::Untag { path } => cli::tag::execute_untag(path),
        Commands::Files { topic, status, untagged, limit } => cli::files::execute(topic, status, untagged, limit),
        Commands::Parser { action } => cli::parser::execute(action),
        Commands::Jobs { topic, pending, running, failed, done, limit } => cli::jobs::execute(topic, pending, running, failed, done, limit),
        Commands::Job { action } => cli::job::execute(action),
        Commands::Worker { action } => cli::worker::execute(action),
        Commands::Source { action } => cli::source::execute(action),
        Commands::Rule { action } => cli::rule::execute(action),
        Commands::Topic { action } => cli::topic::execute(action),

        // Existing commands unchanged
        Commands::Start { .. } => run_unified(...),
        // etc.
    }
}
```

### Stub Pattern for Unimplemented Commands

W1 creates stubs for commands other workers will implement:

```rust
// cli/tag.rs (created by W1 as stub)
pub fn execute(path: Option<PathBuf>, topic: Option<String>, dry_run: bool, no_queue: bool) -> Result<()> {
    todo!("W2 implements this")
}

pub fn execute_untag(path: PathBuf) -> Result<()> {
    todo!("W2 implements this")
}
```

W2 then replaces the `todo!()` with actual implementation.

### Done When (W1)
- `cargo check -p casparian` passes
- `casparian scan --help` shows correct help text
- `casparian scan` actually runs (discovers files)
- `casparian tag` compiles but panics with "W2 implements this"
- All other commands compile but panic with "W# implements this"

---

## W2: Tagging + Files

**Branch:** `feat/cli-tag`
**Directory:** `../cf-cli-w2`
**Depends on:** W1 complete

### Files to Modify (ONLY these)
- `cli/tag.rs` - replace stubs with implementation
- `cli/files.rs` - replace stubs with implementation

### Deliverables

**cli/tag.rs:**
```rust
pub fn execute(path: Option<PathBuf>, topic: Option<String>, dry_run: bool, no_queue: bool) -> Result<()> {
    match (path, topic) {
        // Manual tag: casparian tag /path/to/file sales
        (Some(p), Some(t)) => tag_file_manually(&p, &t),

        // Apply rules: casparian tag [--dry-run] [--no-queue]
        (None, None) => apply_rules(dry_run, no_queue),

        _ => Err(HelpfulError::new("Invalid arguments")
            .with_suggestion("casparian tag                    # Apply rules")
            .with_suggestion("casparian tag <path> <topic>     # Manual tag")
            .into())
    }
}

fn apply_rules(dry_run: bool, no_queue: bool) -> Result<()> {
    // 1. Load rules from config/database
    // 2. Query untagged files
    // 3. Apply glob patterns
    // 4. Update tags in database
    // 5. Queue for processing (unless --no-queue)
    // 6. Print summary
}

fn tag_file_manually(path: &Path, topic: &str) -> Result<()> {
    // 1. Verify file exists in scout_files
    // 2. Update tag
    // 3. Queue for processing
}
```

**cli/files.rs:**
```rust
pub fn execute(topic: Option<String>, status: Option<String>, untagged: bool, limit: usize) -> Result<()> {
    // 1. Build SQL query based on filters
    // 2. Query scout_files table
    // 3. Format as table with columns: PATH, SIZE, TOPIC, STATUS, ERROR
    // 4. Print summary count
}
```

### Output Examples

**`casparian tag --dry-run`:**
```
DRY RUN - No changes

Applying rules to 1,481 files...

WOULD TAG:
  sales/**/*.csv     →  sales       823 files
  invoices/*.json    →  invoices    234 files

WOULD QUEUE: 15 files (12 new + 3 changed)
UNTAGGED: 145 files

Run without --dry-run to apply.
```

**`casparian files --topic sales --status failed`:**
```
PATH                                SIZE      STATUS    ERROR
/data/sales/corrupt.csv             1.2 MB    failed    Row 15: invalid date
/data/sales/weird.csv               890 KB    failed    Schema mismatch

2 files
```

### Done When (W2)
- `cargo check -p casparian` passes
- `casparian tag --dry-run` shows preview
- `casparian tag` applies rules and queues files
- `casparian tag /path/file sales` manually tags
- `casparian files --untagged` lists untagged files
- `casparian files --topic sales --status failed` filters correctly

---

## W3: Parser Workflow

**Branch:** `feat/cli-parser`
**Directory:** `../cf-cli-w3`
**Depends on:** W1 complete

### Files to Modify (ONLY these)
- `cli/parser.rs` - replace stubs with implementation

### Deliverables

```rust
pub fn execute(action: ParserAction) -> Result<()> {
    match action {
        ParserAction::Ls => list_parsers(),
        ParserAction::Show { name } => show_parser(&name),
        ParserAction::Test { file, input } => test_parser(&file, &input),
        ParserAction::Publish { file, topic } => publish_parser(&file, &topic),
        ParserAction::Unpublish { name } => unpublish_parser(&name),
        ParserAction::Backtest { name, limit } => backtest_parser(&name, limit),
    }
}
```

**Key: backtest_parser()** - This is the most complex:
1. Get parser from database
2. Get all files for parser's topic
3. Run parser against each file (with limit)
4. Collect failures, categorize by error type
5. Detect schema variants
6. Print rich failure analysis
7. Suggest next actions

### Backtest Output

```
Testing sales.py against 823 files...

[████████████████████████████████████████] 823/823

RESULTS
  ✓ Passed:   743 files (90.3%)
  ✗ Failed:   80 files

FAILURE ANALYSIS

[SCHEMA MISMATCH] 62 files
  Missing column: 'tax'
  Pattern: sales/2023/**/*.csv

  Options:
    A) Re-tag: casparian rule add "sales/2023/**" --topic sales_v1
    B) Fix parser to handle missing column

[INVALID DATA] 15 files
  Cannot parse 'N/A' as float at column 'amount'

  Sample:
    sales/2024/march_03.csv  line 47  amount='N/A'

SCHEMA VARIANTS DETECTED
  Schema A (743 files): date, product, amount, tax
  Schema B (62 files): date, product, amount

SUGGESTED WORKFLOW
  1. Fix parser to handle 'N/A' values
  2. Re-tag 2023 files as sales_v1
  3. Re-run backtest
```

### Done When (W3)
- `cargo check -p casparian` passes
- `casparian parser ls` lists parsers
- `casparian parser test sales.py --input sample.csv` runs test
- `casparian parser publish sales.py --topic sales` deploys
- `casparian parser backtest sales` runs against all files with rich output

---

## W4: Jobs + Worker

**Branch:** `feat/cli-jobs`
**Directory:** `../cf-cli-w4`
**Depends on:** W1 complete

### Files to Modify (ONLY these)
- `cli/jobs.rs`
- `cli/job.rs`
- `cli/worker.rs`

### Deliverables

**cli/jobs.rs:**
```rust
pub fn execute(topic: Option<String>, pending: bool, running: bool, failed: bool, done: bool, limit: usize) -> Result<()> {
    // 1. Build filter from flags
    // 2. Query cf_processing_queue
    // 3. Print summary counts
    // 4. Print job table
}
```

**cli/job.rs:**
```rust
pub fn execute(action: JobAction) -> Result<()> {
    match action {
        JobAction::Show { id } => show_job(id),
        JobAction::Retry { id, all_failed, topic } => retry_jobs(id, all_failed, topic),
        JobAction::Cancel { id } => cancel_job(id),
    }
}

fn show_job(id: i64) -> Result<()> {
    // 1. Query job
    // 2. Query failure details if failed
    // 3. Print full info with logs
}

fn retry_jobs(id: Option<i64>, all_failed: bool, topic: Option<String>) -> Result<()> {
    // 1. Find jobs to retry
    // 2. Reset status to QUEUED
    // 3. Print count
}
```

**cli/worker.rs:**
```rust
pub fn execute(action: WorkerAction) -> Result<()> {
    match action {
        WorkerAction::Status => show_status(),
        WorkerAction::Start { daemon } => start_worker(daemon),
        WorkerAction::Stop => stop_worker(),
        WorkerAction::Restart => { stop_worker()?; start_worker(false) }
    }
}

fn show_status() -> Result<()> {
    // 1. Check if worker process is running (pid file)
    // 2. Query processing stats from database
    // 3. Print status
}
```

### Done When (W4)
- `cargo check -p casparian` passes
- `casparian jobs` shows queue status
- `casparian jobs --failed` filters to failures
- `casparian job show 123` shows full details
- `casparian job retry 123` resets job
- `casparian job retry --all-failed` retries all
- `casparian worker status` shows worker state
- `casparian worker start` launches worker

---

## W5: Resource Management

**Branch:** `feat/cli-resources`
**Directory:** `../cf-cli-w5`
**Depends on:** W1 complete

### Files to Modify (ONLY these)
- `cli/source.rs`
- `cli/rule.rs`
- `cli/topic.rs`

### Deliverables

**cli/source.rs:**
```rust
pub fn execute(action: SourceAction) -> Result<()> {
    match action {
        SourceAction::Add { path, name } => add_source(&path, name.as_deref()),
        SourceAction::Rm { path_or_name } => remove_source(&path_or_name),
        SourceAction::Ls => list_sources(),
    }
}
```

**cli/rule.rs:**
```rust
pub fn execute(action: RuleAction) -> Result<()> {
    match action {
        RuleAction::Add { pattern, topic, priority } => add_rule(&pattern, &topic, priority),
        RuleAction::Rm { pattern } => remove_rule(&pattern),
        RuleAction::Ls => list_rules(),
    }
}
```

**cli/topic.rs:**
```rust
pub fn execute(action: TopicAction) -> Result<()> {
    match action {
        TopicAction::Ls => list_topics(),
        TopicAction::Show { topic } => show_topic(&topic),
        TopicAction::Create { topic } => create_topic(&topic),
        TopicAction::Rm { topic } => remove_topic(&topic),
    }
}

fn show_topic(topic: &str) -> Result<()> {
    // 1. Query rules that produce this topic
    // 2. Query files with this topic
    // 3. Query parser subscribed to topic
    // 4. Query jobs for this topic
    // 5. Print comprehensive view
}
```

### Topic Show Output

```
TOPIC: sales

RULES
  sales/**/*.csv     823 files matched

PARSER
  sales_parser v1.2
  Published 2024-01-10

FILES
  Total:      823
  Processed:  812
  Pending:    8
  Failed:     3

OUTPUT
  ~/.casparian_flow/output/sales/
  812 parquet files (2.4 GB)
```

### Done When (W5)
- `cargo check -p casparian` passes
- `casparian source add /data/sales` adds source
- `casparian source ls` lists sources
- `casparian rule add "*.csv" --topic csv_data` adds rule
- `casparian rule ls` lists rules
- `casparian topic ls` lists topics with stats
- `casparian topic show sales` shows comprehensive view

---

## Dependencies to Add

W1 adds these to `crates/casparian/Cargo.toml`:

```toml
# CLI output formatting
comfy-table = "7"

# Already have: clap, serde_json, tokio, etc.
```

---

## Integration Points with Existing Code

### Database Access

All commands need database access. Use existing pattern from `main.rs`:

```rust
fn get_db_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".casparian_flow")
        .join("casparian_flow.sqlite3")
}

fn open_db() -> Result<rusqlite::Connection> {
    let path = get_db_path();
    Connection::open(&path).map_err(|e| HelpfulError::new(format!("Cannot open database: {}", e)).into())
}
```

### Reuse Existing Functions

- **Scan:** Reuse `casparian_scout::Scanner` (already exists)
- **Tag:** Query `scout_tagging_rules`, update `scout_files`
- **Parser test:** Reuse validation logic from `ui/src-tauri/src/scout.rs`
- **Jobs:** Query `cf_processing_queue`
- **Worker:** Spawn using existing `run_unified()` logic

### Config File

Commands should read from `~/.casparian_flow/casparian.toml`:

```toml
[sources]
paths = ["/data/sales", "/data/invoices"]

[rules]
"*.csv" = "csv_data"
"sales/**/*.csv" = "sales"
```

W1 creates config loading, others use it.

---

## Execution Timeline

```
Day 1:
  [Sequential] W1 creates skeleton + scan
  [Wait for W1]

Day 1-2:
  [Parallel] W2, W3, W4, W5 implement their commands

Day 2:
  [Sequential] Merge all branches
  [Sequential] Integration testing
  [Sequential] Fix issues
```

---

## Validation Checklist

### After W1 Merge
```bash
cargo build -p casparian
./target/debug/casparian scan --help
./target/debug/casparian scan                  # Should work
./target/debug/casparian tag                   # Should panic "W2 implements"
./target/debug/casparian parser ls             # Should panic "W3 implements"
```

### After All Merges
```bash
cargo build -p casparian
cargo test -p casparian

# Full workflow test
./target/debug/casparian source add /tmp/test_data
./target/debug/casparian rule add "*.csv" --topic test
./target/debug/casparian scan
./target/debug/casparian tag --dry-run
./target/debug/casparian tag
./target/debug/casparian files --topic test
./target/debug/casparian topic show test
./target/debug/casparian parser test sample.py --input /tmp/test_data/sample.csv
./target/debug/casparian parser publish sample.py --topic test
./target/debug/casparian parser backtest sample --limit 10
./target/debug/casparian jobs
./target/debug/casparian worker start --daemon
./target/debug/casparian worker status
./target/debug/casparian worker stop
```

---

## Failure Handling

**Worker fails validation:**
1. Read error
2. If type error in their file: they fix
3. If import error from W1: W1 needs to fix first

**Merge conflict:**
Should not happen - each worker owns different files.

**Integration test fails:**
Orchestrator debugs and fixes directly.

---

## Success Criteria

After all merges:

1. **Scan works:** `casparian scan` discovers files
2. **Tag works:** `casparian tag` applies rules
3. **Parser workflow:** test → publish → backtest cycle works
4. **Jobs visible:** `casparian jobs` shows queue
5. **Full workflow:** scan → tag → process → check results

---

## Notes

- W1 is blocking - must complete before others start
- No shared state between workers - each owns their files
- Workers should read existing code patterns before writing
- Keep it simple - direct code, no abstractions
- Test against actual database with real data
