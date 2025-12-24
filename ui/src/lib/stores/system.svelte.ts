/**
 * System Store - Real-time metrics from the embedded Sentinel
 *
 * Listens to "system-pulse" events from the Rust backend and exposes
 * reactive state for the dashboard.
 */

import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

/** System pulse payload from Rust backend */
export interface SystemPulse {
  connectedWorkers: number;
  jobsCompleted: number;
  jobsFailed: number;
  jobsDispatched: number;
  jobsInFlight: number;
  avgDispatchMs: number;
  avgConcludeMs: number;
  messagesSent: number;
  messagesReceived: number;
  timestamp: number;
}

/** Initial empty pulse state */
const INITIAL_PULSE: SystemPulse = {
  connectedWorkers: 0,
  jobsCompleted: 0,
  jobsFailed: 0,
  jobsDispatched: 0,
  jobsInFlight: 0,
  avgDispatchMs: 0,
  avgConcludeMs: 0,
  messagesSent: 0,
  messagesReceived: 0,
  timestamp: 0,
};

/** Reactive system state using Svelte 5 runes */
class SystemStore {
  // Current pulse data
  pulse = $state<SystemPulse>(INITIAL_PULSE);

  // Connection state
  isConnected = $state(false);

  // Heartbeat animation trigger (increments on each pulse)
  heartbeatTick = $state(0);

  // Last pulse timestamp for calculating delta
  private lastPulseTime = 0;

  // Throughput calculation (jobs per second)
  private lastJobsCompleted = 0;
  throughput = $state(0);

  // Initialization flag
  private initialized = false;

  // Sentinel bind address
  bindAddress = $state("ipc://...");

  constructor() {
    // Defer initialization to avoid issues with SSR/module loading
    if (typeof window !== "undefined") {
      // Use setTimeout to ensure Tauri is ready
      setTimeout(() => this.init(), 100);
    }
  }

  private async init() {
    if (this.initialized) return;
    this.initialized = true;

    try {
      console.log("[SystemStore] Initializing...");

      // Get bind address
      this.bindAddress = await invoke<string>("get_bind_address");

      // Listen for real-time updates first
      await listen<SystemPulse>("system-pulse", (event) => {
        const newPulse = event.payload;

        // Calculate throughput (jobs/sec)
        const now = Date.now() / 1000;
        const deltaTime = now - this.lastPulseTime;
        if (deltaTime > 0 && this.lastPulseTime > 0) {
          const deltaJobs = newPulse.jobsCompleted - this.lastJobsCompleted;
          this.throughput = Math.round(deltaJobs / deltaTime);
        }

        this.lastPulseTime = now;
        this.lastJobsCompleted = newPulse.jobsCompleted;
        this.pulse = newPulse;
        this.heartbeatTick++;
        this.isConnected = true;
      });

      // Get initial state
      const initialPulse = await invoke<SystemPulse>("get_system_pulse");
      this.pulse = initialPulse;
      this.lastJobsCompleted = initialPulse.jobsCompleted;
      this.isConnected = true;

      console.log("[SystemStore] Initialized and listening for pulses");
    } catch (error) {
      console.error("[SystemStore] Failed to initialize:", error);
      this.isConnected = false;
    }
  }

  /** Get Prometheus-formatted metrics (for debugging) */
  async getPrometheusMetrics(): Promise<string> {
    return invoke<string>("get_prometheus_metrics");
  }

  /** Check if sentinel is running */
  async isSentinelRunning(): Promise<boolean> {
    return invoke<boolean>("is_sentinel_running");
  }
}

// Singleton instance
export const systemStore = new SystemStore();

// Derived values for convenience
export function getConnectedWorkers(): number {
  return systemStore.pulse.connectedWorkers;
}

export function getThroughput(): number {
  return systemStore.throughput;
}

export function getJobStats() {
  return {
    completed: systemStore.pulse.jobsCompleted,
    failed: systemStore.pulse.jobsFailed,
    inFlight: systemStore.pulse.jobsInFlight,
  };
}
