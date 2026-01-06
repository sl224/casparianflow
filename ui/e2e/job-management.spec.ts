/**
 * Job Management E2E Tests
 *
 * Tests critical paths for job lifecycle:
 * 1. Job cancellation by user
 * 2. Job failure due to stale worker (heartbeat timeout)
 * 3. Error message consistency for different failure reasons
 *
 * These tests use NO MOCKS - they test the actual bridge and database.
 */
import { test, expect } from "@playwright/test";
import { join } from "path";
import { homedir } from "os";

const BRIDGE_URL = "http://localhost:9999";
const HOME = homedir();
const CF_DIR = join(HOME, ".casparian_flow");

async function bridgeCall<T = unknown>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  const response = await fetch(`${BRIDGE_URL}/api/rpc`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ command, args }),
  });
  const data = await response.json();
  if (data.error) throw new Error(data.error);
  return data.result as T;
}

// Clean up test jobs after all tests to prevent polluting the real database
test.afterAll(async () => {
  try {
    const result = await bridgeCall<{ deleted: number }>("cleanup_test_jobs");
    console.log(`Cleaned up ${result.deleted} test jobs`);
  } catch (e) {
    console.warn("Failed to cleanup test jobs:", e);
  }
});

test.describe("Job Cancellation", () => {
  test("user can cancel a QUEUED job", async () => {
    // 1. Create a job in QUEUED state
    const { jobId } = await bridgeCall<{ jobId: number }>("create_processing_job", {
      pluginName: "test_plugin_cancel_queued",
      inputFile: "/tmp/test.csv",
    });
    expect(jobId).toBeDefined();

    // 2. Verify job is QUEUED
    const beforeCancel = await bridgeCall<{
      status: string;
      error_message: string | null;
    }>("get_job_status", { jobId });
    expect(beforeCancel.status).toBe("QUEUED");
    expect(beforeCancel.error_message).toBeNull();

    // 3. Cancel the job
    const cancelResult = await bridgeCall<string>("cancel_job", { jobId });
    expect(cancelResult).toContain("cancelled");

    // 4. Verify job is CANCELLED with proper error message
    const afterCancel = await bridgeCall<{
      status: string;
      error_message: string | null;
    }>("get_job_status", { jobId });
    expect(afterCancel.status).toBe("CANCELLED");
    expect(afterCancel.error_message).toBe("Cancelled by user");
  });

  test("user can cancel a RUNNING job", async () => {
    // 1. Create a job
    const { jobId } = await bridgeCall<{ jobId: number }>("create_processing_job", {
      pluginName: "test_plugin_cancel_running",
      inputFile: "/tmp/test.csv",
    });

    // 2. Simulate job starting (set to RUNNING)
    await bridgeCall("update_job_status", {
      jobId,
      status: "RUNNING",
    });

    // 3. Verify job is RUNNING
    const beforeCancel = await bridgeCall<{ status: string }>("get_job_status", { jobId });
    expect(beforeCancel.status).toBe("RUNNING");

    // 4. Cancel the job
    const cancelResult = await bridgeCall<string>("cancel_job", { jobId });
    expect(cancelResult).toContain("cancelled");

    // 5. Verify job is CANCELLED with proper error message
    const afterCancel = await bridgeCall<{
      status: string;
      error_message: string | null;
    }>("get_job_status", { jobId });
    expect(afterCancel.status).toBe("CANCELLED");
    expect(afterCancel.error_message).toBe("Cancelled by user");
  });

  test("cannot cancel COMPLETED job", async () => {
    // 1. Create and complete a job
    const { jobId } = await bridgeCall<{ jobId: number }>("create_processing_job", {
      pluginName: "test_plugin_completed",
      inputFile: "/tmp/test.csv",
    });

    await bridgeCall("update_job_status", {
      jobId,
      status: "COMPLETED",
      resultSummary: "10 rows processed",
    });

    // 2. Attempt to cancel should fail
    let error: Error | null = null;
    try {
      await bridgeCall("cancel_job", { jobId });
    } catch (e) {
      error = e as Error;
    }

    expect(error).toBeTruthy();
    expect(error!.message).toContain("not in cancellable state");

    // 3. Job should still be COMPLETED
    const job = await bridgeCall<{ status: string }>("get_job_status", { jobId });
    expect(job.status).toBe("COMPLETED");
  });

  test("cannot cancel FAILED job", async () => {
    // 1. Create and fail a job
    const { jobId } = await bridgeCall<{ jobId: number }>("create_processing_job", {
      pluginName: "test_plugin_failed",
      inputFile: "/tmp/test.csv",
    });

    await bridgeCall("update_job_status", {
      jobId,
      status: "FAILED",
      errorMessage: "Some processing error",
    });

    // 2. Attempt to cancel should fail
    let error: Error | null = null;
    try {
      await bridgeCall("cancel_job", { jobId });
    } catch (e) {
      error = e as Error;
    }

    expect(error).toBeTruthy();
    expect(error!.message).toContain("not in cancellable state");

    // 3. Job should still be FAILED with original error
    const job = await bridgeCall<{
      status: string;
      error_message: string | null;
    }>("get_job_status", { jobId });
    expect(job.status).toBe("FAILED");
    expect(job.error_message).toBe("Some processing error");
  });
});

test.describe("Stale Worker Detection", () => {
  test("RUNNING job is marked FAILED when worker goes stale", async () => {
    // 1. Create a job and set it to RUNNING (simulating worker picked it up)
    const { jobId } = await bridgeCall<{ jobId: number }>("create_processing_job", {
      pluginName: "test_plugin_stale",
      inputFile: "/tmp/test.csv",
    });

    await bridgeCall("update_job_status", {
      jobId,
      status: "RUNNING",
    });

    // 2. Verify job is RUNNING
    const beforeStale = await bridgeCall<{ status: string }>("get_job_status", { jobId });
    expect(beforeStale.status).toBe("RUNNING");

    // 3. Simulate stale worker cleanup (sentinel detects worker heartbeat timeout)
    const staleResult = await bridgeCall<string>("simulate_stale_worker", { jobId });
    expect(staleResult).toContain("stale worker");

    // 4. Verify job is FAILED with stale heartbeat message
    const afterStale = await bridgeCall<{
      status: string;
      error_message: string | null;
    }>("get_job_status", { jobId });
    expect(afterStale.status).toBe("FAILED");
    expect(afterStale.error_message).toBe("Worker became unresponsive (stale heartbeat)");
  });

  test("stale worker detection only affects RUNNING jobs", async () => {
    // 1. Create a QUEUED job
    const { jobId } = await bridgeCall<{ jobId: number }>("create_processing_job", {
      pluginName: "test_plugin_queued_stale",
      inputFile: "/tmp/test.csv",
    });

    // 2. Attempt stale worker simulation on QUEUED job should fail
    let error: Error | null = null;
    try {
      await bridgeCall("simulate_stale_worker", { jobId });
    } catch (e) {
      error = e as Error;
    }

    expect(error).toBeTruthy();
    expect(error!.message).toContain("not running");

    // 3. Job should still be QUEUED
    const job = await bridgeCall<{ status: string }>("get_job_status", { jobId });
    expect(job.status).toBe("QUEUED");
  });
});

test.describe("Error Message Consistency", () => {
  test("different failure reasons have distinct error messages", async () => {
    const testCases = [
      {
        name: "user_cancel",
        setup: async (jobId: number) => {
          await bridgeCall("cancel_job", { jobId });
        },
        expectedStatus: "CANCELLED",
        expectedMessage: "Cancelled by user",
      },
      {
        name: "stale_worker",
        setup: async (jobId: number) => {
          // First set to RUNNING (required for stale worker)
          await bridgeCall("update_job_status", { jobId, status: "RUNNING" });
          await bridgeCall("simulate_stale_worker", { jobId });
        },
        expectedStatus: "FAILED",
        expectedMessage: "Worker became unresponsive (stale heartbeat)",
      },
      {
        name: "processing_error",
        setup: async (jobId: number) => {
          await bridgeCall("update_job_status", {
            jobId,
            status: "FAILED",
            errorMessage: "Plugin raised exception: ValueError",
          });
        },
        expectedStatus: "FAILED",
        expectedMessage: "Plugin raised exception: ValueError",
      },
    ];

    for (const tc of testCases) {
      // Create job
      const { jobId } = await bridgeCall<{ jobId: number }>("create_processing_job", {
        pluginName: `test_consistency_${tc.name}`,
        inputFile: "/tmp/test.csv",
      });

      // Apply failure condition
      await tc.setup(jobId);

      // Verify status and message
      const job = await bridgeCall<{
        status: string;
        error_message: string | null;
      }>("get_job_status", { jobId });

      expect(job.status, `${tc.name}: status`).toBe(tc.expectedStatus);
      expect(job.error_message, `${tc.name}: error_message`).toBe(tc.expectedMessage);
    }
  });

  test("CANCELLED is a distinct status from FAILED", async () => {
    // Create two jobs
    const { jobId: cancelledJobId } = await bridgeCall<{ jobId: number }>("create_processing_job", {
      pluginName: "test_status_cancelled",
      inputFile: "/tmp/test.csv",
    });

    const { jobId: failedJobId } = await bridgeCall<{ jobId: number }>("create_processing_job", {
      pluginName: "test_status_failed",
      inputFile: "/tmp/test.csv",
    });

    // Cancel one, fail the other
    await bridgeCall("cancel_job", { jobId: cancelledJobId });
    await bridgeCall("update_job_status", {
      jobId: failedJobId,
      status: "RUNNING",
    });
    await bridgeCall("simulate_stale_worker", { jobId: failedJobId });

    // Verify distinct statuses
    const cancelledJob = await bridgeCall<{ status: string }>("get_job_status", { jobId: cancelledJobId });
    const failedJob = await bridgeCall<{ status: string }>("get_job_status", { jobId: failedJobId });

    expect(cancelledJob.status).toBe("CANCELLED");
    expect(failedJob.status).toBe("FAILED");
    expect(cancelledJob.status).not.toBe(failedJob.status);
  });
});

test.describe("Job Lifecycle Transitions", () => {
  test("valid state transitions: QUEUED -> RUNNING -> COMPLETED", async () => {
    const { jobId } = await bridgeCall<{ jobId: number }>("create_processing_job", {
      pluginName: "test_lifecycle_success",
      inputFile: "/tmp/test.csv",
    });

    // QUEUED (initial)
    let job = await bridgeCall<{ status: string }>("get_job_status", { jobId });
    expect(job.status).toBe("QUEUED");

    // QUEUED -> RUNNING
    await bridgeCall("update_job_status", { jobId, status: "RUNNING" });
    job = await bridgeCall<{ status: string }>("get_job_status", { jobId });
    expect(job.status).toBe("RUNNING");

    // RUNNING -> COMPLETED
    await bridgeCall("update_job_status", {
      jobId,
      status: "COMPLETED",
      resultSummary: "100 rows processed",
    });
    job = await bridgeCall<{
      status: string;
      result_summary: string | null;
    }>("get_job_status", { jobId });
    expect(job.status).toBe("COMPLETED");
    expect(job.result_summary).toBe("100 rows processed");
  });

  test("valid state transitions: QUEUED -> CANCELLED", async () => {
    const { jobId } = await bridgeCall<{ jobId: number }>("create_processing_job", {
      pluginName: "test_lifecycle_cancel_queued",
      inputFile: "/tmp/test.csv",
    });

    // QUEUED (initial)
    let job = await bridgeCall<{ status: string }>("get_job_status", { jobId });
    expect(job.status).toBe("QUEUED");

    // QUEUED -> CANCELLED
    await bridgeCall("cancel_job", { jobId });
    job = await bridgeCall<{
      status: string;
      error_message: string | null;
    }>("get_job_status", { jobId });
    expect(job.status).toBe("CANCELLED");
    expect(job.error_message).toBe("Cancelled by user");
  });

  test("valid state transitions: RUNNING -> FAILED (stale)", async () => {
    const { jobId } = await bridgeCall<{ jobId: number }>("create_processing_job", {
      pluginName: "test_lifecycle_stale",
      inputFile: "/tmp/test.csv",
    });

    // QUEUED -> RUNNING
    await bridgeCall("update_job_status", { jobId, status: "RUNNING" });
    let job = await bridgeCall<{ status: string }>("get_job_status", { jobId });
    expect(job.status).toBe("RUNNING");

    // RUNNING -> FAILED (stale worker)
    await bridgeCall("simulate_stale_worker", { jobId });
    job = await bridgeCall<{
      status: string;
      error_message: string | null;
    }>("get_job_status", { jobId });
    expect(job.status).toBe("FAILED");
    expect(job.error_message).toContain("stale heartbeat");
  });
});
