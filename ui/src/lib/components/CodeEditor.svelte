<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import * as monaco from "monaco-editor";
  import { editorStore } from "$lib/stores/editor.svelte";

  let container: HTMLDivElement;
  let editor: monaco.editor.IStandaloneCodeEditor | null = null;

  // Define the cyberpunk theme
  function defineTheme() {
    monaco.editor.defineTheme("cyberpunk", {
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
        "editor.background": "#0a0a0f",
        "editor.foreground": "#e0e0e8",
        "editor.lineHighlightBackground": "#16161f",
        "editor.selectionBackground": "#00d4ff33",
        "editorCursor.foreground": "#00d4ff",
        "editorLineNumber.foreground": "#555566",
        "editorLineNumber.activeForeground": "#8888aa",
        "editorIndentGuide.background": "#2a2a3a",
        "editorIndentGuide.activeBackground": "#3a3a4a",
        "editorWidget.background": "#12121a",
        "editorWidget.border": "#2a2a3a",
        "input.background": "#16161f",
        "input.border": "#2a2a3a",
        "input.foreground": "#e0e0e8",
        "scrollbarSlider.background": "#2a2a3a88",
        "scrollbarSlider.hoverBackground": "#3a3a4aaa",
      },
    });
  }

  onMount(() => {
    defineTheme();

    editor = monaco.editor.create(container, {
      value: editorStore.currentContent,
      language: "python",
      theme: "cyberpunk",
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
      const newContent = editor!.getValue();
      lastSyncedContent = newContent;
      editorStore.updateContent(newContent);
    });

    // Keyboard shortcuts
    editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => {
      editorStore.saveFile();
    });

    // Initialize sync tracking
    lastSyncedContent = editorStore.currentContent;
  });

  onDestroy(() => {
    editor?.dispose();
  });

  // Track last synced content to prevent loops
  let lastSyncedContent = "";

  // Reactive update when file changes (external update only)
  $effect(() => {
    const content = editorStore.currentContent;
    if (editor && content !== lastSyncedContent && content !== editor.getValue()) {
      const pos = editor.getPosition();
      editor.setValue(content);
      lastSyncedContent = content;
      if (pos) editor.setPosition(pos);
    }
  });
</script>

<div class="editor-wrapper">
  <div class="editor-container" bind:this={container}></div>
</div>

<style>
  .editor-wrapper {
    flex: 1;
    display: flex;
    flex-direction: column;
    background: var(--color-bg-primary);
    overflow: hidden;
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
