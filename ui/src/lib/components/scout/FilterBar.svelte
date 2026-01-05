<script lang="ts">
  import { scoutStore, type FilterType } from "$lib/stores/scout.svelte";

  interface Props {
    onScan: () => void;
    onAutoTag: () => void;
    onProcess: () => void;
  }

  let { onScan, onAutoTag, onProcess }: Props = $props();

  const filterOptions: { value: FilterType; label: string }[] = [
    { value: "all", label: "All" },
    { value: "manual", label: "Manual" },
    { value: "pending", label: "Pending" },
    { value: "tagged", label: "Tagged" },
    { value: "queued", label: "Queued" },
    { value: "processed", label: "Processed" },
    { value: "failed", label: "Failed" },
  ];

  function getFilterCount(filter: FilterType): number {
    switch (filter) {
      case "all": return scoutStore.files.length;
      case "manual": return scoutStore.manualFiles.length;
      case "pending": return scoutStore.pendingFiles.length;
      case "tagged": return scoutStore.taggedFiles.length;
      case "queued": return scoutStore.queuedFiles.length;
      case "processed": return scoutStore.processedFiles.length;
      case "failed": return scoutStore.failedFiles.length;
      default: return 0;
    }
  }

  function handleFilterChange(event: Event) {
    const target = event.target as HTMLSelectElement;
    scoutStore.setFilter(target.value as FilterType);
  }
</script>

<div class="filter-bar">
  <div class="filter-section">
    <label class="filter-label" for="filter-select">Filter:</label>
    <select
      id="filter-select"
      class="filter-select"
      value={scoutStore.currentFilter}
      onchange={handleFilterChange}
    >
      {#each filterOptions as option}
        {@const count = getFilterCount(option.value)}
        <option value={option.value}>
          {option.label} ({count})
        </option>
      {/each}
    </select>
  </div>

  <div class="action-section">
    <button
      class="action-btn"
      onclick={onScan}
      disabled={scoutStore.scanning || !scoutStore.selectedSourceId}
    >
      {scoutStore.scanning ? "Scanning..." : "Scan"}
    </button>
    <button
      class="action-btn primary"
      onclick={onAutoTag}
      disabled={scoutStore.tagging || !scoutStore.hasTaggingRules || scoutStore.pendingFiles.length === 0}
      title={!scoutStore.hasTaggingRules ? "Add tagging rules first" : scoutStore.pendingFiles.length === 0 ? "No pending files" : "Auto-tag pending files"}
    >
      {scoutStore.tagging ? "Tagging..." : "Auto-tag"}
    </button>
    <button
      class="action-btn success"
      onclick={onProcess}
      disabled={scoutStore.submitting || scoutStore.taggedFiles.length === 0}
      title={scoutStore.taggedFiles.length === 0 ? "Tag files first" : "Process tagged files"}
    >
      {scoutStore.submitting ? "Processing..." : "Process Tagged"}
    </button>
  </div>
</div>

<style>
  .filter-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--space-sm) var(--space-md);
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    gap: var(--space-md);
  }

  .filter-section {
    display: flex;
    align-items: center;
    gap: var(--space-sm);
  }

  .filter-label {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .filter-select {
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
  }

  .filter-select:focus {
    outline: none;
    border-color: var(--color-accent-cyan);
  }

  .action-section {
    display: flex;
    gap: var(--space-xs);
  }

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
</style>
