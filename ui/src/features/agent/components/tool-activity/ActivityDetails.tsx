/**
 * ActivityDetails — the expandable result panel for one tool activity (result
 * via ToolResultRenderer + any generated images, with a copy button). Plus the
 * ToolResultImage thumbnail (decode via useAttachmentImage hook, save-as via the
 * agent bridge). Extracted from ToolActivityItem.tsx during the features/agent
 * migration split.
 */

import * as React from 'react'
import { Download } from 'lucide-react'
import { getToolPhrase, ToolResultRenderer } from '@/shared/tool-rendering'
import { type ToolActivity } from '@/atoms/agent-atoms'
import { saveImageAs } from '@/lib/bridge/agent'
import { useAttachmentImage } from '../../hooks/useAttachmentImage'

// ===== 工具结果图片 =====

function ToolResultImage({ attachment }: { attachment: { localPath: string; filename: string; mediaType: string } }): React.ReactElement {
  const imageSrc = useAttachmentImage(attachment.localPath, attachment.mediaType)

  const handleSave = React.useCallback((): void => {
    saveImageAs(attachment.localPath, attachment.filename)
  }, [attachment.localPath, attachment.filename])

  if (!imageSrc) {
    return <div className="size-[120px] rounded-md bg-muted/30 animate-pulse" />
  }

  return (
    <div className="relative group inline-block">
      <img
        src={imageSrc}
        alt={attachment.filename}
        className="max-w-[240px] max-h-[240px] rounded-md object-cover"
      />
      <button
        type="button"
        onClick={handleSave}
        className="absolute bottom-1.5 right-1.5 p-1 rounded-md bg-black/50 text-white opacity-0 group-hover:opacity-100 transition-opacity hover:bg-black/70"
        title="保存图片"
      >
        <Download className="size-3.5" />
      </button>
    </div>
  )
}

// ===== 详情面板（仅显示结果，使用 ToolResultRenderer） =====

export function ActivityDetails({ activity, onClose }: { activity: ToolActivity; onClose: () => void }): React.ReactElement {
  const [copied, setCopied] = React.useState(false)

  const handleCopy = (): void => {
    const parts: string[] = [`[${activity.toolName}]`]
    if (activity.result) {
      parts.push(activity.result)
    }
    navigator.clipboard.writeText(parts.join('\n\n')).then(() => {
      setCopied(true)
      setTimeout(() => setCopied(false), 1500)
    })
  }

  return (
    <div className="rounded-lg border border-border/40 bg-muted/20 overflow-hidden animate-in fade-in slide-in-from-top-1 duration-200 ease-out">
      <div className="flex items-center justify-between px-3 py-1.5 border-b border-border/25">
        <span className="text-[10.5px] font-medium text-foreground/45 uppercase tracking-wide">{getToolPhrase(activity.toolName, activity.input).label}</span>
        <button
          type="button"
          onClick={handleCopy}
          className="text-[10.5px] text-muted-foreground/40 hover:text-foreground/70 transition-colors"
        >
          {copied ? '已复制' : '复制'}
        </button>
      </div>

      <div className="px-3 py-2 space-y-2 max-h-[320px] overflow-y-auto">
        {activity.result && (
          <ToolResultRenderer
            toolName={activity.toolName}
            input={activity.input}
            result={activity.result}
            isError={activity.isError ?? false}
          />
        )}
        {activity.imageAttachments && activity.imageAttachments.length > 0 && (
          <div>
            <div className="text-[10px] font-medium text-foreground/40 mb-1">生成图片</div>
            <div className="flex flex-wrap gap-2">
              {activity.imageAttachments.map((img, i) => (
                <ToolResultImage key={i} attachment={img} />
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  )
}
