<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  // Types from Rust backend
  interface ShredStrategyInfo {
    strategyType: string;
    delimiter?: string;
    colIndex?: number;
    hasHeader?: boolean;
    keyPath?: string;
    pattern?: string;
    keyGroup?: string;
  }

  interface ChatMessage {
    role: "user" | "assistant";
    content: string;
  }

  interface ChatResponse {
    message: string;
    proposedStrategy: ShredStrategyInfo | null;
    suggestedReplies: string[];
    isReady: boolean;
    samplePreview: string[];
  }

  // Props
  interface Props {
    filePath: string;
    onStrategyReady?: (strategy: ShredStrategyInfo, reasoning: string) => void;
    onClose?: () => void;
  }

  let { filePath, onStrategyReady, onClose }: Props = $props();

  // State
  let messages = $state<ChatMessage[]>([]);
  let currentInput = $state("");
  let userContext = $state("");  // Optional upfront guidance
  let isLoading = $state(false);
  let samplePreview = $state<string[]>([]);
  let suggestedReplies = $state<string[]>([]);
  let proposedStrategy = $state<ShredStrategyInfo | null>(null);
  let error = $state<string | null>(null);
  let showContextInput = $state(true);  // Show context input initially

  // Start analysis when component mounts
  $effect(() => {
    // Reset when file changes
    if (filePath) {
      messages = [];
      proposedStrategy = null;
      suggestedReplies = [];
      samplePreview = [];
      error = null;
    }
  });

  // Start initial analysis (with or without context)
  async function startAnalysis() {
    showContextInput = false;
    await sendMessage(userContext.trim() || undefined, true);
  }

  // Send message to LLM
  async function sendMessage(input?: string, isInitial = false) {
    if (isLoading) return;
    if (!isInitial && !input?.trim()) return;

    isLoading = true;
    error = null;

    // Add user message to history (if not initial)
    if (input && !isInitial) {
      messages = [...messages, { role: "user", content: input }];
    } else if (input && isInitial) {
      // Add context as first user message
      messages = [{ role: "user", content: input }];
    }

    try {
      const messagesJson = JSON.stringify(messages);
      const response = await invoke<ChatResponse>("shredder_chat", {
        path: filePath,
        messagesJson,
        userInput: input || null
      });

      // Update sample preview if provided
      if (response.samplePreview.length > 0) {
        samplePreview = response.samplePreview;
      }

      // Add assistant response to history
      messages = [...messages, { role: "assistant", content: response.message }];

      // Update suggested replies
      suggestedReplies = response.suggestedReplies;

      // Check if strategy is ready
      if (response.proposedStrategy) {
        proposedStrategy = response.proposedStrategy;
      }

      currentInput = "";
    } catch (e) {
      error = e as string;
    } finally {
      isLoading = false;
    }
  }

  // Handle suggested reply click
  function handleSuggestedReply(reply: string) {
    sendMessage(reply);
  }

  // Handle text input submit
  function handleSubmit(e: Event) {
    e.preventDefault();
    if (currentInput.trim()) {
      sendMessage(currentInput.trim());
    }
  }

  // Accept proposed strategy
  function handleAcceptStrategy() {
    if (proposedStrategy && onStrategyReady) {
      // Find the last assistant message for reasoning
      const lastAssistant = messages.filter(m => m.role === "assistant").pop();
      onStrategyReady(proposedStrategy, lastAssistant?.content || "");
    }
  }

  // Format strategy for display
  function formatStrategy(strategy: ShredStrategyInfo): string {
    if (strategy.strategyType === "CsvColumn") {
      const delim = strategy.delimiter === "\t" ? "TAB" : strategy.delimiter;
      return `Split by column ${strategy.colIndex} (delimiter: ${delim}, header: ${strategy.hasHeader ? "yes" : "no"})`;
    } else if (strategy.strategyType === "JsonKey") {
      return `Split by JSON key: ${strategy.keyPath}`;
    } else if (strategy.strategyType === "Regex") {
      return `Split by pattern: ${strategy.pattern}`;
    }
    return strategy.strategyType;
  }
</script>

<div class="chat-container">
  <!-- Header -->
  <div class="chat-header">
    <div class="header-info">
      <span class="header-title">FILE ANALYSIS</span>
      <span class="file-name">{filePath.split("/").pop()}</span>
    </div>
    {#if onClose}
      <button class="close-btn" onclick={onClose}>Close</button>
    {/if}
  </div>

  <!-- Sample Preview -->
  {#if samplePreview.length > 0}
    <div class="sample-section">
      <div class="sample-header">Sample Preview</div>
      <div class="sample-content">
        {#each samplePreview.slice(0, 5) as line, i}
          <div class="sample-line">
            <span class="line-num">{i + 1}</span>
            <span class="line-text">{line}</span>
          </div>
        {/each}
        {#if samplePreview.length > 5}
          <div class="sample-more">...and {samplePreview.length - 5} more lines</div>
        {/if}
      </div>
    </div>
  {/if}

  <!-- Context Input (shown before first analysis) -->
  {#if showContextInput}
    <div class="context-section">
      <div class="context-header">
        <span class="context-title">Optional: Tell me about this file</span>
        <span class="context-hint">This helps the AI understand your data better</span>
      </div>
      <textarea
        class="context-input"
        placeholder="e.g., This is aircraft telemetry. Message types are in column 1 and end with colons. Skip lines starting with #."
        bind:value={userContext}
        rows="3"
      ></textarea>
      <div class="context-actions">
        <button class="action-btn" onclick={() => startAnalysis()}>
          {userContext.trim() ? "Analyze with Context" : "Analyze Without Context"}
        </button>
      </div>
    </div>
  {/if}

  <!-- Conversation -->
  {#if !showContextInput}
    <div class="messages-container">
      {#each messages as message}
        <div class="message {message.role}">
          <div class="message-role">{message.role === "user" ? "You" : "AI"}</div>
          <div class="message-content">{message.content}</div>
        </div>
      {/each}

      {#if isLoading}
        <div class="message assistant loading">
          <div class="message-role">AI</div>
          <div class="message-content">Analyzing...</div>
        </div>
      {/if}
    </div>

    <!-- Proposed Strategy -->
    {#if proposedStrategy}
      <div class="strategy-proposal">
        <div class="strategy-header">
          <span class="strategy-icon">&#10004;</span>
          <span class="strategy-title">Proposed Strategy</span>
        </div>
        <div class="strategy-content">
          {formatStrategy(proposedStrategy)}
        </div>
        <div class="strategy-actions">
          <button class="action-btn primary" onclick={handleAcceptStrategy}>
            Accept & Continue
          </button>
          <button class="action-btn" onclick={() => { proposedStrategy = null; }}>
            Ask for Changes
          </button>
        </div>
      </div>
    {/if}

    <!-- Suggested Replies -->
    {#if suggestedReplies.length > 0 && !isLoading}
      <div class="suggested-replies">
        {#each suggestedReplies as reply}
          <button class="reply-btn" onclick={() => handleSuggestedReply(reply)}>
            {reply}
          </button>
        {/each}
      </div>
    {/if}

    <!-- Input -->
    <form class="input-section" onsubmit={handleSubmit}>
      <input
        type="text"
        class="chat-input"
        placeholder="Type your guidance or ask a question..."
        bind:value={currentInput}
        disabled={isLoading}
      />
      <button type="submit" class="send-btn" disabled={isLoading || !currentInput.trim()}>
        Send
      </button>
    </form>
  {/if}

  <!-- Error -->
  {#if error}
    <div class="error-message">
      <span class="error-icon">!</span>
      <span>{error}</span>
      <button class="dismiss-btn" onclick={() => error = null}>&#10005;</button>
    </div>
  {/if}
</div>

<style>
  .chat-container {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--color-bg-secondary);
    border-radius: var(--radius-md);
    border: 1px solid var(--color-border);
    overflow: hidden;
  }

  /* Header */
  .chat-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--space-md);
    background: var(--color-bg-tertiary);
    border-bottom: 1px solid var(--color-border);
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
    font-size: 13px;
    color: var(--color-text-primary);
  }

  .close-btn {
    padding: 4px 12px;
    background: transparent;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-muted);
    cursor: pointer;
  }

  .close-btn:hover {
    border-color: var(--color-text-muted);
    color: var(--color-text-primary);
  }

  /* Sample Section */
  .sample-section {
    background: var(--color-bg-card);
    border-bottom: 1px solid var(--color-border);
  }

  .sample-header {
    padding: var(--space-sm) var(--space-md);
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    background: var(--color-bg-tertiary);
    border-bottom: 1px solid var(--color-border);
  }

  .sample-content {
    max-height: 120px;
    overflow: auto;
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

  /* Context Section */
  .context-section {
    padding: var(--space-md);
    display: flex;
    flex-direction: column;
    gap: var(--space-sm);
  }

  .context-header {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .context-title {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-primary);
  }

  .context-hint {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
  }

  .context-input {
    width: 100%;
    padding: var(--space-sm);
    background: var(--color-bg-tertiary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-primary);
    resize: none;
  }

  .context-input:focus {
    outline: none;
    border-color: var(--color-accent-cyan);
  }

  .context-input::placeholder {
    color: var(--color-text-muted);
  }

  .context-actions {
    display: flex;
    justify-content: flex-end;
  }

  /* Messages */
  .messages-container {
    flex: 1;
    overflow: auto;
    padding: var(--space-md);
    display: flex;
    flex-direction: column;
    gap: var(--space-md);
  }

  .message {
    padding: var(--space-sm) var(--space-md);
    border-radius: var(--radius-sm);
    max-width: 85%;
  }

  .message.user {
    align-self: flex-end;
    background: var(--color-accent-cyan);
    color: var(--color-bg-primary);
  }

  .message.assistant {
    align-self: flex-start;
    background: var(--color-bg-tertiary);
    border: 1px solid var(--color-border);
  }

  .message.loading {
    opacity: 0.7;
  }

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

  /* Strategy Proposal */
  .strategy-proposal {
    margin: 0 var(--space-md);
    padding: var(--space-md);
    background: rgba(0, 212, 255, 0.1);
    border: 1px solid rgba(0, 212, 255, 0.3);
    border-radius: var(--radius-sm);
  }

  .strategy-header {
    display: flex;
    align-items: center;
    gap: var(--space-sm);
    margin-bottom: var(--space-sm);
  }

  .strategy-icon {
    color: var(--color-success);
    font-weight: bold;
  }

  .strategy-title {
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 600;
    color: var(--color-accent-cyan);
  }

  .strategy-content {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-primary);
    margin-bottom: var(--space-md);
  }

  .strategy-actions {
    display: flex;
    gap: var(--space-sm);
  }

  /* Suggested Replies */
  .suggested-replies {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-xs);
    padding: var(--space-sm) var(--space-md);
    border-top: 1px solid var(--color-border);
  }

  .reply-btn {
    padding: 6px 12px;
    background: var(--color-bg-tertiary);
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

  /* Input Section */
  .input-section {
    display: flex;
    gap: var(--space-sm);
    padding: var(--space-md);
    border-top: 1px solid var(--color-border);
    background: var(--color-bg-tertiary);
  }

  .chat-input {
    flex: 1;
    padding: var(--space-sm) var(--space-md);
    background: var(--color-bg-secondary);
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

  .chat-input::placeholder {
    color: var(--color-text-muted);
  }

  .send-btn {
    padding: 6px 16px;
    background: var(--color-accent-cyan);
    border: none;
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-bg-primary);
    cursor: pointer;
  }

  .send-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  /* Action Button */
  .action-btn {
    padding: 6px 16px;
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

  .action-btn.primary {
    background: var(--color-accent-cyan);
    border-color: var(--color-accent-cyan);
    color: var(--color-bg-primary);
  }

  .action-btn.primary:hover {
    opacity: 0.9;
  }

  /* Error */
  .error-message {
    display: flex;
    align-items: center;
    gap: var(--space-sm);
    padding: var(--space-sm) var(--space-md);
    margin: var(--space-md);
    background: rgba(239, 68, 68, 0.1);
    border: 1px solid rgba(239, 68, 68, 0.3);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-error);
  }

  .error-icon {
    width: 16px;
    height: 16px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--color-error);
    color: white;
    border-radius: 50%;
    font-size: 10px;
    font-weight: bold;
    flex-shrink: 0;
  }

  .dismiss-btn {
    margin-left: auto;
    background: none;
    border: none;
    color: var(--color-error);
    cursor: pointer;
    padding: 2px;
    opacity: 0.7;
  }

  .dismiss-btn:hover {
    opacity: 1;
  }
</style>
