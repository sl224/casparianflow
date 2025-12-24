/**
 * Topology Store - Pipeline graph data from database
 *
 * Queries the backend for plugin/topic topology and transforms it
 * for use with Svelte Flow.
 */

import { invoke } from "@tauri-apps/api/core";
import type { Node, Edge } from "@xyflow/svelte";

/** Raw topology from Rust backend */
interface TopologyNode {
  id: string;
  label: string;
  nodeType: "plugin" | "topic";
  status: string | null;
  metadata: Record<string, string>;
}

interface TopologyEdge {
  id: string;
  source: string;
  target: string;
  label: string | null;
  animated: boolean;
}

interface PipelineTopology {
  nodes: TopologyNode[];
  edges: TopologyEdge[];
}

/** Node positions using auto-layout */
function calculatePositions(nodes: TopologyNode[]): Map<string, { x: number; y: number }> {
  const positions = new Map<string, { x: number; y: number }>();

  // Separate plugins and topics
  const plugins = nodes.filter(n => n.nodeType === "plugin");
  const topics = nodes.filter(n => n.nodeType === "topic");

  // Layout: plugins on left, topics on right
  const pluginX = 100;
  const topicX = 500;
  const startY = 100;
  const gapY = 150;

  plugins.forEach((node, i) => {
    positions.set(node.id, { x: pluginX, y: startY + i * gapY });
  });

  topics.forEach((node, i) => {
    positions.set(node.id, { x: topicX, y: startY + i * gapY });
  });

  return positions;
}

/** Convert backend topology to Svelte Flow format */
function toFlowElements(topology: PipelineTopology): { nodes: Node[]; edges: Edge[] } {
  const positions = calculatePositions(topology.nodes);

  const nodes: Node[] = topology.nodes.map(node => {
    const pos = positions.get(node.id) || { x: 0, y: 0 };
    return {
      id: node.id,
      type: node.nodeType === "plugin" ? "plugin" : "topic",
      position: pos,
      data: {
        label: node.label,
        status: node.status,
        metadata: node.metadata,
      },
    };
  });

  const edges: Edge[] = topology.edges.map(edge => ({
    id: edge.id,
    source: edge.source,
    target: edge.target,
    label: edge.label || undefined,
    animated: edge.animated,
    type: "smoothstep",
    style: edge.animated ? "stroke: var(--color-accent-cyan); stroke-width: 2px;" : undefined,
  }));

  return { nodes, edges };
}

/** Reactive topology store using Svelte 5 runes */
class TopologyStore {
  // Flow elements
  nodes = $state<Node[]>([]);
  edges = $state<Edge[]>([]);

  // Loading state
  loading = $state(false);
  error = $state<string | null>(null);

  // Last refresh time
  lastRefresh = $state<Date | null>(null);

  constructor() {
    // Auto-load on init (deferred for Tauri readiness)
    if (typeof window !== "undefined") {
      setTimeout(() => this.refresh(), 200);
    }
  }

  /** Refresh topology from backend */
  async refresh(): Promise<void> {
    this.loading = true;
    this.error = null;

    try {
      const topology = await invoke<PipelineTopology>("get_topology");
      const { nodes, edges } = toFlowElements(topology);

      this.nodes = nodes;
      this.edges = edges;
      this.lastRefresh = new Date();

      console.log("[TopologyStore] Loaded", nodes.length, "nodes,", edges.length, "edges");
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      this.error = message;
      console.error("[TopologyStore] Failed to load topology:", message);
    } finally {
      this.loading = false;
    }
  }

  /** Check if topology is empty (no plugins/topics configured) */
  get isEmpty(): boolean {
    return this.nodes.length === 0;
  }
}

// Singleton instance
export const topologyStore = new TopologyStore();
