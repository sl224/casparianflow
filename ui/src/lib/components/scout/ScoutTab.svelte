<script lang="ts">
  import { scoutStore, formatBytes } from "$lib/stores/scout.svelte";
  import { invoke } from "$lib/tauri";
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";

  import FilterBar from "./FilterBar.svelte";
  import FileList from "./FileList.svelte";
  import FileDetailPane from "./FileDetailPane.svelte";
  import TaggingRulesBar from "./TaggingRulesBar.svelte";

  // Modal state
  let showAddRuleModal = $state(false);
  let showManualTagModal = $state(false);
  let showPluginSelectModal = $state(false);
  let manualTagFileIds = $state<number[]>([]);
  let manualTagValue = $state("");
  let pluginSelectFileId = $state<number | null>(null);
  let availablePlugins = $state<string[]>([]);
  let isLoadingPlugins = $state(false);

  // Add rule form state
  let newRuleName = $state("");
  let newRulePattern = $state("");
  let newRuleTag = $state("");

  onMount(async () => {
    try {
      await scoutStore.initDb();
      await scoutStore.loadSources();
      await scoutStore.loadStatus();
    } catch (e) {
      console.error("[ScoutTab] Init failed:", e);
    }
  });

  // ============================================================================
  // Source Actions
  // ============================================================================

  async function handleSelectFolder() {
    try {
      console.log("[ScoutTab] Opening folder dialog...");
      const selected = await open({
        directory: true,
        multiple: false,
        title: "Select folder to scan",
      });

      if (selected) {
        const path = selected as string;
        const name = path.split("/").pop() || "source";
        const id = `src-${Date.now()}`;

        console.log("[ScoutTab] Adding source:", id, path);
        await scoutStore.addSource(id, name, path);

        console.log("[ScoutTab] Selecting source...");
        scoutStore.selectSource(id);

        console.log("[ScoutTab] Starting scan...");
        await scoutStore.scan(id);

        console.log("[ScoutTab] Scan complete, files:", scoutStore.files.length);
      }
    } catch (e) {
      console.error("[ScoutTab] handleSelectFolder error:", e);
      scoutStore.error = `Failed to add folder: ${e}`;
    }
  }

  function handleSourceChange(event: Event) {
    const target = event.target as HTMLSelectElement;
    const sourceId = target.value;
    if (sourceId) {
      scoutStore.selectSource(sourceId);
    }
  }

  // ============================================================================
  // Filter Bar Actions
  // ============================================================================

  async function handleScan() {
    if (!scoutStore.selectedSourceId) return;
    await scoutStore.scan(scoutStore.selectedSourceId);
  }

  async function handleAutoTag() {
    if (!scoutStore.selectedSourceId) return;
    await scoutStore.autoTag(scoutStore.selectedSourceId);
  }

  async function handleProcessTagged() {
    if (!scoutStore.selectedSourceId) return;
    const result = await scoutStore.submitAllTagged(scoutStore.selectedSourceId);

    if (result.noPlugin.length > 0) {
      const tags = [...new Set(result.noPlugin.map(([, tag]) => tag))];
      scoutStore.error = `No plugins configured for tags: ${tags.join(", ")}`;
    }
  }

  // ============================================================================
  // Tag Modal
  // ============================================================================

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

  // ============================================================================
  // Plugin Select Modal
  // ============================================================================

  async function openPluginSelectModal(fileId: number) {
    pluginSelectFileId = fileId;
    showPluginSelectModal = true;
    isLoadingPlugins = true;
    availablePlugins = [];

    try {
      // Load registered plugins from database (source of truth)
      const plugins = await invoke<string[]>("list_registered_plugins");
      availablePlugins = plugins;
    } catch (e) {
      console.error("[ScoutTab] Failed to load plugins:", e);
      availablePlugins = [];
    } finally {
      isLoadingPlugins = false;
    }
  }

  async function handlePluginSelect(pluginName: string) {
    if (pluginSelectFileId === null) return;
    await scoutStore.setManualPlugin(pluginSelectFileId, pluginName);
    showPluginSelectModal = false;
    pluginSelectFileId = null;
  }

  // ============================================================================
  // Process Single File
  // ============================================================================

  async function handleProcessFile(fileId: number) {
    const result = await scoutStore.submitTaggedFiles([fileId]);
    if (result.noPlugin.length > 0) {
      scoutStore.error = `No plugin configured for this file's tag`;
    }
  }

  // ============================================================================
  // Tagging Rules
  // ============================================================================

  function openAddRuleModal() {
    newRuleName = "";
    newRulePattern = "";
    newRuleTag = "";
    showAddRuleModal = true;
    if (scoutStore.selectedSourceId) {
      scoutStore.selectSource(scoutStore.selectedSourceId);
    }
  }

  function handlePatternInput(e: Event) {
    const target = e.target as HTMLInputElement;
    newRulePattern = target.value;
    scoutStore.updatePreviewPattern(target.value);
  }

  async function handleAddRule() {
    if (!scoutStore.selectedSourceId || !newRuleName || !newRulePattern || !newRuleTag) return;

    const id = `rule-${Date.now()}`;
    await scoutStore.addTaggingRule(id, newRuleName, scoutStore.selectedSourceId, newRulePattern, newRuleTag);

    showAddRuleModal = false;
    newRuleName = "";
    newRulePattern = "";
    newRuleTag = "";
    scoutStore.updatePreviewPattern("");
  }

  async function handleRemoveRule(ruleId: string) {
    await scoutStore.removeTaggingRule(ruleId);
  }

  // ============================================================================
  // Detail Pane
  // ============================================================================

  function handleCloseDetail() {
    scoutStore.selectFile(null);
  }

  function handleViewJob(jobId: number) {
    window.dispatchEvent(new CustomEvent('navigate-to-job', { detail: { jobId } }));
  }
</script>

<div class="scout-tab">
  <!-- Header -->
  <div class="header">
    <div class="header-left">
      <h2 class="title">SCOUT - File Discovery</h2>
      {#if scoutStore.sources.length > 0}
        <select class="source-select" onchange={handleSourceChange} value={scoutStore.selectedSourceId ?? ""}>
          <option value="" disabled>Select source...</option>
          {#each scoutStore.sources as source}
            {#if source}
              <option value={source.id}>{source.name}</option>
            {/if}
          {/each}
        </select>
      {/if}
    </div>
    <button class="action-btn primary" onclick={handleSelectFolder}>+ Add Folder</button>
  </div>

  {#if scoutStore.sources.length === 0}
    <!-- Empty State -->
    <div class="empty-state">
      <span class="empty-icon">&#128193;</span>
      <span class="empty-title">No Sources</span>
      <span class="empty-message">Add a folder to start discovering files and assigning tags.</span>
    </div>
  {:else if !scoutStore.selectedSourceId}
    <!-- No Source Selected -->
    <div class="empty-state">
      <span class="empty-icon">&#128193;</span>
      <span class="empty-title">Select a Source</span>
      <span class="empty-message">Choose a source from the dropdown above to view files.</span>
    </div>
  {:else}
    <!-- Filter Bar -->
    <FilterBar
      onScan={handleScan}
      onAutoTag={handleAutoTag}
      onProcess={handleProcessTagged}
    />

    <!-- Main Content: Two-Pane Layout -->
    <div class="main-content" class:has-selection={scoutStore.selectedFile !== null}>
      <!-- Left: File List -->
      <FileList onTagFiles={openManualTagModal} />

      <!-- Right: Detail Pane (shown when file selected) -->
      {#if scoutStore.selectedFile}
        <FileDetailPane
          file={scoutStore.selectedFile}
          onChangeTag={(id) => openManualTagModal([id])}
          onChangePlugin={openPluginSelectModal}
          onProcess={handleProcessFile}
          onViewJob={handleViewJob}
          onClose={handleCloseDetail}
        />
      {/if}
    </div>

    <!-- Tagging Rules Bar -->
    <TaggingRulesBar
      onAddRule={openAddRuleModal}
      onRemoveRule={handleRemoveRule}
    />
  {/if}

  <!-- Add Rule Modal -->
  {#if showAddRuleModal}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div class="modal-overlay" role="presentation" onclick={() => showAddRuleModal = false}>
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div class="modal" role="dialog" aria-modal="true" tabindex="-1" onclick={(e) => e.stopPropagation()}>
        <div class="modal-header">
          <span class="modal-title">Add Tagging Rule</span>
          <button class="close-btn" onclick={() => showAddRuleModal = false}>&#10005;</button>
        </div>
        <div class="modal-body">
          <div class="form-group">
            <label class="form-label" for="rule-name">Rule Name</label>
            <input
              id="rule-name"
              type="text"
              bind:value={newRuleName}
              placeholder="e.g., CSV Files"
              class="form-input"
            />
          </div>
          <div class="form-group">
            <label class="form-label" for="rule-pattern">Pattern</label>
            <input
              id="rule-pattern"
              type="text"
              value={newRulePattern}
              oninput={handlePatternInput}
              placeholder="e.g., *.csv or **/*.pdf"
              class="form-input"
            />
            {#if scoutStore.previewResult}
              <div class="preview-info" class:error={!scoutStore.previewResult.isValid}>
                {#if scoutStore.previewResult.isValid}
                  <span class="preview-count">{scoutStore.previewResult.matchedCount}</span>
                  <span class="preview-label">files match ({formatBytes(scoutStore.previewResult.matchedBytes)})</span>
                {:else}
                  <span class="preview-error">{scoutStore.previewResult.error}</span>
                {/if}
              </div>
            {/if}
          </div>
          <div class="form-group">
            <label class="form-label" for="rule-tag">Tag</label>
            <input
              id="rule-tag"
              type="text"
              bind:value={newRuleTag}
              placeholder="e.g., invoices"
              class="form-input tag-input"
            />
          </div>
        </div>
        <div class="modal-actions">
          <button class="action-btn" onclick={() => showAddRuleModal = false}>Cancel</button>
          <button
            class="action-btn primary"
            onclick={handleAddRule}
            disabled={!newRuleName || !newRulePattern || !newRuleTag || !scoutStore.previewResult?.isValid}
          >
            Add Rule
          </button>
        </div>
      </div>
    </div>
  {/if}

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

  <!-- Plugin Select Modal -->
  {#if showPluginSelectModal}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div class="modal-overlay" role="presentation" onclick={() => showPluginSelectModal = false}>
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div class="modal" role="dialog" aria-modal="true" tabindex="-1" onclick={(e) => e.stopPropagation()}>
        <div class="modal-header">
          <span class="modal-title">Override Plugin</span>
          <button class="close-btn" onclick={() => showPluginSelectModal = false}>&#10005;</button>
        </div>
        <div class="modal-body">
          <p class="modal-text">Select a plugin to process this file:</p>
          <div class="plugin-list">
            {#if isLoadingPlugins}
              <div class="plugin-loading">Loading plugins...</div>
            {:else if availablePlugins.length === 0}
              <div class="plugin-empty">
                No plugins available. Deploy a parser from Parser Lab first.
              </div>
            {:else}
              {#each availablePlugins as plugin}
                <button class="plugin-option" onclick={() => handlePluginSelect(plugin)}>
                  {plugin}
                </button>
              {/each}
            {/if}
          </div>
        </div>
        <div class="modal-actions">
          <button class="action-btn" onclick={() => showPluginSelectModal = false}>Cancel</button>
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
    gap: var(--space-md);
  }

  /* Header */
  .header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .header-left {
    display: flex;
    align-items: center;
    gap: var(--space-md);
  }

  .title {
    font-family: var(--font-mono);
    font-size: 14px;
    font-weight: 600;
    letter-spacing: 1px;
    color: var(--color-text-muted);
    margin: 0;
  }

  .source-select {
    padding: 6px 24px 6px 10px;
    background: var(--color-bg-tertiary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-primary);
    cursor: pointer;
    appearance: none;
    background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 12 12'%3E%3Cpath fill='%23888' d='M6 8L2 4h8z'/%3E%3C/svg%3E");
    background-repeat: no-repeat;
    background-position: right 8px center;
    min-width: 150px;
  }

  .source-select:focus {
    outline: none;
    border-color: var(--color-accent-cyan);
  }

  /* Main Content */
  .main-content {
    flex: 1;
    display: grid;
    grid-template-columns: 1fr;
    gap: var(--space-md);
    min-height: 0;
  }

  .main-content.has-selection {
    grid-template-columns: 3fr 2fr;
  }

  /* Mobile: stack vertically */
  @media (max-width: 768px) {
    .main-content.has-selection {
      grid-template-columns: 1fr;
      grid-template-rows: 1fr auto;
    }
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
    max-width: 450px;
    width: 90%;
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
    gap: var(--space-md);
  }

  .modal-text {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-secondary);
    margin: 0;
  }

  .modal-actions {
    display: flex;
    gap: var(--space-sm);
    justify-content: flex-end;
    margin-top: var(--space-md);
  }

  /* Form */
  .form-group {
    display: flex;
    flex-direction: column;
    gap: var(--space-xs);
  }

  .form-label {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .form-input {
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

  .form-input.tag-input {
    border-color: rgba(0, 255, 136, 0.3);
  }

  .preview-info {
    display: flex;
    align-items: baseline;
    gap: var(--space-xs);
    font-family: var(--font-mono);
    font-size: 11px;
    padding: var(--space-xs) 0;
  }

  .preview-info.error {
    color: var(--color-error);
  }

  .preview-count {
    font-size: 16px;
    font-weight: 700;
    color: var(--color-accent-cyan);
  }

  .preview-label {
    color: var(--color-text-secondary);
  }

  .preview-error {
    color: var(--color-error);
  }

  /* Tag Suggestions */
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

  /* Plugin List */
  .plugin-list {
    display: flex;
    flex-direction: column;
    gap: var(--space-xs);
  }

  .plugin-option {
    padding: var(--space-sm) var(--space-md);
    background: var(--color-bg-tertiary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-primary);
    cursor: pointer;
    text-align: left;
  }

  .plugin-option:hover {
    border-color: var(--color-accent-cyan);
    background: rgba(0, 212, 255, 0.05);
  }

  .plugin-loading,
  .plugin-empty {
    padding: var(--space-md);
    text-align: center;
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-muted);
    font-style: italic;
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
