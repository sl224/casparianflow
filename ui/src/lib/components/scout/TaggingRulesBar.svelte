<script lang="ts">
  import { scoutStore } from "$lib/stores/scout.svelte";

  interface Props {
    onAddRule: () => void;
    onRemoveRule: (ruleId: string) => void;
  }

  let { onAddRule, onRemoveRule }: Props = $props();

  let expanded = $state(false);
</script>

<div class="rules-bar" class:expanded>
  <button class="toggle-btn" onclick={() => expanded = !expanded}>
    <span class="toggle-icon">{expanded ? "&#9660;" : "&#9654;"}</span>
    <span class="toggle-label">TAGGING RULES</span>
    <span class="rule-count">{scoutStore.taggingRules.length}</span>
  </button>

  {#if expanded}
    <div class="rules-content">
      {#if scoutStore.taggingRules.length === 0}
        <div class="empty-rules">
          <span class="empty-text">No tagging rules defined</span>
          <button class="add-rule-btn" onclick={onAddRule}>
            + Add Rule
          </button>
        </div>
      {:else}
        <div class="rules-list">
          {#each scoutStore.taggingRules as rule (rule.id)}
            <div class="rule-chip">
              <span class="rule-pattern">{rule.pattern}</span>
              <span class="rule-arrow">&#8594;</span>
              <span class="rule-tag">{rule.tag}</span>
              <span class="rule-priority">P{rule.priority}</span>
              <button
                class="remove-rule-btn"
                onclick={() => onRemoveRule(rule.id)}
                title="Remove rule"
              >
                &#10005;
              </button>
            </div>
          {/each}
          <button class="add-rule-chip" onclick={onAddRule}>
            + Add Rule
          </button>
        </div>
      {/if}
    </div>
  {:else}
    <!-- Collapsed: show compact inline view -->
    {#if scoutStore.taggingRules.length > 0}
      <div class="rules-inline">
        {#each scoutStore.taggingRules.slice(0, 3) as rule (rule.id)}
          <span class="rule-mini">
            <span class="mini-pattern">{rule.pattern}</span>
            &#8594;
            <span class="mini-tag">{rule.tag}</span>
          </span>
          <span class="rule-sep">&#8226;</span>
        {/each}
        {#if scoutStore.taggingRules.length > 3}
          <span class="more-rules">+{scoutStore.taggingRules.length - 3} more</span>
        {/if}
      </div>
    {/if}
  {/if}
</div>

<style>
  .rules-bar {
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    overflow: hidden;
  }

  .rules-bar.expanded {
    border-color: var(--color-accent-cyan);
  }

  .toggle-btn {
    display: flex;
    align-items: center;
    gap: var(--space-sm);
    width: 100%;
    padding: var(--space-sm) var(--space-md);
    background: var(--color-bg-tertiary);
    border: none;
    cursor: pointer;
    text-align: left;
  }

  .toggle-btn:hover {
    background: var(--color-bg-primary);
  }

  .toggle-icon {
    font-size: 10px;
    color: var(--color-text-muted);
  }

  .toggle-label {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .rule-count {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-accent-cyan);
    background: rgba(0, 212, 255, 0.15);
    padding: 1px 6px;
    border-radius: 8px;
  }

  .rules-inline {
    display: flex;
    align-items: center;
    gap: var(--space-xs);
    padding: 0 var(--space-md) var(--space-sm);
    flex-wrap: wrap;
  }

  .rule-mini {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-secondary);
  }

  .mini-pattern {
    color: var(--color-accent-cyan);
  }

  .mini-tag {
    color: var(--color-success);
  }

  .rule-sep {
    color: var(--color-text-muted);
    font-size: 8px;
  }

  .rule-sep:last-of-type {
    display: none;
  }

  .more-rules {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
  }

  .rules-content {
    padding: var(--space-md);
    border-top: 1px solid var(--color-border);
  }

  .empty-rules {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-md);
  }

  .empty-text {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-muted);
    font-style: italic;
  }

  .rules-list {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-sm);
    align-items: center;
  }

  .rule-chip {
    display: flex;
    align-items: center;
    gap: var(--space-xs);
    padding: 4px 8px;
    background: var(--color-bg-tertiary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 11px;
  }

  .rule-pattern {
    color: var(--color-accent-cyan);
    background: rgba(0, 212, 255, 0.1);
    padding: 1px 4px;
    border-radius: 2px;
  }

  .rule-arrow {
    color: var(--color-text-muted);
  }

  .rule-tag {
    color: var(--color-success);
    background: rgba(0, 255, 136, 0.1);
    padding: 1px 4px;
    border-radius: 2px;
  }

  .rule-priority {
    color: var(--color-text-muted);
    font-size: 9px;
    background: var(--color-bg-primary);
    padding: 1px 4px;
    border-radius: 2px;
  }

  .remove-rule-btn {
    background: none;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    padding: 0 2px;
    font-size: 10px;
    opacity: 0.5;
  }

  .remove-rule-btn:hover {
    color: var(--color-error);
    opacity: 1;
  }

  .add-rule-btn, .add-rule-chip {
    padding: 4px 10px;
    background: transparent;
    border: 1px dashed var(--color-border);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
    cursor: pointer;
  }

  .add-rule-btn:hover, .add-rule-chip:hover {
    border-color: var(--color-accent-cyan);
    color: var(--color-accent-cyan);
  }
</style>
