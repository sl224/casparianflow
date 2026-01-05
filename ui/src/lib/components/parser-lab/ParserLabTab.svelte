<script lang="ts">
  import { invoke } from "$lib/tauri";
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import FileEditor from "./FileEditor.svelte";

  // Types (v6 - parser-centric, no project layer)
  interface ParserLabParserSummary {
    id: string;
    name: string;
    filePattern: string;
    patternType: string;
    validationStatus: string;
    isSample: boolean;
    testFileCount: number;
    updatedAt: number;
  }

  interface PublishedPlugin {
    plugin_name: string;
    version: string;
    status: string;
    deployed_at: string | null;
  }

  // State
  let parsers = $state<ParserLabParserSummary[]>([]);
  let publishedPlugins = $state<PublishedPlugin[]>([]);
  let isLoading = $state(true);
  let error = $state<string | null>(null);

  // Active parser being edited
  let activeParserId = $state<string | null>(null);

  onMount(async () => {
    await Promise.all([loadParsers(), loadPublishedPlugins()]);
  });

  async function loadPublishedPlugins() {
    try {
      publishedPlugins = await invoke<PublishedPlugin[]>("list_deployed_plugins", {});
    } catch (e) {
      // Silently fail - this is a nice-to-have feature
      console.warn("Failed to load published plugins:", e);
      publishedPlugins = [];
    }
  }

  async function loadParsers() {
    isLoading = true;
    error = null;
    try {
      parsers = await invoke<ParserLabParserSummary[]>("parser_lab_list_parsers", { limit: 20 });
    } catch (e) {
      error = String(e);
    } finally {
      isLoading = false;
    }
  }

  async function openFile() {
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
        title: "Select data file",
        defaultPath,
      });

      if (selected) {
        // Create a parser for this file type
        const fileName = selected.split("/").pop() || "Untitled";
        const parser = await invoke<{ id: string }>("parser_lab_create_parser", {
          name: fileName,
          filePattern: "",
        });

        // Add the file as a test file
        await invoke("parser_lab_add_test_file", {
          parserId: parser.id,
          filePath: selected,
        });

        activeParserId = parser.id;
      }
    } catch (e) {
      error = String(e);
    }
  }

  async function loadParserCode() {
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
        title: "Select parser file",
        filters: [{ name: "Python", extensions: ["py"] }],
        defaultPath,
      });

      if (selected) {
        // Import creates a new parser from the file
        const parser = await invoke<{ id: string }>("parser_lab_import_plugin", {
          pluginPath: selected,
        });

        activeParserId = parser.id;
      }
    } catch (e) {
      error = String(e);
    }
  }

  async function loadSample() {
    try {
      const parser = await invoke<{ id: string }>("parser_lab_load_sample");
      activeParserId = parser.id;
    } catch (e) {
      error = String(e);
    }
  }

  async function deleteParser(id: string, e: Event) {
    e.stopPropagation();
    try {
      await invoke("parser_lab_delete_parser", { parserId: id });
      await loadParsers();
    } catch (e) {
      error = String(e);
    }
  }

  function formatTime(timestamp: number): string {
    const now = Date.now();
    const diff = now - timestamp;
    const mins = Math.floor(diff / 60000);
    const hours = Math.floor(diff / 3600000);
    const days = Math.floor(diff / 86400000);

    if (mins < 1) return "just now";
    if (mins < 60) return `${mins}m ago`;
    if (hours < 24) return `${hours}h ago`;
    if (days === 1) return "yesterday";
    return `${days}d ago`;
  }

  function getStatusText(p: ParserLabParserSummary): string {
    const status = p.validationStatus;
    const files = p.testFileCount;
    const fileText = files === 1 ? "1 test file" : `${files} test files`;

    if (status === "valid") return `valid, ${fileText}`;
    if (status === "invalid") return `error, ${fileText}`;
    return files > 0 ? fileText : "pending";
  }

  function handleBack() {
    activeParserId = null;
    loadParsers();
    loadPublishedPlugins();
  }

  function formatDeployTime(timestamp: string | null): string {
    if (!timestamp) return "unknown";
    try {
      const date = new Date(timestamp);
      const now = new Date();
      const diff = now.getTime() - date.getTime();
      const days = Math.floor(diff / 86400000);
      const hours = Math.floor(diff / 3600000);

      if (hours < 1) return "just now";
      if (hours < 24) return `${hours}h ago`;
      if (days === 1) return "yesterday";
      if (days < 7) return `${days}d ago`;
      return date.toLocaleDateString();
    } catch {
      return timestamp;
    }
  }
</script>

<div class="parser-lab">
  {#if activeParserId}
    <FileEditor parserId={activeParserId} onBack={handleBack} />
  {:else}
    <div class="main-view">
      <header class="header">
        <h1>Parser Lab</h1>
      </header>

      <div class="actions">
        <button class="action-btn" onclick={openFile}>
          Open File
        </button>
        <button class="action-btn" onclick={loadParserCode}>
          Load Parser Code
        </button>
        <button class="action-btn action-sample" onclick={loadSample}>
          Load Sample
        </button>
      </div>

      {#if error}
        <div class="error">{error}</div>
      {/if}

      <section class="recent">
        <h2>Recent</h2>

        {#if isLoading}
          <div class="empty">Loading...</div>
        {:else if parsers.length === 0}
          <div class="empty">
            Open a file to start developing parsers, or load the sample to see how it works.
          </div>
        {:else}
          <div class="file-list">
            {#each parsers as parser}
              <div
                class="file-row"
                role="button"
                tabindex="0"
                onclick={() => (activeParserId = parser.id)}
                onkeydown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') activeParserId = parser.id;
                }}
              >
                <div class="file-info">
                  <span class="file-name">{parser.name}</span>
                  <span class="file-meta">{getStatusText(parser)}</span>
                </div>
                <div class="file-actions">
                  <span class="file-time">{formatTime(parser.updatedAt)}</span>
                  <button class="delete-btn" onclick={(e) => deleteParser(parser.id, e)}>
                    delete
                  </button>
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </section>

      {#if publishedPlugins.length > 0}
        <section class="published">
          <h2>Published Plugins</h2>
          <div class="plugin-list">
            {#each publishedPlugins as plugin}
              <div class="plugin-row">
                <div class="plugin-info">
                  <span class="plugin-name">{plugin.plugin_name}</span>
                  <span class="plugin-meta">v{plugin.version}</span>
                </div>
                <div class="plugin-status">
                  <span class="status-badge" class:active={plugin.status === 'ACTIVE'}>
                    {plugin.status}
                  </span>
                  <span class="plugin-time">{formatDeployTime(plugin.deployed_at)}</span>
                </div>
              </div>
            {/each}
          </div>
        </section>
      {/if}
    </div>
  {/if}
</div>

<style>
  .parser-lab {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--color-bg-primary);
    color: var(--color-text-primary);
  }

  .main-view {
    flex: 1;
    display: flex;
    flex-direction: column;
    padding: 2rem;
    max-width: 800px;
    margin: 0 auto;
    width: 100%;
  }

  .header h1 {
    margin: 0 0 2rem 0;
    font-size: 1.5rem;
    font-weight: 500;
    color: var(--color-text-primary);
  }

  .actions {
    display: flex;
    gap: 1rem;
    margin-bottom: 3rem;
  }

  .action-btn {
    padding: 0.75rem 1.5rem;
    background: var(--color-bg-secondary);
    border: 1px solid var(--color-border);
    color: var(--color-text-primary);
    font-size: 0.9rem;
    cursor: pointer;
    border-radius: var(--radius-sm);
    transition: var(--transition-fast);
  }

  .action-btn:hover {
    background: var(--color-bg-tertiary);
    border-color: var(--color-accent-cyan);
  }

  .action-sample {
    border-style: dashed;
    color: var(--color-text-secondary);
  }

  .action-sample:hover {
    color: var(--color-text-primary);
  }

  .error {
    padding: 0.75rem 1rem;
    margin-bottom: 1rem;
    background: rgba(255, 51, 85, 0.15);
    border: 1px solid var(--color-error);
    color: var(--color-error);
    font-size: 0.875rem;
    border-radius: var(--radius-sm);
  }

  .recent h2 {
    margin: 0 0 1rem 0;
    font-size: 0.75rem;
    font-weight: 600;
    color: var(--color-text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.1em;
  }

  .empty {
    padding: 2rem;
    text-align: center;
    color: var(--color-text-muted);
    font-size: 0.9rem;
  }

  .file-list {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }

  .file-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 1rem;
    background: var(--color-bg-secondary);
    border: none;
    border-bottom: 1px solid var(--color-border);
    cursor: pointer;
    text-align: left;
    width: 100%;
    transition: var(--transition-fast);
  }

  .file-row:hover {
    background: var(--color-bg-tertiary);
  }

  .file-row:last-child {
    border-bottom: none;
  }

  .file-info {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .file-name {
    font-size: 0.95rem;
    color: var(--color-text-primary);
  }

  .file-meta {
    font-size: 0.8rem;
    color: var(--color-text-secondary);
  }

  .file-actions {
    display: flex;
    align-items: center;
    gap: 1rem;
  }

  .file-time {
    font-size: 0.8rem;
    color: var(--color-text-muted);
  }

  .delete-btn {
    padding: 0.25rem 0.5rem;
    background: transparent;
    border: none;
    color: var(--color-text-muted);
    font-size: 0.75rem;
    cursor: pointer;
    opacity: 0;
    transition: opacity 0.1s, color 0.1s;
  }

  .file-row:hover .delete-btn {
    opacity: 1;
  }

  .delete-btn:hover {
    color: var(--color-error);
  }

  /* Published Plugins section */
  .published {
    margin-top: 2rem;
  }

  .published h2 {
    margin: 0 0 1rem 0;
    font-size: 0.75rem;
    font-weight: 600;
    color: var(--color-text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.1em;
  }

  .plugin-list {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }

  .plugin-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.75rem 1rem;
    background: var(--color-bg-secondary);
    border-bottom: 1px solid var(--color-border);
  }

  .plugin-row:last-child {
    border-bottom: none;
  }

  .plugin-info {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }

  .plugin-name {
    font-size: 0.9rem;
    color: var(--color-text-primary);
    font-family: var(--font-mono);
  }

  .plugin-meta {
    font-size: 0.75rem;
    color: var(--color-text-muted);
  }

  .plugin-status {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }

  .status-badge {
    padding: 0.125rem 0.5rem;
    font-size: 0.65rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    border-radius: 2px;
    background: var(--color-bg-tertiary);
    color: var(--color-text-muted);
  }

  .status-badge.active {
    background: rgba(0, 255, 135, 0.15);
    color: var(--color-success);
  }

  .plugin-time {
    font-size: 0.75rem;
    color: var(--color-text-muted);
  }
</style>
