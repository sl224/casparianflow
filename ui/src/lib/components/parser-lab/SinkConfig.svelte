<script lang="ts">
  interface Props {
    sinkType: string;
    configJson: string | null;
    parserName: string;
    onChange: (config: string) => void;
  }

  let { sinkType, configJson, parserName, onChange }: Props = $props();

  // Parquet config
  interface ParquetConfig {
    outputDir: string;
    compression: string;
    partitionBy: string | null;
    rowGroupSize: number | null;
  }

  // SQLite config
  interface SqliteConfig {
    databasePath: string;
    tableName: string;
    createIfNotExists: boolean;
    writeMode: string;
  }

  // CSV config
  interface CsvConfig {
    outputDir: string;
    delimiter: string;
    includeHeader: boolean;
    quoteAll: boolean;
  }

  // Parse config from JSON
  function parseConfig<T>(defaultConfig: T): T {
    if (!configJson) return defaultConfig;
    try {
      return { ...defaultConfig, ...JSON.parse(configJson) };
    } catch {
      return defaultConfig;
    }
  }

  // Default configs
  const defaultParquetConfig: ParquetConfig = {
    outputDir: `~/.casparian_flow/output/${sanitizeName(parserName)}/`,
    compression: "snappy",
    partitionBy: null,
    rowGroupSize: null,
  };

  const defaultSqliteConfig: SqliteConfig = {
    databasePath: "~/.casparian_flow/output/analytics.db",
    tableName: sanitizeName(parserName),
    createIfNotExists: true,
    writeMode: "append",
  };

  const defaultCsvConfig: CsvConfig = {
    outputDir: `~/.casparian_flow/output/${sanitizeName(parserName)}/`,
    delimiter: ",",
    includeHeader: true,
    quoteAll: false,
  };

  // Get current config based on sink type
  let parquetConfig = $derived(parseConfig(defaultParquetConfig));
  let sqliteConfig = $derived(parseConfig(defaultSqliteConfig));
  let csvConfig = $derived(parseConfig(defaultCsvConfig));

  function sanitizeName(name: string): string {
    return name.toLowerCase().replace(/[^a-z0-9]/g, "_").replace(/_+/g, "_");
  }

  function updateParquetConfig(updates: Partial<ParquetConfig>) {
    const config = { ...parquetConfig, ...updates };
    onChange(JSON.stringify(config));
  }

  function updateSqliteConfig(updates: Partial<SqliteConfig>) {
    const config = { ...sqliteConfig, ...updates };
    onChange(JSON.stringify(config));
  }

  function updateCsvConfig(updates: Partial<CsvConfig>) {
    const config = { ...csvConfig, ...updates };
    onChange(JSON.stringify(config));
  }
</script>

<div class="sink-config">
  {#if sinkType === "parquet"}
    <!-- Parquet Configuration -->
    <div class="config-grid">
      <label>
        <span>Output Directory:</span>
        <input
          type="text"
          value={parquetConfig.outputDir}
          onchange={(e) => updateParquetConfig({ outputDir: e.currentTarget.value })}
          placeholder="~/.casparian_flow/output/"
        />
      </label>
      <label>
        <span>Compression:</span>
        <select
          value={parquetConfig.compression}
          onchange={(e) => updateParquetConfig({ compression: e.currentTarget.value })}
        >
          <option value="snappy">Snappy (fast, moderate)</option>
          <option value="gzip">GZIP (slow, best)</option>
          <option value="lz4">LZ4 (fastest, less)</option>
          <option value="none">None</option>
        </select>
      </label>
      <label>
        <span>Partition By:</span>
        <input
          type="text"
          value={parquetConfig.partitionBy || ""}
          onchange={(e) => updateParquetConfig({ partitionBy: e.currentTarget.value || null })}
          placeholder="Column name (optional)"
        />
      </label>
    </div>

  {:else if sinkType === "sqlite"}
    <!-- SQLite Configuration -->
    <div class="config-grid">
      <label>
        <span>Database:</span>
        <input
          type="text"
          value={sqliteConfig.databasePath}
          onchange={(e) => updateSqliteConfig({ databasePath: e.currentTarget.value })}
          placeholder="~/.casparian_flow/output/data.db"
        />
      </label>
      <label>
        <span>Table Name:</span>
        <input
          type="text"
          value={sqliteConfig.tableName}
          onchange={(e) => updateSqliteConfig({ tableName: e.currentTarget.value })}
          placeholder="table_name"
        />
      </label>
      <label>
        <span>Write Mode:</span>
        <select
          value={sqliteConfig.writeMode}
          onchange={(e) => updateSqliteConfig({ writeMode: e.currentTarget.value })}
        >
          <option value="append">Append</option>
          <option value="replace">Replace (drop & recreate)</option>
          <option value="fail">Fail if exists</option>
        </select>
      </label>
      <label class="checkbox-label">
        <input
          type="checkbox"
          checked={sqliteConfig.createIfNotExists}
          onchange={(e) => updateSqliteConfig({ createIfNotExists: e.currentTarget.checked })}
        />
        <span>Create table if not exists</span>
      </label>
    </div>

  {:else if sinkType === "csv"}
    <!-- CSV Configuration -->
    <div class="config-grid">
      <label>
        <span>Output Directory:</span>
        <input
          type="text"
          value={csvConfig.outputDir}
          onchange={(e) => updateCsvConfig({ outputDir: e.currentTarget.value })}
          placeholder="~/.casparian_flow/output/"
        />
      </label>
      <label>
        <span>Delimiter:</span>
        <select
          value={csvConfig.delimiter}
          onchange={(e) => updateCsvConfig({ delimiter: e.currentTarget.value })}
        >
          <option value=",">Comma (,)</option>
          <option value="&#9;">Tab (\t)</option>
          <option value="|">Pipe (|)</option>
        </select>
      </label>
      <label class="checkbox-label">
        <input
          type="checkbox"
          checked={csvConfig.includeHeader}
          onchange={(e) => updateCsvConfig({ includeHeader: e.currentTarget.checked })}
        />
        <span>Include header row</span>
      </label>
      <label class="checkbox-label">
        <input
          type="checkbox"
          checked={csvConfig.quoteAll}
          onchange={(e) => updateCsvConfig({ quoteAll: e.currentTarget.checked })}
        />
        <span>Quote all fields</span>
      </label>
    </div>
  {/if}
</div>

<style>
  .sink-config {
    margin-top: 0.75rem;
  }

  .config-grid {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.8rem;
  }

  label span {
    color: var(--color-text-secondary);
    min-width: 100px;
    flex-shrink: 0;
  }

  input[type="text"],
  select {
    flex: 1;
    padding: 0.375rem 0.5rem;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-bg-tertiary);
    color: var(--color-text-primary);
    font-size: 0.8rem;
    font-family: var(--font-mono);
  }

  input[type="text"]:focus,
  select:focus {
    outline: none;
    border-color: var(--color-accent-cyan);
  }

  .checkbox-label {
    flex-direction: row-reverse;
    justify-content: flex-end;
    gap: 0.5rem;
  }

  .checkbox-label span {
    min-width: auto;
  }

  input[type="checkbox"] {
    width: 14px;
    height: 14px;
    accent-color: var(--color-accent-cyan);
  }
</style>
