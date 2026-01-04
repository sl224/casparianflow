<script lang="ts">
  import { scoutStore, formatBytes } from "$lib/stores/scout.svelte";
  import type { TaggingRule, FailedFile, TagStats } from "$lib/stores/scout.svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";

  // Local state
  let newRulePattern = $state("");
  let newRuleName = $state("");
  let newRuleTag = $state("");
  let expandedSourceId = $state<string | null>(null);
  let showAddRuleForSource = $state<string | null>(null);
  let showManualTagModal = $state(false);
  let manualTagFileIds = $state<number[]>([]);
  let manualTagValue = $state("");

  onMount(async () => {
    try {
      await scoutStore.initDb();
      await scoutStore.loadSources();
      await scoutStore.loadStatus();
    } catch (e) {
      console.error("[ScoutTab] Init failed:", e);
    }
  });

  async function handleSelectFolder() {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Select folder to scan",
    });

    if (selected) {
      const path = selected as string;
      const name = path.split("/").pop() || "source";
      const id = `src-${Date.now()}`;

      await scoutStore.addSource(id, name, path);
      expandedSourceId = id;
      scoutStore.selectSource(id);
      await scoutStore.scan(id);
    }
  }

  async function handleScan(sourceId: string) {
    expandedSourceId = sourceId;
    scoutStore.selectSource(sourceId);
    await scoutStore.scan(sourceId);
  }

  async function handleAutoTag(sourceId: string) {
    expandedSourceId = sourceId;
    scoutStore.selectSource(sourceId);
    await scoutStore.autoTag(sourceId);
  }

  async function handleSubmit(sourceId: string) {
    expandedSourceId = sourceId;
    scoutStore.selectSource(sourceId);
    const result = await scoutStore.submitAllTagged(sourceId);

    if (result.noPlugin.length > 0) {
      const tags = [...new Set(result.noPlugin.map(([, tag]) => tag))];
      scoutStore.error = `No plugins configured for tags: ${tags.join(", ")}`;
    }
  }

  async function handleAddTaggingRule() {
    const sourceId = showAddRuleForSource;
    if (!sourceId || !newRulePattern || !newRuleName || !newRuleTag) return;

    const id = `rule-${Date.now()}`;
    await scoutStore.addTaggingRule(id, newRuleName, sourceId, newRulePattern, newRuleTag);

    // Reset form
    newRulePattern = "";
    newRuleName = "";
    newRuleTag = "";
    showAddRuleForSource = null;
  }

  async function handleRemoveTaggingRule(id: string) {
    await scoutStore.removeTaggingRule(id);
  }

  function handlePatternInput(e: Event) {
    const target = e.target as HTMLInputElement;
    newRulePattern = target.value;
    scoutStore.updatePreviewPattern(target.value);
  }

  function toggleSource(sourceId: string) {
    if (expandedSourceId === sourceId) {
      expandedSourceId = null;
    } else {
      expandedSourceId = sourceId;
      scoutStore.selectSource(sourceId);
    }
  }

  function openManualTagModal(fileIds: number[]) {
    manualTagFileIds = fileIds;
    manualTagValue = "";
    showManualTagModal = true;
  }

  async function handleManualTag() {
    if (!manualTagValue || manualTagFileIds.length === 0) return;

    await scoutStore.tagFiles(manualTagFileIds, manualTagValue);
    showManualTagModal = false;
    manualTagFileIds = [];
    manualTagValue = "";
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

  function getTagStats(tag: string): TagStats | undefined {
    return scoutStore.tagStats.find(s => s.tag === tag);
  }
</script>

<div class="scout-tab">
  <!-- Header -->
  <div class="header">
    <h2 class="title">SCOUT - File Discovery</h2>
    <button class="action-btn primary" onclick={handleSelectFolder}>+ Add Folder</button>
  </div>

  <!-- Unified Tree View -->
  <div class="tree-view">
    {#if scoutStore.sources.length === 0}
      <div class="empty-state">
        <span class="empty-icon">&#128193;</span>
        <span class="empty-title">No Sources</span>
        <span class="empty-message">Add a folder to start discovering files and assigning tags.</span>
      </div>
    {:else}
      {#each scoutStore.sources as source}
        {@const isExpanded = expandedSourceId === source.id}
        {@const isSelected = scoutStore.selectedSourceId === source.id}
        {@const rules = isSelected ? scoutStore.taggingRules : []}

        <div class="source-node" class:expanded={isExpanded}>
          <!-- Source Header -->
          <div class="source-header" class:selected={isExpanded}>
            <button class="expand-btn" onclick={() => toggleSource(source.id)}>
              {isExpanded ? "&#9660;" : "&#9654;"}
            </button>
            <span class="source-icon">&#128193;</span>
            <div class="source-info">
              <span class="source-name">{source.name}</span>
              <span class="source-path">{source.path}</span>
            </div>
            <div class="source-actions">
              <button
                class="action-btn small"
                onclick={() => handleScan(source.id)}
                disabled={scoutStore.scanning}
              >
                {scoutStore.scanning && isSelected ? "..." : "Scan"}
              </button>
              <button
                class="action-btn small primary"
                onclick={() => handleAutoTag(source.id)}
                disabled={scoutStore.tagging || !scoutStore.hasTaggingRules}
                title={!scoutStore.hasTaggingRules ? "Add tagging rules first" : "Auto-tag pending files"}
              >
                {scoutStore.tagging && isSelected ? "..." : "Auto-tag"}
              </button>
              <button
                class="action-btn small success"
                onclick={() => handleSubmit(source.id)}
                disabled={scoutStore.submitting || scoutStore.taggedFiles.length === 0}
                title={scoutStore.taggedFiles.length === 0 ? "Tag files first" : "Submit tagged files to Sentinel"}
              >
                {scoutStore.submitting && isSelected ? "..." : "Process"}
              </button>
            </div>
          </div>

          {#if isExpanded}
            <div class="source-content">
              <!-- Tag Stats Summary -->
              {#if scoutStore.tagStats.length > 0}
                <div class="tag-stats-summary">
                  {#each scoutStore.tagStats as stat}
                    <div class="tag-stat-chip">
                      <span class="tag-name">{stat.tag}</span>
                      <span class="tag-count">{stat.fileCount}</span>
                    </div>
                  {/each}
                </div>
              {/if}

              <!-- Tagging Rules -->
              {#if rules.length > 0}
                <div class="rules-section">
                  <div class="section-header">Tagging Rules</div>
                  {#each rules as rule}
                    {@const stats = getTagStats(rule.tag)}

                    <div class="rule-node">
                      <div class="rule-header">
                        <span class="rule-pattern">{rule.pattern}</span>
                        <span class="rule-arrow">&#8594;</span>
                        <span class="rule-tag">{rule.tag}</span>
                        <span class="rule-priority">P{rule.priority}</span>
                        <button class="remove-btn" onclick={() => handleRemoveTaggingRule(rule.id)}>&#10005;</button>
                      </div>
                      {#if stats}
                        <div class="rule-stats">
                          <span class="stat-item">{stats.fileCount} files</span>
                          <span class="stat-sep">|</span>
                          <span class="stat-item">{formatBytes(stats.totalBytes)}</span>
                          {#if stats.processedCount > 0}
                            <span class="stat-sep">|</span>
                            <span class="stat-item ok">{stats.processedCount} processed</span>
                          {/if}
                          {#if stats.failedCount > 0}
                            <span class="stat-sep">|</span>
                            <span class="stat-item fail">{stats.failedCount} failed</span>
                          {/if}
                        </div>
                      {:else}
                        <div class="rule-stats muted">No files match this pattern yet</div>
                      {/if}
                    </div>
                  {/each}
                </div>
              {/if}

              <!-- Add Tagging Rule -->
              {#if showAddRuleForSource === source.id}
                <div class="add-rule-form">
                  <div class="form-row">
                    <input
                      type="text"
                      bind:value={newRuleName}
                      placeholder="Rule name"
                      class="form-input"
                    />
                    <input
                      type="text"
                      value={newRulePattern}
                      oninput={handlePatternInput}
                      placeholder="*.csv"
                      class="form-input"
                    />
                    <input
                      type="text"
                      bind:value={newRuleTag}
                      placeholder="tag_name"
                      class="form-input tag-input"
                    />
                  </div>

                  {#if scoutStore.previewResult}
                    <div class="preview-inline" class:error={!scoutStore.previewResult.isValid}>
                      {#if scoutStore.previewResult.isValid}
                        <span class="preview-count">{scoutStore.previewResult.matchedCount}</span>
                        <span class="preview-label">files ({formatBytes(scoutStore.previewResult.matchedBytes)})</span>
                      {:else}
                        <span class="preview-error">{scoutStore.previewResult.error}</span>
                      {/if}
                    </div>
                  {/if}

                  <div class="form-actions">
                    <button class="action-btn" onclick={() => showAddRuleForSource = null}>Cancel</button>
                    <button
                      class="action-btn primary"
                      onclick={handleAddTaggingRule}
                      disabled={!newRuleName || !newRulePattern || !newRuleTag || !scoutStore.previewResult?.isValid}
                    >
                      Add Rule
                    </button>
                  </div>
                </div>
              {:else}
                <button
                  class="add-rule-btn"
                  onclick={() => { showAddRuleForSource = source.id; scoutStore.selectSource(source.id); }}
                >
                  + Add Tagging Rule
                </button>
              {/if}

              <!-- Untagged Files Section -->
              {#if scoutStore.coverage && scoutStore.coverage.untaggedCount > 0}
                <div class="untagged-section">
                  <div class="untagged-header">
                    <span class="untagged-icon">&#128196;</span>
                    <span class="untagged-title">
                      {scoutStore.coverage.untaggedCount} Untagged Files
                      <span class="untagged-size">({formatBytes(scoutStore.coverage.untaggedBytes)})</span>
                    </span>
                  </div>
                  <div class="untagged-actions">
                    <button
                      class="action-btn small"
                      onclick={async () => {
                        await scoutStore.loadUntaggedFiles(source.id);
                        // Auto-open the file list
                        const details = document.querySelector('.files-section') as HTMLDetailsElement | null;
                        if (details) details.open = true;
                      }}
                    >
                      Show Files
                    </button>
                    <button
                      class="action-btn small primary"
                      onclick={async () => {
                        await scoutStore.loadUntaggedFiles(source.id);
                        openManualTagModal(scoutStore.files.filter(f => !f.tag).map(f => f.id));
                      }}
                    >
                      Tag All
                    </button>
                  </div>
                  {#if scoutStore.coverage.untaggedSamples.length > 0}
                    <div class="untagged-samples">
                      <span class="sample-label">Examples:</span>
                      {#each scoutStore.coverage.untaggedSamples.slice(0, 3) as sample}
                        <span class="sample-file">{sample}</span>
                      {/each}
                    </div>
                  {/if}
                </div>
              {/if}

              <!-- Failed Files -->
              {#if scoutStore.failedFilesList.length > 0}
                <div class="failed-section">
                  <div class="failed-header">
                    <span class="failed-icon">&#9888;</span>
                    <span class="failed-title">{scoutStore.failedFilesList.length} Failed Files</span>
                  </div>
                  <div class="failed-list">
                    {#each scoutStore.failedFilesList.slice(0, 5) as file}
                      <div class="failed-item">
                        <span class="failed-path">{file.relPath}</span>
                        {#if file.tag}
                          <span class="failed-tag">{file.tag}</span>
                        {/if}
                        <span class="failed-error">{file.error}</span>
                      </div>
                    {/each}
                    {#if scoutStore.failedFilesList.length > 5}
                      <div class="failed-more">+ {scoutStore.failedFilesList.length - 5} more</div>
                    {/if}
                  </div>
                </div>
              {/if}

              <!-- File List -->
              {#if scoutStore.files.length > 0}
                {@const untaggedFiles = scoutStore.files.filter(f => !f.tag)}
                <details class="files-section" open={untaggedFiles.length > 0}>
                  <summary class="files-summary">
                    {scoutStore.files.length} files
                    ({formatBytes(scoutStore.files.reduce((sum, f) => sum + f.size, 0))})
                    {#if untaggedFiles.length > 0}
                      <span class="untagged-badge">{untaggedFiles.length} untagged</span>
                    {/if}
                  </summary>
                  <div class="file-list-header">
                    {#if untaggedFiles.length > 0}
                      <button
                        class="action-btn small primary"
                        onclick={() => openManualTagModal(untaggedFiles.map(f => f.id))}
                      >
                        Tag {untaggedFiles.length} untagged files
                      </button>
                    {/if}
                  </div>
                  <div class="file-list">
                    {#each scoutStore.files.slice(0, 50) as file}
                      <div class="file-item" class:untagged={!file.tag}>
                        {#if !file.tag}
                          <button
                            class="tag-btn"
                            onclick={() => openManualTagModal([file.id])}
                            title="Tag this file"
                          >
                            +
                          </button>
                        {/if}
                        <span class="file-name">{file.relPath}</span>
                        <span class="file-size">{formatBytes(file.size)}</span>
                        {#if file.tag}
                          <span class="file-tag">{file.tag}</span>
                        {:else}
                          <span class="file-no-tag">no tag</span>
                        {/if}
                        <span
                          class="file-status"
                          style="color: {getStatusColor(file.status)}; background: {getStatusColor(file.status)}20;"
                        >
                          {file.status}
                        </span>
                      </div>
                    {/each}
                    {#if scoutStore.files.length > 50}
                      <div class="file-more">+ {scoutStore.files.length - 50} more</div>
                    {/if}
                  </div>
                </details>
              {/if}
            </div>
          {/if}
        </div>
      {/each}
    {/if}
  </div>

  <!-- Manual Tag Modal -->
  {#if showManualTagModal}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div class="modal-overlay" role="presentation" onclick={() => showManualTagModal = false}>
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div class="modal" role="dialog" aria-modal="true" tabindex="-1" onclick={(e) => e.stopPropagation()}>
        <div class="modal-header">
          <span class="modal-title">Tag {manualTagFileIds.length} file(s)</span>
          <button class="close-btn" onclick={() => showManualTagModal = false}>&#10005;</button>
        </div>
        <div class="modal-body">
          <input
            type="text"
            bind:value={manualTagValue}
            placeholder="Enter tag name"
            class="form-input"
          />
          {#if scoutStore.availableTags.length > 0}
            <div class="tag-suggestions">
              <span class="suggestion-label">Existing tags:</span>
              {#each scoutStore.availableTags as tag}
                <button
                  class="tag-suggestion"
                  onclick={() => manualTagValue = tag}
                >
                  {tag}
                </button>
              {/each}
            </div>
          {/if}
        </div>
        <div class="modal-actions">
          <button class="action-btn" onclick={() => showManualTagModal = false}>Cancel</button>
          <button
            class="action-btn primary"
            onclick={handleManualTag}
            disabled={!manualTagValue}
          >
            Apply Tag
          </button>
        </div>
      </div>
    </div>
  {/if}

  <!-- Error Toast -->
  {#if scoutStore.error}
    <div class="error-toast">
      <span class="error-icon">!</span>
      <span class="error-message">{scoutStore.error}</span>
      <button class="dismiss-btn" onclick={() => scoutStore.error = null}>&#10005;</button>
    </div>
  {/if}
</div>

<style>
  .scout-tab {
    display: flex;
    flex-direction: column;
    height: 100%;
    padding: var(--space-lg);
    gap: var(--space-lg);
  }

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

  .tree-view {
    flex: 1;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: var(--space-sm);
  }

  /* Source Node */
  .source-node {
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    overflow: hidden;
  }

  .source-node.expanded {
    border-color: var(--color-accent-cyan);
  }

  .source-header {
    display: flex;
    align-items: center;
    gap: var(--space-sm);
    padding: var(--space-md);
    cursor: pointer;
    transition: background 0.15s ease;
  }

  .source-header:hover {
    background: var(--color-bg-tertiary);
  }

  .source-header.selected {
    background: var(--color-bg-tertiary);
  }

  .expand-btn {
    background: none;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    padding: 0;
    font-size: 10px;
    width: 16px;
  }

  .source-icon {
    font-size: 16px;
  }

  .source-info {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .source-name {
    font-family: var(--font-mono);
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .source-path {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .source-actions {
    display: flex;
    gap: var(--space-xs);
  }

  .source-content {
    padding: 0 var(--space-md) var(--space-md);
    display: flex;
    flex-direction: column;
    gap: var(--space-sm);
    border-top: 1px solid var(--color-border);
  }

  /* Tag Stats Summary */
  .tag-stats-summary {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-xs);
    padding: var(--space-sm) 0;
  }

  .tag-stat-chip {
    display: flex;
    align-items: center;
    gap: var(--space-xs);
    padding: 4px 8px;
    background: rgba(0, 212, 255, 0.1);
    border: 1px solid rgba(0, 212, 255, 0.3);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 11px;
  }

  .tag-name {
    color: var(--color-accent-cyan);
  }

  .tag-count {
    color: var(--color-text-muted);
    background: var(--color-bg-primary);
    padding: 1px 4px;
    border-radius: 3px;
    font-size: 10px;
  }

  /* Rules Section */
  .rules-section {
    display: flex;
    flex-direction: column;
    gap: var(--space-xs);
  }

  .section-header {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    padding: var(--space-xs) 0;
  }

  .rule-node {
    background: var(--color-bg-tertiary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    padding: var(--space-sm) var(--space-md);
    margin-left: var(--space-md);
  }

  .rule-header {
    display: flex;
    align-items: center;
    gap: var(--space-sm);
    font-family: var(--font-mono);
    font-size: 12px;
  }

  .rule-pattern {
    color: var(--color-accent-cyan);
    background: rgba(0, 212, 255, 0.1);
    padding: 2px 6px;
    border-radius: 3px;
  }

  .rule-arrow {
    color: var(--color-text-muted);
  }

  .rule-tag {
    color: var(--color-success);
    background: rgba(0, 255, 136, 0.1);
    padding: 2px 6px;
    border-radius: 3px;
    flex: 1;
  }

  .rule-priority {
    color: var(--color-text-muted);
    font-size: 10px;
    background: var(--color-bg-primary);
    padding: 2px 6px;
    border-radius: 3px;
  }

  .remove-btn {
    background: none;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    padding: 2px 6px;
    font-size: 12px;
    opacity: 0.5;
  }

  .remove-btn:hover {
    color: var(--color-error);
    opacity: 1;
  }

  .rule-stats {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-secondary);
    margin-top: var(--space-xs);
    display: flex;
    gap: var(--space-xs);
  }

  .rule-stats.muted {
    color: var(--color-text-muted);
    font-style: italic;
  }

  .stat-sep {
    color: var(--color-text-muted);
  }

  .stat-item.ok {
    color: var(--color-success);
  }

  .stat-item.fail {
    color: var(--color-error);
  }

  /* Add Rule */
  .add-rule-btn {
    margin-left: var(--space-md);
    padding: var(--space-sm);
    background: transparent;
    border: 1px dashed var(--color-border);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-muted);
    cursor: pointer;
    text-align: left;
  }

  .add-rule-btn:hover {
    border-color: var(--color-accent-cyan);
    color: var(--color-accent-cyan);
  }

  .add-rule-form {
    margin-left: var(--space-md);
    padding: var(--space-md);
    background: var(--color-bg-primary);
    border: 1px solid var(--color-accent-cyan);
    border-radius: var(--radius-sm);
    display: flex;
    flex-direction: column;
    gap: var(--space-sm);
  }

  .form-row {
    display: flex;
    gap: var(--space-sm);
  }

  .form-input {
    flex: 1;
    padding: var(--space-sm);
    background: var(--color-bg-tertiary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-primary);
  }

  .form-input:focus {
    outline: none;
    border-color: var(--color-accent-cyan);
  }

  .tag-input {
    max-width: 150px;
    border-color: rgba(0, 255, 136, 0.3);
  }

  .form-actions {
    display: flex;
    gap: var(--space-sm);
    justify-content: flex-end;
  }

  .preview-inline {
    display: flex;
    align-items: baseline;
    gap: var(--space-xs);
    font-family: var(--font-mono);
    font-size: 12px;
    padding: var(--space-xs) 0;
  }

  .preview-inline.error {
    color: var(--color-error);
  }

  .preview-count {
    font-size: 18px;
    font-weight: 700;
    color: var(--color-accent-cyan);
  }

  .preview-label {
    color: var(--color-text-secondary);
  }

  .preview-error {
    color: var(--color-error);
  }

  /* Untagged Section */
  .untagged-section {
    margin-left: var(--space-md);
    padding: var(--space-md);
    background: rgba(255, 170, 0, 0.1);
    border: 1px solid rgba(255, 170, 0, 0.3);
    border-radius: var(--radius-sm);
  }

  .untagged-header {
    display: flex;
    align-items: center;
    gap: var(--space-sm);
    margin-bottom: var(--space-sm);
  }

  .untagged-icon {
    font-size: 16px;
  }

  .untagged-title {
    font-family: var(--font-mono);
    font-size: 12px;
    font-weight: 600;
    color: #ffaa00;
  }

  .untagged-size {
    font-weight: 400;
    color: var(--color-text-muted);
  }

  .untagged-actions {
    display: flex;
    gap: var(--space-sm);
    margin-bottom: var(--space-sm);
  }

  .untagged-samples {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-xs);
    align-items: center;
  }

  .sample-label {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
  }

  .sample-file {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-secondary);
    background: var(--color-bg-primary);
    padding: 2px 6px;
    border-radius: 3px;
    max-width: 150px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .untagged-badge {
    background: rgba(255, 170, 0, 0.2);
    color: #ffaa00;
    padding: 2px 6px;
    border-radius: 3px;
    font-size: 10px;
    margin-left: var(--space-sm);
  }

  /* Failed Section */
  .failed-section {
    margin-left: var(--space-md);
    padding: var(--space-sm);
    background: rgba(255, 85, 85, 0.1);
    border: 1px solid rgba(255, 85, 85, 0.3);
    border-radius: var(--radius-sm);
  }

  .failed-header {
    display: flex;
    align-items: center;
    gap: var(--space-sm);
    margin-bottom: var(--space-sm);
  }

  .failed-icon {
    color: var(--color-error);
  }

  .failed-title {
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 600;
    color: var(--color-error);
  }

  .failed-list {
    display: flex;
    flex-direction: column;
    gap: var(--space-xs);
  }

  .failed-item {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: var(--space-xs);
    background: var(--color-bg-primary);
    border-radius: var(--radius-sm);
  }

  .failed-path {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-primary);
  }

  .failed-tag {
    font-family: var(--font-mono);
    font-size: 9px;
    color: var(--color-accent-cyan);
    background: rgba(0, 212, 255, 0.1);
    padding: 1px 4px;
    border-radius: 2px;
    align-self: flex-start;
  }

  .failed-error {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-error);
    opacity: 0.8;
  }

  .failed-more {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
    text-align: center;
    padding: var(--space-xs);
  }

  /* Files Section */
  .files-section {
    margin-left: var(--space-md);
  }

  .files-summary {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-muted);
    cursor: pointer;
    padding: var(--space-xs) 0;
  }

  .files-summary:hover {
    color: var(--color-text-secondary);
  }

  .file-list-header {
    display: flex;
    justify-content: flex-end;
    padding: var(--space-xs) 0;
  }

  .file-list {
    max-height: 200px;
    overflow: auto;
  }

  .file-item {
    display: flex;
    align-items: center;
    gap: var(--space-sm);
    padding: var(--space-xs) var(--space-sm);
    font-family: var(--font-mono);
    font-size: 10px;
  }

  .file-item:hover {
    background: var(--color-bg-tertiary);
  }

  .file-name {
    flex: 1;
    color: var(--color-text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .file-size {
    color: var(--color-text-muted);
  }

  .file-tag {
    color: var(--color-accent-cyan);
    background: rgba(0, 212, 255, 0.1);
    padding: 1px 4px;
    border-radius: 2px;
    font-size: 9px;
  }

  .file-no-tag {
    color: #ffaa00;
    background: rgba(255, 170, 0, 0.1);
    padding: 1px 4px;
    border-radius: 2px;
    font-size: 9px;
    font-style: italic;
  }

  .file-item.untagged {
    background: rgba(255, 170, 0, 0.05);
  }

  .file-item.untagged:hover {
    background: rgba(255, 170, 0, 0.1);
  }

  .file-status {
    padding: 1px 4px;
    border-radius: 2px;
    font-size: 9px;
    text-transform: uppercase;
  }

  .tag-btn {
    background: rgba(255, 170, 0, 0.2);
    border: 1px solid rgba(255, 170, 0, 0.5);
    border-radius: 3px;
    cursor: pointer;
    padding: 2px 6px;
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: bold;
    color: #ffaa00;
    flex-shrink: 0;
  }

  .tag-btn:hover {
    background: rgba(255, 170, 0, 0.3);
    border-color: #ffaa00;
  }

  .file-more {
    text-align: center;
    padding: var(--space-xs);
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
  }

  /* Modal */
  .modal-overlay {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(0, 0, 0, 0.7);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }

  .modal {
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    padding: var(--space-lg);
    min-width: 300px;
    max-width: 400px;
  }

  .modal-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: var(--space-md);
  }

  .modal-title {
    font-family: var(--font-mono);
    font-size: 14px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .close-btn {
    background: none;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 16px;
  }

  .close-btn:hover {
    color: var(--color-text-primary);
  }

  .modal-body {
    display: flex;
    flex-direction: column;
    gap: var(--space-sm);
  }

  .tag-suggestions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-xs);
    align-items: center;
  }

  .suggestion-label {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
  }

  .tag-suggestion {
    background: rgba(0, 212, 255, 0.1);
    border: 1px solid rgba(0, 212, 255, 0.3);
    border-radius: var(--radius-sm);
    padding: 2px 6px;
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-accent-cyan);
    cursor: pointer;
  }

  .tag-suggestion:hover {
    background: rgba(0, 212, 255, 0.2);
  }

  .modal-actions {
    display: flex;
    gap: var(--space-sm);
    justify-content: flex-end;
    margin-top: var(--space-md);
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

  .action-btn.success {
    background: var(--color-success);
    border-color: var(--color-success);
    color: var(--color-bg-primary);
  }

  .action-btn.success:hover:not(:disabled) {
    opacity: 0.9;
  }

  .action-btn.small {
    padding: 4px 8px;
    font-size: 10px;
  }

  /* Empty State */
  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    gap: var(--space-md);
    text-align: center;
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
    max-width: 300px;
  }

  /* Error Toast */
  .error-toast {
    position: absolute;
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
  }

  .dismiss-btn {
    background: none;
    border: none;
    color: white;
    cursor: pointer;
    padding: 4px;
    margin-left: var(--space-sm);
    opacity: 0.8;
  }

  .dismiss-btn:hover {
    opacity: 1;
  }
</style>
