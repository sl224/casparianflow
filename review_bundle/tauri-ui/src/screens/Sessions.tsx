import { useState, useEffect } from 'react'
import { useNavigate } from 'react-router-dom'
import { sessionList, sessionCreate, isTauri } from '../api'
import type { SessionSummary } from '../api'

// Intent state labels and colors
const stateConfig: Record<string, { label: string; color: string }> = {
  S0_INITIATED: { label: 'Initiated', color: 'var(--muted-foreground)' },
  S1_SCANNED: { label: 'Scanned', color: 'var(--primary)' },
  S2_SELECTION_PROPOSED: { label: 'Selection Proposed', color: 'var(--warning-foreground)' },
  G1_SELECTION_APPROVED: { label: 'Selection Approved', color: 'var(--success-foreground)' },
  S3_TAG_RULES_PROPOSED: { label: 'Tag Rules Proposed', color: 'var(--warning-foreground)' },
  G2_TAG_RULES_APPROVED: { label: 'Tags Approved', color: 'var(--success-foreground)' },
  S4_PATH_FIELDS_PROPOSED: { label: 'Path Fields Proposed', color: 'var(--warning-foreground)' },
  G3_PATH_FIELDS_APPROVED: { label: 'Path Fields Approved', color: 'var(--success-foreground)' },
  S5_SCHEMA_INTENT_PROPOSED: { label: 'Schema Proposed', color: 'var(--warning-foreground)' },
  G4_SCHEMA_INTENT_APPROVED: { label: 'Schema Approved', color: 'var(--success-foreground)' },
  S6_GENERATE_PARSER_DRAFT: { label: 'Parser Draft', color: 'var(--primary)' },
  S7_BACKTEST_FAIL_FAST: { label: 'Backtest (Fail-Fast)', color: 'var(--primary)' },
  S8_BACKTEST_FULL: { label: 'Backtest (Full)', color: 'var(--primary)' },
  S9_PUBLISH_PLAN_PROPOSED: { label: 'Publish Planned', color: 'var(--warning-foreground)' },
  G5_PUBLISH_APPROVED: { label: 'Publish Approved', color: 'var(--success-foreground)' },
  S10_PUBLISHING: { label: 'Publishing', color: 'var(--primary)' },
  S11_RUN_PLAN_PROPOSED: { label: 'Run Planned', color: 'var(--warning-foreground)' },
  G6_RUN_APPROVED: { label: 'Run Approved', color: 'var(--success-foreground)' },
  S12_COMPLETE: { label: 'Complete', color: 'var(--success-foreground)' },
}

// Mock data for development without Tauri
const mockSessions: SessionSummary[] = [
  {
    id: 'a1b2c3d4-e5f6-7890-abcd-ef1234567890',
    intent: 'Process all sales CSV files from Q4',
    state: 'S5_SCHEMA_INTENT_PROPOSED',
    filesSelected: 247,
    createdAt: '2 hours ago',
    hasQuestion: true,
  },
  {
    id: 'b2c3d4e5-f6a7-8901-bcde-f12345678901',
    intent: 'Ingest auth logs for security analysis',
    state: 'S7_BACKTEST_FAIL_FAST',
    filesSelected: 1842,
    createdAt: '5 hours ago',
    hasQuestion: false,
  },
  {
    id: 'c3d4e5f6-a7b8-9012-cdef-123456789012',
    intent: 'Parse trading records for compliance',
    state: 'G4_SCHEMA_INTENT_APPROVED',
    filesSelected: 56,
    createdAt: '1 day ago',
    hasQuestion: false,
  },
  {
    id: 'd4e5f6a7-b8c9-0123-defa-234567890123',
    intent: 'Convert legacy inventory exports',
    state: 'S2_SELECTION_PROPOSED',
    filesSelected: 0,
    createdAt: '2 days ago',
    hasQuestion: true,
  },
  {
    id: 'e5f6a7b8-c9d0-1234-efab-345678901234',
    intent: 'Process customer feedback forms',
    state: 'S12_COMPLETE',
    filesSelected: 89,
    createdAt: '3 days ago',
    hasQuestion: false,
  },
]

export default function Sessions() {
  const navigate = useNavigate()
  const [sessions, setSessions] = useState<SessionSummary[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [stateFilter, setStateFilter] = useState<string>('')

  const fetchData = async () => {
    try {
      if (isTauri()) {
        const data = await sessionList()
        setSessions(data)
      } else {
        // Use mock data in browser development
        setSessions(mockSessions)
      }
      setError(null)
    } catch (err) {
      console.error('Failed to fetch sessions:', err)
      setError(err instanceof Error ? err.message : 'Unknown error')
      setSessions(mockSessions)
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    fetchData()
  }, [])

  // Calculate stats from sessions
  const stats = [
    {
      label: 'Active',
      value: String(sessions.filter(s => !s.state.includes('COMPLETE') && !s.state.includes('APPROVED')).length),
      description: 'In progress',
      color: 'var(--primary)'
    },
    {
      label: 'Awaiting',
      value: String(sessions.filter(s => s.hasQuestion || s.state.includes('PROPOSED')).length),
      description: 'Need approval',
      color: 'var(--warning-foreground)'
    },
    {
      label: 'Complete',
      value: String(sessions.filter(s => s.state === 'S12_COMPLETE').length),
      description: 'This month',
      color: 'var(--success-foreground)'
    },
    {
      label: 'Failed',
      value: '0',
      description: 'Needs attention',
      color: 'var(--destructive)'
    },
  ]

  const getStateInfo = (state: string) => {
    return stateConfig[state] || { label: state, color: 'var(--muted-foreground)' }
  }

  const handleNewSession = async () => {
    if (isTauri()) {
      try {
        const response = await sessionCreate({ intent: 'New session' })
        navigate(`/sessions/${response.sessionId}`)
      } catch (err) {
        console.error('Failed to create session:', err)
      }
    } else {
      navigate('/sessions/new')
    }
  }

  const handleSessionClick = (sessionId: string) => {
    navigate(`/sessions/${sessionId}`)
  }

  return (
    <main className="main-content" data-testid="sessions-screen">
      <div className="page-header">
        <div>
          <h1 className="page-title">Intent Sessions</h1>
          <p className="page-subtitle">Manage data pipeline workflows</p>
        </div>
        <div className="header-actions">
          <button
            className="btn btn-outline"
            onClick={() => { setLoading(true); fetchData() }}
            disabled={loading}
            data-testid="refresh-btn"
          >
            <span className="material-symbols-sharp" style={{ fontSize: 18 }}>refresh</span>
            {loading ? 'Loading...' : 'Refresh'}
          </button>
          <button className="btn btn-primary" onClick={handleNewSession} data-testid="new-session-btn">
            <span className="material-symbols-sharp" style={{ fontSize: 18 }}>add</span>
            New Session
          </button>
        </div>
      </div>

      {error && (
        <div className="alert alert-warning" style={{ marginBottom: 16 }}>
          Using cached data: {error}
        </div>
      )}

      <div className="stats-row" data-testid="session-stats">
        {stats.map((stat) => (
          <div key={stat.label} className="stat-card" data-testid={`stat-${stat.label.toLowerCase()}`}>
            <div className="stat-label">{stat.label}</div>
            <div className="stat-value" style={{ color: stat.color }}>{stat.value}</div>
            <div className="stat-description">{stat.description}</div>
          </div>
        ))}
      </div>

      <div className="card" style={{ flex: 1 }}>
        <div className="card-header">
          <span className="card-title">Recent Sessions</span>
          <div style={{ display: 'flex', gap: 8 }}>
            <select
              className="filter-select"
              data-testid="state-filter"
              value={stateFilter}
              onChange={(e) => setStateFilter(e.target.value)}
            >
              <option value="">All States</option>
              <option value="active">Active</option>
              <option value="awaiting">Awaiting Approval</option>
              <option value="complete">Complete</option>
            </select>
          </div>
        </div>

        <div className="table-header">
          <span style={{ flex: 2 }}>Intent</span>
          <span style={{ width: 160 }}>State</span>
          <span style={{ width: 100 }}>Files</span>
          <span style={{ width: 120 }}>Created</span>
          <span style={{ width: 100, textAlign: 'right' }}>Actions</span>
        </div>

        <div data-testid="sessions-list">
          {sessions.filter(session => {
            if (!stateFilter) return true
            if (stateFilter === 'active') return !session.state.includes('COMPLETE') && !session.state.includes('APPROVED')
            if (stateFilter === 'awaiting') return session.hasQuestion || session.state.includes('PROPOSED')
            if (stateFilter === 'complete') return session.state === 'S12_COMPLETE'
            return true
          }).map((session, index) => {
            const stateInfo = getStateInfo(session.state)
            return (
              <div
                key={session.id}
                className="table-row table-row-clickable"
                data-testid={`session-row-${index}`}
                onClick={() => handleSessionClick(session.id)}
              >
                <div style={{ flex: 2, display: 'flex', alignItems: 'center', gap: 8 }}>
                  <span className="table-cell">{session.intent}</span>
                  {session.hasQuestion && (
                    <span
                      className="badge badge-warning"
                      title="Requires human input"
                      data-testid={`question-badge-${index}`}
                    >
                      <span className="material-symbols-sharp" style={{ fontSize: 14 }}>help</span>
                    </span>
                  )}
                </div>
                <span className="table-cell" style={{ width: 160 }}>
                  <span className="status-badge" style={{ color: stateInfo.color }}>
                    {stateInfo.label}
                  </span>
                </span>
                <span className="table-cell-mono" style={{ width: 100 }}>
                  {session.filesSelected > 0 ? session.filesSelected.toLocaleString() : '-'}
                </span>
                <span className="table-cell text-muted" style={{ width: 120 }}>
                  {session.createdAt}
                </span>
                <div style={{ width: 100, display: 'flex', justifyContent: 'flex-end' }}>
                  <button
                    className="btn btn-outline btn-sm"
                    onClick={(e) => {
                      e.stopPropagation()
                      handleSessionClick(session.id)
                    }}
                    data-testid={`open-btn-${index}`}
                  >
                    Open
                  </button>
                </div>
              </div>
            )
          })}
          {sessions.length === 0 && !loading && (
            <div className="table-row text-muted" style={{ justifyContent: 'center' }}>
              No sessions yet. Click "New Session" to get started.
            </div>
          )}
        </div>
      </div>
    </main>
  )
}
