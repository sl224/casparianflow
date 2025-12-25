<script lang="ts">
  import { jobsStore, type JobDetails } from "$lib/stores/jobs.svelte";

  interface Props {
    details: JobDetails;
  }

  let { details }: Props = $props();

  // Active tab for switching views
  let activeView = $state<"logs" | "data">("logs");

  // Format timestamp for display
  function formatTime(isoString: string | null): string {
    if (!isoString) return "--";
    try {
      const date = new Date(isoString);
      return date.toLocaleTimeString();
    } catch {
      return isoString;
    }
  }

  // Calculate duration between claim and end time
  function getDuration(): string {
    if (!details.claimTime || !details.endTime) return "--";
    try {
      const start = new Date(details.claimTime).getTime();
      const end = new Date(details.endTime).getTime();
      const ms = end - start;
      if (ms < 1000) return `${ms}ms`;
      if (ms < 60000) return `${(ms / 1000).toFixed(1)}s`;
      return `${(ms / 60000).toFixed(1)}m`;
    } catch {
      return "--";
    }
  }

  // Format log lines with color-coded prefixes
  function formatLogs(logs: string): string {
    const lines = logs.split("\n");
    return lines.map((line, idx) => {
      // Escape HTML
      const escaped = line
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;");

      // Add line number
      const lineNum = String(idx + 1).padStart(3, " ");
      const lineNumSpan = `<span class="line-num">${lineNum}</span>`;

      // Colorize log level prefixes
      if (escaped.startsWith("[STDOUT]")) {
        return `${lineNumSpan}<span class="log-stdout">${escaped}</span>`;
      } else if (escaped.startsWith("[STDERR]")) {
        return `${lineNumSpan}<span class="log-stderr">${escaped}</span>`;
      } else if (escaped.startsWith("[ERROR]")) {
        return `${lineNumSpan}<span class="log-error">${escaped}</span>`;
      } else if (escaped.startsWith("[WARN]")) {
        return `${lineNumSpan}<span class="log-warn">${escaped}</span>`;
      } else if (escaped.startsWith("[INFO]")) {
        return `${lineNumSpan}<span class="log-info">${escaped}</span>`;
      } else if (escaped.startsWith("[DEBUG]")) {
        return `${lineNumSpan}<span class="log-debug">${escaped}</span>`;
      } else if (escaped.startsWith("[SYSTEM]")) {
        return `${lineNumSpan}<span class="log-system">${escaped}</span>`;
      } else if (escaped.startsWith("Traceback") || escaped.includes("Error:")) {
        return `${lineNumSpan}<span class="log-error">${escaped}</span>`;
      }
      return `${lineNumSpan}${escaped}`;
    }).join("\n");
  }

  // Check if job has logs to display
  $effect(() => {
    // Auto-switch to data view if there's an output path but no logs
    if (details.outputPath && !details.logs && !details.errorMessage) {
      activeView = "data";
    }
  });
</script>

<div class="log-viewer">
  <!-- Header -->
  <div class="viewer-header">
    <div class="job-info">
      <span class="job-name">{details.pluginName}</span>
      <span class="job-id">#{details.jobId}</span>
      <span
        class="status-badge"
        class:success={details.status === "COMPLETED"}
        class:failed={details.status === "FAILED"}
      >
        {details.status}
      </span>
    </div>
    <div class="job-meta">
      <span class="meta-item">
        <span class="meta-icon">&#9200;</span>
        {getDuration()}
      </span>
      <span class="meta-item">
        <span class="meta-icon">&#128337;</span>
        {formatTime(details.endTime)}
      </span>
      {#if details.retryCount > 0}
        <span class="meta-item retry">
          <span class="meta-icon">&#8635;</span>
          {details.retryCount} retries
        </span>
      {/if}
    </div>
    <button class="close-btn" onclick={() => jobsStore.closeJobDetails()}>
      &times;
    </button>
  </div>

  <!-- Error Banner (if failed) -->
  {#if details.status === "FAILED" && details.errorMessage}
    <div class="error-banner">
      <span class="error-icon">&#9888;</span>
      <div class="error-content">
        <span class="error-label">Error</span>
        <span class="error-text">{details.errorMessage}</span>
      </div>
    </div>
  {/if}

  <!-- Tab Bar -->
  <div class="tab-bar">
    <button
      class="tab-btn"
      class:active={activeView === "logs"}
      onclick={() => (activeView = "logs")}
    >
      LOGS
      {#if details.logs}
        <span class="tab-badge">{details.logs.split("\n").length}</span>
      {/if}
    </button>
    {#if details.outputPath}
      <button
        class="tab-btn"
        class:active={activeView === "data"}
        onclick={() => (activeView = "data")}
      >
        OUTPUT
      </button>
    {/if}
  </div>

  <!-- Content Area -->
  <div class="viewer-content">
    {#if activeView === "logs"}
      {#if details.logs}
        <pre class="logs-display">{@html formatLogs(details.logs)}</pre>
      {:else}
        <div class="empty-state">
          <span class="empty-icon">&#128196;</span>
          <span class="empty-text">No execution logs captured</span>
          <span class="empty-hint">Logs will appear here when the job runs with sideband logging enabled</span>
        </div>
      {/if}
    {:else if activeView === "data"}
      <div class="output-panel">
        <div class="output-info">
          <span class="output-label">Output File</span>
          <code class="output-path">{details.outputPath}</code>
        </div>
        <button
          class="query-btn"
          onclick={() => {
            if (details.outputPath) {
              jobsStore.queryFile(details.outputPath);
            }
          }}
        >
          <span class="btn-icon">&#9654;</span>
          Query Data
        </button>
      </div>
    {/if}
  </div>
</div>

<style>
  .log-viewer {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    overflow: hidden;
  }

  /* Header */
  .viewer-header {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 12px 16px;
    background: var(--color-bg-secondary);
    border-bottom: 1px solid var(--color-border);
  }

  .job-info {
    display: flex;
    align-items: center;
    gap: 10px;
    flex: 1;
  }

  .job-name {
    font-family: var(--font-mono);
    font-size: 14px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .job-id {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-muted);
  }

  .status-badge {
    padding: 3px 10px;
    border-radius: 12px;
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.5px;
    text-transform: uppercase;
  }

  .status-badge.success {
    background: rgba(0, 255, 136, 0.15);
    color: var(--color-success);
  }

  .status-badge.failed {
    background: rgba(255, 51, 85, 0.15);
    color: var(--color-error);
  }

  .job-meta {
    display: flex;
    gap: 16px;
  }

  .meta-item {
    display: flex;
    align-items: center;
    gap: 4px;
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-muted);
  }

  .meta-item.retry {
    color: var(--color-warning);
  }

  .meta-icon {
    font-size: 12px;
  }

  .close-btn {
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: 1px solid var(--color-border);
    border-radius: 4px;
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 18px;
    transition: all 0.15s ease;
  }

  .close-btn:hover {
    border-color: var(--color-text-secondary);
    color: var(--color-text-primary);
  }

  /* Error Banner */
  .error-banner {
    display: flex;
    gap: 12px;
    padding: 12px 16px;
    background: rgba(255, 51, 85, 0.08);
    border-bottom: 1px solid var(--color-error);
  }

  .error-icon {
    font-size: 18px;
    color: var(--color-error);
    flex-shrink: 0;
  }

  .error-content {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }

  .error-label {
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 600;
    color: var(--color-error);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .error-text {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-error);
    word-break: break-word;
  }

  /* Tab Bar */
  .tab-bar {
    display: flex;
    gap: 2px;
    padding: 8px 16px 0;
    background: var(--color-bg-tertiary);
    border-bottom: 1px solid var(--color-border);
  }

  .tab-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 16px;
    background: transparent;
    border: 1px solid transparent;
    border-bottom: none;
    border-radius: 6px 6px 0 0;
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 600;
    color: var(--color-text-muted);
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .tab-btn:hover {
    color: var(--color-text-secondary);
  }

  .tab-btn.active {
    background: var(--color-bg-card);
    border-color: var(--color-border);
    color: var(--color-text-primary);
    margin-bottom: -1px;
  }

  .tab-badge {
    padding: 2px 6px;
    background: var(--color-bg-tertiary);
    border-radius: 10px;
    font-size: 10px;
    color: var(--color-text-muted);
  }

  .tab-btn.active .tab-badge {
    background: var(--color-accent-cyan);
    color: var(--color-bg-primary);
  }

  /* Content Area */
  .viewer-content {
    flex: 1;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  /* Logs Display */
  .logs-display {
    flex: 1;
    margin: 0;
    padding: 12px 16px;
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 1.6;
    color: var(--color-text-secondary);
    background: var(--color-bg-primary);
    overflow: auto;
    white-space: pre;
  }

  .logs-display :global(.line-num) {
    display: inline-block;
    width: 32px;
    margin-right: 12px;
    color: var(--color-text-muted);
    text-align: right;
    user-select: none;
    opacity: 0.5;
  }

  .logs-display :global(.log-stdout) {
    color: var(--color-text-primary);
  }

  .logs-display :global(.log-stderr) {
    color: var(--color-warning);
  }

  .logs-display :global(.log-error) {
    color: var(--color-error);
    font-weight: 500;
  }

  .logs-display :global(.log-warn) {
    color: var(--color-warning);
  }

  .logs-display :global(.log-info) {
    color: var(--color-accent-cyan);
  }

  .logs-display :global(.log-debug) {
    color: var(--color-text-muted);
  }

  .logs-display :global(.log-system) {
    color: var(--color-accent-green);
    font-style: italic;
  }

  /* Empty State */
  .empty-state {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 32px;
  }

  .empty-icon {
    font-size: 48px;
    opacity: 0.3;
  }

  .empty-text {
    font-family: var(--font-mono);
    font-size: 14px;
    color: var(--color-text-secondary);
  }

  .empty-hint {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-muted);
    text-align: center;
    max-width: 300px;
  }

  /* Output Panel */
  .output-panel {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 24px;
    padding: 32px;
  }

  .output-info {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
  }

  .output-label {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .output-path {
    padding: 8px 16px;
    background: var(--color-bg-tertiary);
    border: 1px solid var(--color-border);
    border-radius: 6px;
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-accent-cyan);
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .query-btn {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 24px;
    background: var(--color-accent-cyan);
    border: none;
    border-radius: 6px;
    font-family: var(--font-mono);
    font-size: 13px;
    font-weight: 600;
    color: var(--color-bg-primary);
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .query-btn:hover {
    background: var(--color-accent-green);
    transform: translateY(-1px);
  }

  .btn-icon {
    font-size: 12px;
  }
</style>
