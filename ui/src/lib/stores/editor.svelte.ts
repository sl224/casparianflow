/**
 * Editor Store - Plugin source code editing
 */

import { invoke } from "@tauri-apps/api/core";
import { $derived } from "svelte/reactivity";

/** Plugin file information */
export interface PluginFile {
  name: string;
  path: string;
}

/** Deploy result from Rust backend */
export interface DeployResult {
  success: boolean;
  pluginName: string;
  version: string;
  sourceHash: string;
  validationErrors: string[];
}

function createEditorStore() {
  // Available plugins
  let plugins = $state<PluginFile[]>([]);
  let loadingPlugins = $state(false);
  let pluginsError = $state<string | null>(null);

  // Currently open file
  let currentFile = $state<PluginFile | null>(null);
  let currentContent = $state("");
  let originalContent = $state(""); // For tracking unsaved changes

  // Editor state
  let loading = $state(false);
  let saving = $state(false);
  let error = $state<string | null>(null);

  // Plugin directory - will be initialized later
  let pluginDir = $state("");

  /** Check if there are unsaved changes */
  const hasChanges = $derived(currentContent !== originalContent);

  /** Load list of plugins from directory */
  async function loadPlugins(): Promise<void> {
    // Defer plugin directory initialization to avoid race conditions on cold start
    if (!pluginDir) {
      pluginDir =
        typeof window !== "undefined" && (window as Record<string, unknown>).__CASPARIAN_PLUGIN_DIR__
          ? String((window as Record<string, unknown>).__CASPARIAN_PLUGIN_DIR__)
          : "/Users/shan/workspace/casparianflow/demo/plugins"; // Fallback for safety
    }

    loadingPlugins = true;
    pluginsError = null;

    try {
      const files = await invoke<string[]>("list_plugins", { dir: pluginDir });
      plugins = files.map((name) => ({
        name,
        path: `${pluginDir}/${name}`,
      }));
      console.log("[EditorStore] Loaded", plugins.length, "plugins");
    } catch (err) {
      pluginsError = err instanceof Error ? err.message : String(err);
      console.error("[EditorStore] Failed to load plugins:", pluginsError);
    } finally {
      loadingPlugins = false;
    }
  }

  /** Open a plugin file for editing */
  async function openFile(file: PluginFile): Promise<void> {
    // Check for unsaved changes
    if (hasChanges && currentFile) {
      const discard = confirm(
        `You have unsaved changes in ${currentFile.name}. Discard?`
      );
      if (!discard) return;
    }

    loading = true;
    error = null;

    try {
      const content = await invoke<string>("read_plugin_file", { path: file.path });
      currentFile = file;
      currentContent = content;
      originalContent = content;
      console.log("[EditorStore] Opened", file.name);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
      console.error("[EditorStore] Failed to open file:", error);
    } finally {
      loading = false;
    }
  }

  /** Save current file */
  async function saveFile(): Promise<boolean> {
    if (!currentFile) return false;

    saving = true;
    error = null;

    try {
      await invoke("write_plugin_file", {
        path: currentFile.path,
        content: currentContent,
      });
      originalContent = currentContent;
      console.log("[EditorStore] Saved", currentFile.name);
      return true;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
      console.error("[EditorStore] Failed to save file:", error);
      return false;
    } finally {
      saving = false;
    }
  }

  /** Update content (from editor) */
  function updateContent(content: string): void {
    currentContent = content;
  }

  /** Revert to original content */
  function revert(): void {
    currentContent = originalContent;
  }

  /** Close current file */
  function closeFile(): void {
    if (hasChanges) {
      const discard = confirm("You have unsaved changes. Discard?");
      if (!discard) return;
    }

    currentFile = null;
    currentContent = "";
    originalContent = "";
    error = null;
  }

  // Deploy state
  let deploying = $state(false);
  let deployResult = $state<DeployResult | null>(null);

  /** Deploy the current plugin */
  async function deployPlugin(): Promise<boolean> {
    if (!currentFile) return false;

    // Save first if there are changes
    if (hasChanges) {
      const saved = await saveFile();
      if (!saved) return false;
    }

    deploying = true;
    deployResult = null;
    error = null;

    try {
      const result = await invoke<DeployResult>("deploy_plugin", {
        path: currentFile.path,
        code: currentContent,
      });

      deployResult = result;

      if (result.success) {
        console.log("[EditorStore] Deployed", result.pluginName, "v" + result.version);
      } else {
        console.warn("[EditorStore] Deploy failed:", result.validationErrors);
      }

      return result.success;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
      console.error("[EditorStore] Deploy error:", error);
      return false;
    } finally {
      deploying = false;
    }
  }

  /** Clear deploy result */
  function clearDeployResult(): void {
    deployResult = null;
  }

  return {
    // State (read-only access via getters)
    get plugins() { return plugins; },
    get loadingPlugins() { return loadingPlugins; },
    get pluginsError() { return pluginsError; },
    get currentFile() { return currentFile; },
    get currentContent() { return currentContent; },
    get originalContent() { return originalContent; },
    get loading() { return loading; },
    get saving() { return saving; },
    get error() { return error; },
    get deploying() { return deploying; },
    get deployResult() { return deployResult; },

    // Derived state
    get hasChanges() { return hasChanges; },

    // Methods
    loadPlugins,
    openFile,
    saveFile,
    updateContent,
    revert,
    closeFile,
    deployPlugin,
    clearDeployResult,
  };
}

export const editorStore = createEditorStore();
