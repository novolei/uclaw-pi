import * as React from 'react'
import { invoke } from '@tauri-apps/api/core'
import { Activity, ChevronDown, ChevronUp, PlayCircle, RefreshCw, RotateCcw, Power } from 'lucide-react'
import { cn } from '@/lib/utils'
import { EmbeddingEndpointSection } from './EmbeddingEndpointSection'
import { StreamSkillThresholdsSection } from './StreamSkillThresholdsSection'
import { FoldDeltaThresholdSection } from './FoldDeltaThresholdSection'
import { DeveloperOptionsSection } from './DeveloperOptionsSection'

// ── Types (mirror Rust structs) ──────────────────────────────────────

type ServiceStatus =
  | { status: 'Stopped' }
  | { status: 'Starting' }
  | { status: 'Running' }
  | { status: 'Stopping' }
  | { status: 'Failed'; reason: string }

interface ServiceHealth {
  name: string
  status: ServiceStatus
  uptime_secs: number | null
  last_error: string | null
  metrics: Record<string, unknown>
}

interface MemUBridgeStatus {
  running: boolean
  pid: number | null
  reason: string | null
  python_path: string | null
  script_path: string | null
  db_path: string | null
}

interface GbrainStatus {
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
type GbrainInitStatus =
  | { status: 'not_attempted' }
  | { status: 'in_progress' }
  | { status: 'succeeded'; duration_ms: number; at_ms: number }
  | { status: 'skipped_already_initialized'; at_ms: number }
  | { status: 'failed'; error: string; stderr_tail: string | null; at_ms: number }
  | { status: 'bundle_missing' }

interface SystemDiagnosticsReport {
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

interface EvalCheckResult {
  id: string
  passed: boolean
  score: number
  message: string
}

interface EvalScorecard {
  caseId: string
  title: string
  passed: boolean
  score: number
  checks: EvalCheckResult[]
}

interface EvalSuiteReport {
  passed: boolean
  averageScore: number
  runIds: string[]
  scorecards: EvalScorecard[]
}

interface SelfImprovementGateReport {
  candidateId: string
  verdict: 'promote' | 'hold' | 'reject'
  score: number
  checks: Array<{
    id: string
    passed: boolean
    message: string
  }>
}

type EvalKind = 'browser' | 'memory' | 'agent' | 'self'

const evalCommands: Record<EvalKind, string> = {
  browser: 'run_browser_parity_eval',
  memory: 'run_memory_gbrain_eval',
  agent: 'run_agent_control_plane_eval',
  self: 'run_self_improvement_gate_eval',
}

// ── Helpers ──────────────────────────────────────────────────────────

function formatUptime(secs: number): string {
  const h = Math.floor(secs / 3600)
  const m = Math.floor((secs % 3600) / 60)
  return `${h}h ${m}m`
}

function formatMemory(mb: number): string {
  return mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : `${mb} MB`
}

function serviceStatusLabel(s: ServiceStatus): string {
  const map: Record<string, string> = {
    Running: '运行中', Stopped: '未启动',
    Starting: '启动中', Stopping: '停止中',
  }
  if (s.status === 'Failed') return `失败: ${(s as { status: 'Failed'; reason: string }).reason.slice(0, 40)}`
  return map[s.status] ?? s.status
}

function serviceStatusDot(s: ServiceStatus): string {
  if (s.status === 'Running') return 'bg-green-500'
  if (s.status === 'Stopped' || s.status === 'Stopping') return 'bg-muted-foreground/40'
  if (s.status === 'Failed') return 'bg-red-500'
  return 'bg-yellow-400' // Starting
}

function formatReason(reason: string): string {
  const map: Record<string, string> = {
    client_not_initialized: '客户端未初始化',
    python_subprocess_not_alive: 'Python 进程未存活',
    health_check_returned_false: '健康检查失败',
    pglite_lock_timeout: 'PGLite 锁超时',
    mcp_connect_timeout: 'MCP 连接超时',
    process_killed: '进程被系统终止',
    page_not_found: '页面不存在',
    pglite_not_ready: 'PGLite 未就绪',
    permission_denied: '权限不足',
    path_mismatch: '路径不匹配',
    launcher_missing_or_unusable: '启动器缺失或不可用',
    not_registered: '未注册',
    disconnected: '已断开',
    connecting: '连接中',
    connected: '已连接',
    error: '错误',
    unknown: '未知错误',
  }
  return map[reason] ?? reason
}

// ── Main component ───────────────────────────────────────────────────

export function SystemTab() {
  const [report, setReport] = React.useState<SystemDiagnosticsReport | null>(null)
  const [loading, setLoading] = React.useState(false)
  const [lastChecked, setLastChecked] = React.useState<Date | null>(null)
  const [healthExpanded, setHealthExpanded] = React.useState(false)
  const [busyMemu, setBusyMemu] = React.useState(false)
  const [busyGbrain, setBusyGbrain] = React.useState(false)
  const [busyReset, setBusyReset] = React.useState(false)
  const [busyRestart, setBusyRestart] = React.useState(false)
  const [actionError, setActionError] = React.useState<string | null>(null)
  const [evalBusy, setEvalBusy] = React.useState<EvalKind | 'all' | null>(null)
  const [evalReports, setEvalReports] = React.useState<Record<EvalKind, EvalSuiteReport | null>>({
    browser: null,
    memory: null,
    agent: null,
    self: null,
  })

  const runDiagnostics = React.useCallback(async () => {
    setLoading(true)
    setActionError(null)
    try {
      const r = await invoke<SystemDiagnosticsReport>('get_system_diagnostics')
      setReport(r)
      setLastChecked(new Date())
    } catch (e) {
      setActionError(String(e))
    } finally {
      setLoading(false)
    }
  }, [])

  // Optional local HTTP API server toggle (memory / embeddings / external).
  // Persisted backend setting; null = still loading. Applies on next restart.
  const [httpApiEnabled, setHttpApiEnabled] = React.useState<boolean | null>(null)
  React.useEffect(() => {
    invoke<boolean>('get_http_api_enabled')
      .then(setHttpApiEnabled)
      .catch(() => setHttpApiEnabled(false))
  }, [])
  async function toggleHttpApi() {
    if (httpApiEnabled === null) return
    const next = !httpApiEnabled
    try {
      await invoke('set_http_api_enabled', { enabled: next })
      setHttpApiEnabled(next)
    } catch (e) {
      setActionError(String(e))
    }
  }

  const isHealthy = report
    ? report.consecutive_failures === 0
      && !report.services.some(s => s.status.status === 'Failed')
      && report.memu.running
      && report.gbrain.connected
      && report.gbrain.tool_count > 0
      && report.gbrain.pgdata_ready
      && !report.gbrain.error_kind
      && !report.gbrain.path_stale
    : true

  const failedServices = report?.services.filter(s => s.status.status === 'Failed') ?? []
  const hasGbrainIssue = report
    ? !report.gbrain.connected
      || report.gbrain.tool_count === 0
      || !report.gbrain.pgdata_ready
      || Boolean(report.gbrain.error_kind)
      || report.gbrain.path_stale
    : false
  const gbrainOperational = report
    ? report.gbrain.connected
      && report.gbrain.tool_count > 0
      && report.gbrain.pgdata_ready
      && !report.gbrain.error_kind
      && !report.gbrain.path_stale
    : false

  async function handleBridgeAction(
    command: string,
    setBusy: (v: boolean) => void,
  ) {
    setBusy(true)
    setActionError(null)
    try {
      await invoke(command)
      await runDiagnostics()
    } catch (e) {
      setActionError(String(e))
    } finally {
      setBusy(false)
    }
  }

  async function handleEvalRun(kind: EvalKind, command: string) {
    setEvalBusy(kind)
    setActionError(null)
    try {
      const result = await invoke<unknown>(command)
      setEvalReports(prev => ({ ...prev, [kind]: normalizeEvalReport(kind, result) }))
    } catch (e) {
      setActionError(String(e))
    } finally {
      setEvalBusy(null)
    }
  }

  async function handleRunAllEvals() {
    setEvalBusy('all')
    setActionError(null)
    try {
      for (const kind of Object.keys(evalCommands) as EvalKind[]) {
        const result = await invoke<unknown>(evalCommands[kind])
        setEvalReports(prev => ({ ...prev, [kind]: normalizeEvalReport(kind, result) }))
      }
    } catch (e) {
      setActionError(String(e))
    } finally {
      setEvalBusy(null)
    }
  }

  function handleCopyReport() {
    if (!report) return
    navigator.clipboard.writeText(JSON.stringify(report, null, 2))
  }

  function handleExportReport() {
    if (!report) return
    const blob = new Blob([JSON.stringify(report, null, 2)], { type: 'application/json' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `uclaw-diagnostics-${new Date().toISOString().slice(0, 19).replace(/:/g, '-')}.json`
    document.body.appendChild(a)
    a.click()
    document.body.removeChild(a)
    URL.revokeObjectURL(url)
  }

  return (
    <div className="flex flex-col gap-4 p-4 max-w-2xl">
      {/* Header */}
      <div className="flex items-start justify-between">
        <div>
          <h2 className="text-base font-semibold text-foreground">系统诊断</h2>
          <p className="text-xs text-muted-foreground mt-0.5">检查系统健康状态并修复问题</p>
        </div>
        <button
          onClick={runDiagnostics}
          disabled={loading}
          className="flex items-center gap-1.5 text-xs px-3 py-1.5 rounded-lg bg-accent text-accent-foreground hover:bg-accent/80 disabled:opacity-50 transition-colors"
        >
          <RefreshCw size={12} className={loading ? 'animate-spin' : ''} />
          运行诊断
        </button>
      </div>

      {actionError && (
        <div className="text-xs text-red-400 bg-red-400/10 rounded-lg px-3 py-2">
          {actionError}
        </div>
      )}

      {/* 本地 HTTP API 服务开关（可选，默认关） */}
      <div className="rounded-xl border border-border px-4 py-3">
        <div className="flex items-center justify-between gap-3">
          <div className="min-w-0">
            <div className="text-sm font-medium text-foreground">本地 HTTP API 服务</div>
            <p className="text-xs text-muted-foreground mt-0.5">
              可选的本地接口（记忆 / embedding / 外部访问，端口 19528）。聊天与 Agent 不依赖它，默认关闭。修改后需<span className="text-foreground/70">重启应用</span>生效。
            </p>
          </div>
          <button
            onClick={toggleHttpApi}
            disabled={httpApiEnabled === null}
            className={cn(
              'shrink-0 text-xs px-3 py-1.5 rounded-lg transition-colors disabled:opacity-50',
              httpApiEnabled
                ? 'bg-green-500/15 text-green-400 hover:bg-green-500/25'
                : 'bg-muted text-muted-foreground hover:bg-muted/70',
            )}
          >
            {httpApiEnabled === null ? '…' : httpApiEnabled ? '已开启' : '已关闭'}
          </button>
        </div>
      </div>

      {/* 系统健康 collapsible card */}
      {report && (
        <div
          className={cn(
            'rounded-xl border px-4 py-3 cursor-pointer select-none',
            isHealthy
              ? 'bg-green-500/10 border-green-500/20'
              : 'bg-red-500/10 border-red-500/20',
          )}
          onClick={() => setHealthExpanded(v => !v)}
        >
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <span className={cn('text-sm font-medium', isHealthy ? 'text-green-400' : 'text-red-400')}>
                {isHealthy ? '✓ 系统健康' : '✗ 发现问题'}
              </span>
              {lastChecked && (
                <span className="text-xs text-muted-foreground">
                  上次检查: {lastChecked.toLocaleString('zh-CN')}
                </span>
              )}
            </div>
            {healthExpanded ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
          </div>
          {healthExpanded && (failedServices.length > 0 || !report.memu.running || hasGbrainIssue) && (
            <ul className="mt-2 text-xs text-red-400 space-y-0.5">
              {failedServices.map(s => (
                <li key={s.name}>• {s.name}: {serviceStatusLabel(s.status)}</li>
              ))}
              {!report.memu.running && (
                <li>• memU: {report.memu.reason ? formatReason(report.memu.reason) : 'Python Bridge 未运行或健康检查失败'}</li>
              )}
              {!report.gbrain.connected && (
                <li>• gbrain: MCP 未连接{report.gbrain.suggested_action ? ` — ${report.gbrain.suggested_action}` : ''}</li>
              )}
              {report.gbrain.connected && report.gbrain.tool_count === 0 && <li>• gbrain: MCP 已连接但没有可用工具</li>}
              {report.gbrain.connected && !report.gbrain.pgdata_ready && <li>• gbrain: PGLite 未就绪</li>}
              {report.gbrain.path_stale && <li>• gbrain: MCP 配置路径与当前数据目录不一致</li>}
            </ul>
          )}
        </div>
      )}

      {report && (
        <>
          {/* 系统信息 */}
          <Section title="系统信息">
            <Grid4>
              <InfoCell label="版本" value={report.app_version} />
              <InfoCell label="平台" value={`${report.platform} (${report.arch})`} />
              <InfoCell label="内存" value={`${formatMemory(report.memory_used_mb)} / ${formatMemory(report.memory_total_mb)}`} />
              <InfoCell label="运行时间" value={formatUptime(report.uptime_secs)} />
            </Grid4>
          </Section>

          {/* 健康指标 */}
          <Section title="健康指标">
            <Grid4>
              <InfoCell label="连续失败次数" value={String(report.consecutive_failures)} />
              <InfoCell label="恢复尝试次数" value={String(report.recovery_attempts)} />
              <InfoCell label="活跃进程" value={String(report.active_processes)} />
              <InfoCell label="发现孤儿进程" value={String(report.orphan_processes)} />
            </Grid4>
          </Section>

          {/* 桥接服务 */}
          <Section title="桥接服务">
            <div className="flex flex-col gap-2">
              <BridgeCard
                name="memU"
                subtitle="Python Bridge"
                running={report.memu.running}
                detail={report.memu.running
                  ? (report.memu.pid ? `PID ${report.memu.pid}` : '运行中')
                  : `未运行${report.memu.reason ? `: ${formatReason(report.memu.reason)}` : ''}`}
                diagnostics={[
                  report.memu.python_path ? `Python: ${report.memu.python_path}` : null,
                  report.memu.script_path ? `Bridge: ${report.memu.script_path}` : null,
                  report.memu.db_path ? `DB: ${report.memu.db_path}` : null,
                ]}
              />
              <BridgeCard
                name="gbrain"
                subtitle="Bun MCP"
                running={gbrainOperational}
                detail={gbrainOperational
                  ? `${report.gbrain.tool_count} 工具 · PGlite pgdata ${report.gbrain.pgdata_ready ? '已就绪' : '未就绪'}`
                  : `不可用${report.gbrain.error_kind ? `: ${formatReason(report.gbrain.error_kind)}` : report.gbrain.connected ? '' : ': MCP 未连接'}`}
                diagnostics={[
                  `MCP: ${formatReason(report.gbrain.status)}`,
                  `Home: ${report.gbrain.home_path}`,
                  `Launcher: ${report.gbrain.launcher_path}`,
                  `PGlite: ${report.gbrain.pgdata_path}`,
                  report.gbrain.config_command ? `Config command: ${report.gbrain.config_command} (${report.gbrain.config_command_exists ? 'exists' : 'missing'})` : null,
                  report.gbrain.config_entry_path ? `Config entry: ${report.gbrain.config_entry_path} (${report.gbrain.config_entry_exists ? 'exists' : 'missing'})` : null,
                  report.gbrain.config_gbrain_home ? `Config GBRAIN_HOME: ${report.gbrain.config_gbrain_home}` : null,
                  report.gbrain.path_stale ? '路径状态: 配置已过期' : '路径状态: 当前',
                  report.gbrain.suggested_action ? `建议: ${report.gbrain.suggested_action}` : null,
                  report.gbrain.error ? `错误: ${report.gbrain.error.slice(0, 220)}` : null,
                ]}
              />
            </div>
            {/* Sprint 2.2.5b — init status row.
                Only render when init was attempted (not_attempted = boot
                pre-Stage-3, no useful signal). Failed shows actionable
                hint pointing at scripts/init-gbrain.sh. */}
            {report.gbrain_init.status !== 'not_attempted' && (
              <GbrainInitRow status={report.gbrain_init} />
            )}
          </Section>

          {/* 服务状态 */}
          <Section title="服务状态">
            <div className="flex flex-col divide-y divide-border/50">
              {report.services.map(svc => (
                <div key={svc.name} className="flex items-center justify-between py-2">
                  <div className="flex items-center gap-2">
                    <span className={cn('size-2 rounded-full flex-shrink-0', serviceStatusDot(svc.status))} />
                    <span className="text-sm text-foreground">{svc.name}</span>
                  </div>
                  <span className="text-xs text-muted-foreground">{serviceStatusLabel(svc.status)}</span>
                </div>
              ))}
            </div>
          </Section>

          {/* 评估套件 */}
          <Section title="评估套件">
            <div className="rounded-lg border border-border/50 bg-muted/20">
              <div className="flex items-center justify-between gap-3 border-b border-border/50 px-3 py-2">
                <div className="flex min-w-0 items-center gap-2">
                  <Activity size={14} className="text-muted-foreground" />
                  <div className="min-w-0">
                    <div className="text-sm font-medium text-foreground">自治回归套件</div>
                    <div className="text-[11px] text-muted-foreground">
                      运行 Browser、Memory、Agent 与自我改进 gates
                    </div>
                  </div>
                </div>
                <div className="flex shrink-0 flex-wrap justify-end gap-2">
                  <EvalButton
                    label="All"
                    busy={evalBusy === 'all'}
                    onClick={handleRunAllEvals}
                    disabled={Boolean(evalBusy)}
                  />
                  <EvalButton
                    label="Browser"
                    busy={evalBusy === 'browser'}
                    onClick={() => handleEvalRun('browser', evalCommands.browser)}
                    disabled={Boolean(evalBusy)}
                  />
                  <EvalButton
                    label="Memory"
                    busy={evalBusy === 'memory'}
                    onClick={() => handleEvalRun('memory', evalCommands.memory)}
                    disabled={Boolean(evalBusy)}
                  />
                  <EvalButton
                    label="Agent"
                    busy={evalBusy === 'agent'}
                    onClick={() => handleEvalRun('agent', evalCommands.agent)}
                    disabled={Boolean(evalBusy)}
                  />
                  <EvalButton
                    label="Self"
                    busy={evalBusy === 'self'}
                    onClick={() => handleEvalRun('self', evalCommands.self)}
                    disabled={Boolean(evalBusy)}
                  />
                </div>
              </div>
              <div className="space-y-2 p-3">
                <EvalSummary name="browser parity" report={evalReports.browser} />
                <EvalSummary name="memory/gbrain" report={evalReports.memory} />
                <EvalSummary name="agent control-plane" report={evalReports.agent} />
                <EvalSummary name="self-improvement gates" report={evalReports.self} />
                {!evalReports.browser && !evalReports.memory && !evalReports.agent && !evalReports.self && (
                  <div className="text-xs text-muted-foreground">
                    尚未运行。结果会显示通过率、平均分和失败 case 的具体检查项。
                  </div>
                )}
              </div>
            </div>
          </Section>

          {/* 恢复操作 */}
          <Section title="恢复操作">
            <div className="flex flex-col gap-2">
              <div className="flex gap-2">
                <ActionButton
                  icon={<RotateCcw size={13} />}
                  label="重置 AI 引擎"
                  busy={busyReset}
                  variant="warm"
                  onClick={() => handleBridgeAction('reset_ai_engine', setBusyReset)}
                />
                <ActionButton
                  icon={<Power size={13} />}
                  label="重启应用"
                  busy={busyRestart}
                  variant="danger"
                  onClick={() => handleBridgeAction('restart_app', setBusyRestart)}
                />
              </div>
              <div className="flex gap-2">
                <ActionButton
                  icon={<RotateCcw size={13} />}
                  label="重启 memU"
                  busy={busyMemu}
                  variant="bridge"
                  onClick={() => handleBridgeAction('restart_memu_bridge', setBusyMemu)}
                />
                <ActionButton
                  icon={<RotateCcw size={13} />}
                  label="重启 gbrain"
                  busy={busyGbrain}
                  variant="bridge"
                  onClick={() => handleBridgeAction('restart_gbrain_mcp', setBusyGbrain)}
                />
              </div>
            </div>
          </Section>
        </>
      )}

      {/* Sprint 2.2 followon #4 — embedding endpoint configuration */}
      <EmbeddingEndpointSection />

      {/* Bundle 26-B / 26-D / 27-B — stream idle timeout + skill
          distillation thresholds, originally hardcoded, now
          user-tunable. */}
      <StreamSkillThresholdsSection />

      {/* Bundle 17-B — /compact fold delta threshold. Loose default 50
          favors the delta path until Bundle 17-C telemetry tunes it. */}
      <FoldDeltaThresholdSection />

      {/* Sprint 2.2 followon #4 — developer options (collapsed by default) */}
      <DeveloperOptionsSection />

      {/* Footer */}
      {report && (
        <div className="flex gap-4 pt-1 border-t border-border/50">
          <button
            onClick={handleCopyReport}
            className="text-xs text-muted-foreground hover:text-foreground transition-colors"
          >
            复制报告
          </button>
          <button
            onClick={handleExportReport}
            className="text-xs text-muted-foreground hover:text-foreground transition-colors"
          >
            导出报告
          </button>
        </div>
      )}

      {!report && !loading && (
        <p className="text-sm text-muted-foreground text-center py-8">
          点击「运行诊断」开始检查系统状态
        </p>
      )}
    </div>
  )
}

function EvalButton({ label, busy, disabled, onClick }: {
  label: string; busy: boolean; disabled?: boolean; onClick: () => void
}) {
  return (
    <button
      onClick={onClick}
      disabled={busy || disabled}
      className="flex min-h-8 cursor-pointer items-center gap-1.5 rounded-md border border-border/60 bg-background px-2.5 text-xs text-foreground transition-colors hover:bg-accent disabled:cursor-default disabled:opacity-50"
    >
      {busy ? <RefreshCw size={12} className="animate-spin" /> : <PlayCircle size={12} />}
      {label}
    </button>
  )
}

function EvalSummary({ name, report }: { name: string; report: EvalSuiteReport | null }) {
  if (!report) return null
  const failed = report.scorecards.filter(card => !card.passed)
  return (
    <div className="rounded-md bg-background/70 px-3 py-2">
      <div className="flex items-center justify-between gap-3">
        <div className="min-w-0">
          <div className="truncate text-sm font-medium text-foreground">{name}</div>
          <div className="text-[11px] text-muted-foreground">
            {report.scorecards.length} cases · {report.runIds.length} runs
          </div>
        </div>
        <div className="shrink-0 text-right">
          <div className={cn('text-xs font-medium', report.passed ? 'text-green-400' : 'text-red-400')}>
            {report.passed ? '通过' : '失败'}
          </div>
          <div className="font-mono text-[11px] text-muted-foreground">
            {(report.averageScore * 100).toFixed(0)}%
          </div>
        </div>
      </div>
      <div className="mt-2 overflow-hidden rounded border border-border/40">
        {report.scorecards.map(card => (
          <div
            key={card.caseId}
            className="grid grid-cols-[1fr_auto] gap-2 border-b border-border/40 px-2 py-1.5 last:border-b-0"
          >
            <div className="min-w-0">
              <div className="truncate text-xs text-foreground">{card.title}</div>
              {!card.passed && (
                <div className="mt-0.5 text-[11px] text-red-400">
                  {card.checks.filter(check => !check.passed).map(check => check.id).join(', ')}
                </div>
              )}
            </div>
            <div className={cn('font-mono text-[11px]', card.passed ? 'text-green-400' : 'text-red-400')}>
              {(card.score * 100).toFixed(0)}%
            </div>
          </div>
        ))}
      </div>
      {failed.length > 0 && (
        <div className="mt-2 text-[11px] leading-4 text-muted-foreground">
          首个失败：{failed[0].checks.find(check => !check.passed)?.message ?? failed[0].title}
        </div>
      )}
    </div>
  )
}

function normalizeEvalReport(kind: EvalKind, result: unknown): EvalSuiteReport {
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

// ── Sub-components ───────────────────────────────────────────────────

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-2">
      <p className="text-[10px] uppercase tracking-wider text-muted-foreground font-medium">{title}</p>
      {children}
    </div>
  )
}

function Grid4({ children }: { children: React.ReactNode }) {
  return <div className="grid grid-cols-2 gap-x-8 gap-y-2">{children}</div>
}

function InfoCell({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between py-1.5 border-b border-border/40">
      <span className="text-xs text-muted-foreground">{label}</span>
      <span className="text-xs text-foreground font-mono">{value}</span>
    </div>
  )
}

function BridgeCard({ name, subtitle, running, detail, diagnostics = [] }: {
  name: string; subtitle: string; running: boolean; detail: string; diagnostics?: Array<string | null>
}) {
  const visibleDiagnostics = diagnostics.filter(Boolean) as string[]
  return (
    <div className="rounded-lg bg-muted/40 px-3 py-2.5">
      <div className="flex items-center justify-between gap-4">
        <div className="flex items-center gap-2">
          <span className={cn('size-2 rounded-full flex-shrink-0', running ? 'bg-green-500' : 'bg-muted-foreground/40')} />
          <span className="text-sm font-medium text-foreground">{name}</span>
          <span className="text-xs text-muted-foreground">({subtitle})</span>
        </div>
        <span className={cn('text-xs text-right', running ? 'text-green-400' : 'text-muted-foreground')}>{detail}</span>
      </div>
      {visibleDiagnostics.length > 0 && (
        <div className="mt-2 space-y-0.5 border-t border-border/40 pt-2">
          {visibleDiagnostics.map((line, idx) => (
            <div key={idx} className="break-all text-[11px] leading-4 text-muted-foreground">
              {line}
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

// Sprint 2.2.5b — surface the gbrain init outcome with actionable copy.
// Each status branch picks an appropriate dot color + 1-line message +
// optional remediation hint.
function GbrainInitRow({ status }: { status:
  | { status: 'not_attempted' }
  | { status: 'in_progress' }
  | { status: 'succeeded'; duration_ms: number; at_ms: number }
  | { status: 'skipped_already_initialized'; at_ms: number }
  | { status: 'failed'; error: string; stderr_tail: string | null; at_ms: number }
  | { status: 'bundle_missing' }
}) {
  let dotClass = 'bg-muted-foreground/40'
  let label = '初始化未尝试'
  let detail = ''
  let hint: string | null = null

  switch (status.status) {
    case 'in_progress':
      dotClass = 'bg-yellow-400 animate-pulse'
      label = '初始化进行中'
      detail = '首次启动 — PGlite 正在跑 ~63 次迁移 (30-60s)'
      break
    case 'succeeded':
      dotClass = 'bg-green-500'
      label = '初始化成功'
      detail = `首次初始化耗时 ${(status.duration_ms / 1000).toFixed(1)}s`
      break
    case 'skipped_already_initialized':
      dotClass = 'bg-green-500'
      label = '已初始化'
      detail = 'PGlite 数据库已就绪'
      break
    case 'failed':
      dotClass = 'bg-red-500'
      label = '初始化失败'
      detail = status.error
      hint = '运行 scripts/init-gbrain.sh 或删除 ~/.uclaw/gbrain/ 后重启'
      break
    case 'bundle_missing':
      dotClass = 'bg-red-500'
      label = 'bundle 缺失'
      detail = 'bunembed/bun 或 gbrain-source 未找到'
      hint = '运行 scripts/setup-bun-runtime.sh + scripts/setup-gbrain-source.sh'
      break
    case 'not_attempted':
      // Caller filters this out, but TS demands exhaustive match.
      break
  }

  return (
    <div className="mt-2 rounded-lg bg-muted/30 px-3 py-2 text-xs">
      <div className="flex items-center gap-2">
        <span className={cn('size-2 rounded-full flex-shrink-0', dotClass)} />
        <span className="font-medium text-foreground">gbrain init</span>
        <span className="text-muted-foreground">— {label}</span>
      </div>
      {detail && (
        <div className="mt-1 pl-4 text-muted-foreground">{detail}</div>
      )}
      {hint && (
        <div className="mt-1 pl-4 text-amber-400">{hint}</div>
      )}
    </div>
  )
}

function ActionButton({ icon, label, busy, variant, onClick }: {
  icon: React.ReactNode; label: string; busy: boolean
  variant: 'warm' | 'danger' | 'bridge'; onClick: () => void
}) {
  const cls = {
    warm: 'bg-amber-500/10 text-amber-400 hover:bg-amber-500/20 border border-amber-500/20',
    danger: 'bg-red-500/10 text-red-400 hover:bg-red-500/20 border border-red-500/20',
    bridge: 'bg-green-500/10 text-green-400 hover:bg-green-500/20 border border-green-500/20',
  }[variant]

  return (
    <button
      onClick={onClick}
      disabled={busy}
      className={cn(
        'flex items-center gap-1.5 text-xs px-3 py-1.5 rounded-lg transition-colors disabled:opacity-50',
        cls,
      )}
    >
      {busy ? <RefreshCw size={12} className="animate-spin" /> : icon}
      {label}
    </button>
  )
}
