import { useState } from 'react'

export default function Settings() {
  const [theme, setTheme] = useState<'system' | 'light' | 'dark'>('system')
  const [auditLog, setAuditLog] = useState(true)
  const [defaultSinkPath, setDefaultSinkPath] = useState('/output')

  return (
    <main className="main-content" data-testid="settings-screen">
      <div className="page-header">
        <div>
          <h1 className="page-title">Settings</h1>
          <p className="page-subtitle">Configure application preferences</p>
        </div>
      </div>

      <div className="card">
        <div className="card-header">
          <span className="card-title">Appearance</span>
        </div>
        <div className="card-body">
          <div className="form-group">
            <label className="form-label">Theme</label>
            <select
              className="form-select"
              value={theme}
              onChange={(e) => setTheme(e.target.value as 'system' | 'light' | 'dark')}
            >
              <option value="system">System Default</option>
              <option value="light">Light</option>
              <option value="dark">Dark</option>
            </select>
          </div>
        </div>
      </div>

      <div className="card" style={{ marginTop: 16 }}>
        <div className="card-header">
          <span className="card-title">Security</span>
        </div>
        <div className="card-body">
          <div className="form-group">
            <label className="checkbox-label">
              <input
                type="checkbox"
                checked={auditLog}
                onChange={(e) => setAuditLog(e.target.checked)}
              />
              <span>Enable Audit Logging</span>
            </label>
            <p className="form-hint">Log all operations for compliance and debugging</p>
          </div>
        </div>
      </div>

      <div className="card" style={{ marginTop: 16 }}>
        <div className="card-header">
          <span className="card-title">Defaults</span>
        </div>
        <div className="card-body">
          <div className="form-group">
            <label className="form-label">Default Output Sink Path</label>
            <input
              type="text"
              className="form-input"
              value={defaultSinkPath}
              onChange={(e) => setDefaultSinkPath(e.target.value)}
              placeholder="/output"
            />
            <p className="form-hint">Default directory for parquet output files</p>
          </div>
        </div>
      </div>

      <div className="card" style={{ marginTop: 16 }}>
        <div className="card-header">
          <span className="card-title">About</span>
        </div>
        <div className="card-body">
          <div className="about-info">
            <div className="about-row">
              <span className="text-muted">Application</span>
              <span>Casparian Flow</span>
            </div>
            <div className="about-row">
              <span className="text-muted">Version</span>
              <span className="table-cell-mono">0.1.0</span>
            </div>
            <div className="about-row">
              <span className="text-muted">Build</span>
              <span className="table-cell-mono">dev</span>
            </div>
          </div>
        </div>
      </div>
    </main>
  )
}
