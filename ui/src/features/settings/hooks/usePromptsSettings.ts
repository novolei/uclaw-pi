// Owns all PromptsSettings side effects — load/save of the workspace uclaw.md,
// the default-prompts fetch, the safety-mode-derived guardrail preview, the
// "jump to 通用 tab" nav, and the external-editor open. Extracted out of the
// component during the P2 move; all IPC goes through `settingsBridge`
// (no `@tauri-apps/api` here). Behavior (toasts, error logging) preserved exactly.
import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { toast } from 'sonner'
import type { DefaultPromptsResponse } from '@/lib/types'
import { safetyModeAtom } from '@/atoms/safety-atoms'
import { settingsTabAtom } from '@/atoms/settings-tab'
import { settingsBridge } from '../../../lib/bridge/settings'

export function usePromptsSettings() {
  const [content, setContent] = React.useState('')
  const [pristine, setPristine] = React.useState('')
  const [defaults, setDefaults] = React.useState<DefaultPromptsResponse | null>(null)
  const [loading, setLoading] = React.useState(true)
  const [saving, setSaving] = React.useState(false)
  const [showGuardrails, setShowGuardrails] = React.useState(false)
  const mode = useAtomValue(safetyModeAtom)
  const setSettingsTab = useSetAtom(settingsTabAtom)

  React.useEffect(() => {
    Promise.all([settingsBridge.readWorkspaceUclawMd(), settingsBridge.readDefaultPrompts()])
      .then(([md, p]) => {
        setContent(md)
        setPristine(md)
        setDefaults(p)
      })
      .catch((e) => {
        console.error('[PromptsSettings] load failed:', e)
        toast.error('加载提示词失败')
      })
      .finally(() => setLoading(false))
  }, [])

  const dirty = content !== pristine

  const onSave = React.useCallback(async () => {
    setSaving(true)
    try {
      await settingsBridge.writeWorkspaceUclawMd(content)
      setPristine(content)
      toast.success('uclaw.md 已保存')
    } catch (e) {
      console.error('[PromptsSettings] save failed:', e)
      toast.error('保存失败')
    } finally {
      setSaving(false)
    }
  }, [content])

  const openExternally = React.useCallback(async () => {
    try {
      await settingsBridge.openWorkspaceUclawMdExternally()
    } catch (e) {
      console.error('[PromptsSettings] open external failed:', e)
      toast.error('打开外部编辑器失败')
    }
  }, [])

  const currentModeAddition = React.useMemo(() => {
    if (!defaults) return ''
    switch (mode) {
      case 'ask': return defaults.modeAsk
      case 'acceptedits': return defaults.modeAcceptEdits
      case 'plan': return defaults.modePlan
      case 'yolo': return defaults.modeBypass
      default: return '(Auto mode — no mode-specific addition)'
    }
  }, [mode, defaults])

  return {
    content, setContent,
    defaults,
    loading, saving,
    showGuardrails, setShowGuardrails,
    mode,
    dirty,
    onSave,
    openExternally,
    currentModeAddition,
    goToGeneralTab: () => setSettingsTab('general'),
  }
}
