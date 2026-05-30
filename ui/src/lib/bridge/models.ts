/**
 * Model / provider bridge (§2A.3). Re-exports legacy model commands from
 * `lib/tauri-bridge.ts`; names + payloads unchanged (contract preserved).
 */
export {
  getActiveModel,
  setActiveModel,
  getRoleModels,
  setRoleModel,
  configureProviderWithModels,
  listProviderModels,
  getConfiguredModels,
  getAllConfiguredModels,
} from '../tauri-bridge'
