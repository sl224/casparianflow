<script lang="ts">
  import { systemStore } from "$lib/stores/system.svelte";
  import { jobsStore } from "$lib/stores/jobs.svelte";
  import RoutingTable from "$lib/components/RoutingTable.svelte";
  import DataGrid from "$lib/components/DataGrid.svelte";
  import LogViewer from "$lib/components/LogViewer.svelte";

  // Current view tab
  let activeTab = $state<"dashboard" | "config" | "data">("dashboard");
</script>

<div class="app">
  <!-- Header -->
  <header class="header">
    <div class="logo">
      <span class="logo-icon">&#9671;</span>
      <span class="logo-text">CASPARIAN DECK</span>
    </div>

    <nav class="tabs">
      <button
        class="tab"
        class:active={activeTab === "dashboard"}
        onclick={() => (activeTab = "dashboard")}
      >
        DASHBOARD
      </button>
      <button
        class="tab"
        class:active={activeTab === "config"}
        onclick={() => (activeTab = "config")}
      >
        CONFIG
      </button>
      <button
        class="tab"
        class:active={activeTab === "data"}
        onclick={() => (activeTab = "data")}
      >
        DATA
      </button>
    </nav>

    <div class="status-bar">
      <div class="status-item" class:connected={systemStore.isConnected}>
        <span class="status-dot"></span>
        <span class="status-label">{systemStore.isConnected ? "ONLINE" : "OFFLINE"}</span>
      </div>
    </div>
  </header>

  <!-- Main Content -->
  <main class="main">
    {#if activeTab === "dashboard"}
      <!-- Dashboard View -->
      <div class="dashboard">
        <!-- Left Panel: Workers -->
        <section class="panel workers-panel">
          <h2 class="panel-title">WORKERS</h2>
          <div class="metric-large">
            <span class="metric-value" class:glow-cyan={systemStore.pulse.connectedWorkers > 0}>
              {systemStore.pulse.connectedWorkers}
            </span>
            <span class="metric-label">CONNECTED</span>
          </div>
          <div class="metric-row">
            <div class="metric-small">
              <span class="metric-value">{systemStore.pulse.messagesSent.toLocaleString()}</span>
              <span class="metric-label">MSG SENT</span>
            </div>
            <div class="metric-small">
              <span class="metric-value">{systemStore.pulse.messagesReceived.toLocaleString()}</span>
              <span class="metric-label">MSG RECV</span>
            </div>
          </div>
        </section>

        <!-- Center Panel: Jobs -->
        <section class="panel jobs-panel">
          <h2 class="panel-title">JOB QUEUE</h2>

          <div class="jobs-grid">
            <div class="job-metric in-flight">
              <div class="job-value">{systemStore.pulse.jobsInFlight}</div>
              <div class="job-label">IN FLIGHT</div>
              <div class="job-bar">
                <div
                  class="job-bar-fill"
                  style="width: {Math.min(systemStore.pulse.jobsInFlight * 10, 100)}%"
                ></div>
              </div>
            </div>

            <div class="job-metric completed">
              <div class="job-value">{systemStore.pulse.jobsCompleted.toLocaleString()}</div>
              <div class="job-label">COMPLETED</div>
            </div>

            <div class="job-metric failed">
              <div class="job-value">{systemStore.pulse.jobsFailed.toLocaleString()}</div>
              <div class="job-label">FAILED</div>
            </div>

            <div class="job-metric total">
              <div class="job-value">{systemStore.pulse.jobsDispatched.toLocaleString()}</div>
              <div class="job-label">DISPATCHED</div>
            </div>
          </div>

          <!-- Throughput indicator -->
          <div class="throughput">
            <span class="throughput-value">{systemStore.throughput}</span>
            <span class="throughput-label">jobs/sec</span>
          </div>
        </section>

        <!-- Right Panel: Performance -->
        <section class="panel perf-panel">
          <h2 class="panel-title">PERFORMANCE</h2>

          <div class="perf-metrics">
            <div class="perf-metric">
              <div class="perf-label">DISPATCH LATENCY</div>
              <div class="perf-value">
                {systemStore.pulse.avgDispatchMs < 1 ? "<1ms" : `${systemStore.pulse.avgDispatchMs.toFixed(1)}ms`}
              </div>
            </div>

            <div class="perf-metric">
              <div class="perf-label">CONCLUDE LATENCY</div>
              <div class="perf-value">
                {systemStore.pulse.avgConcludeMs < 1 ? "<1ms" : `${systemStore.pulse.avgConcludeMs.toFixed(1)}ms`}
              </div>
            </div>

            <div class="perf-metric">
              <div class="perf-label">SUCCESS RATE</div>
              <div class="perf-value success-rate">
                {#if systemStore.pulse.jobsCompleted + systemStore.pulse.jobsFailed === 0}
                  100%
                {:else}
                  {((systemStore.pulse.jobsCompleted / (systemStore.pulse.jobsCompleted + systemStore.pulse.jobsFailed)) * 100).toFixed(1)}%
                {/if}
              </div>
            </div>
          </div>
        </section>
      </div>
    {:else if activeTab === "config"}
      <!-- Config View - Routing Rules -->
      <div class="config-view">
        <RoutingTable />
      </div>
    {:else if activeTab === "data"}
      <!-- Data View -->
      <div class="data-view">
        <div class="data-sidebar">
          <h3 class="sidebar-title">COMPLETED JOBS</h3>
          <div class="job-list">
            {#if jobsStore.loadingJobs}
              <div class="loading">Loading...</div>
            {:else if jobsStore.jobs.length === 0}
              <div class="empty">No completed jobs</div>
            {:else}
              {#each jobsStore.jobs as job}
                <button
                  class="job-item"
                  class:selected={jobsStore.selectedJob?.jobId === job.jobId}
                  class:queryable={job.outputPath !== null}
                  class:failed={job.status === "FAILED"}
                  onclick={() => jobsStore.selectJob(job)}
                >
                  <span class="job-name">{job.pluginName}</span>
                  <span class="job-id">#{job.jobId}</span>
                  {#if job.status === "FAILED"}
                    <span class="job-failed">&#10007;</span>
                  {:else if job.outputPath}
                    <span class="job-output">&#9654;</span>
                  {/if}
                </button>
              {/each}
            {/if}
          </div>
          <button class="refresh-btn" onclick={() => jobsStore.refreshJobs()}>
            &#8635; Refresh
          </button>
        </div>

        <div class="data-content">
          {#if jobsStore.loadingQuery}
            <div class="loading-overlay">
              <div class="loading-spinner"></div>
              <span>Querying...</span>
            </div>
          {:else if jobsStore.queryError}
            <div class="error-overlay">
              <span class="error-icon">!</span>
              <span class="error-message">{jobsStore.queryError}</span>
            </div>
          {:else if jobsStore.queryResult}
            <DataGrid result={jobsStore.queryResult} />
          {:else if jobsStore.loadingDetails}
            <div class="loading-overlay">
              <div class="loading-spinner"></div>
              <span>Loading details...</span>
            </div>
          {:else if jobsStore.detailsError}
            <div class="error-overlay">
              <span class="error-icon">!</span>
              <span class="error-message">{jobsStore.detailsError}</span>
            </div>
          {:else if jobsStore.jobDetails}
            <LogViewer details={jobsStore.jobDetails} />
          {:else}
            <div class="empty-state">
              <span class="empty-icon">&#128202;</span>
              <span class="empty-title">Select a Job</span>
              <span class="empty-message">Click on a job to view details, errors, or query output data</span>
            </div>
          {/if}
        </div>
      </div>
    {/if}
  </main>

  <!-- Footer status bar -->
  <footer class="footer">
    <div class="footer-item">
      <span class="footer-label">SENTINEL</span>
      <span class="footer-value" class:active={systemStore.isConnected}>{systemStore.bindAddress}</span>
    </div>
    <div class="footer-item">
      <span class="footer-label">LAST PULSE</span>
      <span class="footer-value mono">
        {systemStore.pulse.timestamp ? new Date(systemStore.pulse.timestamp * 1000).toLocaleTimeString() : "--:--:--"}
      </span>
    </div>
  </footer>
</div>

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: calc(100vh - 32px); /* Account for custom title bar */
    background: var(--color-bg-primary);
    overflow: hidden;
  }

  /* Header */
  .header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--space-md) var(--space-lg);
    background: var(--color-bg-secondary);
    border-bottom: 1px solid var(--color-border);
  }

  .logo {
    display: flex;
    align-items: center;
    gap: var(--space-sm);
  }

  .logo-icon {
    font-size: 24px;
    color: var(--color-accent-cyan);
  }

  .logo-text {
    font-family: var(--font-mono);
    font-size: 16px;
    font-weight: 600;
    letter-spacing: 2px;
    color: var(--color-text-primary);
  }

  .tabs {
    display: flex;
    gap: 4px;
    background: var(--color-bg-tertiary);
    padding: 4px;
    border-radius: 6px;
  }

  .tab {
    padding: 8px 20px;
    background: transparent;
    border: none;
    border-radius: 4px;
    font-family: var(--font-mono);
    font-size: 12px;
    font-weight: 500;
    letter-spacing: 1px;
    color: var(--color-text-secondary);
    cursor: pointer;
    transition: all 0.2s ease;
  }

  .tab:hover {
    color: var(--color-text-primary);
  }

  .tab.active {
    background: var(--color-bg-card);
    color: var(--color-accent-cyan);
  }

  .status-bar {
    display: flex;
    gap: var(--space-md);
  }

  .status-item {
    display: flex;
    align-items: center;
    gap: var(--space-sm);
  }

  .status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--color-error);
    transition: background var(--transition-fast);
  }

  .status-item.connected .status-dot {
    background: var(--color-success);
    box-shadow: 0 0 8px var(--color-success);
  }

  .status-label {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-secondary);
  }

  /* Main Content */
  .main {
    flex: 1;
    overflow: hidden;
  }

  /* Dashboard View */
  .dashboard {
    display: grid;
    grid-template-columns: 1fr 2fr 1fr;
    gap: var(--space-lg);
    padding: var(--space-lg);
    height: 100%;
    overflow: auto;
  }

  .panel {
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    padding: var(--space-lg);
  }

  .panel-title {
    font-family: var(--font-mono);
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 1px;
    color: var(--color-text-muted);
    margin-bottom: var(--space-lg);
  }

  /* Workers Panel */
  .metric-large {
    text-align: center;
    margin-bottom: var(--space-lg);
  }

  .metric-large .metric-value {
    font-family: var(--font-mono);
    font-size: 64px;
    font-weight: 700;
    color: var(--color-text-primary);
    display: block;
    line-height: 1;
  }

  .metric-large .metric-value.glow-cyan {
    color: var(--color-accent-cyan);
    text-shadow: 0 0 20px rgba(0, 212, 255, 0.5);
  }

  .metric-large .metric-label {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-muted);
    letter-spacing: 1px;
    margin-top: var(--space-sm);
    display: block;
  }

  .metric-row {
    display: flex;
    justify-content: space-around;
    gap: var(--space-md);
  }

  .metric-small {
    text-align: center;
  }

  .metric-small .metric-value {
    font-family: var(--font-mono);
    font-size: 20px;
    font-weight: 600;
    color: var(--color-text-primary);
    display: block;
  }

  .metric-small .metric-label {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
    letter-spacing: 1px;
  }

  /* Jobs Panel */
  .jobs-grid {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: var(--space-md);
    margin-bottom: var(--space-lg);
  }

  .job-metric {
    background: var(--color-bg-tertiary);
    border-radius: var(--radius-sm);
    padding: var(--space-md);
  }

  .job-metric.in-flight {
    grid-column: span 2;
    border: 1px solid var(--color-accent-cyan);
  }

  .job-value {
    font-family: var(--font-mono);
    font-size: 28px;
    font-weight: 700;
    color: var(--color-text-primary);
  }

  .job-metric.in-flight .job-value {
    color: var(--color-accent-cyan);
  }

  .job-metric.completed .job-value {
    color: var(--color-success);
  }

  .job-metric.failed .job-value {
    color: var(--color-error);
  }

  .job-label {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
    letter-spacing: 1px;
    margin-top: var(--space-xs);
  }

  .job-bar {
    height: 4px;
    background: var(--color-bg-primary);
    border-radius: 2px;
    margin-top: var(--space-sm);
    overflow: hidden;
  }

  .job-bar-fill {
    height: 100%;
    background: var(--color-accent-cyan);
    border-radius: 2px;
    transition: width var(--transition-medium);
  }

  .throughput {
    text-align: center;
    padding: var(--space-md);
    background: var(--color-bg-tertiary);
    border-radius: var(--radius-sm);
    border: 1px dashed var(--color-border);
  }

  .throughput-value {
    font-family: var(--font-mono);
    font-size: 32px;
    font-weight: 700;
    color: var(--color-accent-green);
  }

  .throughput-label {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-muted);
    margin-left: var(--space-sm);
  }

  /* Performance Panel */
  .perf-metrics {
    display: flex;
    flex-direction: column;
    gap: var(--space-lg);
  }

  .perf-metric {
    background: var(--color-bg-tertiary);
    border-radius: var(--radius-sm);
    padding: var(--space-md);
  }

  .perf-label {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
    letter-spacing: 1px;
    margin-bottom: var(--space-sm);
  }

  .perf-value {
    font-family: var(--font-mono);
    font-size: 20px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .perf-value.success-rate {
    color: var(--color-success);
  }

  /* Config View */
  .config-view {
    height: 100%;
    padding: var(--space-lg);
  }

  /* Data View */
  .data-view {
    display: flex;
    height: 100%;
  }

  .data-sidebar {
    width: 280px;
    background: var(--color-bg-secondary);
    border-right: 1px solid var(--color-border);
    display: flex;
    flex-direction: column;
  }

  .sidebar-title {
    font-family: var(--font-mono);
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 1px;
    color: var(--color-text-muted);
    padding: var(--space-md) var(--space-lg);
    border-bottom: 1px solid var(--color-border);
  }

  .job-list {
    flex: 1;
    overflow: auto;
    padding: var(--space-sm);
  }

  .job-item {
    display: flex;
    align-items: center;
    gap: var(--space-sm);
    width: 100%;
    padding: var(--space-sm) var(--space-md);
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-secondary);
    cursor: pointer;
    text-align: left;
    transition: all 0.15s ease;
  }

  .job-item:hover:not(:disabled) {
    background: var(--color-bg-tertiary);
    border-color: var(--color-border);
  }

  .job-item.selected {
    background: var(--color-bg-tertiary);
    border-color: var(--color-accent-cyan);
    color: var(--color-text-primary);
  }

  .job-item:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .job-item.queryable:not(:disabled) {
    color: var(--color-text-primary);
  }

  .job-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .job-id {
    color: var(--color-text-muted);
    font-size: 10px;
  }

  .job-output {
    color: var(--color-accent-green);
    font-size: 10px;
  }

  .job-failed {
    color: var(--color-error);
    font-size: 10px;
    font-weight: bold;
  }

  .job-item.failed {
    border-left: 2px solid var(--color-error);
  }

  .job-item.failed .job-name {
    color: var(--color-error);
  }

  .refresh-btn {
    margin: var(--space-sm);
    padding: var(--space-sm) var(--space-md);
    background: var(--color-bg-tertiary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-secondary);
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .refresh-btn:hover {
    border-color: var(--color-accent-cyan);
    color: var(--color-accent-cyan);
  }

  .data-content {
    flex: 1;
    position: relative;
    padding: var(--space-lg);
    overflow: hidden;
  }

  .loading-overlay,
  .error-overlay,
  .empty-state {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 16px;
    color: var(--color-text-secondary);
    font-family: var(--font-mono);
  }

  .loading-spinner {
    width: 40px;
    height: 40px;
    border: 3px solid var(--color-border);
    border-top-color: var(--color-accent-cyan);
    border-radius: 50%;
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .error-icon {
    width: 48px;
    height: 48px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--color-error);
    color: var(--color-bg-primary);
    border-radius: 50%;
    font-size: 24px;
    font-weight: bold;
  }

  .error-message {
    color: var(--color-error);
    max-width: 400px;
    text-align: center;
  }

  .empty-icon {
    font-size: 48px;
    color: var(--color-text-muted);
  }

  .empty-title {
    font-size: 18px;
    color: var(--color-text-primary);
  }

  .empty-message {
    font-size: 14px;
    color: var(--color-text-muted);
  }

  .loading {
    padding: var(--space-lg);
    text-align: center;
    color: var(--color-text-muted);
    font-family: var(--font-mono);
    font-size: 12px;
  }

  .empty {
    padding: var(--space-lg);
    text-align: center;
    color: var(--color-text-muted);
    font-family: var(--font-mono);
    font-size: 12px;
  }

  /* Footer */
  .footer {
    display: flex;
    justify-content: space-between;
    padding: var(--space-sm) var(--space-lg);
    background: var(--color-bg-secondary);
    border-top: 1px solid var(--color-border);
  }

  .footer-item {
    display: flex;
    align-items: center;
    gap: var(--space-sm);
  }

  .footer-label {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
    letter-spacing: 1px;
  }

  .footer-value {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-secondary);
  }

  .footer-value.active {
    color: var(--color-success);
  }

  .mono {
    font-family: var(--font-mono);
  }

  /* Responsive */
  @media (max-width: 1024px) {
    .dashboard {
      grid-template-columns: 1fr;
    }

    .jobs-panel {
      order: -1;
    }

    .data-view {
      flex-direction: column;
    }

    .data-sidebar {
      width: 100%;
      max-height: 200px;
    }
  }
</style>
