/**
 * useScreencastCanvas — draws a CDP screencast JPEG frame onto a <canvas>.
 * Extracted from BrowserPreviewOverlay during the features/agent migration so
 * the overlay stays presentational.
 *
 * Behavior (unchanged from the inline version): decodes the base64 frame into a
 * Blob → ImageBitmap, resizes the canvas only when the frame dimensions change
 * (avoids per-frame layout thrash), draws it, and guards the async bitmap
 * decode with a `cancelled` flag so a superseded frame never paints. No
 * `@tauri-apps/api` here — frames arrive via the caller's atom subscription.
 *
 * Returns the canvas ref to attach to the <canvas> element.
 */

import * as React from 'react'
import type { ScreencastFrameEntry } from '@/atoms/browser-atoms'

export function useScreencastCanvas(
  frame: ScreencastFrameEntry | undefined,
): React.RefObject<HTMLCanvasElement> {
  const canvasRef = React.useRef<HTMLCanvasElement>(null)
  const lastDimsRef = React.useRef({ w: 0, h: 0 })

  React.useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas || !frame) return
    const ctx = canvas.getContext('2d')
    if (!ctx) return
    const binary = atob(frame.dataB64)
    const bytes = new Uint8Array(binary.length)
    for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i)
    const blob = new Blob([bytes], { type: frame.mimeType ?? 'image/jpeg' })
    let cancelled = false
    createImageBitmap(blob).then((bitmap) => {
      if (cancelled) { bitmap.close(); return }
      if (lastDimsRef.current.w !== bitmap.width || lastDimsRef.current.h !== bitmap.height) {
        canvas.width = bitmap.width
        canvas.height = bitmap.height
        lastDimsRef.current = { w: bitmap.width, h: bitmap.height }
      }
      ctx.drawImage(bitmap, 0, 0)
      bitmap.close()
    }).catch(() => {})
    return () => { cancelled = true }
  }, [frame])

  return canvasRef
}
