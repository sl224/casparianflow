/**
 * Tauri API Wrapper
 *
 * Supports two modes:
 * 1. Tauri mode - Real Tauri IPC (desktop app)
 * 2. Bridge mode - HTTP to SQLite bridge (Playwright tests)
 *
 * If neither mode is available, throws an error. No mocks.
 *
 * Import from this module instead of @tauri-apps/api/* for cross-environment compatibility.
 */

// Check if we're in Tauri - must be a function for dynamic checking
// since __TAURI_INTERNALS__ may be injected after module load
// Note: Tauri 2.x uses __TAURI_INTERNALS__, not __TAURI__ (unless withGlobalTauri is enabled)
function checkTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

// Check if bridge mode is enabled (set by test runner)
function checkBridgeMode(): boolean {
  return typeof window !== 'undefined' && (window as any).__CASPARIAN_BRIDGE__;
}

// Bridge server URL (set by Playwright)
const BRIDGE_URL = 'http://localhost:9999';

// Cache for Tauri detection after successful check
let tauriConfirmed = false;

// Export for backwards compatibility
export const isTauri = checkTauri();

/**
 * Wait for Tauri to be available (injected after page load)
 */
async function waitForTauri(maxWaitMs = 500): Promise<boolean> {
  if (tauriConfirmed) return true;

  if (checkTauri()) {
    tauriConfirmed = true;
    return true;
  }

  // Poll for Tauri injection (usually immediate, but just in case)
  const start = Date.now();
  while (Date.now() - start < maxWaitMs) {
    await new Promise(r => setTimeout(r, 20));
    if (checkTauri()) {
      tauriConfirmed = true;
      return true;
    }
  }

  return false;
}

/**
 * Invoke a command - routes to Tauri or Bridge based on environment
 * Throws if neither backend is available (no mock fallback)
 */
export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  // 1. Real Tauri (wait for injection on first call)
  if (await waitForTauri()) {
    const { invoke: tauriInvoke } = await import('@tauri-apps/api/core');
    return tauriInvoke<T>(cmd, args);
  }

  // 2. Bridge mode (Playwright tests)
  if (checkBridgeMode()) {
    const response = await fetch(`${BRIDGE_URL}/api/rpc`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ command: cmd, args }),
    });

    if (!response.ok) {
      const error = await response.json();
      throw new Error(error.error || 'Bridge request failed');
    }

    const data = await response.json();
    if (data.error) {
      throw new Error(data.error);
    }

    return data.result as T;
  }

  // No backend available - fail hard
  throw new Error(
    `Backend not available. Command: ${cmd}. ` +
    'This UI requires either Tauri desktop app or test bridge server.'
  );
}

/**
 * Listen for events - routes to Tauri or Bridge
 * Throws if neither backend is available (no mock fallback)
 */
export async function listen<T>(
  event: string,
  handler: (event: { payload: T }) => void
): Promise<() => void> {
  // Wait for Tauri to be available
  if (await waitForTauri()) {
    const { listen: tauriListen } = await import('@tauri-apps/api/event');
    return tauriListen(event, handler);
  }

  // Bridge mode: poll for updates (events not supported over HTTP)
  if (checkBridgeMode() && event === 'system-pulse') {
    const intervalId = setInterval(async () => {
      try {
        const pulse = await invoke<T>('get_system_pulse');
        handler({ payload: pulse });
      } catch {
        // Ignore polling errors
      }
    }, 500);

    return () => clearInterval(intervalId);
  }

  // No backend available - fail hard
  throw new Error(
    `Backend not available for event: ${event}. ` +
    'This UI requires either Tauri desktop app or test bridge server.'
  );
}

/**
 * Get current window - for window controls
 * Returns a no-op mock when not in Tauri (window controls only work in desktop app)
 */
export async function getCurrentWindow() {
  if (await waitForTauri()) {
    const { getCurrentWindow: tauriGetCurrentWindow } = await import('@tauri-apps/api/window');
    return tauriGetCurrentWindow();
  }

  // Window controls are desktop-only, return no-op for web/test environments
  return {
    minimize: async () => console.warn('Window controls only available in desktop app'),
    toggleMaximize: async () => console.warn('Window controls only available in desktop app'),
    close: async () => console.warn('Window controls only available in desktop app'),
  };
}

/**
 * Enable bridge mode (called by test runner)
 */
export function enableBridgeMode() {
  if (typeof window !== 'undefined') {
    (window as any).__CASPARIAN_BRIDGE__ = true;
  }
}
