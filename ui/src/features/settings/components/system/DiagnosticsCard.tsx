// 系统诊断 — health summary + the full `get_system_diagnostics` report
// (system info, health metrics, bridge services, service status, gbrain init).
// Moved verbatim out of `components/settings/SystemTab.tsx` during the P1 split;
// the fetch lives in `useSystemDiagnostics`. Never calls IPC directly.
import * as React from 'react'
import { ChevronDown, ChevronUp, RefreshCw } from 'lucide-react'
import { cn } from '@/lib/utils'
import { useSystemDiagnostics } from '../../hooks/useSystemDiagnostics'
import { formatMemory, formatReason, formatUptime, serviceStatusDot, serviceStatusLabel } from '../../lib/format'
import { BridgeCard, GbrainInitRow, Grid4, InfoCell, Section } from './report-parts'

export function DiagnosticsCard({ onError }: { onError?: (m: string) => void }) {
  const { report, loading, lastChecked, runDiagnostics } = useSystemDiagnostics(onError)
  const [healthExpanded, setHealthExpanded] = React.useState(false)

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
    <div className="flex flex-col gap-4">
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

          {/* Footer */}
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
        </>
      )}

      {!report && !loading && (
        <p className="text-sm text-muted-foreground text-center py-8">
          点击「运行诊断」开始检查系统状态
        </p>
      )}
    </div>
  )
}
