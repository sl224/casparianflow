import { useState, useEffect } from 'react'
import { useNavigate } from 'react-router-dom'
import { dashboardStats, isTauri } from '../api'
import type { DashboardStats, OutputInfo, ActiveRun } from '../api'

// Mock data for development without Tauri
const mockStats: DashboardStats = {
  readyOutputs: 12,
  runningJobs: 3,
  quarantinedRows: 47,
  failedJobs: 2,
  recentOutputs: [
    { name: 'fix_order_lifecycle', rows: '1.2M rows', updated: 'Updated 5 min ago' },
    { name: 'fix_executions', rows: '420K rows', updated: 'Updated 5 min ago' },
    { name: 'hl7_observations', rows: '89K rows', updated: 'Updated 2 hrs ago' },
  ],
  activeRuns: [
    { name: 'Fidesrex_bc_parser', progress: 67 },
    { name: 'MT_multi_type', progress: 23 },
  ],
}

export default function Home() {
  const navigate = useNavigate()
  const [data, setData] = useState<DashboardStats | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    async function fetchData() {
      try {
        if (isTauri()) {
          const stats = await dashboardStats()
          setData(stats)
        } else {
          // Use mock data in browser development
          setData(mockStats)
        }
      } catch (err) {
        console.error('Failed to fetch dashboard stats:', err)
        setError(err instanceof Error ? err.message : 'Unknown error')
        // Fall back to mock data on error
        setData(mockStats)
      } finally {
        setLoading(false)
      }
    }

    fetchData()
  }, [])

  const handleRefresh = async () => {
    setLoading(true)
    try {
      if (isTauri()) {
        const stats = await dashboardStats()
        setData(stats)
      }
    } catch (err) {
      console.error('Failed to refresh:', err)
    } finally {
      setLoading(false)
    }
  }

  // Use data or fall back to mock
  const stats = data || mockStats

  const statCards = [
    { label: 'Ready Outputs', value: String(stats.readyOutputs), description: 'Tables ready to query', color: 'var(--foreground)' },
    { label: 'Running Jobs', value: String(stats.runningJobs), description: 'Currently processing', color: 'var(--primary)' },
    { label: 'Quarantined', value: String(stats.quarantinedRows), description: 'Rows need review', color: 'var(--warning-foreground)' },
    { label: 'Failed Jobs', value: String(stats.failedJobs), description: 'Require attention', color: 'var(--destructive)' },
  ]

  return (
    <main className="main-content" data-testid="home-screen">
      <div className="page-header">
        <div>
          <h1 className="page-title">Home</h1>
          <p className="page-subtitle">Readiness Board - Output-first triage dashboard</p>
        </div>
        <div className="header-actions">
          <button
            className="btn btn-outline"
            onClick={handleRefresh}
            disabled={loading}
          >
            {loading ? 'Loading...' : 'Refresh'}
          </button>
        </div>
      </div>

      {error && (
        <div className="alert alert-warning" style={{ marginBottom: 16 }}>
          Using cached data: {error}
        </div>
      )}

      <div className="stats-row" data-testid="stats-row">
        {statCards.map((stat) => (
          <div key={stat.label} className="stat-card" data-testid={`stat-${stat.label.toLowerCase().replace(' ', '-')}`}>
            <div className="stat-label">{stat.label}</div>
            <div className="stat-value" style={{ color: stat.color }}>{stat.value}</div>
            <div className="stat-description">{stat.description}</div>
          </div>
        ))}
      </div>

      <div className="content-row">
        <div className="content-column content-column-main">
          <div className="card" style={{ flex: 1 }}>
            <div className="card-header">
              <span className="card-title">Ready Outputs</span>
              <span className="text-primary" style={{ fontSize: 12, cursor: 'pointer' }} onClick={() => navigate('/query')}>View All &rarr;</span>
            </div>
            <div data-testid="ready-outputs-list">
              {stats.recentOutputs.map((output: OutputInfo) => (
                <div key={output.name} className="table-row" data-testid={`output-${output.name}`}>
                  <span className="material-symbols-sharp text-primary" style={{ marginRight: 16 }}>table_chart</span>
                  <div style={{ flex: 1 }}>
                    <div className="table-cell-mono">{output.name}</div>
                    <div className="text-muted" style={{ fontSize: 12 }}>{output.rows} &bull; {output.updated}</div>
                  </div>
                  <span className="btn btn-outline" style={{ fontSize: 12 }}>Ready</span>
                </div>
              ))}
              {stats.recentOutputs.length === 0 && (
                <div className="table-row text-muted">No outputs yet</div>
              )}
            </div>
          </div>
        </div>

        <div className="content-column content-column-side">
          <div className="card">
            <div className="card-header">
              <span className="card-title">Active Runs</span>
              <span className="text-primary" style={{ fontSize: 12, cursor: 'pointer' }} onClick={() => navigate('/jobs')}>View All &rarr;</span>
            </div>
            <div style={{ padding: 20 }} data-testid="active-runs-list">
              {stats.activeRuns.map((run: ActiveRun) => (
                <div key={run.name} style={{ marginBottom: 16 }}>
                  <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 8 }}>
                    <span className="table-cell-mono">{run.name}</span>
                    <span className="text-muted" style={{ fontSize: 12 }}>{run.progress}%</span>
                  </div>
                  <div style={{ height: 4, background: 'var(--muted)', borderRadius: 2 }}>
                    <div style={{ height: '100%', width: `${run.progress}%`, background: 'var(--primary)', borderRadius: 2 }} />
                  </div>
                </div>
              ))}
              {stats.activeRuns.length === 0 && (
                <div className="text-muted">No active runs</div>
              )}
            </div>
          </div>

          <div className="card">
            <div className="card-header">
              <span className="card-title">Quick Actions</span>
            </div>
            <div style={{ padding: 20, display: 'flex', flexDirection: 'column', gap: 12 }}>
              <button className="btn btn-primary" style={{ justifyContent: 'center' }} data-testid="btn-open-files" onClick={() => navigate('/discover')}>
                Open Files
              </button>
              <button className="btn btn-outline" style={{ justifyContent: 'center' }} data-testid="btn-scan-folder" onClick={() => navigate('/discover')}>
                Scan Folder
              </button>
              <button className="btn btn-outline" style={{ justifyContent: 'center' }} data-testid="btn-query-output" onClick={() => navigate('/query')}>
                Query Output
              </button>
            </div>
          </div>
        </div>
      </div>
    </main>
  )
}
