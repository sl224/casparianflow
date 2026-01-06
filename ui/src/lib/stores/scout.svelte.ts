/**
 * Scout Store - File discovery and tagging state
 *
 * Scout v6.0: Tag-based model
 * - Scout discovers files and assigns tags via patterns
 * - Sentinel handles processing (Tag → Plugin → Sink)
 *
 * File status flow: pending → tagged → queued → processing → processed/failed
 */

import { invoke } from "$lib/tauri";

// ============================================================================
// Types (matching Rust backend - tag-based model v6.0)
// ============================================================================

export interface Source {
  id: string;
  name: string;
  path: string;
  pollIntervalSecs: number;
  enabled: boolean;
}

export interface ScannedFile {
  id: number;
  sourceId: string;
  path: string;
  relPath: string;
  size: number;
  status: FileStatus;
  tag: string | null;
  /** How the tag was assigned: "rule" or "manual" */
  tagSource: "rule" | "manual" | null;
  /** ID of the tagging rule that matched (if tagSource = "rule") */
  ruleId: string | null;
  /** Manual plugin override (null = use tag subscription) */
  manualPlugin: string | null;
  error: string | null;
  sentinelJobId: number | null;
}

/** Filter types for file list */
export type FilterType = "all" | "manual" | "pending" | "tagged" | "queued" | "processed" | "failed";

export type FileStatus =
  | "pending"     // Discovered, awaiting tagging
  | "tagged"      // Has tag, ready for processing
  | "queued"      // Submitted to Sentinel
  | "processing"  // Worker is processing
  | "processed"   // Success
  | "failed"      // Error (with message)
  | "skipped"     // User skipped
  | "deleted";    // Removed from source

export interface TaggingRule {
  id: string;
  name: string;
  sourceId: string;
  pattern: string;
  tag: string;
  priority: number;
  enabled: boolean;
}

export interface ScanStats {
  filesDiscovered: number;
  filesNew: number;
  filesChanged: number;
  filesDeleted: number;
  bytesScanned: number;
  durationMs: number;
  errors: string[];
}

export interface PatternPreview {
  pattern: string;
  matchedCount: number;
  matchedBytes: number;
  sampleFiles: string[];
  isValid: boolean;
  error: string | null;
}

export interface ScoutStatus {
  sources: number;
  taggingRules: number;
  totalFiles: number;
  pendingFiles: number;
  taggedFiles: number;
  queuedFiles: number;
  processingFiles: number;
  processedFiles: number;
  failedFiles: number;
  pendingBytes: number;
  taggedBytes: number;
  processedBytes: number;
}

// Tag coverage analysis - understand how files are being tagged
export interface TagCoverage {
  rules: TagCoverageStats[];
  untaggedCount: number;
  untaggedBytes: number;
  untaggedSamples: string[];
  overlaps: TagOverlap[];
  totalFiles: number;
  totalBytes: number;
  taggedFiles: number;
  taggedBytes: number;
}

export interface TagCoverageStats {
  ruleId: string;
  ruleName: string;
  tag: string;
  pattern: string;
  matchedCount: number;
  matchedBytes: number;
  sampleFiles: string[];
}

export interface TagOverlap {
  rule1Id: string;
  rule1Name: string;
  rule2Id: string;
  rule2Name: string;
  overlapCount: number;
  sampleFiles: string[];
}

// Tag stats from database
export interface TagStats {
  tag: string;
  fileCount: number;
  totalBytes: number;
  pendingCount: number;
  taggedCount: number;
  queuedCount: number;
  processingCount: number;
  processedCount: number;
  failedCount: number;
}

// Failed files with error details
export interface FailedFile {
  id: number;
  path: string;
  relPath: string;
  size: number;
  tag: string | null;
  error: string;
}

// Result of submitting tagged files to Sentinel
export interface SubmitResult {
  submitted: number;
  skipped: number;
  jobIds: Array<[number, number]>; // [file_id, job_id]
  noPlugin: Array<[number, string]>; // [file_id, tag]
}

// ============================================================================
// Store
// ============================================================================

/** Debounce helper */
function debounce<Args extends unknown[]>(
  fn: (...args: Args) => void,
  ms: number
): (...args: Args) => void {
  let timeoutId: ReturnType<typeof setTimeout> | null = null;
  return (...args: Args) => {
    if (timeoutId) clearTimeout(timeoutId);
    timeoutId = setTimeout(() => fn(...args), ms);
  };
}

/** Format bytes as human-readable string */
export function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
}

class ScoutStore {
  // Data state
  sources = $state<Source[]>([]);
  files = $state<ScannedFile[]>([]);
  taggingRules = $state<TaggingRule[]>([]);
  status = $state<ScoutStatus | null>(null);
  tagStats = $state<TagStats[]>([]);

  // Selection state
  selectedSourceId = $state<string | null>(null);

  // File selection state (for detail pane)
  selectedFileId = $state<number | null>(null);
  selectedFileIds = $state<Set<number>>(new Set());

  // Filter state
  currentFilter = $state<FilterType>("all");

  // Live preview state
  previewPattern = $state("");
  previewResult = $state<PatternPreview | null>(null);
  previewLoading = $state(false);

  // Operation state
  loading = $state(false);
  scanning = $state(false);
  tagging = $state(false);
  error = $state<string | null>(null);

  // Last operation results
  lastScanStats = $state<ScanStats | null>(null);

  // Tag coverage analysis
  coverage = $state<TagCoverage | null>(null);
  coverageLoading = $state(false);

  // Failed files
  failedFilesList = $state<FailedFile[]>([]);

  private debouncedPreview: (sourceId: string, pattern: string) => void;

  constructor() {
    // 150ms debounce for live preview
    this.debouncedPreview = debounce(
      (sourceId: string, pattern: string) => this.fetchPatternPreview(sourceId, pattern),
      150
    );
  }

  // --------------------------------------------------------------------------
  // Database Initialization
  // --------------------------------------------------------------------------

  async initDb(path?: string): Promise<void> {
    try {
      await invoke("scout_init_db", { path });
      console.log("[ScoutStore] Database initialized");
    } catch (err) {
      this.error = `Failed to initialize database: ${err}`;
      throw err;
    }
  }

  // --------------------------------------------------------------------------
  // Sources
  // --------------------------------------------------------------------------

  async loadSources(): Promise<void> {
    this.loading = true;
    this.error = null;

    try {
      this.sources = await invoke<Source[]>("scout_list_sources");
      console.log("[ScoutStore] Loaded", this.sources.length, "sources");
    } catch (err) {
      this.error = `Failed to load sources: ${err}`;
      console.error("[ScoutStore]", this.error);
    } finally {
      this.loading = false;
    }
  }

  async addSource(id: string, name: string, path: string, pollIntervalSecs?: number): Promise<Source> {
    this.error = null;

    try {
      const source = await invoke<Source>("scout_add_source", {
        id,
        name,
        path,
        pollIntervalSecs,
      });
      this.sources = [...this.sources, source];
      console.log("[ScoutStore] Added source:", id);
      return source;
    } catch (err) {
      this.error = `Failed to add source: ${err}`;
      throw err;
    }
  }

  async removeSource(id: string): Promise<boolean> {
    this.error = null;

    try {
      const removed = await invoke<boolean>("scout_remove_source", { id });
      if (removed) {
        this.sources = this.sources.filter(s => s.id !== id);
        if (this.selectedSourceId === id) {
          this.selectedSourceId = null;
          this.files = [];
          this.taggingRules = [];
        }
        console.log("[ScoutStore] Removed source:", id);
      }
      return removed;
    } catch (err) {
      this.error = `Failed to remove source: ${err}`;
      throw err;
    }
  }

  selectSource(id: string | null): void {
    this.selectedSourceId = id;
    this.files = [];
    this.taggingRules = [];
    this.previewPattern = "";
    this.previewResult = null;
    this.failedFilesList = [];
    this.tagStats = [];
    this.coverage = null;

    if (id) {
      this.loadFiles(id);
      this.loadTaggingRulesForSource(id);
      this.loadCoverage(id);
      this.loadTagStats();
      this.loadFailedFiles(id);
    }
  }

  // --------------------------------------------------------------------------
  // Scanning
  // --------------------------------------------------------------------------

  async scan(sourceId: string): Promise<ScanStats> {
    this.scanning = true;
    this.error = null;
    this.lastScanStats = null;

    try {
      console.log("[ScoutStore] Starting scan for source:", sourceId);
      const stats = await invoke<ScanStats>("scout_scan_source", { sourceId });
      this.lastScanStats = stats;
      console.log("[ScoutStore] Scan complete:", stats.filesDiscovered, "files");

      // Reload files and coverage after scan
      console.log("[ScoutStore] Loading files...");
      await this.loadFiles(sourceId);
      console.log("[ScoutStore] Files loaded:", this.files.length);

      console.log("[ScoutStore] Loading status...");
      await this.loadStatus();

      console.log("[ScoutStore] Loading coverage...");
      await this.loadCoverage(sourceId);

      console.log("[ScoutStore] All post-scan loading complete");
      return stats;
    } catch (err) {
      console.error("[ScoutStore] Scan error:", err);
      this.error = `Scan failed: ${err}`;
      throw err;
    } finally {
      this.scanning = false;
    }
  }

  // --------------------------------------------------------------------------
  // Files
  // --------------------------------------------------------------------------

  async loadFiles(sourceId: string, status?: string, limit?: number): Promise<void> {
    this.loading = true;
    this.error = null;

    try {
      this.files = await invoke<ScannedFile[]>("scout_list_files", {
        sourceId,
        status,
        limit,
      });
      console.log("[ScoutStore] Loaded", this.files.length, "files");
    } catch (err) {
      this.error = `Failed to load files: ${err}`;
      console.error("[ScoutStore]", this.error);
    } finally {
      this.loading = false;
    }
  }

  async loadFilesByTag(tag: string, limit?: number): Promise<void> {
    this.loading = true;
    this.error = null;

    try {
      this.files = await invoke<ScannedFile[]>("scout_list_files_by_tag", {
        tag,
        limit,
      });
      console.log("[ScoutStore] Loaded", this.files.length, "files with tag:", tag);
    } catch (err) {
      this.error = `Failed to load files by tag: ${err}`;
      console.error("[ScoutStore]", this.error);
    } finally {
      this.loading = false;
    }
  }

  async loadUntaggedFiles(sourceId: string, limit?: number): Promise<void> {
    this.loading = true;
    this.error = null;

    try {
      this.files = await invoke<ScannedFile[]>("scout_list_untagged_files", {
        sourceId,
        limit,
      });
      console.log("[ScoutStore] Loaded", this.files.length, "untagged files");
    } catch (err) {
      this.error = `Failed to load untagged files: ${err}`;
      console.error("[ScoutStore]", this.error);
    } finally {
      this.loading = false;
    }
  }

  // --------------------------------------------------------------------------
  // Tagging Rules
  // --------------------------------------------------------------------------

  async loadTaggingRules(): Promise<void> {
    this.loading = true;
    this.error = null;

    try {
      this.taggingRules = await invoke<TaggingRule[]>("scout_list_tagging_rules");
      console.log("[ScoutStore] Loaded", this.taggingRules.length, "tagging rules");
    } catch (err) {
      this.error = `Failed to load tagging rules: ${err}`;
      console.error("[ScoutStore]", this.error);
    } finally {
      this.loading = false;
    }
  }

  async loadTaggingRulesForSource(sourceId: string): Promise<void> {
    this.loading = true;
    this.error = null;

    try {
      this.taggingRules = await invoke<TaggingRule[]>("scout_list_tagging_rules_for_source", { sourceId });
      console.log("[ScoutStore] Loaded", this.taggingRules.length, "tagging rules for source", sourceId);
    } catch (err) {
      this.error = `Failed to load tagging rules: ${err}`;
      console.error("[ScoutStore]", this.error);
    } finally {
      this.loading = false;
    }
  }

  async addTaggingRule(
    id: string,
    name: string,
    sourceId: string,
    pattern: string,
    tag: string,
    priority?: number
  ): Promise<TaggingRule> {
    this.error = null;

    try {
      const rule = await invoke<TaggingRule>("scout_add_tagging_rule", {
        id,
        name,
        sourceId,
        pattern,
        tag,
        priority,
      });
      this.taggingRules = [...this.taggingRules, rule];
      console.log("[ScoutStore] Added tagging rule:", id);

      // Refresh coverage after rule change
      if (this.selectedSourceId) {
        await this.loadCoverage(this.selectedSourceId);
      }

      return rule;
    } catch (err) {
      this.error = `Failed to add tagging rule: ${err}`;
      throw err;
    }
  }

  async removeTaggingRule(id: string): Promise<boolean> {
    this.error = null;

    try {
      const removed = await invoke<boolean>("scout_remove_tagging_rule", { id });
      if (removed) {
        this.taggingRules = this.taggingRules.filter(r => r.id !== id);
        console.log("[ScoutStore] Removed tagging rule:", id);

        // Refresh coverage after rule change
        if (this.selectedSourceId) {
          await this.loadCoverage(this.selectedSourceId);
        }
      }
      return removed;
    } catch (err) {
      this.error = `Failed to remove tagging rule: ${err}`;
      throw err;
    }
  }

  // --------------------------------------------------------------------------
  // Tag Coverage Analysis
  // --------------------------------------------------------------------------

  async loadCoverage(sourceId: string): Promise<void> {
    this.coverageLoading = true;

    try {
      this.coverage = await invoke<TagCoverage>("scout_analyze_coverage", { sourceId });
      console.log(
        "[ScoutStore] Coverage:",
        this.coverage.taggedFiles, "tagged,",
        this.coverage.untaggedCount, "untagged,",
        this.coverage.overlaps.length, "overlaps"
      );
    } catch (err) {
      console.error("[ScoutStore] Failed to load coverage:", err);
      this.coverage = null;
    } finally {
      this.coverageLoading = false;
    }
  }

  // --------------------------------------------------------------------------
  // Tag Stats
  // --------------------------------------------------------------------------

  async loadTagStats(): Promise<void> {
    try {
      this.tagStats = await invoke<TagStats[]>("scout_tag_stats");
      console.log("[ScoutStore] Loaded stats for", this.tagStats.length, "tags");
    } catch (err) {
      console.error("[ScoutStore] Failed to load tag stats:", err);
      this.tagStats = [];
    }
  }

  // --------------------------------------------------------------------------
  // Tagging Operations
  // --------------------------------------------------------------------------

  /** Tag a single file manually (sets tagSource = 'manual') */
  async tagFile(fileId: number, tag: string): Promise<void> {
    this.error = null;

    try {
      // Use scout_tag_files (plural) - backend only has batch version
      await invoke("scout_tag_files", { fileIds: [fileId], tag });
      // Update local state - manual tagging sets tagSource to 'manual'
      this.files = this.files.map(f =>
        f.id === fileId ? { ...f, tag, tagSource: "manual" as const, ruleId: null, status: "tagged" as FileStatus } : f
      );
      console.log("[ScoutStore] Tagged file", fileId, "with", tag);
    } catch (err) {
      this.error = `Failed to tag file: ${err}`;
      throw err;
    }
  }

  /** Tag multiple files manually (sets tagSource = 'manual') */
  async tagFiles(fileIds: number[], tag: string): Promise<number> {
    this.error = null;
    this.tagging = true;

    try {
      const count = await invoke<number>("scout_tag_files", { fileIds, tag });
      // Update local state - manual tagging sets tagSource to 'manual'
      const idSet = new Set(fileIds);
      this.files = this.files.map(f =>
        idSet.has(f.id) ? { ...f, tag, tagSource: "manual" as const, ruleId: null, status: "tagged" as FileStatus } : f
      );
      console.log("[ScoutStore] Tagged", count, "files with", tag);
      return count;
    } catch (err) {
      this.error = `Failed to tag files: ${err}`;
      throw err;
    } finally {
      this.tagging = false;
    }
  }

  /** Auto-tag all pending files using tagging rules */
  async autoTag(sourceId: string): Promise<number> {
    this.error = null;
    this.tagging = true;

    try {
      const count = await invoke<number>("scout_auto_tag", { sourceId });
      console.log("[ScoutStore] Auto-tagged", count, "files");

      // Reload files to see updated tags
      await this.loadFiles(sourceId);
      await this.loadCoverage(sourceId);
      await this.loadTagStats();

      return count;
    } catch (err) {
      this.error = `Auto-tagging failed: ${err}`;
      throw err;
    } finally {
      this.tagging = false;
    }
  }

  // --------------------------------------------------------------------------
  // Live Pattern Preview
  // --------------------------------------------------------------------------

  updatePreviewPattern(pattern: string): void {
    this.previewPattern = pattern;

    if (!pattern || !this.selectedSourceId) {
      this.previewResult = null;
      return;
    }

    this.debouncedPreview(this.selectedSourceId, pattern);
  }

  private async fetchPatternPreview(sourceId: string, pattern: string): Promise<void> {
    this.previewLoading = true;

    try {
      this.previewResult = await invoke<PatternPreview>("scout_preview_pattern", {
        sourceId,
        pattern,
      });
    } catch (err) {
      this.previewResult = {
        pattern,
        matchedCount: 0,
        matchedBytes: 0,
        sampleFiles: [],
        isValid: false,
        error: String(err),
      };
    } finally {
      this.previewLoading = false;
    }
  }

  // --------------------------------------------------------------------------
  // Failed Files
  // --------------------------------------------------------------------------

  async loadFailedFiles(sourceId: string): Promise<void> {
    try {
      this.failedFilesList = await invoke<FailedFile[]>("scout_list_failed_files", { sourceId, limit: 50 });
      console.log("[ScoutStore] Loaded", this.failedFilesList.length, "failed files");
    } catch (err) {
      console.error("[ScoutStore] Failed to load failed files:", err);
      this.failedFilesList = [];
    }
  }

  // --------------------------------------------------------------------------
  // Status
  // --------------------------------------------------------------------------

  async loadStatus(): Promise<void> {
    try {
      this.status = await invoke<ScoutStatus>("scout_status");
    } catch (err) {
      console.error("[ScoutStore] Failed to load status:", err);
    }
  }

  // --------------------------------------------------------------------------
  // Scout-Sentinel Bridge
  // --------------------------------------------------------------------------

  submitting = $state(false);
  lastSubmitResult = $state<SubmitResult | null>(null);

  /** Submit tagged files to Sentinel for processing */
  async submitTaggedFiles(fileIds: number[]): Promise<SubmitResult> {
    this.submitting = true;
    this.error = null;
    this.lastSubmitResult = null;

    try {
      const result = await invoke<SubmitResult>("submit_tagged_files", { fileIds });
      this.lastSubmitResult = result;
      console.log(
        "[ScoutStore] Submitted",
        result.submitted,
        "files,",
        result.skipped,
        "skipped"
      );

      // Update local file status to queued
      const queuedIds = new Set(result.jobIds.map(([fileId]) => fileId));
      this.files = this.files.map((f) =>
        queuedIds.has(f.id) ? { ...f, status: "queued" as FileStatus } : f
      );

      // Spawn worker processes for each job
      for (const [, jobId] of result.jobIds) {
        try {
          await invoke("process_job_async", { jobId });
          console.log("[ScoutStore] Spawned worker for job", jobId);
        } catch (err) {
          console.error("[ScoutStore] Failed to spawn worker for job", jobId, err);
          // Don't fail the whole submission if spawning fails
        }
      }

      // Reload stats
      await this.loadTagStats();

      return result;
    } catch (err) {
      this.error = `Failed to submit files: ${err}`;
      throw err;
    } finally {
      this.submitting = false;
    }
  }

  /** Submit all tagged files for a source */
  async submitAllTagged(sourceId: string): Promise<SubmitResult> {
    const taggedFileIds = this.files
      .filter((f) => f.sourceId === sourceId && f.status === "tagged")
      .map((f) => f.id);

    if (taggedFileIds.length === 0) {
      return { submitted: 0, skipped: 0, jobIds: [], noPlugin: [] };
    }

    return this.submitTaggedFiles(taggedFileIds);
  }

  /** Get plugins available for a tag */
  async getPluginsForTag(tag: string): Promise<string[]> {
    try {
      return await invoke<string[]>("get_plugins_for_tag", { tag });
    } catch (err) {
      console.error("[ScoutStore] Failed to get plugins for tag:", err);
      return [];
    }
  }

  // --------------------------------------------------------------------------
  // Status Sync (Sentinel -> Scout)
  // --------------------------------------------------------------------------

  /** Current source ID for status sync */
  get currentSourceId(): string | null {
    return this.selectedSourceId;
  }

  /** Sync file statuses from Sentinel job statuses */
  async syncStatuses(): Promise<void> {
    try {
      const updated = await invoke<number>("sync_scout_file_statuses");
      if (updated > 0) {
        console.log("[ScoutStore] Synced", updated, "file statuses from Sentinel");
        // Reload files to reflect updated statuses
        if (this.currentSourceId) {
          await this.loadFiles(this.currentSourceId);
        }
      }
    } catch (err) {
      // Don't show error to user - this is a background sync
      console.error("[ScoutStore] Failed to sync statuses:", err);
    }
  }

  // --------------------------------------------------------------------------
  // File Selection & Detail
  // --------------------------------------------------------------------------

  /** Select a file to show in detail pane */
  selectFile(fileId: number | null): void {
    this.selectedFileId = fileId;
  }

  /** Clear file selection */
  clearFileSelection(): void {
    this.selectedFileId = null;
    this.selectedFileIds = new Set();
  }

  /** Toggle file selection for bulk operations */
  toggleFileSelection(fileId: number): void {
    const newSet = new Set(this.selectedFileIds);
    if (newSet.has(fileId)) {
      newSet.delete(fileId);
    } else {
      newSet.add(fileId);
    }
    this.selectedFileIds = newSet;
  }

  /** Select all visible files */
  selectAllFiles(): void {
    this.selectedFileIds = new Set(this.filteredFiles.map(f => f.id));
  }

  /** Get a single file by ID */
  async getFile(fileId: number): Promise<ScannedFile | null> {
    try {
      return await invoke<ScannedFile | null>("scout_get_file", { fileId });
    } catch (err) {
      console.error("[ScoutStore] Failed to get file:", err);
      return null;
    }
  }

  // --------------------------------------------------------------------------
  // Manual Override Operations
  // --------------------------------------------------------------------------

  /** Set manual plugin override for a file */
  async setManualPlugin(fileId: number, pluginName: string): Promise<void> {
    this.error = null;

    try {
      await invoke("scout_set_manual_plugin", { fileId, pluginName });
      // Update local state
      this.files = this.files.map(f =>
        f.id === fileId ? { ...f, manualPlugin: pluginName } : f
      );
      console.log("[ScoutStore] Set manual plugin", pluginName, "for file", fileId);
    } catch (err) {
      this.error = `Failed to set manual plugin: ${err}`;
      throw err;
    }
  }

  /** Clear all manual overrides for a file (reset to auto) */
  async clearManualOverrides(fileId: number): Promise<void> {
    this.error = null;

    try {
      await invoke("scout_clear_manual_overrides", { fileId });
      // Update local state
      this.files = this.files.map(f =>
        f.id === fileId
          ? { ...f, tag: null, tagSource: null, ruleId: null, manualPlugin: null, status: "pending" as FileStatus }
          : f
      );
      console.log("[ScoutStore] Cleared manual overrides for file", fileId);
    } catch (err) {
      this.error = `Failed to clear manual overrides: ${err}`;
      throw err;
    }
  }

  /** Load files with manual overrides */
  async loadManualFiles(sourceId: string, limit?: number): Promise<void> {
    this.loading = true;
    this.error = null;

    try {
      this.files = await invoke<ScannedFile[]>("scout_list_manual_files", {
        sourceId,
        limit,
      });
      console.log("[ScoutStore] Loaded", this.files.length, "manual files");
    } catch (err) {
      this.error = `Failed to load manual files: ${err}`;
      console.error("[ScoutStore]", this.error);
    } finally {
      this.loading = false;
    }
  }

  // --------------------------------------------------------------------------
  // Filtering
  // --------------------------------------------------------------------------

  /** Set the current filter */
  setFilter(filter: FilterType): void {
    this.currentFilter = filter;
  }

  /** Filter files based on current filter */
  private filterFilesByType(files: ScannedFile[], filter: FilterType): ScannedFile[] {
    switch (filter) {
      case "all":
        return files;
      case "manual":
        return files.filter(f => f.tagSource === "manual" || f.manualPlugin !== null);
      case "pending":
        return files.filter(f => f.status === "pending");
      case "tagged":
        return files.filter(f => f.status === "tagged");
      case "queued":
        return files.filter(f => f.status === "queued");
      case "processed":
        return files.filter(f => f.status === "processed");
      case "failed":
        return files.filter(f => f.status === "failed");
      default:
        return files;
    }
  }

  // --------------------------------------------------------------------------
  // Computed properties (getters are reactive when accessing $state)
  // --------------------------------------------------------------------------

  get selectedSource(): Source | null {
    return this.sources.find(s => s.id === this.selectedSourceId) ?? null;
  }

  /** Get the currently selected file for detail pane */
  get selectedFile(): ScannedFile | null {
    return this.files.find(f => f.id === this.selectedFileId) ?? null;
  }

  /** Get files filtered by current filter */
  get filteredFiles(): ScannedFile[] {
    return this.filterFilesByType(this.files, this.currentFilter);
  }

  /** Get files with manual overrides (tag or plugin) */
  get manualFiles(): ScannedFile[] {
    return this.files.filter(f => f.tagSource === "manual" || f.manualPlugin !== null);
  }

  /** Check if a file has any manual override */
  isManualFile(file: ScannedFile): boolean {
    return file.tagSource === "manual" || file.manualPlugin !== null;
  }

  get pendingFiles(): ScannedFile[] {
    return this.files.filter(f => f.status === "pending");
  }

  get taggedFiles(): ScannedFile[] {
    return this.files.filter(f => f.status === "tagged");
  }

  get queuedFiles(): ScannedFile[] {
    return this.files.filter(f => f.status === "queued");
  }

  get processingFilesList(): ScannedFile[] {
    return this.files.filter(f => f.status === "processing");
  }

  get processedFiles(): ScannedFile[] {
    return this.files.filter(f => f.status === "processed");
  }

  get failedFiles(): ScannedFile[] {
    return this.files.filter(f => f.status === "failed");
  }

  get hasTaggingRules(): boolean {
    return this.taggingRules.length > 0;
  }

  get canAutoTag(): boolean {
    return this.hasTaggingRules && this.pendingFiles.length > 0 && !this.tagging;
  }

  /** Get unique tags from tagging rules */
  get availableTags(): string[] {
    const tags = new Set(this.taggingRules.map(r => r.tag));
    return Array.from(tags).sort();
  }
}

// Singleton instance
export const scoutStore = new ScoutStore();
