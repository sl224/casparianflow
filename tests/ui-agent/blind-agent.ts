/**
 * Blind Agent - UI Fuzzer for Playwright
 *
 * A "blind" agent that discovers and interacts with UI elements
 * without prior knowledge of the application structure.
 *
 * Used for:
 * 1. Stress testing (rapid random actions)
 * 2. Exploratory testing (discovering UI elements)
 * 3. Regression testing (verifying element presence)
 */

import { Page, Locator } from '@playwright/test';

// ============================================================================
// Types
// ============================================================================

/** Types of interactive elements we can discover */
export type TargetType =
  | 'button'
  | 'input'
  | 'toggle'
  | 'tab'
  | 'link'
  | 'delete'
  | 'add'
  | 'refresh'
  | 'unknown';

/** A discovered interactive element */
export interface Target {
  /** Unique identifier for this target */
  id: string;
  /** Type of element */
  type: TargetType;
  /** Visible text content */
  text: string;
  /** Selector to locate this element */
  selector: string;
  /** Location in UI (header, navigation, content, footer) */
  location: string;
  /** The Playwright locator */
  locator: Locator;
}

/** Options for creating an agent */
export interface AgentOptions {
  /** Milliseconds to wait after actions (default: 100) */
  waitMs?: number;
  /** Enable verbose logging (default: false) */
  verbose?: boolean;
}

/** Result of an action */
export interface ActionResult {
  status: 'OK' | 'WARNING' | 'CRITICAL';
  message: string;
  duration: number;
}

/** Log entry for an action */
export interface ActionEntry {
  timestamp: number;
  target: Target;
  result: ActionResult;
}

// ============================================================================
// BlindAgent Class
// ============================================================================

export class BlindAgent {
  private page: Page;
  private options: Required<AgentOptions>;
  private log: ActionEntry[] = [];
  private discoveredTargets: Target[] = [];

  constructor(page: Page, options: AgentOptions = {}) {
    this.page = page;
    this.options = {
      waitMs: options.waitMs ?? 100,
      verbose: options.verbose ?? false,
    };
  }

  /**
   * Discover all interactive elements on the current page
   */
  async discover(): Promise<Target[]> {
    const targets: Target[] = [];
    let id = 0;

    // Define element patterns to search for
    const patterns: Array<{
      selector: string;
      type: TargetType;
      textSelector?: string;
    }> = [
      // Buttons
      { selector: 'button', type: 'button' },
      { selector: '[role="button"]', type: 'button' },
      // Inputs
      { selector: 'input:not([type="hidden"])', type: 'input' },
      { selector: 'textarea', type: 'input' },
      // Toggles/Switches
      { selector: '[role="switch"]', type: 'toggle' },
      { selector: '.toggle', type: 'toggle' },
      { selector: 'input[type="checkbox"]', type: 'toggle' },
      // Tabs
      { selector: '.tab', type: 'tab' },
      { selector: '[role="tab"]', type: 'tab' },
      // Links
      { selector: 'a[href]', type: 'link' },
    ];

    for (const pattern of patterns) {
      try {
        const elements = this.page.locator(pattern.selector);
        const count = await elements.count();

        for (let i = 0; i < count; i++) {
          const element = elements.nth(i);

          // Skip invisible elements
          if (!(await element.isVisible().catch(() => false))) {
            continue;
          }

          // Get text content
          const text = await element.textContent().catch(() => '') || '';
          const trimmedText = text.trim().substring(0, 50);

          // Determine specific type based on text content
          let type = pattern.type;
          const lowerText = trimmedText.toLowerCase();

          if (lowerText.includes('delete') || lowerText.includes('remove')) {
            type = 'delete';
          } else if (lowerText.includes('add') || lowerText.includes('new') || lowerText.includes('create')) {
            type = 'add';
          } else if (lowerText.includes('refresh') || lowerText.includes('reload')) {
            type = 'refresh';
          }

          // Determine location in UI
          const location = await this.determineLocation(element);

          // Create unique selector
          const uniqueSelector = `${pattern.selector}:nth-of-type(${i + 1})`;

          targets.push({
            id: `target-${id++}`,
            type,
            text: trimmedText || `[${type}]`,
            selector: uniqueSelector,
            location,
            locator: element,
          });
        }
      } catch (e) {
        // Pattern didn't match anything, continue
        if (this.options.verbose) {
          console.log(`Pattern ${pattern.selector} failed:`, e);
        }
      }
    }

    this.discoveredTargets = targets;

    if (this.options.verbose) {
      console.log(`Discovered ${targets.length} targets`);
    }

    return targets;
  }

  /**
   * Determine the location of an element in the UI
   */
  private async determineLocation(element: Locator): Promise<string> {
    try {
      // Check if in header
      const inHeader = await this.page
        .locator('header, .header, [role="banner"]')
        .locator(element)
        .count();
      if (inHeader > 0) return 'header';

      // Check if in navigation
      const inNav = await this.page
        .locator('nav, .nav, .navigation, .tabs, [role="navigation"], [role="tablist"]')
        .locator(element)
        .count();
      if (inNav > 0) return 'navigation';

      // Check if in footer
      const inFooter = await this.page
        .locator('footer, .footer, [role="contentinfo"]')
        .locator(element)
        .count();
      if (inFooter > 0) return 'footer';

      // Default to content
      return 'content';
    } catch {
      return 'unknown';
    }
  }

  /**
   * Find targets by type
   */
  async findByType(type: TargetType): Promise<Target[]> {
    if (this.discoveredTargets.length === 0) {
      await this.discover();
    }
    return this.discoveredTargets.filter((t) => t.type === type);
  }

  /**
   * Find targets by text content (partial match, case-insensitive)
   */
  async findByText(text: string): Promise<Target[]> {
    if (this.discoveredTargets.length === 0) {
      await this.discover();
    }
    const lowerText = text.toLowerCase();
    return this.discoveredTargets.filter((t) =>
      t.text.toLowerCase().includes(lowerText)
    );
  }

  /**
   * Perform an action on a target
   */
  async act(target: Target): Promise<ActionEntry> {
    const startTime = Date.now();
    let result: ActionResult;

    try {
      // Perform action based on type
      switch (target.type) {
        case 'input':
          // Type random text into inputs
          await target.locator.fill('test-' + Date.now());
          break;

        case 'toggle':
          // Click toggles
          await target.locator.click();
          break;

        default:
          // Click everything else
          await target.locator.click();
          break;
      }

      // Wait for UI to settle
      await this.page.waitForTimeout(this.options.waitMs);

      // Check for error states in UI
      const hasError = await this.page
        .locator('.error, [role="alert"], .toast-error')
        .count();

      if (hasError > 0) {
        result = {
          status: 'WARNING',
          message: 'Action succeeded but UI shows error state',
          duration: Date.now() - startTime,
        };
      } else {
        result = {
          status: 'OK',
          message: 'Action completed',
          duration: Date.now() - startTime,
        };
      }
    } catch (e: any) {
      // Check if page crashed
      const pageOk = await this.page
        .locator('body')
        .count()
        .catch(() => 0);

      if (pageOk === 0) {
        result = {
          status: 'CRITICAL',
          message: `Page crashed: ${e.message}`,
          duration: Date.now() - startTime,
        };
      } else {
        result = {
          status: 'WARNING',
          message: `Action failed: ${e.message}`,
          duration: Date.now() - startTime,
        };
      }
    }

    const entry: ActionEntry = {
      timestamp: Date.now(),
      target,
      result,
    };

    this.log.push(entry);

    if (this.options.verbose) {
      console.log(
        `[${result.status}] ${target.type} "${target.text}" - ${result.message}`
      );
    }

    return entry;
  }

  /**
   * Perform random actions on discovered elements
   */
  async chaos(iterations: number = 10): Promise<ActionEntry[]> {
    if (this.discoveredTargets.length === 0) {
      await this.discover();
    }

    const entries: ActionEntry[] = [];

    for (let i = 0; i < iterations; i++) {
      // Re-discover to handle dynamic elements
      if (i % 5 === 0) {
        await this.discover();
      }

      if (this.discoveredTargets.length === 0) {
        break;
      }

      // Pick random target
      const target =
        this.discoveredTargets[
          Math.floor(Math.random() * this.discoveredTargets.length)
        ];

      const entry = await this.act(target);
      entries.push(entry);

      // Stop if we hit a critical error
      if (entry.result.status === 'CRITICAL') {
        console.error('CRITICAL error - stopping chaos test');
        break;
      }
    }

    return entries;
  }

  /**
   * Get the action log
   */
  getLog(): ActionEntry[] {
    return [...this.log];
  }

  /**
   * Clear the action log
   */
  clearLog(): void {
    this.log = [];
  }

  /**
   * Get summary statistics
   */
  getSummary(): {
    total: number;
    ok: number;
    warnings: number;
    critical: number;
    avgDuration: number;
  } {
    const total = this.log.length;
    const ok = this.log.filter((e) => e.result.status === 'OK').length;
    const warnings = this.log.filter((e) => e.result.status === 'WARNING').length;
    const critical = this.log.filter((e) => e.result.status === 'CRITICAL').length;
    const avgDuration =
      total > 0
        ? this.log.reduce((sum, e) => sum + e.result.duration, 0) / total
        : 0;

    return { total, ok, warnings, critical, avgDuration };
  }
}

// ============================================================================
// Factory Function
// ============================================================================

/**
 * Create a new BlindAgent instance
 */
export async function createAgent(
  page: Page,
  options: AgentOptions = {}
): Promise<BlindAgent> {
  const agent = new BlindAgent(page, options);
  await agent.discover();
  return agent;
}
