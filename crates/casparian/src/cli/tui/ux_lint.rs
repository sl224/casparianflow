//! Heuristic UX linting for TUI snapshot frames.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use chrono::Utc;
use clap::{Args, ValueEnum};
use regex::Regex;
use serde::Serialize;
use walkdir::WalkDir;

use super::snapshot::normalize_for_snapshot;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum UxLintMode {
    Snapshots,
    StateGraph,
}

#[derive(Debug, Args)]
pub struct TuiUxLintArgs {
    /// Input directory containing rendered frame text files
    #[arg(long = "in")]
    pub input: PathBuf,

    /// Output directory for the lint report
    #[arg(long, default_value = ".test_output/tui_ux_lint")]
    pub out: PathBuf,

    /// Lint mode (snapshots or state-graph)
    #[arg(long, value_enum, default_value = "snapshots")]
    pub mode: UxLintMode,
}

#[derive(Debug, Serialize)]
pub struct UxLintReport {
    version: u32,
    generated_at: String,
    mode: String,
    input: String,
    frames: Vec<UxFrameReport>,
    summary: UxLintSummary,
}

#[derive(Debug, Serialize)]
struct UxLintSummary {
    frames: usize,
    border_collision_frames: usize,
    footer_truncated_frames: usize,
    header_truncated_frames: usize,
    truncated_panel_title_frames: usize,
    total_border_collision_hits: usize,
    total_truncated_panel_titles: usize,
}

#[derive(Debug, Serialize)]
pub struct UxFrameReport {
    frame: String,
    width: Option<u16>,
    height: Option<u16>,
    border_collision_hits: usize,
    footer_truncated: bool,
    header_truncated: bool,
    truncated_panel_titles: usize,
}

pub fn run(args: TuiUxLintArgs) -> Result<()> {
    let report = lint_dir(&args.input, args.mode)?;
    write_report(&report, &args.out)?;
    Ok(())
}

pub fn lint_dir(input: &Path, mode: UxLintMode) -> Result<UxLintReport> {
    if !input.exists() {
        bail!("input directory {} does not exist", input.display());
    }

    let size_re = Regex::new(r"(\d+)x(\d+)").expect("size regex");
    let mut frames = Vec::new();

    for entry in WalkDir::new(input).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if !path.extension().is_some_and(|ext| ext == "txt") {
            continue;
        }
        let path_str = path.to_string_lossy();
        if path_str.ends_with(".mask.txt") {
            continue;
        }

        let raw = fs::read_to_string(path)
            .with_context(|| format!("read frame {}", path.display()))?;
        let contents = normalize_for_snapshot(&raw);

        let relative = path
            .strip_prefix(input)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        let (width, height) = parse_size_from_name(&size_re, &relative);
        let report = analyze_frame(&relative, &contents, width, height);
        frames.push(report);
    }

    if frames.is_empty() {
        bail!("no frame .txt files found under {}", input.display());
    }

    frames.sort_by(|a, b| a.frame.cmp(&b.frame));

    let summary = summarize(&frames);

    Ok(UxLintReport {
        version: 1,
        generated_at: Utc::now().to_rfc3339(),
        mode: mode_label(mode).to_string(),
        input: input.display().to_string(),
        frames,
        summary,
    })
}

pub fn write_report(report: &UxLintReport, out_dir: &Path) -> Result<PathBuf> {
    fs::create_dir_all(out_dir)
        .with_context(|| format!("create ux lint output dir {}", out_dir.display()))?;
    let path = out_dir.join("ux_report.json");
    fs::write(&path, serde_json::to_string_pretty(report)?)
        .with_context(|| format!("write {}", path.display()))?;
    Ok(path)
}

fn parse_size_from_name(size_re: &Regex, name: &str) -> (Option<u16>, Option<u16>) {
    let mut width = None;
    let mut height = None;
    for caps in size_re.captures_iter(name) {
        let w = caps.get(1).and_then(|m| m.as_str().parse::<u16>().ok());
        let h = caps.get(2).and_then(|m| m.as_str().parse::<u16>().ok());
        width = w;
        height = h;
    }
    (width, height)
}

fn analyze_frame(
    frame_name: &str,
    contents: &str,
    width: Option<u16>,
    height: Option<u16>,
) -> UxFrameReport {
    let lines: Vec<&str> = contents.lines().collect();
    let header_truncated = lines
        .first()
        .map_or(false, |line| has_ellipsis(line));
    let footer_truncated = lines
        .last()
        .map_or(false, |line| has_ellipsis(line));
    let border_collision_hits = count_border_collisions(contents);
    let truncated_panel_titles = lines.iter().filter(|line| is_truncated_panel_title(line)).count();

    UxFrameReport {
        frame: frame_name.to_string(),
        width,
        height,
        border_collision_hits,
        footer_truncated,
        header_truncated,
        truncated_panel_titles,
    }
}

fn summarize(frames: &[UxFrameReport]) -> UxLintSummary {
    let mut border_collision_frames = 0;
    let mut footer_truncated_frames = 0;
    let mut header_truncated_frames = 0;
    let mut truncated_panel_title_frames = 0;
    let mut total_border_collision_hits = 0;
    let mut total_truncated_panel_titles = 0;

    for frame in frames {
        if frame.border_collision_hits > 0 {
            border_collision_frames += 1;
        }
        if frame.footer_truncated {
            footer_truncated_frames += 1;
        }
        if frame.header_truncated {
            header_truncated_frames += 1;
        }
        if frame.truncated_panel_titles > 0 {
            truncated_panel_title_frames += 1;
        }
        total_border_collision_hits += frame.border_collision_hits;
        total_truncated_panel_titles += frame.truncated_panel_titles;
    }

    UxLintSummary {
        frames: frames.len(),
        border_collision_frames,
        footer_truncated_frames,
        header_truncated_frames,
        truncated_panel_title_frames,
        total_border_collision_hits,
        total_truncated_panel_titles,
    }
}

fn mode_label(mode: UxLintMode) -> &'static str {
    match mode {
        UxLintMode::Snapshots => "snapshots",
        UxLintMode::StateGraph => "state-graph",
    }
}

fn has_ellipsis(line: &str) -> bool {
    line.contains("...") || line.contains('…')
}

fn count_border_collisions(contents: &str) -> usize {
    const PATTERNS: [&str; 10] = [
        "└─│", "┌ │", "┌│", "│└", "┘│", "│┘", "└┐", "┌┘", "┤┌", "┐└",
    ];
    PATTERNS
        .iter()
        .map(|pat| contents.match_indices(pat).count())
        .sum()
}

fn is_truncated_panel_title(line: &&str) -> bool {
    if !has_ellipsis(line) {
        return false;
    }
    let border_chars = ['┌', '┐', '┬', '┴', '┤', '├', '─', '│', '[', ']'];
    line.chars().any(|ch| border_chars.contains(&ch))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_border_collision() {
        let sample = "└─│\nnormal";
        assert_eq!(count_border_collisions(sample), 1);
    }

    #[test]
    fn detects_ellipsis_in_header_footer() {
        let sample = "header...\nbody\nfooter...";
        let report = analyze_frame("frame", sample, None, None);
        assert!(report.header_truncated);
        assert!(report.footer_truncated);
    }
}
