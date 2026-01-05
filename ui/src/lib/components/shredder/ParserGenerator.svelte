<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  // Props
  interface Props {
    shardPath: string;
    shardKey: string;
    onClose: () => void;
  }

  let { shardPath, shardKey, onClose }: Props = $props();

  // Types
  interface ParserDraft {
    shardKey: string;
    sourceCode: string;
    sampleInput: string[];
    sampleOutput: string | null;
    validationError: string | null;
  }

  // State
  let isGenerating = $state(false);
  let isValidating = $state(false);
  let error = $state<string | null>(null);
  let parserDraft = $state<ParserDraft | null>(null);
  let editedCode = $state("");

  // Generate parser
  async function handleGenerate() {
    isGenerating = true;
    error = null;
    parserDraft = null;

    try {
      const draft = await invoke<ParserDraft>("generate_parser_draft", {
        shardPath
      });
      parserDraft = draft;
      editedCode = draft.sourceCode;
    } catch (e) {
      error = e as string;
    } finally {
      isGenerating = false;
    }
  }

  // Validate parser
  async function handleValidate() {
    if (!editedCode) return;

    isValidating = true;
    error = null;

    try {
      const result = await invoke<ParserDraft>("validate_parser", {
        shardPath,
        sourceCode: editedCode
      });
      parserDraft = result;
    } catch (e) {
      error = e as string;
    } finally {
      isValidating = false;
    }
  }

  // Handle keyboard shortcut for validation
  function handleKeydown(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
      e.preventDefault();
      handleValidate();
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="modal-overlay" onclick={onClose}>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="modal" onclick={(e) => e.stopPropagation()}>
    <div class="modal-header">
      <span class="modal-title">Parser Generator: {shardKey}</span>
      <button class="close-btn" onclick={onClose}>&#10005;</button>
    </div>

    <div class="modal-body">
      <!-- Sample Input -->
      <div class="section">
        <div class="section-header">
          <span class="section-title">SAMPLE INPUT</span>
          <span class="section-meta">{shardPath}</span>
        </div>
        <div class="sample-box">
          {#if parserDraft?.sampleInput}
            {#each parserDraft.sampleInput.slice(0, 5) as line}
              <div class="sample-line">{line}</div>
            {/each}
            {#if parserDraft.sampleInput.length > 5}
              <div class="sample-more">... {parserDraft.sampleInput.length - 5} more rows</div>
            {/if}
          {:else}
            <div class="sample-placeholder">Click "Generate Parser" to start</div>
          {/if}
        </div>
      </div>

      <!-- Code Editor -->
      <div class="section code-section">
        <div class="section-header">
          <span class="section-title">GENERATED PARSER</span>
          {#if parserDraft}
            <span class="hint">Cmd/Ctrl+Enter to validate</span>
          {/if}
        </div>
        {#if parserDraft}
          <textarea
            class="code-editor"
            bind:value={editedCode}
            onkeydown={handleKeydown}
            spellcheck="false"
          ></textarea>
        {:else}
          <div class="code-placeholder">
            {#if isGenerating}
              <div class="spinner"></div>
              <span>Generating parser with Claude Code...</span>
            {:else}
              <span>Click "Generate Parser" to create a parser using Claude Code CLI</span>
            {/if}
          </div>
        {/if}
      </div>

      <!-- Validation Result -->
      {#if parserDraft?.sampleOutput || parserDraft?.validationError}
        <div class="section">
          <div class="section-header">
            <span class="section-title">VALIDATION RESULT</span>
            {#if parserDraft.sampleOutput}
              <span class="success-badge">SUCCESS</span>
            {:else}
              <span class="error-badge">ERROR</span>
            {/if}
          </div>
          {#if parserDraft.sampleOutput}
            <div class="output-box success">
              <pre>{parserDraft.sampleOutput}</pre>
            </div>
          {:else if parserDraft.validationError}
            <div class="output-box error">
              <pre>{parserDraft.validationError}</pre>
            </div>
          {/if}
        </div>
      {/if}
    </div>

    <div class="modal-footer">
      {#if error}
        <div class="error-message">{error}</div>
      {/if}

      <div class="action-row">
        <button class="action-btn" onclick={onClose}>Close</button>

        {#if parserDraft}
          <button
            class="action-btn"
            onclick={handleGenerate}
            disabled={isGenerating}
          >
            {isGenerating ? "Regenerating..." : "Regenerate"}
          </button>
          <button
            class="action-btn primary"
            onclick={handleValidate}
            disabled={isValidating || !editedCode}
          >
            {isValidating ? "Validating..." : "Validate"}
          </button>
        {:else}
          <button
            class="action-btn primary"
            onclick={handleGenerate}
            disabled={isGenerating}
          >
            {isGenerating ? "Generating..." : "Generate Parser"}
          </button>
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
    width: 90%;
    max-width: 900px;
    max-height: 90vh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .modal-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--space-md) var(--space-lg);
    border-bottom: 1px solid var(--color-border);
  }

  .modal-title {
    font-family: var(--font-mono);
    font-size: 14px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .close-btn {
    background: none;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 16px;
    padding: 4px;
  }

  .close-btn:hover {
    color: var(--color-text-primary);
  }

  /* Modal Body */
  .modal-body {
    flex: 1;
    overflow: auto;
    padding: var(--space-lg);
    display: flex;
    flex-direction: column;
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

  .section-meta {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
    max-width: 300px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .hint {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
  }

  /* Sample Box */
  .sample-box {
    padding: var(--space-sm) var(--space-md);
    max-height: 120px;
    overflow: auto;
  }

  .sample-line {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-secondary);
    line-height: 1.5;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .sample-more {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
    margin-top: var(--space-xs);
  }

  .sample-placeholder {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-muted);
    text-align: center;
    padding: var(--space-md);
  }

  /* Code Section */
  .code-section {
    flex: 1;
    min-height: 200px;
    display: flex;
    flex-direction: column;
  }

  .code-editor {
    flex: 1;
    min-height: 200px;
    padding: var(--space-md);
    background: var(--color-bg-primary);
    border: none;
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-primary);
    line-height: 1.5;
    resize: none;
  }

  .code-editor:focus {
    outline: none;
  }

  .code-placeholder {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--space-md);
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-muted);
    padding: var(--space-lg);
    text-align: center;
  }

  .spinner {
    width: 24px;
    height: 24px;
    border: 2px solid var(--color-border);
    border-top-color: var(--color-accent-cyan);
    border-radius: 50%;
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  /* Output Box */
  .output-box {
    padding: var(--space-md);
    max-height: 200px;
    overflow: auto;
  }

  .output-box pre {
    font-family: var(--font-mono);
    font-size: 11px;
    line-height: 1.5;
    margin: 0;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .output-box.success {
    background: rgba(0, 255, 136, 0.05);
  }

  .output-box.success pre {
    color: var(--color-success);
  }

  .output-box.error {
    background: rgba(255, 77, 77, 0.05);
  }

  .output-box.error pre {
    color: var(--color-error);
  }

  /* Badges */
  .success-badge {
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 600;
    color: var(--color-success);
    letter-spacing: 0.5px;
  }

  .error-badge {
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 600;
    color: var(--color-error);
    letter-spacing: 0.5px;
  }

  /* Modal Footer */
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

  /* Buttons */
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

  .action-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .action-btn.primary {
    background: var(--color-accent-cyan);
    border-color: var(--color-accent-cyan);
    color: var(--color-bg-primary);
  }

  .action-btn.primary:hover:not(:disabled) {
    opacity: 0.9;
  }
</style>
