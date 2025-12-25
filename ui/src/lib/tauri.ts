/**
 * Tauri API Wrapper
 *
 * Automatically uses mock implementations when running outside Tauri (e.g., browser testing).
 * Import from this module instead of @tauri-apps/api/* for cross-environment compatibility.
 */

import { isTauri, mockInvoke, mockListen, mockWindow } from './tauri-mock';

// Re-export invoke - uses real or mock based on environment
export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauri) {
    const { invoke: tauriInvoke } = await import('@tauri-apps/api/core');
    return tauriInvoke<T>(cmd, args);
  }
  return mockInvoke<T>(cmd, args);
}

// Re-export listen - uses real or mock based on environment
export async function listen<T>(
  event: string,
  handler: (event: { payload: T }) => void
): Promise<() => void> {
  if (isTauri) {
    const { listen: tauriListen } = await import('@tauri-apps/api/event');
    return tauriListen(event, handler);
  }
  return mockListen(event, handler as (event: { payload: unknown }) => void);
}

// Re-export window operations
export async function getCurrentWindow() {
  if (isTauri) {
    const { getCurrentWindow: tauriGetCurrentWindow } = await import('@tauri-apps/api/window');
    return tauriGetCurrentWindow();
  }
  return mockWindow;
}

// Export isTauri check for conditional logic
export { isTauri };
