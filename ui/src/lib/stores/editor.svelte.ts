/**
 * Editor Store - Plugin source code editing
 */

import { invoke } from "$lib/tauri";

/** Plugin file information */
export interface PluginFile {
  name: string;
  path: string;
}

class EditorStore {
  // Available plugins
  plugins = $state<PluginFile[]>([]);
  loadingPlugins = $state(false);
  pluginsError = $state<string | null>(null);

  // Currently open file
  currentFile = $state<PluginFile | null>(null);
  currentContent = $state("");
  originalContent = $state(""); // For tracking unsaved changes

  // Editor state
  loading = $state(false);
  saving = $state(false);
  error = $state<string | null>(null);

  // Plugin directory (configured at runtime via window property or default)
  // NOTE: Initialized empty to avoid window access during module load (causes freeze on cold start)
  pluginDir = $state("");

  /** Check if there are unsaved changes */
  get hasChanges(): boolean {
    return this.currentContent !== this.originalContent;
  }

  /** Load list of plugins from directory */
  async loadPlugins(): Promise<void> {
    // Defer plugin directory initialization to avoid race conditions on cold start
    if (!this.pluginDir) {
      const win = window as unknown as Record<string, unknown>;
      this.pluginDir =
        typeof window !== "undefined" && win.__CASPARIAN_PLUGIN_DIR__
          ? String(win.__CASPARIAN_PLUGIN_DIR__)
          : "/Users/shan/workspace/casparianflow/demo/plugins";
    }

    this.loadingPlugins = true;
    this.pluginsError = null;

    try {
      const files = await invoke<string[]>("list_plugins", { dir: this.pluginDir });
      this.plugins = files.map(name => ({
        name,
        path: `${this.pluginDir}/${name}`,
      }));
      console.log("[EditorStore] Loaded", this.plugins.length, "plugins");
    } catch (err) {
      this.pluginsError = err instanceof Error ? err.message : String(err);
      console.error("[EditorStore] Failed to load plugins:", this.pluginsError);
    } finally {
      this.loadingPlugins = false;
    }
  }

  /** Open a plugin file for editing */
  async openFile(file: PluginFile): Promise<void> {
    // Check for unsaved changes
    if (this.hasChanges && this.currentFile) {
      const discard = confirm(
        `You have unsaved changes in ${this.currentFile.name}. Discard?`
      );
      if (!discard) return;
    }

    this.loading = true;
    this.error = null;

    try {
      const content = await invoke<string>("read_plugin_file", { path: file.path });
      this.currentFile = file;
      this.currentContent = content;
      this.originalContent = content;
      console.log("[EditorStore] Opened", file.name);
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
      console.error("[EditorStore] Failed to open file:", this.error);
    } finally {
      this.loading = false;
    }
  }

  /** Save current file */
  async saveFile(): Promise<boolean> {
    if (!this.currentFile) return false;

    this.saving = true;
    this.error = null;

    try {
      await invoke("write_plugin_file", {
        path: this.currentFile.path,
        content: this.currentContent,
      });
      this.originalContent = this.currentContent;
      console.log("[EditorStore] Saved", this.currentFile.name);
      return true;
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
      console.error("[EditorStore] Failed to save file:", this.error);
      return false;
    } finally {
      this.saving = false;
    }
  }

  /** Update content (from editor) */
  updateContent(content: string): void {
    this.currentContent = content;
  }

  /** Revert to original content */
  revert(): void {
    this.currentContent = this.originalContent;
  }

  /** Close current file */
  closeFile(): void {
    if (this.hasChanges) {
      const discard = confirm("You have unsaved changes. Discard?");
      if (!discard) return;
    }

    this.currentFile = null;
    this.currentContent = "";
    this.originalContent = "";
    this.error = null;
  }

  // Deploy state
  deploying = $state(false);
  deployResult = $state<DeployResult | null>(null);

  /** Deploy the current plugin */
  async deployPlugin(): Promise<boolean> {
    if (!this.currentFile) return false;

    // Save first if there are changes
    if (this.hasChanges) {
      const saved = await this.saveFile();
      if (!saved) return false;
    }

    this.deploying = true;
    this.deployResult = null;
    this.error = null;

    try {
      const result = await invoke<DeployResult>("deploy_plugin", {
        path: this.currentFile.path,
        code: this.currentContent,
      });

      this.deployResult = result;

      if (result.success) {
        console.log("[EditorStore] Deployed", result.pluginName, "v" + result.version);
      } else {
        console.warn("[EditorStore] Deploy failed:", result.validationErrors);
      }

      return result.success;
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
      console.error("[EditorStore] Deploy error:", this.error);
      return false;
    } finally {
      this.deploying = false;
    }
  }

  /** Clear deploy result */
  clearDeployResult(): void {
    this.deployResult = null;
  }
}

/** Deploy result from Rust backend */
export interface DeployResult {
  success: boolean;
  pluginName: string;
  version: string;
  sourceHash: string;
  validationErrors: string[];
}

export const editorStore = new EditorStore();
