import { useState } from 'react'
import { caspSchemaResolveAmbiguity, caspSchemaPromote } from '../../api/commands'

interface SchemaIntentStepProps {
  sessionId: string
  onApproved?: () => void
  question?: {
    id: string
    kind: string
    text: string
    options: Array<{ id: string; label: string; description: string }>
  }
}

// Mock data
const mockSchemaProposal = {
  proposalId: 'schema-prop-001',
  columns: [
    {
      name: 'order_id',
      source: 'parsed',
      declaredType: 'string',
      nullable: false,
      inference: {
        method: 'constraint_elimination',
        candidates: ['string'],
        confidence: 0.99,
      },
    },
    {
      name: 'customer_name',
      source: 'parsed',
      declaredType: 'string',
      nullable: true,
      inference: {
        method: 'constraint_elimination',
        candidates: ['string'],
        confidence: 0.95,
      },
    },
    {
      name: 'amount',
      source: 'parsed',
      declaredType: 'float64',
      nullable: false,
      inference: {
        method: 'ambiguous_requires_human',
        candidates: ['int64', 'float64'],
        confidence: 0.65,
      },
    },
    {
      name: 'order_date',
      source: 'parsed',
      declaredType: 'date',
      nullable: false,
      inference: {
        method: 'constraint_elimination',
        candidates: ['date'],
        confidence: 0.92,
      },
    },
    {
      name: 'quantity',
      source: 'parsed',
      declaredType: 'int64',
      nullable: false,
      inference: {
        method: 'constraint_elimination',
        candidates: ['int64'],
        confidence: 0.98,
      },
    },
    {
      name: 'path_year',
      source: 'derived',
      declaredType: 'int64',
      nullable: false,
      inference: {
        method: 'constraint_elimination',
        candidates: ['int64'],
        confidence: 0.99,
      },
    },
    {
      name: 'path_quarter',
      source: 'derived',
      declaredType: 'string',
      nullable: false,
      inference: {
        method: 'constraint_elimination',
        candidates: ['string'],
        confidence: 0.99,
      },
    },
  ],
  safeDefaults: {
    timestampTimezone: 'require_utc',
    stringTruncation: 'reject',
    numericOverflow: 'reject',
  },
}

export default function SchemaIntentStep({ sessionId, onApproved, question }: SchemaIntentStepProps) {
  const [proposal, setProposal] = useState(mockSchemaProposal)
  const [selectedAnswer, setSelectedAnswer] = useState<string | null>(null)
  const [editingColumn, setEditingColumn] = useState<string | null>(null)
  const [approvalToken] = useState<string | null>(null)

  const handleAnswerQuestion = async () => {
    if (!selectedAnswer || !question) return
    try {
      await caspSchemaResolveAmbiguity({
        sessionId,
        proposalId: proposal.proposalId,
        resolutions: { [question.id]: selectedAnswer },
        approvalTokenHash: approvalToken || 'mock-token',
      })
      setSelectedAnswer(null)
    } catch (err) {
      console.error('Failed to resolve ambiguity:', err)
    }
  }

  const handleApprove = async () => {
    try {
      await caspSchemaPromote({
        sessionId,
        schemaProposalId: proposal.proposalId,
        schemaName: 'output_schema',
      })
      onApproved?.()
    } catch (err) {
      console.error('Schema approval failed:', err)
    }
  }

  const handleTypeChange = (columnName: string, newType: string) => {
    setProposal(prev => ({
      ...prev,
      columns: prev.columns.map(c =>
        c.name === columnName ? { ...c, declaredType: newType } : c
      ),
    }))
    setEditingColumn(null)
  }

  const handleSafeDefaultChange = (key: keyof typeof proposal.safeDefaults, value: string) => {
    setProposal(prev => ({
      ...prev,
      safeDefaults: { ...prev.safeDefaults, [key]: value },
    }))
  }

  const getConfidenceColor = (confidence: number) => {
    if (confidence >= 0.9) return 'var(--success-foreground)'
    if (confidence >= 0.7) return 'var(--warning-foreground)'
    return 'var(--destructive)'
  }

  const getTypeIcon = (type: string) => {
    if (type.includes('int')) return 'tag'
    if (type.includes('float')) return 'decimal_increase'
    if (type.includes('string')) return 'text_fields'
    if (type.includes('date')) return 'calendar_today'
    if (type.includes('timestamp')) return 'schedule'
    if (type.includes('bool')) return 'toggle_on'
    return 'data_object'
  }

  const hasAmbiguities = proposal.columns.some(c => c.inference.method === 'ambiguous_requires_human')

  return (
    <div className="step-container" data-testid="schema-intent-step">
      {/* Human Question Card - shown when there's an ambiguity */}
      {question && (
        <div className="card card-highlight" data-testid="question-card">
          <div className="card-header">
            <span className="card-title">
              <span className="material-symbols-sharp" style={{ fontSize: 20, marginRight: 8, color: 'var(--warning-foreground)' }}>
                help
              </span>
              Human Input Required
            </span>
          </div>
          <div className="card-body">
            <p className="question-text">{question.text}</p>

            <div className="question-options" data-testid="question-options">
              {question.options.map((option) => (
                <label
                  key={option.id}
                  className={`option-card ${selectedAnswer === option.id ? 'option-card-selected' : ''}`}
                >
                  <input
                    type="radio"
                    name="answer"
                    value={option.id}
                    checked={selectedAnswer === option.id}
                    onChange={() => setSelectedAnswer(option.id)}
                  />
                  <div className="option-content">
                    <span className="option-label">{option.label}</span>
                    <span className="option-description">{option.description}</span>
                  </div>
                </label>
              ))}
            </div>

            <button
              className="btn btn-primary"
              onClick={handleAnswerQuestion}
              disabled={!selectedAnswer}
              data-testid="submit-answer-btn"
            >
              <span className="material-symbols-sharp" style={{ fontSize: 18 }}>check</span>
              Submit Answer
            </button>
          </div>
        </div>
      )}

      {/* Schema Columns Table */}
      <div className="card">
        <div className="card-header">
          <span className="card-title">Schema Columns</span>
          <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
            <span className="text-muted">{proposal.columns.length} columns</span>
            {hasAmbiguities && (
              <span className="badge badge-warning">
                {proposal.columns.filter(c => c.inference.method === 'ambiguous_requires_human').length} ambiguous
              </span>
            )}
          </div>
        </div>
        <div className="schema-table-container">
          <table className="schema-table" data-testid="schema-table">
            <thead>
              <tr>
                <th style={{ width: 180 }}>Column Name</th>
                <th style={{ width: 100 }}>Source</th>
                <th style={{ width: 120 }}>Type</th>
                <th style={{ width: 80 }}>Nullable</th>
                <th style={{ width: 100 }}>Confidence</th>
                <th style={{ flex: 1 }}>Candidates</th>
                <th style={{ width: 80 }}></th>
              </tr>
            </thead>
            <tbody>
              {proposal.columns.map((column) => {
                const isAmbiguous = column.inference.method === 'ambiguous_requires_human'
                return (
                  <tr
                    key={column.name}
                    className={isAmbiguous ? 'row-ambiguous' : ''}
                    data-testid={`column-row-${column.name}`}
                  >
                    <td>
                      <div className="column-name-cell">
                        <span className="material-symbols-sharp" style={{ fontSize: 16, opacity: 0.5 }}>
                          {getTypeIcon(column.declaredType)}
                        </span>
                        <span className="table-cell-mono">{column.name}</span>
                      </div>
                    </td>
                    <td>
                      <span className={`source-badge source-${column.source}`}>
                        {column.source}
                      </span>
                    </td>
                    <td>
                      {editingColumn === column.name ? (
                        <select
                          className="type-select"
                          value={column.declaredType}
                          onChange={(e) => handleTypeChange(column.name, e.target.value)}
                          onBlur={() => setEditingColumn(null)}
                          autoFocus
                        >
                          {column.inference.candidates.map(c => (
                            <option key={c} value={c}>{c}</option>
                          ))}
                        </select>
                      ) : (
                        <span className="table-cell-mono">{column.declaredType}</span>
                      )}
                    </td>
                    <td>
                      <span className={column.nullable ? 'text-muted' : ''}>
                        {column.nullable ? 'yes' : 'no'}
                      </span>
                    </td>
                    <td>
                      <span
                        className="confidence-value"
                        style={{ color: getConfidenceColor(column.inference.confidence) }}
                      >
                        {Math.round(column.inference.confidence * 100)}%
                      </span>
                    </td>
                    <td>
                      <div className="candidates-cell">
                        {column.inference.candidates.map((c, idx) => (
                          <span
                            key={idx}
                            className={`candidate-badge ${c === column.declaredType ? 'candidate-selected' : ''}`}
                          >
                            {c}
                          </span>
                        ))}
                      </div>
                    </td>
                    <td>
                      <button
                        className="btn btn-ghost btn-sm"
                        onClick={() => setEditingColumn(column.name)}
                        title="Edit type"
                      >
                        <span className="material-symbols-sharp" style={{ fontSize: 16 }}>edit</span>
                      </button>
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        </div>
      </div>

      {/* Safe Defaults */}
      <div className="card">
        <div className="card-header">
          <span className="card-title">Safe Defaults</span>
          <span className="text-muted">How to handle edge cases</span>
        </div>
        <div className="card-body">
          <div className="safe-defaults-grid">
            <div className="safe-default-item">
              <label className="form-label">Timestamp Timezone</label>
              <select className="form-select" value={proposal.safeDefaults.timestampTimezone} onChange={(e) => handleSafeDefaultChange('timestampTimezone', e.target.value)}>
                <option value="require_utc">Require UTC</option>
                <option value="assume_utc">Assume UTC if missing</option>
                <option value="local">Use local timezone</option>
              </select>
            </div>
            <div className="safe-default-item">
              <label className="form-label">String Truncation</label>
              <select className="form-select" value={proposal.safeDefaults.stringTruncation} onChange={(e) => handleSafeDefaultChange('stringTruncation', e.target.value)}>
                <option value="reject">Reject (fail on overflow)</option>
                <option value="truncate">Truncate to max length</option>
                <option value="warn">Warn and truncate</option>
              </select>
            </div>
            <div className="safe-default-item">
              <label className="form-label">Numeric Overflow</label>
              <select className="form-select" value={proposal.safeDefaults.numericOverflow} onChange={(e) => handleSafeDefaultChange('numericOverflow', e.target.value)}>
                <option value="reject">Reject (fail on overflow)</option>
                <option value="clamp">Clamp to max value</option>
                <option value="null">Set to NULL</option>
              </select>
            </div>
          </div>
        </div>
      </div>

      {/* Action Buttons */}
      <div className="step-actions">
        <button className="btn btn-outline" data-testid="add-column-btn" onClick={() => {
          setProposal(prev => ({
            ...prev,
            columns: [...prev.columns, {
              name: `new_column_${prev.columns.length + 1}`,
              source: 'manual',
              declaredType: 'string',
              nullable: true,
              inference: { method: 'manual', candidates: ['string', 'int64', 'float64'], confidence: 1.0 },
            }],
          }))
        }}>
          <span className="material-symbols-sharp" style={{ fontSize: 18 }}>add</span>
          Add Column
        </button>
        <button
          className="btn btn-primary"
          onClick={handleApprove}
          disabled={hasAmbiguities}
          data-testid="approve-schema-btn"
        >
          <span className="material-symbols-sharp" style={{ fontSize: 18 }}>check</span>
          {hasAmbiguities ? 'Resolve Ambiguities First' : 'Approve Schema'}
        </button>
      </div>
    </div>
  )
}
