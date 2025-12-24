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
</script>

<div class="plugin-node" class:active={data.status === "active"}>
  <div class="node-header">
    <span class="node-icon">&#9881;</span>
    <span class="node-type">PLUGIN</span>
  </div>
  <div class="node-label">{data.label}</div>
  {#if data.metadata.tags}
    <div class="node-tags">{data.metadata.tags}</div>
  {/if}
  <Handle type="source" position={Position.Right} />
  <Handle type="target" position={Position.Left} />
</div>

<style>
  .plugin-node {
    background: var(--color-bg-card, #16161f);
    border: 2px solid var(--color-border, #2a2a3a);
    border-radius: 8px;
    padding: 12px 16px;
    min-width: 160px;
    font-family: var(--font-mono, monospace);
    transition: all 0.2s ease;
  }

  .plugin-node.active {
    border-color: var(--color-accent-cyan, #00d4ff);
    box-shadow: 0 0 20px rgba(0, 212, 255, 0.2);
  }

  .node-header {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 8px;
  }

  .node-icon {
    font-size: 14px;
    color: var(--color-accent-cyan, #00d4ff);
  }

  .node-type {
    font-size: 10px;
    letter-spacing: 1px;
    color: var(--color-text-muted, #555566);
  }

  .node-label {
    font-size: 14px;
    font-weight: 600;
    color: var(--color-text-primary, #e0e0e8);
  }

  .node-tags {
    margin-top: 8px;
    font-size: 10px;
    color: var(--color-text-secondary, #8888aa);
    padding: 4px 8px;
    background: var(--color-bg-tertiary, #1a1a24);
    border-radius: 4px;
  }
</style>
