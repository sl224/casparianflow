import { useState, useEffect } from 'react'
import { isTauri, safeInvoke } from '../api'

interface Parser {
  name: string
  version: string
  topics: string[]
  createdAt: string
  status: string
}

// Mock data for development
const mockParsers: Parser[] = [
  { name: 'evtx_parser', version: '1.2.0', topics: ['security_logs'], createdAt: '2024-01-10', status: 'active' },
  { name: 'fix_parser', version: '2.0.1', topics: ['fix_messages'], createdAt: '2024-01-08', status: 'active' },
  { name: 'hl7_parser', version: '1.5.0', topics: ['healthcare'], createdAt: '2024-01-05', status: 'active' },
  { name: 'csv_generic', version: '0.3.0', topics: ['csv_data'], createdAt: '2024-01-02', status: 'deprecated' },
]

export default function Parsers() {
  const [parsers, setParsers] = useState<Parser[]>([])
  const [loading, setLoading] = useState(true)
  const [filter, setFilter] = useState<string>('all')

  useEffect(() => {
    if (isTauri()) {
      safeInvoke<Parser[]>('parser_list').then(data => {
        setParsers(data || mockParsers)
      }).catch(() => {
        setParsers(mockParsers)
      }).finally(() => {
        setLoading(false)
      })
    } else {
      setParsers(mockParsers)
      setLoading(false)
    }
  }, [])

  const filteredParsers = filter === 'all'
    ? parsers
    : parsers.filter(p => p.status === filter)

  const statusColors: Record<string, string> = {
    active: 'var(--success-foreground)',
    deprecated: 'var(--warning-foreground)',
    draft: 'var(--muted-foreground)',
  }

  return (
    <main className="main-content" data-testid="parsers-screen">
      <div className="page-header">
        <div>
          <h1 className="page-title">Parsers</h1>
          <p className="page-subtitle">Manage registered parsers and their versions</p>
        </div>
        <div className="header-actions">
          <select
            className="filter-select"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
          >
            <option value="all">All Status</option>
            <option value="active">Active</option>
            <option value="deprecated">Deprecated</option>
            <option value="draft">Draft</option>
          </select>
        </div>
      </div>

      <div className="stats-row" data-testid="stats-row">
        <div className="stat-card">
          <div className="stat-label">Total Parsers</div>
          <div className="stat-value">{parsers.length}</div>
        </div>
        <div className="stat-card">
          <div className="stat-label">Active</div>
          <div className="stat-value" style={{ color: 'var(--success-foreground)' }}>
            {parsers.filter(p => p.status === 'active').length}
          </div>
        </div>
        <div className="stat-card">
          <div className="stat-label">Deprecated</div>
          <div className="stat-value" style={{ color: 'var(--warning-foreground)' }}>
            {parsers.filter(p => p.status === 'deprecated').length}
          </div>
        </div>
      </div>

      <div className="card" style={{ flex: 1 }}>
        <div className="card-header">
          <span className="card-title">Registered Parsers</span>
          <span className="text-muted" style={{ fontSize: 12 }}>{filteredParsers.length} parsers</span>
        </div>

        <div className="table-header">
          <span style={{ flex: 1 }}>Name</span>
          <span style={{ width: 100 }}>Version</span>
          <span style={{ width: 150 }}>Topics</span>
          <span style={{ width: 100 }}>Status</span>
          <span style={{ width: 120 }}>Created</span>
        </div>

        {loading ? (
          <div className="table-row text-muted">Loading...</div>
        ) : filteredParsers.length === 0 ? (
          <div className="table-row text-muted">No parsers found</div>
        ) : (
          filteredParsers.map((parser) => (
            <div key={`${parser.name}-${parser.version}`} className="table-row table-row-clickable">
              <span style={{ flex: 1 }}>
                <span className="table-cell-mono">{parser.name}</span>
              </span>
              <span className="table-cell-mono" style={{ width: 100 }}>v{parser.version}</span>
              <span style={{ width: 150 }}>
                {parser.topics.map(t => (
                  <span key={t} className="tag-badge" style={{ marginRight: 4 }}>{t}</span>
                ))}
              </span>
              <span style={{ width: 100 }}>
                <span
                  className="badge"
                  style={{
                    background: `${statusColors[parser.status]}20`,
                    color: statusColors[parser.status],
                  }}
                >
                  {parser.status}
                </span>
              </span>
              <span className="text-muted" style={{ width: 120, fontSize: 12 }}>{parser.createdAt}</span>
            </div>
          ))
        )}
      </div>
    </main>
  )
}
