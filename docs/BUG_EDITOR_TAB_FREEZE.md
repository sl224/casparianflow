# Bug Report: Editor Tab Freezes on Cold Start

## Status: RESOLVED

## Summary
The Editor tab in Casparian Deck UI freezes (becomes unresponsive) on **cold start only**. The same code works perfectly via **HMR (Hot Module Replacement)**.

## Environment
- Tauri v2
- Svelte 5 (with runes: $state, $effect)
- Vite 6.4.1
- Monaco Editor (ruled out as cause)

## Symptoms
- Click Editor tab → entire UI freezes (nothing is clickable)
- Other tabs (Dashboard, Pipeline, Data) work fine
- Works via HMR, fails on cold restart

## Key Findings from Debugging

### What is NOT the cause:
1. **Monaco Editor** - Completely removed, still freezes
2. **CodeEditor component** - Completely removed (no import, no dynamic import), still freezes
3. **$effect for loading plugins** - Disabled, still freezes
4. **CSS classes/layout** - Tested incrementally, works fine
5. **Individual store property access** - Works fine when tested individually

### What DOES work:
1. **Minimal static div** - `<div>EDITOR TAB</div>` works on cold start
2. **All store properties individually** - `editorStore.loadingPlugins`, `editorStore.plugins.length`, `editorStore.hasChanges` all work
3. **#each loops** - Iterating over `editorStore.plugins` works
4. **onclick handlers** - Buttons with store method calls work
5. **Full Editor UI via HMR** - Everything works after hot reload

### The Pattern:
- Incremental changes via HMR: **WORKS**
- Full cold restart with same code: **FREEZES**

## Reproduction Steps
1. Run `./demo/run_demo.sh`
2. Wait for UI to open
3. Click "EDITOR" tab
4. UI freezes - nothing is clickable

## Hypothesis
The issue appears to be related to **Vite's cold start module initialization** vs **HMR module updates**. Something in how Svelte 5 stores with $state runes are initialized on fresh page load causes the freeze when the Editor tab view is rendered.

Possible causes:
1. Svelte 5 reactivity system initialization differs between cold start and HMR
2. Vite's module graph processing on cold start blocks main thread
3. editorStore initialization triggers something that blocks
4. Race condition between Tauri IPC and Svelte reactivity

## Files Involved
- `/ui/src/routes/+page.svelte` - Main page with tab switching
- `/ui/src/lib/stores/editor.svelte.ts` - Editor store with Svelte 5 $state runes
- `/ui/src/lib/components/CodeEditor.svelte` - Monaco wrapper (ruled out)

## Next Steps to Investigate
1. Check if issue is specific to `editorStore` - try a fresh store
2. Check if issue is Svelte 5 $state related - try Svelte 4 style stores
3. Add console timing logs to trace where freeze occurs
4. Check browser DevTools Performance tab during freeze
5. Test with Vite's `--force` flag to bypass cache
6. Test production build vs dev build

## Root Cause
The `editorStore` class had a `$state` property that accessed `window` during class property initialization:

```typescript
// BAD - runs at MODULE LOAD TIME, blocks on cold start in Tauri WebView
pluginDir = $state(
  typeof window !== "undefined" && (window as Record<string, unknown>).__CASPARIAN_PLUGIN_DIR__
    ? String((window as Record<string, unknown>).__CASPARIAN_PLUGIN_DIR__)
    : "/path/to/plugins"
);
```

On cold start, Vite processes the module graph synchronously. Accessing `window` properties during Svelte 5 `$state` initialization in a Tauri WebView caused the main thread to block indefinitely.

## Fix
Defer `window` access from module initialization to runtime:

```typescript
// GOOD - initialize empty, defer window access to async method
pluginDir = $state("");

async loadPlugins(): Promise<void> {
  if (!this.pluginDir) {
    this.pluginDir = typeof window !== "undefined" && ...
  }
  // ...
}
```

## Lesson Learned
In Svelte 5 stores with `$state` runes, **never access `window` or browser APIs during class property initialization**. Defer such access to methods that run at runtime (e.g., inside `onMount` callbacks or async functions called after mount).
