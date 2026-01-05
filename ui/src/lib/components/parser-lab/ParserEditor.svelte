<script lang="ts">
  import { onMount, onDestroy } from "svelte";

  interface Props {
    value: string;
    onValueChange: (value: string) => void;
    onSelectionChange?: (selection: string | null) => void;
    pendingDiff?: string | null;
    onAcceptDiff?: () => void;
    onRejectDiff?: () => void;
  }

  let {
    value,
    onValueChange,
    onSelectionChange,
    pendingDiff = null,
    onAcceptDiff,
    onRejectDiff
  }: Props = $props();

  // Monaco loaded lazily
  let monaco: typeof import("monaco-editor") | null = null;
  let container: HTMLDivElement;
  let editor: import("monaco-editor").editor.IStandaloneCodeEditor | null = null;
  let diffEditor: import("monaco-editor").editor.IStandaloneDiffEditor | null = null;

  // Track if we're syncing to avoid loops
  let isSyncing = false;

  // Define the cyberpunk theme (matching existing theme)
  function defineTheme() {
    if (!monaco) return;
    monaco.editor.defineTheme("cyberpunk-parser", {
      base: "vs-dark",
      inherit: true,
      rules: [
        { token: "comment", foreground: "555566", fontStyle: "italic" },
        { token: "keyword", foreground: "00d4ff" },
        { token: "string", foreground: "00ff88" },
        { token: "number", foreground: "ffdd00" },
        { token: "function", foreground: "ff00aa" },
        { token: "variable", foreground: "e0e0e8" },
        { token: "type", foreground: "00d4ff" },
        { token: "class", foreground: "ff00aa" },
        { token: "decorator", foreground: "ff6600" },
      ],
      colors: {
        "editor.background": "#1e1e28",
        "editor.foreground": "#e0e0e8",
        "editor.lineHighlightBackground": "#262632",
        "editor.selectionBackground": "#00d4ff33",
        "editorCursor.foreground": "#00d4ff",
        "editorLineNumber.foreground": "#555566",
        "editorLineNumber.activeForeground": "#8888aa",
        "editorIndentGuide.background": "#35354a",
        "editorIndentGuide.activeBackground": "#45455a",
        "editorWidget.background": "#1e1e28",
        "editorWidget.border": "#35354a",
        "diffEditor.insertedTextBackground": "#00ff8822",
        "diffEditor.removedTextBackground": "#ff335522",
        "diffEditor.insertedLineBackground": "#00ff8811",
        "diffEditor.removedLineBackground": "#ff335511",
      },
    });
  }

  onMount(async () => {
    // Configure workers before loading Monaco
    self.MonacoEnvironment = {
      getWorkerUrl: function (_moduleId: string, label: string): string {
        if (label === "json") return "/monaco-workers/json.worker.js";
        if (label === "typescript" || label === "javascript") return "/monaco-workers/ts.worker.js";
        return "/monaco-workers/editor.worker.js";
      },
    };

    // Lazy load Monaco
    monaco = await import("monaco-editor");
    defineTheme();

    // Create editor on next frame
    requestAnimationFrame(() => {
      createEditor();
    });
  });

  function createEditor() {
    if (!container || !monaco) return;

    // Dispose existing editors
    editor?.dispose();
    diffEditor?.dispose();
    editor = null;
    diffEditor = null;

    if (pendingDiff) {
      // Create diff editor
      diffEditor = monaco.editor.createDiffEditor(container, {
        theme: "cyberpunk-parser",
        automaticLayout: true,
        fontSize: 14,
        fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
        fontLigatures: true,
        lineNumbers: "on",
        minimap: { enabled: false },
        scrollBeyondLastLine: false,
        wordWrap: "on",
        renderSideBySide: false, // Inline diff (user preference)
        readOnly: false,
        originalEditable: false,
      });

      const originalModel = monaco.editor.createModel(value, "python");
      const modifiedModel = monaco.editor.createModel(pendingDiff, "python");

      diffEditor.setModel({
        original: originalModel,
        modified: modifiedModel,
      });
    } else {
      // Create regular editor
      editor = monaco.editor.create(container, {
        value: value,
        language: "python",
        theme: "cyberpunk-parser",
        automaticLayout: true,
        fontSize: 14,
        fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
        fontLigatures: true,
        lineNumbers: "on",
        minimap: { enabled: true, scale: 1 },
        scrollBeyondLastLine: false,
        wordWrap: "off",
        tabSize: 4,
        insertSpaces: true,
        renderWhitespace: "selection",
        bracketPairColorization: { enabled: true },
        padding: { top: 16, bottom: 16 },
      });

      // Listen for content changes
      editor.onDidChangeModelContent(() => {
        if (!isSyncing && editor) {
          const newValue = editor.getValue();
          onValueChange(newValue);
        }
      });

      // Listen for selection changes
      editor.onDidChangeCursorSelection(() => {
        if (editor && onSelectionChange) {
          const selection = editor.getSelection();
          if (selection && !selection.isEmpty()) {
            const selectedText = editor.getModel()?.getValueInRange(selection);
            onSelectionChange(selectedText || null);
          } else {
            onSelectionChange(null);
          }
        }
      });

      // Keyboard shortcuts
      editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => {
        // Trigger save (handled by parent)
      });

      // Sync value in case it changed before editor was created
      syncEditorValue();
    }
  }

  // Recreate editor when diff mode changes
  $effect(() => {
    const _ = pendingDiff; // Track dependency
    if (monaco && container) {
      createEditor();
    }
  });

  // Sync value changes from parent
  // This effect must handle the case where value changes BEFORE editor is created
  $effect(() => {
    // Track value dependency explicitly
    const targetValue = value;

    if (editor && !pendingDiff) {
      const currentValue = editor.getValue();
      if (targetValue !== currentValue) {
        console.log("[ParserEditor] Syncing value, length:", targetValue.length);
        isSyncing = true;
        const position = editor.getPosition();
        editor.setValue(targetValue);
        if (position) editor.setPosition(position);
        isSyncing = false;
      }
    }
  });

  // When editor is created, ensure it has the latest value
  function syncEditorValue() {
    if (editor && !pendingDiff) {
      const currentValue = editor.getValue();
      if (value !== currentValue) {
        console.log("[ParserEditor] Post-create sync, length:", value.length);
        isSyncing = true;
        editor.setValue(value);
        isSyncing = false;
      }
    }
  }

  onDestroy(() => {
    editor?.dispose();
    diffEditor?.dispose();
  });
</script>

<div class="parser-editor">
  {#if pendingDiff}
    <div class="diff-actions">
      <span class="diff-label">AI suggested changes:</span>
      <button class="btn-accept" onclick={onAcceptDiff}>
        Accept
      </button>
      <button class="btn-reject" onclick={onRejectDiff}>
        Reject
      </button>
    </div>
  {/if}
  <div class="editor-container" bind:this={container}></div>
</div>

<style>
  .parser-editor {
    flex: 1;
    display: flex;
    flex-direction: column;
    background: var(--color-bg-secondary);
    overflow: hidden;
  }

  .diff-actions {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.5rem 1rem;
    background: var(--color-bg-card);
    border-bottom: 1px solid var(--color-border);
  }

  .diff-label {
    font-size: 0.8rem;
    color: var(--color-text-secondary);
    flex: 1;
  }

  .btn-accept {
    padding: 0.375rem 1rem;
    background: var(--color-success);
    border: none;
    border-radius: var(--radius-sm);
    color: var(--color-bg-primary);
    font-size: 0.8rem;
    font-weight: 600;
    cursor: pointer;
    transition: var(--transition-fast);
  }

  .btn-accept:hover {
    box-shadow: var(--glow-green);
  }

  .btn-reject {
    padding: 0.375rem 1rem;
    background: transparent;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    color: var(--color-text-secondary);
    font-size: 0.8rem;
    cursor: pointer;
    transition: var(--transition-fast);
  }

  .btn-reject:hover {
    border-color: var(--color-error);
    color: var(--color-error);
  }

  .editor-container {
    flex: 1;
    width: 100%;
    height: 100%;
  }

  /* Override Monaco defaults */
  :global(.monaco-editor) {
    font-feature-settings: "liga" on, "calt" on !important;
  }

  :global(.monaco-editor .scroll-decoration) {
    box-shadow: none !important;
  }
</style>
