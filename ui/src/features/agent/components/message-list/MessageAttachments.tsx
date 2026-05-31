/**
 * MessageAttachments — attachment/image presentational pieces extracted from
 * AgentMessages.tsx during the features/agent migration split:
 *  - InlineImage: a single tool-result image (click → lightbox, hover → save).
 *  - ToolResultInlineImages: flattens every tool activity's generated images.
 *  - AttachedFileChip: the file-reference chip under a user message.
 *
 * Image IO routes through the agent bridge (readAttachment via the shared
 * useAttachmentImage hook; saveImageAs directly) — no raw tauri-apps import here.
 * Behavior is unchanged from the originals.
 */

import * as React from 'react'
import { FileText, FileImage, Download } from 'lucide-react'
import { ImageLightbox } from '@/components/ui/image-lightbox'
import { saveImageAs } from '@/lib/bridge/agent'
import type { ToolActivity } from '@/atoms/agent-atoms'
import { useAttachmentImage } from '../../hooks/useAttachmentImage'
import { isImageFile, type AttachedFileRef } from '../../lib/agent-message-helpers'

/** 单张工具结果图片（内联显示），点击可预览大图 */
export function InlineImage({ attachment }: { attachment: { localPath: string; filename: string; mediaType: string } }): React.ReactElement {
  const imageSrc = useAttachmentImage(attachment.localPath, attachment.mediaType)
  const [lightboxOpen, setLightboxOpen] = React.useState(false)

  const handleSave = React.useCallback((): void => {
    saveImageAs(attachment.localPath, attachment.filename)
  }, [attachment.localPath, attachment.filename])

  if (!imageSrc) {
    return <div className="size-[280px] rounded-lg bg-muted/30 animate-pulse shrink-0" />
  }

  return (
    <div className="relative group inline-block">
      <img
        src={imageSrc}
        alt={attachment.filename}
        className="size-[280px] rounded-lg object-cover shrink-0 cursor-pointer"
        onClick={() => setLightboxOpen(true)}
      />
      <button
        type="button"
        onClick={handleSave}
        className="absolute bottom-2 right-2 p-1.5 rounded-md bg-black/50 text-white opacity-0 group-hover:opacity-100 transition-opacity hover:bg-black/70"
        title="保存图片"
      >
        <Download className="size-4" />
      </button>
      <ImageLightbox
        src={imageSrc}
        alt={attachment.filename}
        open={lightboxOpen}
        onOpenChange={setLightboxOpen}
      />
    </div>
  )
}

/** 从工具活动中提取并内联显示所有生成的图片 */
export function ToolResultInlineImages({ activities }: { activities: ToolActivity[] }): React.ReactElement | null {
  const allImages = activities.flatMap((a) => a.imageAttachments ?? [])
  if (allImages.length === 0) return null

  return (
    <div className="flex flex-wrap gap-2 mb-3">
      {allImages.map((img, i) => (
        <InlineImage key={`${img.localPath}-${i}`} attachment={img} />
      ))}
    </div>
  )
}

/** 附件引用芯片 */
export function AttachedFileChip({ file }: { file: AttachedFileRef }): React.ReactElement {
  const isImg = isImageFile(file.filename)
  const Icon = isImg ? FileImage : FileText

  return (
    <div className="inline-flex items-center gap-1.5 rounded-md bg-muted/60 px-2.5 py-1 text-[12px] text-muted-foreground">
      <Icon className="size-3.5 shrink-0" />
      <span className="truncate max-w-[200px]">{file.filename}</span>
    </div>
  )
}
