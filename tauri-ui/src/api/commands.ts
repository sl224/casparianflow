/**
 * TypeScript wrappers for Tauri commands.
 *
 * These functions provide type-safe access to the Rust backend.
 */

import { invoke } from '@tauri-apps/api/tauri'
import type {
  SessionSummary,
  SessionStatus,
  CreateSessionRequest,
  CreateSessionResponse,
  ApprovalItem,
  ApprovalStats,
  ApprovalDecision,
  ApprovalDecisionResponse,
  QueryRequest,
  QueryResult,
  JobItem,
  JobCancelResponse,
  DashboardStats,
} from './types'

// =============================================================================
// Session Commands
// =============================================================================

/**
 * List all sessions.
 */
export async function sessionList(): Promise<SessionSummary[]> {
  return invoke<SessionSummary[]>('session_list')
}

/**
 * Create a new session.
 */
export async function sessionCreate(
  request: CreateSessionRequest
): Promise<CreateSessionResponse> {
  return invoke<CreateSessionResponse>('session_create', { request })
}

/**
 * Get session status by ID.
 */
export async function sessionStatus(sessionId: string): Promise<SessionStatus> {
  return invoke<SessionStatus>('session_status', { sessionId })
}

// =============================================================================
// Approval Commands
// =============================================================================

/**
 * List all approvals, optionally filtered by status.
 */
export async function approvalList(status?: string): Promise<ApprovalItem[]> {
  return invoke<ApprovalItem[]>('approval_list', { status })
}

/**
 * Decide on an approval (approve or reject).
 */
export async function approvalDecide(
  decision: ApprovalDecision
): Promise<ApprovalDecisionResponse> {
  return invoke<ApprovalDecisionResponse>('approval_decide', { decision })
}

/**
 * Get approval statistics.
 */
export async function approvalStats(): Promise<ApprovalStats> {
  return invoke<ApprovalStats>('approval_stats')
}

// =============================================================================
// Query Commands
// =============================================================================

/**
 * Execute a SQL query.
 */
export async function queryExecute(request: QueryRequest): Promise<QueryResult> {
  return invoke<QueryResult>('query_execute', { request })
}

// =============================================================================
// Job Commands
// =============================================================================

/**
 * List all jobs, optionally filtered by status.
 */
export async function jobList(
  status?: string,
  limit?: number
): Promise<JobItem[]> {
  return invoke<JobItem[]>('job_list', { status, limit })
}

/**
 * Get job status by ID.
 */
export async function jobStatus(jobId: string): Promise<JobItem> {
  return invoke<JobItem>('job_status', { jobId })
}

/**
 * Cancel a running job.
 */
export async function jobCancel(jobId: string): Promise<JobCancelResponse> {
  return invoke<JobCancelResponse>('job_cancel', { jobId })
}

// =============================================================================
// Dashboard Commands
// =============================================================================

/**
 * Get dashboard statistics.
 */
export async function dashboardStats(): Promise<DashboardStats> {
  return invoke<DashboardStats>('dashboard_stats')
}

// =============================================================================
// Intent Pipeline Commands - Selection
// =============================================================================

export interface SelectProposeRequest {
  sessionId: string
  baseDir: string
  patterns?: string[]
  semanticTokens?: string[]
  extensions?: string[]
  maxFiles?: number
}

export interface SelectProposeResponse {
  proposalId: string
  proposalHash: string
  fileSetId: string
  fileCount: number
  nearMissCount: number
  confidence: { score: number; label: string }
  evidence: { dirPrefixScore: number; extensionScore: number; semanticScore: number }
  sampleFiles: string[]
}

/**
 * Propose a file selection based on intent-derived criteria.
 */
export async function caspSelectPropose(
  request: SelectProposeRequest
): Promise<SelectProposeResponse> {
  return invoke<SelectProposeResponse>('casp_select_propose', { request })
}

export interface SelectApproveRequest {
  sessionId: string
  proposalId: string
  approvalTokenHash: string
}

export interface SelectApproveResponse {
  approved: boolean
  newState: string
}

/**
 * Approve a file selection proposal.
 */
export async function caspSelectApprove(
  request: SelectApproveRequest
): Promise<SelectApproveResponse> {
  return invoke<SelectApproveResponse>('casp_select_approve', { request })
}

// =============================================================================
// Intent Pipeline Commands - File Sets
// =============================================================================

export interface FileSetSampleRequest {
  sessionId: string
  fileSetId: string
  n?: number
}

export interface FileSetSampleResponse {
  files: string[]
  totalCount: number
  sampledCount: number
}

/**
 * Get a sample of files from a file set.
 */
export async function caspFilesetSample(
  request: FileSetSampleRequest
): Promise<FileSetSampleResponse> {
  return invoke<FileSetSampleResponse>('casp_fileset_sample', { request })
}

export interface FileSetInfoRequest {
  sessionId: string
  fileSetId: string
}

export interface FileSetInfoResponse {
  fileSetId: string
  count: number
  samplingMethod: string
}

/**
 * Get metadata about a file set.
 */
export async function caspFilesetInfo(
  request: FileSetInfoRequest
): Promise<FileSetInfoResponse> {
  return invoke<FileSetInfoResponse>('casp_fileset_info', { request })
}

// =============================================================================
// Intent Pipeline Commands - Tag Rules
// =============================================================================

export interface TagsApplyRulesRequest {
  sessionId: string
  proposalId: string
  selectedRuleId: string
  approvalTokenHash: string
}

export interface TagsApplyRulesResponse {
  applied: boolean
  newState: string
}

/**
 * Apply approved tagging rules.
 */
export async function caspTagsApplyRules(
  request: TagsApplyRulesRequest
): Promise<TagsApplyRulesResponse> {
  return invoke<TagsApplyRulesResponse>('casp_tags_apply_rules', { request })
}

// =============================================================================
// Intent Pipeline Commands - Path Fields
// =============================================================================

export interface PathFieldsApplyRequest {
  sessionId: string
  proposalId: string
  approvalTokenHash: string
  includedFields?: string[]
}

export interface PathFieldsApplyResponse {
  applied: boolean
  newState: string
}

/**
 * Apply approved path-derived fields.
 */
export async function caspPathFieldsApply(
  request: PathFieldsApplyRequest
): Promise<PathFieldsApplyResponse> {
  return invoke<PathFieldsApplyResponse>('casp_path_fields_apply', { request })
}

// =============================================================================
// Intent Pipeline Commands - Schema
// =============================================================================

export interface SchemaPromoteRequest {
  sessionId: string
  schemaProposalId: string
  schemaName: string
  schemaVersion?: string
}

export interface SchemaPromoteResponse {
  promoted: boolean
  schemaRef: string
  newState: string
}

/**
 * Promote ephemeral schema to schema-as-code.
 */
export async function caspSchemaPromote(
  request: SchemaPromoteRequest
): Promise<SchemaPromoteResponse> {
  return invoke<SchemaPromoteResponse>('casp_schema_promote', { request })
}

export interface SchemaResolveAmbiguityRequest {
  sessionId: string
  proposalId: string
  resolutions: Record<string, string>
  approvalTokenHash: string
}

export interface SchemaResolveAmbiguityResponse {
  resolved: boolean
  newState: string
}

/**
 * Resolve schema type ambiguities.
 */
export async function caspSchemaResolveAmbiguity(
  request: SchemaResolveAmbiguityRequest
): Promise<SchemaResolveAmbiguityResponse> {
  return invoke<SchemaResolveAmbiguityResponse>('casp_schema_resolve_ambiguity', { request })
}

// =============================================================================
// Intent Pipeline Commands - Backtest
// =============================================================================

export interface BacktestStartRequest {
  sessionId: string
  draftId: string
  fileSetId: string
  failFast?: boolean
}

export interface BacktestStartResponse {
  backtestJobId: string
  fileSetId: string
  failFast: boolean
}

/**
 * Start a backtest job.
 */
export async function caspIntentBacktestStart(
  request: BacktestStartRequest
): Promise<BacktestStartResponse> {
  return invoke<BacktestStartResponse>('casp_intent_backtest_start', { request })
}

export interface BacktestStatusRequest {
  sessionId: string
  backtestJobId: string
}

export interface BacktestStatusResponse {
  jobId: string
  phase: string
  elapsedMs: number
  filesProcessed: number
  filesTotal: number | null
  rowsEmitted: number
  rowsQuarantined: number
  stalled: boolean
}

/**
 * Get backtest job status.
 */
export async function caspIntentBacktestStatus(
  request: BacktestStatusRequest
): Promise<BacktestStatusResponse> {
  return invoke<BacktestStatusResponse>('casp_intent_backtest_status', { request })
}

export interface BacktestReportRequest {
  sessionId: string
  backtestJobId: string
}

export interface BacktestReportResponse {
  jobId: string
  quality: {
    filesProcessed: number
    rowsEmitted: number
    rowsQuarantined: number
    quarantinePct: number
    passRateFiles: number
  }
  topViolations: Array<{
    violationType: string
    count: number
    topColumns: Array<{ name: string; count: number }>
  }>
}

/**
 * Get backtest report.
 */
export async function caspIntentBacktestReport(
  request: BacktestReportRequest
): Promise<BacktestReportResponse> {
  return invoke<BacktestReportResponse>('casp_intent_backtest_report', { request })
}

// =============================================================================
// Intent Pipeline Commands - Patch
// =============================================================================

export interface PatchApplyRequest {
  sessionId: string
  patchType: 'schema' | 'parser' | 'rule'
  patchContent: unknown
  iterationId: string
}

export interface PatchApplyResponse {
  applied: boolean
  patchRef: string
  nextAction: string
}

/**
 * Apply a patch during backtest iteration.
 */
export async function caspPatchApply(
  request: PatchApplyRequest
): Promise<PatchApplyResponse> {
  return invoke<PatchApplyResponse>('casp_patch_apply', { request })
}

// =============================================================================
// Intent Pipeline Commands - Publish
// =============================================================================

export interface PublishPlanRequest {
  sessionId: string
  draftId: string
  schemaName: string
  schemaVersion: string
  parserName: string
  parserVersion: string
}

export interface PublishPlanResponse {
  proposalId: string
  approvalTokenHash: string
  schemaRef: string
  parserRef: string
  invariantsChecked: boolean
}

/**
 * Create a publish plan.
 */
export async function caspPublishPlan(
  request: PublishPlanRequest
): Promise<PublishPlanResponse> {
  return invoke<PublishPlanResponse>('casp_publish_plan', { request })
}

export interface PublishExecuteRequest {
  sessionId: string
  proposalId: string
  approvalTokenHash: string
}

export interface PublishExecuteResponse {
  published: boolean
  newState: string
}

/**
 * Execute a publish plan.
 */
export async function caspPublishExecute(
  request: PublishExecuteRequest
): Promise<PublishExecuteResponse> {
  return invoke<PublishExecuteResponse>('casp_publish_execute', { request })
}

// =============================================================================
// Intent Pipeline Commands - Run
// =============================================================================

export interface RunPlanRequest {
  sessionId: string
  fileSetId: string
  parserName: string
  parserVersion: string
  sinkUri: string
  routeToTopic: string
}

export interface RunPlanResponse {
  proposalId: string
  approvalTokenHash: string
  fileCount: number
  sinkUri: string
}

/**
 * Create a run plan.
 */
export async function caspRunPlan(
  request: RunPlanRequest
): Promise<RunPlanResponse> {
  return invoke<RunPlanResponse>('casp_run_plan', { request })
}

export interface RunExecuteRequest {
  sessionId: string
  proposalId: string
  approvalTokenHash: string
}

export interface RunExecuteResponse {
  started: boolean
  jobId: string
  newState: string
}

/**
 * Execute a run plan.
 */
export async function caspRunExecute(
  request: RunExecuteRequest
): Promise<RunExecuteResponse> {
  return invoke<RunExecuteResponse>('casp_run_execute', { request })
}

// =============================================================================
// Scan Commands
// =============================================================================

export interface ScanRequest {
  path: string
  pattern?: string
  limit?: number
  recursive?: boolean
}

export interface ScanResponse {
  files: Array<{
    path: string
    size: number
    extension: string | null
  }>
  totalScanned: number
  truncated: boolean
}

/**
 * Scan a directory for files.
 */
export async function casparianScan(
  request: ScanRequest
): Promise<ScanResponse> {
  return invoke<ScanResponse>('casparian_scan', { request })
}

// =============================================================================
// Parser Commands
// =============================================================================

export interface ParserInfo {
  name: string
  version: string
  topics: string[]
  outputs: string[]
}

export interface ParserListResponse {
  parsers: ParserInfo[]
}

/**
 * List available parsers.
 */
export async function parserList(): Promise<ParserListResponse> {
  return invoke<ParserListResponse>('parser_list')
}

// =============================================================================
// Utility: Check if running in Tauri
// =============================================================================

/**
 * Check if the app is running inside Tauri.
 * Returns false when running in browser (dev mode without Tauri).
 */
export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI__' in window
}

/**
 * Safely invoke a Tauri command, returning null if not in Tauri environment.
 */
export function safeInvoke<T>(
  cmd: string,
  args?: Record<string, unknown>
): Promise<T | null> {
  if (!isTauri()) {
    console.warn(`Tauri not available, skipping command: ${cmd}`)
    return Promise.resolve(null)
  }
  return invoke<T>(cmd, args).catch(error => {
    console.error(`Error invoking ${cmd}:`, error)
    throw error
  })
}
