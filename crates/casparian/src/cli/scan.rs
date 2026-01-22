//! Scan command - Discover files in a directory
//!
//! Scans a directory for files matching specified criteria and stores
//! file metadata in the database for later filtering and tagging.
//!
//! Uses casparian::scout::Database as the single source of truth.

use crate::cli::error::HelpfulError;
use crate::cli::output::{color_for_extension, format_size, format_time, print_table_colored};
use casparian::scout::scan_path;
use casparian::scout::{Database, ScannedFile, Scanner, Source, SourceId, SourceType};
use comfy_table::Color;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use globset::{Glob, GlobSet, GlobSetBuilder};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::io::stdout;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Arguments for the scan command
#[derive(Debug)]
pub struct ScanArgs {
    pub path: PathBuf,
    pub types: Vec<String>,
    pub patterns: Vec<String>,
    pub recursive: bool,
    pub depth: Option<usize>,
    pub min_size: Option<String>,
    pub max_size: Option<String>,
    pub json: bool,
    pub stats: bool,
    pub quiet: bool,
    pub interactive: bool,
    pub tag: Option<String>,
}

/// Discovered file information
#[derive(Debug, Clone, Serialize)]
pub struct DiscoveredFile {
    pub path: PathBuf,
    pub name: String,
    pub extension: Option<String>,
    pub size: u64,
    #[serde(with = "system_time_serde")]
    pub modified: SystemTime,
}

/// Summary statistics
#[derive(Debug, Clone, Serialize)]
pub struct ScanSummary {
    pub total_files: usize,
    pub total_size: u64,
    pub files_by_type: HashMap<String, usize>,
    pub size_by_type: HashMap<String, u64>,
    pub directories_scanned: usize,
}

/// Complete scan result
#[derive(Debug, Serialize)]
pub struct ScanResult {
    pub files: Vec<DiscoveredFile>,
    pub summary: ScanSummary,
    pub scan_path: PathBuf,
}

// Custom serialization for SystemTime
mod system_time_serde {
    use serde::{Serialize, Serializer};
    use std::time::SystemTime;

    pub fn serialize<S>(time: &SystemTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let duration = time
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(serde::ser::Error::custom)?;
        duration.as_secs().serialize(serializer)
    }
}

/// Get the active database path
fn get_db_path() -> PathBuf {
    crate::cli::config::active_db_path()
}

/// Build a GlobSet from pattern strings
fn build_glob_set(patterns: &[String]) -> anyhow::Result<(GlobSet, GlobSet)> {
    let mut include_builder = GlobSetBuilder::new();
    let mut exclude_builder = GlobSetBuilder::new();

    for pattern in patterns {
        if let Some(stripped) = pattern.strip_prefix('!') {
            // Exclusion pattern
            exclude_builder.add(Glob::new(stripped)?);
        } else {
            // Inclusion pattern
            include_builder.add(Glob::new(pattern)?);
        }
    }

    Ok((include_builder.build()?, exclude_builder.build()?))
}

/// Check if a path matches the pattern filters
fn matches_patterns(
    rel_path: &str,
    include_set: &GlobSet,
    exclude_set: &GlobSet,
    has_includes: bool,
) -> bool {
    // If excluded, always reject
    if exclude_set.is_match(rel_path) {
        return false;
    }

    // If no include patterns, accept all (that aren't excluded)
    if !has_includes {
        return true;
    }

    // Must match at least one include pattern
    include_set.is_match(rel_path)
}

/// Execute the scan command (async version)
///
/// Uses the consolidated Scanner for file discovery, storage, and cache building.
/// CLI-specific filters are applied post-scan for display and tagging.
pub fn run(args: ScanArgs) -> anyhow::Result<()> {
    if args.json && args.interactive {
        return Err(HelpfulError::new("Cannot combine --json and --interactive")
            .with_context("Interactive mode renders a TUI, not JSON output")
            .with_suggestion("TRY: Remove --interactive to use --json".to_string())
            .into());
    }

    let expanded_path = scan_path::expand_scan_path(&args.path);
    if let Err(err) = scan_path::validate_scan_path(&expanded_path) {
        return Err(match err {
            scan_path::ScanPathError::NotFound(path) => HelpfulError::path_not_found(&path),
            scan_path::ScanPathError::NotDirectory(path) => HelpfulError::not_a_directory(&path),
            scan_path::ScanPathError::NotReadable(path) => HelpfulError::new(format!(
                "Cannot read directory: {}",
                path.display()
            )),
        }
        .into());
    }

    // Canonicalize scan path
    let scan_path = scan_path::canonicalize_scan_path(&expanded_path);

    // Setup database
    let db_path = get_db_path();
    let db_dir = db_path.parent().unwrap();
    fs::create_dir_all(db_dir)?;

    let db = Database::open(&db_path)
        .map_err(|e| HelpfulError::new(format!("Failed to open database: {}", e))
            .with_context(format!("Database path: {}", db_path.display())))?;

    // Get or create source
    let source = get_or_create_source(&db, &scan_path)?;

    // Use Scanner for discovery, storage, and cache building
    // Note: Scanner scans ALL files; CLI filters are applied post-scan
    let scanner = Scanner::new(db.clone());
    let scan_result = scanner
        .scan(&source, None, None)
        
        .map_err(|e| HelpfulError::new(format!("Scan failed: {}", e)))?;

    // Query all files from database
    let db_files = db
        .list_files_by_source(&source.id, 1_000_000)
        
        .map_err(|e| HelpfulError::new(format!("Failed to query files: {}", e)))?;

    // Convert to DiscoveredFile and apply CLI filters
    let files = apply_cli_filters(&db_files, &args, &scan_path)?;

    // Tag filtered files if requested (only tags files matching filters)
    let tagged_count = if let Some(ref tag) = args.tag {
        tag_filtered_files(&db, &source.id, &files, tag)?
    } else {
        0
    };

    // Build summary from filtered files
    let summary = build_summary(&files, scan_result.stats.dirs_scanned as usize);

    let result = ScanResult {
        files,
        summary,
        scan_path: args.path.clone(),
    };

    // Output based on format
    if args.interactive {
        run_interactive(result, args.tag.clone())?;
    } else if args.json {
        output_json(&result)?;
    } else if args.stats {
        output_stats(&result);
    } else if args.quiet {
        output_quiet(&result);
    } else {
        let stored = (scan_result.stats.files_new + scan_result.stats.files_changed) as usize;
        output_table(&result, stored, tagged_count, args.tag.as_deref());
    }

    Ok(())
}

/// Convert ScannedFile to DiscoveredFile
fn scanned_to_discovered(file: &ScannedFile) -> DiscoveredFile {
    let path = PathBuf::from(&file.path);
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|ext| ext.to_lowercase());
    let modified = UNIX_EPOCH + Duration::from_millis(file.mtime as u64);

    DiscoveredFile {
        path,
        name,
        extension,
        size: file.size,
        modified,
    }
}

/// Apply CLI filters (type, pattern, size, depth) to scanned files
fn apply_cli_filters(
    db_files: &[ScannedFile],
    args: &ScanArgs,
    scan_path: &PathBuf,
) -> anyhow::Result<Vec<DiscoveredFile>> {
    // Parse size filters
    let min_size = args
        .min_size
        .as_ref()
        .map(|s| crate::cli::output::parse_size(s))
        .transpose()
        .map_err(|e| HelpfulError::invalid_size_format(&e))?;

    let max_size = args
        .max_size
        .as_ref()
        .map(|s| crate::cli::output::parse_size(s))
        .transpose()
        .map_err(|e| HelpfulError::invalid_size_format(&e))?;

    // Normalize type filters to lowercase
    let type_filters: Vec<String> = args.types.iter().map(|t| t.to_lowercase()).collect();

    // Build glob pattern matchers
    let (include_set, exclude_set) = build_glob_set(&args.patterns)?;
    let has_include_patterns = args.patterns.iter().any(|p| !p.starts_with('!'));

    let mut files = Vec::new();

    for db_file in db_files {
        let discovered = scanned_to_discovered(db_file);

        // Apply depth filter
        if let Some(max_depth) = args.depth {
            let rel_path = discovered
                .path
                .strip_prefix(scan_path)
                .unwrap_or(&discovered.path);
            let depth = rel_path.components().count();
            if depth > max_depth {
                continue;
            }
        }

        // Non-recursive means depth 1 only
        if !args.recursive {
            let rel_path = discovered
                .path
                .strip_prefix(scan_path)
                .unwrap_or(&discovered.path);
            if rel_path.components().count() > 1 {
                continue;
            }
        }

        // Apply pattern filter
        if !args.patterns.is_empty() {
            let rel_path = discovered
                .path
                .strip_prefix(scan_path)
                .unwrap_or(&discovered.path)
                .display()
                .to_string();
            if !matches_patterns(&rel_path, &include_set, &exclude_set, has_include_patterns) {
                continue;
            }
        }

        // Apply size filters
        if let Some(min) = min_size {
            if discovered.size < min {
                continue;
            }
        }
        if let Some(max) = max_size {
            if discovered.size > max {
                continue;
            }
        }

        // Apply type filter
        if !type_filters.is_empty() {
            let ext = discovered.extension.as_deref().unwrap_or("");
            if !type_filters.iter().any(|t| t == ext) {
                continue;
            }
        }

        files.push(discovered);
    }

    // Sort by path for consistent output
    files.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(files)
}

/// Tag only the filtered files
fn tag_filtered_files(
    db: &Database,
    source_id: &SourceId,
    files: &[DiscoveredFile],
    tag: &str,
) -> anyhow::Result<usize> {
    let mut tagged = 0;

    // We need file IDs to tag. Query DB for each file by path.
    // This is not ideal but maintains the CLI behavior of only tagging filtered files.
    for file in files {
        let path_str = file.path.display().to_string();
        if let Ok(Some(db_file)) = db.get_file_by_path(source_id, &path_str) {
            if let Some(id) = db_file.id {
                if db.tag_file(id, tag).is_ok() {
                    tagged += 1;
                }
            }
        }
    }

    Ok(tagged)
}

/// Get or create a source for the scan path
fn get_or_create_source(db: &Database, path: &PathBuf) -> anyhow::Result<Source> {
    let path_str = path.display().to_string();

    // Try to find existing source by path
    let sources = db.list_sources()
        .map_err(|e| HelpfulError::new(format!("Failed to list sources: {}", e)))?;

    for source in sources {
        if source.path == path_str {
            return Ok(source);
        }
    }

    // Create new source
    let id = SourceId::new();
    let name = path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("scan")
        .to_string();

    let source = Source {
        id: id.clone(),
        name,
        source_type: SourceType::Local,
        path: path_str,
        poll_interval_secs: 0, // CLI scans are one-shot
        enabled: true,
    };

    db.upsert_source(&source)
        .map_err(|e| HelpfulError::new(format!("Failed to create source: {}", e)))?;

    Ok(source)
}

/// Build summary statistics from discovered files
fn build_summary(files: &[DiscoveredFile], directories_scanned: usize) -> ScanSummary {
    let mut files_by_type: HashMap<String, usize> = HashMap::new();
    let mut size_by_type: HashMap<String, u64> = HashMap::new();
    let mut total_size: u64 = 0;

    for file in files {
        total_size += file.size;

        let ext = file
            .extension
            .as_deref()
            .unwrap_or("(no ext)")
            .to_string();

        *files_by_type.entry(ext.clone()).or_insert(0) += 1;
        *size_by_type.entry(ext).or_insert(0) += file.size;
    }

    ScanSummary {
        total_files: files.len(),
        total_size,
        files_by_type,
        size_by_type,
        directories_scanned,
    }
}

/// Output as JSON
fn output_json(result: &ScanResult) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(result)?;
    println!("{}", json);
    Ok(())
}

/// Output as statistics summary
fn output_stats(result: &ScanResult) {
    let summary = &result.summary;

    println!("Scan: {}", result.scan_path.display());
    println!();
    println!("Files:       {}", summary.total_files);
    println!("Total Size:  {}", format_size(summary.total_size));
    println!("Directories: {}", summary.directories_scanned);
    println!();

    if !summary.files_by_type.is_empty() {
        println!("By Type:");

        // Sort by count descending
        let mut types: Vec<_> = summary.files_by_type.iter().collect();
        types.sort_by(|a, b| b.1.cmp(a.1));

        for (ext, count) in types {
            let size = summary.size_by_type.get(ext).copied().unwrap_or(0);
            println!("  {:<12} {:>6} files  {:>10}", ext, count, format_size(size));
        }
    }
}

/// Output just file paths (quiet mode)
fn output_quiet(result: &ScanResult) {
    for file in &result.files {
        println!("{}", file.path.display());
    }
}

/// Output as formatted table
fn output_table(result: &ScanResult, stored: usize, tagged: usize, tag: Option<&str>) {
    if result.files.is_empty() {
        println!("No files found in: {}", result.scan_path.display());
        return;
    }

    println!(
        "Found {} files in {} ({} total)",
        result.summary.total_files,
        result.scan_path.display(),
        format_size(result.summary.total_size)
    );

    // Show storage info
    println!("Stored {} files in database", stored);
    if let Some(t) = tag {
        println!("Tagged {} files with: \x1b[36m{}\x1b[0m", tagged, t);
    }
    println!();

    let headers = &["Name", "Type", "Size", "Modified", "Path"];

    let rows: Vec<Vec<(String, Option<Color>)>> = result
        .files
        .iter()
        .map(|file| {
            let ext_value = file.extension.as_deref().unwrap_or("");
            let ext_color = color_for_extension(ext_value);
            let ext_display = if ext_value.is_empty() {
                "-".to_string()
            } else {
                ext_value.to_string()
            };

            // Get relative path from scan root
            let display_path = file
                .path
                .strip_prefix(&result.scan_path)
                .unwrap_or(&file.path)
                .display()
                .to_string();

            vec![
                (file.name.clone(), None),
                (ext_display, Some(ext_color)),
                (format_size(file.size), None),
                (format_time(file.modified), None),
                (display_path, Some(Color::Grey)),
            ]
        })
        .collect();

    print_table_colored(headers, rows);

    // Show next steps
    print_next_steps(result, tag);
}

/// Print helpful next steps after scan
fn print_next_steps(result: &ScanResult, tag: Option<&str>) {
    println!();
    println!("\x1b[90m{}\x1b[0m", "─".repeat(60));
    println!();

    // Get a sample file for examples
    let sample_file = result.files.first().map(|f| {
        f.path
            .strip_prefix(&result.scan_path)
            .unwrap_or(&f.path)
            .display()
            .to_string()
    });

    let sample = sample_file.as_deref().unwrap_or("file.csv");

    println!("\x1b[1mNext steps:\x1b[0m");
    println!();

    if tag.is_some() {
        // Already tagged, show how to view and process
        println!("  \x1b[36mView tagged files:\x1b[0m casparian files --topic {}", tag.unwrap());
        println!("  \x1b[36mList all files:\x1b[0m    casparian files");
    } else {
        // Not tagged, show how to filter and tag
        println!("  \x1b[36mList all files:\x1b[0m    casparian files");
        println!("  \x1b[36mFilter by pattern:\x1b[0m casparian files -p \"*.csv\"");
        println!("  \x1b[36mFilter and tag:\x1b[0m    casparian files -p \"*.csv\" --tag mydata");
    }

    println!();
    println!(
        "  \x1b[36mPreview a file:\x1b[0m    casparian preview {}",
        sample
    );
    println!(
        "  \x1b[36mInteractive mode:\x1b[0m  casparian scan {} -i",
        result.scan_path.display()
    );
    println!();
}

/// Interactive file browser state
struct InteractiveState {
    files: Vec<DiscoveredFile>,
    scan_path: PathBuf,
    list_state: ListState,
    preview_content: Option<String>,
    running: bool,
}

impl InteractiveState {
    fn new(result: ScanResult) -> Self {
        let mut list_state = ListState::default();
        if !result.files.is_empty() {
            list_state.select(Some(0));
        }
        Self {
            files: result.files,
            scan_path: result.scan_path,
            list_state,
            preview_content: None,
            running: true,
        }
    }

    fn selected_file(&self) -> Option<&DiscoveredFile> {
        self.list_state.selected().and_then(|i| self.files.get(i))
    }

    fn next(&mut self) {
        if self.files.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => (i + 1).min(self.files.len() - 1),
            None => 0,
        };
        self.list_state.select(Some(i));
        self.update_preview();
    }

    fn previous(&mut self) {
        if self.files.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => i.saturating_sub(1),
            None => 0,
        };
        self.list_state.select(Some(i));
        self.update_preview();
    }

    fn update_preview(&mut self) {
        self.preview_content = self.selected_file().map(|file| {
            // Read first 40 lines of the file
            match std::fs::read_to_string(&file.path) {
                Ok(content) => {
                    let lines: Vec<&str> = content.lines().take(40).collect();
                    lines.join("\n")
                }
                Err(_) => {
                    // Try reading as binary
                    match std::fs::read(&file.path) {
                        Ok(bytes) => {
                            let preview_bytes = bytes.iter().take(500);
                            format!(
                                "[Binary file: {} bytes]\n\nHex preview:\n{}",
                                bytes.len(),
                                preview_bytes
                                    .take(256)
                                    .map(|b| format!("{:02x}", b))
                                    .collect::<Vec<_>>()
                                    .chunks(32)
                                    .map(|chunk| chunk.join(" "))
                                    .collect::<Vec<_>>()
                                    .join("\n")
                            )
                        }
                        Err(e) => format!("Cannot read file: {}", e),
                    }
                }
            }
        });
    }
}

/// Run interactive file browser
fn run_interactive(result: ScanResult, _tag: Option<String>) -> anyhow::Result<()> {
    if result.files.is_empty() {
        println!("No files found in: {}", result.scan_path.display());
        return Ok(());
    }

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    let mut state = InteractiveState::new(result);
    state.update_preview();

    // Main loop
    while state.running {
        terminal.draw(|frame| draw_interactive(frame, &mut state))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        state.running = false;
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        state.previous();
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        state.next();
                    }
                    KeyCode::Enter => {
                        // Run preview command
                        if let Some(file) = state.selected_file() {
                            // Exit TUI, run preview, then re-enter
                            disable_raw_mode()?;
                            execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

                            let _ = std::process::Command::new("casparian")
                                .args(["preview", &file.path.display().to_string()])
                                .status();

                            println!("\nPress Enter to continue...");
                            let _ = std::io::stdin().read_line(&mut String::new());

                            enable_raw_mode()?;
                            execute!(terminal.backend_mut(), EnterAlternateScreen)?;
                        }
                    }
                    KeyCode::Char('s') => {
                        // Show schema
                        if let Some(file) = state.selected_file() {
                            disable_raw_mode()?;
                            execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

                            let _ = std::process::Command::new("casparian")
                                .args(["preview", &file.path.display().to_string(), "--schema"])
                                .status();

                            println!("\nPress Enter to continue...");
                            let _ = std::io::stdin().read_line(&mut String::new());

                            enable_raw_mode()?;
                            execute!(terminal.backend_mut(), EnterAlternateScreen)?;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

/// Draw the interactive UI
fn draw_interactive(frame: &mut Frame, state: &mut InteractiveState) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(frame.area());

    // File list
    let items: Vec<ListItem> = state
        .files
        .iter()
        .map(|file| {
            let ext_value = file.extension.as_deref().unwrap_or("");
            let ext_display = if ext_value.is_empty() {
                "-".to_string()
            } else {
                ext_value.to_uppercase()
            };
            let ext_style = match ext_value {
                "csv" | "tsv" => Style::default().fg(ratatui::style::Color::Green),
                "json" | "jsonl" | "ndjson" => Style::default().fg(ratatui::style::Color::Yellow),
                "parquet" | "pq" => Style::default().fg(ratatui::style::Color::Magenta),
                _ => Style::default(),
            };

            let display_path = file
                .path
                .strip_prefix(&state.scan_path)
                .unwrap_or(&file.path)
                .display()
                .to_string();

            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{:<4} ", ext_display),
                    ext_style,
                ),
                Span::raw(format!("{:>8}  ", format_size(file.size))),
                Span::raw(display_path),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title(format!(" Files ({}) ", state.files.len()))
                .borders(Borders::ALL),
        )
        .highlight_style(Style::default().bg(ratatui::style::Color::DarkGray))
        .highlight_symbol("> ");

    frame.render_stateful_widget(list, chunks[0], &mut state.list_state);

    // Preview pane
    let preview_title = state
        .selected_file()
        .map(|f| format!(" {} ", f.name))
        .unwrap_or_else(|| " Preview ".to_string());

    let preview_content = state
        .preview_content
        .clone()
        .unwrap_or_else(|| "Select a file to preview".to_string());

    let preview = Paragraph::new(preview_content)
        .block(Block::default().title(preview_title).borders(Borders::ALL))
        .wrap(ratatui::widgets::Wrap { trim: false });

    frame.render_widget(preview, chunks[1]);

    // Help bar at bottom
    let help_area = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(frame.area())[1];

    let help = Paragraph::new(" j/k or arrows: navigate | Enter: preview | s: schema | q: quit ")
        .style(Style::default().fg(ratatui::style::Color::DarkGray));

    frame.render_widget(help, help_area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use std::path::Path;
    use std::sync::Mutex;
    use tempfile::TempDir;

    static TEST_ENV_LOCK: Mutex<()> = Mutex::new(());

    struct TestEnv {
        _lock: std::sync::MutexGuard<'static, ()>,
        _temp_dir: TempDir,
        prev_home: Option<String>,
    }

    impl TestEnv {
        fn new() -> Self {
            let lock = TEST_ENV_LOCK.lock().expect("test env lock");
            let temp_dir = TempDir::new().unwrap();
            let prev_home = std::env::var("CASPARIAN_HOME").ok();

            std::env::set_var("CASPARIAN_HOME", temp_dir.path());

            Self {
                _lock: lock,
                _temp_dir: temp_dir,
                prev_home,
            }
        }
    }

    impl Drop for TestEnv {
        fn drop(&mut self) {
            if let Some(home) = self.prev_home.take() {
                std::env::set_var("CASPARIAN_HOME", home);
            } else {
                std::env::remove_var("CASPARIAN_HOME");
            }
        }
    }

    fn create_test_files(dir: &Path) {
        // Create test files
        File::create(dir.join("test.csv"))
            .unwrap()
            .write_all(b"id,name\n1,foo")
            .unwrap();
        File::create(dir.join("data.json"))
            .unwrap()
            .write_all(b"{}")
            .unwrap();
        File::create(dir.join("readme.txt"))
            .unwrap()
            .write_all(b"Hello")
            .unwrap();

        // Create nested directory
        let nested = dir.join("nested");
        fs::create_dir_all(&nested).unwrap();
        File::create(nested.join("deep.csv"))
            .unwrap()
            .write_all(b"a,b\n1,2")
            .unwrap();
    }

    #[test]
    fn test_scan_basic() {
        let _env = TestEnv::new();
        let temp_dir = TempDir::new().unwrap();
        create_test_files(temp_dir.path());

        let args = ScanArgs {
            path: temp_dir.path().to_path_buf(),
            types: vec![],
            patterns: vec![],
            recursive: true,
            depth: None,
            min_size: None,
            max_size: None,
            json: false,
            stats: false,
            quiet: true,
            interactive: false,
            tag: None,
        };

        run(args).unwrap();
    }

    #[test]
    fn test_scan_type_filter() {
        let _env = TestEnv::new();
        let temp_dir = TempDir::new().unwrap();
        create_test_files(temp_dir.path());

        let args = ScanArgs {
            path: temp_dir.path().to_path_buf(),
            types: vec!["csv".to_string()],
            patterns: vec![],
            recursive: true,
            depth: None,
            min_size: None,
            max_size: None,
            json: false,
            stats: false,
            quiet: true,
            interactive: false,
            tag: None,
        };

        run(args).unwrap();
    }

    #[test]
    fn test_scan_non_recursive() {
        let _env = TestEnv::new();
        let temp_dir = TempDir::new().unwrap();
        create_test_files(temp_dir.path());

        let args = ScanArgs {
            path: temp_dir.path().to_path_buf(),
            types: vec![],
            patterns: vec![],
            recursive: false,
            depth: None,
            min_size: None,
            max_size: None,
            json: false,
            stats: false,
            quiet: true,
            interactive: false,
            tag: None,
        };

        run(args).unwrap();
    }

    #[test]
    fn test_scan_nonexistent_path() {
        let _env = TestEnv::new();
        let args = ScanArgs {
            path: PathBuf::from("/nonexistent/path/that/does/not/exist"),
            types: vec![],
            patterns: vec![],
            recursive: false,
            depth: None,
            min_size: None,
            max_size: None,
            json: false,
            stats: false,
            quiet: true,
            interactive: false,
            tag: None,
        };

        let result = run(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_scan_file_instead_of_dir() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        File::create(&file_path)
            .unwrap()
            .write_all(b"test")
            .unwrap();

        let args = ScanArgs {
            path: file_path,
            types: vec![],
            patterns: vec![],
            recursive: false,
            depth: None,
            min_size: None,
            max_size: None,
            json: false,
            stats: false,
            quiet: true,
            interactive: false,
            tag: None,
        };

        let result = run(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_build_summary() {
        let files = vec![
            DiscoveredFile {
                path: PathBuf::from("test.csv"),
                name: "test.csv".to_string(),
                extension: Some("csv".to_string()),
                size: 100,
                modified: SystemTime::now(),
            },
            DiscoveredFile {
                path: PathBuf::from("data.csv"),
                name: "data.csv".to_string(),
                extension: Some("csv".to_string()),
                size: 200,
                modified: SystemTime::now(),
            },
            DiscoveredFile {
                path: PathBuf::from("info.json"),
                name: "info.json".to_string(),
                extension: Some("json".to_string()),
                size: 50,
                modified: SystemTime::now(),
            },
        ];

        let summary = build_summary(&files, 5);

        assert_eq!(summary.total_files, 3);
        assert_eq!(summary.total_size, 350);
        assert_eq!(summary.files_by_type.get("csv"), Some(&2));
        assert_eq!(summary.files_by_type.get("json"), Some(&1));
        assert_eq!(summary.size_by_type.get("csv"), Some(&300));
        assert_eq!(summary.directories_scanned, 5);
    }
}
