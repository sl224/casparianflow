/**
 * Jobs Store - Completed job outputs and data querying
 */

import { invoke } from "$lib/tauri";

/** Job output from backend */
export interface JobOutput {
  jobId: number;
  pluginName: string;
  status: string;
  outputPath: string | null;
  completedAt: string | null;
}

/** Query result from DuckDB */
export interface QueryResult {
  columns: string[];
  rows: unknown[][];
  rowCount: number;
  executionTimeMs: number;
}

/** Detailed job information for LogViewer */
export interface JobDetails {
  jobId: number;
  pluginName: string;
  status: string;
  outputPath: string | null;
  errorMessage: string | null;
  resultSummary: string | null;
  claimTime: string | null;
  endTime: string | null;
  retryCount: number;
  /** Captured logs (stdout, stderr, logging) from plugin execution */
  logs: string | null;
}

/** Reactive jobs store */
class JobsStore {
  // List of completed jobs
  jobs = $state<JobOutput[]>([]);

  // Current query result
  queryResult = $state<QueryResult | null>(null);

  // Job details for LogViewer
  selectedJob = $state<JobOutput | null>(null);
  jobDetails = $state<JobDetails | null>(null);
  loadingDetails = $state(false);
  detailsError = $state<string | null>(null);

  // Loading states
  loadingJobs = $state(false);
  loadingQuery = $state(false);

  // Errors
  jobsError = $state<string | null>(null);
  queryError = $state<string | null>(null);

  // Currently selected file for querying
  selectedFile = $state<string | null>(null);

  constructor() {
    if (typeof window !== "undefined") {
      setTimeout(() => this.refreshJobs(), 300);
    }
  }

  /** Refresh job list from backend */
  async refreshJobs(limit: number = 50): Promise<void> {
    this.loadingJobs = true;
    this.jobsError = null;

    try {
      this.jobs = await invoke<JobOutput[]>("get_job_outputs", { limit });
      console.log("[JobsStore] Loaded", this.jobs.length, "jobs");
    } catch (err) {
      this.jobsError = err instanceof Error ? err.message : String(err);
      console.error("[JobsStore] Failed to load jobs:", this.jobsError);
    } finally {
      this.loadingJobs = false;
    }
  }

  /** Query a parquet file */
  async queryFile(filePath: string, sql?: string): Promise<void> {
    this.loadingQuery = true;
    this.queryError = null;
    this.selectedFile = filePath;

    try {
      this.queryResult = await invoke<QueryResult>("query_parquet", {
        filePath,
        sql: sql || null,
      });
      console.log(
        "[JobsStore] Query returned",
        this.queryResult.rowCount,
        "rows in",
        this.queryResult.executionTimeMs,
        "ms"
      );
    } catch (err) {
      this.queryError = err instanceof Error ? err.message : String(err);
      this.queryResult = null;
      console.error("[JobsStore] Query failed:", this.queryError);
    } finally {
      this.loadingQuery = false;
    }
  }

  /** Clear current query result */
  clearQuery(): void {
    this.queryResult = null;
    this.selectedFile = null;
    this.queryError = null;
  }

  /** Select a job and fetch its details for LogViewer */
  async selectJob(job: JobOutput): Promise<void> {
    this.selectedJob = job;
    this.loadingDetails = true;
    this.detailsError = null;

    try {
      this.jobDetails = await invoke<JobDetails>("get_job_details", {
        jobId: job.jobId,
      });
      console.log("[JobsStore] Loaded details for job", job.jobId);
    } catch (err) {
      this.detailsError = err instanceof Error ? err.message : String(err);
      this.jobDetails = null;
      console.error("[JobsStore] Failed to load job details:", this.detailsError);
    } finally {
      this.loadingDetails = false;
    }
  }

  /** Close the job details view */
  closeJobDetails(): void {
    this.selectedJob = null;
    this.jobDetails = null;
    this.detailsError = null;
  }

  /** Get jobs that have queryable output files */
  get queryableJobs(): JobOutput[] {
    return this.jobs.filter(j => j.outputPath !== null);
  }

  /** Check if a job is failed */
  isJobFailed(job: JobOutput): boolean {
    return job.status === "FAILED";
  }
}

export const jobsStore = new JobsStore();
