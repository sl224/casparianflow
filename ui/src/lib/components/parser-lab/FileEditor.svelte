<script lang="ts">
  import { invoke } from "$lib/tauri";
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import SinkConfig from "./SinkConfig.svelte";
  import ParserChat from "./ParserChat.svelte";
  import ParserEditor from "./ParserEditor.svelte";

  interface Props {
    parserId: string;
    onBack: () => void;
  }

  let { parserId, onBack }: Props = $props();

  // Types matching Rust backend (v6 - parser-centric, v7 - multi-output)
  interface ParserLabParser {
    id: string;
    name: string;
    filePattern: string;
    patternType: string;
    sourceCode: string | null;
    validationStatus: string;
    validationError: string | null;
    validationOutput: string | null;
    lastValidatedAt: number | null;
    messagesJson: string | null;
    schemaJson: string | null;
    sinkType: string;
    sinkConfigJson: string | null;
    publishedAt: number | null;
    publishedPluginId: number | null;
    isSample: boolean;
    outputMode: string;                  // "single" | "multi"
    detectedTopicsJson: string | null;   // JSON array of topic names
    createdAt: number;
    updatedAt: number;
  }

  // Parsed table output for multi-output display
  interface TableOutput {
    name: string;
    rowCount: number;
    content: string;
    collapsed: boolean;
  }

  // Structured sink config per topic (NEW: supports type selection)
  interface TopicSink {
    type: "parquet" | "sqlite" | "csv";
    path: string;
    config: Record<string, unknown>;  // Type-specific options (compression, tableName, etc.)
  }

  interface TopicSinkConfig {
    [topic: string]: TopicSink;
  }

  interface ParserLabTestFile {
    id: string;
    parserId: string;
    filePath: string;
    fileName: string;
    fileSize: number | null;
    createdAt: number;
  }

  // State
  let parser = $state<ParserLabParser | null>(null);
  let testFiles = $state<ParserLabTestFile[]>([]);
  let isLoading = $state(true);
  let isSaving = $state(false);
  let saveSuccess = $state(false);
  let isValidating = $state(false);
  let isDeploying = $state(false);
  let deployMessage = $state<string | null>(null);
  let error = $state<string | null>(null);
  let hasUnsavedChanges = $state(false);

  // Editor state
  let sourceCode = $state("");
  let filePattern = $state("");
  let patternType = $state("all");
  let sinkType = $state("parquet");
  let sinkConfigJson = $state<string | null>(null);
  let parserName = $state("");

  // Data preview state
  let selectedTestFileId = $state<string | null>(null);
  let dataPreview = $state<string[]>([]);
  let isLoadingPreview = $state(false);

  // Validation output
  let validationOutput = $state<string | null>(null);
  let validationError = $state<string | null>(null);
  let tableOutputs = $state<TableOutput[]>([]);
  let isMultiOutput = $state(false);
  let hasBeenValidated = $state(false);  // Track if validation has ever run

  // Track if parser is deployable (validation passed)
  // Using a derived value for more stable reactivity
  let isDeployable = $derived(parser?.validationStatus === "valid");

  // Multi-sink configuration
  let topicSinks = $state<TopicSinkConfig>({});
  let usePatternForAll = $state(false);
  let sinkUriPattern = $state("parquet://~/.casparian_flow/output/{topic}/");

  // Tab state for code panel
  let activeCodePanelTab = $state<"code" | "output">("code");
  let expandedTopics = $state<Set<string>>(new Set());

  // Subscription tag state
  let subscriptionTag = $state("");
  let pluginVersion = $state("1.0.0");
  let tagValidationStatus = $state<"idle" | "checking" | "valid" | "invalid" | "exists">("idle");
  let tagValidationMessage = $state<string | null>(null);

  // AI integration state
  let selectedCode = $state<string | null>(null);
  let pendingDiff = $state<string | null>(null);
  let isGenerating = $state(false);

  onMount(async () => {
    await loadParser();
  });

  async function loadParser() {
    isLoading = true;
    error = null;
    try {
      const [p, files] = await Promise.all([
        invoke<ParserLabParser | null>("parser_lab_get_parser", { parserId }),
        invoke<ParserLabTestFile[]>("parser_lab_list_test_files", { parserId }),
      ]);

      parser = p;
      testFiles = files;

      if (parser) {
        parserName = parser.name;
        // Don't auto-populate default code - let user write or use AI
        sourceCode = parser.sourceCode || "";
        filePattern = parser.filePattern || "";
        patternType = parser.patternType || "all";
        sinkType = parser.sinkType || "parquet";
        sinkConfigJson = parser.sinkConfigJson;
        validationOutput = parser.validationOutput;
        validationError = parser.validationError;

        // Track if parser was previously validated
        hasBeenValidated = parser.validationStatus === "valid" || parser.validationStatus === "invalid";

        // Restore multi-output state
        isMultiOutput = parser.outputMode === "multi";
        if (isMultiOutput && parser.validationOutput) {
          tableOutputs = parseMultiOutput(parser.validationOutput);
        }

        // Initialize subscription tag from pattern or name
        subscriptionTag = parser.filePattern || parser.name;
        validateSubscriptionTag(subscriptionTag);

        // Restore multi-sink config if this is a multi-output parser
        if (isMultiOutput && parser.sinkConfigJson) {
          try {
            const config = JSON.parse(parser.sinkConfigJson);
            if (config.mode === "multi" && config.topicSinks) {
              // New structured format
              topicSinks = config.topicSinks;
              sinkUriPattern = config.pattern || sinkUriPattern;
            } else if (config.mode === "multi" && config.topicUris) {
              // Migrate old URI-only format to structured format
              const migratedSinks: TopicSinkConfig = {};
              for (const [topic, uri] of Object.entries(config.topicUris as Record<string, string>)) {
                const sinkType = uri.startsWith("sqlite://") ? "sqlite" :
                                 uri.startsWith("csv://") ? "csv" : "parquet";
                const path = uri.replace(/^(parquet|sqlite|csv):\/\//, "");
                migratedSinks[topic] = { type: sinkType, path, config: {} };
              }
              topicSinks = migratedSinks;
              sinkUriPattern = config.pattern || sinkUriPattern;
            }
          } catch {
            // Ignore parse errors
          }
        }
      }

      if (files.length > 0) {
        selectedTestFileId = files[0].id;
        await loadDataPreview();
      }
    } catch (e) {
      error = String(e);
      console.error("Failed to load parser:", e);
    } finally {
      isLoading = false;
    }
  }

  async function loadDataPreview() {
    if (!selectedTestFileId) return;

    const file = testFiles.find((f) => f.id === selectedTestFileId);
    if (!file) return;

    isLoadingPreview = true;
    try {
      const preview = await invoke<string[]>("preview_shard", {
        path: file.filePath,
        numLines: 30,
      });
      dataPreview = preview;
    } catch (e) {
      console.error("Failed to load preview:", e);
      dataPreview = [`Error loading preview: ${e}`];
    } finally {
      isLoadingPreview = false;
    }
  }

  async function saveParser() {
    if (!parser) return;

    isSaving = true;
    error = null;
    try {
      // For multi-output parsers, store structured topic sinks in sinkConfigJson
      let finalSinkConfigJson = sinkConfigJson;
      if (isMultiOutput && Object.keys(topicSinks).length > 0) {
        finalSinkConfigJson = JSON.stringify({
          mode: "multi",
          topicSinks: topicSinks,
          pattern: sinkUriPattern,
        });
      }

      const updatedParser = {
        ...parser,
        name: parserName,
        sourceCode,
        filePattern,
        patternType,
        sinkType,
        sinkConfigJson: finalSinkConfigJson,
      };
      await invoke("parser_lab_update_parser", { parser: updatedParser });

      // Verify save succeeded by re-fetching
      const verified = await invoke<ParserLabParser | null>("parser_lab_get_parser", { parserId });
      if (!verified) {
        error = "Save failed - parser not found after save. Try restarting the app.";
        return;
      }
      if (verified.sourceCode !== sourceCode) {
        error = "Save failed - code not persisted. Database may need reset.";
        return;
      }

      hasUnsavedChanges = false;
      parser = verified;

      // Show success feedback for 2 seconds
      saveSuccess = true;
      setTimeout(() => {
        saveSuccess = false;
      }, 2000);
    } catch (e) {
      error = String(e);
      console.error("Failed to save parser:", e);
    } finally {
      isSaving = false;
    }
  }

  // Debounce timer for tag validation
  let tagValidationTimer: ReturnType<typeof setTimeout> | null = null;

  async function validateSubscriptionTag(tag: string) {
    // Clear previous timer
    if (tagValidationTimer) {
      clearTimeout(tagValidationTimer);
    }

    // Basic format validation
    if (!tag || tag.trim() === "") {
      tagValidationStatus = "invalid";
      tagValidationMessage = "Tag cannot be empty";
      return;
    }

    // Check for invalid characters
    const validTagRegex = /^[a-zA-Z0-9_\-\.]+$/;
    if (!validTagRegex.test(tag)) {
      tagValidationStatus = "invalid";
      tagValidationMessage = "Tag can only contain letters, numbers, underscores, hyphens, and dots";
      return;
    }

    // Debounce the backend check
    tagValidationStatus = "checking";
    tagValidationMessage = null;

    tagValidationTimer = setTimeout(async () => {
      try {
        const result = await invoke<{ valid: boolean; exists: boolean; existingPluginName?: string }>(
          "validate_subscription_tag",
          { tag, currentParserId: parserId }
        );

        if (result.exists) {
          tagValidationStatus = "exists";
          tagValidationMessage = `Tag already used by plugin: ${result.existingPluginName}`;
        } else {
          tagValidationStatus = "valid";
          tagValidationMessage = null;
        }
      } catch (e) {
        // Backend might not have the command yet - treat as valid
        console.warn("Tag validation not available:", e);
        tagValidationStatus = "valid";
        tagValidationMessage = null;
      }
    }, 300);
  }

  function handleTagChange(e: Event) {
    const input = e.target as HTMLInputElement;
    subscriptionTag = input.value;
    hasUnsavedChanges = true;
    validateSubscriptionTag(subscriptionTag);
  }

  async function validateParser() {
    if (!selectedTestFileId) {
      error = "Add a test file first";
      return;
    }

    // Clear previous results
    validationOutput = null;
    validationError = null;

    // Save first
    await saveParser();

    // Check if save failed
    if (error) {
      return;
    }

    isValidating = true;
    error = null;
    try {
      const result = await invoke<ParserLabParser>("parser_lab_validate_parser", {
        parserId,
        testFileId: selectedTestFileId,
      });

      hasBeenValidated = true;  // Mark as validated

      if (result.validationStatus === "valid") {
        validationOutput = result.validationOutput;
        validationError = null;

        // Auto-switch to Output tab to show sink configuration
        activeCodePanelTab = "output";

        // Check if this is a multi-output parser
        isMultiOutput = result.outputMode === "multi";
        if (isMultiOutput && result.validationOutput) {
          tableOutputs = parseMultiOutput(result.validationOutput);

          // Initialize structured topic sinks if not already set
          if (result.detectedTopicsJson) {
            const topics: string[] = JSON.parse(result.detectedTopicsJson);
            const newTopicSinks: TopicSinkConfig = {};
            for (const topic of topics) {
              // Use existing config or create default
              newTopicSinks[topic] = topicSinks[topic] || {
                type: "parquet",
                path: `~/.casparian_flow/output/${topic}/`,
                config: { compression: "snappy" }
              };
            }
            topicSinks = newTopicSinks;
            // Auto-expand first topic
            if (topics.length > 0) {
              expandedTopics = new Set([topics[0]]);
            }
          }
        } else {
          tableOutputs = [];
        }
      } else {
        validationError = result.validationError || "Validation failed";
        validationOutput = null;
        isMultiOutput = false;
        tableOutputs = [];
      }
      parser = result;
    } catch (e) {
      validationError = String(e);
      console.error("Failed to validate parser:", e);
    } finally {
      isValidating = false;
    }
  }

  // Deploy as Plugin
  interface ParserPublishReceipt {
    success: boolean;
    pluginName: string;
    parserFilePath: string;
    manifestId: number | null;
    configId: number | null;
    topicConfigId: number | null;
    message: string;
  }

  async function deployAsPlugin() {
    if (!parser?.sourceCode) {
      error = "No code to deploy";
      return;
    }

    if (parser.validationStatus !== "valid") {
      error = "Parser must pass validation before deploying";
      return;
    }

    isDeploying = true;
    deployMessage = null;
    error = null;

    try {
      // Save first
      await saveParser();
      if (error) return;

      // Get sink config output path (for single-output)
      let outputPath = "";
      if (sinkConfigJson && !isMultiOutput) {
        try {
          const config = JSON.parse(sinkConfigJson);
          outputPath = config.outputDir || config.outputPath || "";
        } catch {
          // Ignore parse errors
        }
      }

      // For multi-output parsers, pass structured topic config
      // Backend expects: { topic: { type, uri, config } }
      let topicUrisJson: string | null = null;
      if (isMultiOutput && Object.keys(topicSinks).length > 0) {
        const topicConfig: Record<string, { type: string; uri: string; config: Record<string, unknown> }> = {};
        for (const [topic, sink] of Object.entries(topicSinks)) {
          topicConfig[topic] = {
            type: sink.type,
            uri: `${sink.type}://${sink.path}`,
            config: sink.config
          };
        }
        topicUrisJson = JSON.stringify(topicConfig);
      }

      const receipt = await invoke<ParserPublishReceipt>("publish_parser", {
        parserKey: subscriptionTag || parser.filePattern || parser.name,
        sourceCode: parser.sourceCode,
        schema: [], // TODO: extract from validation output
        sinkType,
        outputPath,
        outputMode: isMultiOutput ? "multi" : "single",
        topicUrisJson,
        version: pluginVersion || "1.0.0",
      });

      if (receipt.success) {
        const topicCount = isMultiOutput ? Object.keys(topicSinks).length : 1;
        deployMessage = `Deployed as plugin: ${receipt.pluginName} (${topicCount} topic${topicCount > 1 ? 's' : ''})`;
      } else {
        error = receipt.message || "Deploy failed";
      }
    } catch (e) {
      error = String(e);
      console.error("Failed to deploy parser:", e);
    } finally {
      isDeploying = false;
    }
  }

  async function addTestFile() {
    try {
      // Get parsers dir as default path
      let defaultPath: string | undefined;
      try {
        defaultPath = await invoke<string>("get_parsers_dir");
      } catch {
        // Ignore - will use system default
      }

      const selected = await open({
        multiple: false,
        title: "Select test file",
        defaultPath,
      });

      if (selected) {
        await invoke("parser_lab_add_test_file", {
          parserId,
          filePath: selected,
        });
        await loadParser();
      }
    } catch (e) {
      error = String(e);
      console.error("Failed to add test file:", e);
    }
  }

  async function removeTestFile(id: string, e: Event) {
    e.stopPropagation();
    try {
      await invoke("parser_lab_remove_test_file", { testFileId: id });
      await loadParser();
    } catch (e) {
      error = String(e);
    }
  }

  function handleCodeChange(newCode: string) {
    sourceCode = newCode;
    hasUnsavedChanges = true;
  }

  function handleSelectionChange(selection: string | null) {
    selectedCode = selection;
  }

  function handleKeyDown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
      e.preventDefault();
      validateParser();
    }
    if ((e.ctrlKey || e.metaKey) && e.key === "s") {
      e.preventDefault();
      saveParser();
    }
  }

  function handleSinkConfigChange(config: string) {
    sinkConfigJson = config;
    hasUnsavedChanges = true;
  }

  // AI Integration: Handle code suggestion from chat
  function handleCodeSuggestion(suggestedCode: string) {
    if (!sourceCode.trim()) {
      // Editor empty - auto-apply
      sourceCode = suggestedCode;
      hasUnsavedChanges = true;
    } else {
      // Editor has code - show diff
      pendingDiff = suggestedCode;
    }
  }

  function acceptDiff() {
    if (pendingDiff) {
      sourceCode = pendingDiff;
      hasUnsavedChanges = true;
      pendingDiff = null;
    }
  }

  function rejectDiff() {
    pendingDiff = null;
  }

  // Generate parser with AI
  // Extract code blocks from markdown (handles various Claude response formats)
  function extractCodeBlocks(text: string): string[] {
    // Multiple patterns to try
    const patterns = [
      /```python\s*([\s\S]*?)```/g,           // ```python ... ```
      /```py\s*([\s\S]*?)```/g,               // ```py ... ```
      /```\s*([\s\S]*?)```/g,                 // ``` ... ``` (generic)
    ];

    const blocks: string[] = [];

    for (const regex of patterns) {
      let match;
      while ((match = regex.exec(text)) !== null) {
        const code = match[1].trim();
        // Only include if it looks like Python code (has import or def)
        if (code && (code.includes('import ') || code.includes('def ') || code.includes('pl.'))) {
          blocks.push(code);
        }
      }
      if (blocks.length > 0) break; // Stop at first pattern that finds code
    }

    // If no code blocks found, try to extract bare Python code
    if (blocks.length === 0) {
      // Look for code that starts with import
      const importMatch = text.match(/^(import\s+[\s\S]*?)(?:\n\n|$)/m);
      if (importMatch) {
        blocks.push(importMatch[1].trim());
      }
    }

    return blocks;
  }

  async function generateParser() {
    if (isGenerating) return;

    isGenerating = true;
    error = null;

    try {
      console.log("Calling parser_lab_chat with preview:", dataPreview.slice(0, 5));

      const response = await invoke<string>("parser_lab_chat", {
        filePreview: dataPreview.slice(0, 20).join("\n"),
        currentCode: "",
        userMessage: "Generate a parser for this file. Return a complete Python function using Polars that parses the file and returns a DataFrame.",
      });

      console.log("AI Response received, length:", response?.length);
      console.log("AI Response preview:", response?.slice(0, 500));

      if (!response || response.trim().length === 0) {
        error = "AI returned empty response. Check if Claude CLI is working.";
        return;
      }

      // Extract code blocks using robust extraction
      const codeBlocks = extractCodeBlocks(response);
      console.log("Extracted code blocks:", codeBlocks.length);

      if (codeBlocks.length > 0) {
        const newCode = codeBlocks[0];
        console.log("SUCCESS: Extracted code block, length:", newCode.length);
        // Use handleCodeSuggestion - same path as chat (proven to work)
        handleCodeSuggestion(newCode);
      } else {
        // Show full response for debugging
        console.error("FAILED: No code block extracted");
        console.error("Full response:", response);
        console.error("Response length:", response.length);

        // Check if response has backticks at all
        const hasBackticks = response.includes('```');
        console.error("Has backticks:", hasBackticks);

        // Show in UI
        error = `No code found in AI response (${response.length} chars). Check browser console for details.`;
      }
    } catch (e) {
      error = `AI error: ${String(e)}`;
      console.error("Generate parser error:", e);
    } finally {
      isGenerating = false;
    }
  }

  function updateTopicSink(topic: string, updates: Partial<TopicSink>) {
    const current = topicSinks[topic] || { type: "parquet", path: "", config: {} };
    topicSinks = { ...topicSinks, [topic]: { ...current, ...updates } };
    hasUnsavedChanges = true;
  }

  function applyPatternToAll() {
    const newTopicSinks: TopicSinkConfig = {};
    // Extract type from pattern (e.g., "parquet://..." -> "parquet")
    const patternType = sinkUriPattern.startsWith("sqlite://") ? "sqlite" :
                        sinkUriPattern.startsWith("csv://") ? "csv" : "parquet";
    const patternPath = sinkUriPattern.replace(/^(parquet|sqlite|csv):\/\//, "");

    for (const topic of Object.keys(topicSinks)) {
      const path = patternPath.replace("{topic}", topic);
      newTopicSinks[topic] = {
        type: patternType,
        path,
        config: topicSinks[topic]?.config || {}
      };
    }
    topicSinks = newTopicSinks;
    hasUnsavedChanges = true;
  }

  function handlePatternChange(pattern: string) {
    sinkUriPattern = pattern;
    if (usePatternForAll) {
      applyPatternToAll();
    }
  }

  function toggleTopicExpanded(topic: string) {
    const newSet = new Set(expandedTopics);
    if (newSet.has(topic)) {
      newSet.delete(topic);
    } else {
      newSet.add(topic);
    }
    expandedTopics = newSet;
  }

  function formatFileSize(bytes: number | null): string {
    if (bytes === null) return "";
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
    return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} GB`;
  }

  // Parse multi-output validation format
  // Format: "=== topic_name (N rows) ===" followed by table content
  function parseMultiOutput(output: string): TableOutput[] {
    const tables: TableOutput[] = [];
    const pattern = /=== ([^(]+) \((\d+) rows?\) ===/g;
    let match;
    const matches: { name: string; rowCount: number; startIndex: number }[] = [];

    while ((match = pattern.exec(output)) !== null) {
      matches.push({
        name: match[1].trim(),
        rowCount: parseInt(match[2], 10),
        startIndex: match.index + match[0].length,
      });
    }

    // Extract content between headers
    for (let i = 0; i < matches.length; i++) {
      const startIndex = matches[i].startIndex;
      const endIndex = i < matches.length - 1 ? output.indexOf("===", startIndex) : output.length;
      const content = output.slice(startIndex, endIndex).trim();

      tables.push({
        name: matches[i].name,
        rowCount: matches[i].rowCount,
        content,
        collapsed: false, // Start expanded
      });
    }

    return tables;
  }

  function toggleTableCollapse(index: number) {
    tableOutputs = tableOutputs.map((t, i) =>
      i === index ? { ...t, collapsed: !t.collapsed } : t
    );
  }
</script>

<div class="file-editor" onkeydown={handleKeyDown}>
  <!-- Header -->
  <header class="header">
    <button class="btn-back" onclick={onBack}>Back</button>
    <input
      type="text"
      class="name-input"
      bind:value={parserName}
      oninput={() => (hasUnsavedChanges = true)}
      placeholder="Parser name"
    />
    <div class="header-actions">
      {#if hasUnsavedChanges}
        <span class="unsaved">unsaved</span>
      {/if}
      {#if !sourceCode && dataPreview.length > 0}
        <button
          class="btn btn-ai"
          onclick={generateParser}
          disabled={isGenerating}
        >
          {isGenerating ? "Generating..." : "Generate with AI"}
        </button>
      {/if}
      <button
        class="btn"
        class:btn-success={saveSuccess}
        onclick={saveParser}
        disabled={isSaving}
      >
        {#if isSaving}
          Saving...
        {:else if saveSuccess}
          ✓ Saved
        {:else}
          Save
        {/if}
      </button>
      <button
        class="btn btn-primary"
        onclick={validateParser}
        disabled={isValidating || testFiles.length === 0}
      >
        {isValidating ? "Testing..." : "Test"}
      </button>
    </div>
  </header>

  {#if error}
    <div class="error-banner">{error}</div>
  {/if}

  {#if isLoading}
    <div class="loading">Loading...</div>
  {:else}
    <div class="editor-layout">
      <!-- Left: Code Editor with Tabs -->
      <div class="code-panel">
        <!-- Pattern config -->
        <div class="pattern-config">
          <label>
            <span>Pattern:</span>
            <select bind:value={patternType} onchange={() => (hasUnsavedChanges = true)}>
              <option value="all">All files</option>
              <option value="key_column">Key column</option>
              <option value="glob">Glob</option>
            </select>
          </label>
          {#if patternType !== "all"}
            <input
              type="text"
              bind:value={filePattern}
              placeholder={patternType === "key_column" ? "e.g., RFC_DB" : "e.g., *.log"}
              oninput={() => (hasUnsavedChanges = true)}
            />
          {/if}
        </div>

        <!-- Tabs: Code / Output -->
        <div class="code-panel-tabs">
          <button
            class="code-panel-tab"
            class:active={activeCodePanelTab === "code"}
            onclick={() => activeCodePanelTab = "code"}
          >
            Code
          </button>
          <button
            class="code-panel-tab"
            class:active={activeCodePanelTab === "output"}
            onclick={() => activeCodePanelTab = "output"}
          >
            Output
            {#if isMultiOutput}
              <span class="tab-badge">{Object.keys(topicSinks).length}</span>
            {/if}
          </button>
        </div>

        {#if activeCodePanelTab === "code"}
          <!-- Code Tab: Editor -->
          <div class="code-wrapper">
            <div class="code-header">
              <span>Parser Code</span>
              <span class="hint">Ctrl+Enter to test</span>
            </div>

            <ParserEditor
              value={sourceCode}
              onValueChange={handleCodeChange}
              onSelectionChange={handleSelectionChange}
              {pendingDiff}
              onAcceptDiff={acceptDiff}
              onRejectDiff={rejectDiff}
            />
          </div>
        {:else}
          <!-- Output Tab: Sink Configuration -->
          <div class="output-tab-content">
            <div class="topic-list">
              {#if isMultiOutput && Object.keys(topicSinks).length > 0}
                <!-- Multi-output: Expandable rows per topic -->
                <div class="topic-config-header">
                  <span>{Object.keys(topicSinks).length} topics detected</span>
                </div>

                {#each Object.entries(topicSinks) as [topic, sink]}
                  <div class="topic-row" class:expanded={expandedTopics.has(topic)}>
                    <button class="topic-header" onclick={() => toggleTopicExpanded(topic)}>
                      <span class="chevron">{expandedTopics.has(topic) ? "▼" : "▶"}</span>
                      <span class="topic-name">{topic}</span>
                      <span class="sink-type-badge">{sink.type}</span>
                      {#if !expandedTopics.has(topic)}
                        <span class="path-preview">{sink.path}</span>
                      {/if}
                    </button>

                    {#if expandedTopics.has(topic)}
                      <div class="topic-config">
                        <label class="config-row">
                          <span>Sink Type:</span>
                          <select
                            value={sink.type}
                            onchange={(e) => updateTopicSink(topic, { type: e.currentTarget.value as TopicSink["type"] })}
                          >
                            <option value="parquet">Parquet</option>
                            <option value="sqlite">SQLite</option>
                            <option value="csv">CSV</option>
                          </select>
                        </label>

                        <label class="config-row">
                          <span>Path:</span>
                          <input
                            type="text"
                            value={sink.path}
                            oninput={(e) => updateTopicSink(topic, { path: e.currentTarget.value })}
                            placeholder={`~/.casparian_flow/output/${topic}/`}
                          />
                        </label>

                        {#if sink.type === "parquet"}
                          <label class="config-row">
                            <span>Compression:</span>
                            <select
                              value={sink.config.compression || "snappy"}
                              onchange={(e) => updateTopicSink(topic, { config: { ...sink.config, compression: e.currentTarget.value } })}
                            >
                              <option value="snappy">Snappy</option>
                              <option value="gzip">Gzip</option>
                              <option value="lz4">LZ4</option>
                              <option value="none">None</option>
                            </select>
                          </label>
                        {:else if sink.type === "sqlite"}
                          <label class="config-row">
                            <span>Table:</span>
                            <input
                              type="text"
                              value={sink.config.tableName || topic}
                              oninput={(e) => updateTopicSink(topic, { config: { ...sink.config, tableName: e.currentTarget.value } })}
                              placeholder={topic}
                            />
                          </label>
                        {/if}
                      </div>
                    {/if}
                  </div>
                {/each}
              {:else if isMultiOutput}
                <!-- Multi-output parser but topics not yet detected -->
                <div class="waiting-for-topics">
                  <div class="waiting-icon">📊</div>
                  <div class="waiting-title">Multi-Output Parser</div>
                  <div class="waiting-desc">Run test to detect output topics and configure sinks for each.</div>
                </div>
              {:else}
                <!-- Single output: Standard sink config -->
                <div class="single-output-config">
                  <label class="config-row">
                    <span>Sink Type:</span>
                    <select bind:value={sinkType} onchange={() => (hasUnsavedChanges = true)}>
                      <option value="parquet">Parquet</option>
                      <option value="csv">CSV</option>
                      <option value="sqlite">SQLite</option>
                    </select>
                  </label>
                  <SinkConfig
                    {sinkType}
                    configJson={sinkConfigJson}
                    parserName={parserName || "output"}
                    onChange={handleSinkConfigChange}
                  />
                </div>
              {/if}
            </div>

            <!-- Deploy section - always visible at bottom -->
            <div class="deploy-section">
              <!-- Subscription Tag -->
              <div class="tag-config">
                <label class="tag-label">
                  <span>Subscription Tag:</span>
                  <input
                    type="text"
                    class="tag-input"
                    class:tag-valid={tagValidationStatus === "valid"}
                    class:tag-invalid={tagValidationStatus === "invalid" || tagValidationStatus === "exists"}
                    class:tag-checking={tagValidationStatus === "checking"}
                    value={subscriptionTag}
                    oninput={handleTagChange}
                    placeholder="e.g., MCDATA"
                  />
                  {#if tagValidationStatus === "checking"}
                    <span class="tag-status checking">⋯</span>
                  {:else if tagValidationStatus === "valid"}
                    <span class="tag-status valid">✓</span>
                  {:else if tagValidationStatus === "invalid" || tagValidationStatus === "exists"}
                    <span class="tag-status invalid">✗</span>
                  {/if}
                </label>
                {#if tagValidationMessage}
                  <div class="tag-error">{tagValidationMessage}</div>
                {/if}
                <div class="tag-hint">Files tagged with <code>{subscriptionTag}</code> will be processed by this plugin</div>
              </div>

              <!-- Plugin Version -->
              <div class="version-config">
                <label class="version-label">
                  <span>Version:</span>
                  <input
                    type="text"
                    class="version-input"
                    bind:value={pluginVersion}
                    placeholder="1.0.0"
                  />
                </label>
                <div class="version-hint">Semantic version (e.g., 1.0.0, 2.1.3)</div>
              </div>

              {#if isDeployable}
                <button
                  class="btn-deploy"
                  onclick={deployAsPlugin}
                  disabled={isDeploying || tagValidationStatus !== "valid"}
                >
                  {isDeploying ? "Deploying..." : "Deploy as Plugin"}
                </button>
                {#if deployMessage}
                  <div class="deploy-success">{deployMessage}</div>
                {/if}
              {:else}
                <div class="deploy-hint">
                  Run validation to enable deployment (status: {parser?.validationStatus ?? 'no parser'})
                </div>
              {/if}
            </div>
          </div>
        {/if}
      </div>

      <!-- Middle: Data & Output -->
      <div class="data-panel">
        <!-- Test files -->
        <div class="test-files">
          <div class="section-header">
            <span>Test Files</span>
            <button class="btn-sm" onclick={addTestFile}>+ Add</button>
          </div>
          {#if testFiles.length === 0}
            <div class="empty">Add a file to test against</div>
          {:else}
            <div class="file-list">
              {#each testFiles as file}
                <div
                  class="file-item"
                  class:selected={selectedTestFileId === file.id}
                  role="button"
                  tabindex="0"
                  onclick={() => {
                    selectedTestFileId = file.id;
                    loadDataPreview();
                  }}
                  onkeydown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                      selectedTestFileId = file.id;
                      loadDataPreview();
                    }
                  }}
                >
                  <span class="file-name">{file.fileName}</span>
                  <span class="file-size">{formatFileSize(file.fileSize)}</span>
                  <button class="remove-btn" onclick={(e) => removeTestFile(file.id, e)}>x</button>
                </div>
              {/each}
            </div>
          {/if}
        </div>

        <!-- Data preview -->
        <div class="data-preview">
          <div class="section-header">
            <span>Preview</span>
            {#if isLoadingPreview}
              <span class="loading-text">Loading...</span>
            {/if}
          </div>
          <div class="preview-content">
            {#if dataPreview.length === 0}
              <div class="empty">Select a test file</div>
            {:else}
              <pre>{dataPreview.join("\n")}</pre>
            {/if}
          </div>
        </div>

        <!-- Validation output -->
        <div class="validation-output">
          <div class="section-header">
            <span>Output</span>
            {#if isDeployable}
              <span class="badge valid">Valid</span>
              {#if isMultiOutput}
                <span class="badge multi">{tableOutputs.length} tables</span>
              {/if}
            {:else if parser?.validationStatus === "invalid"}
              <span class="badge invalid">Error</span>
            {:else}
              <span class="badge pending">Pending</span>
            {/if}
          </div>
          <div class="output-content">
            {#if validationError}
              <pre class="error-text">{validationError}</pre>
            {:else if isMultiOutput && tableOutputs.length > 0}
              <!-- Multi-output: Collapsible table sections -->
              <div class="multi-output">
                {#each tableOutputs as table, index}
                  <div class="table-section">
                    <button
                      class="table-header"
                      onclick={() => toggleTableCollapse(index)}
                    >
                      <span class="collapse-icon">{table.collapsed ? "▶" : "▼"}</span>
                      <span class="table-name">{table.name}</span>
                      <span class="table-rows">({table.rowCount} rows)</span>
                    </button>
                    {#if !table.collapsed}
                      <pre class="table-content">{table.content}</pre>
                    {/if}
                  </div>
                {/each}
              </div>
            {:else if validationOutput && validationOutput.trim()}
              <pre>{validationOutput}</pre>
            {:else if hasBeenValidated}
              <div class="empty">Parser ran successfully but produced no output</div>
            {:else}
              <div class="empty">Run test to see output</div>
            {/if}
          </div>
        </div>
      </div>

      <!-- Right: AI Chat -->
      <div class="chat-panel">
        <ParserChat
          filePreview={dataPreview}
          currentCode={sourceCode}
          {selectedCode}
          onApplyCode={handleCodeSuggestion}
        />
      </div>
    </div>
  {/if}
</div>

<style>
  .file-editor {
    height: 100%;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    background: var(--color-bg-primary);
    color: var(--color-text-primary);
  }

  .header {
    display: flex;
    align-items: center;
    gap: 1rem;
    padding: 0.75rem 1rem;
    border-bottom: 1px solid var(--color-border);
    background: var(--color-bg-secondary);
  }

  .btn-back {
    padding: 0.25rem 0.75rem;
    background: var(--color-bg-tertiary);
    border: 1px solid var(--color-border);
    color: var(--color-text-secondary);
    cursor: pointer;
    font-size: 0.8rem;
    border-radius: var(--radius-sm);
    transition: var(--transition-fast);
  }

  .btn-back:hover {
    color: var(--color-text-primary);
    border-color: var(--color-accent-cyan);
    background: var(--color-bg-card);
  }

  .name-input {
    flex: 1;
    padding: 0.375rem 0.75rem;
    background: var(--color-bg-tertiary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    color: var(--color-text-primary);
    font-size: 1rem;
    font-weight: 500;
  }

  .name-input:focus {
    outline: none;
    border-color: var(--color-accent-cyan);
    box-shadow: 0 0 0 1px var(--color-accent-cyan);
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .unsaved {
    font-size: 0.75rem;
    color: var(--color-accent-yellow);
    font-style: italic;
  }

  .btn {
    padding: 0.375rem 0.75rem;
    border: 1px solid var(--color-border);
    background: var(--color-bg-tertiary);
    color: var(--color-text-primary);
    cursor: pointer;
    font-size: 0.8rem;
    border-radius: var(--radius-sm);
    transition: var(--transition-fast);
  }

  .btn:hover:not(:disabled) {
    background: var(--color-bg-card);
    border-color: var(--color-border-hover);
  }

  .btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn-primary {
    background: var(--color-accent-cyan);
    border-color: var(--color-accent-cyan);
    color: var(--color-bg-primary);
    font-weight: 500;
  }

  .btn-primary:hover:not(:disabled) {
    box-shadow: var(--glow-cyan);
  }

  .btn-success {
    background: var(--color-success) !important;
    border-color: var(--color-success) !important;
    color: var(--color-bg-primary) !important;
    font-weight: 500;
  }

  .btn-success:hover:not(:disabled) {
    box-shadow: var(--glow-green);
  }

  .btn-ai {
    background: linear-gradient(135deg, var(--color-accent-cyan), var(--color-accent-magenta));
    border: none;
    color: white;
    font-weight: 600;
  }

  .btn-ai:hover:not(:disabled) {
    box-shadow: 0 0 15px rgba(0, 212, 255, 0.5);
  }

  .btn-ai:disabled {
    opacity: 0.7;
    cursor: wait;
  }

  .btn-sm {
    padding: 0.25rem 0.5rem;
    background: var(--color-bg-tertiary);
    border: 1px solid var(--color-border);
    color: var(--color-text-secondary);
    cursor: pointer;
    font-size: 0.75rem;
    border-radius: var(--radius-sm);
    transition: var(--transition-fast);
  }

  .btn-sm:hover {
    background: var(--color-bg-card);
    border-color: var(--color-accent-cyan);
    color: var(--color-accent-cyan);
  }

  .error-banner {
    background: rgba(255, 51, 85, 0.15);
    border-bottom: 1px solid var(--color-error);
    color: var(--color-error);
    padding: 0.5rem 1rem;
    font-size: 0.8rem;
  }

  .loading {
    padding: 2rem;
    text-align: center;
    color: var(--color-text-secondary);
  }

  .editor-layout {
    flex: 1;
    display: grid;
    grid-template-columns: 2fr 1.5fr 1.5fr;
    gap: 1px;
    overflow: hidden;
    background: var(--color-border);
  }

  .code-panel {
    display: flex;
    flex-direction: column;
    /* Allow scrolling so output-config and deploy button aren't truncated */
    overflow: auto;
    background: var(--color-bg-primary);
  }

  .data-panel {
    display: flex;
    flex-direction: column;
    /* Don't clip content - let children handle scrolling */
    overflow: auto;
    background: var(--color-bg-primary);
  }

  .chat-panel {
    display: flex;
    flex-direction: column;
    min-width: 280px;
    background: var(--color-bg-primary);
    /* Allow internal scrolling, don't clip content */
    overflow: visible;
  }

  .pattern-config {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.75rem 1rem;
    border-bottom: 1px solid var(--color-border);
    background: var(--color-bg-secondary);
    font-size: 0.8rem;
  }

  .pattern-config label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .pattern-config span {
    color: var(--color-text-secondary);
    font-weight: 500;
  }

  .pattern-config select,
  .pattern-config input {
    padding: 0.375rem 0.5rem;
    border: 1px solid var(--color-border);
    background: var(--color-bg-tertiary);
    color: var(--color-text-primary);
    font-size: 0.8rem;
    border-radius: var(--radius-sm);
  }

  .pattern-config select:focus,
  .pattern-config input:focus {
    outline: none;
    border-color: var(--color-accent-cyan);
  }

  /* Tab styles */
  .code-panel-tabs {
    display: flex;
    gap: 0;
    border-bottom: 1px solid var(--color-border);
    background: var(--color-bg-card);
  }

  .code-panel-tab {
    padding: 0.5rem 1rem;
    background: transparent;
    border: none;
    border-bottom: 2px solid transparent;
    color: var(--color-text-secondary);
    cursor: pointer;
    font-size: 0.8rem;
    font-weight: 500;
    transition: var(--transition-fast);
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .code-panel-tab:hover {
    color: var(--color-text-primary);
    background: var(--color-bg-tertiary);
  }

  .code-panel-tab.active {
    color: var(--color-accent-cyan);
    border-bottom-color: var(--color-accent-cyan);
  }

  .tab-badge {
    padding: 0.125rem 0.375rem;
    background: var(--color-accent-cyan);
    color: var(--color-bg-primary);
    border-radius: var(--radius-sm);
    font-size: 0.65rem;
    font-weight: 600;
  }

  /* Output tab content */
  .output-tab-content {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .topic-list {
    flex: 1;
    overflow-y: auto;
    padding: 1rem;
  }

  .topic-config-header {
    font-size: 0.75rem;
    color: var(--color-text-secondary);
    margin-bottom: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .topic-row {
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    margin-bottom: 0.5rem;
    overflow: hidden;
    background: var(--color-bg-secondary);
  }

  .topic-row.expanded {
    border-color: var(--color-accent-cyan);
  }

  .topic-header {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.625rem 0.75rem;
    background: transparent;
    border: none;
    color: var(--color-text-primary);
    cursor: pointer;
    font-size: 0.8rem;
    text-align: left;
    transition: var(--transition-fast);
  }

  .topic-header:hover {
    background: var(--color-bg-tertiary);
  }

  .chevron {
    color: var(--color-text-muted);
    font-size: 0.65rem;
    width: 0.75rem;
  }

  .topic-name {
    font-weight: 600;
    color: var(--color-accent-cyan);
    font-family: var(--font-mono);
  }

  .sink-type-badge {
    padding: 0.125rem 0.375rem;
    background: var(--color-bg-tertiary);
    color: var(--color-text-secondary);
    border-radius: var(--radius-sm);
    font-size: 0.65rem;
    font-weight: 500;
    text-transform: uppercase;
  }

  .path-preview {
    flex: 1;
    color: var(--color-text-muted);
    font-size: 0.75rem;
    font-family: var(--font-mono);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: right;
  }

  .topic-config {
    padding: 0.75rem;
    background: var(--color-bg-primary);
    border-top: 1px solid var(--color-border);
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .config-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.8rem;
  }

  .config-row > span {
    min-width: 80px;
    color: var(--color-text-secondary);
  }

  .config-row select,
  .config-row input {
    flex: 1;
    padding: 0.375rem 0.5rem;
    border: 1px solid var(--color-border);
    background: var(--color-bg-tertiary);
    color: var(--color-text-primary);
    font-size: 0.8rem;
    border-radius: var(--radius-sm);
  }

  .config-row select:focus,
  .config-row input:focus {
    outline: none;
    border-color: var(--color-accent-cyan);
  }

  .single-output-config {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .deploy-section {
    padding: 1rem;
    border-top: 1px solid var(--color-border);
    background: var(--color-bg-secondary);
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .tag-config {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .tag-label {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    font-size: 0.8rem;
  }

  .tag-label > span:first-child {
    color: var(--color-text-secondary);
    min-width: 100px;
  }

  .tag-input {
    flex: 1;
    padding: 0.375rem 0.5rem;
    border: 1px solid var(--color-border);
    background: var(--color-bg-tertiary);
    color: var(--color-text-primary);
    font-size: 0.8rem;
    font-family: var(--font-mono);
    border-radius: var(--radius-sm);
    max-width: 200px;
  }

  .tag-input:focus {
    outline: none;
    border-color: var(--color-accent-cyan);
  }

  .tag-input.tag-valid {
    border-color: var(--color-success);
  }

  .tag-input.tag-invalid {
    border-color: var(--color-error);
  }

  .tag-input.tag-checking {
    border-color: var(--color-warning);
  }

  .tag-status {
    font-size: 0.9rem;
    margin-left: 0.25rem;
  }

  .tag-status.valid {
    color: var(--color-success);
  }

  .tag-status.invalid {
    color: var(--color-error);
  }

  .tag-status.checking {
    color: var(--color-warning);
  }

  .tag-error {
    color: var(--color-error);
    font-size: 0.75rem;
    margin-left: 100px;
  }

  .tag-hint {
    color: var(--color-text-muted);
    font-size: 0.7rem;
    margin-left: 100px;
  }

  .tag-hint code {
    background: var(--color-bg-tertiary);
    padding: 0.1rem 0.3rem;
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
  }

  .version-config {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .version-label {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    font-size: 0.8rem;
  }

  .version-label > span:first-child {
    color: var(--color-text-secondary);
    min-width: 100px;
  }

  .version-input {
    width: 100px;
    padding: 0.375rem 0.5rem;
    border: 1px solid var(--color-border);
    background: var(--color-bg-tertiary);
    color: var(--color-text-primary);
    border-radius: var(--radius-sm);
    font-size: 0.85rem;
    font-family: var(--font-mono);
  }

  .version-input:focus {
    outline: none;
    border-color: var(--color-accent-cyan);
  }

  .version-hint {
    color: var(--color-text-muted);
    font-size: 0.7rem;
    margin-left: 100px;
  }

  .deploy-hint {
    text-align: center;
    color: var(--color-text-muted);
    font-size: 0.8rem;
    font-style: italic;
  }

  .code-wrapper {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    min-height: 200px;
  }

  .code-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.5rem 1rem;
    background: var(--color-bg-card);
    border-bottom: 1px solid var(--color-border);
    font-size: 0.75rem;
    font-weight: 600;
    color: var(--color-text-primary);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .hint {
    color: var(--color-text-muted);
    font-weight: 400;
    text-transform: none;
    letter-spacing: normal;
  }

  .btn-deploy {
    display: block;
    width: 100%;
    margin-top: 1rem;
    padding: 0.75rem 1rem;
    background: linear-gradient(135deg, var(--color-success), var(--color-accent-cyan));
    border: none;
    border-radius: var(--radius-md);
    color: var(--color-bg-primary);
    font-size: 0.85rem;
    font-weight: 600;
    cursor: pointer;
    transition: var(--transition-fast);
  }

  .btn-deploy:hover:not(:disabled) {
    transform: translateY(-1px);
    box-shadow: 0 4px 15px rgba(0, 255, 136, 0.3);
  }

  .btn-deploy:disabled {
    opacity: 0.7;
    cursor: wait;
  }

  .deploy-success {
    margin-top: 0.5rem;
    padding: 0.5rem;
    background: rgba(0, 255, 136, 0.1);
    border: 1px solid var(--color-success);
    border-radius: var(--radius-sm);
    color: var(--color-success);
    font-size: 0.75rem;
    text-align: center;
  }

  .test-files {
    background: var(--color-bg-primary);
    border-bottom: 1px solid var(--color-border);
  }

  .section-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.5rem 1rem;
    background: var(--color-bg-card);
    border-bottom: 1px solid var(--color-border);
    font-size: 0.75rem;
    font-weight: 600;
    color: var(--color-text-primary);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .file-list {
    display: flex;
    flex-direction: column;
    max-height: 120px;
    overflow-y: auto;
    background: var(--color-bg-secondary);
  }

  .file-item {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem 1rem;
    background: transparent;
    border: none;
    cursor: pointer;
    text-align: left;
    color: var(--color-text-primary);
    font-size: 0.8rem;
    transition: var(--transition-fast);
  }

  .file-item:hover {
    background: var(--color-bg-tertiary);
  }

  .file-item.selected {
    background: var(--color-bg-card);
    border-left: 2px solid var(--color-accent-cyan);
  }

  .file-name {
    flex: 1;
    font-family: var(--font-mono);
    color: var(--color-text-primary);
  }

  .file-size {
    color: var(--color-text-muted);
    font-size: 0.7rem;
  }

  .remove-btn {
    padding: 0.125rem 0.375rem;
    background: transparent;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    opacity: 0;
    transition: opacity 0.1s;
    font-size: 0.75rem;
  }

  .file-item:hover .remove-btn {
    opacity: 1;
  }

  .remove-btn:hover {
    color: var(--color-error);
  }

  .data-preview,
  .validation-output {
    flex: 1;
    display: flex;
    flex-direction: column;
    /* Allow internal scrolling */
    overflow: auto;
    min-height: 120px;
    max-height: 300px;
  }

  .loading-text {
    font-style: italic;
    color: var(--color-text-muted);
  }

  .preview-content,
  .output-content {
    flex: 1;
    overflow: auto;
    padding: 0.75rem 1rem;
    background: var(--color-bg-secondary);
  }

  .preview-content pre,
  .output-content pre {
    margin: 0;
    font-family: var(--font-mono);
    font-size: 0.75rem;
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-all;
    color: var(--color-text-primary);
  }

  .empty {
    color: var(--color-text-muted);
    font-style: italic;
    font-size: 0.8rem;
    padding: 1rem;
    text-align: center;
  }

  .error-text {
    color: var(--color-error);
  }

  .badge {
    padding: 0.125rem 0.5rem;
    font-size: 0.65rem;
    font-weight: 600;
    border-radius: var(--radius-sm);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .badge.valid {
    background: rgba(0, 255, 136, 0.15);
    color: var(--color-success);
    border: 1px solid var(--color-success);
  }

  .badge.invalid {
    background: rgba(255, 51, 85, 0.15);
    color: var(--color-error);
    border: 1px solid var(--color-error);
  }

  .badge.pending {
    background: rgba(136, 136, 170, 0.15);
    color: var(--color-text-secondary);
    border: 1px solid var(--color-border);
  }

  .badge.multi {
    background: rgba(0, 212, 255, 0.15);
    color: var(--color-accent-cyan);
    border: 1px solid var(--color-accent-cyan);
    margin-left: 0.25rem;
  }

  /* Multi-output table display */
  .multi-output {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .table-section {
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }

  .table-header {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem 0.75rem;
    background: var(--color-bg-card);
    border: none;
    color: var(--color-text-primary);
    cursor: pointer;
    font-size: 0.8rem;
    text-align: left;
    transition: var(--transition-fast);
  }

  .table-header:hover {
    background: var(--color-bg-tertiary);
  }

  .collapse-icon {
    color: var(--color-text-muted);
    font-size: 0.65rem;
    width: 1rem;
  }

  .table-name {
    font-weight: 600;
    color: var(--color-accent-cyan);
    font-family: var(--font-mono);
  }

  .table-rows {
    color: var(--color-text-muted);
    font-size: 0.75rem;
  }

  .table-content {
    margin: 0;
    padding: 0.75rem;
    font-family: var(--font-mono);
    font-size: 0.7rem;
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-all;
    color: var(--color-text-primary);
    background: var(--color-bg-secondary);
    border-top: 1px solid var(--color-border);
    max-height: 200px;
    overflow-y: auto;
  }

  /* Waiting for topics state */
  .waiting-for-topics {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 2rem;
    text-align: center;
    background: var(--color-bg-secondary);
    border: 1px dashed var(--color-border);
    border-radius: var(--radius-md);
  }

  .waiting-icon {
    font-size: 2rem;
    margin-bottom: 0.75rem;
  }

  .waiting-title {
    font-weight: 600;
    color: var(--color-text-primary);
    margin-bottom: 0.5rem;
  }

  .waiting-desc {
    font-size: 0.8rem;
    color: var(--color-text-muted);
    max-width: 250px;
  }

</style>
