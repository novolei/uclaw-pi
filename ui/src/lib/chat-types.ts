/**
 * Chat-layer types — used by components/chat/* and shared with features/agent/*.
 * Split from the legacy proma-types.ts as part of P1 cleanup (Roadmap A4).
 */

// ─────────────────────────────────────────────────────────
// Chat Types
// ─────────────────────────────────────────────────────────

/** Chat 对话元数据 (Proma 格式) */
export interface ConversationMeta {
  id: string
  title: string
  modelId?: string
  channelId?: string
  contextDividers?: string[]
  contextLength?: number | 'infinite'
  /** 是否置顶 */
  pinned?: boolean
  /** 是否已归档 */
  archived?: boolean
  createdAt: number
  updatedAt: number
}

/** Chat 消息 (Proma 格式) */
export interface PrimaChatMessage {
  id: string
  conversationId: string
  role: 'user' | 'assistant' | 'system'
  content: string
  reasoning?: string
  model?: string
  toolActivities?: ChatToolActivity[]
  attachments?: FileAttachment[]
  createdAt: number
  /** Original ordered ContentBlocks. When present, the renderer uses
   *  NativeBlockRenderer for in-order display. Falls back to the flat
   *  `content` + `reasoning` + `toolActivities` path when absent. */
  contentBlocks?: ContentBlock[]
}

/** ChatMessage — Proma 组件使用的消息类型（兼容别名） */
export interface ChatMessage {
  id: string
  role: 'user' | 'assistant' | 'system'
  content: string
  reasoning?: string
  model?: string
  error?: string
  stopped?: boolean
  toolActivities?: ChatToolActivity[]
  attachments?: FileAttachment[]
  createdAt: number
  /** Original ordered ContentBlocks. When present, the renderer uses
   *  NativeBlockRenderer for in-order display. Falls back to the flat
   *  `content` + `reasoning` + `toolActivities` path when absent. */
  contentBlocks?: ContentBlock[]
}

/** 文件附件 */
export interface FileAttachment {
  id?: string
  filename: string
  localPath: string
  mediaType: string
  size: number
  extractedText?: string
}

/** 附件保存输入 */
export interface AttachmentSaveInput {
  conversationId: string
  filename: string
  mediaType: string
  data: string
}

/** Chat 工具活动 */
export interface ChatToolActivity {
  toolCallId: string
  type: 'start' | 'result'
  toolId?: string
  toolName: string
  status?: 'running' | 'completed' | 'failed'
  input?: Record<string, unknown>
  result?: string
  isError?: boolean
  error?: string
  durationMs?: number
  /** 流式工具实时输出 — 仅 `type:"start"` 且工具仍在运行时携带;done 后由 result 接管 */
  liveOutput?: { segments: { stream: 'stdout' | 'stderr'; text: string }[]; bytes: number; droppedHead: boolean }
}

// ===== Native content blocks =====
//
// Mirrors the Rust `ContentBlock` enum at `src-tauri/src/agent/types.rs:55`.
// Serde tags the variant via `type` and uses snake_case, so the wire format
// is e.g. `{ "type": "tool_use", "id": "...", "name": "...", "input": {...} }`.

export type ContentBlock =
  | { type: 'text'; text: string }
  | { type: 'thinking'; thinking: string }
  | { type: 'tool_use'; id: string; name: string; input: Record<string, unknown> }
  | { type: 'tool_result'; tool_use_id: string; content: string; is_error?: boolean }

/** 渠道模型 */
export interface ChannelModel {
  id: string
  name: string
  enabled: boolean
}

/** 渠道（AI 供应商）*/
export interface Channel {
  id: string
  name: string
  provider: string
  providerId?: string
  baseUrl: string
  apiKey?: string
  modelId?: string
  enabled: boolean
  models: ChannelModel[]
  createdAt?: number
  updatedAt?: number
}

/** 模型选项（用于 ModelSelector） */
export interface ModelOption {
  channelId: string
  channelName?: string
  modelId: string
  modelName?: string
  provider?: string
}

/** Chat 发送输入 */
export interface ChatSendInput {
  conversationId: string
  userMessage: string
  messageHistory: unknown[]
  channelId: string
  modelId: string
  contextLength?: number | 'infinite'
  contextDividers?: string[]
  attachments?: FileAttachment[]
  thinkingEnabled?: boolean
  systemMessage?: string
  enabledToolIds?: string[]
}

// ─────────────────────────────────────────────────────────
// Chat Tool Types
// ─────────────────────────────────────────────────────────

/** Chat 工具信息 */
export interface ChatToolInfo {
  meta: ChatToolMeta
  enabled: boolean
  available: boolean
}

/** Chat 工具元数据 */
export interface ChatToolMeta {
  id: string
  name: string
  description: string
  category?: string
  icon?: string
}

/** Chat 工具状态 */
export interface ChatToolState {
  enabled: boolean
}

// ─────────────────────────────────────────────────────────
// Search Result Types (Chat)
// ─────────────────────────────────────────────────────────

export interface MessageSearchResult {
  conversationId: string
  conversationTitle: string
  snippet: string
  matchStart: number
  matchLength: number
  archived?: boolean
}

// ─────────────────────────────────────────────────────────
// Settings & Theme Types
// ─────────────────────────────────────────────────────────

/** 主题模式 */
export type ThemeMode = 'light' | 'dark' | 'system' | 'special'

/** 特殊风格主题 */
export type ThemeStyle = 'default' | 'ocean-light' | 'ocean-dark' | 'forest-light' | 'forest-dark' | 'slate-light' | 'slate-dark' | 'warm-paper' | 'qingye' | 'black' | 'the-finals'

/** 通知音场景类型 */
export type NotificationSoundType = 'taskComplete' | 'permissionRequest' | 'exitPlanMode'

/** 可选通知音 ID */
export type NotificationSoundId = 'ding' | 'ding-dong' | 'discord' | 'done' | 'down-power' | 'food' | 'lite' | 'quiet' | 'none'

/** 各场景通知音配置 */
export interface NotificationSoundSettings {
  taskComplete?: NotificationSoundId
  permissionRequest?: NotificationSoundId
  exitPlanMode?: NotificationSoundId
}

/** 用户档案 */
export interface UserProfile {
  userName: string
  avatar: string
}

export const DEFAULT_USER_AVATAR = '🧑‍💻'
export const DEFAULT_USER_NAME = '用户'

// ─────────────────────────────────────────────────────────
// Environment & Runtime Types
// ─────────────────────────────────────────────────────────

/** 环境检测结果 */
export interface EnvironmentCheckResult {
  hasIssues: boolean
  details?: Record<string, unknown>
}

/** 运行时状态 */
export interface RuntimeStatus {
  node?: { available: boolean; version?: string }
  shell?: {
    gitBash?: { available: boolean }
    wsl?: { available: boolean }
  }
}

/** 安装包清单 */
export interface InstallerManifest {
  items: any[]
}

// ─────────────────────────────────────────────────────────
// File System Types
// ─────────────────────────────────────────────────────────

/** 文件系统条目 */
export interface FileEntry {
  name: string
  path: string
  isDirectory: boolean
  isFile: boolean
  size?: number
  modifiedAt?: number
  children?: FileEntry[]
  extension?: string
}

// ─────────────────────────────────────────────────────────
// Proxy Types
// ─────────────────────────────────────────────────────────

/** 代理配置 */
export interface ProxyConfig {
  mode: 'system' | 'manual' | 'none'
  httpProxy?: string
  httpsProxy?: string
  noProxy?: string
}

// ─────────────────────────────────────────────────────────
// System Prompt Types
// ─────────────────────────────────────────────────────────

/** 系统提示词 */
export interface SystemPrompt {
  id: string
  name: string
  content: string
  isBuiltin?: boolean
  createdAt?: number
  updatedAt?: number
}

/** 系统提示词创建输入 */
export interface SystemPromptCreateInput {
  name: string
  content: string
}

/** 系统提示词更新输入 */
export interface SystemPromptUpdateInput {
  name?: string
  content?: string
}

/** 系统提示词配置 */
export interface SystemPromptConfig {
  prompts: SystemPrompt[]
  defaultPromptId?: string
  appendDateTimeAndUserName: boolean
}

/** 系统提示词版本快照 */
export interface SystemPromptVersion {
  id: string
  promptId: string
  name: string
  content: string
  createdAt: number
}

/** 内置默认提示词 ID */
export const BUILTIN_DEFAULT_ID = 'builtin-default'

/** 内置默认提示词 */
export const BUILTIN_DEFAULT_PROMPT: SystemPrompt = {
  id: BUILTIN_DEFAULT_ID,
  name: '默认',
  content: 'You are a helpful assistant.',
  isBuiltin: true,
}

// ─────────────────────────────────────────────────────────
// IM Integration Types (飞书 / 钉钉 / 微信)
// ─────────────────────────────────────────────────────────

/** 飞书 Bridge 状态 */
export interface FeishuBridgeState {
  status: 'connected' | 'disconnected' | 'connecting' | 'error'
  activeBindings?: number
  error?: string
}

/** 飞书 Bot Bridge 状态 */
export interface FeishuBotBridgeState extends FeishuBridgeState {
  botId?: string
}

/** 飞书通知模式 */
export type FeishuNotifyMode = 'auto' | 'always' | 'off'

/** 飞书聊天绑定 */
export interface FeishuChatBinding {
  chatId: string
  chatName: string
  botId: string
  workspaceSlug?: string
  sessionId?: string
}

/** 钉钉 Bridge 状态 */
export interface DingTalkBridgeState {
  status: 'connected' | 'disconnected' | 'connecting' | 'error'
  error?: string
}

/** 钉钉 Bot Bridge 状态 */
export interface DingTalkBotBridgeState extends DingTalkBridgeState {
  botId?: string
}

/** 微信 Bridge 状态 */
export interface WeChatBridgeState {
  status: 'connected' | 'disconnected' | 'connecting' | 'scanning' | 'confirming' | 'error'
  error?: string
}

// ─────────────────────────────────────────────────────────
// Shortcut Types
// ─────────────────────────────────────────────────────────

/** 用户自定义快捷键覆盖 */
export interface ShortcutOverrides {
  [shortcutId: string]: {
    mac?: string
    win?: string
  }
}
