<script lang="ts">
  import { invoke } from "$lib/tauri";

  interface Props {
    filePreview: string[];
    currentCode: string;
    selectedCode: string | null;
    onApplyCode: (code: string) => void;
  }

  let { filePreview, currentCode, selectedCode, onApplyCode }: Props = $props();

  interface Message {
    role: "user" | "assistant";
    content: string;
    codeBlocks: string[];
  }

  // State
  let messages = $state<Message[]>([]);
  let inputValue = $state("");
  let isLoading = $state(false);
  let isAnalyzing = $state(false);
  let error = $state<string | null>(null);
  let chatContainer: HTMLDivElement | undefined = $state();

  // Extract code blocks from markdown response
  function extractCodeBlocks(text: string): string[] {
    const regex = /```(?:python)?\n([\s\S]*?)```/g;
    const blocks: string[] = [];
    let match;
    while ((match = regex.exec(text)) !== null) {
      blocks.push(match[1].trim());
    }
    return blocks;
  }

  async function sendMessage() {
    if (!inputValue.trim() || isLoading) return;

    const userMessage = inputValue.trim();
    inputValue = "";
    error = null;

    // Add user message (include selection context indicator)
    const displayMessage = selectedCode
      ? `[Selected code: ${selectedCode.split('\n').length} lines]\n${userMessage}`
      : userMessage;
    messages = [
      ...messages,
      { role: "user", content: displayMessage, codeBlocks: [] },
    ];

    isLoading = true;

    try {
      // Build context with selection if present
      let contextMessage = userMessage;
      if (selectedCode) {
        contextMessage = `The user has selected this code:\n\`\`\`python\n${selectedCode}\n\`\`\`\n\nUser request: ${userMessage}`;
      }

      // Call Tauri backend for chat
      const response = await invoke<string>("parser_lab_chat", {
        filePreview: filePreview.slice(0, 20).join("\n"),
        currentCode: currentCode || "",
        userMessage: contextMessage,
      });

      const codeBlocks = extractCodeBlocks(response);

      messages = [
        ...messages,
        { role: "assistant", content: response, codeBlocks },
      ];

      // Auto-apply first code block
      if (codeBlocks.length > 0) {
        onApplyCode(codeBlocks[0]);
      }
    } catch (e) {
      error = String(e);
      console.error("Chat error:", e);
    } finally {
      isLoading = false;
      scrollToBottom();
    }
  }

  function scrollToBottom() {
    if (chatContainer) {
      setTimeout(() => {
        chatContainer!.scrollTop = chatContainer!.scrollHeight;
      }, 50);
    }
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      sendMessage();
    }
  }

  function applyCode(code: string) {
    onApplyCode(code);
  }

  async function analyzeStructure() {
    if (isAnalyzing || filePreview.length === 0) return;

    isAnalyzing = true;
    error = null;

    // Add user message indicating structure analysis
    messages = [
      ...messages,
      { role: "user", content: "Analyze the structure of this file and suggest how to parse it.", codeBlocks: [] },
    ];

    try {
      const analysisPrompt = `Analyze this file and decide: should it be DEMUXED into multiple outputs or parsed as a single table?

File preview:
\`\`\`
${filePreview.slice(0, 30).join("\n")}
\`\`\`

## Decision Criteria

DEMUX (multi-output) if the file has:
- Header/detail pattern (invoice header + line items)
- Multiple record types with different schemas
- Sections that would go to different database tables
- Parent-child relationships

SINGLE output if:
- All rows have the same schema
- It's a simple CSV/JSON with uniform structure

## Your Response

1. State your decision: SINGLE or DEMUX
2. If DEMUX, list the logical sections you identified
3. Generate the appropriate parser:

SINGLE:
\`\`\`python
def parse(input_path: str) -> pl.DataFrame:
    return df
\`\`\`

DEMUX:
\`\`\`python
def parse(input_path: str) -> dict[str, pl.DataFrame]:
    return {
        "topic_name": df,  # lowercase, underscores only
    }
\`\`\`

Topic names must be: lowercase, alphanumeric + underscore, start with letter.
Good: header, line_items, order_totals
Bad: "Line Items", "Header!", lineItems`;

      const response = await invoke<string>("parser_lab_chat", {
        filePreview: filePreview.slice(0, 30).join("\n"),
        currentCode: currentCode || "",
        userMessage: analysisPrompt,
      });

      const codeBlocks = extractCodeBlocks(response);

      messages = [
        ...messages,
        { role: "assistant", content: response, codeBlocks },
      ];

      // Auto-apply first code block
      if (codeBlocks.length > 0) {
        onApplyCode(codeBlocks[0]);
      }
    } catch (e) {
      error = String(e);
      console.error("Structure analysis error:", e);
    } finally {
      isAnalyzing = false;
      scrollToBottom();
    }
  }

  // Format message content (remove code blocks for display, show separately)
  function formatContent(content: string): string {
    return content.replace(/```(?:python)?\n[\s\S]*?```/g, "").trim();
  }

  // Start with a welcome message
  function initializeChat() {
    if (messages.length === 0 && filePreview.length > 0) {
      const previewSample = filePreview.slice(0, 5).join("\n");
      messages = [
        {
          role: "assistant",
          content: `I can help you write a parser for this data. Here's what I see:\n\n\`\`\`\n${previewSample}\n\`\`\`\n\nDescribe how you want to parse this data, or ask me to generate a parser.`,
          codeBlocks: [],
        },
      ];
    }
  }

  $effect(() => {
    if (filePreview.length > 0 && messages.length === 0) {
      initializeChat();
    }
  });
</script>

<div class="parser-chat">
  <div class="chat-header">
    <span>AI Assistant</span>
    <div class="header-actions">
      {#if filePreview.length > 0}
        <button
          class="analyze-btn"
          onclick={analyzeStructure}
          disabled={isAnalyzing || isLoading}
          title="Analyze file structure for multi-table parsing"
        >
          {isAnalyzing ? "Analyzing..." : "Analyze Structure"}
        </button>
      {/if}
      {#if selectedCode}
        <span class="selection-badge">
          {selectedCode.split('\n').length} lines
        </span>
      {/if}
    </div>
  </div>

  <div class="chat-messages" bind:this={chatContainer}>
    {#if messages.length === 0}
      <div class="empty-state">
        <p>Describe your data and I'll help you write a parser.</p>
        <p class="hint">Example: "Parse this CSV and convert the date column to datetime"</p>
      </div>
    {:else}
      {#each messages as message}
        <div class="message {message.role}">
          <div class="message-content">
            {#if formatContent(message.content)}
              <p>{formatContent(message.content)}</p>
            {/if}
            {#each message.codeBlocks as code}
              <div class="code-block">
                <pre>{code}</pre>
                <button class="apply-btn" onclick={() => applyCode(code)}>
                  Apply to Editor
                </button>
              </div>
            {/each}
          </div>
        </div>
      {/each}
      {#if isLoading}
        <div class="message assistant">
          <div class="message-content loading">
            <span class="dot"></span>
            <span class="dot"></span>
            <span class="dot"></span>
          </div>
        </div>
      {/if}
    {/if}
  </div>

  {#if error}
    <div class="error">{error}</div>
  {/if}

  <div class="chat-input">
    <textarea
      bind:value={inputValue}
      placeholder="Describe what you want to parse..."
      onkeydown={handleKeyDown}
      disabled={isLoading}
      rows="2"
    ></textarea>
    <button class="send-btn" onclick={sendMessage} disabled={isLoading || !inputValue.trim()}>
      Send
    </button>
  </div>
</div>

<style>
  .parser-chat {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--color-bg-primary);
    border-left: 1px solid var(--color-border);
  }

  .chat-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.5rem 1rem;
    background: var(--color-bg-card);
    border-bottom: 1px solid var(--color-border);
    font-size: 0.75rem;
    font-weight: 600;
    color: var(--color-text-primary);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .analyze-btn {
    padding: 0.25rem 0.5rem;
    background: linear-gradient(135deg, var(--color-accent-cyan), var(--color-accent-magenta));
    border: none;
    border-radius: var(--radius-sm);
    color: white;
    font-size: 0.65rem;
    font-weight: 500;
    cursor: pointer;
    text-transform: none;
    letter-spacing: normal;
    transition: var(--transition-fast);
  }

  .analyze-btn:hover:not(:disabled) {
    box-shadow: 0 0 10px rgba(0, 212, 255, 0.4);
  }

  .analyze-btn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .selection-badge {
    padding: 0.25rem 0.5rem;
    background: var(--color-accent-cyan);
    color: var(--color-bg-primary);
    border-radius: var(--radius-sm);
    font-size: 0.65rem;
    font-weight: 500;
    text-transform: none;
    letter-spacing: normal;
  }

  .chat-messages {
    flex: 1;
    overflow-y: auto;
    padding: 1rem;
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .empty-state {
    text-align: center;
    padding: 2rem 1rem;
    color: var(--color-text-secondary);
  }

  .empty-state p {
    margin: 0 0 0.5rem 0;
    font-size: 0.9rem;
  }

  .empty-state .hint {
    font-size: 0.8rem;
    color: var(--color-text-muted);
    font-style: italic;
  }

  .message {
    display: flex;
  }

  .message.user {
    justify-content: flex-end;
  }

  .message.assistant {
    justify-content: flex-start;
  }

  .message-content {
    max-width: 100%;
    padding: 0.75rem 1rem;
    border-radius: var(--radius-md);
    font-size: 0.85rem;
    line-height: 1.5;
    word-break: break-word;
    overflow-wrap: break-word;
  }

  .message.user .message-content {
    background: var(--color-accent-cyan);
    color: var(--color-bg-primary);
  }

  .message.assistant .message-content {
    background: var(--color-bg-secondary);
    color: var(--color-text-primary);
    border: 1px solid var(--color-border);
  }

  .message-content p {
    margin: 0 0 0.5rem 0;
    white-space: pre-wrap;
  }

  .message-content p:last-child {
    margin-bottom: 0;
  }

  .code-block {
    margin-top: 0.75rem;
    background: var(--color-bg-primary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }

  .code-block pre {
    margin: 0;
    padding: 0.75rem;
    font-family: var(--font-mono);
    font-size: 0.75rem;
    line-height: 1.5;
    overflow-x: auto;
    overflow-y: auto;
    color: var(--color-text-primary);
    max-height: 300px;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .apply-btn {
    display: block;
    width: 100%;
    padding: 0.5rem;
    background: var(--color-bg-tertiary);
    border: none;
    border-top: 1px solid var(--color-border);
    color: var(--color-accent-cyan);
    font-size: 0.75rem;
    font-weight: 500;
    cursor: pointer;
    transition: var(--transition-fast);
  }

  .apply-btn:hover {
    background: var(--color-bg-card);
  }

  .loading {
    display: flex;
    gap: 0.25rem;
    padding: 0.75rem 1rem;
  }

  .dot {
    width: 6px;
    height: 6px;
    background: var(--color-text-muted);
    border-radius: 50%;
    animation: bounce 1.4s infinite ease-in-out both;
  }

  .dot:nth-child(1) {
    animation-delay: -0.32s;
  }

  .dot:nth-child(2) {
    animation-delay: -0.16s;
  }

  @keyframes bounce {
    0%, 80%, 100% {
      transform: scale(0);
    }
    40% {
      transform: scale(1);
    }
  }

  .error {
    padding: 0.5rem 1rem;
    background: rgba(255, 51, 85, 0.15);
    color: var(--color-error);
    font-size: 0.8rem;
    border-top: 1px solid var(--color-error);
  }

  .chat-input {
    display: flex;
    gap: 0.5rem;
    padding: 0.75rem;
    background: var(--color-bg-secondary);
    border-top: 1px solid var(--color-border);
  }

  .chat-input textarea {
    flex: 1;
    padding: 0.5rem 0.75rem;
    background: var(--color-bg-tertiary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    color: var(--color-text-primary);
    font-family: var(--font-sans);
    font-size: 0.85rem;
    resize: none;
  }

  .chat-input textarea:focus {
    outline: none;
    border-color: var(--color-accent-cyan);
  }

  .chat-input textarea::placeholder {
    color: var(--color-text-muted);
  }

  .send-btn {
    padding: 0.5rem 1rem;
    background: var(--color-accent-cyan);
    border: none;
    border-radius: var(--radius-sm);
    color: var(--color-bg-primary);
    font-size: 0.8rem;
    font-weight: 500;
    cursor: pointer;
    transition: var(--transition-fast);
  }

  .send-btn:hover:not(:disabled) {
    box-shadow: var(--glow-cyan);
  }

  .send-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
