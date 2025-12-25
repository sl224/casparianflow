/**
 * Config Store - Routing rules and topic configurations
 * Uses optimistic updates for responsive UI
 */

import { invoke } from "$lib/tauri";

/** A routing rule that maps file patterns to tags */
export interface RoutingRule {
  id: number;
  pattern: string;
  tag: string;
  priority: number;
  enabled: boolean;
  description: string | null;
}

/** Topic configuration for plugin outputs */
export interface TopicConfig {
  id: number;
  pluginName: string;
  topicName: string;
  uri: string;
  mode: string;
}

/** Pending change for optimistic updates */
interface PendingChange<T> {
  original: T;
  optimistic: T;
  timestamp: number;
}

/** Reactive config store */
class ConfigStore {
  // Routing rules
  rules = $state<RoutingRule[]>([]);
  loadingRules = $state(false);
  rulesError = $state<string | null>(null);

  // Topic configurations
  topics = $state<TopicConfig[]>([]);
  loadingTopics = $state(false);
  topicsError = $state<string | null>(null);

  // Pending changes for optimistic updates (keyed by rule id)
  pendingRuleChanges = $state<Map<number, PendingChange<RoutingRule>>>(
    new Map()
  );

  // Track save operations in flight
  savingRules = $state<Set<number>>(new Set());

  constructor() {
    if (typeof window !== "undefined") {
      setTimeout(() => {
        this.loadRules();
        this.loadTopics();
      }, 300);
    }
  }

  // ============================================================================
  // Routing Rules
  // ============================================================================

  /** Load all routing rules from backend */
  async loadRules(): Promise<void> {
    this.loadingRules = true;
    this.rulesError = null;

    try {
      this.rules = await invoke<RoutingRule[]>("get_routing_rules");
      console.log("[ConfigStore] Loaded", this.rules.length, "routing rules");
    } catch (err) {
      this.rulesError = err instanceof Error ? err.message : String(err);
      console.error("[ConfigStore] Failed to load rules:", this.rulesError);
    } finally {
      this.loadingRules = false;
    }
  }

  /** Get a rule with any pending optimistic changes applied */
  getRuleWithPending(id: number): RoutingRule | undefined {
    const pending = this.pendingRuleChanges.get(id);
    if (pending) {
      return pending.optimistic;
    }
    return this.rules.find((r) => r.id === id);
  }

  /** Update a rule field optimistically */
  updateRuleField<K extends keyof RoutingRule>(
    id: number,
    field: K,
    value: RoutingRule[K]
  ): void {
    const rule = this.rules.find((r) => r.id === id);
    if (!rule) return;

    // Get or create pending change
    let pending = this.pendingRuleChanges.get(id);
    if (!pending) {
      pending = {
        original: { ...rule },
        optimistic: { ...rule },
        timestamp: Date.now(),
      };
    }

    // Apply optimistic update
    pending.optimistic = { ...pending.optimistic, [field]: value };
    pending.timestamp = Date.now();

    // Update the map (triggers reactivity)
    this.pendingRuleChanges = new Map(this.pendingRuleChanges).set(id, pending);

    // Also update the rules array for immediate UI feedback
    this.rules = this.rules.map((r) =>
      r.id === id ? { ...r, [field]: value } : r
    );
  }

  /** Commit pending changes to the backend */
  async commitRuleChange(id: number): Promise<void> {
    const pending = this.pendingRuleChanges.get(id);
    if (!pending) return;

    // Track saving state
    this.savingRules = new Set(this.savingRules).add(id);

    try {
      await invoke("update_routing_rule", { rule: pending.optimistic });
      console.log("[ConfigStore] Saved rule", id);

      // Clear pending on success
      const newPending = new Map(this.pendingRuleChanges);
      newPending.delete(id);
      this.pendingRuleChanges = newPending;
    } catch (err) {
      // Rollback on failure
      this.rules = this.rules.map((r) =>
        r.id === id ? pending.original : r
      );
      const newPending = new Map(this.pendingRuleChanges);
      newPending.delete(id);
      this.pendingRuleChanges = newPending;

      const errMsg = err instanceof Error ? err.message : String(err);
      console.error("[ConfigStore] Failed to save rule:", errMsg);
      this.rulesError = errMsg;
    } finally {
      const newSaving = new Set(this.savingRules);
      newSaving.delete(id);
      this.savingRules = newSaving;
    }
  }

  /** Check if a rule has uncommitted changes */
  hasRuleChanges(id: number): boolean {
    return this.pendingRuleChanges.has(id);
  }

  /** Discard pending changes for a rule */
  discardRuleChanges(id: number): void {
    const pending = this.pendingRuleChanges.get(id);
    if (!pending) return;

    // Rollback to original
    this.rules = this.rules.map((r) =>
      r.id === id ? pending.original : r
    );

    // Clear pending
    const newPending = new Map(this.pendingRuleChanges);
    newPending.delete(id);
    this.pendingRuleChanges = newPending;
  }

  /** Create a new routing rule */
  async createRule(
    pattern: string,
    tag: string,
    priority: number = 0,
    description: string | null = null
  ): Promise<number | null> {
    try {
      const id = await invoke<number>("create_routing_rule", {
        pattern,
        tag,
        priority,
        description,
      });

      // Add to local state
      const newRule: RoutingRule = {
        id,
        pattern,
        tag,
        priority,
        enabled: true,
        description,
      };
      this.rules = [...this.rules, newRule].sort(
        (a, b) => b.priority - a.priority || a.id - b.id
      );

      console.log("[ConfigStore] Created rule", id);
      return id;
    } catch (err) {
      const errMsg = err instanceof Error ? err.message : String(err);
      console.error("[ConfigStore] Failed to create rule:", errMsg);
      this.rulesError = errMsg;
      return null;
    }
  }

  /** Delete a routing rule */
  async deleteRule(id: number): Promise<boolean> {
    // Optimistically remove from UI
    const originalRules = this.rules;
    this.rules = this.rules.filter((r) => r.id !== id);

    try {
      await invoke("delete_routing_rule", { id });
      console.log("[ConfigStore] Deleted rule", id);
      return true;
    } catch (err) {
      // Rollback on failure
      this.rules = originalRules;
      const errMsg = err instanceof Error ? err.message : String(err);
      console.error("[ConfigStore] Failed to delete rule:", errMsg);
      this.rulesError = errMsg;
      return false;
    }
  }

  /** Toggle rule enabled state */
  async toggleRuleEnabled(id: number): Promise<void> {
    const rule = this.rules.find((r) => r.id === id);
    if (!rule) return;

    this.updateRuleField(id, "enabled", !rule.enabled);
    await this.commitRuleChange(id);
  }

  // ============================================================================
  // Topic Configurations
  // ============================================================================

  /** Load all topic configurations from backend */
  async loadTopics(): Promise<void> {
    this.loadingTopics = true;
    this.topicsError = null;

    try {
      this.topics = await invoke<TopicConfig[]>("get_topic_configs");
      console.log("[ConfigStore] Loaded", this.topics.length, "topic configs");
    } catch (err) {
      this.topicsError = err instanceof Error ? err.message : String(err);
      console.error("[ConfigStore] Failed to load topics:", this.topicsError);
    } finally {
      this.loadingTopics = false;
    }
  }

  /** Update a topic's URI */
  async updateTopicUri(id: number, uri: string): Promise<void> {
    // Optimistic update
    const originalTopics = this.topics;
    this.topics = this.topics.map((t) => (t.id === id ? { ...t, uri } : t));

    try {
      await invoke("update_topic_uri", { id, uri });
      console.log("[ConfigStore] Updated topic", id, "URI");
    } catch (err) {
      // Rollback on failure
      this.topics = originalTopics;
      const errMsg = err instanceof Error ? err.message : String(err);
      console.error("[ConfigStore] Failed to update topic URI:", errMsg);
      this.topicsError = errMsg;
    }
  }

  // ============================================================================
  // Computed Properties
  // ============================================================================

  /** Get enabled rules only */
  get enabledRules(): RoutingRule[] {
    return this.rules.filter((r) => r.enabled);
  }

  /** Get rules sorted by priority (highest first) */
  get sortedRules(): RoutingRule[] {
    return [...this.rules].sort(
      (a, b) => b.priority - a.priority || a.id - b.id
    );
  }

  /** Group topics by plugin */
  get topicsByPlugin(): Map<string, TopicConfig[]> {
    const grouped = new Map<string, TopicConfig[]>();
    for (const topic of this.topics) {
      const existing = grouped.get(topic.pluginName) || [];
      existing.push(topic);
      grouped.set(topic.pluginName, existing);
    }
    return grouped;
  }

  /** Check if any rules are being saved */
  get isSaving(): boolean {
    return this.savingRules.size > 0;
  }

  /** Check if any rules have unsaved changes */
  get hasUnsavedChanges(): boolean {
    return this.pendingRuleChanges.size > 0;
  }
}

export const configStore = new ConfigStore();
