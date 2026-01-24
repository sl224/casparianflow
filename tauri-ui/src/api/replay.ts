/**
 * Replay Mode API - WS7-08
 *
 * Provides a mock transport layer that reads responses from a tape file,
 * enabling UI-only replay without a running backend.
 *
 * Usage:
 *   // Load a tape file
 *   const context = await loadTapeForReplay('/path/to/session.tape')
 *
 *   // Use replay context in components
 *   const jobs = await replayJobList(context)
 */

import type {
  SessionSummary,
  JobItem,
  ApprovalItem,
  DashboardStats,
} from './types'

// =============================================================================
// Tape Format Types
// =============================================================================

interface TapeEnvelope {
  schema_version: number
  event_id: string
  seq: number
  timestamp: string
  correlation_id: string | null
  parent_id: string | null
  event_name: TapeEventName
  payload: Record<string, unknown>
}

type TapeEventName =
  | { type: 'tape_started' }
  | { type: 'tape_stopped' }
  | { type: 'ui_command'; name: string }
  | { type: 'domain_event'; name: string }
  | { type: 'system_response'; name: string }
  | { type: 'error_event'; name: string }

// =============================================================================
// Replay Context
// =============================================================================

export interface ReplayContext {
  /** All events from the tape */
  events: TapeEnvelope[]
  /** Schema version from tape */
  schemaVersion: number
  /** Extracted job data */
  jobs: Map<string, JobSummary>
  /** Extracted command responses */
  commandResponses: Map<string, unknown>
  /** Tape metadata */
  metadata: {
    eventCount: number
    startTime: string | null
    endTime: string | null
  }
}

interface JobSummary {
  jobId: string
  pluginName: string | null
  pluginVersion: string | null
  status: string
  rows: number | null
  outputs: string[]
  error: string | null
}

// =============================================================================
// Tape Parsing
// =============================================================================

/**
 * Parse a tape file content (NDJSON format) into a ReplayContext.
 */
export function parseTape(content: string): ReplayContext {
  const lines = content.split('\n').filter((line) => line.trim().length > 0)
  const events: TapeEnvelope[] = []

  for (const line of lines) {
    try {
      const envelope = JSON.parse(line) as TapeEnvelope
      events.push(envelope)
    } catch (e) {
      console.warn('Failed to parse tape line:', e)
    }
  }

  // Extract job data from domain events
  const jobs = new Map<string, JobSummary>()
  const commandResponses = new Map<string, unknown>()

  let schemaVersion = 1
  let startTime: string | null = null
  let endTime: string | null = null

  for (const event of events) {
    schemaVersion = event.schema_version

    if (event.event_name.type === 'tape_started') {
      startTime = event.timestamp
    } else if (event.event_name.type === 'tape_stopped') {
      endTime = event.timestamp
    } else if (event.event_name.type === 'domain_event') {
      processDomainEvent(event.event_name.name, event.payload, jobs)
    } else if (event.event_name.type === 'system_response') {
      // Store command responses keyed by correlation_id
      if (event.correlation_id) {
        commandResponses.set(event.correlation_id, event.payload)
      }
    }
  }

  return {
    events,
    schemaVersion,
    jobs,
    commandResponses,
    metadata: {
      eventCount: events.length,
      startTime,
      endTime,
    },
  }
}

function processDomainEvent(
  name: string,
  payload: Record<string, unknown>,
  jobs: Map<string, JobSummary>
) {
  const jobId = extractJobId(payload)
  if (!jobId) return

  switch (name) {
    case 'JobDispatched': {
      jobs.set(jobId, {
        jobId,
        pluginName: payload.plugin_name as string | null,
        pluginVersion: payload.plugin_version as string | null,
        status: 'dispatched',
        rows: null,
        outputs: [],
        error: null,
      })
      break
    }
    case 'JobCompleted': {
      const job = jobs.get(jobId)
      if (job) {
        job.status = 'completed'
        job.rows = (payload.rows as number) ?? null
      }
      break
    }
    case 'JobFailed': {
      const job = jobs.get(jobId)
      if (job) {
        job.status = 'failed'
        job.error = (payload.error as string) ?? null
      }
      break
    }
    case 'MaterializationRecorded': {
      const job = jobs.get(jobId)
      if (job) {
        const outputName = payload.output_name as string
        if (outputName && !job.outputs.includes(outputName)) {
          job.outputs.push(outputName)
        }
        job.rows = (payload.rows as number) ?? job.rows
      }
      break
    }
  }
}

function extractJobId(payload: Record<string, unknown>): string | null {
  const jobId = payload.job_id
  if (typeof jobId === 'string') return jobId
  if (typeof jobId === 'number') return jobId.toString()
  return null
}

// =============================================================================
// Replay API Functions
// =============================================================================

/**
 * Get job list from replay context.
 */
export function replayJobList(context: ReplayContext): JobItem[] {
  const jobs: JobItem[] = []

  for (const [, summary] of context.jobs) {
    jobs.push({
      id: summary.jobId,
      jobType: 'run',
      status: mapStatus(summary.status),
      pluginName: summary.pluginName ?? 'unknown',
      pluginVersion: summary.pluginVersion,
      inputDir: '/replay',
      createdAt: context.metadata.startTime ?? new Date().toISOString(),
      startedAt: context.metadata.startTime,
      finishedAt: context.metadata.endTime,
      errorMessage: summary.error,
      progress: summary.rows
        ? {
            phase: 'complete',
            itemsDone: summary.rows,
            itemsTotal: summary.rows,
            message: null,
          }
        : null,
    })
  }

  return jobs
}

function mapStatus(status: string): string {
  switch (status) {
    case 'dispatched':
      return 'running'
    case 'completed':
      return 'completed'
    case 'failed':
      return 'failed'
    default:
      return 'queued'
  }
}

/**
 * Get session list from replay context (mock data).
 */
export function replaySessionList(_context: ReplayContext): SessionSummary[] {
  return [
    {
      id: 'replay-session',
      intent: 'Replay Session',
      state: 'completed',
      filesSelected: 0,
      createdAt: _context.metadata.startTime ?? new Date().toISOString(),
      hasQuestion: false,
    },
  ]
}

/**
 * Get approval list from replay context (empty for now).
 */
export function replayApprovalList(_context: ReplayContext): ApprovalItem[] {
  return []
}

/**
 * Get dashboard stats from replay context.
 */
export function replayDashboardStats(context: ReplayContext): DashboardStats {
  let completedJobs = 0
  let failedJobs = 0
  let totalRows = 0
  const outputs: Set<string> = new Set()

  for (const [, job] of context.jobs) {
    if (job.status === 'completed') {
      completedJobs++
      totalRows += job.rows ?? 0
      for (const output of job.outputs) {
        outputs.add(output)
      }
    } else if (job.status === 'failed') {
      failedJobs++
    }
  }

  return {
    readyOutputs: outputs.size,
    runningJobs: 0, // Replay is always past data
    quarantinedRows: 0,
    failedJobs,
    recentOutputs: Array.from(outputs).map((name) => ({
      name,
      rows: totalRows.toString(),
      updated: context.metadata.endTime ?? new Date().toISOString(),
    })),
    activeRuns: [],
  }
}

// =============================================================================
// Replay Mode State
// =============================================================================

let globalReplayContext: ReplayContext | null = null

/**
 * Enable replay mode with the given tape content.
 */
export function enableReplayMode(tapeContent: string): ReplayContext {
  globalReplayContext = parseTape(tapeContent)
  console.log(
    `[Replay] Loaded tape with ${globalReplayContext.events.length} events, ${globalReplayContext.jobs.size} jobs`
  )
  return globalReplayContext
}

/**
 * Disable replay mode.
 */
export function disableReplayMode(): void {
  globalReplayContext = null
  console.log('[Replay] Replay mode disabled')
}

/**
 * Check if replay mode is enabled.
 */
export function isReplayMode(): boolean {
  return globalReplayContext !== null
}

/**
 * Get the current replay context (throws if not in replay mode).
 */
export function getReplayContext(): ReplayContext {
  if (!globalReplayContext) {
    throw new Error('Not in replay mode')
  }
  return globalReplayContext
}
