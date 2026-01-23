import { useState, useEffect } from 'react'
import { useNavigate } from 'react-router-dom'
import { jobList, isTauri } from '../api'
import type { JobItem } from '../api'

// Simplified job for display
interface JobDisplay {
  id: string
  status: string
  parserName: string
  parserVersion: string
  progress: number
  createdAt: string
}

// Mock data for development
const mockJobs: JobDisplay[] = [
  {
    id: 'job_4521',
    status: 'running',
    parserName: 'evtx_parser',
    parserVersion: '1.2.0',
    progress: 67,
    createdAt: '2 min ago',
  },
  {
    id: 'job_4520',
    status: 'completed',
    parserName: 'fix_parser',
    parserVersion: '2.0.1',
    progress: 100,
    createdAt: '15 min ago',
  },
  {
    id: 'job_4519',
    status: 'completed',
    parserName: 'hl7_parser',
    parserVersion: '1.5.0',
    progress: 100,
    createdAt: '1 hour ago',
  },
  {
    id: 'job_4518',
    status: 'failed',
    parserName: 'custom_parser_v2',
    parserVersion: '0.3.0',
    progress: 0,
    createdAt: '2 hours ago',
  },
]

const statusColors: Record<string, string> = {
  running: 'var(--primary)',
  completed: 'var(--success-foreground)',
  failed: 'var(--destructive)',
  queued: 'var(--muted-foreground)',
}

export default function Jobs() {
  const navigate = useNavigate()
  const [jobs, setJobs] = useState<JobDisplay[]>([])
  const [loading, setLoading] = useState(true)
  const [filter, setFilter] = useState<string>('all')

  useEffect(() => {
    async function fetchJobs() {
      try {
        if (isTauri()) {
          const data = await jobList()
          // Transform JobItem to JobDisplay
          const displayJobs: JobDisplay[] = data.map((j: JobItem) => ({
            id: j.id,
            status: j.status,
            parserName: j.pluginName,
            parserVersion: j.pluginVersion || '0.0.0',
            progress: j.progress?.itemsTotal
              ? Math.round((j.progress.itemsDone / j.progress.itemsTotal) * 100)
              : 0,
            createdAt: j.createdAt,
          }))
          setJobs(displayJobs)
        } else {
          setJobs(mockJobs)
        }
      } catch (err) {
        console.error('Failed to fetch jobs:', err)
        setJobs(mockJobs)
      } finally {
        setLoading(false)
      }
    }
    fetchJobs()
  }, [])

  const filteredJobs = filter === 'all'
    ? jobs
    : jobs.filter(j => j.status === filter)

  const stats = {
    running: jobs.filter(j => j.status === 'running').length,
    completed: jobs.filter(j => j.status === 'completed').length,
    failed: jobs.filter(j => j.status === 'failed').length,
    queued: jobs.filter(j => j.status === 'queued').length,
  }

  return (
    <main className="main-content" data-testid="jobs-screen">
      <div className="page-header">
        <div>
          <h1 className="page-title">Jobs</h1>
          <p className="page-subtitle">Monitor parser execution and job status</p>
        </div>
        <div className="header-actions">
          <select
            className="filter-select"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
          >
            <option value="all">All Status</option>
            <option value="running">Running</option>
            <option value="completed">Completed</option>
            <option value="failed">Failed</option>
            <option value="queued">Queued</option>
          </select>
          <button className="btn btn-primary" onClick={() => navigate('/sessions/new')}>
            <span className="material-symbols-sharp" style={{ fontSize: 16 }}>add</span>
            New Job
          </button>
        </div>
      </div>

      <div className="stats-row" data-testid="stats-row">
        <div className="stat-card">
          <div className="stat-label">Running</div>
          <div className="stat-value" style={{ color: 'var(--primary)' }}>{stats.running}</div>
        </div>
        <div className="stat-card">
          <div className="stat-label">Completed</div>
          <div className="stat-value" style={{ color: 'var(--success-foreground)' }}>{stats.completed}</div>
        </div>
        <div className="stat-card">
          <div className="stat-label">Failed</div>
          <div className="stat-value" style={{ color: 'var(--destructive)' }}>{stats.failed}</div>
        </div>
        <div className="stat-card">
          <div className="stat-label">Queued</div>
          <div className="stat-value">{stats.queued}</div>
        </div>
      </div>

      <div className="card" style={{ flex: 1 }}>
        <div className="card-header">
          <span className="card-title">Recent Jobs</span>
          <span className="text-muted" style={{ fontSize: 12 }}>{filteredJobs.length} jobs</span>
        </div>

        <div className="table-header">
          <span style={{ width: 100 }}>ID</span>
          <span style={{ width: 100 }}>Status</span>
          <span style={{ flex: 1 }}>Parser</span>
          <span style={{ width: 120 }}>Progress</span>
          <span style={{ width: 100 }}>Created</span>
        </div>

        {loading ? (
          <div className="table-row text-muted">Loading...</div>
        ) : filteredJobs.length === 0 ? (
          <div className="table-row text-muted">No jobs found</div>
        ) : (
          filteredJobs.map((job) => (
            <div key={job.id} className="table-row table-row-clickable" onClick={() => navigate(`/jobs/${job.id}`)}>
              <span className="table-cell-mono" style={{ width: 100 }}>{job.id}</span>
              <span style={{ width: 100 }}>
                <span
                  className="badge"
                  style={{
                    background: `${statusColors[job.status]}20`,
                    color: statusColors[job.status],
                  }}
                >
                  {job.status}
                </span>
              </span>
              <span style={{ flex: 1 }}>
                <span style={{ fontWeight: 500 }}>{job.parserName}</span>
                <span className="text-muted" style={{ marginLeft: 8, fontSize: 12 }}>
                  v{job.parserVersion}
                </span>
              </span>
              <span style={{ width: 120 }}>
                {job.status === 'running' ? (
                  <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                    <div style={{ flex: 1, height: 4, background: 'var(--muted)', borderRadius: 2 }}>
                      <div
                        style={{
                          height: '100%',
                          width: `${job.progress}%`,
                          background: 'var(--primary)',
                          borderRadius: 2
                        }}
                      />
                    </div>
                    <span className="text-muted" style={{ fontSize: 12 }}>{job.progress}%</span>
                  </div>
                ) : (
                  <span className="text-muted">-</span>
                )}
              </span>
              <span className="text-muted" style={{ width: 100, fontSize: 12 }}>{job.createdAt}</span>
            </div>
          ))
        )}
      </div>
    </main>
  )
}
