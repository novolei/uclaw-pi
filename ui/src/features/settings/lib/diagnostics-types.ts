// System-diagnostics + eval type contracts — mirror the Rust structs the
// settings IPC returns. Moved verbatim out of the legacy
// `components/settings/SystemTab.tsx` during the P1 split so the cards + hooks
// share one definition (docs/superpowers/plans/2026-05-31-frontend-settings-feature-migration.md).

export type ServiceStatus =
  | { status: 'Stopped' }
  | { status: 'Starting' }
  | { status: 'Running' }
  | { status: 'Stopping' }
  | { status: 'Failed'; reason: string }

export interface ServiceHealth {
  name: string
  status: ServiceStatus
  uptime_secs: number | null
  last_error: string | null
  metrics: Record<string, unknown>
}

export interface MemUBridgeStatus {
  running: boolean
  pid: number | null
  reason: string | null
  python_path: string | null
  script_path: string | null
  db_path: string | null
}

export interface GbrainStatus {
  connected: boolean
  tool_count: number
  pgdata_ready: boolean
  error: string | null
  status: string
  error_kind: string | null
  suggested_action: string | null
  home_path: string
  launcher_path: string
  pgdata_path: string
  config_command: string | null
  config_entry_path: string | null
  config_command_exists: boolean
  config_entry_exists: boolean
  config_gbrain_home: string | null
  path_stale: boolean
}

// Sprint 2.2.5b — mirror of Rust's `mcp::GbrainInitStatus`. Discriminated
// union via serde's `tag = "status"`. The frontend pattern-matches on
// the `status` field to pick the right label + remediation hint.
export type GbrainInitStatus =
  | { status: 'not_attempted' }
  | { status: 'in_progress' }
  | { status: 'succeeded'; duration_ms: number; at_ms: number }
  | { status: 'skipped_already_initialized'; at_ms: number }
  | { status: 'failed'; error: string; stderr_tail: string | null; at_ms: number }
  | { status: 'bundle_missing' }

export interface SystemDiagnosticsReport {
  app_version: string
  platform: string
  arch: string
  memory_used_mb: number
  memory_total_mb: number
  uptime_secs: number
  consecutive_failures: number
  recovery_attempts: number
  active_processes: number
  orphan_processes: number
  services: ServiceHealth[]
  memu: MemUBridgeStatus
  gbrain: GbrainStatus
  gbrain_init: GbrainInitStatus
}

export interface EvalCheckResult {
  id: string
  passed: boolean
  score: number
  message: string
}

export interface EvalScorecard {
  caseId: string
  title: string
  passed: boolean
  score: number
  checks: EvalCheckResult[]
}

export interface EvalSuiteReport {
  passed: boolean
  averageScore: number
  runIds: string[]
  scorecards: EvalScorecard[]
}

export interface SelfImprovementGateReport {
  candidateId: string
  verdict: 'promote' | 'hold' | 'reject'
  score: number
  checks: Array<{
    id: string
    passed: boolean
    message: string
  }>
}

export type EvalKind = 'browser' | 'memory' | 'agent' | 'self'

export const evalCommands: Record<EvalKind, string> = {
  browser: 'run_browser_parity_eval',
  memory: 'run_memory_gbrain_eval',
  agent: 'run_agent_control_plane_eval',
  self: 'run_self_improvement_gate_eval',
}

// Normalize raw eval IPC payloads into the unified `EvalSuiteReport` shape.
// Only the self-improvement gate returns a different array shape; the other
// three already match `EvalSuiteReport`.
export function normalizeEvalReport(kind: EvalKind, result: unknown): EvalSuiteReport {
  if (kind !== 'self') return result as EvalSuiteReport
  const reports = result as SelfImprovementGateReport[]
  const scorecards: EvalScorecard[] = reports.map(report => ({
    caseId: report.candidateId,
    title: `${report.candidateId} · ${report.verdict}`,
    passed: report.verdict !== 'hold',
    score: report.verdict === 'hold' ? 0.5 : 1,
    checks: report.checks.map(check => ({
      id: check.id,
      passed: check.passed,
      score: check.passed ? 1 : 0,
      message: check.message,
    })),
  }))
  return {
    passed: scorecards.every(card => card.passed),
    averageScore: scorecards.length
      ? scorecards.reduce((sum, card) => sum + card.score, 0) / scorecards.length
      : 0,
    runIds: [],
    scorecards,
  }
}
