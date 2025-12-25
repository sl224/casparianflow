/**
 * Tauri API Mock for Browser Testing
 *
 * Provides mock implementations of Tauri APIs when running outside the Tauri webview.
 * This enables Playwright testing of the UI without requiring the full Tauri backend.
 */

// Check if we're in Tauri or browser
export const isTauri = typeof window !== 'undefined' && '__TAURI__' in window;

// Mock data for testing
const mockRoutingRules = [
  { id: 1, pattern: 'data/sales/*.csv', tag: 'finance', priority: 100, enabled: true, description: 'Sales CSV files' },
  { id: 2, pattern: 'data/marketing/**/*.json', tag: 'marketing', priority: 90, enabled: true, description: 'Marketing JSON data' },
  { id: 3, pattern: 'data/logs/*.log', tag: 'logs', priority: 50, enabled: true, description: 'Application logs' },
  { id: 4, pattern: '**/*.parquet', tag: 'processed', priority: 80, enabled: false, description: 'Processed parquet files' },
];

const mockTopicConfigs = [
  { id: 1, pluginName: 'slow_processor', topicName: 'processed_output', uri: 'parquet://output/processed.parquet', mode: 'write' },
  { id: 2, pluginName: 'data_validator', topicName: 'validated_data', uri: 'parquet://output/validated.parquet', mode: 'write' },
];

const mockJobOutputs = [
  { jobId: 100, pluginName: 'slow_processor', status: 'COMPLETED', outputPath: '/tmp/output.parquet', completedAt: new Date().toISOString() },
  { jobId: 101, pluginName: 'data_validator', status: 'COMPLETED', outputPath: '/tmp/validated.parquet', completedAt: new Date().toISOString() },
  { jobId: 102, pluginName: 'broken_plugin', status: 'FAILED', outputPath: null, completedAt: new Date().toISOString() },
];

let nextRuleId = 100;

// Mock invoke function
export async function mockInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  console.log('[TauriMock] invoke:', cmd, args);

  // Simulate network delay
  await new Promise(r => setTimeout(r, 50));

  switch (cmd) {
    case 'get_routing_rules':
      return [...mockRoutingRules] as T;

    case 'create_routing_rule': {
      const newRule = {
        id: nextRuleId++,
        pattern: args?.pattern as string,
        tag: args?.tag as string,
        priority: (args?.priority as number) || 0,
        enabled: true,
        description: (args?.description as string) || '',
      };
      mockRoutingRules.push(newRule);
      return newRule.id as T;
    }

    case 'update_routing_rule': {
      const rule = args?.rule as typeof mockRoutingRules[0];
      const idx = mockRoutingRules.findIndex(r => r.id === rule.id);
      if (idx >= 0) {
        mockRoutingRules[idx] = rule;
      }
      return undefined as T;
    }

    case 'delete_routing_rule': {
      const id = args?.id as number;
      const idx = mockRoutingRules.findIndex(r => r.id === id);
      if (idx >= 0) {
        mockRoutingRules.splice(idx, 1);
      }
      return undefined as T;
    }

    case 'get_topic_configs':
      return [...mockTopicConfigs] as T;

    case 'update_topic_uri': {
      const { id, uri } = args as { id: number; uri: string };
      const topic = mockTopicConfigs.find(t => t.id === id);
      if (topic) {
        topic.uri = uri;
      }
      return undefined as T;
    }

    case 'get_job_outputs':
      return [...mockJobOutputs] as T;

    case 'get_job_details': {
      const jobId = args?.jobId as number;
      const job = mockJobOutputs.find(j => j.jobId === jobId);
      if (job) {
        return {
          jobId: job.jobId,
          pluginName: job.pluginName,
          status: job.status,
          outputPath: job.outputPath,
          errorMessage: job.status === 'FAILED' ? 'Mock error message' : null,
          resultSummary: job.outputPath,
          claimTime: new Date(Date.now() - 60000).toISOString(),
          endTime: job.completedAt,
          retryCount: 0,
          logs: '[INFO] Mock log entry\n[DEBUG] Processing...\n[INFO] Complete',
        } as T;
      }
      throw new Error('Job not found');
    }

    case 'get_topology':
      return {
        nodes: [
          { id: 'plugin:slow_processor', label: 'slow_processor', nodeType: 'plugin', status: 'active', metadata: {}, x: 100, y: 50 },
          { id: 'topic:slow_processor:processed_output', label: 'processed_output', nodeType: 'topic', status: null, metadata: {}, x: 500, y: 50 },
        ],
        edges: [
          { id: 'e0', source: 'plugin:slow_processor', target: 'topic:slow_processor:processed_output', label: 'publishes', animated: true },
        ],
      } as T;

    case 'query_parquet':
      return {
        columns: ['id', 'value', 'timestamp'],
        rows: [
          [1, 10.5, '2024-01-01'],
          [2, 20.3, '2024-01-02'],
          [3, 30.1, '2024-01-03'],
        ],
        rowCount: 3,
        executionTimeMs: 42,
      } as T;

    case 'get_system_pulse':
      return {
        connectedWorkers: 2,
        jobsCompleted: 150,
        jobsFailed: 3,
        jobsDispatched: 155,
        jobsInFlight: 2,
        avgDispatchMs: 1.5,
        avgConcludeMs: 12.3,
        messagesSent: 1000,
        messagesReceived: 998,
        timestamp: Math.floor(Date.now() / 1000),
      } as T;

    case 'is_sentinel_running':
      return true as T;

    case 'get_bind_address':
      return 'ipc:///tmp/casparian_mock.sock' as T;

    default:
      console.warn('[TauriMock] Unhandled command:', cmd);
      return undefined as T;
  }
}

// Mock state for stable incrementing values
let mockPulseState = {
  jobsCompleted: 150,
  jobsDispatched: 155,
  messagesSent: 1000,
  messagesReceived: 998,
};

// Mock listen function
export function mockListen(event: string, callback: (event: { payload: unknown }) => void): () => void {
  console.log('[TauriMock] listen:', event);

  // For system-pulse, emit periodic updates with stable incrementing values
  if (event === 'system-pulse') {
    const interval = setInterval(() => {
      // Simulate slow job completion (1 job every ~5 seconds)
      if (Math.random() < 0.1) {
        mockPulseState.jobsCompleted++;
        mockPulseState.jobsDispatched++;
      }
      mockPulseState.messagesSent += 2;
      mockPulseState.messagesReceived += 2;

      callback({
        payload: {
          connectedWorkers: 2,
          jobsCompleted: mockPulseState.jobsCompleted,
          jobsFailed: 3,
          jobsDispatched: mockPulseState.jobsDispatched,
          jobsInFlight: mockPulseState.jobsDispatched - mockPulseState.jobsCompleted - 3,
          avgDispatchMs: 1.5,
          avgConcludeMs: 12.3,
          messagesSent: mockPulseState.messagesSent,
          messagesReceived: mockPulseState.messagesReceived,
          timestamp: Math.floor(Date.now() / 1000),
        },
      });
    }, 500);

    return () => clearInterval(interval);
  }

  return () => {};
}

// Mock window operations
export const mockWindow = {
  minimize: async () => console.log('[TauriMock] window.minimize'),
  toggleMaximize: async () => console.log('[TauriMock] window.toggleMaximize'),
  close: async () => console.log('[TauriMock] window.close'),
};
