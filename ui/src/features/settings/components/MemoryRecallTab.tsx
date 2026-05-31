/**
 * MemoryRecallTab — 记忆召回设置页
 *
 * 组合 MemoryRecallSettings 表单和说明文字。
 */
import * as React from 'react'
import { MemoryRecallSettings } from './MemoryRecallSettings'

export function MemoryRecallTab(): React.ReactElement {
  return (
    <div className="space-y-8">
      <section data-settings-section="记忆召回配置">
        <MemoryRecallSettings />
      </section>
    </div>
  )
}
