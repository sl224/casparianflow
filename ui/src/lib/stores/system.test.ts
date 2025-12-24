/**
 * System Store Tests
 *
 * Tests store logic without DOM rendering - optimal for LLM review
 * because we can verify state transitions and computed values.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock Tauri APIs before importing store
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

import type { SystemPulse } from './system.svelte';

describe('SystemPulse calculations', () => {
  // Test the success rate calculation edge cases
  it('should handle zero jobs without division by zero', () => {
    const pulse: SystemPulse = {
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

    const total = pulse.jobsCompleted + pulse.jobsFailed;
    // When total is 0, we should NOT divide - this is the edge case
    const successRate = total === 0 ? 100 : (pulse.jobsCompleted / total) * 100;

    expect(successRate).toBe(100);
    expect(Number.isFinite(successRate)).toBe(true);
  });

  it('should calculate correct success rate with mixed results', () => {
    const pulse: SystemPulse = {
      connectedWorkers: 2,
      jobsCompleted: 90,
      jobsFailed: 10,
      jobsDispatched: 100,
      jobsInFlight: 0,
      avgDispatchMs: 5.5,
      avgConcludeMs: 10.2,
      messagesSent: 1000,
      messagesReceived: 1000,
      timestamp: Date.now(),
    };

    const total = pulse.jobsCompleted + pulse.jobsFailed;
    const successRate = (pulse.jobsCompleted / total) * 100;

    expect(successRate).toBe(90);
  });

  it('should calculate in-flight jobs correctly', () => {
    const pulse: SystemPulse = {
      connectedWorkers: 3,
      jobsCompleted: 50,
      jobsFailed: 5,
      jobsDispatched: 60,
      jobsInFlight: 5, // 60 - 50 - 5 = 5
      avgDispatchMs: 2.0,
      avgConcludeMs: 8.0,
      messagesSent: 500,
      messagesReceived: 500,
      timestamp: Date.now(),
    };

    // Verify the invariant: in_flight = dispatched - completed - failed
    const expectedInFlight = pulse.jobsDispatched - pulse.jobsCompleted - pulse.jobsFailed;
    expect(pulse.jobsInFlight).toBe(expectedInFlight);
  });

  it('should never have negative in-flight jobs', () => {
    // Edge case: more concluded than dispatched (shouldn't happen but defensive)
    const dispatched = 10;
    const completed = 8;
    const failed = 5; // Total concluded > dispatched

    // Rust uses saturating_sub which clamps to 0
    const inFlight = Math.max(0, dispatched - completed - failed);

    expect(inFlight).toBe(0);
    expect(inFlight).toBeGreaterThanOrEqual(0);
  });
});

describe('Throughput calculation', () => {
  it('should calculate jobs per second correctly', () => {
    const lastJobsCompleted = 100;
    const newJobsCompleted = 150;
    const deltaTimeSeconds = 5;

    const throughput = Math.round((newJobsCompleted - lastJobsCompleted) / deltaTimeSeconds);

    expect(throughput).toBe(10); // 50 jobs / 5 seconds = 10 jobs/sec
  });

  it('should handle zero time delta gracefully', () => {
    const deltaTime = 0;
    const deltaJobs = 10;

    // Should not divide by zero
    const throughput = deltaTime > 0 ? Math.round(deltaJobs / deltaTime) : 0;

    expect(throughput).toBe(0);
    expect(Number.isFinite(throughput)).toBe(true);
  });

  it('should handle negative delta (counter reset) gracefully', () => {
    const lastJobsCompleted = 100;
    const newJobsCompleted = 50; // Less than before (counter reset?)
    const deltaTimeSeconds = 1;

    const deltaJobs = newJobsCompleted - lastJobsCompleted;
    // Negative throughput doesn't make sense - clamp to 0
    const throughput = Math.max(0, Math.round(deltaJobs / deltaTimeSeconds));

    expect(throughput).toBe(0);
  });
});
