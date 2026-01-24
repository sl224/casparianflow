import { useState } from 'react'
import { caspTagsApplyRules } from '../../api/commands'

interface TagRulesStepProps {
  sessionId: string
  onApproved?: () => void
}

// Mock data
const mockTagProposal = {
  proposalId: 'tag-prop-001',
  candidates: [
    {
      id: 'rule-1',
      tagName: 'sales_data',
      when: {
        type: 'extension_match',
        extensions: ['.csv', '.xlsx'],
      },
      confidence: 0.92,
      matchCount: 235,
      evaluation: {
        truePositives: 230,
        falsePositives: 5,
        falseNegatives: 2,
      },
    },
    {
      id: 'rule-2',
      tagName: 'transaction_log',
      when: {
        type: 'filename_pattern',
        pattern: 'transactions*.xlsx',
      },
      confidence: 0.88,
      matchCount: 12,
      evaluation: {
        truePositives: 11,
        falsePositives: 1,
        falseNegatives: 0,
      },
    },
    {
      id: 'rule-3',
      tagName: 'sales_data',
      when: {
        type: 'path_contains',
        segment: '/sales/',
      },
      confidence: 0.75,
      matchCount: 247,
      evaluation: {
        truePositives: 235,
        falsePositives: 12,
        falseNegatives: 0,
      },
    },
  ],
  conflicts: [
    {
      rule1: 'rule-1',
      rule2: 'rule-3',
      overlapFiles: 235,
      description: 'Both rules match the same CSV files in /sales/ directory',
    },
  ],
}

export default function TagRulesStep({ sessionId, onApproved }: TagRulesStepProps) {
  const [proposal, setProposal] = useState(mockTagProposal)
  const [selectedRules, setSelectedRules] = useState<string[]>(['rule-1', 'rule-2'])
  const [approvalToken] = useState<string | null>(null)

  const handleToggleRule = (ruleId: string) => {
    setSelectedRules(prev =>
      prev.includes(ruleId)
        ? prev.filter(id => id !== ruleId)
        : [...prev, ruleId]
    )
  }

  const handleApprove = async () => {
    try {
      for (const ruleId of selectedRules) {
        await caspTagsApplyRules({
          sessionId,
          proposalId: proposal.proposalId,
          selectedRuleId: ruleId,
          approvalTokenHash: approvalToken || 'mock-token',
        })
      }
      onApproved?.()
    } catch (err) {
      console.error('Tag rules approval failed:', err)
    }
  }

  const getConditionDescription = (when: { type: string; extensions?: string[]; pattern?: string; segment?: string }) => {
    switch (when.type) {
      case 'extension_match':
        return `Extension is ${when.extensions?.join(' or ')}`
      case 'filename_pattern':
        return `Filename matches "${when.pattern}"`
      case 'path_contains':
        return `Path contains "${when.segment}"`
      default:
        return when.type
    }
  }

  const getConfidenceColor = (score: number) => {
    if (score >= 0.85) return 'var(--success-foreground)'
    if (score >= 0.7) return 'var(--warning-foreground)'
    return 'var(--destructive)'
  }

  return (
    <div className="step-container" data-testid="tag-rules-step">
      {/* Proposed Rules */}
      <div className="card">
        <div className="card-header">
          <span className="card-title">Proposed Tag Rules</span>
          <span className="text-muted">{proposal.candidates.length} rules generated</span>
        </div>
        <div className="card-body">
          <p className="help-text">
            Select which tag rules to apply. Rules determine how files are routed to parsers.
          </p>

          <div className="rules-list" data-testid="rules-list">
            {proposal.candidates.map((rule) => {
              const isSelected = selectedRules.includes(rule.id)
              const hasConflict = proposal.conflicts.some(
                c => (c.rule1 === rule.id || c.rule2 === rule.id) && selectedRules.includes(c.rule1) && selectedRules.includes(c.rule2)
              )

              return (
                <div
                  key={rule.id}
                  className={`rule-card ${isSelected ? 'rule-card-selected' : ''} ${hasConflict ? 'rule-card-conflict' : ''}`}
                  data-testid={`rule-${rule.id}`}
                >
                  <div className="rule-header">
                    <label className="checkbox-label">
                      <input
                        type="checkbox"
                        checked={isSelected}
                        onChange={() => handleToggleRule(rule.id)}
                      />
                      <span className="rule-tag-name">{rule.tagName}</span>
                    </label>
                    <span
                      className="confidence-badge"
                      style={{ color: getConfidenceColor(rule.confidence) }}
                    >
                      {Math.round(rule.confidence * 100)}%
                    </span>
                  </div>

                  <div className="rule-condition">
                    <span className="material-symbols-sharp text-muted" style={{ fontSize: 16 }}>filter_alt</span>
                    <span>{getConditionDescription(rule.when)}</span>
                  </div>

                  <div className="rule-stats">
                    <span className="rule-stat">
                      <span className="rule-stat-value">{rule.matchCount}</span>
                      <span className="rule-stat-label">matches</span>
                    </span>
                    <span className="rule-stat text-success">
                      <span className="rule-stat-value">{rule.evaluation.truePositives}</span>
                      <span className="rule-stat-label">true pos</span>
                    </span>
                    <span className="rule-stat text-destructive">
                      <span className="rule-stat-value">{rule.evaluation.falsePositives}</span>
                      <span className="rule-stat-label">false pos</span>
                    </span>
                  </div>

                  {hasConflict && (
                    <div className="rule-conflict-warning">
                      <span className="material-symbols-sharp" style={{ fontSize: 16 }}>warning</span>
                      <span>Conflicts with another selected rule</span>
                    </div>
                  )}
                </div>
              )
            })}
          </div>
        </div>
      </div>

      {/* Conflicts Warning */}
      {proposal.conflicts.length > 0 && (
        <div className="card card-warning">
          <div className="card-header">
            <span className="card-title">
              <span className="material-symbols-sharp" style={{ fontSize: 18, marginRight: 8 }}>warning</span>
              Rule Conflicts
            </span>
          </div>
          <div className="card-body">
            {proposal.conflicts.map((conflict, index) => (
              <div key={index} className="conflict-item">
                <p>{conflict.description}</p>
                <p className="text-muted">
                  {conflict.overlapFiles} files matched by both rules
                </p>
              </div>
            ))}
            <p className="help-text">
              Tip: Select only one of the conflicting rules, or the first matching rule will take precedence.
            </p>
          </div>
        </div>
      )}

      {/* Preview of Tag Application */}
      <div className="card">
        <div className="card-header">
          <span className="card-title">Tag Application Preview</span>
        </div>
        <div className="card-body">
          <div className="tag-preview-grid">
            {selectedRules.map(ruleId => {
              const rule = proposal.candidates.find(r => r.id === ruleId)
              if (!rule) return null
              return (
                <div key={ruleId} className="tag-preview-item">
                  <span className="tag-badge">{rule.tagName}</span>
                  <span className="text-muted">→ {rule.matchCount} files</span>
                </div>
              )
            })}
          </div>
        </div>
      </div>

      {/* Action Buttons */}
      <div className="step-actions">
        <button className="btn btn-outline" data-testid="add-rule-btn" onClick={() => {
          setProposal(prev => ({
            ...prev,
            candidates: [...prev.candidates, {
              id: `rule-${prev.candidates.length + 1}`,
              tagName: 'custom_tag',
              when: { type: 'extension_match', extensions: ['.csv'] },
              confidence: 0.5,
              matchCount: 0,
              evaluation: { truePositives: 0, falsePositives: 0, falseNegatives: 0 },
            }],
          }))
        }}>
          <span className="material-symbols-sharp" style={{ fontSize: 18 }}>add</span>
          Add Custom Rule
        </button>
        <button
          className="btn btn-primary"
          onClick={handleApprove}
          disabled={selectedRules.length === 0}
          data-testid="apply-rules-btn"
        >
          <span className="material-symbols-sharp" style={{ fontSize: 18 }}>check</span>
          Apply Selected Rules ({selectedRules.length})
        </button>
      </div>
    </div>
  )
}
