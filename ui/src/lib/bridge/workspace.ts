/**
 * Workspace bridge (§2A.3). Re-exports from `lib/tauri-bridge.ts`; names +
 * payloads unchanged (contract preserved). workspace is a uClaw concept (pi has
 * none) — these stay uClaw-implemented; the engine only maps cwd → working dir.
 */
export {
  listSpaces,
  createSpace,
  deleteSpace,
  getActiveWorkspaceId,
  setActiveWorkspaceId,
  createWorkspace,
  deleteWorkspace,
  updateWorkspace,
  reorderWorkspaces,
  getWorkspaceDirectories,
  attachWorkspaceDirectory,
  detachWorkspaceDirectory,
} from '../tauri-bridge'
