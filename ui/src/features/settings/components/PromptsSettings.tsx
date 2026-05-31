/**
 * PromptsSettings — Settings → 提示词 tab.
 *
 * Three sections:
 *   1. Global system prompt (link to existing 通用 tab — don't duplicate)
 *   2. uclaw.md (workspace-level, editable textarea + 保存 + 外部编辑器)
 *   3. uClaw 内置行为护栏 (read-only collapsible: Karpathy baseline +
 *      current mode addition for transparency)
 */

import * as React from 'react'
import { Save, ExternalLink, FileCode2, ChevronDown, ChevronRight } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import { usePromptsSettings } from '../hooks/usePromptsSettings'

const PLACEHOLDER_TEMPLATE = `# uClaw — <project name>

<!-- 这个文件描述当前项目的上下文。uClaw agent 在每次对话时都会
     读取它，作为 "项目说明" 注入到系统提示词。
     文件位置：<workspace>/uclaw.md
     编辑后保存即生效。 -->

## 项目约定

-

## Do

-

## Don't

-

## 常用命令 / 路径

-
`

export function PromptsSettings(): React.ReactElement {
  const {
    content, setContent,
    defaults,
    loading, saving,
    showGuardrails, setShowGuardrails,
    mode,
    dirty,
    onSave,
    openExternally,
    currentModeAddition,
    goToGeneralTab,
  } = usePromptsSettings()

  return (
    <div className="space-y-6 pb-8">
      {/* Section 1: link to existing global system prompt tab */}
      <section>
        <h3 className="mb-2 text-[12px] font-semibold uppercase tracking-widest text-muted-foreground/70">
          全局系统提示词
        </h3>
        <Button variant="outline" size="sm" onClick={goToGeneralTab}>
          跳到 通用 tab 编辑
        </Button>
      </section>

      {/* Section 2: uclaw.md textarea */}
      <section>
        <div className="mb-2 flex items-center justify-between">
          <h3 className="text-[12px] font-semibold uppercase tracking-widest text-muted-foreground/70">
            项目说明 (uclaw.md)
          </h3>
          <div className="flex items-center gap-2">
            <Button
              variant="ghost" size="sm"
              onClick={() => void openExternally()}
            >
              <ExternalLink className="size-3.5 mr-1" />
              在外部编辑器打开
            </Button>
            <Button
              size="sm"
              onClick={() => void onSave()}
              disabled={!dirty || saving}
            >
              <Save className="size-3.5 mr-1" />
              {saving ? '保存中…' : '保存'}
            </Button>
          </div>
        </div>
        <textarea
          value={loading ? '加载中…' : content}
          placeholder={loading ? '' : PLACEHOLDER_TEMPLATE}
          onChange={(e) => setContent(e.target.value)}
          disabled={loading}
          spellCheck={false}
          className={cn(
            'w-full min-h-[280px] font-mono text-[12.5px] p-3',
            'bg-background border border-border/50 rounded',
            'focus:outline-none focus:border-border',
          )}
        />
        <p className="mt-1 text-[11px] text-muted-foreground/60">
          路径：<code className="font-mono">&lt;workspace&gt;/uclaw.md</code>
          {dirty && <span className="ml-2 text-amber-600">• 未保存</span>}
        </p>
      </section>

      {/* Section 3: read-only guardrails preview */}
      <section>
        <button
          type="button"
          onClick={() => setShowGuardrails((v) => !v)}
          className="flex items-center gap-1.5 text-[12px] font-semibold uppercase tracking-widest text-muted-foreground/70 hover:text-foreground"
        >
          {showGuardrails ? <ChevronDown className="size-3.5" /> : <ChevronRight className="size-3.5" />}
          uClaw 内置行为护栏 (只读)
        </button>
        {showGuardrails && defaults && (
          <div className="mt-2 space-y-3">
            <div>
              <h4 className="mb-1 text-[11px] font-medium text-muted-foreground/80 flex items-center gap-1">
                <FileCode2 className="size-3" /> baseline.md (Karpathy guardrails)
              </h4>
              <pre className="text-[11.5px] font-mono p-2 bg-muted/30 border border-border/50 rounded whitespace-pre-wrap">
                {defaults.baseline}
              </pre>
            </div>
            <div>
              <h4 className="mb-1 text-[11px] font-medium text-muted-foreground/80 flex items-center gap-1">
                <FileCode2 className="size-3" /> 当前模式 ({mode}) 的特化提示词
              </h4>
              <pre className="text-[11.5px] font-mono p-2 bg-muted/30 border border-border/50 rounded whitespace-pre-wrap">
                {currentModeAddition || '(empty)'}
              </pre>
            </div>
          </div>
        )}
      </section>
    </div>
  )
}
