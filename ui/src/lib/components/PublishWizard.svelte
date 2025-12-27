<script lang="ts">
  import { invoke } from "$lib/tauri";
  import { open } from "@tauri-apps/plugin-dialog";

  // Plugin analysis result from backend
  interface PluginManifest {
    pluginName: string;
    sourceHash: string;
    isValid: boolean;
    validationErrors: string[];
    hasLockfile: boolean;
    envHash: string | null;
    handlerMethods: string[];
    detectedTopics: string[];
  }

  // Publish result from backend
  interface PublishReceipt {
    success: boolean;
    pluginName: string;
    version: string;
    sourceHash: string;
    envHash: string | null;
    routingRuleId: number | null;
    topicConfigId: number | null;
    message: string;
  }

  // Wizard state
  type WizardStep = "select" | "analyze" | "configure" | "publish" | "complete";
  let step = $state<WizardStep>("select");

  // Form state
  let pluginPath = $state("");
  let manifest = $state<PluginManifest | null>(null);
  let analyzing = $state(false);
  let analyzeError = $state<string | null>(null);

  // Configuration overrides
  let routingPattern = $state("");
  let routingTag = $state("");
  let routingPriority = $state(50);
  let topicUri = $state("");

  // Publishing state
  let publishing = $state(false);
  let publishError = $state<string | null>(null);
  let receipt = $state<PublishReceipt | null>(null);

  // Browse for a plugin file using native OS dialog
  async function browseForPlugin() {
    try {
      const selected = await open({
        multiple: false,
        filters: [
          { name: "Python Files", extensions: ["py"] },
        ],
        title: "Select Plugin File",
      });

      if (selected) {
        pluginPath = selected as string;
        // Auto-analyze after selection
        await analyzePlugin();
      }
    } catch (e) {
      analyzeError = e instanceof Error ? e.message : String(e);
    }
  }

  // Analyze the plugin when path is entered
  async function analyzePlugin() {
    if (!pluginPath.trim()) {
      analyzeError = "Please enter a plugin path";
      return;
    }

    analyzing = true;
    analyzeError = null;
    manifest = null;

    try {
      // Real I/O: Call Tauri backend to analyze the actual file
      manifest = await invoke<PluginManifest>("analyze_plugin_manifest", {
        path: pluginPath.trim(),
      });

      // Auto-fill routing tag from plugin name
      if (manifest.pluginName) {
        routingTag = manifest.pluginName;
      }

      // Auto-fill topic URI if topics detected
      if (manifest.detectedTopics.length > 0) {
        const topic = manifest.detectedTopics[0];
        topicUri = `parquet://output/${manifest.pluginName}/${topic}.parquet`;
      }

      step = "analyze";
    } catch (e) {
      analyzeError = e instanceof Error ? e.message : String(e);
    } finally {
      analyzing = false;
    }
  }

  // Move to configuration step
  function proceedToConfigure() {
    if (!manifest?.isValid) return;
    step = "configure";
  }

  // Publish the plugin with overrides
  async function publishPlugin() {
    if (!manifest) return;

    publishing = true;
    publishError = null;

    try {
      // Real I/O: Call Tauri backend to publish to SQLite
      receipt = await invoke<PublishReceipt>("publish_with_overrides", {
        args: {
          path: pluginPath.trim(),
          routingPattern: routingPattern.trim() || null,
          routingTag: routingTag.trim() || null,
          routingPriority: routingPriority,
          topicUriOverride: topicUri.trim() || null,
        },
      });

      step = "complete";
    } catch (e) {
      publishError = e instanceof Error ? e.message : String(e);
    } finally {
      publishing = false;
    }
  }

  // Reset wizard to start over
  function reset() {
    step = "select";
    pluginPath = "";
    manifest = null;
    analyzing = false;
    analyzeError = null;
    routingPattern = "";
    routingTag = "";
    routingPriority = 50;
    topicUri = "";
    publishing = false;
    publishError = null;
    receipt = null;
  }
</script>

<div class="publish-wizard">
  <!-- Progress indicator -->
  <div class="progress">
    <div class="progress-step" class:active={step === "select"} class:done={step !== "select"}>
      <span class="step-number">1</span>
      <span class="step-label">SELECT</span>
    </div>
    <div class="progress-line" class:done={step !== "select"}></div>
    <div class="progress-step" class:active={step === "analyze"} class:done={step === "configure" || step === "publish" || step === "complete"}>
      <span class="step-number">2</span>
      <span class="step-label">ANALYZE</span>
    </div>
    <div class="progress-line" class:done={step === "configure" || step === "publish" || step === "complete"}></div>
    <div class="progress-step" class:active={step === "configure" || step === "publish"} class:done={step === "complete"}>
      <span class="step-number">3</span>
      <span class="step-label">CONFIGURE</span>
    </div>
    <div class="progress-line" class:done={step === "complete"}></div>
    <div class="progress-step" class:active={step === "complete"}>
      <span class="step-number">4</span>
      <span class="step-label">PUBLISHED</span>
    </div>
  </div>

  <!-- Step Content -->
  <div class="content">
    {#if step === "select"}
      <!-- Step 1: Select Plugin -->
      <div class="step-content">
        <h2 class="step-title">Select Plugin File</h2>
        <p class="step-description">Enter the path to your Python plugin file (.py)</p>

        <div class="form-group">
          <label for="plugin-path">Plugin Path</label>
          <div class="path-input-row">
            <input
              id="plugin-path"
              type="text"
              class="text-input"
              placeholder="/path/to/my_plugin.py"
              bind:value={pluginPath}
              onkeydown={(e) => e.key === "Enter" && analyzePlugin()}
            />
            <button
              type="button"
              class="btn browse"
              onclick={browseForPlugin}
              disabled={analyzing}
            >
              Browse...
            </button>
          </div>
        </div>

        {#if analyzeError}
          <div class="error-message">{analyzeError}</div>
        {/if}

        <div class="actions">
          <button
            class="btn primary"
            onclick={analyzePlugin}
            disabled={analyzing || !pluginPath.trim()}
          >
            {analyzing ? "Analyzing..." : "Analyze Plugin"}
          </button>
        </div>
      </div>
    {:else if step === "analyze"}
      <!-- Step 2: Analysis Results -->
      <div class="step-content">
        <h2 class="step-title">Analysis Results</h2>
        <p class="step-description">Plugin validation and detection results</p>

        {#if manifest}
          <div class="analysis-card">
            <div class="analysis-header">
              <span class="plugin-name">{manifest.pluginName}</span>
              <span class="validation-badge" class:valid={manifest.isValid} class:invalid={!manifest.isValid}>
                {manifest.isValid ? "VALID" : "INVALID"}
              </span>
            </div>

            <div class="analysis-details">
              <div class="detail-row">
                <span class="detail-label">Source Hash</span>
                <span class="detail-value hash">{manifest.sourceHash.substring(0, 16)}...</span>
              </div>

              <div class="detail-row">
                <span class="detail-label">Lockfile</span>
                <span class="detail-value" class:has={manifest.hasLockfile} class:missing={!manifest.hasLockfile}>
                  {manifest.hasLockfile ? "Found" : "Missing (will be generated)"}
                </span>
              </div>

              <div class="detail-row">
                <span class="detail-label">Handler Methods</span>
                <span class="detail-value methods">
                  {manifest.handlerMethods.join(", ") || "None detected"}
                </span>
              </div>

              <div class="detail-row">
                <span class="detail-label">Detected Topics</span>
                <span class="detail-value topics">
                  {manifest.detectedTopics.join(", ") || "None"}
                </span>
              </div>
            </div>

            {#if manifest.validationErrors.length > 0}
              <div class="validation-errors">
                <h4>Validation Errors</h4>
                <ul>
                  {#each manifest.validationErrors as error}
                    <li>{error}</li>
                  {/each}
                </ul>
              </div>
            {/if}
          </div>
        {/if}

        <div class="actions">
          <button class="btn secondary" onclick={reset}>
            Back
          </button>
          <button
            class="btn primary"
            onclick={proceedToConfigure}
            disabled={!manifest?.isValid}
          >
            Configure Routing
          </button>
        </div>
      </div>
    {:else if step === "configure" || step === "publish"}
      <!-- Step 3: Configure Routing & Topic -->
      <div class="step-content">
        <h2 class="step-title">Configure Routing</h2>
        <p class="step-description">Optional: Set up routing rules and topic configuration</p>

        <div class="form-section">
          <h3>Routing Rule (Optional)</h3>
          <p class="form-hint">Route matching files to this plugin</p>

          <div class="form-row">
            <div class="form-group">
              <label for="routing-pattern">Pattern</label>
              <input
                id="routing-pattern"
                type="text"
                class="text-input"
                placeholder="data/**/*.csv"
                bind:value={routingPattern}
              />
            </div>

            <div class="form-group">
              <label for="routing-tag">Tag</label>
              <input
                id="routing-tag"
                type="text"
                class="text-input"
                placeholder="my_processor"
                bind:value={routingTag}
              />
            </div>

            <div class="form-group small">
              <label for="routing-priority">Priority</label>
              <input
                id="routing-priority"
                type="number"
                class="text-input"
                bind:value={routingPriority}
              />
            </div>
          </div>
        </div>

        <div class="form-section">
          <h3>Topic URI (Optional)</h3>
          <p class="form-hint">Override output destination for detected topics</p>

          <div class="form-group">
            <label for="topic-uri">Output URI</label>
            <input
              id="topic-uri"
              type="text"
              class="text-input"
              placeholder="parquet://output/data.parquet"
              bind:value={topicUri}
            />
          </div>
        </div>

        {#if publishError}
          <div class="error-message">{publishError}</div>
        {/if}

        <div class="actions">
          <button class="btn secondary" onclick={() => (step = "analyze")}>
            Back
          </button>
          <button
            class="btn primary"
            onclick={publishPlugin}
            disabled={publishing}
          >
            {publishing ? "Publishing..." : "Publish Plugin"}
          </button>
        </div>
      </div>
    {:else if step === "complete"}
      <!-- Step 4: Published -->
      <div class="step-content">
        <h2 class="step-title">
          {receipt?.success ? "Published Successfully!" : "Publication Failed"}
        </h2>

        {#if receipt}
          <div class="receipt-card" class:success={receipt.success} class:failed={!receipt.success}>
            <div class="receipt-header">
              <span class="receipt-icon">{receipt.success ? "&#10003;" : "&#10007;"}</span>
              <span class="receipt-plugin">{receipt.pluginName}</span>
              <span class="receipt-version">v{receipt.version}</span>
            </div>

            <div class="receipt-message">{receipt.message}</div>

            {#if receipt.success}
              <div class="receipt-details">
                <div class="receipt-row">
                  <span class="receipt-label">Source Hash</span>
                  <span class="receipt-value">{receipt.sourceHash.substring(0, 16)}...</span>
                </div>

                {#if receipt.envHash}
                  <div class="receipt-row">
                    <span class="receipt-label">Env Hash</span>
                    <span class="receipt-value">{receipt.envHash.substring(0, 16)}...</span>
                  </div>
                {/if}

                {#if receipt.routingRuleId}
                  <div class="receipt-row">
                    <span class="receipt-label">Routing Rule ID</span>
                    <span class="receipt-value">#{receipt.routingRuleId}</span>
                  </div>
                {/if}

                {#if receipt.topicConfigId}
                  <div class="receipt-row">
                    <span class="receipt-label">Topic Config ID</span>
                    <span class="receipt-value">#{receipt.topicConfigId}</span>
                  </div>
                {/if}
              </div>
            {/if}
          </div>
        {/if}

        <div class="actions">
          <button class="btn primary" onclick={reset}>
            Publish Another
          </button>
        </div>
      </div>
    {/if}
  </div>
</div>

<style>
  .publish-wizard {
    display: flex;
    flex-direction: column;
    height: 100%;
    max-width: 800px;
    margin: 0 auto;
  }

  /* Progress Indicator */
  .progress {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--space-lg);
    gap: 8px;
  }

  .progress-step {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    opacity: 0.4;
    transition: opacity 0.2s ease;
  }

  .progress-step.active,
  .progress-step.done {
    opacity: 1;
  }

  .step-number {
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--color-bg-tertiary);
    border: 2px solid var(--color-border);
    border-radius: 50%;
    font-family: var(--font-mono);
    font-size: 12px;
    font-weight: 600;
    color: var(--color-text-muted);
    transition: all 0.2s ease;
  }

  .progress-step.active .step-number {
    border-color: var(--color-accent-cyan);
    color: var(--color-accent-cyan);
    background: rgba(0, 212, 255, 0.1);
  }

  .progress-step.done .step-number {
    border-color: var(--color-accent-green);
    background: var(--color-accent-green);
    color: var(--color-bg-primary);
  }

  .step-label {
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 1px;
    color: var(--color-text-muted);
  }

  .progress-step.active .step-label {
    color: var(--color-accent-cyan);
  }

  .progress-line {
    width: 60px;
    height: 2px;
    background: var(--color-border);
    transition: background 0.2s ease;
  }

  .progress-line.done {
    background: var(--color-accent-green);
  }

  /* Content */
  .content {
    flex: 1;
    padding: var(--space-lg);
    overflow: auto;
  }

  .step-content {
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    padding: var(--space-xl);
  }

  .step-title {
    font-family: var(--font-mono);
    font-size: 18px;
    font-weight: 600;
    color: var(--color-text-primary);
    margin-bottom: var(--space-sm);
  }

  .step-description {
    font-family: var(--font-mono);
    font-size: 13px;
    color: var(--color-text-muted);
    margin-bottom: var(--space-lg);
  }

  /* Form Elements */
  .form-group {
    margin-bottom: var(--space-md);
  }

  .path-input-row {
    display: flex;
    gap: var(--space-sm);
  }

  .path-input-row .text-input {
    flex: 1;
  }

  .btn.browse {
    background: var(--color-bg-tertiary);
    color: var(--color-text-secondary);
    border: 1px solid var(--color-border);
    white-space: nowrap;
  }

  .btn.browse:hover:not(:disabled) {
    border-color: var(--color-accent-cyan);
    color: var(--color-accent-cyan);
  }

  .form-group.small {
    width: 100px;
  }

  .form-group label {
    display: block;
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.5px;
    color: var(--color-text-muted);
    margin-bottom: var(--space-xs);
    text-transform: uppercase;
  }

  .text-input {
    width: 100%;
    padding: 10px 14px;
    background: var(--color-bg-primary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 13px;
    color: var(--color-text-primary);
    transition: all 0.15s ease;
  }

  .text-input:focus {
    outline: none;
    border-color: var(--color-accent-cyan);
    box-shadow: 0 0 0 3px rgba(0, 212, 255, 0.1);
  }

  .text-input::placeholder {
    color: var(--color-text-muted);
  }

  .form-section {
    margin-bottom: var(--space-lg);
    padding-bottom: var(--space-lg);
    border-bottom: 1px solid var(--color-border);
  }

  .form-section h3 {
    font-family: var(--font-mono);
    font-size: 14px;
    font-weight: 600;
    color: var(--color-text-primary);
    margin-bottom: var(--space-xs);
  }

  .form-hint {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-muted);
    margin-bottom: var(--space-md);
  }

  .form-row {
    display: flex;
    gap: var(--space-md);
  }

  .form-row .form-group {
    flex: 1;
  }

  /* Error Message */
  .error-message {
    padding: var(--space-md);
    background: rgba(255, 51, 85, 0.1);
    border: 1px solid var(--color-error);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-error);
    margin-bottom: var(--space-md);
  }

  /* Actions */
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-sm);
    margin-top: var(--space-lg);
  }

  .btn {
    padding: 10px 20px;
    border: none;
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.5px;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn.primary {
    background: var(--color-accent-cyan);
    color: var(--color-bg-primary);
  }

  .btn.primary:hover:not(:disabled) {
    background: var(--color-accent-green);
  }

  .btn.secondary {
    background: var(--color-bg-tertiary);
    color: var(--color-text-secondary);
    border: 1px solid var(--color-border);
  }

  .btn.secondary:hover:not(:disabled) {
    border-color: var(--color-text-muted);
    color: var(--color-text-primary);
  }

  /* Analysis Card */
  .analysis-card {
    background: var(--color-bg-tertiary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    margin-bottom: var(--space-md);
    overflow: hidden;
  }

  .analysis-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--space-md);
    background: var(--color-bg-secondary);
    border-bottom: 1px solid var(--color-border);
  }

  .plugin-name {
    font-family: var(--font-mono);
    font-size: 16px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .validation-badge {
    padding: 4px 12px;
    border-radius: 12px;
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.5px;
  }

  .validation-badge.valid {
    background: rgba(0, 255, 136, 0.15);
    color: var(--color-accent-green);
  }

  .validation-badge.invalid {
    background: rgba(255, 51, 85, 0.15);
    color: var(--color-error);
  }

  .analysis-details {
    padding: var(--space-md);
  }

  .detail-row {
    display: flex;
    justify-content: space-between;
    padding: var(--space-sm) 0;
    border-bottom: 1px solid var(--color-border);
  }

  .detail-row:last-child {
    border-bottom: none;
  }

  .detail-label {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-muted);
  }

  .detail-value {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-secondary);
  }

  .detail-value.hash {
    color: var(--color-accent-cyan);
  }

  .detail-value.has {
    color: var(--color-accent-green);
  }

  .detail-value.missing {
    color: var(--color-warning);
  }

  .detail-value.methods,
  .detail-value.topics {
    color: var(--color-accent-cyan);
  }

  .validation-errors {
    padding: var(--space-md);
    background: rgba(255, 51, 85, 0.05);
    border-top: 1px solid var(--color-error);
  }

  .validation-errors h4 {
    font-family: var(--font-mono);
    font-size: 12px;
    font-weight: 600;
    color: var(--color-error);
    margin-bottom: var(--space-sm);
  }

  .validation-errors ul {
    margin: 0;
    padding-left: var(--space-lg);
  }

  .validation-errors li {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-error);
    margin-bottom: var(--space-xs);
  }

  /* Receipt Card */
  .receipt-card {
    background: var(--color-bg-tertiary);
    border: 2px solid var(--color-border);
    border-radius: var(--radius-md);
    overflow: hidden;
  }

  .receipt-card.success {
    border-color: var(--color-accent-green);
  }

  .receipt-card.failed {
    border-color: var(--color-error);
  }

  .receipt-header {
    display: flex;
    align-items: center;
    gap: var(--space-md);
    padding: var(--space-lg);
    background: var(--color-bg-secondary);
    border-bottom: 1px solid var(--color-border);
  }

  .receipt-icon {
    width: 32px;
    height: 32px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 50%;
    font-size: 18px;
  }

  .receipt-card.success .receipt-icon {
    background: var(--color-accent-green);
    color: var(--color-bg-primary);
  }

  .receipt-card.failed .receipt-icon {
    background: var(--color-error);
    color: white;
  }

  .receipt-plugin {
    font-family: var(--font-mono);
    font-size: 18px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .receipt-version {
    font-family: var(--font-mono);
    font-size: 14px;
    color: var(--color-text-muted);
  }

  .receipt-message {
    padding: var(--space-md) var(--space-lg);
    font-family: var(--font-mono);
    font-size: 13px;
    color: var(--color-text-secondary);
    border-bottom: 1px solid var(--color-border);
  }

  .receipt-details {
    padding: var(--space-md) var(--space-lg);
  }

  .receipt-row {
    display: flex;
    justify-content: space-between;
    padding: var(--space-sm) 0;
  }

  .receipt-label {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-muted);
  }

  .receipt-value {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-accent-cyan);
  }
</style>
