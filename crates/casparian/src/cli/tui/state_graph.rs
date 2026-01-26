//! Deterministic UI state graph explorer.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fs;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use chrono::Utc;
use clap::Args;
use serde::{Deserialize, Serialize};

use super::app::{App, DiscoverViewState, IngestTab, TuiMode};
use super::flow::{FlowKey, FlowKeyCode, FlowModifiers};
use super::snapshot::{buffer_to_plain_text, normalize_for_snapshot, render_app_to_buffer};
use super::snapshot_states::snapshot_cases;
use super::ui_signature::UiSignature;
use super::ux_lint::{lint_dir, write_report, UxLintMode};

const ROOT_SIG: &str = "__ROOT__";

#[derive(Debug, Args)]
pub struct TuiStateGraphArgs {
    /// Output directory for graph artifacts
    #[arg(long, default_value = ".test_output/tui_state_graph")]
    pub out: PathBuf,

    /// Comma-separated list of snapshot seeds or "all"
    #[arg(long, default_value = "all")]
    pub seeds: String,

    /// Maximum number of nodes to discover
    #[arg(long, default_value_t = 500)]
    pub max_nodes: usize,

    /// Maximum depth (key steps after seed)
    #[arg(long, default_value_t = 12)]
    pub max_depth: usize,

    /// Render frames for each discovered signature
    #[arg(long, default_value_t = false)]
    pub render: bool,

    /// Comma-separated list of render sizes (e.g., 80x24,120x40)
    #[arg(long, default_value = "80x24,120x40")]
    pub render_sizes: String,

    /// Run UX lint on rendered frames
    #[arg(long, default_value_t = false)]
    pub lint: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExploreAction {
    Seed { case: String },
    Key { key: FlowKey },
}

#[derive(Debug, Clone, Serialize)]
pub struct PredecessorEntry {
    pub prev: String,
    pub action: ExploreAction,
    pub depth: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    pub action: ExploreAction,
}

#[derive(Debug, Serialize)]
struct GraphNode {
    key: String,
    signature: UiSignature,
}

#[derive(Debug, Serialize)]
struct GraphOutput {
    version: u32,
    generated_at: String,
    seeds: Vec<String>,
    alphabet: Vec<String>,
    max_nodes: usize,
    max_depth: usize,
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
    predecessors: BTreeMap<String, PredecessorEntry>,
}

#[derive(Debug, Serialize)]
struct PathEntry {
    seed: String,
    keys: Vec<FlowKey>,
}

#[derive(Debug, Serialize)]
struct PathsOutput {
    paths: BTreeMap<String, PathEntry>,
}

#[derive(Debug, Serialize)]
struct FrameIndex {
    version: u32,
    generated_at: String,
    frames: Vec<FrameIndexEntry>,
}

#[derive(Debug, Serialize)]
struct FrameIndexEntry {
    key: String,
    hash: String,
    sizes: Vec<String>,
}

struct GraphBuild {
    nodes: HashMap<String, UiSignature>,
    predecessors: HashMap<String, PredecessorEntry>,
    edges: Vec<GraphEdge>,
}

pub fn run(args: TuiStateGraphArgs) -> Result<()> {
    let seed_map = seed_builders();
    let seeds = parse_seeds(&args.seeds, &seed_map)?;
    let alphabet = default_alphabet();

    let graph = build_graph(&seeds, &seed_map, &alphabet, args.max_nodes, args.max_depth)?;

    write_outputs(&args, &seeds, &alphabet, &graph)?;
    if args.lint && !args.render {
        bail!("--lint requires --render to be set");
    }

    if args.render {
        let sizes = parse_sizes(&args.render_sizes)?;
        let frames_dir = render_frames(&args, &graph, &seed_map, &sizes)?;
        if args.lint {
            let report = lint_dir(&frames_dir, UxLintMode::StateGraph)?;
            write_report(&report, &args.out)?;
        }
    }
    Ok(())
}

fn seed_builders() -> HashMap<String, fn() -> App> {
    let mut map = HashMap::new();
    for case in snapshot_cases() {
        map.insert(case.name.to_string(), case.build);
    }
    map
}

fn parse_seeds(input: &str, available: &HashMap<String, fn() -> App>) -> Result<Vec<String>> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("all") {
        let mut all: Vec<String> = available.keys().cloned().collect();
        all.sort();
        return Ok(all);
    }

    let mut seeds = Vec::new();
    for part in trimmed.split(',') {
        let name = part.trim();
        if name.is_empty() {
            continue;
        }
        if !available.contains_key(name) {
            bail!("unknown seed '{}'", name);
        }
        seeds.push(name.to_string());
    }

    if seeds.is_empty() {
        bail!("no seeds provided");
    }

    Ok(seeds)
}

fn default_alphabet() -> Vec<FlowKey> {
    let mut keys = Vec::new();
    push_unique(&mut keys, key_code(FlowKeyCode::Esc));
    push_unique(&mut keys, key_code(FlowKeyCode::Tab));
    push_unique(&mut keys, key_code(FlowKeyCode::BackTab));
    push_unique(&mut keys, key_code(FlowKeyCode::Up));
    push_unique(&mut keys, key_code(FlowKeyCode::Down));
    push_unique(&mut keys, key_code(FlowKeyCode::Left));
    push_unique(&mut keys, key_code(FlowKeyCode::Right));

    push_unique(&mut keys, key_char('0'));
    push_unique(&mut keys, key_char('H'));
    push_unique(&mut keys, key_char('1'));
    push_unique(&mut keys, key_char('2'));
    push_unique(&mut keys, key_char('3'));
    push_unique(&mut keys, key_char('4'));
    push_unique(&mut keys, key_char('5'));
    push_unique(&mut keys, key_char(','));
    push_unique(&mut keys, key_char('['));
    push_unique(&mut keys, key_char(']'));

    push_unique(&mut keys, key_char('?'));
    push_unique(&mut keys, key_char('I'));
    push_unique(&mut keys, key_char('J'));
    push_unique(&mut keys, key_char('S'));
    push_unique(&mut keys, key_char(':'));
    push_unique(&mut keys, key_char('>'));
    push_unique(&mut keys, key_ctrl('w'));

    keys
}

fn discover_keys(app: &App) -> Vec<FlowKey> {
    if app.mode != TuiMode::Ingest || app.ingest_tab == IngestTab::Sources {
        return Vec::new();
    }

    let mut keys = Vec::new();
    push_unique(&mut keys, key_char('R'));
    push_unique(&mut keys, key_char('M'));

    match app.discover.view_state {
        DiscoverViewState::Files => {
            push_unique(&mut keys, key_char('/'));
            push_unique(&mut keys, key_char('p'));
            push_unique(&mut keys, key_char('s'));
            push_unique(&mut keys, key_char('t'));
        }
        DiscoverViewState::RuleBuilder => {
            push_unique(&mut keys, key_char('s'));
        }
        _ => {}
    }

    keys
}

fn allowed_keys(app: &App, alphabet: &[FlowKey]) -> Vec<FlowKey> {
    if app.command_palette.visible {
        return vec![key_code(FlowKeyCode::Esc)];
    }
    if app.workspace_switcher.visible {
        return vec![
            key_code(FlowKeyCode::Esc),
            key_code(FlowKeyCode::Up),
            key_code(FlowKeyCode::Down),
        ];
    }
    if app.show_help {
        return vec![key_code(FlowKeyCode::Esc), key_char('?')];
    }
    if app.jobs_drawer_open || app.sources_drawer_open {
        return vec![
            key_code(FlowKeyCode::Esc),
            key_code(FlowKeyCode::Up),
            key_code(FlowKeyCode::Down),
            key_code(FlowKeyCode::Enter),
        ];
    }
    if app.is_text_input_mode() {
        return vec![key_code(FlowKeyCode::Esc)];
    }

    let mut keys = alphabet.to_vec();
    let extra = discover_keys(app);
    for key in extra {
        push_unique(&mut keys, key);
    }
    keys
}

fn build_graph(
    seeds: &[String],
    seed_map: &HashMap<String, fn() -> App>,
    alphabet: &[FlowKey],
    max_nodes: usize,
    max_depth: usize,
) -> Result<GraphBuild> {
    let mut visited = HashSet::new();
    let mut nodes = HashMap::new();
    let mut predecessors: HashMap<String, PredecessorEntry> = HashMap::new();
    let mut edges: Vec<GraphEdge> = Vec::new();
    let mut queue: VecDeque<String> = VecDeque::new();

    for seed_name in seeds {
        let builder = seed_map
            .get(seed_name)
            .with_context(|| format!("missing seed '{}'", seed_name))?;
        let app = builder();
        let signature = app.ui_signature();
        let key = signature.key();

        if visited.insert(key.clone()) {
            nodes.insert(key.clone(), signature);
            predecessors.insert(
                key.clone(),
                PredecessorEntry {
                    prev: ROOT_SIG.to_string(),
                    action: ExploreAction::Seed {
                        case: seed_name.clone(),
                    },
                    depth: 0,
                },
            );
            edges.push(GraphEdge {
                from: ROOT_SIG.to_string(),
                to: key.clone(),
                action: ExploreAction::Seed {
                    case: seed_name.clone(),
                },
            });
            queue.push_back(key);
        }

        if visited.len() >= max_nodes {
            break;
        }
    }

    while let Some(current_key) = queue.pop_front() {
        if visited.len() >= max_nodes {
            break;
        }

        let current_depth = predecessors
            .get(&current_key)
            .map(|entry| entry.depth)
            .unwrap_or(0);
        if current_depth >= max_depth {
            continue;
        }

        let app = reconstruct_app(&current_key, &predecessors, seed_map)?;
        let keys = allowed_keys(&app, alphabet);

        for key in keys {
            if visited.len() >= max_nodes {
                break;
            }

            let mut next_app = reconstruct_app(&current_key, &predecessors, seed_map)?;
            apply_key_step(&mut next_app, &key);

            if !next_app.running {
                continue;
            }

            let next_signature = next_app.ui_signature();
            let next_key = next_signature.key();

            edges.push(GraphEdge {
                from: current_key.clone(),
                to: next_key.clone(),
                action: ExploreAction::Key { key: key.clone() },
            });

            if visited.insert(next_key.clone()) {
                nodes.insert(next_key.clone(), next_signature);
                predecessors.insert(
                    next_key.clone(),
                    PredecessorEntry {
                        prev: current_key.clone(),
                        action: ExploreAction::Key { key: key.clone() },
                        depth: current_depth + 1,
                    },
                );
                queue.push_back(next_key);
            }
        }
    }

    Ok(GraphBuild {
        nodes,
        predecessors,
        edges,
    })
}

fn reconstruct_app(
    target_sig: &str,
    predecessors: &HashMap<String, PredecessorEntry>,
    seeds: &HashMap<String, fn() -> App>,
) -> Result<App> {
    let mut actions: Vec<ExploreAction> = Vec::new();
    let mut current = target_sig;

    loop {
        let entry = predecessors
            .get(current)
            .with_context(|| format!("missing predecessor for {}", current))?;
        actions.push(entry.action.clone());
        if entry.prev == ROOT_SIG {
            break;
        }
        current = &entry.prev;
    }

    actions.reverse();
    let mut iter = actions.into_iter();
    let seed_action = iter
        .next()
        .with_context(|| format!("missing seed action for {}", target_sig))?;

    let seed_name = match seed_action {
        ExploreAction::Seed { case } => case,
        ExploreAction::Key { .. } => {
            bail!("path for {} does not start with a seed", target_sig)
        }
    };

    let builder = seeds
        .get(&seed_name)
        .with_context(|| format!("unknown seed '{}'", seed_name))?;
    let mut app = builder();

    for action in iter {
        match action {
            ExploreAction::Key { key } => apply_key_step(&mut app, &key),
            ExploreAction::Seed { .. } => {
                bail!("unexpected seed action in path for {}", target_sig)
            }
        }
    }

    let computed = app.ui_signature_key();
    if computed != target_sig {
        bail!(
            "signature mismatch for {} (replayed {})",
            target_sig,
            computed
        );
    }

    Ok(app)
}

fn apply_key_step(app: &mut App, key: &FlowKey) {
    app.handle_key(key.to_key_event());
    app.tick();
}

fn write_outputs(
    args: &TuiStateGraphArgs,
    seeds: &[String],
    alphabet: &[FlowKey],
    graph: &GraphBuild,
) -> Result<()> {
    fs::create_dir_all(&args.out)
        .with_context(|| format!("create output dir {}", args.out.display()))?;

    let mut nodes: Vec<GraphNode> = graph
        .nodes
        .iter()
        .map(|(key, signature)| GraphNode {
            key: key.clone(),
            signature: signature.clone(),
        })
        .collect();
    nodes.sort_by(|a, b| a.key.cmp(&b.key));

    let mut edges = graph.edges.clone();
    edges.sort_by(|a, b| {
        let a_label = edge_label(a);
        let b_label = edge_label(b);
        (a.from.clone(), a.to.clone(), a_label).cmp(&(b.from.clone(), b.to.clone(), b_label))
    });

    let mut predecessors: BTreeMap<String, PredecessorEntry> = BTreeMap::new();
    for (key, entry) in &graph.predecessors {
        predecessors.insert(key.clone(), entry.clone());
    }

    let alphabet_labels: Vec<String> = alphabet.iter().map(|k| k.to_string()).collect();

    let output = GraphOutput {
        version: 1,
        generated_at: Utc::now().to_rfc3339(),
        seeds: seeds.to_vec(),
        alphabet: alphabet_labels,
        max_nodes: args.max_nodes,
        max_depth: args.max_depth,
        nodes,
        edges,
        predecessors,
    };

    let graph_path = args.out.join("graph.json");
    fs::write(&graph_path, serde_json::to_string_pretty(&output)?)
        .with_context(|| format!("write {}", graph_path.display()))?;

    let paths = build_paths(&graph.predecessors)?;
    let paths_output = PathsOutput { paths };
    let paths_path = args.out.join("paths.json");
    fs::write(&paths_path, serde_json::to_string_pretty(&paths_output)?)
        .with_context(|| format!("write {}", paths_path.display()))?;

    Ok(())
}

fn render_frames(
    args: &TuiStateGraphArgs,
    graph: &GraphBuild,
    seed_map: &HashMap<String, fn() -> App>,
    sizes: &[(u16, u16)],
) -> Result<PathBuf> {
    if sizes.is_empty() {
        bail!("render sizes cannot be empty");
    }

    let frames_dir = args.out.join("frames");
    fs::create_dir_all(&frames_dir)
        .with_context(|| format!("create frames dir {}", frames_dir.display()))?;

    let mut keys: Vec<String> = graph.nodes.keys().cloned().collect();
    keys.sort();

    let mut index_entries = Vec::new();

    for key in keys {
        let app = reconstruct_app(&key, &graph.predecessors, seed_map)?;
        let hash = blake3::hash(key.as_bytes()).to_hex().to_string();
        let mut size_labels = Vec::new();

        for (width, height) in sizes {
            let buffer = render_app_to_buffer(&app, *width, *height).with_context(|| {
                format!("render signature {} at {}x{}", key, width, height)
            })?;
            let plain = normalize_for_snapshot(&buffer_to_plain_text(&buffer));
            let label = format!("{}x{}", width, height);
            let filename = format!("{}_{}.txt", hash, label);
            let path = frames_dir.join(filename);
            fs::write(&path, plain).with_context(|| format!("write {}", path.display()))?;
            size_labels.push(label);
        }

        index_entries.push(FrameIndexEntry {
            key,
            hash,
            sizes: size_labels,
        });
    }

    let index = FrameIndex {
        version: 1,
        generated_at: Utc::now().to_rfc3339(),
        frames: index_entries,
    };
    let index_path = frames_dir.join("index.json");
    fs::write(&index_path, serde_json::to_string_pretty(&index)?)
        .with_context(|| format!("write {}", index_path.display()))?;

    Ok(frames_dir)
}

fn parse_sizes(input: &str) -> Result<Vec<(u16, u16)>> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        bail!("render sizes cannot be empty");
    }

    let mut sizes = Vec::new();
    for part in trimmed.split(',') {
        let token = part.trim();
        if token.is_empty() {
            continue;
        }
        let (w, h) = token
            .split_once('x')
            .with_context(|| format!("invalid size '{}', expected WxH", token))?;
        let width: u16 = w
            .parse()
            .with_context(|| format!("invalid width '{}', expected u16", w))?;
        let height: u16 = h
            .parse()
            .with_context(|| format!("invalid height '{}', expected u16", h))?;
        if width == 0 || height == 0 {
            bail!("invalid size '{}': width/height must be > 0", token);
        }
        sizes.push((width, height));
    }

    if sizes.is_empty() {
        bail!("render sizes cannot be empty");
    }

    Ok(sizes)
}

fn build_paths(
    predecessors: &HashMap<String, PredecessorEntry>,
) -> Result<BTreeMap<String, PathEntry>> {
    let mut paths: BTreeMap<String, PathEntry> = BTreeMap::new();
    for key in predecessors.keys() {
        let entry = build_path_for(key, predecessors)?;
        paths.insert(key.clone(), entry);
    }
    Ok(paths)
}

fn build_path_for(
    target_sig: &str,
    predecessors: &HashMap<String, PredecessorEntry>,
) -> Result<PathEntry> {
    let mut actions: Vec<ExploreAction> = Vec::new();
    let mut current = target_sig;

    loop {
        let entry = predecessors
            .get(current)
            .with_context(|| format!("missing predecessor for {}", current))?;
        actions.push(entry.action.clone());
        if entry.prev == ROOT_SIG {
            break;
        }
        current = &entry.prev;
    }

    actions.reverse();
    let mut iter = actions.into_iter();
    let seed_action = iter
        .next()
        .with_context(|| format!("missing seed action for {}", target_sig))?;
    let seed = match seed_action {
        ExploreAction::Seed { case } => case,
        ExploreAction::Key { .. } => bail!("path for {} missing seed", target_sig),
    };

    let mut keys = Vec::new();
    for action in iter {
        match action {
            ExploreAction::Key { key } => keys.push(key),
            ExploreAction::Seed { .. } => {
                bail!("unexpected seed in path for {}", target_sig)
            }
        }
    }

    Ok(PathEntry { seed, keys })
}

fn edge_label(edge: &GraphEdge) -> String {
    match &edge.action {
        ExploreAction::Seed { case } => format!("seed:{}", case),
        ExploreAction::Key { key } => format!("key:{}", key),
    }
}

fn key_code(code: FlowKeyCode) -> FlowKey {
    FlowKey {
        code,
        modifiers: FlowModifiers::default(),
    }
}

fn key_char(ch: char) -> FlowKey {
    FlowKey {
        code: FlowKeyCode::Char(ch),
        modifiers: FlowModifiers::default(),
    }
}

fn key_ctrl(ch: char) -> FlowKey {
    FlowKey {
        code: FlowKeyCode::Char(ch),
        modifiers: FlowModifiers {
            ctrl: true,
            alt: false,
            shift: false,
        },
    }
}

fn push_unique(keys: &mut Vec<FlowKey>, key: FlowKey) {
    if !keys.contains(&key) {
        keys.push(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::tui::snapshot_states::snapshot_cases;

    fn find_case(name: &str) -> &'static super::super::snapshot_states::SnapshotCase {
        snapshot_cases()
            .iter()
            .find(|case| case.name == name)
            .unwrap_or_else(|| panic!("missing snapshot case {}", name))
    }

    #[test]
    fn explorer_runs() {
        let seed_map = seed_builders();
        let seeds = vec!["home_default".to_string()];
        let alphabet = default_alphabet();
        let graph = build_graph(&seeds, &seed_map, &alphabet, 25, 6).unwrap();

        assert!(graph.nodes.len() > 1);
        for key in graph.nodes.keys() {
            assert!(graph.predecessors.contains_key(key));
        }
    }

    #[test]
    fn replay_matches_signature() {
        let seed_map = seed_builders();
        let seeds = vec!["home_default".to_string()];
        let alphabet = default_alphabet();
        let graph = build_graph(&seeds, &seed_map, &alphabet, 25, 6).unwrap();

        let mut keys: Vec<String> = graph.nodes.keys().cloned().collect();
        keys.sort();
        for key in keys.into_iter().take(3) {
            let app = reconstruct_app(&key, &graph.predecessors, &seed_map).unwrap();
            assert_eq!(app.ui_signature_key(), key);
        }
    }

    #[test]
    fn command_palette_gates_keys() {
        let case = find_case("command_palette_open");
        let app = (case.build)();
        let alphabet = default_alphabet();
        let keys = allowed_keys(&app, &alphabet);
        assert_eq!(keys, vec![key_code(FlowKeyCode::Esc)]);
    }
}
