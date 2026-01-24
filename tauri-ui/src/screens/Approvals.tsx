import { useState, useEffect } from 'react'
import { approvalList, approvalDecide, approvalStats, isTauri } from '../api'
import type { ApprovalItem, ApprovalStats } from '../api'

// Mock data for development without Tauri
const mockApprovals: ApprovalItem[] = [
  { id: 'apr-001', operation: 'Run parser on /data/sales', plugin: 'fix_parser', files: '247', expires: 'in 45 min', urgent: true, status: 'pending' },
  { id: 'apr-002', operation: 'Promote schema for orders', plugin: 'evtx_native', files: '-', expires: 'in 2 hours', urgent: false, status: 'pending' },
  { id: 'apr-003', operation: 'Run parser on /logs/auth', plugin: 'syslog_parser', files: '1,842', expires: 'in 5 hours', urgent: false, status: 'pending' },
  { id: 'apr-004', operation: 'Run parser on /data/trades', plugin: 'csv_generic', files: '56', expires: 'in 23 hours', urgent: false, status: 'pending' },
]

const mockStats: ApprovalStats = {
  pending: 5,
  approved: 23,
  rejected: 2,
  expired: 8,
}

export default function Approvals() {
  const [approvals, setApprovals] = useState<ApprovalItem[]>([])
  const [stats, setStats] = useState<ApprovalStats>(mockStats)
  const [loading, setLoading] = useState(true)
  const [actionLoading, setActionLoading] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  const fetchData = async () => {
    try {
      if (isTauri()) {
        const [approvalsData, statsData] = await Promise.all([
          approvalList('pending'),
          approvalStats(),
        ])
        setApprovals(approvalsData)
        setStats(statsData)
      } else {
        // Use mock data in browser development
        setApprovals(mockApprovals)
        setStats(mockStats)
      }
      setError(null)
    } catch (err) {
      console.error('Failed to fetch approvals:', err)
      setError(err instanceof Error ? err.message : 'Unknown error')
      // Fall back to mock data
      setApprovals(mockApprovals)
      setStats(mockStats)
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    fetchData()
  }, [])

  const handleApprove = async (approvalId: string) => {
    setActionLoading(approvalId)
    try {
      if (isTauri()) {
        await approvalDecide({ approvalId, decision: 'approve' })
        await fetchData()
      } else {
        // Mock: remove from list
        setApprovals(prev => prev.filter(a => a.id !== approvalId))
        setStats(prev => ({ ...prev, pending: prev.pending - 1, approved: prev.approved + 1 }))
      }
    } catch (err) {
      console.error('Failed to approve:', err)
    } finally {
      setActionLoading(null)
    }
  }

  const handleReject = async (approvalId: string) => {
    setActionLoading(approvalId)
    try {
      if (isTauri()) {
        await approvalDecide({ approvalId, decision: 'reject' })
        await fetchData()
      } else {
        // Mock: remove from list
        setApprovals(prev => prev.filter(a => a.id !== approvalId))
        setStats(prev => ({ ...prev, pending: prev.pending - 1, rejected: prev.rejected + 1 }))
      }
    } catch (err) {
      console.error('Failed to reject:', err)
    } finally {
      setActionLoading(null)
    }
  }

  const statCards = [
    { label: 'Pending', value: String(stats.pending), description: 'Awaiting review', color: 'var(--primary)' },
    { label: 'Approved', value: String(stats.approved), description: 'This week', color: 'var(--success-foreground)' },
    { label: 'Rejected', value: String(stats.rejected), description: 'This week', color: 'var(--destructive)' },
    { label: 'Expired', value: String(stats.expired), description: 'Auto-expired', color: 'var(--muted-foreground)' },
  ]

  return (
    <main className="main-content" data-testid="approvals-screen">
      <div className="page-header">
        <div>
          <h1 className="page-title">Approvals</h1>
          <p className="page-subtitle">Review and approve pending operations</p>
        </div>
        <div className="header-actions">
          <button
            className="btn btn-outline"
            onClick={() => { setLoading(true); fetchData() }}
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

      <div className="stats-row" data-testid="approval-stats">
        {statCards.map((stat) => (
          <div key={stat.label} className="stat-card" data-testid={`stat-${stat.label.toLowerCase()}`}>
            <div className="stat-label">{stat.label}</div>
            <div className="stat-value" style={{ color: stat.color }}>{stat.value}</div>
            <div className="stat-description">{stat.description}</div>
          </div>
        ))}
      </div>

      <div className="card" style={{ flex: 1 }}>
        <div className="card-header">
          <span className="card-title">Pending Approvals</span>
          <span className="text-primary" style={{ fontSize: 12, cursor: 'pointer' }}>View All &rarr;</span>
        </div>

        <div className="table-header">
          <span style={{ width: 200 }}>Operation</span>
          <span style={{ width: 150 }}>Plugin</span>
          <span style={{ width: 80 }}>Files</span>
          <span style={{ width: 120 }}>Expires</span>
          <span style={{ flex: 1, textAlign: 'right' }}>Actions</span>
        </div>

        <div data-testid="approvals-list">
          {approvals.map((approval, index) => (
            <div key={approval.id} className="table-row" data-testid={`approval-row-${index}`}>
              <span className="table-cell" style={{ width: 200 }}>{approval.operation}</span>
              <span className="table-cell-mono text-muted" style={{ width: 150 }}>{approval.plugin}</span>
              <span className="table-cell-mono" style={{ width: 80 }}>{approval.files}</span>
              <span
                className={`table-cell ${approval.urgent ? 'text-warning' : ''}`}
                style={{ width: 120 }}
              >
                {approval.expires}
              </span>
              <div style={{ flex: 1, display: 'flex', justifyContent: 'flex-end', gap: 8 }}>
                <button
                  className="btn btn-primary"
                  onClick={() => handleApprove(approval.id)}
                  disabled={actionLoading === approval.id}
                  data-testid={`approve-btn-${index}`}
                >
                  {actionLoading === approval.id ? '...' : 'Approve'}
                </button>
                <button
                  className="btn btn-outline-destructive"
                  onClick={() => handleReject(approval.id)}
                  disabled={actionLoading === approval.id}
                  data-testid={`reject-btn-${index}`}
                >
                  Reject
                </button>
              </div>
            </div>
          ))}
          {approvals.length === 0 && !loading && (
            <div className="table-row text-muted" style={{ justifyContent: 'center' }}>
              No pending approvals
            </div>
          )}
        </div>
      </div>
    </main>
  )
}
