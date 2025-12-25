<script lang="ts">
  import { configStore, type RoutingRule } from "$lib/stores/config.svelte";

  // Track which cells are being edited
  let editingCell = $state<{ id: number; field: keyof RoutingRule } | null>(
    null
  );
  let editValue = $state("");
  let inputRef = $state<HTMLInputElement | null>(null);

  // Track selected rows for batch operations
  let selectedRows = $state<Set<number>>(new Set());

  // New rule form state
  let showNewRow = $state(false);
  let newRule = $state({ pattern: "", tag: "", priority: 0, description: "" });

  // Start editing a cell
  function startEdit(
    id: number,
    field: keyof RoutingRule,
    currentValue: string | number
  ) {
    editingCell = { id, field };
    editValue = String(currentValue);
    // Focus input after render
    requestAnimationFrame(() => {
      inputRef?.focus();
      inputRef?.select();
    });
  }

  // Save the current edit
  function saveEdit() {
    if (!editingCell) return;

    const { id, field } = editingCell;
    let value: string | number = editValue;

    // Convert to number for priority field
    if (field === "priority") {
      value = parseInt(editValue, 10) || 0;
    }

    configStore.updateRuleField(id, field, value as never);
    configStore.commitRuleChange(id);
    cancelEdit();
  }

  // Cancel current edit
  function cancelEdit() {
    if (editingCell) {
      configStore.discardRuleChanges(editingCell.id);
    }
    editingCell = null;
    editValue = "";
  }

  // Handle keyboard events in edit mode
  function handleEditKeydown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      saveEdit();
    } else if (e.key === "Escape") {
      e.preventDefault();
      cancelEdit();
    }
  }

  // Global keyboard shortcuts
  function handleKeydown(e: KeyboardEvent) {
    // Cmd/Ctrl+S to save all pending changes
    if ((e.metaKey || e.ctrlKey) && e.key === "s") {
      e.preventDefault();
      for (const id of configStore.pendingRuleChanges.keys()) {
        configStore.commitRuleChange(id);
      }
    }
    // Escape to cancel edit
    if (e.key === "Escape" && editingCell) {
      cancelEdit();
    }
    // Delete selected rows
    if (e.key === "Backspace" && selectedRows.size > 0 && !editingCell) {
      deleteSelected();
    }
  }

  // Toggle row selection
  function toggleSelect(id: number) {
    const newSelected = new Set(selectedRows);
    if (newSelected.has(id)) {
      newSelected.delete(id);
    } else {
      newSelected.add(id);
    }
    selectedRows = newSelected;
  }

  // Delete selected rows
  async function deleteSelected() {
    const ids = Array.from(selectedRows);
    for (const id of ids) {
      await configStore.deleteRule(id);
    }
    selectedRows = new Set();
  }

  // Add new rule
  async function addNewRule() {
    if (!newRule.pattern || !newRule.tag) return;

    await configStore.createRule(
      newRule.pattern,
      newRule.tag,
      newRule.priority,
      newRule.description || null
    );

    // Reset form
    newRule = { pattern: "", tag: "", priority: 0, description: "" };
    showNewRow = false;
  }

  // Check if a specific cell is being edited
  function isEditing(id: number, field: keyof RoutingRule): boolean {
    return editingCell?.id === id && editingCell?.field === field;
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="routing-table">
  <!-- Header -->
  <div class="table-header">
    <div class="header-info">
      <span class="title">Routing Rules</span>
      <span class="count">{configStore.rules.length} rules</span>
      {#if configStore.hasUnsavedChanges}
        <span class="unsaved-badge">unsaved changes</span>
      {/if}
    </div>
    <div class="header-actions">
      {#if selectedRows.size > 0}
        <button class="action-btn danger" onclick={deleteSelected}>
          Delete ({selectedRows.size})
        </button>
      {/if}
      <button
        class="action-btn primary"
        onclick={() => (showNewRow = !showNewRow)}
      >
        {showNewRow ? "Cancel" : "+ Add Rule"}
      </button>
      <button class="action-btn" onclick={() => configStore.loadRules()}>
        Refresh
      </button>
    </div>
  </div>

  <!-- Column Headers -->
  <div class="column-headers">
    <div class="col col-select"></div>
    <div class="col col-priority">Priority</div>
    <div class="col col-pattern">Pattern</div>
    <div class="col col-tag">Tag</div>
    <div class="col col-enabled">Enabled</div>
    <div class="col col-description">Description</div>
    <div class="col col-actions"></div>
  </div>

  <!-- Table Body -->
  <div class="table-body">
    {#if configStore.loadingRules}
      <div class="loading-state">Loading rules...</div>
    {:else if configStore.rules.length === 0 && !showNewRow}
      <div class="empty-state">
        <span class="empty-icon">&#128203;</span>
        <span class="empty-text">No routing rules configured</span>
        <button class="action-btn primary" onclick={() => (showNewRow = true)}>
          + Add First Rule
        </button>
      </div>
    {:else}
      <!-- New Rule Row -->
      {#if showNewRow}
        <div class="table-row new-row">
          <div class="col col-select"></div>
          <div class="col col-priority">
            <input
              type="number"
              class="cell-input"
              placeholder="0"
              bind:value={newRule.priority}
            />
          </div>
          <div class="col col-pattern">
            <input
              type="text"
              class="cell-input"
              placeholder="data/**/*.csv"
              bind:value={newRule.pattern}
            />
          </div>
          <div class="col col-tag">
            <input
              type="text"
              class="cell-input"
              placeholder="finance"
              bind:value={newRule.tag}
            />
          </div>
          <div class="col col-enabled">
            <span class="toggle enabled">ON</span>
          </div>
          <div class="col col-description">
            <input
              type="text"
              class="cell-input"
              placeholder="Optional description"
              bind:value={newRule.description}
            />
          </div>
          <div class="col col-actions">
            <button
              class="save-btn"
              onclick={addNewRule}
              disabled={!newRule.pattern || !newRule.tag}
            >
              Save
            </button>
          </div>
        </div>
      {/if}

      <!-- Existing Rules -->
      {#each configStore.sortedRules as rule (rule.id)}
        {@const isSaving = configStore.savingRules.has(rule.id)}
        {@const hasChanges = configStore.hasRuleChanges(rule.id)}
        <div
          class="table-row"
          class:selected={selectedRows.has(rule.id)}
          class:saving={isSaving}
          class:disabled={!rule.enabled}
        >
          <!-- Select checkbox -->
          <div class="col col-select">
            <input
              type="checkbox"
              checked={selectedRows.has(rule.id)}
              onchange={() => toggleSelect(rule.id)}
            />
          </div>

          <!-- Priority -->
          <div class="col col-priority">
            {#if isEditing(rule.id, "priority")}
              <input
                type="number"
                class="cell-input"
                bind:this={inputRef}
                bind:value={editValue}
                onkeydown={handleEditKeydown}
                onblur={saveEdit}
              />
            {:else}
              <button
                class="editable-cell"
                onclick={() => startEdit(rule.id, "priority", rule.priority)}
              >
                {rule.priority}
              </button>
            {/if}
          </div>

          <!-- Pattern -->
          <div class="col col-pattern">
            {#if isEditing(rule.id, "pattern")}
              <input
                type="text"
                class="cell-input"
                bind:this={inputRef}
                bind:value={editValue}
                onkeydown={handleEditKeydown}
                onblur={saveEdit}
              />
            {:else}
              <button
                class="editable-cell pattern"
                onclick={() => startEdit(rule.id, "pattern", rule.pattern)}
                title={rule.pattern}
              >
                {rule.pattern}
              </button>
            {/if}
          </div>

          <!-- Tag -->
          <div class="col col-tag">
            {#if isEditing(rule.id, "tag")}
              <input
                type="text"
                class="cell-input"
                bind:this={inputRef}
                bind:value={editValue}
                onkeydown={handleEditKeydown}
                onblur={saveEdit}
              />
            {:else}
              <button
                class="editable-cell tag-badge"
                onclick={() => startEdit(rule.id, "tag", rule.tag)}
              >
                {rule.tag}
              </button>
            {/if}
          </div>

          <!-- Enabled Toggle -->
          <div class="col col-enabled">
            <button
              class="toggle"
              class:enabled={rule.enabled}
              onclick={() => configStore.toggleRuleEnabled(rule.id)}
              disabled={isSaving}
            >
              {rule.enabled ? "ON" : "OFF"}
            </button>
          </div>

          <!-- Description -->
          <div class="col col-description">
            {#if isEditing(rule.id, "description")}
              <input
                type="text"
                class="cell-input"
                bind:this={inputRef}
                bind:value={editValue}
                onkeydown={handleEditKeydown}
                onblur={saveEdit}
              />
            {:else}
              <button
                class="editable-cell description"
                onclick={() =>
                  startEdit(rule.id, "description", rule.description || "")}
                title={rule.description || ""}
              >
                {rule.description || "--"}
              </button>
            {/if}
          </div>

          <!-- Actions -->
          <div class="col col-actions">
            {#if hasChanges}
              <button class="save-btn" onclick={() => saveEdit()}>Save</button>
            {/if}
            <button
              class="delete-btn"
              onclick={() => configStore.deleteRule(rule.id)}
              title="Delete rule"
            >
              &times;
            </button>
          </div>
        </div>
      {/each}
    {/if}
  </div>

  <!-- Footer -->
  <div class="table-footer">
    <span class="hint">
      Click to edit | Enter to save | Escape to cancel | Cmd+S to save all
    </span>
    {#if configStore.rulesError}
      <span class="error">{configStore.rulesError}</span>
    {/if}
  </div>
</div>

<style>
  .routing-table {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    overflow: hidden;
  }

  /* Header */
  .table-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 12px 16px;
    background: var(--color-bg-secondary);
    border-bottom: 1px solid var(--color-border);
    flex-shrink: 0;
  }

  .header-info {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .title {
    font-family: var(--font-mono);
    font-size: 14px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .count {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-muted);
  }

  .unsaved-badge {
    padding: 2px 8px;
    background: rgba(255, 170, 0, 0.15);
    border-radius: 10px;
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-warning);
  }

  .header-actions {
    display: flex;
    gap: 8px;
  }

  .action-btn {
    padding: 6px 12px;
    background: var(--color-bg-tertiary);
    border: 1px solid var(--color-border);
    border-radius: 4px;
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-secondary);
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .action-btn:hover {
    border-color: var(--color-text-muted);
    color: var(--color-text-primary);
  }

  .action-btn.primary {
    background: var(--color-accent-cyan);
    border-color: var(--color-accent-cyan);
    color: var(--color-bg-primary);
  }

  .action-btn.primary:hover {
    background: var(--color-accent-green);
    border-color: var(--color-accent-green);
  }

  .action-btn.danger {
    background: rgba(255, 51, 85, 0.1);
    border-color: var(--color-error);
    color: var(--color-error);
  }

  .action-btn.danger:hover {
    background: var(--color-error);
    color: white;
  }

  /* Column Headers */
  .column-headers {
    display: flex;
    background: var(--color-bg-tertiary);
    border-bottom: 1px solid var(--color-border);
    flex-shrink: 0;
  }

  .column-headers .col {
    padding: 10px 12px;
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 600;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  /* Column Widths */
  .col-select {
    width: 40px;
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .col-priority {
    width: 80px;
    flex-shrink: 0;
    text-align: right;
  }

  .col-pattern {
    flex: 2;
    min-width: 200px;
  }

  .col-tag {
    width: 120px;
    flex-shrink: 0;
  }

  .col-enabled {
    width: 80px;
    flex-shrink: 0;
    text-align: center;
  }

  .col-description {
    flex: 1;
    min-width: 150px;
  }

  .col-actions {
    width: 80px;
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 4px;
    padding-right: 12px;
  }

  /* Table Body */
  .table-body {
    flex: 1;
    overflow-y: auto;
    overflow-x: auto;
  }

  .table-row {
    display: flex;
    border-bottom: 1px solid var(--color-border);
    transition: background 0.1s ease;
  }

  .table-row:hover {
    background: var(--color-bg-tertiary);
  }

  .table-row.selected {
    background: rgba(0, 212, 255, 0.08);
  }

  .table-row.saving {
    opacity: 0.6;
    pointer-events: none;
  }

  .table-row.disabled {
    opacity: 0.5;
  }

  .table-row.new-row {
    background: rgba(0, 255, 136, 0.05);
    border-bottom: 2px solid var(--color-accent-green);
  }

  .table-row .col {
    padding: 8px 12px;
    display: flex;
    align-items: center;
  }

  /* Editable Cells */
  .editable-cell {
    width: 100%;
    padding: 4px 8px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 4px;
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-secondary);
    text-align: left;
    cursor: pointer;
    transition: all 0.1s ease;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .editable-cell:hover {
    background: var(--color-bg-tertiary);
    border-color: var(--color-border);
  }

  .editable-cell.pattern {
    color: var(--color-accent-cyan);
    font-weight: 500;
  }

  .editable-cell.tag-badge {
    display: inline-block;
    width: auto;
    padding: 2px 10px;
    background: rgba(0, 255, 136, 0.1);
    border-radius: 12px;
    color: var(--color-accent-green);
    font-weight: 600;
    font-size: 11px;
    text-align: center;
  }

  .editable-cell.description {
    color: var(--color-text-muted);
    font-style: italic;
  }

  .col-priority .editable-cell {
    text-align: right;
    font-variant-numeric: tabular-nums;
    color: var(--color-text-primary);
    font-weight: 600;
  }

  /* Cell Input */
  .cell-input {
    width: 100%;
    padding: 4px 8px;
    background: var(--color-bg-primary);
    border: 1px solid var(--color-accent-cyan);
    border-radius: 4px;
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-primary);
    outline: none;
  }

  .cell-input:focus {
    box-shadow: 0 0 0 2px rgba(0, 212, 255, 0.2);
  }

  .col-priority .cell-input {
    text-align: right;
  }

  /* Toggle Switch */
  .toggle {
    padding: 3px 10px;
    border-radius: 12px;
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 600;
    border: none;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .toggle.enabled {
    background: rgba(0, 255, 136, 0.15);
    color: var(--color-accent-green);
  }

  .toggle:not(.enabled) {
    background: var(--color-bg-tertiary);
    color: var(--color-text-muted);
  }

  .toggle:hover {
    transform: scale(1.05);
  }

  .toggle:disabled {
    cursor: not-allowed;
    opacity: 0.5;
  }

  /* Action Buttons */
  .save-btn {
    padding: 3px 8px;
    background: var(--color-accent-green);
    border: none;
    border-radius: 4px;
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 600;
    color: var(--color-bg-primary);
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .save-btn:hover:not(:disabled) {
    transform: translateY(-1px);
  }

  .save-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .delete-btn {
    width: 20px;
    height: 20px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 4px;
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 14px;
    transition: all 0.15s ease;
  }

  .delete-btn:hover {
    border-color: var(--color-error);
    color: var(--color-error);
    background: rgba(255, 51, 85, 0.1);
  }

  /* Empty State */
  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 48px;
  }

  .empty-icon {
    font-size: 48px;
    opacity: 0.3;
  }

  .empty-text {
    font-family: var(--font-mono);
    font-size: 14px;
    color: var(--color-text-muted);
  }

  /* Loading State */
  .loading-state {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100px;
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-muted);
  }

  /* Footer */
  .table-footer {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 8px 16px;
    background: var(--color-bg-secondary);
    border-top: 1px solid var(--color-border);
    flex-shrink: 0;
  }

  .hint {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
  }

  .error {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-error);
  }

  /* Checkbox */
  input[type="checkbox"] {
    width: 14px;
    height: 14px;
    cursor: pointer;
    accent-color: var(--color-accent-cyan);
  }
</style>
