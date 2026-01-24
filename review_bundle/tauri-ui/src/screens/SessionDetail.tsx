import { useState, useEffect } from 'react'
import { useParams, useNavigate } from 'react-router-dom'
import { invoke } from '@tauri-apps/api/tauri'
import SelectionStep from '../components/intent/SelectionStep'
import TagRulesStep from '../components/intent/TagRulesStep'
import PathFieldsStep from '../components/intent/PathFieldsStep'
import SchemaIntentStep from '../components/intent/SchemaIntentStep'
import BacktestStep from '../components/intent/BacktestStep'
import PublishRunStep from '../components/intent/PublishRunStep'

// Workflow steps definition
const workflowSteps = [
  { id: 'selection', label: 'File Selection', icon: 'folder_open', states: ['S1_SCANNED', 'S2_SELECTION_PROPOSED', 'G1_SELECTION_APPROVED'] },
  { id: 'tags', label: 'Tag Rules', icon: 'label', states: ['S3_TAG_RULES_PROPOSED', 'G2_TAG_RULES_APPROVED'] },
  { id: 'pathfields', label: 'Path Fields', icon: 'route', states: ['S4_PATH_FIELDS_PROPOSED', 'G3_PATH_FIELDS_APPROVED'] },
  { id: 'schema', label: 'Schema Intent', icon: 'schema', states: ['S5_SCHEMA_INTENT_PROPOSED', 'G4_SCHEMA_INTENT_APPROVED'] },
  { id: 'backtest', label: 'Backtest', icon: 'science', states: ['S6_GENERATE_PARSER_DRAFT', 'S7_BACKTEST_FAIL_FAST', 'S8_BACKTEST_FULL'] },
  { id: 'publish', label: 'Publish & Run', icon: 'rocket_launch', states: ['S9_PUBLISH_PLAN_PROPOSED', 'G5_PUBLISH_APPROVED', 'S10_PUBLISHING', 'S11_RUN_PLAN_PROPOSED', 'G6_RUN_APPROVED', 'S12_COMPLETE'] },
]

// Mock session data - in production this would come from Tauri backend
const mockSessionData = {
  id: 'a1b2c3d4-e5f6-7890-abcd-ef1234567890',
  intent: 'Process all sales CSV files from Q4',
  state: 'S11_RUN_PLAN_PROPOSED', // Advanced state so all steps are accessible for testing
  createdAt: '2024-01-15T10:30:00Z',
  fileSetId: 'fs-001',
  filesSelected: 247,
  currentQuestion: {
    id: 'q-001',
    kind: 'type_ambiguity',
    text: 'The column "amount" has ambiguous type. Should it be parsed as integer or decimal?',
    options: [
      { id: 'opt-1', label: 'Integer (int64)', description: 'Whole numbers only, truncates decimals' },
      { id: 'opt-2', label: 'Decimal (float64)', description: 'Supports decimal values' },
      { id: 'opt-3', label: 'String', description: 'Keep as text, no numeric operations' },
    ],
  },
}

// Derive active step from session state
function getActiveStepFromState(state: string): string {
  for (const step of workflowSteps) {
    if (step.states.includes(state)) {
      return step.id
    }
  }
  return 'selection' // Default to first step
}

export default function SessionDetail() {
  const { sessionId } = useParams<{ sessionId: string }>()
  const navigate = useNavigate()
  const [session, setSession] = useState(mockSessionData)
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    if (sessionId && sessionId !== 'new') {
      fetchSession()
    } else {
      setLoading(false)
    }
  }, [sessionId])

  const fetchSession = async () => {
    try {
      const result = await invoke<{
        id: string
        intent: string
        state: string
        createdAt: string
        fileSetId: string | null
        filesSelected: number
        currentQuestion: typeof mockSessionData.currentQuestion | null
      }>('session_status', { sessionId })
      setSession({
        ...mockSessionData,
        id: result.id,
        intent: result.intent,
        state: result.state,
        createdAt: result.createdAt,
        fileSetId: result.fileSetId || 'fs-001',
        filesSelected: result.filesSelected,
        currentQuestion: result.currentQuestion || mockSessionData.currentQuestion,
      })
    } catch (err) {
      console.error('Failed to fetch session:', err)
    } finally {
      setLoading(false)
    }
  }

  const handleSave = async () => {
    // Sessions auto-save, but we can refresh the data
    await fetchSession()
  }

  // Derive active step from session state
  const [activeStep, setActiveStep] = useState(() => getActiveStepFromState(session.state))

  const isNewSession = sessionId === 'new'

  const getCurrentStepIndex = () => {
    return workflowSteps.findIndex(step => step.id === activeStep)
  }

  const getStepStatus = (stepId: string) => {
    const currentIndex = getCurrentStepIndex()
    const stepIndex = workflowSteps.findIndex(step => step.id === stepId)

    if (stepIndex < currentIndex) return 'completed'
    if (stepIndex === currentIndex) return 'active'
    return 'pending'
  }

  const renderStepContent = () => {
    switch (activeStep) {
      case 'selection':
        return <SelectionStep sessionId={sessionId || ''} />
      case 'tags':
        return <TagRulesStep sessionId={sessionId || ''} />
      case 'pathfields':
        return <PathFieldsStep sessionId={sessionId || ''} />
      case 'schema':
        return <SchemaIntentStep sessionId={sessionId || ''} question={session.currentQuestion} />
      case 'backtest':
        return <BacktestStep sessionId={sessionId || ''} />
      case 'publish':
        return <PublishRunStep sessionId={sessionId || ''} />
      default:
        return <SelectionStep sessionId={sessionId || ''} />
    }
  }

  return (
    <main className="main-content" data-testid="session-detail-screen">
      <div className="page-header">
        <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
          <button
            className="btn btn-ghost"
            onClick={() => navigate('/sessions')}
            data-testid="back-btn"
          >
            <span className="material-symbols-sharp">arrow_back</span>
          </button>
          <div>
            <h1 className="page-title">
              {isNewSession ? 'New Session' : session.intent}
            </h1>
            <p className="page-subtitle">
              {isNewSession ? 'Create a new intent pipeline' : `Session ${sessionId?.slice(0, 8)}...`}
            </p>
          </div>
        </div>
        <div className="header-actions">
          <button className="btn btn-outline" data-testid="save-btn" onClick={handleSave}>
            <span className="material-symbols-sharp" style={{ fontSize: 18 }}>save</span>
            {loading ? 'Loading...' : 'Refresh'}
          </button>
        </div>
      </div>

      {/* Workflow Progress Stepper */}
      <div className="workflow-stepper" data-testid="workflow-stepper">
        {workflowSteps.map((step, index) => {
          const status = getStepStatus(step.id)
          return (
            <div key={step.id} className="workflow-step-container">
              <button
                className={`workflow-step workflow-step-${status}`}
                onClick={() => status !== 'pending' && setActiveStep(step.id)}
                disabled={status === 'pending'}
                data-testid={`step-${step.id}`}
              >
                <div className="workflow-step-icon">
                  {status === 'completed' ? (
                    <span className="material-symbols-sharp">check</span>
                  ) : (
                    <span className="material-symbols-sharp">{step.icon}</span>
                  )}
                </div>
                <span className="workflow-step-label">{step.label}</span>
              </button>
              {index < workflowSteps.length - 1 && (
                <div className={`workflow-step-connector workflow-step-connector-${status === 'completed' ? 'completed' : 'pending'}`} />
              )}
            </div>
          )
        })}
      </div>

      {/* Session Info Bar */}
      {!isNewSession && (
        <div className="session-info-bar" data-testid="session-info">
          <div className="session-info-item">
            <span className="material-symbols-sharp text-muted">folder</span>
            <span>{session.filesSelected.toLocaleString()} files selected</span>
          </div>
          <div className="session-info-item">
            <span className="material-symbols-sharp text-muted">schedule</span>
            <span>Created {new Date(session.createdAt).toLocaleDateString()}</span>
          </div>
          <div className="session-info-item">
            <span className="material-symbols-sharp text-muted">info</span>
            <span className="status-text">{session.state.replace(/_/g, ' ')}</span>
          </div>
        </div>
      )}

      {/* Step Content */}
      <div className="step-content" data-testid="step-content">
        {renderStepContent()}
      </div>
    </main>
  )
}
