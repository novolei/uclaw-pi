/**
 * Desk-pet window root (S4) — the LIGHT root rendered for `?view=deskpet`.
 *
 * This is NOT the full app shell: no agent loop, no AppShell, no global
 * listeners. It renders the existing `PetWidget` (sprite + state machine) plus a
 * `ChatBubble` streamed from local MiniCPM.
 *
 * Click-through hit-testing
 * --------------------------
 * The OS window is created click-through (mouse-transparent) so it never steals
 * clicks from apps behind it. We flip click-through OFF while the cursor is over
 * the interactive pet/bubble (pointer-enter) and back ON when it leaves
 * (pointer-leave) — JS-side pointer hit-testing, since the window itself can't
 * do per-pixel transparency.
 *
 * Dragging
 * --------
 * Pointer-down on the sprite starts an OS window drag via Tauri v2's
 * `getCurrentWindow().startDragging()`. Both bridge calls are guarded so the
 * component still renders in a plain browser (dev / vitest) without a Tauri
 * runtime.
 */
import { useCallback, useEffect } from 'react'
import { PetWidget } from '@/features/agent/components/PetWidget'
import { petEnabledAtom } from '@/atoms/pet-atoms'
import { useSetAtom } from 'jotai'
import { setDeskPetClickThrough } from '@/lib/bridge/pet'
import { ChatBubble } from './ChatBubble'
import './DeskPetApp.css'

async function startWindowDrag() {
  try {
    const { getCurrentWindow } = await import('@tauri-apps/api/window')
    await getCurrentWindow().startDragging()
  } catch (e) {
    console.warn('[DeskPetApp] startDragging unavailable:', e)
  }
}

export function DeskPetApp() {
  // The desk-pet window always shows the pet regardless of the in-app
  // `pet.enabled` preference (that toggle governs the in-composer pet). Force it
  // enabled for this window only.
  const setEnabled = useSetAtom(petEnabledAtom)
  useEffect(() => {
    setEnabled(true)
  }, [setEnabled])

  const setClickThrough = useCallback((ignore: boolean) => {
    setDeskPetClickThrough(ignore).catch((e) =>
      console.warn('[DeskPetApp] setDeskPetClickThrough failed:', e),
    )
  }, [])

  const onPointerEnter = useCallback(() => setClickThrough(false), [setClickThrough])
  const onPointerLeave = useCallback(() => setClickThrough(true), [setClickThrough])

  const onSpritePointerDown = useCallback((e: React.PointerEvent) => {
    // Left button only; let the bubble's input/button handle their own events.
    if (e.button !== 0) return
    void startWindowDrag()
  }, [])

  return (
    <div
      className="deskpet-root"
      onPointerEnter={onPointerEnter}
      onPointerLeave={onPointerLeave}
    >
      <div
        className="deskpet-sprite"
        onPointerDown={onSpritePointerDown}
        data-testid="deskpet-sprite"
      >
        <PetWidget />
      </div>
      <ChatBubble />
    </div>
  )
}
