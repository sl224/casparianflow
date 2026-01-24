/**
 * TypeScript types matching Rust response structs.
 *
 * These types are used by the Tauri invoke calls and React components.
 */

// =============================================================================
// Session Types
// =============================================================================

export interface SessionSummary {
  id: string
  intent: string
  state: string
  filesSelected: number
  createdAt: string
  hasQuestion: boolean
}

export interface SessionStatus {
  id: string
  intent: string
  state: string
  createdAt: string
  fileSetId: string | null
  filesSelected: number
  currentQuestion: SessionQuestion | null
}

export interface SessionQuestion {
  id: string
  kind: string
  text: string
  options: QuestionOption[]
}

export interface QuestionOption {
  id: string
  label: string
  description: string
}

export interface CreateSessionRequest {
  intent: string
  inputDir?: string
}

export interface CreateSessionResponse {
  sessionId: string
}

// =============================================================================
// Approval Types
// =============================================================================

export interface ApprovalItem {
  id: string
  operation: string
  plugin: string
  files: string
  expires: string
  urgent: boolean
  status: string
}

export interface ApprovalStats {
  pending: number
  approved: number
  rejected: number
  expired: number
}

export interface ApprovalDecision {
  approvalId: string
  decision: 'approve' | 'reject'
  reason?: string
}

export interface ApprovalDecisionResponse {
  success: boolean
  status: string
}

// =============================================================================
// Query Types
// =============================================================================

export interface QueryRequest {
  sql: string
  limit?: number
}

export interface QueryResult {
  columns: string[]
  rows: unknown[][]
  rowCount: number
  execTimeMs: number
}

// =============================================================================
// Job Types
// =============================================================================

export interface JobItem {
  id: string
  jobType: string
  status: string
  pluginName: string
  pluginVersion: string | null
  inputDir: string
  createdAt: string
  startedAt: string | null
  finishedAt: string | null
  errorMessage: string | null
  progress: JobProgress | null
}

export interface JobProgress {
  phase: string
  itemsDone: number
  itemsTotal: number | null
  message: string | null
}

export interface JobCancelResponse {
  success: boolean
  status: string
}

// =============================================================================
// Dashboard Types
// =============================================================================

export interface DashboardStats {
  readyOutputs: number
  runningJobs: number
  quarantinedRows: number
  failedJobs: number
  recentOutputs: OutputInfo[]
  activeRuns: ActiveRun[]
}

export interface OutputInfo {
  name: string
  rows: string
  updated: string
}

export interface ActiveRun {
  name: string
  progress: number
}

// =============================================================================
// Intent Pipeline Types (for future use)
// =============================================================================

export interface SelectionProposal {
  proposalHash: string
  fileCount: number
  sampleFiles: string[]
  suggestedSelectionExpression: string
}

export interface TagRuleProposal {
  proposalHash: string
  rules: TagRule[]
}

export interface TagRule {
  pattern: string
  tag: string
  confidence: number
}

export interface BacktestProgress {
  jobId: string
  phase: string
  elapsedMs: number
  metrics: BacktestMetrics
  topViolationSummary: ViolationSummary[]
  stalled: boolean
}

export interface BacktestMetrics {
  filesProcessed: number
  filesTotalEstimate: number | null
  rowsEmitted: number
  rowsQuarantined: number
}

export interface ViolationSummary {
  violationType: string
  count: number
  topColumns: ColumnCount[]
}

export interface ColumnCount {
  name: string
  count: number
}

export interface BacktestReport {
  jobId: string
  quality: QualityMetrics
  topKViolations: ViolationDetail[]
}

export interface QualityMetrics {
  filesProcessed: number
  rowsEmitted: number
  rowsQuarantined: number
  quarantinePct: number
  passRateFiles: number
}

export interface ViolationDetail {
  violationType: string
  count: number
  topColumns: ColumnCount[]
  exampleContexts: ViolationContext[]
}

export interface ViolationContext {
  file: string
  row: number
  value: unknown
}
