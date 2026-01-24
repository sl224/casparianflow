import { useState } from 'react'
import { caspPathFieldsApply } from '../../api/commands'

interface PathFieldsStepProps {
  sessionId: string
  onApproved?: () => void
}

// Mock data
const mockPathFieldsProposal = {
  proposalId: 'pf-prop-001',
  fields: [
    {
      fieldName: 'year',
      dtype: 'int',
      pattern: {
        type: 'regex',
        regex: '/data/sales/q\\d/(\\d{4})_\\d{2}\\.csv',
        group: 1,
      },
      coverage: { matchedFiles: 235, totalFiles: 247 },
      confidence: 0.95,
      sampleValues: ['2024', '2024', '2024', '2023'],
    },
    {
      fieldName: 'month',
      dtype: 'int',
      pattern: {
        type: 'regex',
        regex: '/data/sales/q\\d/\\d{4}_(\\d{2})\\.csv',
        group: 1,
      },
      coverage: { matchedFiles: 235, totalFiles: 247 },
      confidence: 0.95,
      sampleValues: ['10', '11', '12', '09'],
    },
    {
      fieldName: 'quarter',
      dtype: 'string',
      pattern: {
        type: 'segment',
        position: 3,
      },
      coverage: { matchedFiles: 247, totalFiles: 247 },
      confidence: 0.88,
      sampleValues: ['q4', 'q4', 'q4', 'q3'],
    },
    {
      fieldName: 'file_type',
      dtype: 'string',
      pattern: {
        type: 'extension',
      },
      coverage: { matchedFiles: 247, totalFiles: 247 },
      confidence: 0.99,
      sampleValues: ['csv', 'csv', 'xlsx', 'csv'],
    },
  ],
  collisions: [],
  namespacing: 'prefix_path',
}

export default function PathFieldsStep({ sessionId, onApproved }: PathFieldsStepProps) {
  const [proposal, setProposal] = useState(mockPathFieldsProposal)
  const [selectedFields, setSelectedFields] = useState<string[]>(
    proposal.fields.map(f => f.fieldName)
  )
  const [approvalToken] = useState<string | null>(null)

  const handleToggleField = (fieldName: string) => {
    setSelectedFields(prev =>
      prev.includes(fieldName)
        ? prev.filter(f => f !== fieldName)
        : [...prev, fieldName]
    )
  }

  const handleNamespacingChange = (value: string) => {
    setProposal(prev => ({ ...prev, namespacing: value }))
  }

  const handleApprove = async () => {
    try {
      await caspPathFieldsApply({
        sessionId,
        proposalId: proposal.proposalId,
        includedFields: selectedFields,
        approvalTokenHash: approvalToken || 'mock-token',
      })
      onApproved?.()
    } catch (err) {
      console.error('Path fields approval failed:', err)
    }
  }

  const getPatternDescription = (pattern: { type: string; regex?: string; group?: number; position?: number }) => {
    switch (pattern.type) {
      case 'regex':
        return `Regex: ${pattern.regex} (group ${pattern.group})`
      case 'segment':
        return `Path segment at position ${pattern.position}`
      case 'extension':
        return 'File extension'
      default:
        return pattern.type
    }
  }

  const getDtypeIcon = (dtype: string) => {
    switch (dtype) {
      case 'int': return 'tag'
      case 'date': return 'calendar_today'
      case 'timestamp': return 'schedule'
      default: return 'text_fields'
    }
  }

  const getCoveragePercent = (coverage: { matchedFiles: number; totalFiles: number }) => {
    return Math.round((coverage.matchedFiles / coverage.totalFiles) * 100)
  }

  return (
    <div className="step-container" data-testid="path-fields-step">
      {/* Explanation */}
      <div className="info-banner">
        <span className="material-symbols-sharp">info</span>
        <div>
          <strong>Path-Derived Fields</strong>
          <p>
            Extract metadata from file paths to add as columns in your output.
            These fields will be available alongside parsed data.
          </p>
        </div>
      </div>

      {/* Proposed Fields */}
      <div className="card">
        <div className="card-header">
          <span className="card-title">Detected Path Fields</span>
          <span className="text-muted">{proposal.fields.length} fields found</span>
        </div>
        <div className="card-body">
          <div className="path-fields-list" data-testid="fields-list">
            {proposal.fields.map((field) => {
              const isSelected = selectedFields.includes(field.fieldName)
              const coveragePercent = getCoveragePercent(field.coverage)

              return (
                <div
                  key={field.fieldName}
                  className={`path-field-card ${isSelected ? 'path-field-selected' : ''}`}
                  data-testid={`field-${field.fieldName}`}
                >
                  <div className="path-field-header">
                    <label className="checkbox-label">
                      <input
                        type="checkbox"
                        checked={isSelected}
                        onChange={() => handleToggleField(field.fieldName)}
                      />
                      <span className="material-symbols-sharp" style={{ fontSize: 18 }}>
                        {getDtypeIcon(field.dtype)}
                      </span>
                      <span className="path-field-name">{field.fieldName}</span>
                      <span className="dtype-badge">{field.dtype}</span>
                    </label>
                    <span
                      className="confidence-badge"
                      style={{
                        color: field.confidence >= 0.9
                          ? 'var(--success-foreground)'
                          : field.confidence >= 0.7
                            ? 'var(--warning-foreground)'
                            : 'var(--destructive)'
                      }}
                    >
                      {Math.round(field.confidence * 100)}%
                    </span>
                  </div>

                  <div className="path-field-pattern">
                    <span className="material-symbols-sharp text-muted" style={{ fontSize: 14 }}>
                      data_object
                    </span>
                    <code className="pattern-code">{getPatternDescription(field.pattern)}</code>
                  </div>

                  <div className="path-field-coverage">
                    <div className="coverage-bar-container">
                      <div
                        className="coverage-bar"
                        style={{ width: `${coveragePercent}%` }}
                      />
                    </div>
                    <span className="coverage-text">
                      {field.coverage.matchedFiles}/{field.coverage.totalFiles} files ({coveragePercent}%)
                    </span>
                  </div>

                  <div className="path-field-samples">
                    <span className="text-muted" style={{ fontSize: 12 }}>Sample values:</span>
                    <div className="sample-values">
                      {field.sampleValues.slice(0, 4).map((val, idx) => (
                        <span key={idx} className="sample-value">{val}</span>
                      ))}
                    </div>
                  </div>
                </div>
              )
            })}
          </div>
        </div>
      </div>

      {/* Namespacing Option */}
      <div className="card">
        <div className="card-header">
          <span className="card-title">Column Naming</span>
        </div>
        <div className="card-body">
          <div className="radio-group">
            <label className="radio-label">
              <input
                type="radio"
                name="namespacing"
                value="prefix_path"
                checked={proposal.namespacing === 'prefix_path'}
                onChange={() => handleNamespacingChange('prefix_path')}
              />
              <div>
                <span className="radio-title">Prefix with "path_"</span>
                <span className="radio-description">
                  e.g., path_year, path_month (prevents collisions with parsed columns)
                </span>
              </div>
            </label>
            <label className="radio-label">
              <input
                type="radio"
                name="namespacing"
                value="no_prefix"
                checked={proposal.namespacing === 'no_prefix'}
                onChange={() => handleNamespacingChange('no_prefix')}
              />
              <div>
                <span className="radio-title">No prefix</span>
                <span className="radio-description">
                  e.g., year, month (may conflict with parsed columns)
                </span>
              </div>
            </label>
          </div>
        </div>
      </div>

      {/* Preview Table */}
      <div className="card">
        <div className="card-header">
          <span className="card-title">Output Preview</span>
        </div>
        <div className="preview-table-container">
          <table className="preview-table">
            <thead>
              <tr>
                <th>File</th>
                {selectedFields.map(f => (
                  <th key={f} className="table-cell-mono">{f}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              <tr>
                <td className="table-cell-mono text-muted">/data/sales/q4/sales_2024_10.csv</td>
                {selectedFields.includes('year') && <td>2024</td>}
                {selectedFields.includes('month') && <td>10</td>}
                {selectedFields.includes('quarter') && <td>q4</td>}
                {selectedFields.includes('file_type') && <td>csv</td>}
              </tr>
              <tr>
                <td className="table-cell-mono text-muted">/data/sales/q4/sales_2024_11.csv</td>
                {selectedFields.includes('year') && <td>2024</td>}
                {selectedFields.includes('month') && <td>11</td>}
                {selectedFields.includes('quarter') && <td>q4</td>}
                {selectedFields.includes('file_type') && <td>csv</td>}
              </tr>
              <tr>
                <td className="table-cell-mono text-muted">/data/sales/q3/transactions.xlsx</td>
                {selectedFields.includes('year') && <td className="text-muted">—</td>}
                {selectedFields.includes('month') && <td className="text-muted">—</td>}
                {selectedFields.includes('quarter') && <td>q3</td>}
                {selectedFields.includes('file_type') && <td>xlsx</td>}
              </tr>
            </tbody>
          </table>
        </div>
      </div>

      {/* Action Buttons */}
      <div className="step-actions">
        <button className="btn btn-outline" data-testid="add-field-btn" onClick={() => {
          setProposal(prev => ({
            ...prev,
            fields: [...prev.fields, {
              fieldName: `custom_field_${prev.fields.length + 1}`,
              dtype: 'string',
              pattern: { type: 'segment', position: 0 },
              coverage: { matchedFiles: 0, totalFiles: prev.fields[0]?.coverage.totalFiles || 0 },
              confidence: 0.5,
              sampleValues: [],
            }],
          }))
        }}>
          <span className="material-symbols-sharp" style={{ fontSize: 18 }}>add</span>
          Add Custom Field
        </button>
        <button
          className="btn btn-primary"
          onClick={handleApprove}
          disabled={selectedFields.length === 0}
          data-testid="apply-fields-btn"
        >
          <span className="material-symbols-sharp" style={{ fontSize: 18 }}>check</span>
          Apply Path Fields ({selectedFields.length})
        </button>
      </div>
    </div>
  )
}
