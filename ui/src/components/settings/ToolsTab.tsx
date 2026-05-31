/**
 * ToolsTab — composes ToolSettings + PermissionsSettings.
 *
 * 学得技能与 MCP 已迁至万花筒（技能 / 集成模块）。本 tab 只剩工作区
 * skill 标签、活动技能调试面板、工具权限。
 */
import * as React from 'react'
import { ToolSettings } from '@/features/settings'
import { PermissionsSettings } from './PermissionsSettings'

export function ToolsTab(): React.ReactElement {
  return (
    <div className="space-y-8">
      <section data-settings-section="工具与 MCP">
        <ToolSettings />
      </section>
      <section data-settings-section="工具权限">
        <PermissionsSettings />
      </section>
    </div>
  )
}
