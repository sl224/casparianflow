import { useState, useEffect } from 'react'
import { caspIntentBacktestStart, caspPatchApply, jobCancel } from '../../api/commands'

interface BacktestStepProps {
  sessionId: string
  onComplete?: () => void
}

type BacktestPhase = 'idle' | 'generating' | 'fail_fast' | 'full' | 'complete' | 'failed'

// Mock data
const mockBacktestProgress = {
  jobId: 'bt-job-001',
  phase: 'validate' as const,
  elapsedMs: 45000,
  metrics: {
    filesProcessed: 187,
    filesTotalEstimate: 247,
    rowsEmitted: 45230,
    rowsQuarantined: 127,
  },
  topViolationSummary: [
    { violationType: 'TypeMismatch', count: 89, topColumns: [{ name: 'amount', count: 67 }] },
    { violationType: 'NullConstraint', count: 38, topColumns: [{ name: 'customer_id', count: 25 }] },
  ],
  stalled: false,
}

const mockBacktestReport = {
  jobId: 'bt-job-001',
  quality: {
    filesProcessed: 247,
    rowsEmitted: 58420,
    rowsQuarantined: 342,
    quarantinePct: 0.58,
    passRateFiles: 0.97,
  },
  topKViolations: [
    {
      violationType: 'TypeMismatch',
      count: 234,
      topColumns: [
        { name: 'amount', count: 156 },
        { name: 'quantity', count: 78 },
      ],
      exampleContexts: [
        { file: '/data/sales/q4/sales_2024_10.csv', row: 145, value: '12.5a' },
        { file: '/data/sales/q4/sales_2024_11.csv', row: 2301, value: 'N/A' },
      ],
    },
    {
      violationType: 'NullConstraint',
      count: 108,
      topColumns: [{ name: 'customer_id', count: 108 }],
      exampleContexts: [
        { file: '/data/sales/q4/transactions.xlsx', row: 567, value: null },
      ],
    },
  ],
}

export default function BacktestStep({ sessionId, onComplete }: BacktestStepProps) {
  const [phase, setPhase] = useState<BacktestPhase>('complete')
  const [progress, setProgress] = useState(mockBacktestProgress)
  const [report] = useState(mockBacktestReport)
  const [backtestJobId, setBacktestJobId] = useState<string | null>(null)

  // Simulate progress updates
  useEffect(() => {
    if (phase === 'fail_fast' || phase === 'full') {
      const interval = setInterval(() => {
        setProgress(p => ({
          ...p,
          metrics: {
            ...p.metrics,
            filesProcessed: Math.min(p.metrics.filesProcessed + 5, p.metrics.filesTotalEstimate || 247),
          },
          elapsedMs: p.elapsedMs + 1000,
        }))
      }, 1000)
      return () => clearInterval(interval)
    }
  }, [phase])

  const handleStartBacktest = async (failFast: boolean) => {
    setPhase(failFast ? 'fail_fast' : 'full')
    try {
      const result = await caspIntentBacktestStart({
        sessionId,
        draftId: 'draft-001',
        fileSetId: 'fs-001',
        failFast,
      })
      setBacktestJobId(result.backtestJobId)
    } catch (err) {
      console.error('Failed to start backtest:', err)
      setPhase('idle')
    }
  }

  const handleCancel = async () => {
    if (!backtestJobId) return
    try {
      await jobCancel(backtestJobId)
      setPhase('idle')
    } catch (err) {
      console.error('Failed to cancel backtest:', err)
    }
  }

  const handleApplyPatch = async (patchType: string) => {
    try {
      await caspPatchApply({
        sessionId,
        patchType: patchType as 'schema' | 'parser' | 'rule',
        patchContent: {},
        iterationId: `iter-${Date.now()}`,
      })
    } catch (err) {
      console.error('Patch application failed:', err)
    }
  }

  const handleProceed = () => {
    onComplete?.()
  }

  const getProgressPercent = () => {
    const total = progress.metrics.filesTotalEstimate || progress.metrics.filesProcessed
    return Math.round((progress.metrics.filesProcessed / total) * 100)
  }

  const formatDuration = (ms: number) => {
    const seconds = Math.floor(ms / 1000)
    const minutes = Math.floor(seconds / 60)
    const remainingSeconds = seconds % 60
    return `${minutes}:${remainingSeconds.toString().padStart(2, '0')}`
  }

  return (
    <div className="step-container" data-testid="backtest-step">
      {/* Parser Draft Info */}
      <div className="card">
        <div className="card-header">
          <span className="card-title">Parser Draft</span>
          <span className="badge badge-success">Build: Pass</span>
        </div>
        <div className="card-body">
          <div className="parser-info-grid">
            <div className="parser-info-item">
              <span className="text-muted">Name</span>
              <span className="table-cell-mono">sales_csv_parser</span>
            </div>
            <div className="parser-info-item">
              <span className="text-muted">Version</span>
              <span className="table-cell-mono">0.1.0</span>
            </div>
            <div className="parser-info-item">
              <span className="text-muted">Topics</span>
              <span className="tag-badge">sales_data</span>
            </div>
            <div className="parser-info-item">
              <span className="text-muted">Source Hash</span>
              <span className="table-cell-mono text-muted">a1b2c3d4...</span>
            </div>
          </div>
        </div>
      </div>

      {/* Backtest Controls */}
      {phase === 'idle' && (
        <div className="card">
          <div className="card-header">
            <span className="card-title">Start Backtest</span>
          </div>
          <div className="card-body">
            <p className="help-text">
              Run the parser against your selected files to validate the schema and identify issues.
            </p>
            <div className="backtest-options">
              <button
                className="btn btn-primary"
                onClick={() => handleStartBacktest(true)}
                data-testid="start-fail-fast-btn"
              >
                <span className="material-symbols-sharp" style={{ fontSize: 18 }}>bolt</span>
                Fail-Fast Mode
                <span className="btn-hint">Quick validation, stops on first error</span>
              </button>
              <button
                className="btn btn-outline"
                onClick={() => handleStartBacktest(false)}
                data-testid="start-full-btn"
              >
                <span className="material-symbols-sharp" style={{ fontSize: 18 }}>science</span>
                Full Backtest
                <span className="btn-hint">Process all files, comprehensive report</span>
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Progress Card */}
      {(phase === 'fail_fast' || phase === 'full') && (
        <div className="card">
          <div className="card-header">
            <span className="card-title">
              <span className="material-symbols-sharp spinning" style={{ fontSize: 20, marginRight: 8 }}>
                progress_activity
              </span>
              Backtest Running
            </span>
            <span className="text-muted">{formatDuration(progress.elapsedMs)}</span>
          </div>
          <div className="card-body">
            <div className="progress-section">
              <div className="progress-header">
                <span>Processing files...</span>
                <span>{progress.metrics.filesProcessed} / {progress.metrics.filesTotalEstimate}</span>
              </div>
              <div className="progress-bar-container">
                <div
                  className="progress-bar"
                  style={{ width: `${getProgressPercent()}%` }}
                />
              </div>
            </div>

            <div className="live-stats">
              <div className="live-stat">
                <span className="live-stat-value text-success">{progress.metrics.rowsEmitted.toLocaleString()}</span>
                <span className="live-stat-label">Rows Emitted</span>
              </div>
              <div className="live-stat">
                <span className="live-stat-value text-warning">{progress.metrics.rowsQuarantined.toLocaleString()}</span>
                <span className="live-stat-label">Quarantined</span>
              </div>
            </div>

            {progress.topViolationSummary.length > 0 && (
              <div className="live-violations">
                <div className="live-violations-title">Top Violations (so far)</div>
                {progress.topViolationSummary.map((v, idx) => (
                  <div key={idx} className="live-violation-item">
                    <span className="violation-type">{v.violationType}</span>
                    <span className="violation-count">{v.count}</span>
                  </div>
                ))}
              </div>
            )}

            <button className="btn btn-outline-destructive" data-testid="cancel-btn" onClick={handleCancel}>
              <span className="material-symbols-sharp" style={{ fontSize: 18 }}>stop</span>
              Cancel
            </button>
          </div>
        </div>
      )}

      {/* Report Card */}
      {phase === 'complete' && report && (
        <>
          <div className="card">
            <div className="card-header">
              <span className="card-title">Backtest Results</span>
              <span className={`badge ${report.quality.passRateFiles >= 0.95 ? 'badge-success' : 'badge-warning'}`}>
                {Math.round(report.quality.passRateFiles * 100)}% Pass Rate
              </span>
            </div>
            <div className="card-body">
              <div className="stats-row-compact">
                <div className="stat-mini">
                  <div className="stat-mini-value">{report.quality.filesProcessed}</div>
                  <div className="stat-mini-label">Files Processed</div>
                </div>
                <div className="stat-mini">
                  <div className="stat-mini-value text-success">{report.quality.rowsEmitted.toLocaleString()}</div>
                  <div className="stat-mini-label">Rows Emitted</div>
                </div>
                <div className="stat-mini">
                  <div className="stat-mini-value text-warning">{report.quality.rowsQuarantined}</div>
                  <div className="stat-mini-label">Quarantined</div>
                </div>
                <div className="stat-mini">
                  <div className="stat-mini-value text-muted">{report.quality.quarantinePct}%</div>
                  <div className="stat-mini-label">Quarantine Rate</div>
                </div>
              </div>
            </div>
          </div>

          {/* Violations Detail */}
          <div className="card">
            <div className="card-header">
              <span className="card-title">Top Violations</span>
              <span className="text-muted">{report.topKViolations.length} violation types</span>
            </div>
            <div className="card-body">
              {report.topKViolations.map((violation, idx) => (
                <div key={idx} className="violation-detail" data-testid={`violation-${idx}`}>
                  <div className="violation-header">
                    <span className="violation-type-badge">{violation.violationType}</span>
                    <span className="violation-count-large">{violation.count} occurrences</span>
                  </div>

                  <div className="violation-columns">
                    <span className="text-muted">Affected columns:</span>
                    {violation.topColumns.map((col, cIdx) => (
                      <span key={cIdx} className="column-badge">
                        {col.name} <span className="text-muted">({col.count})</span>
                      </span>
                    ))}
                  </div>

                  <div className="violation-examples">
                    <div className="examples-title">Example occurrences:</div>
                    {violation.exampleContexts.map((ex, eIdx) => (
                      <div key={eIdx} className="example-item">
                        <span className="table-cell-mono text-muted" style={{ fontSize: 11 }}>{ex.file}</span>
                        <span className="text-muted">row {ex.row}:</span>
                        <code className="example-value">{ex.value === null ? 'NULL' : String(ex.value)}</code>
                      </div>
                    ))}
                  </div>

                  <div className="violation-actions">
                    <button
                      className="btn btn-outline btn-sm"
                      onClick={() => handleApplyPatch('schema')}
                    >
                      <span className="material-symbols-sharp" style={{ fontSize: 16 }}>schema</span>
                      Fix Schema
                    </button>
                    <button
                      className="btn btn-outline btn-sm"
                      onClick={() => handleApplyPatch('parser')}
                    >
                      <span className="material-symbols-sharp" style={{ fontSize: 16 }}>code</span>
                      Fix Parser
                    </button>
                    <button
                      className="btn btn-outline btn-sm"
                      onClick={() => handleApplyPatch('rule')}
                    >
                      <span className="material-symbols-sharp" style={{ fontSize: 16 }}>rule</span>
                      Add Rule
                    </button>
                  </div>
                </div>
              ))}
            </div>
          </div>

          {/* Action Buttons */}
          <div className="step-actions">
            <button
              className="btn btn-outline"
              onClick={() => setPhase('idle')}
              data-testid="rerun-btn"
            >
              <span className="material-symbols-sharp" style={{ fontSize: 18 }}>refresh</span>
              Re-run Backtest
            </button>
            <button
              className="btn btn-primary"
              disabled={report.quality.passRateFiles < 0.9}
              data-testid="proceed-btn"
              onClick={handleProceed}
            >
              <span className="material-symbols-sharp" style={{ fontSize: 18 }}>arrow_forward</span>
              {report.quality.passRateFiles >= 0.9 ? 'Proceed to Publish' : 'Fix Issues First'}
            </button>
          </div>
        </>
      )}
    </div>
  )
}
