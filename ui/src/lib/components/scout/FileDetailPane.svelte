<script lang="ts">
  import { scoutStore, formatBytes, type ScannedFile } from "$lib/stores/scout.svelte";

  interface Props {
    file: ScannedFile;
    onChangeTag: (fileId: number) => void;
    onChangePlugin: (fileId: number) => void;
    onProcess: (fileId: number) => void;
    onViewJob: (jobId: number) => void;
    onClose: () => void;
  }

  let { file, onChangeTag, onChangePlugin, onProcess, onViewJob, onClose }: Props = $props();

  // Resolved plugins state
  let resolvedPlugins = $state<string[]>([]);
  let pluginLoading = $state(false);

  // Load plugins when file tag changes
  $effect(() => {
    if (file.tag && !file.manualPlugin) {
      loadPluginsForTag(file.tag);
    } else {
      resolvedPlugins = [];
    }
  });

  async function loadPluginsForTag(tag: string) {
    pluginLoading = true;
    try {
      resolvedPlugins = await scoutStore.getPluginsForTag(tag);
    } catch {
      resolvedPlugins = [];
    } finally {
      pluginLoading = false;
    }
  }

  // Find the matching rule if tag was assigned by rule
  function getMatchingRule() {
    if (!file.ruleId) return null;
    return scoutStore.taggingRules.find(r => r.id === file.ruleId) ?? null;
  }

  // Get tag source display text
  function getTagSourceText(): string {
    if (!file.tag) return "";
    if (file.tagSource === "manual") return "Manual assignment";
    if (file.tagSource === "rule") {
      const rule = getMatchingRule();
      return rule ? `Rule: ${rule.name}` : "Rule (unknown)";
    }
    return "Unknown";
  }

  // Get plugin source display text
  function getPluginSourceText(): string {
    if (file.manualPlugin) return "Manual override";
    return "Tag subscription (auto)";
  }

  async function handleResetOverrides() {
    if (confirm("Reset all manual overrides for this file? This will clear the tag and plugin assignments.")) {
      await scoutStore.clearManualOverrides(file.id);
    }
  }

  async function handleProcess() {
    onProcess(file.id);
  }

  function handleViewJob() {
    if (file.sentinelJobId) {
      onViewJob(file.sentinelJobId);
    }
  }

  function getStatusColor(status: string): string {
    switch (status) {
      case "pending": return "var(--color-text-muted)";
      case "tagged": return "var(--color-accent-cyan)";
      case "queued": return "#ffaa00";
      case "processing": return "#ffaa00";
      case "processed": return "var(--color-success)";
      case "failed": return "var(--color-error)";
      default: return "var(--color-text-muted)";
    }
  }
</script>

<div class="detail-pane">
  <!-- Header -->
  <div class="pane-header">
    <span class="file-icon">&#128196;</span>
    <span class="file-name" title={file.relPath}>{file.relPath.split('/').pop()}</span>
    <button class="close-btn" onclick={onClose} title="Close">&#10005;</button>
  </div>

  <!-- Scrollable Content Area -->
  <div class="pane-content">
    <!-- File Info -->
    <div class="section">
      <div class="section-label">FILE INFO</div>
      <div class="info-row">
        <span class="info-label">Path</span>
        <span class="info-value path" title={file.path}>{file.path}</span>
      </div>
      <div class="info-row">
        <span class="info-label">Size</span>
        <span class="info-value">{formatBytes(file.size)}</span>
      </div>
      <div class="info-row">
        <span class="info-label">Status</span>
        <span
          class="info-value status"
          style="color: {getStatusColor(file.status)}"
        >
          {file.status}
        </span>
      </div>
      {#if file.error}
        <div class="info-row error-row">
          <span class="info-label">Error</span>
          <span class="info-value error">{file.error}</span>
        </div>
      {/if}
    </div>

    <!-- Tag Section -->
    <div class="section">
      <div class="section-label">
        TAG
        {#if scoutStore.isManualFile(file)}
          <span class="manual-badge" title="Has manual overrides">&#9995;</span>
        {/if}
      </div>

      {#if file.tag}
        <div class="tag-display">
          <span class="tag-value">{file.tag}</span>
        </div>
        <div class="source-info">
          <span class="source-label">Source:</span>
          {#if file.tagSource === "manual"}
            <span class="source-value manual">Manual assignment</span>
          {:else if file.tagSource === "rule"}
            {@const rule = getMatchingRule()}
            {#if rule}
              <span class="source-value rule-source">
                Rule: <code class="pattern">{rule.pattern}</code> &#8594; <span class="mapped-tag">{rule.tag}</span>
              </span>
            {:else}
              <span class="source-value">Rule (unknown)</span>
            {/if}
          {:else}
            <span class="source-value">Unknown</span>
          {/if}
        </div>
      {:else}
        <div class="no-tag-message">No tag assigned</div>
      {/if}

      <button class="action-btn" onclick={() => onChangeTag(file.id)}>
        {file.tag ? "Change Tag" : "Assign Tag"}
      </button>
    </div>

    <!-- Plugin Section -->
    <div class="section">
      <div class="section-label">PLUGIN</div>

      {#if file.manualPlugin}
        <div class="plugin-display">
          <span class="plugin-value">{file.manualPlugin}</span>
        </div>
        <div class="source-info">
          <span class="source-label">Source:</span>
          <span class="source-value manual">Manual override</span>
        </div>
      {:else if file.tag}
        {#if pluginLoading}
          <div class="plugin-loading">Loading...</div>
        {:else if resolvedPlugins.length === 0}
          <div class="warning-box">
            <span class="warning-icon">&#9888;</span>
            <span>No plugin configured for tag "{file.tag}"</span>
          </div>
        {:else}
          <div class="plugin-display">
            <span class="plugin-value">{resolvedPlugins[0]}</span>
            {#if resolvedPlugins.length > 1}
              <span class="plugin-more">+{resolvedPlugins.length - 1} more</span>
            {/if}
          </div>
          <div class="source-info">
            <span class="source-label">Source:</span>
            <span class="source-value">Via tag subscription</span>
          </div>
        {/if}
      {:else}
        <div class="no-plugin-message">Assign a tag first</div>
      {/if}

      {#if file.tag}
        <button class="action-btn" onclick={() => onChangePlugin(file.id)}>
          Override Plugin
        </button>
      {/if}
    </div>

    <!-- Sinks Section (read-only, from plugin config) -->
    <div class="section">
      <div class="section-label">SINKS</div>
      <div class="sinks-info">
        <span class="sinks-note">Sinks are determined by the plugin configuration</span>
      </div>
    </div>

    <!-- Job Status Section (only when file has sentinelJobId) -->
    {#if file.sentinelJobId}
      <div class="section">
        <div class="section-label">JOB STATUS</div>

        <div class="job-status-row">
          <span class="job-id">Job #{file.sentinelJobId}</span>
          <span class="job-status" style="color: {getStatusColor(file.status)}">
            {file.status.toUpperCase()}
          </span>
        </div>

        {#if file.status === "processed"}
          <div class="success-info">
            <span class="success-icon">&#10003;</span>
            <span>Data written to sinks successfully</span>
          </div>
        {/if}

        {#if file.status === "failed" && file.error}
          <div class="error-box">
            <span class="error-label">Error:</span>
            <span class="error-text">{file.error}</span>
          </div>
        {/if}

        <button class="action-btn link" onclick={handleViewJob}>
          View Job Details &#8594;
        </button>
      </div>
    {/if}
  </div>

  <!-- Actions (sticky at bottom) -->
  <div class="actions-section">
    {#if file.status === "tagged"}
      <button class="action-btn primary" onclick={handleProcess}>
        Process File
      </button>
    {/if}

    {#if scoutStore.isManualFile(file)}
      <button class="action-btn reset" onclick={handleResetOverrides}>
        Reset Overrides
      </button>
    {/if}
  </div>
</div>

<style>
  .detail-pane {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    overflow: hidden;
  }

  .pane-content {
    flex: 1;
    overflow-y: auto;
    min-height: 0;
  }

  .pane-header {
    display: flex;
    align-items: center;
    gap: var(--space-sm);
    padding: var(--space-md);
    background: var(--color-bg-tertiary);
    border-bottom: 1px solid var(--color-border);
  }

  .file-icon {
    font-size: 20px;
  }

  .file-name {
    flex: 1;
    font-family: var(--font-mono);
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .close-btn {
    background: none;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    padding: 4px;
    font-size: 14px;
    border-radius: var(--radius-sm);
  }

  .close-btn:hover {
    color: var(--color-text-primary);
    background: var(--color-bg-primary);
  }

  .section {
    padding: var(--space-md);
    border-bottom: 1px solid var(--color-border);
  }

  .section-label {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    margin-bottom: var(--space-sm);
    display: flex;
    align-items: center;
    gap: var(--space-xs);
  }

  .manual-badge {
    font-size: 12px;
  }

  .info-row {
    display: flex;
    align-items: flex-start;
    gap: var(--space-sm);
    margin-bottom: var(--space-xs);
    font-family: var(--font-mono);
    font-size: 11px;
  }

  .info-label {
    color: var(--color-text-muted);
    min-width: 50px;
    flex-shrink: 0;
  }

  .info-value {
    color: var(--color-text-primary);
    word-break: break-all;
  }

  .info-value.path {
    font-size: 10px;
    color: var(--color-text-secondary);
  }

  .info-value.status {
    text-transform: uppercase;
    font-size: 10px;
    font-weight: 600;
  }

  .info-value.error {
    color: var(--color-error);
    font-size: 10px;
  }

  .error-row {
    background: rgba(255, 85, 85, 0.1);
    padding: var(--space-xs);
    border-radius: var(--radius-sm);
    margin-top: var(--space-xs);
  }

  .tag-display, .plugin-display {
    margin-bottom: var(--space-sm);
  }

  .tag-value {
    display: inline-block;
    padding: 4px 10px;
    background: rgba(0, 212, 255, 0.15);
    border: 1px solid rgba(0, 212, 255, 0.3);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-accent-cyan);
  }

  .plugin-value {
    display: inline-block;
    padding: 4px 10px;
    background: rgba(0, 255, 136, 0.15);
    border: 1px solid rgba(0, 255, 136, 0.3);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-success);
  }

  .no-tag-message, .no-plugin-message {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-muted);
    font-style: italic;
    margin-bottom: var(--space-sm);
  }

  .plugin-loading {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-muted);
    font-style: italic;
    margin-bottom: var(--space-sm);
  }

  .plugin-more {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
    margin-left: var(--space-sm);
  }

  .warning-box {
    display: flex;
    align-items: center;
    gap: var(--space-xs);
    padding: var(--space-sm);
    background: rgba(255, 170, 0, 0.15);
    border: 1px solid rgba(255, 170, 0, 0.3);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 11px;
    color: #ffaa00;
    margin-bottom: var(--space-sm);
  }

  .warning-icon {
    font-size: 14px;
  }

  .job-status-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: var(--space-sm);
  }

  .job-id {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-secondary);
  }

  .job-status {
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.5px;
  }

  .success-info {
    display: flex;
    align-items: center;
    gap: var(--space-xs);
    padding: var(--space-sm);
    background: rgba(0, 255, 136, 0.1);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-success);
    margin-bottom: var(--space-sm);
  }

  .success-icon {
    font-size: 14px;
  }

  .error-box {
    padding: var(--space-sm);
    background: rgba(255, 85, 85, 0.1);
    border: 1px solid rgba(255, 85, 85, 0.3);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 11px;
    margin-bottom: var(--space-sm);
  }

  .error-label {
    color: var(--color-error);
    font-weight: 600;
  }

  .error-text {
    color: var(--color-error);
    word-break: break-all;
  }

  .action-btn.link {
    background: transparent;
    color: var(--color-accent-cyan);
    border: none;
    padding: var(--space-xs) 0;
    text-align: left;
  }

  .action-btn.link:hover {
    text-decoration: underline;
  }

  .source-info {
    display: flex;
    gap: var(--space-xs);
    margin-bottom: var(--space-sm);
    font-family: var(--font-mono);
    font-size: 10px;
  }

  .source-label {
    color: var(--color-text-muted);
  }

  .source-value {
    color: var(--color-text-secondary);
  }

  .source-value.manual {
    color: #ffaa00;
  }

  .source-value.rule-source {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-wrap: wrap;
  }

  .pattern {
    background: rgba(0, 212, 255, 0.1);
    padding: 1px 4px;
    border-radius: 2px;
    color: var(--color-accent-cyan);
    font-family: var(--font-mono);
  }

  .mapped-tag {
    color: var(--color-success);
  }

  .sinks-info {
    padding: var(--space-sm);
    background: var(--color-bg-tertiary);
    border-radius: var(--radius-sm);
  }

  .sinks-note {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
    font-style: italic;
  }

  .actions-section {
    flex-shrink: 0;
    padding: var(--space-md);
    display: flex;
    flex-direction: column;
    gap: var(--space-sm);
    border-top: 1px solid var(--color-border);
    background: var(--color-bg-card);
  }

  .action-btn {
    padding: 8px 12px;
    background: var(--color-bg-tertiary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-secondary);
    cursor: pointer;
    transition: all 0.15s ease;
    text-align: center;
  }

  .action-btn:hover {
    border-color: var(--color-accent-cyan);
    color: var(--color-accent-cyan);
  }

  .action-btn.primary {
    background: var(--color-accent-cyan);
    border-color: var(--color-accent-cyan);
    color: var(--color-bg-primary);
  }

  .action-btn.primary:hover {
    opacity: 0.9;
  }

  .action-btn.reset {
    color: var(--color-error);
    border-color: rgba(255, 85, 85, 0.3);
  }

  .action-btn.reset:hover {
    border-color: var(--color-error);
    background: rgba(255, 85, 85, 0.1);
  }
</style>
