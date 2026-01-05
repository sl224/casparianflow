<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  // Types
  interface ChatMessage {
    role: "user" | "assistant";
    content: string;
  }

  interface ParserChatResponse {
    message: string;
    refinedCode: string | null;
    suggestedReplies: string[];
    autoValidate: boolean;
  }

  interface ParserDraft {
    shardKey: string;
    sourceCode: string;
    sampleInput: string[];
    sampleOutput: string | null;
    validationError: string | null;
  }

  interface SchemaColumn {
    name: string;
    inferredType: string;
    nullable: boolean;
    description: string | null;
  }

  interface SchemaProposal {
    columns: SchemaColumn[];
    suggestedSink: string;
    suggestedOutputPath: string;
    reasoning: string;
  }

  interface ParserPublishReceipt {
    success: boolean;
    pluginName: string;
    parserFilePath: string;
    manifestId: number | null;
    configId: number | null;
    topicConfigId: number | null;
    message: string;
  }

  // Wizard phases
  type WizardPhase = "refining" | "configuring" | "publishing" | "published";

  // Props
  interface Props {
    shardPath: string;
    shardKey: string;
    initialCode?: string;
    onApprove: (code: string) => void;
    onClose: () => void;
  }

  let { shardPath, shardKey, initialCode, onApprove, onClose }: Props = $props();

  // Phase state
  let phase = $state<WizardPhase>("refining");

  // Refining phase state
  let messages = $state<ChatMessage[]>([]);
  let currentCode = $state("");
  let validationError = $state<string | null>(null);
  let validationOutput = $state<string | null>(null);
  let suggestedReplies = $state<string[]>([]);
  let sampleInput = $state<string[]>([]);
  let isLoading = $state(false);
  let isValidating = $state(false);
  let isGenerating = $state(false);
  let userInput = $state("");
  let error = $state<string | null>(null);
  let validationSuccess = $state(false);

  // Configuring phase state
  let schemaProposal = $state<SchemaProposal | null>(null);
  let editedSchema = $state<SchemaColumn[]>([]);
  let sinkType = $state<string>("parquet");
  let outputPath = $state<string>("");
  let isProposingSchema = $state(false);

  // Publishing phase state
  let isPublishing = $state(false);
  let publishReceipt = $state<ParserPublishReceipt | null>(null);

  // Load sample data and generate initial code on mount
  $effect(() => {
    if (shardPath) {
      initializeParser();
    }
  });

  async function initializeParser() {
    await loadSampleData();
    if (initialCode) {
      currentCode = initialCode;
      await validateParser();
    } else {
      await generateInitialCode();
    }
  }

  async function generateInitialCode() {
    isGenerating = true;
    error = null;
    try {
      const draft = await invoke<ParserDraft>("generate_parser_draft", { shardPath });
      currentCode = draft.sourceCode;
      sampleInput = draft.sampleInput;
      messages = [{ role: "assistant", content: `I've generated a parser for ${shardKey}. Let me validate it...` }];
      await validateParser();
    } catch (e) {
      error = `Failed to generate parser: ${e}`;
    } finally {
      isGenerating = false;
    }
  }

  async function loadSampleData() {
    try {
      const rows = await invoke<string[]>("preview_shard", { path: shardPath, numRows: 10 });
      sampleInput = rows;
    } catch (e) {
      error = `Failed to load sample data: ${e}`;
    }
  }

  async function validateParser() {
    if (!currentCode) return;
    isValidating = true;
    error = null;
    try {
      const result = await invoke<ParserDraft>("validate_parser", { shardPath, sourceCode: currentCode });
      if (result.validationError) {
        validationError = result.validationError;
        validationOutput = null;
        validationSuccess = false;
        await sendErrorToLLM(result.validationError);
      } else {
        validationError = null;
        validationOutput = result.sampleOutput;
        validationSuccess = true;
        messages = [...messages, { role: "assistant", content: "Validation successful! The parser works correctly. Click 'Approve Parser' to configure output." }];
        suggestedReplies = ["Approve parser", "Make additional changes"];
      }
    } catch (e) {
      error = e as string;
    } finally {
      isValidating = false;
    }
  }

  async function sendErrorToLLM(errorMsg: string) {
    isLoading = true;
    try {
      const messagesJson = JSON.stringify(messages);
      const response = await invoke<ParserChatResponse>("parser_refinement_chat", {
        shardPath, parserCode: currentCode, validationError: errorMsg, messagesJson,
        userInput: `Validation failed with error:\n${errorMsg}`
      });
      messages = [...messages, { role: "assistant", content: response.message }];
      if (response.refinedCode) currentCode = response.refinedCode;
      suggestedReplies = response.suggestedReplies;
      if (response.autoValidate && response.refinedCode) await validateParser();
    } catch (e) {
      error = e as string;
    } finally {
      isLoading = false;
    }
  }

  async function sendMessage(input: string) {
    if (isLoading || !input.trim()) return;
    isLoading = true;
    error = null;
    messages = [...messages, { role: "user", content: input }];
    try {
      const messagesJson = JSON.stringify(messages);
      const response = await invoke<ParserChatResponse>("parser_refinement_chat", {
        shardPath, parserCode: currentCode, validationError, messagesJson, userInput: input
      });
      messages = [...messages, { role: "assistant", content: response.message }];
      if (response.refinedCode) {
        currentCode = response.refinedCode;
        validationError = null;
        validationOutput = null;
        validationSuccess = false;
      }
      suggestedReplies = response.suggestedReplies;
      userInput = "";
      if (response.autoValidate && response.refinedCode) await validateParser();
    } catch (e) {
      error = e as string;
    } finally {
      isLoading = false;
    }
  }

  function handleSuggestedReply(reply: string) {
    if (reply === "Approve parser") handleApprove();
    else if (reply === "Validate again") validateParser();
    else sendMessage(reply);
  }

  function handleSubmit(e: Event) {
    e.preventDefault();
    if (userInput.trim()) sendMessage(userInput.trim());
  }

  // Transition to configuring phase
  async function handleApprove() {
    if (!validationSuccess || !validationOutput) return;

    phase = "configuring";
    isProposingSchema = true;
    error = null;

    try {
      const proposal = await invoke<SchemaProposal>("propose_schema", {
        sampleOutput: validationOutput,
        shardKey
      });
      schemaProposal = proposal;
      editedSchema = proposal.columns.map(c => ({ ...c }));
      sinkType = proposal.suggestedSink;
      outputPath = proposal.suggestedOutputPath;
    } catch (e) {
      error = `Schema inference failed: ${e}. You can configure manually below.`;
      // Provide default schema
      editedSchema = [{ name: "column_1", inferredType: "string", nullable: true, description: null }];
      sinkType = "parquet";
      outputPath = `~/.casparian_flow/output/${shardKey}/${shardKey}.parquet`;
    } finally {
      isProposingSchema = false;
    }
  }

  // Publish the parser
  async function handlePublish() {
    phase = "publishing";
    isPublishing = true;
    error = null;

    try {
      const receipt = await invoke<ParserPublishReceipt>("publish_parser", {
        shardKey,
        sourceCode: currentCode,
        schema: editedSchema,
        sinkType,
        outputPath
      });
      publishReceipt = receipt;
      phase = "published";
    } catch (e) {
      error = `Publication failed: ${e}`;
      phase = "configuring";
    } finally {
      isPublishing = false;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
      e.preventDefault();
      validateParser();
    }
  }

  function addSchemaColumn() {
    editedSchema = [...editedSchema, { name: `column_${editedSchema.length + 1}`, inferredType: "string", nullable: true, description: null }];
  }

  function removeSchemaColumn(index: number) {
    editedSchema = editedSchema.filter((_, i) => i !== index);
  }

  function handleDone() {
    onApprove(currentCode);
    onClose();
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="modal-overlay" onclick={onClose}>
  <div class="modal" onclick={(e) => e.stopPropagation()}>
    <!-- Header with Phase Indicator -->
    <div class="modal-header">
      <div class="header-left">
        <div class="header-info">
          <span class="header-title">PARSER WIZARD</span>
          <span class="file-name">{shardKey}</span>
        </div>
      </div>

      <div class="phase-indicator">
        <div class="phase" class:active={phase === "refining"} class:done={phase !== "refining"}>
          <span class="phase-dot">{phase === "refining" ? "1" : "✓"}</span>
          <span class="phase-label">VALIDATE</span>
        </div>
        <div class="phase-line" class:done={phase !== "refining"}></div>
        <div class="phase" class:active={phase === "configuring"} class:done={phase === "publishing" || phase === "published"}>
          <span class="phase-dot">{phase === "configuring" ? "2" : (phase === "publishing" || phase === "published") ? "✓" : "2"}</span>
          <span class="phase-label">CONFIGURE</span>
        </div>
        <div class="phase-line" class:done={phase === "publishing" || phase === "published"}></div>
        <div class="phase" class:active={phase === "publishing" || phase === "published"}>
          <span class="phase-dot">{phase === "published" ? "✓" : "3"}</span>
          <span class="phase-label">PUBLISH</span>
        </div>
      </div>

      <button class="close-btn" onclick={onClose}>&times;</button>
    </div>

    <div class="modal-body">
      <!-- PHASE 1: REFINING -->
      {#if phase === "refining"}
        <!-- Sample Data -->
        <div class="section sample-section">
          <div class="section-header">
            <span class="section-title">SAMPLE DATA</span>
          </div>
          <div class="sample-content">
            {#each sampleInput.slice(0, 5) as line, i}
              <div class="sample-line">
                <span class="line-num">{i + 1}</span>
                <span class="line-text">{line}</span>
              </div>
            {/each}
            {#if sampleInput.length > 5}
              <div class="sample-more">...and {sampleInput.length - 5} more rows</div>
            {/if}
          </div>
        </div>

        <!-- Two-Column Layout: Chat + Code -->
        <div class="main-content">
          <div class="chat-column">
            <div class="section-header">
              <span class="section-title">CONVERSATION</span>
              {#if isLoading}<span class="loading-badge">AI thinking...</span>{/if}
            </div>
            <div class="messages-container">
              {#if messages.length === 0 && !isLoading && !isGenerating}
                <div class="empty-state"><span>Validating parser...</span></div>
              {:else if isGenerating}
                <div class="empty-state"><div class="spinner"></div><span>Generating parser with Claude...</span></div>
              {/if}
              {#each messages as message}
                <div class="message {message.role}">
                  <div class="message-role">{message.role === "user" ? "You" : "AI"}</div>
                  <div class="message-content">{message.content}</div>
                </div>
              {/each}
              {#if isLoading}
                <div class="message assistant loading"><div class="message-role">AI</div><div class="message-content">Analyzing...</div></div>
              {/if}
            </div>
            {#if suggestedReplies.length > 0 && !isLoading}
              <div class="suggested-replies">
                {#each suggestedReplies as reply}
                  <button class="reply-btn" onclick={() => handleSuggestedReply(reply)}>{reply}</button>
                {/each}
              </div>
            {/if}
            <form class="input-section" onsubmit={handleSubmit}>
              <input type="text" class="chat-input" placeholder="Describe the issue or ask for help..." bind:value={userInput} disabled={isLoading} />
              <button type="submit" class="send-btn" disabled={isLoading || !userInput.trim()}>Send</button>
            </form>
          </div>

          <div class="code-column">
            <div class="section-header">
              <span class="section-title">PARSER CODE</span>
              <span class="hint">Cmd/Ctrl+Enter to validate</span>
            </div>
            <textarea class="code-editor" bind:value={currentCode} onkeydown={handleKeydown} spellcheck="false"></textarea>
            {#if validationSuccess && validationOutput}
              <div class="validation-box success">
                <div class="validation-header"><span class="success-badge">SUCCESS</span></div>
                <pre class="validation-output">{validationOutput}</pre>
              </div>
            {:else if validationError}
              <div class="validation-box error">
                <div class="validation-header"><span class="error-badge">ERROR</span></div>
                <pre class="validation-output">{validationError}</pre>
              </div>
            {/if}
          </div>
        </div>

      <!-- PHASE 2: CONFIGURING -->
      {:else if phase === "configuring"}
        <div class="config-content">
          {#if isProposingSchema}
            <div class="loading-section">
              <div class="spinner"></div>
              <span>AI is analyzing output and proposing schema...</span>
            </div>
          {:else}
            <!-- Schema Editor -->
            <div class="config-section">
              <div class="section-header">
                <span class="section-title">OUTPUT SCHEMA</span>
                <button class="add-btn" onclick={addSchemaColumn}>+ Add Column</button>
              </div>
              <div class="schema-table-container">
                <table class="schema-table">
                  <thead>
                    <tr>
                      <th>Column Name</th>
                      <th>Type</th>
                      <th>Nullable</th>
                      <th>Description</th>
                      <th></th>
                    </tr>
                  </thead>
                  <tbody>
                    {#each editedSchema as col, i}
                      <tr>
                        <td><input type="text" bind:value={col.name} class="schema-input" /></td>
                        <td>
                          <select bind:value={col.inferredType} class="schema-select">
                            <option value="string">string</option>
                            <option value="int64">int64</option>
                            <option value="float64">float64</option>
                            <option value="datetime">datetime</option>
                            <option value="boolean">boolean</option>
                          </select>
                        </td>
                        <td><input type="checkbox" bind:checked={col.nullable} /></td>
                        <td><input type="text" bind:value={col.description} class="schema-input" placeholder="Optional description" /></td>
                        <td><button class="remove-btn" onclick={() => removeSchemaColumn(i)}>&times;</button></td>
                      </tr>
                    {/each}
                  </tbody>
                </table>
              </div>
            </div>

            <!-- Sink Configuration -->
            <div class="config-section">
              <div class="section-header">
                <span class="section-title">OUTPUT CONFIGURATION</span>
              </div>
              <div class="config-grid">
                <div class="config-row">
                  <label class="config-label">Sink Type</label>
                  <select bind:value={sinkType} class="config-select">
                    <option value="parquet">Parquet (recommended for analytics)</option>
                    <option value="csv">CSV (for interoperability)</option>
                    <option value="sqlite">SQLite (for queryable storage)</option>
                  </select>
                </div>
                <div class="config-row">
                  <label class="config-label">Output Path</label>
                  <input type="text" bind:value={outputPath} class="config-input" />
                </div>
              </div>
            </div>

            <!-- AI Reasoning -->
            {#if schemaProposal?.reasoning}
              <div class="reasoning-section">
                <span class="reasoning-label">AI Reasoning:</span>
                <span class="reasoning-text">{schemaProposal.reasoning}</span>
              </div>
            {/if}
          {/if}
        </div>

      <!-- PHASE 3: PUBLISHED -->
      {:else if phase === "publishing" || phase === "published"}
        <div class="publish-content">
          {#if isPublishing}
            <div class="loading-section">
              <div class="spinner"></div>
              <span>Publishing parser...</span>
            </div>
          {:else if publishReceipt}
            <div class="receipt-card" class:success={publishReceipt.success}>
              <div class="receipt-header">
                <span class="receipt-icon">{publishReceipt.success ? "✓" : "✗"}</span>
                <span class="receipt-title">{publishReceipt.success ? "Parser Published Successfully" : "Publication Failed"}</span>
              </div>
              <div class="receipt-body">
                <div class="receipt-row">
                  <span class="receipt-label">Plugin Name</span>
                  <span class="receipt-value">{publishReceipt.pluginName}</span>
                </div>
                <div class="receipt-row">
                  <span class="receipt-label">Parser File</span>
                  <span class="receipt-value mono">{publishReceipt.parserFilePath}</span>
                </div>
                <div class="receipt-row">
                  <span class="receipt-label">Sink</span>
                  <span class="receipt-value">{sinkType} → {outputPath}</span>
                </div>
                {#if publishReceipt.manifestId}
                  <div class="receipt-row">
                    <span class="receipt-label">Manifest ID</span>
                    <span class="receipt-value">#{publishReceipt.manifestId}</span>
                  </div>
                {/if}
                <div class="receipt-row">
                  <span class="receipt-label">Subscription Tag</span>
                  <span class="receipt-value">{shardKey}</span>
                </div>
              </div>
              <div class="receipt-message">{publishReceipt.message}</div>
            </div>
          {/if}
        </div>
      {/if}
    </div>

    <!-- Footer -->
    <div class="modal-footer">
      {#if error}
        <div class="error-message">{error}</div>
      {/if}

      <div class="action-row">
        {#if phase === "refining"}
          <button class="action-btn" onclick={onClose}>Close</button>
          <button class="action-btn" onclick={() => validateParser()} disabled={isValidating || isLoading || !currentCode}>
            {isValidating ? "Validating..." : "Validate"}
          </button>
          <button class="action-btn primary" onclick={handleApprove} disabled={!validationSuccess}>
            Approve Parser
          </button>
        {:else if phase === "configuring"}
          <button class="action-btn" onclick={() => phase = "refining"}>Back</button>
          <button class="action-btn primary" onclick={handlePublish} disabled={isPublishing || !outputPath || editedSchema.length === 0}>
            {isPublishing ? "Publishing..." : "Publish"}
          </button>
        {:else if phase === "published"}
          <button class="action-btn primary" onclick={handleDone}>Done</button>
        {/if}
      </div>
    </div>
  </div>
</div>

<style>
  .modal-overlay {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(0, 0, 0, 0.8);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }

  .modal {
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    width: 95%;
    max-width: 1200px;
    height: 90vh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  /* Header */
  .modal-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--space-md) var(--space-lg);
    border-bottom: 1px solid var(--color-border);
    background: var(--color-bg-tertiary);
    gap: var(--space-lg);
  }

  .header-left {
    display: flex;
    align-items: center;
  }

  .header-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .header-title {
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 1px;
    color: var(--color-text-muted);
  }

  .file-name {
    font-family: var(--font-mono);
    font-size: 14px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  /* Phase Indicator */
  .phase-indicator {
    display: flex;
    align-items: center;
    gap: var(--space-xs);
  }

  .phase {
    display: flex;
    align-items: center;
    gap: var(--space-xs);
  }

  .phase-dot {
    width: 24px;
    height: 24px;
    border-radius: 50%;
    background: var(--color-bg-secondary);
    border: 2px solid var(--color-border);
    display: flex;
    align-items: center;
    justify-content: center;
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 600;
    color: var(--color-text-muted);
  }

  .phase.active .phase-dot {
    background: var(--color-accent-cyan);
    border-color: var(--color-accent-cyan);
    color: var(--color-bg-primary);
  }

  .phase.done .phase-dot {
    background: var(--color-success);
    border-color: var(--color-success);
    color: var(--color-bg-primary);
  }

  .phase-label {
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 600;
    color: var(--color-text-muted);
    letter-spacing: 0.5px;
  }

  .phase.active .phase-label {
    color: var(--color-accent-cyan);
  }

  .phase.done .phase-label {
    color: var(--color-success);
  }

  .phase-line {
    width: 40px;
    height: 2px;
    background: var(--color-border);
  }

  .phase-line.done {
    background: var(--color-success);
  }

  .close-btn {
    background: none;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 20px;
    padding: 4px 8px;
  }

  .close-btn:hover {
    color: var(--color-text-primary);
  }

  /* Modal Body */
  .modal-body {
    flex: 1;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    padding: var(--space-md);
    gap: var(--space-md);
  }

  .section {
    background: var(--color-bg-tertiary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }

  .section-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--space-sm) var(--space-md);
    background: var(--color-bg-secondary);
    border-bottom: 1px solid var(--color-border);
  }

  .section-title {
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 600;
    color: var(--color-text-muted);
    letter-spacing: 0.5px;
  }

  .hint {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
  }

  .loading-badge {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-accent-cyan);
  }

  /* Sample Section */
  .sample-section { flex-shrink: 0; }

  .sample-content {
    max-height: 80px;
    overflow: auto;
    padding: var(--space-xs) 0;
  }

  .sample-line {
    display: flex;
    padding: 2px var(--space-md);
    font-family: var(--font-mono);
    font-size: 11px;
    gap: var(--space-sm);
  }

  .line-num {
    color: var(--color-text-muted);
    min-width: 20px;
    user-select: none;
  }

  .line-text {
    color: var(--color-text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .sample-more {
    padding: 4px var(--space-md);
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
  }

  /* Main Content */
  .main-content {
    flex: 1;
    display: flex;
    gap: var(--space-md);
    overflow: hidden;
    min-height: 0;
  }

  .chat-column, .code-column {
    flex: 1;
    display: flex;
    flex-direction: column;
    background: var(--color-bg-tertiary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }

  .messages-container {
    flex: 1;
    overflow: auto;
    padding: var(--space-md);
    display: flex;
    flex-direction: column;
    gap: var(--space-sm);
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    gap: var(--space-md);
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-muted);
  }

  .spinner {
    width: 24px;
    height: 24px;
    border: 2px solid var(--color-border);
    border-top-color: var(--color-accent-cyan);
    border-radius: 50%;
    animation: spin 1s linear infinite;
  }

  @keyframes spin { to { transform: rotate(360deg); } }

  .message {
    padding: var(--space-sm) var(--space-md);
    border-radius: var(--radius-sm);
    max-width: 90%;
  }

  .message.user {
    align-self: flex-end;
    background: var(--color-accent-cyan);
    color: var(--color-bg-primary);
  }

  .message.assistant {
    align-self: flex-start;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
  }

  .message.loading { opacity: 0.7; }

  .message-role {
    font-family: var(--font-mono);
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    margin-bottom: 4px;
    opacity: 0.7;
  }

  .message-content {
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 1.5;
    white-space: pre-wrap;
  }

  .suggested-replies {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-xs);
    padding: var(--space-sm) var(--space-md);
    border-top: 1px solid var(--color-border);
  }

  .reply-btn {
    padding: 6px 12px;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 16px;
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-secondary);
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .reply-btn:hover {
    border-color: var(--color-accent-cyan);
    color: var(--color-accent-cyan);
  }

  .input-section {
    display: flex;
    gap: var(--space-sm);
    padding: var(--space-sm) var(--space-md);
    border-top: 1px solid var(--color-border);
    background: var(--color-bg-secondary);
  }

  .chat-input {
    flex: 1;
    padding: var(--space-xs) var(--space-sm);
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-primary);
  }

  .chat-input:focus {
    outline: none;
    border-color: var(--color-accent-cyan);
  }

  .send-btn {
    padding: 6px 12px;
    background: var(--color-accent-cyan);
    border: none;
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-bg-primary);
    cursor: pointer;
  }

  .send-btn:disabled { opacity: 0.5; cursor: not-allowed; }

  .code-editor {
    flex: 1;
    padding: var(--space-md);
    background: var(--color-bg-primary);
    border: none;
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-primary);
    line-height: 1.5;
    resize: none;
    min-height: 200px;
  }

  .code-editor:focus { outline: none; }

  .validation-box {
    border-top: 1px solid var(--color-border);
    max-height: 150px;
    overflow: auto;
  }

  .validation-header {
    padding: var(--space-xs) var(--space-md);
    background: var(--color-bg-secondary);
    border-bottom: 1px solid var(--color-border);
  }

  .validation-output {
    padding: var(--space-sm) var(--space-md);
    font-family: var(--font-mono);
    font-size: 11px;
    line-height: 1.5;
    margin: 0;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .validation-box.success { background: rgba(0, 255, 136, 0.05); }
  .validation-box.success .validation-output { color: var(--color-success); }
  .validation-box.error { background: rgba(255, 77, 77, 0.05); }
  .validation-box.error .validation-output { color: var(--color-error); }

  .success-badge, .error-badge {
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.5px;
  }

  .success-badge { color: var(--color-success); }
  .error-badge { color: var(--color-error); }

  /* Config Content (Phase 2) */
  .config-content {
    flex: 1;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: var(--space-md);
  }

  .loading-section {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: var(--space-xl);
    gap: var(--space-md);
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-muted);
  }

  .config-section {
    background: var(--color-bg-tertiary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }

  .add-btn {
    padding: 4px 8px;
    background: transparent;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
    cursor: pointer;
  }

  .add-btn:hover {
    border-color: var(--color-accent-cyan);
    color: var(--color-accent-cyan);
  }

  .schema-table-container {
    overflow: auto;
    max-height: 300px;
  }

  .schema-table {
    width: 100%;
    border-collapse: collapse;
    font-family: var(--font-mono);
    font-size: 11px;
  }

  .schema-table th, .schema-table td {
    padding: var(--space-sm) var(--space-md);
    text-align: left;
    border-bottom: 1px solid var(--color-border);
  }

  .schema-table th {
    background: var(--color-bg-secondary);
    color: var(--color-text-muted);
    font-weight: 600;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .schema-input, .schema-select {
    width: 100%;
    padding: var(--space-xs);
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-primary);
  }

  .schema-input:focus, .schema-select:focus {
    outline: none;
    border-color: var(--color-accent-cyan);
  }

  .remove-btn {
    background: transparent;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 14px;
  }

  .remove-btn:hover { color: var(--color-error); }

  .config-grid {
    padding: var(--space-md);
    display: flex;
    flex-direction: column;
    gap: var(--space-md);
  }

  .config-row {
    display: flex;
    flex-direction: column;
    gap: var(--space-xs);
  }

  .config-label {
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 600;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .config-select, .config-input {
    padding: var(--space-sm);
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-primary);
  }

  .config-select:focus, .config-input:focus {
    outline: none;
    border-color: var(--color-accent-cyan);
  }

  .reasoning-section {
    padding: var(--space-md);
    background: var(--color-bg-tertiary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
  }

  .reasoning-label {
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 600;
    color: var(--color-accent-cyan);
    margin-right: var(--space-sm);
  }

  .reasoning-text {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-secondary);
  }

  /* Publish Content (Phase 3) */
  .publish-content {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .receipt-card {
    background: var(--color-bg-tertiary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    max-width: 500px;
    width: 100%;
    overflow: hidden;
  }

  .receipt-card.success {
    border-color: var(--color-success);
  }

  .receipt-header {
    display: flex;
    align-items: center;
    gap: var(--space-md);
    padding: var(--space-md) var(--space-lg);
    background: var(--color-bg-secondary);
    border-bottom: 1px solid var(--color-border);
  }

  .receipt-icon {
    width: 32px;
    height: 32px;
    border-radius: 50%;
    background: var(--color-success);
    color: white;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 18px;
    font-weight: bold;
  }

  .receipt-title {
    font-family: var(--font-mono);
    font-size: 14px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .receipt-body {
    padding: var(--space-md) var(--space-lg);
    display: flex;
    flex-direction: column;
    gap: var(--space-sm);
  }

  .receipt-row {
    display: flex;
    justify-content: space-between;
    font-family: var(--font-mono);
    font-size: 12px;
  }

  .receipt-label { color: var(--color-text-muted); }
  .receipt-value { color: var(--color-text-primary); }
  .receipt-value.mono { font-size: 10px; }

  .receipt-message {
    padding: var(--space-md) var(--space-lg);
    background: rgba(0, 255, 136, 0.05);
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-success);
    border-top: 1px solid var(--color-border);
  }

  /* Footer */
  .modal-footer {
    padding: var(--space-md) var(--space-lg);
    border-top: 1px solid var(--color-border);
    display: flex;
    flex-direction: column;
    gap: var(--space-sm);
  }

  .error-message {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-error);
    padding: var(--space-sm);
    background: rgba(255, 77, 77, 0.1);
    border-radius: var(--radius-sm);
  }

  .action-row {
    display: flex;
    justify-content: flex-end;
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

  .action-btn:hover:not(:disabled) {
    border-color: var(--color-accent-cyan);
    color: var(--color-accent-cyan);
  }

  .action-btn:disabled { opacity: 0.5; cursor: not-allowed; }

  .action-btn.primary {
    background: var(--color-accent-cyan);
    border-color: var(--color-accent-cyan);
    color: var(--color-bg-primary);
  }

  .action-btn.primary:hover:not(:disabled) { opacity: 0.9; }
</style>
