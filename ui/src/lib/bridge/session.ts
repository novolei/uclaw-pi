/**
 * Session storage / directory / title bridge (§2A.3). Re-exports from
 * `lib/tauri-bridge.ts`; names + payloads unchanged (contract preserved).
 */
export {
  listSessionDirectories,
  attachSessionDirectory,
  detachSessionDirectory,
  listSessionAllowedPaths,
  promoteSessionPathToGlobal,
  generateSessionTitle,
  getSessionTrajectory,
  getSessionCosts,
  rewindSession,
} from '../tauri-bridge'
