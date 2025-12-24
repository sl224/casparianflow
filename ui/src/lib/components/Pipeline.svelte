<script lang="ts">
  import { SvelteFlow, Background, Controls, MiniMap } from "@xyflow/svelte";
  import "@xyflow/svelte/dist/style.css";
  import { topologyStore } from "$lib/stores/topology.svelte";
  import PluginNode from "./PluginNode.svelte";
  import TopicNode from "./TopicNode.svelte";

  const nodeTypes = {
    plugin: PluginNode,
    topic: TopicNode,
  };
</script>

<div class="pipeline-container">
  {#if topologyStore.loading}
    <div class="loading-overlay">
      <div class="loading-spinner"></div>
      <span>Loading topology...</span>
    </div>
  {:else if topologyStore.error}
    <div class="error-overlay">
      <span class="error-icon">!</span>
      <span class="error-message">{topologyStore.error}</span>
      <button class="retry-btn" onclick={() => topologyStore.refresh()}>Retry</button>
    </div>
  {:else if topologyStore.isEmpty}
    <div class="empty-state">
      <span class="empty-icon">&#9673;</span>
      <span class="empty-title">No Pipeline Configured</span>
      <span class="empty-message">Add plugins and topics to see the data flow graph</span>
    </div>
  {:else}
    <SvelteFlow
      nodes={topologyStore.nodes}
      edges={topologyStore.edges}
      {nodeTypes}
      fitView
      minZoom={0.5}
      maxZoom={2}
      defaultEdgeOptions={{
        type: "smoothstep",
        animated: false,
      }}
    >
      <Background bgColor="var(--color-bg-primary)" gap={20} />
      <Controls />
      <MiniMap
        nodeColor={(node) => {
          if (node.type === "plugin") return "var(--color-accent-cyan)";
          return "var(--color-accent-green)";
        }}
        maskColor="rgba(0, 0, 0, 0.8)"
      />
    </SvelteFlow>
  {/if}

  <div class="toolbar">
    <button class="toolbar-btn" onclick={() => topologyStore.refresh()} title="Refresh topology">
      &#8635;
    </button>
    {#if topologyStore.lastRefresh}
      <span class="last-refresh">
        {topologyStore.lastRefresh.toLocaleTimeString()}
      </span>
    {/if}
  </div>
</div>

<style>
  .pipeline-container {
    position: relative;
    width: 100%;
    height: 100%;
    background: var(--color-bg-primary);
  }

  .loading-overlay,
  .error-overlay,
  .empty-state {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 16px;
    background: var(--color-bg-primary);
    color: var(--color-text-secondary);
    font-family: var(--font-mono);
  }

  .loading-spinner {
    width: 40px;
    height: 40px;
    border: 3px solid var(--color-border);
    border-top-color: var(--color-accent-cyan);
    border-radius: 50%;
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .error-icon {
    width: 48px;
    height: 48px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--color-error);
    color: var(--color-bg-primary);
    border-radius: 50%;
    font-size: 24px;
    font-weight: bold;
  }

  .error-message {
    color: var(--color-error);
  }

  .retry-btn {
    padding: 8px 16px;
    background: var(--color-bg-tertiary);
    border: 1px solid var(--color-border);
    border-radius: 4px;
    color: var(--color-text-primary);
    cursor: pointer;
    font-family: var(--font-mono);
    transition: all 0.2s ease;
  }

  .retry-btn:hover {
    border-color: var(--color-accent-cyan);
    color: var(--color-accent-cyan);
  }

  .empty-icon {
    font-size: 48px;
    color: var(--color-text-muted);
  }

  .empty-title {
    font-size: 18px;
    color: var(--color-text-primary);
  }

  .empty-message {
    font-size: 14px;
    color: var(--color-text-muted);
  }

  .toolbar {
    position: absolute;
    top: 16px;
    right: 16px;
    display: flex;
    align-items: center;
    gap: 12px;
    z-index: 10;
  }

  .toolbar-btn {
    width: 36px;
    height: 36px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 6px;
    color: var(--color-text-secondary);
    cursor: pointer;
    font-size: 18px;
    transition: all 0.2s ease;
  }

  .toolbar-btn:hover {
    border-color: var(--color-accent-cyan);
    color: var(--color-accent-cyan);
  }

  .last-refresh {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-muted);
    background: var(--color-bg-card);
    padding: 6px 10px;
    border-radius: 4px;
  }

  /* Override Svelte Flow defaults for dark theme */
  :global(.svelte-flow) {
    background: var(--color-bg-primary) !important;
  }

  :global(.svelte-flow__edge-path) {
    stroke: var(--color-border) !important;
    stroke-width: 2px !important;
  }

  :global(.svelte-flow__edge.animated .svelte-flow__edge-path) {
    stroke: var(--color-accent-cyan) !important;
  }

  :global(.svelte-flow__controls) {
    background: var(--color-bg-card) !important;
    border: 1px solid var(--color-border) !important;
    border-radius: 6px !important;
  }

  :global(.svelte-flow__controls-button) {
    background: var(--color-bg-card) !important;
    border-color: var(--color-border) !important;
    fill: var(--color-text-secondary) !important;
  }

  :global(.svelte-flow__controls-button:hover) {
    background: var(--color-bg-tertiary) !important;
  }

  :global(.svelte-flow__minimap) {
    background: var(--color-bg-card) !important;
    border: 1px solid var(--color-border) !important;
    border-radius: 6px !important;
  }
</style>
