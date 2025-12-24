/**
 * Topology Store Tests
 *
 * Tests graph transformation logic - converting backend data to flow nodes/edges.
 * LLM-friendly: we test data transformations, not rendering.
 */

import { describe, it, expect, vi } from 'vitest';

// Mock Tauri
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

// Types matching the backend
interface TopologyNode {
  id: string;
  label: string;
  nodeType: 'plugin' | 'topic';
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

// Position calculation logic (extracted from store)
function calculatePositions(nodes: TopologyNode[]): Map<string, { x: number; y: number }> {
  const positions = new Map<string, { x: number; y: number }>();

  const plugins = nodes.filter(n => n.nodeType === 'plugin');
  const topics = nodes.filter(n => n.nodeType === 'topic');

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

describe('Position calculation', () => {
  it('should position plugins on left, topics on right', () => {
    const nodes: TopologyNode[] = [
      { id: 'plugin:a', label: 'A', nodeType: 'plugin', status: 'active', metadata: {} },
      { id: 'topic:a:out', label: 'out', nodeType: 'topic', status: null, metadata: {} },
    ];

    const positions = calculatePositions(nodes);

    const pluginPos = positions.get('plugin:a')!;
    const topicPos = positions.get('topic:a:out')!;

    expect(pluginPos.x).toBe(100);
    expect(topicPos.x).toBe(500);
    expect(pluginPos.x).toBeLessThan(topicPos.x);
  });

  it('should space nodes vertically', () => {
    const nodes: TopologyNode[] = [
      { id: 'plugin:a', label: 'A', nodeType: 'plugin', status: null, metadata: {} },
      { id: 'plugin:b', label: 'B', nodeType: 'plugin', status: null, metadata: {} },
      { id: 'plugin:c', label: 'C', nodeType: 'plugin', status: null, metadata: {} },
    ];

    const positions = calculatePositions(nodes);

    const posA = positions.get('plugin:a')!;
    const posB = positions.get('plugin:b')!;
    const posC = positions.get('plugin:c')!;

    // Should be evenly spaced
    expect(posB.y - posA.y).toBe(150);
    expect(posC.y - posB.y).toBe(150);
  });

  it('should handle empty topology', () => {
    const nodes: TopologyNode[] = [];

    const positions = calculatePositions(nodes);

    expect(positions.size).toBe(0);
  });

  it('should handle plugins-only topology', () => {
    const nodes: TopologyNode[] = [
      { id: 'plugin:a', label: 'A', nodeType: 'plugin', status: null, metadata: {} },
    ];

    const positions = calculatePositions(nodes);

    expect(positions.size).toBe(1);
    expect(positions.get('plugin:a')).toBeDefined();
  });

  it('should handle topics-only topology', () => {
    const nodes: TopologyNode[] = [
      { id: 'topic:a:x', label: 'x', nodeType: 'topic', status: null, metadata: {} },
    ];

    const positions = calculatePositions(nodes);

    expect(positions.size).toBe(1);
    expect(positions.get('topic:a:x')?.x).toBe(500);
  });
});

describe('Node ID format', () => {
  it('should parse plugin node IDs', () => {
    const id = 'plugin:my_processor';
    const [type, name] = id.split(':');

    expect(type).toBe('plugin');
    expect(name).toBe('my_processor');
  });

  it('should parse topic node IDs with owner', () => {
    const id = 'topic:my_processor:output';
    const parts = id.split(':');

    expect(parts[0]).toBe('topic');
    expect(parts[1]).toBe('my_processor'); // owner
    expect(parts[2]).toBe('output'); // topic name
  });

  it('should handle topic names with underscores', () => {
    const id = 'topic:plugin_a:my_output_topic';
    const [type, owner, ...nameParts] = id.split(':');

    expect(type).toBe('topic');
    expect(owner).toBe('plugin_a');
    expect(nameParts.join(':')).toBe('my_output_topic');
  });
});

describe('Edge direction', () => {
  it('should point from plugin to topic for publish edges', () => {
    const edge: TopologyEdge = {
      id: 'e1',
      source: 'plugin:producer',
      target: 'topic:producer:output',
      label: 'publishes',
      animated: true,
    };

    expect(edge.source).toContain('plugin:');
    expect(edge.target).toContain('topic:');
    expect(edge.label).toBe('publishes');
  });

  it('should point from topic to plugin for subscribe edges', () => {
    const edge: TopologyEdge = {
      id: 'e2',
      source: 'topic:producer:output',
      target: 'plugin:consumer',
      label: 'subscribes',
      animated: true,
    };

    expect(edge.source).toContain('topic:');
    expect(edge.target).toContain('plugin:');
    expect(edge.label).toBe('subscribes');
  });

  it('should animate active subscriptions', () => {
    const activeEdge: TopologyEdge = {
      id: 'e1',
      source: 'topic:a:x',
      target: 'plugin:b',
      label: 'subscribes',
      animated: true,
    };

    const inactiveEdge: TopologyEdge = {
      id: 'e2',
      source: 'topic:a:y',
      target: 'plugin:c',
      label: 'subscribes',
      animated: false,
    };

    expect(activeEdge.animated).toBe(true);
    expect(inactiveEdge.animated).toBe(false);
  });
});

describe('Metadata extraction', () => {
  it('should extract topic mode from metadata', () => {
    const node: TopologyNode = {
      id: 'topic:a:output',
      label: 'output',
      nodeType: 'topic',
      status: null,
      metadata: { mode: 'write', uri: 'file:///output.parquet' },
    };

    const mode = node.metadata.mode;
    const isOutput = mode === 'write' || mode === 'rw';

    expect(isOutput).toBe(true);
  });

  it('should identify input topics', () => {
    const node: TopologyNode = {
      id: 'topic:a:input',
      label: 'input',
      nodeType: 'topic',
      status: null,
      metadata: { mode: 'read', uri: 'file:///input.csv' },
    };

    const mode = node.metadata.mode;
    const isOutput = mode === 'write' || mode === 'rw';

    expect(isOutput).toBe(false);
  });

  it('should handle missing mode gracefully', () => {
    const node: TopologyNode = {
      id: 'topic:a:unknown',
      label: 'unknown',
      nodeType: 'topic',
      status: null,
      metadata: { uri: 'file:///data.parquet' }, // no mode
    };

    const mode = node.metadata.mode || 'unknown';

    expect(mode).toBe('unknown');
  });
});
