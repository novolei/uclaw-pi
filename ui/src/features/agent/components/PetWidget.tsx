/**
 * AI desktop pet anchored to the AgentView composer's top-right edge.
 * Two layered <img> elements crossfade (280ms) between states.
 *
 * Animation files are animated WebP with alpha at /pet/<char>-<state>.webp.
 * State machine is driven by petPrimaryStateAtom + petHoverActiveAtom from
 * usePetStateSync + usePetHover.
 *
 * Spec: docs/superpowers/specs/2026-05-13-pet-widget-design.md
 */
import { useAtomValue } from 'jotai'
import { useEffect, useRef, useState, type HTMLAttributes } from 'react'
import { usePetHover } from '@/hooks/usePetHover'
import {
  petCharacterAtom,
  petDisplayStateAtom,
  petEnabledAtom,
  type PetState,
} from '@/atoms/pet-atoms'
import './PetWidget.css'

type Props = HTMLAttributes<HTMLDivElement>

export function PetWidget(props: Props) {
  const enabled = useAtomValue(petEnabledAtom)
  const character = useAtomValue(petCharacterAtom)
  const state = useAtomValue(petDisplayStateAtom)
  const hoverHandlers = usePetHover()

  const [activeLayer, setActiveLayer] = useState<'a' | 'b'>('a')
  const [layerAState, setLayerAState] = useState<PetState>('idle')
  const [layerBState, setLayerBState] = useState<PetState | null>(null)
  const lastShown = useRef<PetState>('idle')

  useEffect(() => {
    if (state === lastShown.current) return
    const next = activeLayer === 'a' ? 'b' : 'a'
    if (next === 'a') setLayerAState(state)
    else setLayerBState(state)
    requestAnimationFrame(() => {
      setActiveLayer(next)
      lastShown.current = state
    })
  }, [state, activeLayer])

  if (!enabled) return null

  const src = (s: PetState | null) => (s ? `/pet/${character}-${s}.webp` : '')

  return (
    <div
      {...props}
      className={`pet-widget ${props.className ?? ''}`}
      data-char={character}
      {...hoverHandlers}
    >
      <img
        role="img"
        className={`pet-layer ${activeLayer === 'a' ? 'active' : ''}`}
        src={src(layerAState)}
        alt=""
      />
      {layerBState !== null && (
        <img
          role="img"
          className={`pet-layer ${activeLayer === 'b' ? 'active' : ''}`}
          src={src(layerBState)}
          alt=""
        />
      )}
    </div>
  )
}
