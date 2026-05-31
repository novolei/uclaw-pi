/**
 * useAttachmentImage — loads a tool-result image attachment as a data: URL.
 *
 * Reads the attachment bytes through the agent bridge (was a direct
 * `@/lib/tauri-bridge` `readAttachment` call inside ToolResultImage) and returns
 * a `data:<mediaType>;base64,...` src (or null while loading / on error).
 * Extracted from ToolActivityItem.tsx during the features/agent migration split
 * so ActivityDetails stays presentational.
 */

import * as React from 'react'
import { readAttachment } from '@/lib/bridge/agent'

export function useAttachmentImage(localPath: string, mediaType: string): string | null {
  const [imageSrc, setImageSrc] = React.useState<string | null>(null)

  React.useEffect(() => {
    readAttachment(localPath)
      .then((base64: string) => {
        setImageSrc(`data:${mediaType};base64,${base64}`)
      })
      .catch((error: unknown) => {
        console.error('[ToolResultImage] 读取附件失败:', error)
      })
  }, [localPath, mediaType])

  return imageSrc
}
