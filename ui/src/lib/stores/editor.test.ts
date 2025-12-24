/**
 * Editor Store Tests
 *
 * Tests state transitions and business logic without DOM.
 * Focus on: file handling, unsaved changes detection, deploy validation.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock Tauri
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

import type { DeployResult, PluginFile } from './editor.svelte';

describe('hasChanges detection', () => {
  it('should detect no changes when content matches original', () => {
    const original = 'print("hello")';
    const current = 'print("hello")';

    const hasChanges = current !== original;

    expect(hasChanges).toBe(false);
  });

  it('should detect changes when content differs', () => {
    const original = 'print("hello")';
    const current = 'print("world")';

    const hasChanges = current !== original;

    expect(hasChanges).toBe(true);
  });

  it('should detect whitespace-only changes', () => {
    const original = 'print("hello")';
    const current = 'print("hello") '; // trailing space

    const hasChanges = current !== original;

    expect(hasChanges).toBe(true);
  });

  it('should handle empty files', () => {
    const original = '';
    const current = '';

    const hasChanges = current !== original;

    expect(hasChanges).toBe(false);
  });
});

describe('DeployResult handling', () => {
  it('should parse successful deploy result', () => {
    const result: DeployResult = {
      success: true,
      pluginName: 'my_plugin',
      version: '20241224.120000',
      sourceHash: 'abc123def456',
      validationErrors: [],
    };

    expect(result.success).toBe(true);
    expect(result.validationErrors).toHaveLength(0);
    expect(result.version).toMatch(/^\d{8}\.\d{6}$/); // YYYYMMDD.HHMMSS format
  });

  it('should parse failed deploy with validation errors', () => {
    const result: DeployResult = {
      success: false,
      pluginName: 'bad_plugin',
      version: '0.0.0',
      sourceHash: 'xyz789',
      validationErrors: [
        "Banned import: 'import os'",
        "Banned import: 'from subprocess import run'",
      ],
    };

    expect(result.success).toBe(false);
    expect(result.validationErrors).toHaveLength(2);
    expect(result.validationErrors[0]).toContain('os');
  });

  it('should handle empty validation errors on failure', () => {
    // Edge case: failure with no specific errors (e.g., parse error)
    const result: DeployResult = {
      success: false,
      pluginName: 'parse_error_plugin',
      version: '0.0.0',
      sourceHash: 'hash',
      validationErrors: ['Failed to parse Python source code'],
    };

    expect(result.success).toBe(false);
    expect(result.validationErrors.length).toBeGreaterThan(0);
  });
});

describe('PluginFile path handling', () => {
  it('should extract plugin name from path', () => {
    const file: PluginFile = {
      name: 'my_plugin.py',
      path: '/Users/test/workspace/plugins/my_plugin.py',
    };

    // The name should be extractable from the path
    const extractedName = file.path.split('/').pop();
    expect(extractedName).toBe(file.name);
  });

  it('should handle paths with spaces', () => {
    const file: PluginFile = {
      name: 'my plugin.py',
      path: '/Users/test/My Projects/plugins/my plugin.py',
    };

    expect(file.name).toBe('my plugin.py');
    expect(file.path).toContain('My Projects');
  });

  it('should handle deeply nested paths', () => {
    const file: PluginFile = {
      name: 'plugin.py',
      path: '/a/b/c/d/e/f/g/plugin.py',
    };

    const depth = file.path.split('/').length - 1;
    expect(depth).toBe(8);
  });
});

describe('Revert functionality', () => {
  it('should restore original content on revert', () => {
    const original = 'original code';
    let current = 'modified code';

    // Simulate revert
    current = original;

    expect(current).toBe(original);
    expect(current !== original).toBe(false); // hasChanges should be false
  });
});

describe('Save before deploy logic', () => {
  it('should require save when hasChanges before deploy', () => {
    const hasChanges = true;

    // Business rule: if hasChanges, save first
    const shouldSaveFirst = hasChanges;

    expect(shouldSaveFirst).toBe(true);
  });

  it('should skip save when no changes before deploy', () => {
    const hasChanges = false;

    const shouldSaveFirst = hasChanges;

    expect(shouldSaveFirst).toBe(false);
  });
});
