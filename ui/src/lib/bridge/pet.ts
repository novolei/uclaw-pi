/**
 * Desk-pet (S4) bridge — the frontend side of the pet IPC contract.
 *
 * Commands (see `src-tauri/src/commands/pet.rs`):
 *   - `pet_chat(message, personaId?)` → streams events (no return value)
 *   - `list_pet_personas() -> PetPersona[]`
 *   - `set_pet_persona(id)`
 *   - `show_desk_pet()` / `hide_desk_pet()`
 *   - `set_desk_pet_click_through(ignore)`
 *
 * Streaming events (the contract — byte-identical to the backend emits):
 *   - `pet:reply-delta` { text }     per answer-text delta
 *   - `pet:reply-done`  {}           once the stream finishes
 *   - `pet:reply-error` { message }  model not ready / stream failed
 *
 * Tauri v2 converts snake_case command args to camelCase on the JS side, so
 * `persona_id` is passed as `personaId` and `ignore` stays `ignore`.
 */
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { PetPersona } from '@/atoms/pet-atoms'

export const PET_EVENT = {
  replyDelta: 'pet:reply-delta',
  replyDone: 'pet:reply-done',
  replyError: 'pet:reply-error',
} as const

/** `pet:reply-delta` payload. */
export interface PetReplyDelta {
  text: string
}

/** `pet:reply-error` payload. */
export interface PetReplyError {
  message: string
}

/** Stream a pet reply for `message` under `personaId` (or the active persona). */
export function petChat(message: string, personaId?: string): Promise<void> {
  return invoke('pet_chat', { message, personaId })
}

/** List the built-in personas for the Settings dropdown. */
export function listPetPersonas(): Promise<PetPersona[]> {
  return invoke('list_pet_personas')
}

/** Set the active persona by id. */
export function setPetPersona(id: string): Promise<void> {
  return invoke('set_pet_persona', { id })
}

/** Show (and lazily create) the desk-pet window. */
export function showDeskPet(): Promise<void> {
  return invoke('show_desk_pet')
}

/** Hide the desk-pet window. */
export function hideDeskPet(): Promise<void> {
  return invoke('hide_desk_pet')
}

/** Toggle mouse-transparency (click-through) on the desk-pet window. */
export function setDeskPetClickThrough(ignore: boolean): Promise<void> {
  return invoke('set_desk_pet_click_through', { ignore })
}

const sub =
  <T>(name: string) =>
  (cb: (payload: T) => void): Promise<UnlistenFn> =>
    listen<T>(name, (e) => cb(e.payload))

export const onPetReplyDelta = sub<PetReplyDelta>(PET_EVENT.replyDelta)
export const onPetReplyDone = sub<Record<string, never>>(PET_EVENT.replyDone)
export const onPetReplyError = sub<PetReplyError>(PET_EVENT.replyError)
