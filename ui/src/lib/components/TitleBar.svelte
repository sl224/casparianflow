<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";

  const appWindow = getCurrentWindow();

  async function minimize() {
    await appWindow.minimize();
  }

  async function toggleMaximize() {
    const isMaximized = await appWindow.isMaximized();
    if (isMaximized) {
      await appWindow.unmaximize();
    } else {
      await appWindow.maximize();
    }
  }

  async function close() {
    await appWindow.close();
  }
</script>

<div class="titlebar" data-tauri-drag-region>
  <div class="titlebar-left">
    <span class="titlebar-icon">&#9671;</span>
    <span class="titlebar-title">CASPARIAN DECK</span>
  </div>

  <div class="titlebar-controls">
    <button class="control-btn minimize" onclick={minimize} title="Minimize">
      <svg width="12" height="12" viewBox="0 0 12 12">
        <rect y="5" width="12" height="2" fill="currentColor" />
      </svg>
    </button>
    <button class="control-btn maximize" onclick={toggleMaximize} title="Maximize">
      <svg width="12" height="12" viewBox="0 0 12 12">
        <rect x="1" y="1" width="10" height="10" fill="none" stroke="currentColor" stroke-width="1.5" />
      </svg>
    </button>
    <button class="control-btn close" onclick={close} title="Close">
      <svg width="12" height="12" viewBox="0 0 12 12">
        <path d="M1 1L11 11M1 11L11 1" stroke="currentColor" stroke-width="1.5" />
      </svg>
    </button>
  </div>
</div>

<style>
  .titlebar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    height: 32px;
    background: linear-gradient(180deg, #16161f 0%, #12121a 100%);
    border-bottom: 1px solid var(--color-border);
    user-select: none;
    -webkit-app-region: drag;
  }

  .titlebar-left {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-left: 12px;
  }

  .titlebar-icon {
    font-size: 14px;
    color: var(--color-accent-cyan);
  }

  .titlebar-title {
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 1.5px;
    color: var(--color-text-secondary);
  }

  .titlebar-controls {
    display: flex;
    -webkit-app-region: no-drag;
  }

  .control-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 46px;
    height: 32px;
    background: transparent;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .control-btn:hover {
    background: var(--color-bg-tertiary);
    color: var(--color-text-primary);
  }

  .control-btn.close:hover {
    background: var(--color-error);
    color: white;
  }

  .control-btn svg {
    width: 12px;
    height: 12px;
  }
</style>
