import { useState } from 'react'
import { queryExecute, isTauri } from '../api'
import type { QueryResult } from '../api'

// Mock result for development without Tauri
const mockResult: QueryResult = {
  rowCount: 247,
  execTimeMs: 23,
  columns: ['output_name', 'row_count', 'total_bytes'],
  rows: [
    ['fix_order_lifecycle', 1247832, '2.4 GB'],
    ['fix_executions', 420156, '856 MB'],
    ['hl7_observations', 89421, '142 MB'],
    ['syslog_events', 56789, '98 MB'],
    ['csv_trades', 12345, '24 MB'],
  ],
}

const defaultSql = `SELECT
  plugin_name AS output_name,
  COUNT(*) as row_count,
  result_rows_processed as total_bytes
FROM cf_api_jobs
WHERE status = 'completed'
GROUP BY plugin_name, result_rows_processed
ORDER BY row_count DESC
LIMIT 10;`

export default function Query() {
  const [sql, setSql] = useState(defaultSql)
  const [results, setResults] = useState<QueryResult | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const handleRunQuery = async () => {
    setLoading(true)
    setError(null)

    try {
      if (isTauri()) {
        const result = await queryExecute({ sql, limit: 1000 })
        setResults(result)
      } else {
        // Use mock data in browser development
        await new Promise(resolve => setTimeout(resolve, 100)) // Simulate delay
        setResults(mockResult)
      }
    } catch (err) {
      console.error('Query failed:', err)
      setError(err instanceof Error ? err.message : 'Query execution failed')
      setResults(null)
    } finally {
      setLoading(false)
    }
  }

  // Format cell value for display
  const formatCell = (value: unknown): string => {
    if (value === null || value === undefined) return 'NULL'
    if (typeof value === 'number') return value.toLocaleString()
    return String(value)
  }

  return (
    <main className="main-content" data-testid="query-screen">
      <div className="page-header">
        <div>
          <h1 className="page-title">Query Console</h1>
          <p className="page-subtitle">Run SQL queries on output data</p>
        </div>
        <div className="header-actions">
          <button className="btn btn-outline" onClick={handleRunQuery}>Refresh</button>
        </div>
      </div>

      <div className="content-column" style={{ flex: 1, gap: 16 }}>
        <div className="card">
          <div className="card-header">
            <span className="card-title">SQL Query</span>
            <button
              className="btn btn-primary"
              onClick={handleRunQuery}
              disabled={loading}
              data-testid="run-query-btn"
            >
              <span className="material-symbols-sharp" style={{ fontSize: 16 }}>play_arrow</span>
              {loading ? 'Running...' : 'Run Query'}
            </button>
          </div>
          <textarea
            className="sql-editor"
            value={sql}
            onChange={(e) => setSql(e.target.value)}
            data-testid="sql-editor"
            style={{
              width: '100%',
              border: 'none',
              outline: 'none',
              resize: 'vertical',
            }}
          />
        </div>

        {error && (
          <div className="alert alert-error" data-testid="query-error">
            <span className="material-symbols-sharp" style={{ fontSize: 18, marginRight: 8 }}>error</span>
            {error}
          </div>
        )}

        <div className="card" style={{ flex: 1 }}>
          <div className="card-header">
            <span className="card-title">Results</span>
            {results && (
              <div style={{ display: 'flex', gap: 16, alignItems: 'center' }}>
                <span className="text-muted" style={{ fontSize: 12 }} data-testid="row-count">
                  {results.rowCount} rows
                </span>
                <span className="text-success table-cell-mono" style={{ fontSize: 12 }} data-testid="exec-time">
                  {results.execTimeMs}ms
                </span>
              </div>
            )}
          </div>

          {results ? (
            <>
              <div className="table-header">
                {results.columns.map((col, i) => (
                  <span
                    key={col}
                    style={{
                      width: i === 0 ? 200 : i === 1 ? 120 : undefined,
                      flex: i === results.columns.length - 1 ? 1 : undefined,
                      textAlign: i > 0 ? 'right' : undefined,
                    }}
                  >
                    {col}
                  </span>
                ))}
              </div>

              <div data-testid="results-table">
                {results.rows.map((row, rowIndex) => (
                  <div key={rowIndex} className="table-row" data-testid={`result-row-${rowIndex}`}>
                    {row.map((cell, cellIndex) => (
                      <span
                        key={cellIndex}
                        className="table-cell-mono"
                        style={{
                          width: cellIndex === 0 ? 200 : cellIndex === 1 ? 120 : undefined,
                          flex: cellIndex === results.columns.length - 1 ? 1 : undefined,
                          textAlign: cellIndex > 0 ? 'right' : undefined,
                        }}
                      >
                        {formatCell(cell)}
                      </span>
                    ))}
                  </div>
                ))}
                {results.rows.length === 0 && (
                  <div className="table-row text-muted" style={{ justifyContent: 'center' }}>
                    No results
                  </div>
                )}
              </div>
            </>
          ) : (
            <div style={{ padding: 40, textAlign: 'center' }} className="text-muted">
              {loading ? 'Running query...' : 'Run a query to see results'}
            </div>
          )}
        </div>
      </div>
    </main>
  )
}
