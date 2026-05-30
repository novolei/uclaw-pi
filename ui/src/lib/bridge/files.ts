/**
 * Files-rail bridge (§2A.3 / §2A.4 `dock`/`files`). Re-exports from
 * `lib/tauri-bridge.ts`; names + payloads unchanged (contract preserved).
 */
export {
  filesRailListMounts,
  filesRailReadDir,
  filesRailWatchStart,
  filesRailWatchStop,
} from '../tauri-bridge'
