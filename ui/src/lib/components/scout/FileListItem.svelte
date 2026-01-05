<script lang="ts">
  import { scoutStore, formatBytes, type ScannedFile } from "$lib/stores/scout.svelte";

  interface Props {
    file: ScannedFile;
    selected: boolean;
    checked: boolean;
    onSelect: (fileId: number) => void;
    onCheck: (fileId: number) => void;
  }

  let { file, selected, checked, onSelect, onCheck }: Props = $props();

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

  function handleCheckboxClick(event: Event) {
    event.stopPropagation();
    onCheck(file.id);
  }

  function handleRowClick() {
    onSelect(file.id);
  }

  $effect(() => {
    // This effect tracks `file` reactively for debugging if needed
  });
</script>

<div
  class="file-item"
  class:selected
  class:untagged={!file.tag}
  role="button"
  tabindex="0"
  onclick={handleRowClick}
  onkeydown={(e) => e.key === 'Enter' && handleRowClick()}
>
  <input
    type="checkbox"
    class="file-checkbox"
    checked={checked}
    onclick={handleCheckboxClick}
  />

  <span class="file-name" title={file.relPath}>
    {file.relPath}
  </span>

  {#if scoutStore.isManualFile(file)}
    <span class="manual-indicator" title="Manual override">&#9995;</span>
  {/if}

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

  <span class="file-size">{formatBytes(file.size)}</span>
</div>

<style>
  .file-item {
    display: flex;
    align-items: center;
    gap: var(--space-sm);
    padding: var(--space-xs) var(--space-sm);
    font-family: var(--font-mono);
    font-size: 11px;
    cursor: pointer;
    border-radius: var(--radius-sm);
    transition: background 0.1s ease;
  }

  .file-item:hover {
    background: var(--color-bg-tertiary);
  }

  .file-item.selected {
    background: rgba(0, 212, 255, 0.1);
    border-left: 2px solid var(--color-accent-cyan);
    padding-left: calc(var(--space-sm) - 2px);
  }

  .file-item.untagged {
    background: rgba(255, 170, 0, 0.03);
  }

  .file-item.untagged:hover {
    background: rgba(255, 170, 0, 0.08);
  }

  .file-checkbox {
    width: 14px;
    height: 14px;
    cursor: pointer;
    accent-color: var(--color-accent-cyan);
    flex-shrink: 0;
  }

  .file-name {
    flex: 1;
    color: var(--color-text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }

  .manual-indicator {
    font-size: 12px;
    flex-shrink: 0;
    cursor: help;
  }

  .file-tag {
    color: var(--color-accent-cyan);
    background: rgba(0, 212, 255, 0.1);
    padding: 2px 6px;
    border-radius: 3px;
    font-size: 10px;
    flex-shrink: 0;
  }

  .file-no-tag {
    color: #ffaa00;
    background: rgba(255, 170, 0, 0.1);
    padding: 2px 6px;
    border-radius: 3px;
    font-size: 10px;
    font-style: italic;
    flex-shrink: 0;
  }

  .file-status {
    padding: 2px 6px;
    border-radius: 3px;
    font-size: 9px;
    text-transform: uppercase;
    flex-shrink: 0;
  }

  .file-size {
    color: var(--color-text-muted);
    font-size: 10px;
    min-width: 60px;
    text-align: right;
    flex-shrink: 0;
  }
</style>
