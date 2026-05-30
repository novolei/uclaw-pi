/**
 * Chat / conversation bridge (§2A.3). Re-exports legacy chat commands from
 * `lib/tauri-bridge.ts`; names + payloads unchanged (contract preserved).
 */
export {
  sendMessage,
  getMessages,
  listConversations,
  createConversation,
  deleteConversation,
  getConversationMessages,
  getRecentMessages,
  stopGeneration,
  truncateMessagesFrom,
  deleteMessage,
  updateConversationTitle,
  togglePinConversation,
  updateConversationModel,
  generateTitle,
} from '../tauri-bridge'
