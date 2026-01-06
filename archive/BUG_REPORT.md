# Bug Report: UI Freezes When Clicking Editor Tab

## Summary
The Tauri v2 + Svelte 5 desktop application freezes completely when clicking the "EDITOR" tab. The freeze affects all UI interactions - buttons become unresponsive and tab switching stops working.

## Environment
- **Framework**: Tauri v2 + Svelte 5 (with runes)
- **Build tool**: Vite 6.4.1
- **Platform**: macOS Darwin 25.1.0
- **Node runtime**: Bun

## Symptoms
1. Click EDITOR tab from any other tab (DASHBOARD, PIPELINE, DATA)
2. UI completely freezes
3. Cannot click any plugins in the sidebar
4. Cannot switch to other tabs
5. Refresh button does nothing
6. Must force-quit the application

## What Was Tried
1. Added guard to prevent multiple `loadPlugins()` calls
2. Used `setTimeout` to break out of reactive context
3. Disabled Monaco editor component entirely
4. **None of these fixed the freeze**

## Relevant Files

### 1. Main Page Component (`ui/src/routes/+page.svelte`)

```svelte
<script lang="ts">
  import { systemStore } from "$lib/stores/system.svelte";
  import { jobsStore } from "$lib/stores/jobs.svelte";
  import { editorStore } from "$lib/stores/editor.svelte";
  import Pipeline from "$lib/components/Pipeline.svelte";
  import DataGrid from "$lib/components/DataGrid.svelte";
  import CodeEditor from "$lib/components/CodeEditor.svelte";

  // Current view tab
  let activeTab = $state<"dashboard" | "pipeline" | "editor" | "data">("dashboard");

  // Load plugins when editor tab is activated (only once)
  let pluginsLoaded = false;
  $effect(() => {
    if (activeTab === "editor" && !pluginsLoaded) {
      pluginsLoaded = true;
      // Use setTimeout to break out of the reactive context
      setTimeout(() => editorStore.loadPlugins(), 0);
    }
  });
</script>

<!-- Editor tab section (around line 169) -->
{:else if activeTab === "editor"}
  <!-- Editor View -->
  <div class="editor-view">
    <div class="editor-sidebar">
      <h3 class="sidebar-title">PLUGINS</h3>
      <div class="plugin-list">
        {#if editorStore.loadingPlugins}
          <div class="loading">Loading...</div>
        {:else if editorStore.pluginsError}
          <div class="error">{editorStore.pluginsError}</div>
        {:else if editorStore.plugins.length === 0}
          <div class="empty">No plugins found</div>
        {:else}
          {#each editorStore.plugins as plugin}
            <button
              class="plugin-item"
              class:selected={editorStore.currentFile?.path === plugin.path}
              onclick={() => editorStore.openFile(plugin)}
            >
              <span class="plugin-icon">&#128196;</span>
              <span class="plugin-name">{plugin.name}</span>
            </button>
          {/each}
        {/if}
      </div>
      <button class="refresh-btn" onclick={() => editorStore.loadPlugins()}>
        &#8635; Refresh
      </button>
    </div>
    <!-- ... rest of editor view ... -->
  </div>
{/if}
```

### 2. Editor Store (`ui/src/lib/stores/editor.svelte.ts`)

```typescript
import { invoke } from "@tauri-apps/api/core";

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
  originalContent = $state("");

  // Editor state
  loading = $state(false);
  saving = $state(false);
  error = $state<string | null>(null);

  // Plugin directory
  pluginDir = $state(
    typeof window !== "undefined" && (window as Record<string, unknown>).__CASPARIAN_PLUGIN_DIR__
      ? String((window as Record<string, unknown>).__CASPARIAN_PLUGIN_DIR__)
      : "/Users/shan/workspace/casparianflow/demo/plugins"
  );

  get hasChanges(): boolean {
    return this.currentContent !== this.originalContent;
  }

  async loadPlugins(): Promise<void> {
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

  async openFile(file: PluginFile): Promise<void> {
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

  // ... other methods (saveFile, updateContent, revert, closeFile, deployPlugin)
}

export const editorStore = new EditorStore();
```

### 3. Rust Backend Commands (`ui/src-tauri/src/lib.rs`)

```rust
/// List plugin files in a directory
#[tauri::command]
async fn list_plugins(dir: String) -> Result<Vec<String>, String> {
    let canonical_dir = std::fs::canonicalize(&dir)
        .map_err(|e| format!("Invalid directory: {}", e))?;

    if !canonical_dir.is_dir() {
        return Err("Path is not a directory".to_string());
    }

    let mut entries = tokio::fs::read_dir(&canonical_dir)
        .await
        .map_err(|e| format!("Failed to read directory: {}", e))?;

    let mut plugins = Vec::new();
    while let Some(entry) = entries.next_entry().await.map_err(|e| e.to_string())? {
        let path = entry.path();
        if path.is_file() && path.extension().map(|e| e == "py").unwrap_or(false) {
            if let Some(name) = path.file_name() {
                plugins.push(name.to_string_lossy().to_string());
            }
        }
    }

    plugins.sort();
    Ok(plugins)
}
```

### 4. Other Stores That Initialize on Import

**System Store** (`system.svelte.ts`) - listens to Tauri events:
```typescript
constructor() {
  if (typeof window !== "undefined") {
    setTimeout(() => this.init(), 100);
  }
}

private async init() {
  // Listens to "system-pulse" events from Rust backend
  await listen<SystemPulse>("system-pulse", (event) => {
    this.pulse = event.payload;
    // ... updates state
  });
}
```

**Jobs Store** (`jobs.svelte.ts`) - auto-refreshes on construction:
```typescript
constructor() {
  if (typeof window !== "undefined") {
    setTimeout(() => this.refreshJobs(), 300);
  }
}
```

## Working Tabs
- **DASHBOARD**: Works fine, shows real-time metrics
- **PIPELINE**: Works fine (though may show empty if no topology data)
- **DATA**: Works fine, shows completed jobs

## Observations
1. The freeze happens IMMEDIATELY when clicking the EDITOR tab
2. Even with Monaco editor completely disabled, it still freezes
3. No errors appear in the terminal output
4. HMR updates apply successfully (seen in logs)
5. Other tabs work perfectly before clicking EDITOR

## Possible Causes to Investigate
1. **Svelte 5 $state reactivity loop** - Something in how the editor store's $state properties are being read could cause an infinite re-render
2. **Tauri invoke blocking** - The `list_plugins` command might be blocking somehow
3. **Import side effects** - The editorStore singleton might be doing something on import that causes issues
4. **CSS/Layout issue** - The `.editor-view` or `.editor-sidebar` CSS might cause infinite layout recalculation

## Package Versions
```json
{
  "dependencies": {
    "@tauri-apps/api": "^2",
    "@xyflow/svelte": "^1.5.0",
    "monaco-editor": "^0.55.1"
  },
  "devDependencies": {
    "@sveltejs/kit": "^2.9.0",
    "svelte": "^5.0.0",
    "vite": "^6.0.3"
  }
}
```

## To Reproduce
1. Run `./demo/run_demo.sh` from project root
2. Wait for UI window to open
3. Click "EDITOR" tab
4. Observe freeze

## Files to Examine
- `ui/src/routes/+page.svelte` - Main page with tab switching
- `ui/src/lib/stores/editor.svelte.ts` - Editor store with $state
- `ui/src/lib/components/CodeEditor.svelte` - Monaco wrapper (currently disabled)
- `ui/src-tauri/src/lib.rs` - Rust Tauri commands
