<script lang="ts">
  import { jobsStore, type JobOutput } from "$lib/stores/jobs.svelte";
  import LogViewer from "./LogViewer.svelte";
  import { onMount, onDestroy } from "svelte";

  let pollInterval: ReturnType<typeof setInterval> | null = null;
  let isPolling = $state(false);

  // Check if there are any active jobs (QUEUED or RUNNING)
  function hasActiveJobs(): boolean {
    return jobsStore.jobs.some(j => j.status === "QUEUED" || j.status === "RUNNING");
  }

  // Start polling when active jobs exist
  function startPolling() {
    if (pollInterval) return; // Already polling
    console.log("[JobsTab] Starting status polling");
    isPolling = true;
    pollInterval = setInterval(async () => {
      await jobsStore.refreshJobs();
      // Stop polling when no more active jobs
      if (!hasActiveJobs()) {
        stopPolling();
        console.log("[JobsTab] Stopped polling - no active jobs");
      }
    }, 2000);
  }

  // Stop polling
  function stopPolling() {
    if (pollInterval) {
      clearInterval(pollInterval);
      pollInterval = null;
      isPolling = false;
    }
  }

  // Check if we need to start polling after refresh
  async function refreshAndCheckPolling() {
    await jobsStore.refreshJobs();
    if (hasActiveJobs() && !pollInterval) {
      startPolling();
    }
  }

  onMount(async () => {
    await refreshAndCheckPolling();
  });

  onDestroy(() => {
    stopPolling();
  });

  // Status filter
  let statusFilter = $state<"all" | "QUEUED" | "RUNNING" | "COMPLETED" | "FAILED">("all");

  // Filtered jobs based on status
  function getFilteredJobs(): JobOutput[] {
    if (statusFilter === "all") return jobsStore.jobs;
    return jobsStore.jobs.filter(j => j.status === statusFilter);
  }

  // Format completed time
  function formatTime(isoString: string | null): string {
    if (!isoString) return "--";
    try {
      const date = new Date(isoString);
      return date.toLocaleString();
    } catch {
      return isoString;
    }
  }

  // Get status color
  function getStatusColor(status: string): string {
    switch (status) {
      case "COMPLETED": return "var(--color-success)";
      case "FAILED": return "var(--color-error)";
      case "RUNNING": return "#ffaa00";
      case "QUEUED": return "var(--color-accent-cyan)";
      default: return "var(--color-text-muted)";
    }
  }
</script>

<div class="jobs-tab">
  <!-- Header -->
  <div class="header">
    <div class="header-left">
      <h2 class="title">JOBS</h2>
      <span class="subtitle">Processing Queue</span>
      {#if isPolling}
        <span class="polling-indicator">Auto-refreshing</span>
      {/if}
    </div>
    <div class="header-actions">
      <button class="action-btn" onclick={() => refreshAndCheckPolling()}>
        Refresh
      </button>
    </div>
  </div>

  <!-- Status Filter Bar -->
  <div class="filter-bar">
    <button
      class="filter-btn"
      class:active={statusFilter === "all"}
      onclick={() => statusFilter = "all"}
    >
      ALL
      <span class="filter-count">{jobsStore.jobs.length}</span>
    </button>
    <button
      class="filter-btn"
      class:active={statusFilter === "QUEUED"}
      onclick={() => statusFilter = "QUEUED"}
    >
      QUEUED
      <span class="filter-count queued">{jobsStore.jobs.filter(j => j.status === "QUEUED").length}</span>
    </button>
    <button
      class="filter-btn"
      class:active={statusFilter === "COMPLETED"}
      onclick={() => statusFilter = "COMPLETED"}
    >
      COMPLETED
      <span class="filter-count success">{jobsStore.jobs.filter(j => j.status === "COMPLETED").length}</span>
    </button>
    <button
      class="filter-btn"
      class:active={statusFilter === "FAILED"}
      onclick={() => statusFilter = "FAILED"}
    >
      FAILED
      <span class="filter-count error">{jobsStore.jobs.filter(j => j.status === "FAILED").length}</span>
    </button>
    <button
      class="filter-btn"
      class:active={statusFilter === "RUNNING"}
      onclick={() => statusFilter = "RUNNING"}
    >
      RUNNING
      <span class="filter-count warning">{jobsStore.jobs.filter(j => j.status === "RUNNING").length}</span>
    </button>
  </div>

  <!-- Main Content -->
  <div class="main-content" class:has-selection={jobsStore.selectedJob}>
    <!-- Left: Job List -->
    <div class="job-list">
      {#if jobsStore.loadingJobs}
        <div class="loading-state">Loading jobs...</div>
      {:else if getFilteredJobs().length === 0}
        <div class="empty-state">
          <span class="empty-icon">&#128196;</span>
          <span class="empty-text">No jobs found</span>
        </div>
      {:else}
        {#each getFilteredJobs() as job (job.jobId)}
          <button
            class="job-item"
            class:selected={jobsStore.selectedJob?.jobId === job.jobId}
            onclick={() => jobsStore.selectJob(job)}
          >
            <div class="job-header">
              <span class="job-id">#{job.jobId}</span>
              <span class="job-status" style="color: {getStatusColor(job.status)}">
                {job.status}
              </span>
            </div>
            <div class="job-plugin">{job.pluginName}</div>
            <div class="job-time">{formatTime(job.completedAt)}</div>
          </button>
        {/each}
      {/if}
    </div>

    <!-- Right: Log Viewer -->
    {#if jobsStore.jobDetails}
      <div class="detail-pane">
        <LogViewer details={jobsStore.jobDetails} />
      </div>
    {:else if jobsStore.selectedJob && jobsStore.loadingDetails}
      <div class="detail-pane loading">
        <div class="loading-state">Loading job details...</div>
      </div>
    {:else}
      <div class="detail-pane empty">
        <div class="empty-state">
          <span class="empty-icon">&#128269;</span>
          <span class="empty-text">Select a job to view details</span>
        </div>
      </div>
    {/if}
  </div>
</div>

<style>
  .jobs-tab {
    display: flex;
    flex-direction: column;
    height: 100%;
    gap: var(--space-md);
  }

  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-shrink: 0;
  }

  .header-left {
    display: flex;
    align-items: baseline;
    gap: var(--space-sm);
  }

  .title {
    font-family: var(--font-mono);
    font-size: 18px;
    font-weight: 700;
    color: var(--color-text-primary);
    margin: 0;
    letter-spacing: 1px;
  }

  .subtitle {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-muted);
  }

  .polling-indicator {
    font-family: var(--font-mono);
    font-size: 10px;
    color: #ffaa00;
    padding: 2px 8px;
    background: rgba(255, 170, 0, 0.15);
    border-radius: 4px;
    animation: pulse 1.5s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.5; }
  }

  .header-actions {
    display: flex;
    gap: var(--space-sm);
  }

  .action-btn {
    padding: 8px 16px;
    background: var(--color-bg-tertiary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-secondary);
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .action-btn:hover {
    border-color: var(--color-accent-cyan);
    color: var(--color-accent-cyan);
  }

  .filter-bar {
    display: flex;
    gap: var(--space-xs);
    flex-shrink: 0;
  }

  .filter-btn {
    display: flex;
    align-items: center;
    gap: var(--space-xs);
    padding: 6px 12px;
    background: var(--color-bg-tertiary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .filter-btn:hover {
    border-color: var(--color-accent-cyan);
    color: var(--color-text-secondary);
  }

  .filter-btn.active {
    background: var(--color-accent-cyan);
    border-color: var(--color-accent-cyan);
    color: var(--color-bg-primary);
  }

  .filter-count {
    padding: 2px 6px;
    background: rgba(255, 255, 255, 0.1);
    border-radius: 8px;
    font-size: 9px;
  }

  .filter-btn.active .filter-count {
    background: rgba(0, 0, 0, 0.2);
  }

  .filter-count.success {
    color: var(--color-success);
  }

  .filter-count.error {
    color: var(--color-error);
  }

  .filter-count.warning {
    color: #ffaa00;
  }

  .filter-count.queued {
    color: var(--color-accent-cyan);
  }

  .filter-btn.active .filter-count.success,
  .filter-btn.active .filter-count.error,
  .filter-btn.active .filter-count.warning,
  .filter-btn.active .filter-count.queued {
    color: inherit;
  }

  .main-content {
    flex: 1;
    display: grid;
    grid-template-columns: 300px 1fr;
    gap: var(--space-md);
    min-height: 0;
  }

  .job-list {
    display: flex;
    flex-direction: column;
    gap: var(--space-xs);
    overflow-y: auto;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    padding: var(--space-sm);
  }

  .job-item {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: var(--space-sm) var(--space-md);
    background: var(--color-bg-tertiary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    cursor: pointer;
    text-align: left;
    transition: all 0.15s ease;
  }

  .job-item:hover {
    border-color: var(--color-accent-cyan);
  }

  .job-item.selected {
    border-color: var(--color-accent-cyan);
    background: rgba(0, 212, 255, 0.1);
  }

  .job-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .job-id {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-secondary);
  }

  .job-status {
    font-family: var(--font-mono);
    font-size: 9px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .job-plugin {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-primary);
    font-weight: 500;
  }

  .job-time {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
  }

  .detail-pane {
    min-height: 0;
    overflow: hidden;
  }

  .detail-pane.empty,
  .detail-pane.loading {
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
  }

  .loading-state {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-muted);
    padding: var(--space-lg);
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-sm);
    padding: var(--space-lg);
  }

  .empty-icon {
    font-size: 48px;
    opacity: 0.3;
  }

  .empty-text {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-muted);
  }
</style>
