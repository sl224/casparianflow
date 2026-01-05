<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import ParserRefinementChat from "./ParserRefinementChat.svelte";
  import ShredderChat from "./ShredderChat.svelte";

  // Analysis result types (from Rust backend)
  interface ShredStrategyInfo {
    strategyType: string;
    delimiter?: string;
    colIndex?: number;
    hasHeader?: boolean;
    keyPath?: string;
    pattern?: string;
    keyGroup?: string;
  }

  interface ShredAnalysisResult {
    strategy: ShredStrategyInfo;
    confidence: string;
    sampleKeys: string[];
    estimatedShardCount: number;
    headBytes: number;
    reasoning: string;
    warning?: string;
  }

  interface FullAnalysisInfo {
    keyCounts: [string, number][];
    totalRows: number;
    bytesScanned: number;
    durationMs: number;
  }

  interface ShardInfo {
    path: string;
    key: string;
    rowCount: number;
    byteSize: number;
  }

  interface ShredResultInfo {
    shards: ShardInfo[];
    freezerPath?: string;
    freezerKeyCount: number;
    totalRows: number;
    durationMs: number;
    lineageIndexPath: string;
  }

  // State
  let filePath = $state("");
  let outputDir = $state("");
  let defaultOutputDir = $state(""); // App data directory
  let isAnalyzing = $state(false);
  let isFullAnalyzing = $state(false);
  let isShredding = $state(false);
  let analysisResult = $state<ShredAnalysisResult | null>(null);
  let fullAnalysis = $state<FullAnalysisInfo | null>(null);
  let shredResult = $state<ShredResultInfo | null>(null);
  let error = $state<string | null>(null);

  // Parser generator state
  let showParserGenerator = $state(false);
  let selectedShard = $state<ShardInfo | null>(null);

  // Shard preview state
  let previewingShard = $state<string | null>(null);
  let shardPreview = $state<string[] | null>(null);

  // Chat mode state - for interactive LLM analysis
  let showChat = $state(false);

  // Load default output directory on mount
  onMount(async () => {
    try {
      defaultOutputDir = await invoke<string>("get_shredder_output_dir");
      outputDir = defaultOutputDir;
    } catch (e) {
      console.error("Failed to get default output dir:", e);
    }
  });

  // Open parser generator for a shard
  function openParserGenerator(shard: ShardInfo) {
    selectedShard = shard;
    showParserGenerator = true;
  }

  // Close parser generator
  function closeParserGenerator() {
    showParserGenerator = false;
    selectedShard = null;
  }

  // Preview shard contents
  async function handlePreviewShard(shard: ShardInfo) {
    if (previewingShard === shard.path) {
      // Toggle off
      previewingShard = null;
      shardPreview = null;
      return;
    }

    try {
      const rows = await invoke<string[]>("preview_shard", {
        path: shard.path,
        numRows: 5
      });
      previewingShard = shard.path;
      shardPreview = rows;
    } catch (e) {
      error = e as string;
    }
  }

  // Configuration (user can modify after analysis)
  let colIndex = $state(0);
  let delimiter = $state(",");
  let hasHeader = $state(true);
  let topN = $state<number | null>(null);

  // Select file via dialog
  async function handleSelectFile() {
    // No filters - allow any file since many data files have no extension
    const selected = await open({
      multiple: false,
      title: "Select file to analyze"
    });

    if (selected) {
      filePath = selected as string;
      analysisResult = null;
      fullAnalysis = null;
      shredResult = null;
      error = null;
    }
  }

  // Select output directory
  async function handleSelectOutputDir() {
    const selected = await save({
      title: "Select output directory",
      defaultPath: outputDir || defaultOutputDir
    });

    if (selected) {
      outputDir = selected as string;
    }
  }

  // Open chat interface for interactive LLM analysis
  function handleAnalyze() {
    if (!filePath) return;
    error = null;
    analysisResult = null;
    fullAnalysis = null;
    shredResult = null;
    showChat = true;  // Show chat interface
  }

  // Handle strategy from chat - when user accepts LLM proposal
  function handleStrategyFromChat(strategy: ShredStrategyInfo, reasoning: string) {
    // Pre-populate config from strategy
    if (strategy.colIndex !== undefined) {
      colIndex = strategy.colIndex;
    }
    if (strategy.delimiter) {
      delimiter = strategy.delimiter;
    }
    if (strategy.hasHeader !== undefined) {
      hasHeader = strategy.hasHeader;
    }

    // Create a synthetic analysis result
    analysisResult = {
      strategy,
      confidence: "High",  // User-confirmed through conversation
      sampleKeys: [],
      estimatedShardCount: 0,
      headBytes: 0,
      reasoning,
      warning: undefined
    };

    // Use app data directory for output
    if (!outputDir || outputDir === defaultOutputDir) {
      const inputName = filePath.split("/").pop()?.replace(/\.[^/.]+$/, "") || "shards";
      outputDir = defaultOutputDir + "/" + inputName;
    }

    // Close chat
    showChat = false;
  }

  // Close chat without accepting strategy
  function handleCloseChat() {
    showChat = false;
  }

  // Full file analysis - scans entire file for accurate key counts
  async function handleFullAnalysis() {
    if (!filePath || !analysisResult) return;

    isFullAnalyzing = true;
    error = null;

    try {
      const result = await invoke<FullAnalysisInfo>("shredder_analyze_full", {
        path: filePath,
        colIndex,
        delimiter,
        hasHeader
      });
      fullAnalysis = result;
    } catch (e) {
      error = e as string;
    } finally {
      isFullAnalyzing = false;
    }
  }

  // Execute shredding
  async function handleShred() {
    if (!filePath || !outputDir) return;

    isShredding = true;
    error = null;
    shredResult = null;

    try {
      const result = await invoke<ShredResultInfo>("shredder_run", {
        path: filePath,
        outputDir: outputDir,
        colIndex: colIndex,
        delimiter: delimiter,
        hasHeader: hasHeader,
        topN: topN
      });
      shredResult = result;
    } catch (e) {
      error = e as string;
    } finally {
      isShredding = false;
    }
  }

  // Reset state
  function handleReset() {
    filePath = "";
    outputDir = "";
    analysisResult = null;
    shredResult = null;
    error = null;
    colIndex = 0;
    delimiter = ",";
    hasHeader = true;
    topN = null;
  }

  // Format bytes
  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  }

  // Get confidence color
  function getConfidenceColor(confidence: string): string {
    switch (confidence) {
      case "High": return "var(--color-success)";
      case "Medium": return "var(--color-warning, #ffa500)";
      case "Low": return "var(--color-error)";
      default: return "var(--color-text-muted)";
    }
  }
</script>

<div class="shredder-tab">
  <!-- Header -->
  <div class="header">
    <h2 class="title">SHREDDER - Multiplexed File Splitter</h2>
  </div>

  <!-- File Selection -->
  <div class="section">
    <div class="section-header">
      <span class="section-title">1. SELECT FILE</span>
    </div>
    <div class="input-row">
      <input
        type="text"
        class="file-input"
        placeholder="Enter file path or click Browse..."
        bind:value={filePath}
      />
      <button class="action-btn" onclick={handleSelectFile}>Browse</button>
      <button
        class="action-btn primary"
        onclick={handleAnalyze}
        disabled={!filePath || isAnalyzing}
      >
        {isAnalyzing ? "Analyzing..." : "Analyze"}
      </button>
    </div>
  </div>

  <!-- Analysis Result -->
  {#if analysisResult}
    <div class="section">
      <div class="section-header">
        <span class="section-title">2. ANALYSIS RESULT</span>
        <span
          class="confidence-badge"
          style="color: {getConfidenceColor(analysisResult.confidence)}"
        >
          {analysisResult.confidence} Confidence
        </span>
      </div>

      <div class="analysis-card">
        <div class="analysis-row">
          <span class="analysis-label">Strategy:</span>
          <span class="analysis-value">{analysisResult.strategy.strategyType}</span>
        </div>

        {#if analysisResult.strategy.strategyType === "CsvColumn"}
          <div class="analysis-row">
            <span class="analysis-label">Delimiter:</span>
            <span class="analysis-value mono">
              {analysisResult.strategy.delimiter === "\t" ? "TAB" : `"${analysisResult.strategy.delimiter}"`}
            </span>
          </div>
          <div class="analysis-row">
            <span class="analysis-label">Shard Column:</span>
            <span class="analysis-value">Column {analysisResult.strategy.colIndex}</span>
          </div>
          <div class="analysis-row">
            <span class="analysis-label">Has Header:</span>
            <span class="analysis-value">{analysisResult.strategy.hasHeader ? "Yes" : "No"}</span>
          </div>
        {:else if analysisResult.strategy.strategyType === "JsonKey"}
          <div class="analysis-row">
            <span class="analysis-label">Key Path:</span>
            <span class="analysis-value mono">{analysisResult.strategy.keyPath}</span>
          </div>
        {/if}

        <div class="analysis-row">
          <span class="analysis-label">Estimated Shards:</span>
          <span class="analysis-value highlight">{analysisResult.estimatedShardCount}</span>
        </div>

        <div class="analysis-row">
          <span class="analysis-label">Analyzed:</span>
          <span class="analysis-value">{formatBytes(analysisResult.headBytes)}</span>
        </div>

        <div class="reasoning">
          <span class="reasoning-text">{analysisResult.reasoning}</span>
        </div>

        {#if analysisResult.warning}
          <div class="warning-box">
            <span class="warning-icon">!</span>
            <span class="warning-text">{analysisResult.warning}</span>
          </div>
        {/if}

        <!-- Sample Keys -->
        {#if analysisResult.sampleKeys.length > 0}
          <div class="sample-keys">
            <span class="sample-keys-title">Sample Keys Found (from {formatBytes(analysisResult.headBytes)} sample):</span>
            <div class="keys-list">
              {#each analysisResult.sampleKeys.slice(0, 10) as key}
                <span class="key-tag">{key}</span>
              {/each}
              {#if analysisResult.sampleKeys.length > 10}
                <span class="key-more">+{analysisResult.sampleKeys.length - 10} more</span>
              {/if}
            </div>
          </div>
        {/if}

        <!-- Full File Analysis -->
        <div class="full-analysis-section">
          <button
            class="action-btn"
            onclick={handleFullAnalysis}
            disabled={isFullAnalyzing}
          >
            {isFullAnalyzing ? "Scanning..." : "Scan Entire File"}
          </button>
          <span class="hint">Get accurate key counts for large files</span>

          {#if fullAnalysis}
            <div class="full-analysis-result">
              <div class="full-analysis-header">
                <span class="full-analysis-title">Full Scan Complete</span>
                <span class="full-analysis-meta">
                  {fullAnalysis.keyCounts.length} unique keys in {fullAnalysis.totalRows.toLocaleString()} rows
                  ({fullAnalysis.durationMs}ms)
                </span>
              </div>
              <div class="keys-table">
                <div class="keys-table-header">
                  <span class="kt-col-key">Key</span>
                  <span class="kt-col-count">Row Count</span>
                </div>
                {#each fullAnalysis.keyCounts.slice(0, 20) as [key, count]}
                  <div class="keys-table-row">
                    <span class="kt-col-key">{key}</span>
                    <span class="kt-col-count">{count.toLocaleString()}</span>
                  </div>
                {/each}
                {#if fullAnalysis.keyCounts.length > 20}
                  <div class="keys-table-more">
                    +{fullAnalysis.keyCounts.length - 20} more keys
                  </div>
                {/if}
              </div>
            </div>
          {/if}
        </div>
      </div>
    </div>

    <!-- Configuration -->
    <div class="section">
      <div class="section-header">
        <span class="section-title">3. CONFIGURE</span>
      </div>

      <div class="config-grid">
        <div class="config-item">
          <label class="config-label" for="col-index">Shard Column</label>
          <input
            id="col-index"
            type="number"
            class="config-input"
            bind:value={colIndex}
            min="0"
          />
        </div>

        <div class="config-item">
          <label class="config-label" for="delimiter">Delimiter</label>
          <select id="delimiter" class="config-select" bind:value={delimiter}>
            <option value=",">Comma (,)</option>
            <option value="tab">Tab</option>
            <option value="|">Pipe (|)</option>
            <option value=";">Semicolon (;)</option>
          </select>
        </div>

        <div class="config-item">
          <label class="config-label" for="has-header">Has Header</label>
          <select id="has-header" class="config-select" bind:value={hasHeader}>
            <option value={true}>Yes</option>
            <option value={false}>No</option>
          </select>
        </div>

        <div class="config-item">
          <label class="config-label" for="top-n">Top N Shards</label>
          <input
            id="top-n"
            type="number"
            class="config-input"
            placeholder="All"
            bind:value={topN}
            min="1"
          />
          <span class="config-hint">Rest go to _MISC</span>
        </div>
      </div>

      <!-- Output Directory -->
      <div class="output-dir">
        <label class="config-label" for="output-dir">Output Directory</label>
        <div class="input-row">
          <input
            id="output-dir"
            type="text"
            class="file-input"
            placeholder="Output directory..."
            bind:value={outputDir}
          />
          <button class="action-btn" onclick={handleSelectOutputDir}>Browse</button>
        </div>
      </div>

      <!-- Shred Button -->
      <div class="action-row">
        <button class="action-btn" onclick={handleReset}>Reset</button>
        <button
          class="action-btn primary large"
          onclick={handleShred}
          disabled={!outputDir || isShredding}
        >
          {isShredding ? "Shredding..." : "Approve & Shred"}
        </button>
      </div>
    </div>
  {/if}

  <!-- Shred Result -->
  {#if shredResult}
    <div class="section">
      <div class="section-header">
        <span class="section-title">4. SHRED COMPLETE</span>
        <span class="success-badge">SUCCESS</span>
      </div>

      <div class="result-card">
        <div class="result-summary">
          <div class="summary-item">
            <span class="summary-value">{shredResult.shards.length}</span>
            <span class="summary-label">Shards Created</span>
          </div>
          <div class="summary-item">
            <span class="summary-value">{shredResult.totalRows.toLocaleString()}</span>
            <span class="summary-label">Total Rows</span>
          </div>
          <div class="summary-item">
            <span class="summary-value">{shredResult.durationMs}ms</span>
            <span class="summary-label">Duration</span>
          </div>
          {#if shredResult.freezerKeyCount > 0}
            <div class="summary-item freezer">
              <span class="summary-value">{shredResult.freezerKeyCount}</span>
              <span class="summary-label">In Freezer</span>
            </div>
          {/if}
        </div>

        <!-- Shards List -->
        <div class="shards-list">
          <div class="shards-header">
            <span class="shard-col key">Shard Key</span>
            <span class="shard-col rows">Rows</span>
            <span class="shard-col size">Size</span>
            <span class="shard-col actions">Actions</span>
          </div>
          {#each shredResult.shards as shard}
            <div class="shard-row" class:freezer={shard.key === "_MISC"}>
              <span class="shard-col key">
                <span class="key-name">{shard.key}</span>
              </span>
              <span class="shard-col rows">{shard.rowCount.toLocaleString()}</span>
              <span class="shard-col size">{formatBytes(shard.byteSize)}</span>
              <span class="shard-col actions">
                <button
                  class="action-btn-small preview-btn"
                  onclick={() => handlePreviewShard(shard)}
                  title="Preview first 5 rows"
                >
                  {previewingShard === shard.path ? "Hide" : "Preview"}
                </button>
                <button
                  class="action-btn-small"
                  onclick={() => openParserGenerator(shard)}
                  title="Generate a parser for this shard"
                >
                  Generate Parser
                </button>
              </span>
            </div>
            {#if previewingShard === shard.path && shardPreview}
              <div class="shard-preview">
                <div class="preview-header">Sample Data (first 5 rows)</div>
                {#each shardPreview as row}
                  <div class="preview-row">{row}</div>
                {/each}
              </div>
            {/if}
          {/each}
        </div>

        <div class="lineage-info">
          <span class="lineage-label">Lineage Index:</span>
          <span class="lineage-path">{shredResult.lineageIndexPath}</span>
        </div>
      </div>
    </div>
  {/if}

  <!-- Error Display -->
  {#if error}
    <div class="error-toast">
      <span class="error-icon">!</span>
      <span class="error-message">{error}</span>
      <button class="dismiss-btn" onclick={() => error = null}>&#10005;</button>
    </div>
  {/if}

  <!-- Empty State -->
  {#if !analysisResult && !shredResult && !isAnalyzing}
    <div class="empty-state">
      <span class="empty-icon">&#9986;</span>
      <span class="empty-title">Select a File to Shred</span>
      <span class="empty-message">
        Drop a multiplexed file (CSV, JSON Lines, log) to split it into homogeneous shards.
        Each unique message type gets its own file.
      </span>
    </div>
  {/if}

  <!-- Parser Refinement Chat Modal -->
  {#if showParserGenerator && selectedShard}
    <ParserRefinementChat
      shardPath={selectedShard.path}
      shardKey={selectedShard.key}
      onApprove={(code) => {
        console.log("Parser approved:", code);
        closeParserGenerator();
      }}
      onClose={closeParserGenerator}
    />
  {/if}

  <!-- Chat Interface Modal -->
  {#if showChat && filePath}
    <div class="chat-modal-overlay">
      <div class="chat-modal">
        <ShredderChat
          filePath={filePath}
          onStrategyReady={handleStrategyFromChat}
          onClose={handleCloseChat}
        />
      </div>
    </div>
  {/if}
</div>

<style>
  .shredder-tab {
    display: flex;
    flex-direction: column;
    height: 100%;
    padding: var(--space-lg);
    gap: var(--space-md);
    overflow: auto;
  }

  /* Header */
  .header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .title {
    font-family: var(--font-mono);
    font-size: 14px;
    font-weight: 600;
    letter-spacing: 1px;
    color: var(--color-text-muted);
    margin: 0;
  }

  /* Sections */
  .section {
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    padding: var(--space-md);
  }

  .section-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: var(--space-md);
  }

  .section-title {
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 1px;
    color: var(--color-text-muted);
  }

  /* Input Row */
  .input-row {
    display: flex;
    gap: var(--space-sm);
  }

  .file-input {
    flex: 1;
    padding: var(--space-sm) var(--space-md);
    background: var(--color-bg-tertiary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-primary);
  }

  .file-input:focus {
    outline: none;
    border-color: var(--color-accent-cyan);
  }

  /* Buttons */
  .action-btn {
    padding: 6px 12px;
    background: var(--color-bg-tertiary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-secondary);
    cursor: pointer;
    transition: all 0.15s ease;
    white-space: nowrap;
  }

  .action-btn:hover:not(:disabled) {
    border-color: var(--color-accent-cyan);
    color: var(--color-accent-cyan);
  }

  .action-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .action-btn.primary {
    background: var(--color-accent-cyan);
    border-color: var(--color-accent-cyan);
    color: var(--color-bg-primary);
  }

  .action-btn.primary:hover:not(:disabled) {
    opacity: 0.9;
  }

  .action-btn.large {
    padding: 10px 24px;
    font-size: 12px;
  }

  /* Confidence Badge */
  .confidence-badge {
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.5px;
  }

  .success-badge {
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 600;
    color: var(--color-success);
    letter-spacing: 0.5px;
  }

  /* Analysis Card */
  .analysis-card {
    display: flex;
    flex-direction: column;
    gap: var(--space-sm);
  }

  .analysis-row {
    display: flex;
    gap: var(--space-md);
    align-items: baseline;
  }

  .analysis-label {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-muted);
    min-width: 120px;
  }

  .analysis-value {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-primary);
  }

  .analysis-value.highlight {
    color: var(--color-accent-cyan);
    font-weight: 600;
    font-size: 14px;
  }

  .analysis-value.mono {
    font-family: var(--font-mono);
    background: var(--color-bg-tertiary);
    padding: 2px 6px;
    border-radius: var(--radius-sm);
  }

  .reasoning {
    margin-top: var(--space-sm);
    padding: var(--space-sm);
    background: var(--color-bg-tertiary);
    border-radius: var(--radius-sm);
  }

  .reasoning-text {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-secondary);
    line-height: 1.5;
  }

  /* Warning Box */
  .warning-box {
    display: flex;
    gap: var(--space-sm);
    align-items: flex-start;
    margin-top: var(--space-sm);
    padding: var(--space-sm);
    background: rgba(255, 165, 0, 0.1);
    border: 1px solid rgba(255, 165, 0, 0.3);
    border-radius: var(--radius-sm);
  }

  .warning-icon {
    width: 18px;
    height: 18px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(255, 165, 0, 0.8);
    color: white;
    border-radius: 50%;
    font-size: 12px;
    font-weight: bold;
    flex-shrink: 0;
  }

  .warning-text {
    font-family: var(--font-mono);
    font-size: 11px;
    color: rgba(255, 165, 0, 0.9);
    line-height: 1.4;
  }

  /* Sample Keys */
  .sample-keys {
    margin-top: var(--space-md);
    padding-top: var(--space-md);
    border-top: 1px solid var(--color-border);
  }

  .sample-keys-title {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-muted);
    display: block;
    margin-bottom: var(--space-sm);
  }

  .keys-list {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-xs);
  }

  .key-tag {
    font-family: var(--font-mono);
    font-size: 10px;
    padding: 2px 8px;
    background: rgba(0, 212, 255, 0.1);
    border: 1px solid rgba(0, 212, 255, 0.3);
    border-radius: var(--radius-sm);
    color: var(--color-accent-cyan);
  }

  .key-more {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
    padding: 2px 8px;
  }

  /* Config Grid */
  .config-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: var(--space-md);
    margin-bottom: var(--space-md);
  }

  .config-item {
    display: flex;
    flex-direction: column;
    gap: var(--space-xs);
  }

  .config-label {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .config-input,
  .config-select {
    padding: var(--space-sm);
    background: var(--color-bg-tertiary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-primary);
  }

  .config-input:focus,
  .config-select:focus {
    outline: none;
    border-color: var(--color-accent-cyan);
  }

  .config-hint {
    font-family: var(--font-mono);
    font-size: 9px;
    color: var(--color-text-muted);
  }

  /* Output Directory */
  .output-dir {
    margin-bottom: var(--space-md);
  }

  .output-dir .config-label {
    margin-bottom: var(--space-xs);
  }

  /* Action Row */
  .action-row {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-sm);
  }

  /* Result Card */
  .result-card {
    display: flex;
    flex-direction: column;
    gap: var(--space-md);
  }

  .result-summary {
    display: flex;
    gap: var(--space-lg);
    padding: var(--space-md);
    background: var(--color-bg-tertiary);
    border-radius: var(--radius-sm);
  }

  .summary-item {
    display: flex;
    flex-direction: column;
    align-items: center;
  }

  .summary-value {
    font-family: var(--font-mono);
    font-size: 24px;
    font-weight: 700;
    color: var(--color-accent-cyan);
  }

  .summary-item.freezer .summary-value {
    color: rgba(255, 165, 0, 0.9);
  }

  .summary-label {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
    letter-spacing: 0.5px;
  }

  /* Shards List */
  .shards-list {
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }

  .shards-header {
    display: flex;
    padding: var(--space-sm) var(--space-md);
    background: var(--color-bg-tertiary);
    border-bottom: 1px solid var(--color-border);
  }

  .shard-col {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .shard-col.key {
    flex: 1;
  }

  .shard-col.rows,
  .shard-col.size {
    width: 100px;
    text-align: right;
  }

  .shard-col.actions {
    width: 120px;
    text-align: right;
  }

  .action-btn-small {
    padding: 4px 8px;
    background: var(--color-bg-tertiary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-secondary);
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .action-btn-small:hover {
    border-color: var(--color-accent-cyan);
    color: var(--color-accent-cyan);
  }

  .shard-row {
    display: flex;
    padding: var(--space-sm) var(--space-md);
    border-bottom: 1px solid var(--color-border);
  }

  .shard-row:last-child {
    border-bottom: none;
  }

  .shard-row.freezer {
    background: rgba(255, 165, 0, 0.05);
  }

  .shard-row .shard-col {
    font-size: 12px;
    color: var(--color-text-primary);
  }

  .key-name {
    font-family: var(--font-mono);
  }

  .shard-row.freezer .key-name {
    color: rgba(255, 165, 0, 0.9);
  }

  /* Lineage Info */
  .lineage-info {
    display: flex;
    gap: var(--space-sm);
    padding: var(--space-sm);
    background: var(--color-bg-tertiary);
    border-radius: var(--radius-sm);
  }

  .lineage-label {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
    text-transform: uppercase;
  }

  .lineage-path {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-secondary);
    word-break: break-all;
  }

  /* Empty State */
  .empty-state {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--space-md);
    text-align: center;
    padding: var(--space-xl);
  }

  .empty-icon {
    font-size: 48px;
    color: var(--color-text-muted);
  }

  .empty-title {
    font-family: var(--font-mono);
    font-size: 18px;
    color: var(--color-text-primary);
  }

  .empty-message {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-muted);
    max-width: 400px;
    line-height: 1.5;
  }

  /* Error Toast */
  .error-toast {
    position: fixed;
    bottom: var(--space-lg);
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: var(--space-sm);
    padding: var(--space-sm) var(--space-md);
    background: var(--color-error);
    border-radius: var(--radius-sm);
    color: white;
    font-family: var(--font-mono);
    font-size: 12px;
    z-index: 100;
    max-width: 80%;
  }

  .error-toast .error-icon {
    width: 20px;
    height: 20px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: white;
    color: var(--color-error);
    border-radius: 50%;
    font-weight: bold;
    font-size: 14px;
    flex-shrink: 0;
  }

  .error-message {
    word-break: break-word;
  }

  .dismiss-btn {
    background: none;
    border: none;
    color: white;
    cursor: pointer;
    padding: 4px;
    margin-left: var(--space-sm);
    opacity: 0.8;
    flex-shrink: 0;
  }

  .dismiss-btn:hover {
    opacity: 1;
  }

  /* Full Analysis Section */
  .full-analysis-section {
    margin-top: var(--space-md);
    padding-top: var(--space-md);
    border-top: 1px solid var(--color-border);
    display: flex;
    flex-direction: column;
    gap: var(--space-sm);
  }

  .full-analysis-section .hint {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
  }

  .full-analysis-result {
    margin-top: var(--space-sm);
    background: var(--color-bg-tertiary);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }

  .full-analysis-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--space-sm) var(--space-md);
    background: var(--color-bg-secondary);
    border-bottom: 1px solid var(--color-border);
  }

  .full-analysis-title {
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 600;
    color: var(--color-success);
  }

  .full-analysis-meta {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
  }

  .keys-table {
    max-height: 200px;
    overflow: auto;
  }

  .keys-table-header {
    display: flex;
    padding: var(--space-xs) var(--space-md);
    background: var(--color-bg-secondary);
    border-bottom: 1px solid var(--color-border);
    position: sticky;
    top: 0;
  }

  .keys-table-row {
    display: flex;
    padding: var(--space-xs) var(--space-md);
    border-bottom: 1px solid var(--color-border);
  }

  .keys-table-row:last-child {
    border-bottom: none;
  }

  .kt-col-key {
    flex: 1;
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-primary);
  }

  .kt-col-count {
    width: 100px;
    text-align: right;
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-accent-cyan);
  }

  .keys-table-header .kt-col-key,
  .keys-table-header .kt-col-count {
    color: var(--color-text-muted);
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .keys-table-more {
    padding: var(--space-sm) var(--space-md);
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
    text-align: center;
    background: var(--color-bg-secondary);
  }

  /* Shard Preview */
  .preview-btn {
    margin-right: 4px;
  }

  .shard-preview {
    background: var(--color-bg-tertiary);
    border: 1px solid var(--color-border);
    border-top: none;
    margin-left: var(--space-md);
    margin-right: var(--space-md);
    margin-bottom: var(--space-sm);
    border-radius: 0 0 var(--radius-sm) var(--radius-sm);
    overflow: hidden;
  }

  .preview-header {
    padding: var(--space-xs) var(--space-md);
    background: var(--color-bg-secondary);
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .preview-row {
    padding: var(--space-xs) var(--space-md);
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-secondary);
    border-bottom: 1px solid var(--color-border);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .preview-row:last-child {
    border-bottom: none;
  }

  /* Responsive */
  @media (max-width: 768px) {
    .config-grid {
      grid-template-columns: repeat(2, 1fr);
    }
  }

  /* Chat Modal */
  .chat-modal-overlay {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(0, 0, 0, 0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
    padding: var(--space-lg);
  }

  .chat-modal {
    width: 100%;
    max-width: 700px;
    height: 80vh;
    max-height: 600px;
  }
</style>
