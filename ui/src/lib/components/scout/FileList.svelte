<script lang="ts">
  import { scoutStore } from "$lib/stores/scout.svelte";
  import FileListItem from "./FileListItem.svelte";

  interface Props {
    onTagFiles?: (fileIds: number[]) => void;
  }

  let { onTagFiles }: Props = $props();

  let selectAll = $state(false);
  let lastClickedIndex = $state<number | null>(null);

  // Handle select all checkbox
  function handleSelectAllChange() {
    if (selectAll) {
      scoutStore.clearFileSelection();
    } else {
      scoutStore.selectAllFiles();
    }
    selectAll = !selectAll;
  }

  // Handle individual file selection (for detail pane)
  function handleFileSelect(fileId: number) {
    scoutStore.selectFile(fileId);
  }

  // Handle checkbox toggle (for bulk operations)
  function handleFileCheck(fileId: number, index: number, event?: MouseEvent) {
    // Shift-click for range select
    if (event?.shiftKey && lastClickedIndex !== null) {
      const files = scoutStore.filteredFiles;
      const start = Math.min(lastClickedIndex, index);
      const end = Math.max(lastClickedIndex, index);

      for (let i = start; i <= end; i++) {
        const file = files[i];
        if (file && !scoutStore.selectedFileIds.has(file.id)) {
          scoutStore.toggleFileSelection(file.id);
        }
      }
    } else {
      scoutStore.toggleFileSelection(fileId);
    }

    lastClickedIndex = index;

    // Update selectAll state
    selectAll = scoutStore.selectedFileIds.size === scoutStore.filteredFiles.length;
  }

  // Get count of selected untagged files for bulk tag action
  function getSelectedUntaggedCount(): number {
    return scoutStore.filteredFiles.filter(
      f => scoutStore.selectedFileIds.has(f.id) && !f.tag
    ).length;
  }

  function handleBulkTag() {
    const untaggedIds = scoutStore.filteredFiles
      .filter(f => scoutStore.selectedFileIds.has(f.id) && !f.tag)
      .map(f => f.id);

    if (untaggedIds.length > 0 && onTagFiles) {
      onTagFiles(untaggedIds);
    }
  }
</script>

<div class="file-list-container">
  <!-- Header with select all and bulk actions -->
  <div class="file-list-header">
    <div class="select-all-section">
      <input
        type="checkbox"
        class="select-all-checkbox"
        checked={selectAll}
        onchange={handleSelectAllChange}
        id="select-all"
      />
      <label for="select-all" class="select-all-label">
        {#if scoutStore.selectedFileIds.size > 0}
          {scoutStore.selectedFileIds.size} selected
        {:else}
          Select all
        {/if}
      </label>
    </div>

    {#if scoutStore.selectedFileIds.size > 0}
      <div class="bulk-actions">
        {#if getSelectedUntaggedCount() > 0}
          <button class="bulk-btn" onclick={handleBulkTag}>
            Tag {getSelectedUntaggedCount()} untagged
          </button>
        {/if}
        <button class="bulk-btn clear" onclick={() => scoutStore.clearFileSelection()}>
          Clear
        </button>
      </div>
    {/if}

    <div class="file-count">
      {scoutStore.filteredFiles.length} files
    </div>
  </div>

  <!-- Scrollable file list -->
  <div class="file-list-scroll">
    {#if scoutStore.filteredFiles.length === 0}
      <div class="empty-state">
        {#if scoutStore.currentFilter === "all"}
          <span class="empty-icon">&#128193;</span>
          <span class="empty-text">No files discovered yet</span>
          <span class="empty-hint">Click "Scan" to discover files</span>
        {:else}
          <span class="empty-text">No files match filter "{scoutStore.currentFilter}"</span>
        {/if}
      </div>
    {:else}
      {#each scoutStore.filteredFiles as file, index (file.id)}
        <FileListItem
          {file}
          selected={scoutStore.selectedFileId === file.id}
          checked={scoutStore.selectedFileIds.has(file.id)}
          onSelect={handleFileSelect}
          onCheck={(id) => handleFileCheck(id, index)}
        />
      {/each}
    {/if}
  </div>
</div>

<style>
  .file-list-container {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    overflow: hidden;
  }

  .file-list-header {
    display: flex;
    align-items: center;
    gap: var(--space-md);
    padding: var(--space-sm) var(--space-md);
    background: var(--color-bg-tertiary);
    border-bottom: 1px solid var(--color-border);
    flex-shrink: 0;
  }

  .select-all-section {
    display: flex;
    align-items: center;
    gap: var(--space-xs);
  }

  .select-all-checkbox {
    width: 14px;
    height: 14px;
    cursor: pointer;
    accent-color: var(--color-accent-cyan);
  }

  .select-all-label {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-secondary);
    cursor: pointer;
  }

  .bulk-actions {
    display: flex;
    gap: var(--space-xs);
  }

  .bulk-btn {
    padding: 4px 8px;
    background: var(--color-bg-primary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-secondary);
    cursor: pointer;
  }

  .bulk-btn:hover {
    border-color: var(--color-accent-cyan);
    color: var(--color-accent-cyan);
  }

  .bulk-btn.clear {
    color: var(--color-text-muted);
  }

  .file-count {
    margin-left: auto;
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
  }

  .file-list-scroll {
    flex: 1;
    overflow-y: auto;
    padding: var(--space-xs);
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 200px;
    gap: var(--space-sm);
    color: var(--color-text-muted);
  }

  .empty-icon {
    font-size: 32px;
    opacity: 0.5;
  }

  .empty-text {
    font-family: var(--font-mono);
    font-size: 12px;
  }

  .empty-hint {
    font-family: var(--font-mono);
    font-size: 10px;
    opacity: 0.7;
  }
</style>
