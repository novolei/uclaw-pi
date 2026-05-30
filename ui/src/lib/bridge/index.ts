/**
 * `lib/bridge` — the §2A.3 IPC bridge layer (frontend side of the ACL contract).
 *
 * Domain-scoped facades over the legacy `lib/tauri-bridge.ts` monolith. The
 * 343 invoke / 23 listen command names + payload shapes are preserved exactly;
 * this just gives the frontend a modular, mirror-of-backend surface to migrate
 * to (`features/<domain>` import from `lib/bridge/<domain>` instead of the
 * 2.8k-line monolith). Implementations move out of the monolith incrementally.
 */
export * as agent from './agent'
export * as chat from './chat'
export * as models from './models'
export * as events from './events'
