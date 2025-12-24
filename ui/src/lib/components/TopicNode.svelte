<script lang="ts">
  import { Handle, Position } from "@xyflow/svelte";

  interface Props {
    data: {
      label: string;
      status: string | null;
      metadata: Record<string, string>;
    };
  }

  let { data }: Props = $props();

  // Use $derived for reactive values from props
  const mode = $derived(data.metadata.mode || "unknown");
  const isOutput = $derived(mode === "write" || mode === "rw");
</script>

<div class="topic-node" class:output={isOutput}>
  <div class="node-header">
    <span class="node-icon">{isOutput ? "&#9654;" : "&#9664;"}</span>
    <span class="node-type">TOPIC</span>
  </div>
  <div class="node-label">{data.label}</div>
  <div class="node-uri" title={data.metadata.uri}>
    {data.metadata.uri?.split("/").pop() || "..."}
  </div>
  <Handle type="source" position={Position.Right} />
  <Handle type="target" position={Position.Left} />
</div>

<style>
  .topic-node {
    background: var(--color-bg-tertiary, #1a1a24);
    border: 2px solid var(--color-border, #2a2a3a);
    border-radius: 8px;
    padding: 12px 16px;
    min-width: 140px;
    font-family: var(--font-mono, monospace);
    transition: all 0.2s ease;
  }

  .topic-node.output {
    border-color: var(--color-accent-green, #00ff88);
  }

  .topic-node:not(.output) {
    border-color: var(--color-accent-magenta, #ff00aa);
  }

  .node-header {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 8px;
  }

  .node-icon {
    font-size: 12px;
  }

  .topic-node.output .node-icon {
    color: var(--color-accent-green, #00ff88);
  }

  .topic-node:not(.output) .node-icon {
    color: var(--color-accent-magenta, #ff00aa);
  }

  .node-type {
    font-size: 10px;
    letter-spacing: 1px;
    color: var(--color-text-muted, #555566);
  }

  .node-label {
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text-primary, #e0e0e8);
  }

  .node-uri {
    margin-top: 6px;
    font-size: 10px;
    color: var(--color-text-muted, #555566);
    max-width: 120px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
