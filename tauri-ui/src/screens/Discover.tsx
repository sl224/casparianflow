import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { sessionCreate, isTauri, casparianScan } from '../api'

interface ScanFile {
  path: string
  size: number
  modifiedAt: string
}

export default function Discover() {
  const navigate = useNavigate()
  const [scanPath, setScanPath] = useState('')
  const [pattern, setPattern] = useState('*')
  const [files, setFiles] = useState<ScanFile[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const handleBrowse = () => {
    import('@tauri-apps/api/dialog').then(({ open }) => {
      open({ directory: true, multiple: false }).then((dir) => {
        if (dir && typeof dir === 'string') {
          setScanPath(dir)
        }
      }).catch(err => setError(String(err)))
    }).catch(err => setError(String(err)))
  }

  const handleScan = () => {
    if (!scanPath) return
    setLoading(true)
    setError(null)

    if (isTauri()) {
      casparianScan({
        path: scanPath,
        pattern: pattern || undefined,
        recursive: true,
        limit: 1000,
      }).then(result => {
        setFiles(result.files.map(f => ({
          path: f.path,
          size: f.size,
          modifiedAt: '', // Not returned by backend yet
        })))
      }).catch(err => {
        setError(String(err))
      }).finally(() => {
        setLoading(false)
      })
    } else {
      // Mock data for browser development
      setTimeout(() => {
        setFiles([
          { path: `${scanPath}/sales_2024_10.csv`, size: 2400000, modifiedAt: '2024-01-15' },
          { path: `${scanPath}/sales_2024_11.csv`, size: 2100000, modifiedAt: '2024-01-15' },
          { path: `${scanPath}/transactions.xlsx`, size: 1500000, modifiedAt: '2024-01-14' },
        ])
        setLoading(false)
      }, 500)
    }
  }

  const handleStartSession = () => {
    if (isTauri()) {
      sessionCreate({ intent: `Process files from ${scanPath}`, inputDir: scanPath }).then(response => {
        navigate(`/sessions/${response.sessionId}`)
      }).catch(err => setError(String(err)))
    } else {
      navigate('/sessions/new')
    }
  }

  const formatSize = (bytes: number) => {
    if (bytes < 1024) return `${bytes} B`
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  }

  return (
    <main className="main-content" data-testid="discover-screen">
      <div className="page-header">
        <div>
          <h1 className="page-title">Discover</h1>
          <p className="page-subtitle">Scan directories and discover data files</p>
        </div>
      </div>

      {error && (
        <div className="alert alert-error" style={{ marginBottom: 16 }}>
          <span className="material-symbols-sharp" style={{ fontSize: 18, marginRight: 8 }}>error</span>
          {error}
        </div>
      )}

      <div className="card">
        <div className="card-header">
          <span className="card-title">Scan Directory</span>
        </div>
        <div className="card-body">
          <div className="form-group">
            <label className="form-label">Directory Path</label>
            <div className="input-with-button">
              <input
                type="text"
                className="form-input"
                value={scanPath}
                onChange={(e) => setScanPath(e.target.value)}
                placeholder="/path/to/data"
                data-testid="scan-path-input"
              />
              <button className="btn btn-outline" onClick={handleBrowse} data-testid="browse-btn">
                <span className="material-symbols-sharp" style={{ fontSize: 18 }}>folder_open</span>
              </button>
            </div>
          </div>
          <div className="form-group">
            <label className="form-label">File Pattern (glob)</label>
            <input
              type="text"
              className="form-input"
              value={pattern}
              onChange={(e) => setPattern(e.target.value)}
              placeholder="*.csv"
              data-testid="pattern-input"
            />
          </div>
          <button
            className="btn btn-primary"
            onClick={handleScan}
            disabled={loading || !scanPath}
            data-testid="scan-btn"
          >
            {loading ? (
              <>
                <span className="material-symbols-sharp spinning" style={{ fontSize: 18 }}>progress_activity</span>
                Scanning...
              </>
            ) : (
              <>
                <span className="material-symbols-sharp" style={{ fontSize: 18 }}>search</span>
                Scan Directory
              </>
            )}
          </button>
        </div>
      </div>

      {files.length > 0 && (
        <div className="card" style={{ marginTop: 16 }}>
          <div className="card-header">
            <span className="card-title">Discovered Files</span>
            <span className="text-muted">{files.length} files found</span>
          </div>
          <div className="table-header">
            <span style={{ flex: 2 }}>Path</span>
            <span style={{ width: 100 }}>Size</span>
            <span style={{ width: 120 }}>Modified</span>
          </div>
          <div data-testid="files-list">
            {files.slice(0, 50).map((file, index) => (
              <div key={index} className="table-row">
                <span className="table-cell-mono" style={{ flex: 2, fontSize: 12 }}>{file.path}</span>
                <span className="text-muted" style={{ width: 100 }}>{formatSize(file.size)}</span>
                <span className="text-muted" style={{ width: 120 }}>{file.modifiedAt}</span>
              </div>
            ))}
            {files.length > 50 && (
              <div className="table-row text-muted" style={{ justifyContent: 'center' }}>
                ... and {files.length - 50} more files
              </div>
            )}
          </div>
          <div className="card-body" style={{ borderTop: '1px solid var(--border)' }}>
            <button className="btn btn-primary" onClick={handleStartSession} data-testid="start-session-btn">
              <span className="material-symbols-sharp" style={{ fontSize: 18 }}>conversion_path</span>
              Start Processing Session
            </button>
          </div>
        </div>
      )}
    </main>
  )
}
