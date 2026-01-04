/**
 * Vitest Setup - runs before all tests
 *
 * Sets up global mocks and JSDOM environment for Svelte component testing.
 */

import '@testing-library/svelte/vitest';
import { vi } from 'vitest';

// Mock window object properties that Tauri expects
Object.defineProperty(window, '__TAURI_INTERNALS__', {
  value: undefined,
  writable: true,
});

// Mock Tauri APIs globally
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: vi.fn(() => ({
    minimize: vi.fn(),
    toggleMaximize: vi.fn(),
    close: vi.fn(),
  })),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
}));
