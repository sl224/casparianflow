//! Headless runner for TUI flows.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use chrono::Utc;
use clap::{Args, Subcommand};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::Serialize;
use tempfile::TempDir;

use crate::cli::config::active_db_path;
use crate::cli::context;
use crate::cli::tui::app::App;
use crate::cli::tui::flow::{FlowEnv, FlowStep, TerminalSize, TuiFlow};
use crate::cli::tui::flow_record::RecordRedaction;
use crate::cli::tui::flow_assert::{assert_flow, FlowAssertError};
use crate::cli::tui::snapshot::{
    buffer_to_bg_mask, buffer_to_plain_text, layout_tree, normalize_for_snapshot,
    render_app_to_buffer, LayoutNode,
};
use crate::cli::tui::TuiArgs;
use casparian::scout::{Database, Source, SourceId, SourceType};

const DEFAULT_WIDTH: u16 = 120;
const DEFAULT_HEIGHT: u16 = 40;
const WAIT_TICK_MS: u64 = 50;

#[derive(Debug, Subcommand)]
pub enum TuiFlowCommand {
    Run(TuiFlowRunArgs),
}

#[derive(Debug, Args)]
pub struct TuiFlowRunArgs {
    /// Path to the flow JSON file
    pub flow: PathBuf,
    /// Output directory for flow artifacts
    #[arg(long, default_value = ".test_output/tui_flows")]
    pub out: PathBuf,
    /// Run in headless mode (required for now)
    #[arg(long, default_value_t = false)]
    pub headless: bool,
}

pub fn run(command: TuiFlowCommand) -> Result<()> {
    match command {
        TuiFlowCommand::Run(args) => run_flow(args),
    }
}

#[derive(Debug, Serialize)]
struct FlowRunMeta {
    flow: String,
    started_at: String,
    casparian_home: String,
    terminal: String,
    steps: usize,
    fixture: Option<FixtureInfo>,
}

#[derive(Debug, Serialize, Clone)]
struct FixtureInfo {
    name: String,
    path: String,
}

#[derive(Debug, Serialize)]
struct StepMeta {
    index: usize,
    kind: String,
    label: Option<String>,
}

#[derive(Debug, Serialize)]
struct FlowFailure {
    step_index: usize,
    step_kind: String,
    label: Option<String>,
    error: String,
    failures: Vec<String>,
}

struct FlowRunner {
    app: App,
    run_dir: PathBuf,
    casparian_home: PathBuf,
    flow_dir: PathBuf,
    fixture: Option<FixtureInfo>,
    terminal: TerminalSize,
    _temp_home: Option<TempDir>,
}

struct FlowCapture {
    plain: String,
    mask: String,
    layout: Vec<LayoutNode>,
    layout_signature: String,
}

fn run_flow(args: TuiFlowRunArgs) -> Result<()> {
    if !args.headless {
        bail!("tui-flow run currently supports only --headless mode");
    }

    let flow_data = fs::read_to_string(&args.flow)
        .with_context(|| format!("read flow file {}", args.flow.display()))?;
    let flow: TuiFlow = serde_json::from_str(&flow_data)
        .with_context(|| format!("parse flow file {}", args.flow.display()))?;
    flow.validate()?;

    let flow_name = args
        .flow
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("flow")
        .to_string();
    let run_dir = create_run_dir(&args.out, &flow_name)?;

    let flow_dir = args
        .flow
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();

    let (casparian_home, temp_home) = prepare_casparian_home(&flow.env)?;
    fs::create_dir_all(&casparian_home).with_context(|| {
        format!(
            "create CASPARIAN_HOME directory {}",
            casparian_home.display()
        )
    })?;
    std::env::set_var("CASPARIAN_HOME", &casparian_home);

    let fixture = prepare_fixture(&flow.env, &casparian_home)?;

    let terminal = flow
        .env
        .terminal
        .clone()
        .unwrap_or(TerminalSize {
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
        });

    let tui_args = TuiArgs {
        database: flow.env.database.clone(),
        record_flow: None,
        record_redaction: RecordRedaction::Plaintext,
        record_checkpoint_every: None,
    };
    let app = App::new(tui_args, None);

    let mut runner = FlowRunner {
        app,
        run_dir: run_dir.clone(),
        casparian_home: casparian_home.clone(),
        flow_dir,
        fixture: fixture.clone(),
        terminal,
        _temp_home: temp_home,
    };

    write_flow_meta(&run_dir, &args.flow, &runner, flow.steps.len())?;
    fs::write(run_dir.join("flow.json"), &flow_data)?;

    for (idx, step) in flow.steps.iter().enumerate() {
        let (capture, failure) = runner.run_step(idx, step)?;
        runner.write_step(idx, step, &capture)?;
        if let Some(failure) = failure {
            runner.write_failure(&failure)?;
            bail!("flow failed at step {}", failure.step_index);
        }
    }

    println!("Flow completed: {}", run_dir.display());
    Ok(())
}

fn create_run_dir(out: &Path, flow_name: &str) -> Result<PathBuf> {
    let timestamp = Utc::now().format("%Y%m%d_%H%M%S").to_string();
    let run_dir = out.join(format!("{}_{}", flow_name, timestamp));
    fs::create_dir_all(&run_dir)
        .with_context(|| format!("create run dir {}", run_dir.display()))?;
    Ok(run_dir)
}

fn prepare_casparian_home(env: &FlowEnv) -> Result<(PathBuf, Option<TempDir>)> {
    if let Some(ref home) = env.casparian_home {
        return Ok((home.clone(), None));
    }
    let temp = TempDir::new().context("create temp CASPARIAN_HOME")?;
    Ok((temp.path().to_path_buf(), Some(temp)))
}

fn prepare_fixture(env: &FlowEnv, casparian_home: &Path) -> Result<Option<FixtureInfo>> {
    let Some(fixture) = env.fixture.as_ref() else {
        return Ok(None);
    };

    let fixture_path = resolve_fixture_path(&fixture.path, casparian_home)?;
    ensure_fixture_tree(&fixture_path)?;

    let db_path = env
        .database
        .clone()
        .unwrap_or_else(active_db_path);
    let db = Database::open(&db_path).context("open scout database")?;
    let workspace = db.ensure_default_workspace().context("ensure workspace")?;
    context::set_active_workspace(&workspace.id).context("set active workspace")?;

    let existing = db.list_sources(&workspace.id)?;
    if existing.iter().any(|s| s.name == fixture.name) {
        return Ok(Some(FixtureInfo {
            name: fixture.name.clone(),
            path: fixture_path.display().to_string(),
        }));
    }

    let canonical = fixture_path
        .canonicalize()
        .unwrap_or_else(|_| fixture_path.clone());

    let source = Source {
        workspace_id: workspace.id,
        id: SourceId::new(),
        name: fixture.name.clone(),
        source_type: SourceType::Local,
        path: canonical.display().to_string(),
        exec_path: None,
        poll_interval_secs: 30,
        enabled: true,
    };
    db.upsert_source(&source)
        .context("insert fixture source")?;

    Ok(Some(FixtureInfo {
        name: fixture.name.clone(),
        path: canonical.display().to_string(),
    }))
}

fn resolve_fixture_path(path: &Path, casparian_home: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(casparian_home.join(path))
    }
}

fn ensure_fixture_tree(root: &Path) -> Result<()> {
    fs::create_dir_all(root).with_context(|| format!("create fixture dir {}", root.display()))?;

    let file_a = root.join("alpha.txt");
    let file_b = root.join("data").join("sample.csv");
    let file_c = root.join("data").join("sample.json");

    if !file_a.exists() {
        fs::write(&file_a, "alpha")?;
    }

    if let Some(parent) = file_b.parent() {
        fs::create_dir_all(parent)?;
    }
    if !file_b.exists() {
        fs::write(&file_b, "id,value\n1,one\n")?;
    }
    if !file_c.exists() {
        fs::write(&file_c, "{\"id\":1}\n")?;
    }

    Ok(())
}

fn write_flow_meta(run_dir: &Path, flow_path: &Path, runner: &FlowRunner, steps: usize) -> Result<()> {
    let meta = FlowRunMeta {
        flow: flow_path.display().to_string(),
        started_at: Utc::now().to_rfc3339(),
        casparian_home: runner.casparian_home.display().to_string(),
        terminal: format!("{}x{}", runner.terminal.width, runner.terminal.height),
        steps,
        fixture: runner.fixture.clone(),
    };

    let path = run_dir.join("run.json");
    fs::write(&path, serde_json::to_string_pretty(&meta)?)
        .with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

impl FlowRunner {
    fn run_step(
        &mut self,
        index: usize,
        step: &FlowStep,
    ) -> Result<(FlowCapture, Option<FlowFailure>)> {
        match step {
            FlowStep::Key { key, .. } => {
                self.app.handle_key(key.to_key_event());
                self.app.tick();
                let capture = self.capture_state()?;
                Ok((capture, None))
            }
            FlowStep::Text { text, .. } => {
                let resolved = self.resolve_text(text);
                for ch in resolved.chars() {
                    let key = match ch {
                        '\n' => KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                        '\t' => KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
                        _ => KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE),
                    };
                    self.app.handle_key(key);
                }
                self.app.tick();
                let capture = self.capture_state()?;
                Ok((capture, None))
            }
            FlowStep::Wait {
                ticks,
                ms,
                until,
                ..
            } => {
                let wait_ticks = compute_wait_ticks(*ticks, *ms);
                let mut capture = self.capture_state()?;
                if let Some(until_assert) = until {
                    let mut last_err = match assert_flow(
                        until_assert,
                        &capture.plain,
                        &capture.mask,
                        &capture.layout_signature,
                    ) {
                        Ok(_) => return Ok((capture, None)),
                        Err(err) => Some(err),
                    };

                    for _ in 0..wait_ticks {
                        self.app.tick();
                        capture = self.capture_state()?;
                        match assert_flow(
                            until_assert,
                            &capture.plain,
                            &capture.mask,
                            &capture.layout_signature,
                        ) {
                            Ok(_) => return Ok((capture, None)),
                            Err(err) => last_err = Some(err),
                        }
                    }

                    let err = last_err.unwrap_or_else(|| FlowAssertError::new(vec![]));
                    let failure = build_failure(index, step, &err, "wait condition not met");
                    return Ok((capture, Some(failure)));
                }

                for _ in 0..wait_ticks {
                    self.app.tick();
                }
                capture = self.capture_state()?;
                Ok((capture, None))
            }
            FlowStep::Assert { assert, .. } => {
                let capture = self.capture_state()?;
                match assert_flow(assert, &capture.plain, &capture.mask, &capture.layout_signature) {
                    Ok(_) => Ok((capture, None)),
                    Err(err) => {
                        let failure = build_failure(index, step, &err, "assertion failed");
                        Ok((capture, Some(failure)))
                    }
                }
            }
        }
    }

    fn capture_state(&self) -> Result<FlowCapture> {
        let buffer = render_app_to_buffer(&self.app, self.terminal.width, self.terminal.height)
            .context("render app")?;
        let plain = normalize_for_snapshot(&buffer_to_plain_text(&buffer));
        let mask = normalize_for_snapshot(&buffer_to_bg_mask(&buffer));
        let layout = layout_tree(&self.app, self.terminal.width, self.terminal.height);
        let layout_signature = layout_signature(&layout);
        Ok(FlowCapture {
            plain,
            mask,
            layout,
            layout_signature,
        })
    }

    fn write_step(&self, index: usize, step: &FlowStep, capture: &FlowCapture) -> Result<()> {
        let prefix = format!("step_{:03}", index);
        let text_path = self.run_dir.join(format!("{}.screen.txt", prefix));
        let mask_path = self.run_dir.join(format!("{}.mask.txt", prefix));
        let layout_path = self.run_dir.join(format!("{}.layout.json", prefix));
        let meta_path = self.run_dir.join(format!("{}.meta.json", prefix));

        fs::write(&text_path, &capture.plain)
            .with_context(|| format!("write {}", text_path.display()))?;
        fs::write(&mask_path, &capture.mask)
            .with_context(|| format!("write {}", mask_path.display()))?;
        fs::write(&layout_path, serde_json::to_string_pretty(&capture.layout)?)
            .with_context(|| format!("write {}", layout_path.display()))?;

        let meta = StepMeta {
            index,
            kind: step.kind().to_string(),
            label: step.label().map(|s| s.to_string()),
        };
        fs::write(&meta_path, serde_json::to_string_pretty(&meta)?)
            .with_context(|| format!("write {}", meta_path.display()))?;
        Ok(())
    }

    fn write_failure(&self, failure: &FlowFailure) -> Result<()> {
        let path = self.run_dir.join("failure.json");
        fs::write(&path, serde_json::to_string_pretty(&failure)?)
            .with_context(|| format!("write {}", path.display()))?;
        eprintln!("Flow failure at step {}", failure.step_index);
        Ok(())
    }

    fn resolve_text(&self, input: &str) -> String {
        let mut output = input.to_string();
        output = output.replace("{{CASPARIAN_HOME}}", &self.casparian_home.display().to_string());
        output = output.replace("${CASPARIAN_HOME}", &self.casparian_home.display().to_string());
        output = output.replace("{{FLOW_DIR}}", &self.flow_dir.display().to_string());
        if let Some(ref fixture) = self.fixture {
            output = output.replace("{{FIXTURE_PATH}}", &fixture.path);
            output = output.replace("${FIXTURE_PATH}", &fixture.path);
            output = output.replace("{{FIXTURE_NAME}}", &fixture.name);
            output = output.replace("${FIXTURE_NAME}", &fixture.name);
        }
        output
    }
}

fn compute_wait_ticks(ticks: Option<u32>, ms: Option<u64>) -> u32 {
    if let Some(ticks) = ticks {
        return ticks.max(1);
    }
    if let Some(ms) = ms {
        let count = (ms + WAIT_TICK_MS - 1) / WAIT_TICK_MS;
        return count.max(1) as u32;
    }
    1
}

fn layout_signature(layout: &[LayoutNode]) -> String {
    layout
        .iter()
        .map(|node| {
            format!(
                "{}|{}|{}|{}",
                node.id,
                node.title,
                node.kind,
                if node.focused { "focused" } else { "unfocused" }
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn build_failure(
    index: usize,
    step: &FlowStep,
    err: &FlowAssertError,
    summary: &str,
) -> FlowFailure {
    FlowFailure {
        step_index: index,
        step_kind: step.kind().to_string(),
        label: step.label().map(|s| s.to_string()),
        error: summary.to_string(),
        failures: err.failures.clone(),
    }
}
