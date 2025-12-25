<script lang="ts">
  import { createVirtualizer, type SvelteVirtualizer } from "@tanstack/svelte-virtual";
  import { jobsStore, type QueryResult } from "$lib/stores/jobs.svelte";
  import type { Readable } from "svelte/store";

  interface Props {
    result: QueryResult;
  }

  let { result }: Props = $props();

  // Scroll container reference
  let scrollContainer: HTMLDivElement | undefined = $state();

  // Row height estimate
  const ROW_HEIGHT = 36;

  // Create virtualizer for rows - use $effect for Svelte 5 compatibility
  let virtualizer: Readable<SvelteVirtualizer<HTMLDivElement, Element>> | undefined = $state();

  $effect(() => {
    if (scrollContainer) {
      virtualizer = createVirtualizer<HTMLDivElement, Element>({
        count: result.rows.length,
        getScrollElement: () => scrollContainer!,
        estimateSize: () => ROW_HEIGHT,
        overscan: 10,
      });
    }
  });

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

  <!-- Sticky column headers -->
  <div class="column-headers">
    {#each result.columns as column}
      <div class="column-header">{column}</div>
    {/each}
  </div>

  <!-- Virtualized scroll container -->
  <div class="grid-body" bind:this={scrollContainer}>
    {#if virtualizer && $virtualizer}
      <div
        class="virtual-list"
        style="height: {$virtualizer.getTotalSize()}px; position: relative;"
      >
        {#each $virtualizer.getVirtualItems() as virtualRow (virtualRow.index)}
          {@const row = result.rows[virtualRow.index]}
          <div
            class="virtual-row"
            style="
              position: absolute;
              top: 0;
              left: 0;
              width: 100%;
              height: {virtualRow.size}px;
              transform: translateY({virtualRow.start}px);
            "
          >
            {#each row as cell, colIndex}
              <div
                class="cell"
                class:null={isNull(cell)}
                class:numeric={isNumeric(cell)}
              >
                {formatValue(cell)}
              </div>
            {/each}
          </div>
        {/each}
      </div>
    {:else}
      <div class="loading-rows">Loading...</div>
    {/if}
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
    flex-shrink: 0;
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

  /* Column headers - sticky */
  .column-headers {
    display: flex;
    background: var(--color-bg-tertiary);
    border-bottom: 1px solid var(--color-border);
    flex-shrink: 0;
  }

  .column-header {
    flex: 1;
    min-width: 120px;
    max-width: 300px;
    padding: 10px 12px;
    font-family: var(--font-mono);
    font-size: 12px;
    font-weight: 600;
    color: var(--color-text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* Virtualized body */
  .grid-body {
    flex: 1;
    overflow-y: auto;
    overflow-x: auto;
  }

  .virtual-list {
    width: 100%;
  }

  .virtual-row {
    display: flex;
    border-bottom: 1px solid var(--color-border);
    transition: background 0.1s ease;
  }

  .virtual-row:hover {
    background: var(--color-bg-tertiary);
  }

  .cell {
    flex: 1;
    min-width: 120px;
    max-width: 300px;
    padding: 8px 12px;
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .cell.null {
    color: var(--color-text-muted);
    font-style: italic;
  }

  .cell.numeric {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .grid-footer {
    padding: 8px 16px;
    background: var(--color-bg-secondary);
    border-top: 1px solid var(--color-border);
    flex-shrink: 0;
  }

  .file-path {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-muted);
  }

  .loading-rows {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100px;
    color: var(--color-text-muted);
    font-family: var(--font-mono);
    font-size: 12px;
  }
</style>
