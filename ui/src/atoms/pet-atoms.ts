/**
 * Pet widget state atoms. See docs/superpowers/specs/2026-05-13-pet-widget-design.md.
 *
 * Three layers:
 *  - User preferences (persisted): petEnabledAtom, petCharacterAtom
 *  - Primary state (runtime): petPrimaryStateAtom — driven by usePetStateSync
 *  - Hover override (runtime): petHoverActiveAtom — driven by usePetHover
 *  - Display state (derived): petDisplayStateAtom — what PetWidget renders
 *
 * Hover only overrides when primary === 'idle'. Other primary states (thinking /
 * typing / success / error) are agent-critical and must not be interrupted by
 * hover.
 */
import { atom } from 'jotai'
import { atomWithStorage } from 'jotai/utils'

export type PetCharacter = 'astro' | 'clawby'

export type PetPrimaryState = 'idle' | 'thinking' | 'typing' | 'success' | 'error'
export type PetState = PetPrimaryState | 'hover'

export const petEnabledAtom = atomWithStorage<boolean>('pet.enabled', false)
export const petCharacterAtom = atomWithStorage<PetCharacter>('pet.character', 'astro')

export const petPrimaryStateAtom = atom<PetPrimaryState>('idle')
export const petHoverActiveAtom = atom<boolean>(false)

/**
 * Desk-pet persona (S4). A persona is system-prompt-only (no LoRA/adapter) —
 * it pairs a sprite `character` with a voice (`systemPrompt`). Wire shape mirrors
 * the backend `PetPersona` (serde camelCase): id / displayName / character /
 * systemPrompt. See `src-tauri/src/local_llm/persona.rs`.
 */
export interface PetPersona {
  id: string
  displayName: string
  character: string
  systemPrompt: string
}

/** Selected persona id, persisted. Defaults to the backend's first built-in. */
export const petPersonaIdAtom = atomWithStorage<string>('pet.personaId', 'astro')

/** The persona roster loaded from `list_pet_personas` (runtime, not persisted). */
export const petPersonasAtom = atom<PetPersona[]>([])

export const petDisplayStateAtom = atom<PetState>((get) => {
  const primary = get(petPrimaryStateAtom)
  const hovering = get(petHoverActiveAtom)
  return hovering && primary === 'idle' ? 'hover' : primary
})
