/**
 * MCP bridge (§2A.3). Re-exports from `lib/tauri-bridge.ts`; names + payloads
 * unchanged (contract preserved).
 */
export {
  listMcpServers,
  addMcpServer,
  updateMcpServer,
  removeMcpServer,
  toggleMcpServer,
  connectMcpServer,
  disconnectMcpServer,
  restartMcpServer,
  listMcpTools,
  refreshMcpTools,
  pingMcpServer,
  listMcpAudit,
} from '../tauri-bridge'
