import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { caspPublishPlan, caspPublishExecute, caspRunPlan, caspRunExecute } from '../../api/commands'

interface PublishRunStepProps {
  sessionId: string
}

type StepPhase = 'publish_plan' | 'publish_approval' | 'publishing' | 'run_plan' | 'run_approval' | 'running' | 'complete'

// Mock data
const mockPublishPlan = {
  planId: 'pub-plan-001',
  parser: {
    name: 'sales_csv_parser',
    version: '0.1.0',
    topics: ['sales_data'],
    repoRef: 'parsers/sales_csv_parser.py',
  },
  schema: {
    name: 'sales_orders',
    columns: 7,
    contractRef: 'schemas/sales_orders.yaml',
  },
  invariants: {
    noBreakingChanges: true,
    testsPass: true,
    lintPass: true,
  },
  estimatedCost: {
    storage: '~2.4 GB',
    compute: '~15 min',
  },
}

const mockRunPlan = {
  planId: 'run-plan-001',
  inputFileSetId: 'fs-001',
  totalFiles: 247,
  estimatedRows: 58000,
  sinks: [
    { name: 'sales_orders', format: 'parquet', path: '/output/sales_orders/' },
  ],
  partitioning: {
    strategy: 'by_job',
    pattern: '{output}_{job_id}.parquet',
  },
  validations: {
    schemaEnforcement: 'strict',
    deduplication: true,
  },
  estimatedCost: {
    storage: '~2.4 GB',
    compute: '~15 min',
  },
}

export default function PublishRunStep({ sessionId }: PublishRunStepProps) {
  const navigate = useNavigate()
  const [phase, setPhase] = useState<StepPhase>('run_plan')
  const [publishPlan] = useState(mockPublishPlan)
  const [runPlan] = useState(mockRunPlan)
  const [runProgress, setRunProgress] = useState({
    filesProcessed: 0,
    filesTotal: 247,
    rowsWritten: 0,
    elapsedMs: 0,
  })
  const [publishApprovalToken, setPublishApprovalToken] = useState<string | null>(null)
  const [runApprovalToken, setRunApprovalToken] = useState<string | null>(null)

  const handleRequestPublishApproval = async () => {
    setPhase('publish_approval')
    try {
      const result = await caspPublishPlan({
        sessionId,
        draftId: 'draft-001',
        schemaName: publishPlan.schema.name,
        schemaVersion: '1.0.0',
        parserName: publishPlan.parser.name,
        parserVersion: publishPlan.parser.version,
      })
      setPublishApprovalToken(result.approvalTokenHash)
    } catch (err) {
      console.error('Failed to create publish plan:', err)
    }
  }

  const handleApprovePublish = async () => {
    setPhase('publishing')
    try {
      await caspPublishExecute({
        sessionId,
        proposalId: 'pub-plan-001',
        approvalTokenHash: publishApprovalToken || 'mock-token',
      })
      setPhase('run_plan')
    } catch (err) {
      console.error('Publish failed:', err)
      setPhase('publish_plan')
    }
  }

  const handleRequestRunApproval = async () => {
    setPhase('run_approval')
    try {
      const result = await caspRunPlan({
        sessionId,
        fileSetId: runPlan.inputFileSetId,
        parserName: publishPlan.parser.name,
        parserVersion: publishPlan.parser.version,
        sinkUri: 'parquet:///output/',
        routeToTopic: publishPlan.parser.topics[0],
      })
      setRunApprovalToken(result.approvalTokenHash)
    } catch (err) {
      console.error('Failed to create run plan:', err)
    }
  }

  const handleApproveRun = async () => {
    setPhase('running')
    try {
      await caspRunExecute({
        sessionId,
        proposalId: 'run-plan-001',
        approvalTokenHash: runApprovalToken || 'mock-token',
      })
      // Simulate progress (in reality, would poll for job status)
      const interval = setInterval(() => {
        setRunProgress(p => {
          const newFiles = Math.min(p.filesProcessed + 5, p.filesTotal)
          if (newFiles >= p.filesTotal) {
            clearInterval(interval)
            setTimeout(() => setPhase('complete'), 500)
          }
          return {
            ...p,
            filesProcessed: newFiles,
            rowsWritten: Math.round(newFiles * 235),
            elapsedMs: p.elapsedMs + 1000,
          }
        })
      }, 500)
    } catch (err) {
      console.error('Run failed:', err)
      setPhase('run_plan')
    }
  }

  const handleViewOutput = () => {
    navigate('/query')
  }

  const handleNewSession = () => {
    navigate('/sessions/new')
  }

  const formatDuration = (ms: number) => {
    const seconds = Math.floor(ms / 1000)
    const minutes = Math.floor(seconds / 60)
    const remainingSeconds = seconds % 60
    return `${minutes}:${remainingSeconds.toString().padStart(2, '0')}`
  }

  return (
    <div className="step-container" data-testid="publish-run-step">
      {/* Publish Section */}
      {(phase === 'publish_plan' || phase === 'publish_approval' || phase === 'publishing') && (
        <>
          <div className="phase-header">
            <span className="phase-number">1</span>
            <span className="phase-title">Publish Parser & Schema</span>
          </div>

          <div className="card">
            <div className="card-header">
              <span className="card-title">Publish Plan</span>
              {phase === 'publish_approval' && (
                <span className="badge badge-warning">Awaiting Approval</span>
              )}
              {phase === 'publishing' && (
                <span className="badge badge-primary">Publishing...</span>
              )}
            </div>
            <div className="card-body">
              {/* Parser Info */}
              <div className="publish-section">
                <div className="publish-section-header">
                  <span className="material-symbols-sharp" style={{ fontSize: 20 }}>code</span>
                  <span>Parser</span>
                </div>
                <div className="publish-details">
                  <div className="detail-row">
                    <span className="detail-label">Name</span>
                    <span className="table-cell-mono">{publishPlan.parser.name}</span>
                  </div>
                  <div className="detail-row">
                    <span className="detail-label">Version</span>
                    <span className="table-cell-mono">{publishPlan.parser.version}</span>
                  </div>
                  <div className="detail-row">
                    <span className="detail-label">Topics</span>
                    <span className="tag-badge">{publishPlan.parser.topics.join(', ')}</span>
                  </div>
                  <div className="detail-row">
                    <span className="detail-label">Location</span>
                    <span className="table-cell-mono text-muted">{publishPlan.parser.repoRef}</span>
                  </div>
                </div>
              </div>

              {/* Schema Info */}
              <div className="publish-section">
                <div className="publish-section-header">
                  <span className="material-symbols-sharp" style={{ fontSize: 20 }}>schema</span>
                  <span>Schema Contract</span>
                </div>
                <div className="publish-details">
                  <div className="detail-row">
                    <span className="detail-label">Name</span>
                    <span className="table-cell-mono">{publishPlan.schema.name}</span>
                  </div>
                  <div className="detail-row">
                    <span className="detail-label">Columns</span>
                    <span>{publishPlan.schema.columns} columns</span>
                  </div>
                  <div className="detail-row">
                    <span className="detail-label">Location</span>
                    <span className="table-cell-mono text-muted">{publishPlan.schema.contractRef}</span>
                  </div>
                </div>
              </div>

              {/* Invariants Check */}
              <div className="invariants-check">
                <div className="invariant-item">
                  <span className={`material-symbols-sharp ${publishPlan.invariants.noBreakingChanges ? 'text-success' : 'text-destructive'}`}>
                    {publishPlan.invariants.noBreakingChanges ? 'check_circle' : 'cancel'}
                  </span>
                  <span>No breaking changes</span>
                </div>
                <div className="invariant-item">
                  <span className={`material-symbols-sharp ${publishPlan.invariants.testsPass ? 'text-success' : 'text-destructive'}`}>
                    {publishPlan.invariants.testsPass ? 'check_circle' : 'cancel'}
                  </span>
                  <span>All tests pass</span>
                </div>
                <div className="invariant-item">
                  <span className={`material-symbols-sharp ${publishPlan.invariants.lintPass ? 'text-success' : 'text-destructive'}`}>
                    {publishPlan.invariants.lintPass ? 'check_circle' : 'cancel'}
                  </span>
                  <span>Lint checks pass</span>
                </div>
              </div>

              {phase === 'publish_plan' && (
                <button
                  className="btn btn-primary"
                  onClick={handleRequestPublishApproval}
                  data-testid="request-publish-btn"
                >
                  <span className="material-symbols-sharp" style={{ fontSize: 18 }}>upload</span>
                  Request Publish Approval
                </button>
              )}

              {phase === 'publish_approval' && (
                <div className="approval-actions">
                  <p className="approval-warning">
                    <span className="material-symbols-sharp" style={{ fontSize: 18 }}>warning</span>
                    This will register the parser and schema as production artifacts.
                  </p>
                  <div className="btn-group">
                    <button
                      className="btn btn-outline"
                      onClick={() => setPhase('publish_plan')}
                    >
                      Cancel
                    </button>
                    <button
                      className="btn btn-primary"
                      onClick={handleApprovePublish}
                      data-testid="approve-publish-btn"
                    >
                      <span className="material-symbols-sharp" style={{ fontSize: 18 }}>check</span>
                      Approve & Publish
                    </button>
                  </div>
                </div>
              )}

              {phase === 'publishing' && (
                <div className="publishing-progress">
                  <span className="material-symbols-sharp spinning" style={{ fontSize: 24 }}>progress_activity</span>
                  <span>Publishing artifacts...</span>
                </div>
              )}
            </div>
          </div>
        </>
      )}

      {/* Run Section */}
      {(phase === 'run_plan' || phase === 'run_approval' || phase === 'running' || phase === 'complete') && (
        <>
          <div className="phase-header">
            <span className="phase-number">2</span>
            <span className="phase-title">Execute Pipeline</span>
          </div>

          <div className="card">
            <div className="card-header">
              <span className="card-title">Run Plan</span>
              {phase === 'run_approval' && (
                <span className="badge badge-warning">Awaiting Approval</span>
              )}
              {phase === 'running' && (
                <span className="badge badge-primary">Running...</span>
              )}
              {phase === 'complete' && (
                <span className="badge badge-success">Complete</span>
              )}
            </div>
            <div className="card-body">
              {/* Input/Output Summary */}
              <div className="run-summary">
                <div className="run-summary-item">
                  <span className="material-symbols-sharp text-muted">folder</span>
                  <div>
                    <div className="run-summary-value">{runPlan.totalFiles} files</div>
                    <div className="run-summary-label">Input</div>
                  </div>
                </div>
                <span className="material-symbols-sharp text-muted">arrow_forward</span>
                <div className="run-summary-item">
                  <span className="material-symbols-sharp text-muted">table_chart</span>
                  <div>
                    <div className="run-summary-value">~{(runPlan.estimatedRows / 1000).toFixed(0)}K rows</div>
                    <div className="run-summary-label">Estimated Output</div>
                  </div>
                </div>
              </div>

              {/* Sinks */}
              <div className="sinks-section">
                <div className="section-title">Output Sinks</div>
                {runPlan.sinks.map((sink, idx) => (
                  <div key={idx} className="sink-item">
                    <span className="sink-name">{sink.name}</span>
                    <span className="sink-format">{sink.format}</span>
                    <span className="table-cell-mono text-muted" style={{ fontSize: 12 }}>{sink.path}</span>
                  </div>
                ))}
              </div>

              {/* Validations */}
              <div className="validations-section">
                <div className="section-title">Validations</div>
                <div className="validation-item">
                  <span className="text-muted">Schema Enforcement:</span>
                  <span className="badge badge-info">{runPlan.validations.schemaEnforcement}</span>
                </div>
                <div className="validation-item">
                  <span className="text-muted">Deduplication:</span>
                  <span>{runPlan.validations.deduplication ? 'Enabled' : 'Disabled'}</span>
                </div>
              </div>

              {/* Cost Estimate */}
              <div className="cost-estimate">
                <span className="material-symbols-sharp text-muted">payments</span>
                <span>Estimated: {runPlan.estimatedCost.storage} storage, {runPlan.estimatedCost.compute} compute</span>
              </div>

              {phase === 'run_plan' && (
                <button
                  className="btn btn-primary"
                  onClick={handleRequestRunApproval}
                  data-testid="request-run-btn"
                >
                  <span className="material-symbols-sharp" style={{ fontSize: 18 }}>play_arrow</span>
                  Request Run Approval
                </button>
              )}

              {phase === 'run_approval' && (
                <div className="approval-actions">
                  <p className="approval-warning">
                    <span className="material-symbols-sharp" style={{ fontSize: 18 }}>warning</span>
                    This will process {runPlan.totalFiles} files and write to production sinks.
                  </p>
                  <div className="btn-group">
                    <button
                      className="btn btn-outline"
                      onClick={() => setPhase('run_plan')}
                    >
                      Cancel
                    </button>
                    <button
                      className="btn btn-primary"
                      onClick={handleApproveRun}
                      data-testid="approve-run-btn"
                    >
                      <span className="material-symbols-sharp" style={{ fontSize: 18 }}>check</span>
                      Approve & Run
                    </button>
                  </div>
                </div>
              )}

              {phase === 'running' && (
                <div className="run-progress">
                  <div className="progress-header">
                    <span>Processing files...</span>
                    <span>{runProgress.filesProcessed} / {runProgress.filesTotal}</span>
                  </div>
                  <div className="progress-bar-container">
                    <div
                      className="progress-bar"
                      style={{ width: `${(runProgress.filesProcessed / runProgress.filesTotal) * 100}%` }}
                    />
                  </div>
                  <div className="run-stats">
                    <span>{runProgress.rowsWritten.toLocaleString()} rows written</span>
                    <span className="text-muted">{formatDuration(runProgress.elapsedMs)}</span>
                  </div>
                </div>
              )}

              {phase === 'complete' && (
                <div className="complete-summary">
                  <div className="complete-icon">
                    <span className="material-symbols-sharp" style={{ fontSize: 48, color: 'var(--success-foreground)' }}>
                      check_circle
                    </span>
                  </div>
                  <h3>Pipeline Complete!</h3>
                  <div className="complete-stats">
                    <div className="complete-stat">
                      <span className="complete-stat-value">{runProgress.filesTotal}</span>
                      <span className="complete-stat-label">Files Processed</span>
                    </div>
                    <div className="complete-stat">
                      <span className="complete-stat-value">{runProgress.rowsWritten.toLocaleString()}</span>
                      <span className="complete-stat-label">Rows Written</span>
                    </div>
                    <div className="complete-stat">
                      <span className="complete-stat-value">{formatDuration(runProgress.elapsedMs)}</span>
                      <span className="complete-stat-label">Duration</span>
                    </div>
                  </div>
                  <div className="complete-actions">
                    <button className="btn btn-outline" data-testid="view-output-btn" onClick={handleViewOutput}>
                      <span className="material-symbols-sharp" style={{ fontSize: 18 }}>table_chart</span>
                      View Output
                    </button>
                    <button className="btn btn-primary" data-testid="new-session-btn" onClick={handleNewSession}>
                      <span className="material-symbols-sharp" style={{ fontSize: 18 }}>add</span>
                      New Session
                    </button>
                  </div>
                </div>
              )}
            </div>
          </div>
        </>
      )}
    </div>
  )
}
