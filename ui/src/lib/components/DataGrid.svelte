<script lang="ts">
  import { jobsStore, type QueryResult } from "$lib/stores/jobs.svelte";

  interface Props {
    result: QueryResult;
  }

  let { result }: Props = $props();

  // Format cell values for display
  function formatValue(value: unknown): string {
    if (value === null || value === undefined) {
      return "NULL";
    }
    if (typeof value === "object") {
      return JSON.stringify(value);
    }
    if (typeof value === "number") {
      // Format large numbers with commas
      if (Number.isInteger(value) && Math.abs(value) >= 1000) {
        return value.toLocaleString();
      }
      // Format floats with reasonable precision
      if (!Number.isInteger(value)) {
        return value.toFixed(4);
      }
    }
    return String(value);
  }

  function isNull(value: unknown): boolean {
    return value === null || value === undefined;
  }

  function isNumeric(value: unknown): boolean {
    return typeof value === "number";
  }
</script>

<div class="data-grid-container">
  <div class="grid-header">
    <span class="grid-info">
      <span class="row-count">{result.rowCount.toLocaleString()}</span> rows
      <span class="separator">|</span>
      <span class="exec-time">{result.executionTimeMs}ms</span>
    </span>
    <button class="close-btn" onclick={() => jobsStore.clearQuery()}>
      &times;
    </button>
  </div>

  <div class="grid-wrapper">
    <table class="data-table">
      <thead>
        <tr>
          {#each result.columns as column}
            <th>{column}</th>
          {/each}
        </tr>
      </thead>
      <tbody>
        {#each result.rows as row}
          <tr>
            {#each row as cell}
              <td class:null={isNull(cell)} class:numeric={isNumeric(cell)}>
                {formatValue(cell)}
              </td>
            {/each}
          </tr>
        {/each}
      </tbody>
    </table>
  </div>

  {#if jobsStore.selectedFile}
    <div class="grid-footer">
      <span class="file-path" title={jobsStore.selectedFile}>
        {jobsStore.selectedFile.split("/").pop()}
      </span>
    </div>
  {/if}
</div>

<style>
  .data-grid-container {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    overflow: hidden;
  }

  .grid-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 12px 16px;
    background: var(--color-bg-secondary);
    border-bottom: 1px solid var(--color-border);
    font-family: var(--font-mono);
    font-size: 12px;
  }

  .grid-info {
    color: var(--color-text-secondary);
  }

  .row-count {
    color: var(--color-accent-cyan);
    font-weight: 600;
  }

  .separator {
    margin: 0 8px;
    color: var(--color-text-muted);
  }

  .exec-time {
    color: var(--color-accent-green);
  }

  .close-btn {
    width: 24px;
    height: 24px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: 1px solid var(--color-border);
    border-radius: 4px;
    color: var(--color-text-secondary);
    cursor: pointer;
    font-size: 16px;
    transition: all 0.2s ease;
  }

  .close-btn:hover {
    border-color: var(--color-error);
    color: var(--color-error);
  }

  .grid-wrapper {
    flex: 1;
    overflow: auto;
  }

  .data-table {
    width: 100%;
    border-collapse: collapse;
    font-family: var(--font-mono);
    font-size: 12px;
  }

  .data-table th {
    position: sticky;
    top: 0;
    background: var(--color-bg-tertiary);
    padding: 10px 12px;
    text-align: left;
    font-weight: 600;
    color: var(--color-text-primary);
    border-bottom: 1px solid var(--color-border);
    white-space: nowrap;
  }

  .data-table td {
    padding: 8px 12px;
    border-bottom: 1px solid var(--color-border);
    color: var(--color-text-secondary);
    max-width: 300px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .data-table tr:hover td {
    background: var(--color-bg-tertiary);
  }

  .data-table td.null {
    color: var(--color-text-muted);
    font-style: italic;
  }

  .data-table td.numeric {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .grid-footer {
    padding: 8px 16px;
    background: var(--color-bg-secondary);
    border-top: 1px solid var(--color-border);
  }

  .file-path {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-muted);
  }
</style>
