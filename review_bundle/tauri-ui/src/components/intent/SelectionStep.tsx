import { useState } from 'react'
import { caspSelectPropose, caspSelectApprove } from '../../api/commands'

interface SelectionStepProps {
  sessionId: string
  onApproved?: () => void
}

// Mock data - in production this would come from Tauri backend
const mockProposal = {
  proposalId: 'prop-001',
  rootDir: '/data/sales',
  preview: {
    totalFiles: 1247,
    selectedFiles: 247,
    extensions: [
      { ext: '.csv', count: 200, selected: true },
      { ext: '.xlsx', count: 35, selected: true },
      { ext: '.json', count: 12, selected: true },
      { ext: '.log', count: 1000, selected: false },
    ],
    directories: [
      { path: '/data/sales/q4', count: 150, selected: true },
      { path: '/data/sales/q3', count: 97, selected: true },
      { path: '/data/sales/archive', count: 1000, selected: false },
    ],
    sampleFiles: [
      { path: '/data/sales/q4/sales_2024_10.csv', size: '2.4 MB' },
      { path: '/data/sales/q4/sales_2024_11.csv', size: '2.1 MB' },
      { path: '/data/sales/q4/sales_2024_12.csv', size: '2.8 MB' },
      { path: '/data/sales/q4/transactions.xlsx', size: '1.5 MB' },
      { path: '/data/sales/q3/sales_2024_09.csv', size: '2.2 MB' },
    ],
  },
  confidence: {
    score: 0.87,
    label: 'high',
  },
  evidence: {
    dirPrefixScore: 0.92,
    extensionScore: 0.85,
    semanticScore: 0.84,
  },
}

export default function SelectionStep({ sessionId, onApproved }: SelectionStepProps) {
  const [intent, setIntent] = useState('')
  const [rootDir, setRootDir] = useState('/data/sales')
  const [proposal, setProposal] = useState(mockProposal)
  const [isScanning, setIsScanning] = useState(false)
  const [approvalToken, setApprovalToken] = useState<string | null>(null)

  const handleBrowse = async () => {
    try {
      const { open } = await import('@tauri-apps/api/dialog')
      const selected = await open({ directory: true, multiple: false })
      if (selected && typeof selected === 'string') {
        setRootDir(selected)
      }
    } catch (err) {
      console.error('Failed to open directory picker:', err)
    }
  }

  const handleScan = async () => {
    setIsScanning(true)
    try {
      const result = await caspSelectPropose({
        sessionId,
        baseDir: rootDir,
        semanticTokens: intent ? intent.split(/\s+/).filter(t => t.length > 2) : undefined,
      })
      setProposal(prev => ({
        ...prev,
        proposalId: result.proposalId,
        confidence: result.confidence,
        evidence: {
          dirPrefixScore: result.evidence.dirPrefixScore || 0.85,
          extensionScore: result.evidence.extensionScore || 0.85,
          semanticScore: result.evidence.semanticScore || 0.85,
        },
        preview: { ...prev.preview, selectedFiles: result.fileCount },
      }))
      setApprovalToken(result.proposalHash)
    } catch (err) {
      console.error('Scan failed:', err)
    } finally {
      setIsScanning(false)
    }
  }

  const handleApprove = async () => {
    if (!approvalToken) return
    try {
      await caspSelectApprove({
        sessionId,
        proposalId: proposal.proposalId,
        approvalTokenHash: approvalToken,
      })
      onApproved?.()
    } catch (err) {
      console.error('Approval failed:', err)
    }
  }

  const handleToggleExtension = (ext: string) => {
    setProposal(prev => ({
      ...prev,
      preview: {
        ...prev.preview,
        extensions: prev.preview.extensions.map(e =>
          e.ext === ext ? { ...e, selected: !e.selected } : e
        ),
      },
    }))
  }

  const handleToggleDirectory = (path: string) => {
    setProposal(prev => ({
      ...prev,
      preview: {
        ...prev.preview,
        directories: prev.preview.directories.map(d =>
          d.path === path ? { ...d, selected: !d.selected } : d
        ),
      },
    }))
  }

  const getConfidenceColor = (label: string) => {
    switch (label) {
      case 'high': return 'var(--success-foreground)'
      case 'medium': return 'var(--warning-foreground)'
      case 'low': return 'var(--destructive)'
      default: return 'var(--muted-foreground)'
    }
  }

  return (
    <div className="step-container" data-testid="selection-step">
      {/* Intent Input Section */}
      <div className="card">
        <div className="card-header">
          <span className="card-title">Describe Your Intent</span>
        </div>
        <div className="card-body">
          <div className="form-group">
            <label className="form-label">What do you want to process?</label>
            <textarea
              className="form-textarea"
              placeholder="e.g., Process all sales CSV files from Q4 2024"
              value={intent}
              onChange={(e) => setIntent(e.target.value)}
              rows={3}
              data-testid="intent-input"
            />
          </div>
          <div className="form-group">
            <label className="form-label">Root Directory</label>
            <div className="input-with-button">
              <input
                type="text"
                className="form-input"
                value={rootDir}
                onChange={(e) => setRootDir(e.target.value)}
                placeholder="/path/to/data"
                data-testid="root-dir-input"
              />
              <button className="btn btn-outline" data-testid="browse-btn" onClick={handleBrowse}>
                <span className="material-symbols-sharp" style={{ fontSize: 18 }}>folder_open</span>
              </button>
            </div>
          </div>
          <button
            className="btn btn-primary"
            onClick={handleScan}
            disabled={isScanning || !rootDir}
            data-testid="scan-btn"
          >
            {isScanning ? (
              <>
                <span className="material-symbols-sharp spinning" style={{ fontSize: 18 }}>progress_activity</span>
                Scanning...
              </>
            ) : (
              <>
                <span className="material-symbols-sharp" style={{ fontSize: 18 }}>search</span>
                Scan & Propose Selection
              </>
            )}
          </button>
        </div>
      </div>

      {/* Proposal Preview */}
      {proposal && (
        <>
          <div className="content-row" style={{ gap: 16 }}>
            {/* Selection Summary */}
            <div className="card" style={{ flex: 1 }}>
              <div className="card-header">
                <span className="card-title">Selection Proposal</span>
                <span
                  className="confidence-badge"
                  style={{ color: getConfidenceColor(proposal.confidence.label) }}
                  data-testid="confidence-badge"
                >
                  <span className="material-symbols-sharp" style={{ fontSize: 16 }}>verified</span>
                  {Math.round(proposal.confidence.score * 100)}% confidence
                </span>
              </div>
              <div className="card-body">
                <div className="stats-row-compact">
                  <div className="stat-mini">
                    <div className="stat-mini-value">{proposal.preview.selectedFiles}</div>
                    <div className="stat-mini-label">Selected</div>
                  </div>
                  <div className="stat-mini">
                    <div className="stat-mini-value text-muted">{proposal.preview.totalFiles}</div>
                    <div className="stat-mini-label">Total Found</div>
                  </div>
                </div>

                {/* Extensions Breakdown */}
                <div className="breakdown-section">
                  <div className="breakdown-title">By Extension</div>
                  {proposal.preview.extensions.map((ext) => (
                    <div key={ext.ext} className="breakdown-row">
                      <label className="checkbox-label">
                        <input
                          type="checkbox"
                          checked={ext.selected}
                          onChange={() => handleToggleExtension(ext.ext)}
                          data-testid={`ext-checkbox-${ext.ext}`}
                        />
                        <span className="table-cell-mono">{ext.ext}</span>
                      </label>
                      <span className="text-muted">{ext.count} files</span>
                    </div>
                  ))}
                </div>

                {/* Directories Breakdown */}
                <div className="breakdown-section">
                  <div className="breakdown-title">By Directory</div>
                  {proposal.preview.directories.map((dir) => (
                    <div key={dir.path} className="breakdown-row">
                      <label className="checkbox-label">
                        <input
                          type="checkbox"
                          checked={dir.selected}
                          onChange={() => handleToggleDirectory(dir.path)}
                          data-testid={`dir-checkbox-${dir.path}`}
                        />
                        <span className="table-cell-mono" style={{ fontSize: 12 }}>{dir.path}</span>
                      </label>
                      <span className="text-muted">{dir.count} files</span>
                    </div>
                  ))}
                </div>
              </div>
            </div>

            {/* Sample Files Preview */}
            <div className="card" style={{ flex: 1 }}>
              <div className="card-header">
                <span className="card-title">Sample Files</span>
                <span className="text-muted" style={{ fontSize: 12 }}>First 5 of {proposal.preview.selectedFiles}</span>
              </div>
              <div className="card-body">
                <div className="file-list" data-testid="sample-files">
                  {proposal.preview.sampleFiles.map((file, index) => (
                    <div key={index} className="file-item">
                      <span className="material-symbols-sharp text-muted" style={{ fontSize: 18 }}>description</span>
                      <div className="file-info">
                        <span className="file-path">{file.path}</span>
                        <span className="file-size text-muted">{file.size}</span>
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            </div>
          </div>

          {/* Evidence Breakdown */}
          <div className="card">
            <div className="card-header">
              <span className="card-title">Confidence Evidence</span>
            </div>
            <div className="card-body">
              <div className="evidence-grid">
                <div className="evidence-item">
                  <div className="evidence-label">Directory Concentration</div>
                  <div className="evidence-bar-container">
                    <div
                      className="evidence-bar"
                      style={{ width: `${proposal.evidence.dirPrefixScore * 100}%` }}
                    />
                  </div>
                  <div className="evidence-value">{Math.round(proposal.evidence.dirPrefixScore * 100)}%</div>
                </div>
                <div className="evidence-item">
                  <div className="evidence-label">Extension Match</div>
                  <div className="evidence-bar-container">
                    <div
                      className="evidence-bar"
                      style={{ width: `${proposal.evidence.extensionScore * 100}%` }}
                    />
                  </div>
                  <div className="evidence-value">{Math.round(proposal.evidence.extensionScore * 100)}%</div>
                </div>
                <div className="evidence-item">
                  <div className="evidence-label">Semantic Relevance</div>
                  <div className="evidence-bar-container">
                    <div
                      className="evidence-bar"
                      style={{ width: `${proposal.evidence.semanticScore * 100}%` }}
                    />
                  </div>
                  <div className="evidence-value">{Math.round(proposal.evidence.semanticScore * 100)}%</div>
                </div>
              </div>
            </div>
          </div>

          {/* Action Buttons */}
          <div className="step-actions">
            <button className="btn btn-outline" data-testid="modify-btn">
              <span className="material-symbols-sharp" style={{ fontSize: 18 }}>edit</span>
              Modify Selection
            </button>
            <button className="btn btn-primary" onClick={handleApprove} data-testid="approve-btn">
              <span className="material-symbols-sharp" style={{ fontSize: 18 }}>check</span>
              Approve Selection
            </button>
          </div>
        </>
      )}
    </div>
  )
}
