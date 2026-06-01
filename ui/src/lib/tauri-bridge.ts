/**
 * Tauri Bridge — 统一 IPC 适配层
 *
 * 提供与 Proma 的 window.electronAPI 相同的接口签名，
 * 内部使用 Tauri invoke / listen 实现。
 * 供 Jotai atoms 和业务层调用。
 *
 * 覆盖所有 uClaw 后端 #[tauri::command] 命令与事件。
 */

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { open as openShell } from '@tauri-apps/plugin-shell';
import type {
  BrowserRuntimeControlCenterReport,
  BrowserRuntimePackAction,
  BrowserRuntimePackExecutionReport,
  BrowserRuntimeProviderId,
  BrowserRuntimeProviderProbeSummary,
  StartupRuntimePackStatusReport,
} from '@/lib/startup/startup-doctor';
import type {
  Settings,
  PatchSettingsInput,
  PlatformInfo,
  VersionInfo,
  BootstrapStatus,
  ConversationResponse,
  CreateConversationInput,
  SendMessageInput,
  SendMessageResponse,
  GetMessagesInput,
  Message,
  SpaceSummary,
  CreateSpaceInput,
  LlmConfigInput,
  LlmConfigResponse,
  ArtifactNode,
  ArtifactContentResponse,
  ArtifactTreeNodeResponse,
  ListArtifactTreeInput,
  LoadArtifactChildrenInput,
  CreateArtifactInput,
  RenameArtifactInput,
  MoveArtifactInput,
  DetectFileTypeResponse,
  ToggleStarResponse,
  SearchInput,
  SearchResult,
  ProviderInfo,
  ProviderConfigInput,
  ProviderConfigResponse,
  ProviderConfigureInput,
  ModelInfo,
  ModelSelectionInfo,
  TestResultInfo,
  ListModelsInput,
  TestConnectionInput,
  ApproveToolCallInput,
  LearnedSkill,
  StreamTextDelta,
  StreamToolStart,
  StreamToolResult,
  StreamThinking,
  StreamThinkingDone,
  StreamDone,
  StreamError,
  TurnCost,
  ContextStats,
  ApprovalRequest,
  // Notifications
  NotificationItem,
  // Background Tasks
  BackgroundTask,
  // Memory KV
  MemorySetInput,
  MemoryGetInput,
  MemorySearchInput,
  MemoryListInput,
  MemoryClearInput,
  MemoryBulkImportInput,
  MemoryEntryResponse,
  MemoryBulkImportResponse,
  MemoryClearResponse,
  // MCP
  McpServerInfo,
  McpServerInput,
  // Built-in Skills
  SkillInfo,
  // skills.sh / skillsmp marketplace
  MarketplaceProvider,
  MarketplaceSkillSummary,
  MarketplaceSkillDetail,
  MarketplaceSkillAudit,
  SkillToggleInput,
  SkillMatchInput,
  SkillMatchResult,
  SkillDetailResponse,
  // Channels
  ChannelInput,
  // Safety
  SafetyPolicyResponse,
  SetSafetyModeInput,
  SetToolOverrideInput,
  ToolNameInput,
  AssessCommandInput,
  CommandRiskResponse,
  // Memory Graph CRUD
  MemoryGraphSearchInput,
  MemoryGraphGetNodeInput,
  MemoryGraphListBootInput,
  MemoryGraphManageBootInput,
  MemoryGraphTimelineInput,
  MemoryGraphExplainRecallInput,
  MemoryGraphCreateNodeInput,
  MemoryGraphUpdateNodeInput,
  MemoryGraphDeleteNodeInput,
  // EntityPage (Memory OS Foundation Phase 1)
  EntityPageCreateInput,
  EntityPageGetInput,
  EntityPageFindBySlugInput,
  EntityPageListInput,
  EntityPageAppendTimelineInput,
  // Wiki artifacts (Memory OS Foundation Phase 3)
  WikiGetInput,
  WikiRegenerateInput,
  WikiArtifactDto,
  WikiRegenerateOutcome,
  // Health findings (Memory OS Foundation Phase 4)
  HealthListInput,
  HealthDismissInput,
  HealthRunNowInput,
  HealthFindingDto,
  HealthRunOutcome,
  // Drift events + importance candidates (Memory OS L3)
  DriftListInput,
  DriftResolveInput,
  ImportanceListInput,
  DriftEventDto,
  ImportanceCandidateDto,
  // Lint (Memory OS Foundation Phase 5)
  LintRunNowInput,
  LintRunOutcome,
  // Learning / openhuman facets (Sprint 1.10 + Sprint 2.2 + Sprint 2.3)
  LearningListFacetsInput,
  LearningRebuildNowInput,
  LearningDismissFacetInput,
  LearningDismissOutcome,
  LearningPromoteFacetInput,
  LearningDemoteFacetInput,
  LearningSetStateOutcome,
  FacetDto,
  // EntityPage manual synth (Memory OS Foundation Phase 6.3)
  EntityPageSynthesizeNowInput,
  EntitySynthesisOutcome,
  // Markdown export (Memory OS Foundation Phase 7.1)
  WikiExportInput,
  BrainExportOutcome,
  // Markdown sync from disk (Memory OS Foundation Phase 7.2)
  WikiSyncInput,
  BrainSyncOutcome,
  // Cost dashboard
  DailyCostRollup,
  ModelCostRollup,
  SessionCostRollup,
  WorkspaceCostRollup,
  BudgetThresholdPayload,
  // Permission rules
  PermissionRule,
  PermissionAuditEntry,
  CreatePermissionRuleInput,
  DefaultPromptsResponse,
} from './types';
import type { Channel } from './chat-types';
import type { RecentThread, AskUserRequest, ExitPlanModeRequest } from './agent-types';
import type { MountRoot } from '@/atoms/files-rail-atoms';
import type { TreeNode } from '@/components/files-rail/utils/tree-patch';

// ─────────────────────────────────────────────────────────
// Bootstrap / Settings
// ─────────────────────────────────────────────────────────

export const getSettings = (): Promise<Settings> =>
  invoke('get_settings');

export const patchSettings = (input: PatchSettingsInput): Promise<Settings> =>
  invoke('patch_settings', { input });

export const getBrowserRuntimeStatus = (): Promise<StartupRuntimePackStatusReport> =>
  invoke('get_browser_runtime_status');

export const getBrowserRuntimeControlCenter = (): Promise<BrowserRuntimeControlCenterReport> =>
  invoke('get_browser_runtime_control_center');

export const setBrowserRuntimeProviderEnabled = (
  providerId: BrowserRuntimeProviderId,
  enabled: boolean,
): Promise<BrowserRuntimeControlCenterReport> =>
  invoke('set_browser_runtime_provider_enabled', { providerId, enabled });

export const setBrowserRuntimeProviderPriority = (
  providerIds: BrowserRuntimeProviderId[],
): Promise<BrowserRuntimeControlCenterReport> =>
  invoke('set_browser_runtime_provider_priority', { providerIds });

export const setBrowserRuntimeMcpRawToolsExposed = (
  exposed: boolean,
): Promise<BrowserRuntimeControlCenterReport> =>
  invoke('set_browser_runtime_mcp_raw_tools_exposed', { exposed });

export const runBrowserRuntimeProviderProbe = (
  providerId: BrowserRuntimeProviderId,
): Promise<BrowserRuntimeProviderProbeSummary> =>
  invoke('run_browser_runtime_provider_probe', { providerId });

export type PlaywrightSetupAction =
  | 'auto_setup'
  | 'install_node_with_homebrew'
  | 'refresh_skills'
  | 'probe_mcp';

export interface PlaywrightSetupStepExecutionReport {
  stepId: string
  command: string
  args: string[]
  status: 'succeeded' | 'failed' | 'timed_out' | 'spawn_failed'
  exitCode?: number | null
  stdout: string
  stderr: string
  error?: string | null
}

export interface PlaywrightSetupExecutionReport {
  action: PlaywrightSetupAction
  status: 'succeeded' | 'failed' | 'blocked'
  blockedReason?: string | null
  stepReports: PlaywrightSetupStepExecutionReport[]
}

export const runPlaywrightSetup = (
  action: PlaywrightSetupAction,
): Promise<PlaywrightSetupExecutionReport> =>
  invoke('run_playwright_setup', { action });

export const dryRunBrowserRuntimeAction = (
  action: BrowserRuntimePackAction,
): Promise<BrowserRuntimePackExecutionReport> =>
  invoke('dry_run_browser_runtime_action', { action });

export const executeBrowserRuntimeAction = (
  action: BrowserRuntimePackAction,
  confirmed: boolean,
): Promise<BrowserRuntimePackExecutionReport> =>
  invoke('execute_browser_runtime_action', { action, confirmed });

export type BrowserIdentityKind =
  | 'real_browser_profile'
  | 'storage_state'
  | 'cookie_jar'
  | 'bearer_token';

export type BrowserIdentityProvider =
  | 'system_chrome'
  | 'playwright'
  | 'browser_use'
  | 'manual_import';

export type BrowserIdentityScope = 'workspace' | 'session' | 'global';

export type BrowserIdentityStatus = 'live' | 'stale' | 'unknown' | 'revoked';

export interface BrowserIdentityProfileSummary {
  id: string;
  label: string;
  originPattern: string;
  kind: BrowserIdentityKind;
  provider: BrowserIdentityProvider;
  scope: BrowserIdentityScope;
  createdAtMs: number;
  lastUsedAtMs: number | null;
  lastVerifiedAtMs: number | null;
  expiresAtMs: number | null;
  revokedAtMs: number | null;
  status: BrowserIdentityStatus;
  revoked: boolean;
}

export interface BrowserIdentityActiveTaskSummary {
  profileId: string;
  runId: string;
  sessionId: string;
  task: string;
  status:
    | 'running'
    | 'completed'
    | 'failed'
    | 'stopped'
    | 'needs_user_intervention'
    | 'paused_waiting_for_browser_runtime'
    | 'paused_checkpointed';
  startedAtMs: number;
  updatedAtMs: number;
  drainDeadlineMs: number | null;
}

export interface BrowserIdentityStatusReport {
  profiles: BrowserIdentityProfileSummary[];
  authorizedCount: number;
  revokedCount: number;
  activeTaskCount: number;
  activeTasks: BrowserIdentityActiveTaskSummary[];
}

export interface BrowserIdentityRevocationReport {
  profile: BrowserIdentityProfileSummary | null;
  revoked: boolean;
  activeTaskCount: number;
  activeTasks: BrowserIdentityActiveTaskSummary[];
  drainDeadlineMs: number | null;
}

export interface BrowserIdentityAuthorizationTabInput {
  sessionId: string;
  tabId: string;
  label: string;
  url: string;
  scope?: BrowserIdentityScope | null;
}

export interface BrowserIdentityAuthorizationWebviewInput {
  webviewLabel: string;
  label: string;
  url: string;
  scope?: BrowserIdentityScope | null;
}

export interface BrowserIdentityAuthorizationReport {
  completed: boolean;
  profile: BrowserIdentityProfileSummary | null;
  profileId: string | null;
  originPattern: string | null;
  capturedCookieCount: number;
  capturedOriginCount: number;
  message: string | null;
}

export const listBrowserIdentities = (): Promise<BrowserIdentityStatusReport> =>
  invoke('list_browser_identities');

export const completeBrowserIdentityAuthorizationFromTab = (
  input: BrowserIdentityAuthorizationTabInput,
): Promise<BrowserIdentityAuthorizationReport> =>
  invoke('complete_browser_identity_authorization_from_tab', { input });

export const completeBrowserIdentityAuthorizationFromWebview = (
  input: BrowserIdentityAuthorizationWebviewInput,
): Promise<BrowserIdentityAuthorizationReport> =>
  invoke('complete_browser_identity_authorization_from_webview', { input });

export const revokeBrowserIdentity = (
  profileId: string,
): Promise<BrowserIdentityRevocationReport> =>
  invoke('revoke_browser_identity', { profileId });

export const getPlatform = (): Promise<PlatformInfo> =>
  invoke('get_platform');

export const getVersion = (): Promise<VersionInfo> =>
  invoke('get_version');

export const getBootstrapStatus = (): Promise<BootstrapStatus> =>
  invoke('get_bootstrap_status');

// ─────────────────────────────────────────────────────────
// Memory Recall Config
// ─────────────────────────────────────────────────────────

export interface MemoryRecallConfigDto {
  bootLimit?: number;
  triggerLimit?: number;
  seedLimit?: number;
  expansionLimit?: number;
  recentLimit?: number;
  fusionStrategy?: 'rrf' | 'weighted';
  rrfK?: number;
  ftsWeight?: number;
  vectorWeight?: number;
  bootLearnedSkillsLimit?: number;
  tokenBudget?: number;
  layerExpandedSeedTake?: number;
  layerExpandedMaxDepth?: number;
  timeDecayHalfLifeDays?: number;
  ftsFallbackLimitMultiplier?: number;
  bootUserProfileLimit?: number;
}

export const getMemoryRecallConfig = (): Promise<MemoryRecallConfigDto> =>
  invoke('get_memory_recall_config');

export const patchMemoryRecallConfig = (input: MemoryRecallConfigDto): Promise<MemoryRecallConfigDto> =>
  invoke('patch_memory_recall_config', { input });

// ─────────────────────────────────────────────────────────
// Conversations
// ─────────────────────────────────────────────────────────

export const listConversations = (): Promise<ConversationResponse[]> =>
  invoke('list_conversations');

export const createConversation = (input: CreateConversationInput): Promise<ConversationResponse> =>
  invoke('create_conversation', { input });

export const deleteConversation = (id: string): Promise<boolean> =>
  invoke('delete_conversation', { id });

export const getMessages = (input: GetMessagesInput): Promise<Message[]> =>
  invoke('get_messages', { input });

export const sendMessage = (input: SendMessageInput): Promise<SendMessageResponse> =>
  invoke('send_message', { input });

export const approveToolCall = (input: ApproveToolCallInput): Promise<{ success: boolean }> =>
  invoke('approve_tool_call', { input });

export const toggleStarConversation = (conversationId: string): Promise<ToggleStarResponse> =>
  invoke('toggle_star_conversation', { input: { conversationId } });

// ─────────────────────────────────────────────────────────
// Shared workspace types
// ─────────────────────────────────────────────────────────

export interface WorkspaceInfo {
  id: string
  name: string
  icon: string
  path: string | null
  createdAt: string
  updatedAt: string
}

export interface TurnRecord {
  id: string
  sessionId: string
  turnIndex: number
  role: string
  content?: string
  toolName?: string
  toolArgs?: string
  toolResult?: string
  reasoning?: string
  isError: boolean
  durationMs: number
  createdAt: number
}

export interface TrajectorySearchHit {
  sessionId: string
  turnIndex: number
  toolName?: string
  snippet: string
  createdAt: number
}

export interface SessionTitleUpdate {
  sessionId: string
  title: string
  emoji: string
}

// ─────────────────────────────────────────────────────────
// Spaces
// ─────────────────────────────────────────────────────────

export const listSpaces = (): Promise<SpaceSummary[]> =>
  invoke('list_spaces');

export const listRecentThreads = (): Promise<RecentThread[]> =>
  invoke<RecentThread[]>('list_recent_threads')

export const createSpace = (input: CreateSpaceInput): Promise<SpaceSummary> =>
  invoke('create_space', { input });

export const deleteSpace = (id: string): Promise<boolean> =>
  invoke('delete_space', { id });

// ─── Workspace (active space management) ─────────────────────────────

export const getActiveWorkspaceId = (): Promise<string | null> =>
  invoke<string | null>('get_active_workspace_id')

export const setActiveWorkspaceId = (id: string): Promise<void> =>
  invoke('set_active_workspace_id', { id })

export const createWorkspace = (name: string, path?: string, icon?: string): Promise<{
  id: string; name: string; icon: string; path: string | null; createdAt: string
}> =>
  invoke('create_workspace', { name, path: path ?? null, icon: icon ?? null })

export const deleteWorkspace = (id: string): Promise<void> =>
  invoke('delete_workspace', { id })

export const updateWorkspace = (input: { id: string; name?: string; icon?: string }): Promise<{
  id: string; name: string; icon: string; path: string | null; sortOrder: number; createdAt: string; updatedAt: string
}> => invoke('update_workspace', input)

export const reorderWorkspaces = (orderedIds: string[]): Promise<void> =>
  invoke('reorder_workspaces', { orderedIds })

export const getWorkspaceDirectories = (workspaceId: string): Promise<string[]> =>
  invoke('get_workspace_directories', { workspaceId })

export const attachWorkspaceDirectory = (workspaceId: string, dirPath: string): Promise<string[]> =>
  invoke('attach_workspace_directory', { workspaceId, dirPath })

export const detachWorkspaceDirectory = (workspaceId: string, dirPath: string): Promise<string[]> =>
  invoke('detach_workspace_directory', { workspaceId, dirPath })

export const listSessionDirectories = (sessionId: string): Promise<string[]> =>
  invoke('list_session_directories', { sessionId })

export const attachSessionDirectory = (sessionId: string, dirPath: string): Promise<string[]> =>
  invoke('attach_session_directory', { sessionId, dirPath })

export const detachSessionDirectory = (sessionId: string, dirPath: string): Promise<string[]> =>
  invoke('detach_session_directory', { sessionId, dirPath })

export const renameAttachedFile = (path: string, newName: string): Promise<string> =>
  invoke('rename_attached_file', { path, newName })

export const moveAttachedFile = (path: string, destDir: string): Promise<string> =>
  invoke('move_attached_file', { path, destDir })

export const readAttachedFile = (path: string): Promise<number[]> =>
  invoke('read_attached_file', { path })

export const uploadWorkspaceFile = (
  workspaceId: string,
  filename: string,
  content: number[],
): Promise<string> => invoke('upload_workspace_file', { workspaceId, filename, content })

export const pathIsDirectory = (path: string): Promise<boolean> =>
  invoke('path_is_directory', { path })

export const copyFileIntoWorkspace = (workspaceId: string, sourcePath: string): Promise<string> =>
  invoke('copy_file_into_workspace', { workspaceId, sourcePath })

/** Delete a single file by absolute path. Backend rejects directories. */
export const deleteWorkspaceFile = (path: string): Promise<void> =>
  invoke('delete_workspace_file', { path })

// Path policy (Phase 3)
export const listAlwaysAllowedPaths = (): Promise<string[]> =>
  invoke('list_always_allowed_paths')

export const addAlwaysAllowedPath = (path: string): Promise<void> =>
  invoke('add_always_allowed_path', { path })

export const removeAlwaysAllowedPath = (path: string): Promise<void> =>
  invoke('remove_always_allowed_path', { path })

export const listSessionAllowedPaths = (sessionId: string): Promise<string[]> =>
  invoke('list_session_allowed_paths', { sessionId })

export const promoteSessionPathToGlobal = (sessionId: string, path: string): Promise<void> =>
  invoke('promote_session_path_to_global', { sessionId, path })

// ─── Session title ────────────────────────────────────────────────────

export const generateSessionTitle = (sessionId: string, firstMessage: string): Promise<void> =>
  invoke('generate_session_title', { sessionId, firstMessage })

// ─── Trajectory ───────────────────────────────────────────────────────

export const getSessionTrajectory = (sessionId: string): Promise<TurnRecord[]> =>
  invoke('get_session_trajectory', { sessionId })

export const searchTrajectories = (query: string, limit?: number): Promise<TrajectorySearchHit[]> =>
  invoke('search_trajectories', { query, limit: limit ?? 20 })

// ─────────────────────────────────────────────────────────
// LLM Config
// ─────────────────────────────────────────────────────────

export const getLlmConfig = (): Promise<LlmConfigResponse> =>
  invoke('get_llm_config');

export const updateLlmConfig = (input: LlmConfigInput): Promise<LlmConfigResponse> =>
  invoke('update_llm_config', { input });

// ─────────────────────────────────────────────────────────
// Artifacts
// ─────────────────────────────────────────────────────────

export const listArtifacts = (): Promise<ArtifactNode[]> =>
  invoke('list_artifacts');

export const readArtifact = (path: string): Promise<ArtifactContentResponse> =>
  invoke('read_artifact', { input: { path } });

export const writeArtifact = (path: string, content: string): Promise<ArtifactContentResponse> =>
  invoke('write_artifact', { input: { path, content } });

export const deleteArtifact = (path: string): Promise<boolean> =>
  invoke('delete_artifact', { path });

export const listArtifactsTree = (input: ListArtifactTreeInput): Promise<ArtifactTreeNodeResponse[]> =>
  invoke('list_artifacts_tree', { input });

export const loadArtifactChildren = (input: LoadArtifactChildrenInput): Promise<ArtifactTreeNodeResponse[]> =>
  invoke('load_artifact_children', { input });

/** List immediate children of an arbitrary directory path. Used by the
 * Files tab's FileBrowser to show real disk contents under the workspace
 * folder. Hidden files filtered server-side. */
export const listDirectoryEntries = (path: string): Promise<Array<{
  name: string
  path: string
  isDirectory: boolean
  isFile: boolean
  size?: number
  extension?: string
}>> => invoke('list_directory_entries', { path });

export const createArtifact = (input: CreateArtifactInput): Promise<ArtifactTreeNodeResponse> =>
  invoke('create_artifact', { input });

export const renameArtifact = (input: RenameArtifactInput): Promise<boolean> =>
  invoke('rename_artifact', { input });

export const moveArtifact = (input: MoveArtifactInput): Promise<boolean> =>
  invoke('move_artifact', { input });

export const deleteArtifactRecursive = (spaceId: string, path: string): Promise<boolean> =>
  invoke('delete_artifact_recursive', { spaceId, path });

export const detectFileType = (path: string): Promise<DetectFileTypeResponse> =>
  invoke('detect_file_type', { path });

// ─────────────────────────────────────────────────────────
// Search
// ─────────────────────────────────────────────────────────

export const searchWorkspace = (input: SearchInput): Promise<SearchResult[]> =>
  invoke('search_workspace', { input });

export const searchConversations = (input: SearchInput): Promise<SearchResult[]> =>
  invoke('search_conversations', { input });

export const searchAll = (input: SearchInput): Promise<SearchResult[]> =>
  invoke('search_all', { input });

// ─────────────────────────────────────────────────────────
// Providers
// ─────────────────────────────────────────────────────────

export const listProviders = (): Promise<ProviderInfo[]> =>
  invoke('list_providers');

export const listConfiguredProviders = (): Promise<string[]> =>
  invoke('list_configured_providers');

export const getProviderConfig = (providerId: string): Promise<ProviderConfigResponse | null> =>
  invoke('get_provider_config', { providerId });

export const configureProvider = (input: ProviderConfigInput): Promise<void> =>
  invoke('configure_provider', { input });

export const configureProviderWithModels = (input: ProviderConfigureInput): Promise<void> => {
  const { modelIds, ...config } = input;
  return invoke('configure_provider_with_models', { providerConfig: config, modelIds });
};

export const removeProviderConfig = (providerId: string): Promise<void> =>
  invoke('remove_provider_config', { providerId });

export const testProviderConnection = (input: TestConnectionInput): Promise<TestResultInfo> =>
  invoke('test_provider_connection', { input });

export const listProviderModels = (input: ListModelsInput): Promise<ModelInfo[]> =>
  invoke('list_provider_models', { input });

export const getConfiguredModels = (providerId: string): Promise<string[]> =>
  invoke('get_configured_models', { providerId });

export const getAllConfiguredModels = (): Promise<[string, string[]][]> =>
  invoke('get_all_configured_models');

export const getActiveModel = (): Promise<ModelSelectionInfo | null> =>
  invoke('get_active_model');

export const setActiveModel = (providerId: string, modelId: string): Promise<void> =>
  invoke('set_active_model', { providerId, modelId });

export interface ModelRoleConfig {
  role: string
  model_ref: string | null
}

export const getRoleModels = (): Promise<ModelRoleConfig[]> =>
  invoke('get_role_models');

export const setRoleModel = (role: string, modelRef: string | null): Promise<void> =>
  invoke('set_role_model', { role, modelRef });

// ─────────────────────────────────────────────────────────
// Learned Skills
// ─────────────────────────────────────────────────────────

export const listLearnedSkills = (spaceId: string = 'default'): Promise<LearnedSkill[]> =>
  invoke('list_learned_skills', { spaceId });

export const getLearnedSkill = (skillId: string): Promise<LearnedSkill> =>
  invoke('get_learned_skill', { skillId });

export const toggleLearnedSkill = (skillId: string, enabled: boolean): Promise<void> =>
  invoke('toggle_learned_skill', { skillId, enabled });

export const deleteLearnedSkill = (skillId: string): Promise<void> =>
  invoke('delete_learned_skill', { skillId });

/**
 * Set the lifecycle stage of a learned skill.
 * Stages: 'draft' | 'promoted' | 'deprecated'
 */
export const setSkillLifecycle = (
  nodeId: string,
  lifecycle: 'draft' | 'promoted' | 'deprecated',
): Promise<void> =>
  invoke('set_skill_lifecycle', { nodeId, lifecycle });

/**
 * Update editable fields of a learned skill.
 * Non-undefined fields are patched into the node's metadata.
 * A new active version is created with regenerated content.
 */
export interface UpdateLearnedSkillInput {
  nodeId: string;
  context?: string;
  principles?: string;
  steps?: string;
  pitfalls?: string;
  category?: string;
  tags?: string[];
  validationHint?: string;
}

export const updateLearnedSkill = (
  input: UpdateLearnedSkillInput,
): Promise<void> =>
  invoke('update_learned_skill', { input });

/**
 * Record that the LLM cited a learned skill in its response.
 * Bumps `cited_count` in metadata (separate from `usage_count` /
 * `recalled_count`). Returns the matched skill_id, or null if the
 * cited title doesn't match any skill in the DB.
 */
export const recordSkillCited = (
  title: string,
  spaceId: string = 'default',
): Promise<string | null> =>
  invoke('record_skill_cited', { spaceId, title });

/**
 * Backfill `memory_keywords` rows for learned skills that lack them.
 * Idempotent: skills already indexed are skipped. Re-running is safe.
 */
export interface SkillKeywordBackfillResult {
  totalLearnedSkills: number;
  alreadyIndexed: number;
  backfilledSkills: number;
  keywordsInserted: number;
}
export const backfillSkillKeywords = (
  spaceId: string = 'default',
): Promise<SkillKeywordBackfillResult> =>
  invoke('backfill_skill_keywords', { spaceId });

// — Skill consolidation —

export interface SkillConsolidationCluster {
  canonicalId: string
  canonicalTitle: string
  mergedTitle: string
  mergedContext: string
  mergedPrinciples: string
  mergedSteps: string
  mergedPitfalls: string
  duplicateIds: string[]
  duplicateTitles: string[]
  reason: string
}

export interface SkillConsolidationProposal {
  clusters: SkillConsolidationCluster[]
  totalSkills: number
  proposedCanonicalCount: number
}

export interface SkillConsolidationResult {
  appliedClusters: number
  deprecatedSkills: number
  updatedSkills: number
}

export const proposeSkillConsolidation = (
  spaceId: string = 'default',
): Promise<SkillConsolidationProposal> =>
  invoke('propose_skill_consolidation', { spaceId });

export const cancelSkillConsolidation = (): Promise<void> =>
  invoke('cancel_skill_consolidation');

export const applySkillConsolidation = (
  plan: SkillConsolidationProposal,
): Promise<SkillConsolidationResult> =>
  invoke('apply_skill_consolidation', { plan });

// ─ Skill version history ─

export interface SkillVersionInfo {
  id: string;
  status: string;
  content: string;
  createdAt: string;
}

export const getSkillVersions = (nodeId: string): Promise<SkillVersionInfo[]> =>
  invoke<SkillVersionInfo[]>('get_skill_versions', { nodeId }).catch((e) => {
    console.error('[getSkillVersions]', e);
    return [];
  });

// ─ User skill creation / deletion ─

export interface CreateUserSkillInput {
  name: string;
  description: string;
  category?: string;
  keywords?: string[];
}

export const createUserSkill = async (input: CreateUserSkillInput): Promise<string> =>
  invoke('create_user_skill', { input });

export const deleteUserSkill = async (name: string): Promise<void> =>
  invoke('delete_user_skill', { name });

/** Install a skill from a marketplace. scope 'global' = active everywhere
 *  (untagged); 'workspace' = active in `workspaceId` only (tag-scoped). For
 *  skillsmp pass provider 'skillsmp' + `source` = the row's installUrl (githubUrl).
 *  Returns a status slug. */
export const installSkillFromMarketplace = (
  id: string,
  scope: 'global' | 'workspace',
  workspaceId?: string,
  provider?: MarketplaceProvider,
  source?: string,
): Promise<string> =>
  invoke('install_skill_from_marketplace', { id, scope, workspaceId, provider, source });

/** Fully uninstall a marketplace skill (deletes files + V25 row + registry evict). */
export const uninstallSkillFromMarketplace = (slug: string): Promise<void> =>
  invoke('uninstall_skill_from_marketplace', { slug });

/** Search a marketplace by free-text query (backend default skills_sh; the UI
 *  passes 'skillsmp' explicitly). */
export const searchSkillsMarketplace = (
  query: string,
  limit?: number,
  provider?: MarketplaceProvider,
): Promise<MarketplaceSkillSummary[]> =>
  invoke('search_skill_marketplace', { query, limit, provider });
/** List a marketplace (skills.sh views; skillsmp has no list → empty). */
export const listSkillsMarketplace = (
  view?: string,
  page?: number,
  provider?: MarketplaceProvider,
): Promise<MarketplaceSkillSummary[]> =>
  invoke('list_skill_marketplace', { view, page, provider });
/** Fetch a skill's detail (inline files). skillsmp needs `source` (the githubUrl). */
export const getSkillMarketplaceDetail = (
  id: string,
  provider?: MarketplaceProvider,
  source?: string,
): Promise<MarketplaceSkillDetail> =>
  invoke('get_skill_marketplace_detail', { id, provider, source });
/** Fetch a skill's audit verdicts (skillsmp has none → empty). */
export const getSkillMarketplaceAudit = (
  id: string,
  provider?: MarketplaceProvider,
): Promise<MarketplaceSkillAudit> =>
  invoke('get_skill_marketplace_audit', { id, provider });
/** Whether an installed marketplace skill has a newer version. skillsmp needs `source`. */
export const checkSkillMarketplaceUpdate = (
  id: string,
  provider?: MarketplaceProvider,
  source?: string,
): Promise<boolean> =>
  invoke('check_skill_marketplace_update', { id, provider, source });

// ─────────────────────────────────────────────────────────
// GEP Gene Evolution
// ─────────────────────────────────────────────────────────

export interface GeneSummary {
  gene_id: string
  asset_id: string
  category: string
  summary: string
  version: string
  status: string
  created_at: number
  updated_at: number
  capsule_count: number
}

export interface GeneDetail {
  gene: {
    gene_id: string
    version: string
    category: string
    signals_match: string[]
    summary: string
    strategy: string[]
    avoid: string[]
    constraints: { max_files: number; forbidden_paths: string[] }
    validation: string
    asset_id: string
    status: string
    created_at: number
    updated_at: number
  }
  capsules: Array<{
    id: string
    gene_asset_id: string
    gene_id: string
    trigger: string[]
    summary: string
    confidence: number
    blast_radius: { files: number; lines: number }
    outcome: { status: string; score: number }
    raw_streak: number
    effective_streak: number
    env_fingerprint: { rust_version: string; platform: string; arch: string }
    created_at: number
    lineage: string[]
  }>
  events: Array<{
    capsule_id: string
    created_at: number
  }>
}

export interface GeneEvolutionTree {
  gene_id: string
  versions: Array<{
    asset_id: string
    version: string
    parent_asset_id: string | null
    created_at: number
    summary: string
  }>
}

export const listGenes = (statusFilter?: string): Promise<GeneSummary[]> =>
  invoke<GeneSummary[]>('list_genes', { statusFilter });

export const getGeneDetail = (assetId: string): Promise<GeneDetail> =>
  invoke<GeneDetail>('get_gene_detail', { assetId });

export const getGeneEvolutionTree = (geneId: string): Promise<GeneEvolutionTree> =>
  invoke<GeneEvolutionTree>('get_gene_evolution_tree', { geneId });

export const retireGene = (assetId: string, reason: string): Promise<void> =>
  invoke('retire_gene', { assetId, reason });

export const reactivateGene = (assetId: string): Promise<void> =>
  invoke('reactivate_gene', { assetId });

// ─────────────────────────────────────────────────────────
// Memory Graph
// ─────────────────────────────────────────────────────────

export const memoryGraphListBoot = (input: MemoryGraphListBootInput): Promise<unknown> =>
  invoke('memory_graph_list_boot', { input });

export const memoryGraphGetNode = (input: MemoryGraphGetNodeInput): Promise<unknown> =>
  invoke('memory_graph_get_node', { input });

export const memoryGraphSearch = (input: MemoryGraphSearchInput): Promise<unknown> =>
  invoke('memory_graph_search', { input });

export const memoryGraphGetFullGraph = (): Promise<unknown> =>
  invoke('memory_graph_get_full_graph');

export const memoryGraphManageBoot = (input: MemoryGraphManageBootInput): Promise<unknown> =>
  invoke('memory_graph_manage_boot', { input });

export const memoryGraphListTimeline = (input: MemoryGraphTimelineInput): Promise<unknown> =>
  invoke('memory_graph_list_timeline', { input });

export const memoryGraphExplainRecall = (input: MemoryGraphExplainRecallInput): Promise<unknown> =>
  invoke('memory_graph_explain_recall', { input });

export const memoryGraphCreateNode = (input: MemoryGraphCreateNodeInput): Promise<unknown> =>
  invoke('memory_graph_create_node', { input });

export interface MemoryGraphQuickCaptureInput {
  content: string
  title?: string
  spaceId?: string
  source?: string
  tags?: string[]
}

export const memoryGraphQuickCapture = (input: MemoryGraphQuickCaptureInput): Promise<unknown> =>
  invoke('memory_graph_quick_capture', { input })

export const memoryGraphUpdateNode = (input: MemoryGraphUpdateNodeInput): Promise<unknown> =>
  invoke('memory_graph_update_node', { input });

export const memoryGraphDeleteNode = (input: MemoryGraphDeleteNodeInput): Promise<unknown> =>
  invoke('memory_graph_delete_node', { input });

// ─────────────────────────────────────────────────────────
// EntityPage (Memory OS Foundation Phase 1)
// ─────────────────────────────────────────────────────────
//
// Thin invoke wrappers for the `memory_entity_page_*` commands. All return
// `unknown` (matching the existing Memory Graph wrapper style) — caller
// code that wants typed access should narrow via the response shape from
// the Rust side (`MemoryNodeDetail`).

export const memoryEntityPageCreate = (input: EntityPageCreateInput): Promise<unknown> =>
  invoke('memory_entity_page_create', { input });

export const memoryEntityPageGet = (input: EntityPageGetInput): Promise<unknown> =>
  invoke('memory_entity_page_get', { input });

export const memoryEntityPageFindBySlug = (input: EntityPageFindBySlugInput): Promise<unknown> =>
  invoke('memory_entity_page_find_by_slug', { input });

export const memoryEntityPageList = (input: EntityPageListInput): Promise<unknown> =>
  invoke('memory_entity_page_list', { input });

export const memoryEntityPageAppendTimeline = (
  input: EntityPageAppendTimelineInput,
): Promise<unknown> =>
  invoke('memory_entity_page_append_timeline', { input });

// ─────────────────────────────────────────────────────────
// Wiki artifacts (Memory OS Foundation Phase 3)
// ─────────────────────────────────────────────────────────
//
// All three commands return `null` when no row exists (rather than
// throwing) — the WikiView treats null as "empty state, prompt the
// user to regenerate".

export const memoryWikiGetOverview = (
  input: WikiGetInput,
): Promise<WikiArtifactDto | null> =>
  invoke('memory_wiki_get_overview', { input });

export const memoryWikiGetIndex = (
  input: WikiGetInput,
): Promise<WikiArtifactDto | null> =>
  invoke('memory_wiki_get_index', { input });

export const memoryWikiRegenerate = (
  input: WikiRegenerateInput,
): Promise<WikiRegenerateOutcome> =>
  invoke('memory_wiki_regenerate', { input });

// ─────────────────────────────────────────────────────────
// Health findings (Memory OS Foundation Phase 4)
// ─────────────────────────────────────────────────────────
//
// list/dismiss keep working when memory_health_enabled = false (so the
// user can triage existing findings); only run_now refuses.

export const memoryHealthListFindings = (
  input: HealthListInput,
): Promise<HealthFindingDto[]> =>
  invoke('memory_health_list_findings', { input });

export const memoryHealthDismissFinding = (
  input: HealthDismissInput,
): Promise<{ success: boolean; findingId: string; alreadyMissing: boolean }> =>
  invoke('memory_health_dismiss_finding', { input });

export const memoryHealthRunNow = (
  input: HealthRunNowInput,
): Promise<HealthRunOutcome> =>
  invoke('memory_health_run_now', { input });

// ─────────────────────────────────────────────────────────
// Drift events + importance candidates (Memory OS L3)
// ─────────────────────────────────────────────────────────

export const memoryDriftListEvents = (
  input: DriftListInput,
): Promise<DriftEventDto[]> =>
  invoke('memory_drift_list_events', { input });

export const memoryDriftResolveEvent = (
  input: DriftResolveInput,
): Promise<void> =>
  invoke('memory_drift_resolve_event', { input });

export const memoryImportanceListCandidates = (
  input: ImportanceListInput,
): Promise<ImportanceCandidateDto[]> =>
  invoke('memory_importance_list_candidates', { input });

// ─────────────────────────────────────────────────────────
// Lint scan (Memory OS Foundation Phase 5)
// ─────────────────────────────────────────────────────────
//
// run_now respects the daily token budget — if today's spend already
// meets/exceeds the cap the call still succeeds but returns
// total_inserted = 0 and skipped_due_to_budget > 0.

export const memoryLintRunNow = (
  input: LintRunNowInput,
): Promise<LintRunOutcome> =>
  invoke('memory_lint_run_now', { input });

// ─────────────────────────────────────────────────────────
// Learning / openhuman facets (Sprint 1.10 + Sprint 2.2)
// ─────────────────────────────────────────────────────────
//
// list/dismiss work regardless of `memory_os.learning_enabled` so the
// user can still inspect/triage existing facets if they turn the
// pipeline off; only rebuild_now refuses (returns a structured error).

export const memoryLearningListFacets = (
  input: LearningListFacetsInput,
): Promise<FacetDto[]> =>
  invoke('memory_learning_list_facets', { input });

export const memoryLearningDismissFacet = (
  input: LearningDismissFacetInput,
): Promise<LearningDismissOutcome> =>
  invoke('memory_learning_dismiss_facet', { input });

/** Returns the raw `RebuildOutcome` shape (snake_case via serde
 *  default). Frontend treats it as `unknown` because the only fields
 *  the UI needs (totals) are forwarded into a toast. */
export const memoryLearningRebuildNow = (
  input: LearningRebuildNowInput,
): Promise<Record<string, unknown>> =>
  invoke('memory_learning_rebuild_now', { input });

// Sprint 2.3 — soft promote / demote a facet's state. Both return the
// same `{ facet_id, rows_updated, new_state }` shape as dismiss.

export const memoryLearningPromoteFacet = (
  input: LearningPromoteFacetInput,
): Promise<LearningSetStateOutcome> =>
  invoke('memory_learning_promote_facet', { input });

export const memoryLearningDemoteFacet = (
  input: LearningDemoteFacetInput,
): Promise<LearningSetStateOutcome> =>
  invoke('memory_learning_demote_facet', { input });

// ─────────────────────────────────────────────────────────
// Memory OS Phase 6.3 — Manual EntityPage Synthesis
// ─────────────────────────────────────────────────────────
// Re-compiles an EntityPage's compiled_truth via the configured
// EntitySynthesizer (Stub or Real per the
// `memory_os.entity_synthesizer_enabled` flag). Surfaces the produced
// markdown + new aliases + cost in the response so the UI can refresh
// the detail panel without a follow-up fetch.

export const memoryEntityPageSynthesizeNow = (
  input: EntityPageSynthesizeNowInput,
): Promise<EntitySynthesisOutcome> =>
  invoke('memory_entity_page_synthesize_now', { input });

// ─────────────────────────────────────────────────────────
// Memory OS Phase 7.1 — Brain Markdown Export
// ─────────────────────────────────────────────────────────
// One-shot export of every EntityPage in the space to a markdown
// mirror at `<brainRoot>/<subkind>/<slug>.md`, plus overview.md /
// index.md. Idempotent: re-running on an unchanged page costs only
// a SHA-256 comparison. `brainRoot` defaults to
// `~/Documents/workground/brain/` on the backend when omitted.

export const memoryWikiExport = (
  input: WikiExportInput,
): Promise<BrainExportOutcome> =>
  invoke('memory_wiki_export', { input });

// Phase 7.2 — walk the brain dir and reconcile each `.md` file with
// its EntityPage row. Disk-wins on conflict; conflicts get counted in
// the outcome so the UI can surface them (Phase 7.3 writes
// memory_health_findings rows for each one).
export const memoryWikiSyncFromDisk = (
  input: WikiSyncInput,
): Promise<BrainSyncOutcome> =>
  invoke('memory_wiki_sync_from_disk', { input });

// ─────────────────────────────────────────────────────────
// Notifications
// ─────────────────────────────────────────────────────────

export const getNotifications = (): Promise<NotificationItem[]> =>
  invoke('get_notifications');

export const clearNotifications = (): Promise<boolean> =>
  invoke('clear_notifications');

// ─────────────────────────────────────────────────────────
// Background Tasks
// ─────────────────────────────────────────────────────────

export const getBackgroundTasks = (): Promise<BackgroundTask[]> =>
  invoke('get_background_tasks');

// ─────────────────────────────────────────────────────────
// Memory KV Store
// ─────────────────────────────────────────────────────────

export const memorySet = (input: MemorySetInput): Promise<MemoryEntryResponse> =>
  invoke('memory_set', { input });

export const memoryGet = (input: MemoryGetInput): Promise<MemoryEntryResponse | null> =>
  invoke('memory_get', { input });

export const memoryDelete = (input: MemoryGetInput): Promise<boolean> =>
  invoke('memory_delete', { input });

export const memorySearch = (input: MemorySearchInput): Promise<MemoryEntryResponse[]> =>
  invoke('memory_search', { input });

export const memoryList = (input: MemoryListInput): Promise<MemoryEntryResponse[]> =>
  invoke('memory_list', { input });

export const memoryClearNamespace = (input: MemoryClearInput): Promise<MemoryClearResponse> =>
  invoke('memory_clear_namespace', { input });

export const memoryPruneExpired = (): Promise<MemoryClearResponse> =>
  invoke('memory_prune_expired');

export const memoryBulkImport = (input: MemoryBulkImportInput): Promise<MemoryBulkImportResponse> =>
  invoke('memory_bulk_import', { input });

export const memoryExport = (input: MemoryListInput): Promise<MemoryEntryResponse[]> =>
  invoke('memory_export', { input });

export const memoryListNamespaces = (spaceId?: string): Promise<string[]> =>
  invoke('memory_list_namespaces', { spaceId: spaceId ?? null });

// ─────────────────────────────────────────────────────────
// MCP Servers
// ─────────────────────────────────────────────────────────

export const listMcpServers = (): Promise<McpServerInfo[]> =>
  invoke('list_mcp_servers');

export const addMcpServer = (input: McpServerInput): Promise<McpServerInfo> =>
  invoke('add_mcp_server', { input });

export const updateMcpServer = (id: string, input: McpServerInput): Promise<McpServerInfo> =>
  invoke('update_mcp_server', { id, input });

export const removeMcpServer = (id: string): Promise<boolean> =>
  invoke('remove_mcp_server', { id });

export const toggleMcpServer = (id: string, enabled: boolean): Promise<boolean> =>
  invoke('toggle_mcp_server', { id, enabled });

export const connectMcpServer = (id: string): Promise<boolean> =>
  invoke('connect_mcp_server', { id });

export const disconnectMcpServer = (id: string): Promise<boolean> =>
  invoke('disconnect_mcp_server', { id });

export const restartMcpServer = (id: string): Promise<boolean> =>
  invoke('restart_mcp_server', { id });

export const listMcpTools = (): Promise<unknown[]> =>
  invoke('list_mcp_tools');

// MCP PR-2 — explicit tools/list refetch for a connected server. The
// notification-driven path isn't wired yet (PR-4); this button lets
// users pull in tools that the server registered after initial connect.
export const refreshMcpTools = (id: string): Promise<unknown[]> =>
  invoke('refresh_mcp_tools', { id });

// MCP PR-2 — JSON-RPC ping with elapsed-ms response. Used by the
// "Test Connection" affordance in the detail drawer to verify a
// server is alive without restarting it.
export const pingMcpServer = (id: string): Promise<number> =>
  invoke('ping_mcp_server', { id });

// MCP PR-5 — read the audit log. `serverId=null` returns all servers'
// rows, most recent first. Limit defaults to 100 server-side, capped
// at 1000.
export interface McpAuditEntry {
  id: string;
  serverId: string;
  eventKind: string;
  messageRedacted: string;
  createdAt: number;
}
export const listMcpAudit = (
  serverId: string | null,
  limit?: number,
): Promise<McpAuditEntry[]> =>
  invoke('list_mcp_audit', { serverId, limit });

// ─────────────────────────────────────────────────────────
// Built-in Skills
// ─────────────────────────────────────────────────────────

export const listSkills = (): Promise<SkillInfo[]> =>
  invoke('list_skills');

export const toggleSkill = (input: SkillToggleInput): Promise<boolean> =>
  invoke('toggle_skill', { input });

export const discoverSkills = (): Promise<SkillInfo[]> =>
  invoke('discover_skills');

export const reloadSkills = (): Promise<SkillInfo[]> =>
  invoke('reload_skills');

/** Fork a Bundled skill into the user's `~/.uclaw/skills/<name>/` so the
 *  user can edit it freely. The bundled original is left in place but
 *  shadowed by the user copy on the next discovery pass. Refused with
 *  an error if the skill is already in the User tier or if a fork
 *  already exists at the destination. */
export const forkSkillToUser = (name: string): Promise<string> =>
  invoke('fork_skill_to_user', { name });

/** Per-workspace skill tag scoping (V19+).
 *
 *  Empty list = "no filter" — workspace sees every enabled skill.
 *  Non-empty enables the manifest-side intersection filter: a skill
 *  is included iff its own tags overlap the workspace's tags OR the
 *  skill has no tags (untagged = global).
 *
 *  Tags are normalized at write time (trim + lowercase + dedup);
 *  `setWorkspaceSkillTags` returns the normalized list so you don't
 *  need to re-fetch. */
export const getWorkspaceSkillTags = (spaceId: string): Promise<string[]> =>
  invoke('get_workspace_skill_tags', { spaceId });

export const setWorkspaceSkillTags = (spaceId: string, tags: string[]): Promise<string[]> =>
  invoke('set_workspace_skill_tags', { spaceId, tags });

/** Compute the skills manifest that **would be** injected into the agent
 *  loop's system prompt right now. Powers the Settings → 内置技能 →
 *  活动技能 debug panel. Defaults to the same args the agent loop uses
 *  (space_id="default", max_entries=30, bias=Balanced). */
export const listActiveManifestSkills = (
  opts: { spaceId?: string; strategy?: string; maxEntries?: number } = {}
): Promise<import('./types').ActiveManifestSkill[]> =>
  invoke('list_active_manifest_skills', {
    spaceId: opts.spaceId,
    strategy: opts.strategy,
    maxEntries: opts.maxEntries,
  });

/** Returns every skill the user can `/<name>`-invoke from the composer:
 *  static + borrowed + learned. Empty `spaceId` falls back to "default".
 *  Powers the `/` autocomplete popup. */
export const listInvocableSkills = (
  spaceId?: string,
): Promise<import('./types').InvocableSkill[]> =>
  invoke('list_invocable_skills', { spaceId });

/** Search workspace + attached_dirs for files matching `query` (case-
 *  insensitive substring on file name only). Powers the `@` autocomplete
 *  popup. Heavy / VCS dirs (node_modules, .git, target, ...) are pruned
 *  server-side so this stays fast even in mono-repos. */
export const searchWorkspaceFilesForMention = (
  sessionId: string,
  query: string,
  limit?: number,
): Promise<import('./types').WorkspaceFileMatch[]> =>
  invoke('search_workspace_files_for_mention', { sessionId, query, limit });

export const getSkillDetail = (name: string): Promise<SkillDetailResponse> =>
  invoke('get_skill_detail', { name });

export const matchSkills = (input: SkillMatchInput): Promise<SkillMatchResult[]> =>
  invoke('match_skills', { input });

// ─────────────────────────────────────────────────────────
// Channels
// ─────────────────────────────────────────────────────────

export const listChannels = (): Promise<Channel[]> =>
  invoke('list_channels');

export const addChannel = (input: ChannelInput): Promise<Channel> =>
  invoke('add_channel', { input });

export const removeChannel = (id: string): Promise<boolean> =>
  invoke('remove_channel', { id });

export const toggleChannel = (id: string, enabled: boolean): Promise<boolean> =>
  invoke('toggle_channel', { id, enabled });

// ─────────────────────────────────────────────────────────
// Safety Policy
// ─────────────────────────────────────────────────────────

export const getSafetyPolicy = (): Promise<SafetyPolicyResponse> =>
  invoke('get_safety_policy');

export const setSafetyMode = (input: SetSafetyModeInput): Promise<SafetyPolicyResponse> =>
  invoke('set_safety_mode', { input });

export const setToolSafetyOverride = (input: SetToolOverrideInput): Promise<SafetyPolicyResponse> =>
  invoke('set_tool_safety_override', { input });

export const removeToolSafetyOverride = (input: ToolNameInput): Promise<SafetyPolicyResponse> =>
  invoke('remove_tool_safety_override', { input });

export const addAutoApprovedTool = (input: ToolNameInput): Promise<SafetyPolicyResponse> =>
  invoke('add_auto_approved_tool', { input });

export const removeAutoApprovedTool = (input: ToolNameInput): Promise<SafetyPolicyResponse> =>
  invoke('remove_auto_approved_tool', { input });

export const blockTool = (input: ToolNameInput): Promise<SafetyPolicyResponse> =>
  invoke('block_tool', { input });

export const unblockTool = (input: ToolNameInput): Promise<SafetyPolicyResponse> =>
  invoke('unblock_tool', { input });

export const assessCommandRisk = (input: AssessCommandInput): Promise<CommandRiskResponse> =>
  invoke('assess_command_risk', { input });

// ─────────────────────────────────────────────────────────
// Services / Memubot
// ─────────────────────────────────────────────────────────

export const servicesHealth = (): Promise<unknown> =>
  invoke('services_health');

export const memorizationStatus = (): Promise<unknown> =>
  invoke('memorization_status');

export const proactiveStatus = (): Promise<unknown> =>
  invoke('proactive_status');

export const proactiveStart = (): Promise<void> =>
  invoke('proactive_start');

export const proactiveStop = (): Promise<void> =>
  invoke('proactive_stop');

export const metricsSummary = (): Promise<unknown> =>
  invoke('metrics_summary');

export const memubotConfigGet = (): Promise<unknown> =>
  invoke('memubot_config_get');

// ─────────────────────────────────────────────────────────
// Dev / Testing
// ─────────────────────────────────────────────────────────

export const triggerProactiveScenario = (scenarioName: string): Promise<unknown> =>
  invoke('trigger_proactive_scenario', { scenarioName });

// ─────────────────────────────────────────────────────────
// Event Listeners (Tauri global events)
// ─────────────────────────────────────────────────────────

// — Streaming events —

export const onTextDelta = (cb: (payload: StreamTextDelta) => void): Promise<UnlistenFn> =>
  listen('agent:text-delta', (e) => cb(e.payload as StreamTextDelta));

export const onToolStart = (cb: (payload: StreamToolStart) => void): Promise<UnlistenFn> =>
  listen('agent:tool-start', (e) => cb(e.payload as StreamToolStart));

export const onToolResult = (cb: (payload: StreamToolResult) => void): Promise<UnlistenFn> =>
  listen('agent:tool-result', (e) => cb(e.payload as StreamToolResult));

export const onDone = (cb: (payload: StreamDone) => void): Promise<UnlistenFn> =>
  listen('agent:done', (e) => cb(e.payload as StreamDone));

export const onError = (cb: (payload: StreamError) => void): Promise<UnlistenFn> =>
  listen('agent:error', (e) => cb(e.payload as StreamError));

export const onThinking = (cb: (payload: StreamThinking) => void): Promise<UnlistenFn> =>
  listen('agent:thinking', (e) => cb(e.payload as StreamThinking));

export const onThinkingDone = (cb: (payload: StreamThinkingDone) => void): Promise<UnlistenFn> =>
  listen('agent:thinking-done', (e) => cb(e.payload as StreamThinkingDone));

export const onTurnCost = (cb: (payload: TurnCost) => void): Promise<UnlistenFn> =>
  listen('agent:turn_cost', (e) => cb(e.payload as TurnCost));

// — Cost dashboard —

export const getDailyCosts = (daysBack = 30): Promise<DailyCostRollup[]> =>
  invoke<DailyCostRollup[]>('get_daily_costs', { daysBack });

export const getModelCosts = (daysBack = 30): Promise<ModelCostRollup[]> =>
  invoke<ModelCostRollup[]>('get_model_costs', { daysBack });

export const getSessionCosts = (daysBack = 30, limit = 50): Promise<SessionCostRollup[]> =>
  invoke<SessionCostRollup[]>('get_session_costs', { daysBack, limit });

export const listWorkspaceCostRollup = (sinceMs: number): Promise<WorkspaceCostRollup[]> =>
  invoke<WorkspaceCostRollup[]>('list_workspace_cost_rollup', { sinceMs });

export const getMonthCostTotal = (sinceMs: number): Promise<number> =>
  invoke<number>('get_month_cost_total', { sinceMs });

export const onBudgetThreshold = (cb: (payload: BudgetThresholdPayload) => void): Promise<UnlistenFn> =>
  listen('budget:threshold', (e) => cb(e.payload as BudgetThresholdPayload));

export const onContextStats = (cb: (payload: ContextStats) => void): Promise<UnlistenFn> =>
  listen('agent:context_stats', (e) => cb(e.payload as ContextStats));

// — System events —

export const onNeedApproval = (cb: (payload: ApprovalRequest) => void): Promise<UnlistenFn> =>
  listen('agent:need_approval', (e) => cb(e.payload as ApprovalRequest));

export const onArtifactTreeUpdate = (cb: (payload: unknown) => void): Promise<UnlistenFn> =>
  listen('artifact:tree_update', (e) => cb(e.payload));

// — Reflection events —

export const onReflectionUpdate = (cb: (payload: unknown) => void): Promise<UnlistenFn> =>
  listen('agent:reflection-update', (e) => cb(e.payload));

export const onProactiveLearning = (cb: (payload: unknown) => void): Promise<UnlistenFn> =>
  listen('agent:proactive-learning', (e) => cb(e.payload));

// ─────────────────────────────────────────────────────────
// Proma 兼容桩函数
// ─────────────────────────────────────────────────────────
// 以下函数为 Proma 迁移组件提供兼容性 API。
// 对于尚未在 Tauri 后端实现的命令，返回合理的桩数据。
// 随着后端逐步补全，桩函数将被替换为真实的 invoke 调用。

/* eslint-disable @typescript-eslint/no-explicit-any */

// --- Settings compat ---
export const updateSettings = (_patch: any): Promise<void> =>
  patchSettings(_patch).then(() => {})

// --- Conversation compat ---
export const updateConversationTitle = (id: string, title: string): Promise<any> =>
  invoke('update_conversation_title', { id, title }).catch(() => ({ id, title, updatedAt: Date.now() }))

export const togglePinConversation = (id: string): Promise<any> =>
  invoke('toggle_pin_conversation', { id }).catch(() => ({ id, pinned: true, updatedAt: Date.now() }))

export const updateConversationModel = (conversationId: string, modelId: string, channelId: string): Promise<any> =>
  invoke('update_conversation_model', { conversationId, modelId, channelId }).catch(() => ({ id: conversationId, modelId, channelId, updatedAt: Date.now() }))

export const getConversationMessages = (conversationId: string): Promise<any[]> =>
  getMessages({ conversationId }).then((msgs) => msgs as any[])

export const getRecentMessages = (conversationId: string, limit: number): Promise<{ messages: any[]; hasMore: boolean }> =>
  invoke<{ messages: any[]; hasMore: boolean }>('get_recent_messages', { conversationId, limit }).catch(() => getMessages({ conversationId }).then((msgs) => ({ messages: msgs as any[], hasMore: false })))

export const updateContextDividers = (conversationId: string, dividers: string[]): Promise<void> =>
  invoke<void>('update_context_dividers', { conversationId, dividers }).catch(() => {})

export const stopGeneration = (conversationId: string): Promise<void> =>
  invoke<void>('stop_generation', { conversationId }).catch(() => {})

export const truncateMessagesFrom = (conversationId: string, messageId: string, preserveFirstMessageAttachments?: boolean): Promise<any[]> =>
  invoke<any[]>('truncate_messages_from', { conversationId, messageId, preserveFirstMessageAttachments: preserveFirstMessageAttachments ?? false }).catch(() => [])

export const deleteMessage = (conversationId: string, messageId: string): Promise<any[]> =>
  invoke<any[]>('delete_message', { conversationId, messageId }).catch(() => [])

export const deleteAttachment = (localPath: string): Promise<void> =>
  invoke<void>('delete_attachment', { localPath }).catch(() => {})

export const saveAttachment = (input: any): Promise<any> =>
  invoke('save_attachment', { input })

export const readAttachment = (localPath: string): Promise<string> =>
  invoke<string>('read_attachment', { localPath })

export const getSystemPromptConfig = (): Promise<any> =>
  invoke('get_system_prompt_config').catch(() => ({ prompts: [] }))

export const generateTitle = (input: any): Promise<string> =>
  invoke<string>('generate_title', { input }).catch(() => '')

// --- File dialogs ---
export const openFileDialog = (): Promise<{ files: any[] }> =>
  invoke<{ files: any[] }>('open_file_dialog').catch(() => ({ files: [] }))


// --- Agent session compat ---
export const listAgentSessions = (): Promise<any[]> =>
  invoke<any[]>('list_agent_sessions').catch(() => [])

/**
 * One chat thread bound to a spec (Phase 2b cluster A).
 *
 * `identityKey` shape:
 *   - "local"                    → spec owner's local chat thread
 *   - "app-chat:{specId}:{channelType}:{chatId}" → per-IM-user thread
 *   - "{channelType}:{chatId}"   → legacy per-IM-user thread
 */
export interface ChatSessionSummary {
  identityKey: string
  agentSessionId: string
  title: string
  messageCount: number
  updatedAt: number
}

/** List all chat threads for the given spec. Sorted most-recent-first. */
export const listChatSessionsForSpec = (specId: string): Promise<ChatSessionSummary[]> =>
  invoke<Array<{
    identity_key: string
    agent_session_id: string
    title: string
    message_count: number
    updated_at: number
  }>>('list_chat_sessions_for_spec', { specId })
    .then((rows) => rows.map((r) => ({
      identityKey: r.identity_key,
      agentSessionId: r.agent_session_id,
      title: r.title,
      messageCount: r.message_count,
      updatedAt: r.updated_at,
    })))
    .catch(() => [])

export const createAgentSession = (title?: string, channelId?: string, workspaceId?: string): Promise<any> =>
  invoke('create_agent_session', { title: title ?? null, channelId: channelId ?? null, workspaceId: workspaceId ?? null })
    .catch(() => ({ id: crypto.randomUUID(), title: title || '新会话', createdAt: Date.now(), updatedAt: Date.now() }))

/** Estimate context token usage for a session from persisted messages.
 *  Mirrors openhanako's `context_usage` WS request — lets the frontend
 *  initialize ContextUsageBadge immediately on session load/switch without
 *  waiting for a full LLM round-trip. (P0 fix: 2026-05-16) */
export const estimateSessionContext = (sessionId: string): Promise<{
  sessionId: string
  inputTokens: number
  contextWindow: number
  systemPromptTokens: number
  messagesTokens: number
  toolUseTokens: number
  compactBufferTokens: number
  freeTokens: number
} | null> =>
  invoke<{
    sessionId: string
    inputTokens: number
    contextWindow: number
    systemPromptTokens: number
    messagesTokens: number
    toolUseTokens: number
    compactBufferTokens: number
    freeTokens: number
  }>('estimate_session_context', { sessionId }).catch(() => null)

export const getAgentSessionMessages = (sessionId: string): Promise<any[]> =>
  invoke<any[]>('get_agent_session_messages', { sessionId }).catch(() => [])

export interface HomeThreadSession {
  id: string
  workspaceId: string
  title: string
  messageCount: number
  pinned: boolean
  archived: boolean
  createdAt: number
  updatedAt: number
}

export const getOrCreateSpecHomeThread = (specId: string): Promise<HomeThreadSession> =>
  invoke<HomeThreadSession>('get_or_create_spec_home_thread', { specId })

export const sendAgentMessage = (input: any): Promise<void> => {
  return invoke<void>('send_agent_message', { input: {
    sessionId: input.sessionId ?? input.conversationId ?? '',
    userMessage: input.userMessage ?? input.content ?? '',
    channelId: input.channelId ?? null,
    modelId: input.modelId ?? null,
    workspaceId: input.workspaceId ?? null,
    strategy: input.strategy ?? null,
    promptId: input.promptId ?? null,
  }})
}

export const stopAgent = (sessionId: string): Promise<void> =>
  invoke<void>('stop_agent', { sessionId }).catch(() => {})

export const queueAgentMessage = (input: any): Promise<void> =>
  invoke<void>('queue_agent_message', { input })

export const agentSteer = (input: { sessionId: string; userMessage: string; uuid?: string }): Promise<void> =>
  invoke<void>('agent_steer', { input: { session_id: input.sessionId, user_message: input.userMessage, uuid: input.uuid } })

export const agentFollowUp = (input: { sessionId: string; userMessage: string; uuid?: string }): Promise<void> =>
  invoke<void>('agent_follow_up', { input: { session_id: input.sessionId, user_message: input.userMessage, uuid: input.uuid } })

export const migrateChatToAgent = (conversationId: string, sessionId: string): Promise<void> =>
  invoke<void>('migrate_chat_to_agent', { conversationId, sessionId }).catch(() => {})

export const forkAgentSession = (input: { sessionId: string; upToMessageUuid: string }): Promise<any> =>
  invoke('fork_agent_session', { input })

export const rewindSession = (input: { sessionId: string; assistantMessageUuid: string }): Promise<any> =>
  invoke('rewind_session', { input })

export const getAgentSessionPath = (workspaceId: string, sessionId: string): Promise<string> =>
  invoke<string>('get_agent_session_path', { workspaceId, sessionId }).catch(() => '')

export const saveFilesToAgentSession = (input: any): Promise<any[]> =>
  invoke<any[]>('save_files_to_agent_session', { input }).catch(() => [])

// --- Workspace / directory compat ---
export const getWorkspaceCapabilities = (slug: string): Promise<{ mcpServers: any[]; skills: any[] }> =>
  invoke<{ mcpServers: any[]; skills: any[] }>('get_workspace_capabilities', { slug }).catch(() => ({ mcpServers: [], skills: [] }))



export const saveImageAs = (path: string, filename: string): Promise<void> =>
  invoke<void>('save_image_as', { path, filename }).catch(() => {})

// --- File / dialog actions (Tauri plugin-backed) ---

export const openFolderDialog = async (): Promise<{ path: string; name: string } | null> => {
  const selected = await openDialog({ directory: true, multiple: false })
  if (!selected || typeof selected !== 'string') return null
  const name = selected.split('/').pop() ?? selected
  return { path: selected, name }
}

export const openFile = (path: string): Promise<void> => openShell(path)

export const openExternal = (url: string): Promise<void> => openShell(url)

export const showInFinder = (path: string): Promise<void> => {
  // tauri-plugin-shell v2 doesn't expose `open -R` (reveal-in-Finder).
  // Open the path directly: macOS `open <path>` resolves to:
  //   - directory → opens in Finder ✓ (the workspace-folder use case)
  //   - file      → opens in its default app (best we can do without -R)
  // Previously this returned `openShell(parent)` which opened the
  // grandparent when the caller passed a workspace folder — wrong UX.
  return openShell(path)
}

export const getPathForFile = (_file: File): string | null => {
  // Electron 的 webUtils.getPathForFile 无法在 Tauri 中使用
  // 返回 null 使调用方走 fallback 路径
  return null
}

export const checkPathsType = (paths: string[]): Promise<{ directories: string[]; files: string[] }> =>
  invoke<{ directories: string[]; files: string[] }>('check_paths_type', { paths }).catch(() => ({ directories: [], files: paths }))

// --- Safety mode quick-toggle ---
//
// The previous `getPermissionMode` / `setPermissionMode` wrappers called
// Tauri commands that never existed and silenced errors via `.catch()`, so
// the input-bar selector visibly cycled but the backend never received the
// value. Replaced by the existing `getSafetyPolicy` / `setSafetyMode` (above)
// which talk to the real SafetyManager.
//
// `SafetyModeWire` mirrors the Rust enum's serde shape
// (`#[serde(rename_all = "lowercase")]` in `src-tauri/src/safety/mod.rs:11`).

export type SafetyModeWire = 'ask' | 'acceptedits' | 'plan' | 'supervised' | 'yolo'

// --- System prompt compat ---
export const createSystemPrompt = (input: any): Promise<any> =>
  invoke('create_system_prompt', { input }).catch(() => ({ id: crypto.randomUUID(), name: input?.name ?? '', content: input?.content ?? '', isBuiltin: false }))

export const deleteSystemPrompt = (id: string): Promise<void> =>
  invoke<void>('delete_system_prompt', { id }).catch(() => {})

export const setDefaultPrompt = (id: string): Promise<void> =>
  invoke<void>('set_default_prompt', { id }).catch(() => {})

export const updateSystemPrompt = (id: string, input: any): Promise<any> =>
  invoke('update_system_prompt', { id, input }).catch(() => ({ id, ...input }))

export const updateAppendSetting = (enabled: boolean): Promise<void> =>
  invoke<void>('update_append_setting', { enabled }).catch(() => {})

export const getSystemPromptVersions = (promptId: string): Promise<any[]> =>
  invoke<any[]>('get_system_prompt_versions', { promptId }).catch(() => [])

// --- Chat tools compat ---
export const updateChatToolState = (toolId: string, patch: any): Promise<void> =>
  invoke<void>('update_chat_tool_state', { toolId, patch }).catch(() => {})

export const getChatTools = (): Promise<any[]> =>
  invoke<any[]>('get_chat_tools').catch(() => [])

// --- Feishu compat ---
export const setFeishuSessionNotify = (sessionId: string, mode: string): Promise<void> =>
  invoke<void>('set_feishu_session_notify', { sessionId, mode }).catch(() => {})


// --- Agent permission / ask-user / exit-plan compat ---
export const respondPermission = (input: any): Promise<void> =>
  invoke<void>('respond_permission', { input }).catch(() => {})

export const respondAskUser = (input: { requestId: string; answers: Record<string, string> }): Promise<void> =>
  invoke<void>('respond_ask_user', { input })

export const onAskUserRequest = (cb: (payload: AskUserRequest) => void): Promise<UnlistenFn> =>
  listen('agent:ask_user_request', (e) => cb(e.payload as AskUserRequest))

export interface RespondExitPlanModeInput {
  requestId: string
  decision: 'accept_and_auto' | 'accept_keep_plan' | 'reject'
  feedback?: string
  allowedPrompts?: string[]
  sessionId: string
}

export const respondExitPlanMode = (input: RespondExitPlanModeInput): Promise<void> =>
  invoke<void>('respond_exit_plan_mode', { input })

export const onExitPlanRequest = (cb: (payload: ExitPlanModeRequest) => void): Promise<UnlistenFn> =>
  listen('agent:exit_plan_request', (e) => cb(e.payload as ExitPlanModeRequest))

// --- Agent session management compat ---
export const moveAgentSessionToWorkspace = (input: any): Promise<any> =>
  invoke('move_agent_session_to_workspace', { input })

/**
 * Delete an agent session and its derived rows. Surfaces backend errors —
 * the previous `.catch(() => {})` silenced "command not found" so the
 * delete UI optimistically closed the tab while the session lingered in
 * the DB. Callers should toast on rejection. Returns true when a row
 * was actually deleted.
 */
export const deleteAgentSession = (id: string): Promise<boolean> =>
  invoke<boolean>('delete_agent_session', { id })

export const updateAgentSessionTitle = (id: string, title: string): Promise<any> =>
  invoke('update_agent_session_title', { id, title }).catch(() => ({ id, title, updatedAt: Date.now() }))

/**
 * Toggle pin state on an agent session. Returns the new pinnedAt:
 * number (ms) when pinned, null when unpinned. Surfaces backend errors —
 * the previous `.catch(() => fake)` masked a missing Tauri command and
 * pretended every toggle succeeded. The Rust command now exists
 * (Phase 6-A Task 2).
 */
export const togglePinAgentSession = (id: string): Promise<number | null> =>
  invoke<number | null>('toggle_pin_agent_session', { id })

export const toggleManualWorkingAgentSession = (id: string): Promise<any> =>
  invoke('toggle_manual_working_agent_session', { id }).catch(() => ({ id, manualWorking: true, updatedAt: Date.now() }))

export const toggleArchiveAgentSession = (id: string): Promise<number | null> =>
  invoke<number | null>('toggle_archive_agent_session', { id })

export const toggleArchiveConversation = (id: string): Promise<number | null> =>
  invoke<number | null>('toggle_archive_conversation', { id })

// --- User profile compat ---
export const getUserProfile = (): Promise<any> =>
  invoke('get_user_profile').catch(() => ({ userName: 'User', avatar: '' }))

// --- Search compat ---
export const searchConversationMessages = (query: string): Promise<any[]> =>
  invoke<any[]>('search_conversation_messages', { query }).catch(() => [])

export const searchAgentSessionMessages = (query: string): Promise<any[]> =>
  invoke<any[]>('search_agent_session_messages', { query }).catch(() => [])

// --- Chat stream event listeners (Proma-style synchronous unlisten) ---
type CleanupFn = () => void

// Helper: register a Tauri event listener and return a synchronous cleanup.
// React StrictMode fires effects twice: cleanup runs *before* the listen() Promise
// resolves, so a plain `let unlisten = null` guard leaves the first listener alive.
// The `cancelled` flag closes this gap — if cleanup runs first, the unlisten fn is
// called immediately when the Promise eventually settles.
function makeListener(event: string, cb: (payload: any) => void): CleanupFn {
  let cancelled = false
  let unlisten: (() => void) | null = null
  listen(event, (e) => cb(e.payload)).then((fn) => {
    if (cancelled) {
      fn() // cleanup already ran — unlisten immediately
    } else {
      unlisten = fn
    }
  }).catch(console.error)
  return () => {
    cancelled = true
    unlisten?.()
  }
}

export const onStreamChunk = (cb: (event: any) => void): CleanupFn =>
  makeListener('chat:stream-chunk', cb)

export const onStreamReasoning = (cb: (event: any) => void): CleanupFn =>
  makeListener('chat:stream-reasoning', cb)

export const onStreamComplete = (cb: (event: any) => void): CleanupFn =>
  makeListener('chat:stream-complete', cb)

export const onStreamError = (cb: (event: any) => void): CleanupFn =>
  makeListener('chat:stream-error', cb)

export const onStreamToolActivity = (cb: (event: any) => void): CleanupFn =>
  makeListener('chat:stream-tool-activity', cb)

export const onQueuedConsumed = (
  handler: (payload: { sessionId: string; uuid: string }) => void,
): CleanupFn =>
  makeListener('agent:queued-consumed', (payload: { sessionId: string; uuid: string }) => handler(payload))

/* eslint-enable @typescript-eslint/no-explicit-any */

// ─── Agent Teams ──────────────────────────────────────────────────────
export interface TeamChannelMessage {
  id: string
  fromRole: string
  toRole: string | null
  message: string
  createdAt: number
}

export const startAgentTeams = (sessionId: string, task: string, maxReviewCycles?: number): Promise<string> =>
  invoke<string>('start_agent_teams', { input: { sessionId, task, maxReviewCycles } })

export const getTeamChannel = (teamId: string): Promise<TeamChannelMessage[]> =>
  invoke<TeamChannelMessage[]>('get_team_channel', { teamId })

export const stopAgentTeams = (teamId: string): Promise<void> =>
  invoke('stop_agent_teams', { teamId })

// ─── Browser (v2) ────────────────────────────────────────────────────

export interface BrowserStateResponse {
  running: boolean
  tabs: { tabId: string; url: string; title: string }[]
  activeTabId: string | null
}

export interface ScreencastFramePayload {
  sessionId: string
  tabId: string
  dataB64: string
  pageWidth: number
  pageHeight: number
}

export interface DOMElementResponse {
  index: number
  tag: string
  text: string
  isInViewport: boolean
  xpath: string
  boundingBox?: { x: number; y: number; width: number; height: number }
}

export interface BrowserTabResponse {
  tabId: string
  url: string
  title: string
  active: boolean
}

export interface DOMStateResponse {
  url: string
  title: string
  elements: DOMElementResponse[]
  pageText: string
  tabs: BrowserTabResponse[]
}

// ── Legacy commands (backward compat) ────────────────────────────────

export const browserGetState = (): Promise<BrowserStateResponse> =>
  invoke<BrowserStateResponse>('browser_get_state')

export const browserLaunch = (): Promise<boolean> =>
  invoke<boolean>('browser_launch')

export const browserShutdown = (): Promise<boolean> =>
  invoke<boolean>('browser_shutdown')

export const browserTakeScreenshot = (tabId: string): Promise<string> =>
  invoke<string>('browser_take_screenshot', { tabId })

// ── v2 session commands ───────────────────────────────────────────────

export const browserListSessions = (): Promise<string[]> =>
  invoke<string[]>('browser_list_sessions')

export const browserDestroySession = (sessionId: string): Promise<void> =>
  invoke<void>('browser_destroy_session', { sessionId })

// ── v2 UI control commands ────────────────────────────────────────────

export const browserUINavigate = (sessionId: string, tabId: string, url: string): Promise<string> =>
  invoke<string>('browser_ui_navigate', { sessionId, tabId, url })

export const browserUIGoBack = (sessionId: string, tabId: string): Promise<void> =>
  invoke<void>('browser_ui_go_back', { sessionId, tabId })

export const browserUIGoForward = (sessionId: string, tabId: string): Promise<void> =>
  invoke<void>('browser_ui_go_forward', { sessionId, tabId })

export const browserUISwitchTab = (sessionId: string, tabId: string): Promise<void> =>
  invoke<void>('browser_ui_switch_tab', { sessionId, tabId })

export const browserUIReload = (sessionId: string, tabId: string): Promise<void> =>
  invoke<void>('browser_ui_reload', { sessionId, tabId })

export const browserUICloseTab = (sessionId: string, tabId: string): Promise<void> =>
  invoke<void>('browser_ui_close_tab', { sessionId, tabId })

export const browserUIClick = (sessionId: string, tabId: string, x: number, y: number): Promise<void> =>
  invoke<void>('browser_ui_click', { sessionId, tabId, x, y })

export type BrowserUIMouseEventType = 'mousePressed' | 'mouseMoved' | 'mouseReleased'

export const browserUIMouseEvent = (
  sessionId: string,
  tabId: string,
  eventType: BrowserUIMouseEventType,
  x: number,
  y: number,
): Promise<void> =>
  invoke<void>('browser_ui_mouse_event', { sessionId, tabId, eventType, x, y })

export interface BrowserLoginCompletionPayload {
  specId: string
  label: string
  url: string
  profileId: string
  status: 'live'
  completedAt: number
}

export interface BrowserLoginCompletionProbe {
  completed: boolean
  payload?: BrowserLoginCompletionPayload
  message?: string
}

export const browserUICompleteLogin = (
  sessionId: string,
  tabId: string,
  specId: string,
  label: string,
  url: string,
): Promise<BrowserLoginCompletionProbe> =>
  invoke<BrowserLoginCompletionProbe>('browser_ui_complete_login', { sessionId, tabId, specId, label, url })

export const browserWebviewCompleteLogin = (
  webviewLabel: string,
  specId: string,
  label: string,
  url: string,
): Promise<BrowserLoginCompletionProbe> =>
  invoke<BrowserLoginCompletionProbe>('browser_webview_complete_login', { webviewLabel, specId, label, url })

export const browserGetDOMState = (sessionId: string, tabId: string): Promise<DOMStateResponse> =>
  invoke<DOMStateResponse>('browser_get_dom_state', { sessionId, tabId })

export const browserStartScreencast = (sessionId: string, tabId: string): Promise<void> =>
  invoke<void>('browser_start_screencast', { sessionId, tabId })

export const browserCaptureScreenshot = (sessionId: string, tabId: string): Promise<string> =>
  invoke<string>('browser_capture_screenshot', { sessionId, tabId })

export const browserStopScreencast = (sessionId: string, tabId: string): Promise<void> =>
  invoke<void>('browser_stop_screencast', { sessionId, tabId })

/** Subscribe to CDP screencast frames. Returns an unlisten function. */
export const listenScreencastFrames = (
  handler: (payload: ScreencastFramePayload) => void,
): Promise<UnlistenFn> =>
  listen<ScreencastFramePayload>('browser:screencast-frame', ({ payload }) => handler(payload))

export interface NavStatePayload {
  sessionId: string
  tabId: string
  url: string
  title: string
  isLoading: boolean
  canGoBack: boolean
  canGoForward: boolean
}

/** Subscribe to browser:nav-state events. Returns an unlisten function. */
export const listenNavState = (
  handler: (payload: NavStatePayload) => void,
): Promise<UnlistenFn> =>
  listen<NavStatePayload>('browser:nav-state', ({ payload }) => handler(payload))

export const listenAutomationBrowserLoginCompleted = (
  handler: (payload: BrowserLoginCompletionPayload) => void,
): Promise<UnlistenFn> =>
  listen<BrowserLoginCompletionPayload>('automation:browser-login-completed', ({ payload }) => handler(payload))

export type BrowserTaskStatus =
  | 'running'
  | 'completed'
  | 'failed'
  | 'stopped'
  | 'needs_user_intervention'
  | 'paused_waiting_for_browser_runtime'
  | 'paused_checkpointed'
export type BrowserTaskStepPhase = 'observe' | 'decide' | 'act' | 'recover' | 'user_intervention' | 'done'

export interface BrowserTaskStepPayload {
  stepIndex: number
  phase: BrowserTaskStepPhase
  observationSummary: string
  reasoning: string
  actionName: string
  actionArgs: unknown
  ok: boolean
  message: string | null
  error: string | null
  timestampMs: number
}

export interface BrowserTaskRunPayload {
  runId: string
  sessionId: string
  task: string
  status: BrowserTaskStatus
  steps: BrowserTaskStepPayload[]
}

export interface BrowserTaskStepEventPayload {
  runId: string
  sessionId: string
  status: BrowserTaskStatus
  step: BrowserTaskStepPayload
}

export const listenBrowserTaskRun = (
  handler: (payload: BrowserTaskRunPayload) => void,
): Promise<UnlistenFn> =>
  listen<BrowserTaskRunPayload>('browser:task-run', ({ payload }) => handler(payload))

export const listenBrowserTaskStep = (
  handler: (payload: BrowserTaskStepEventPayload) => void,
): Promise<UnlistenFn> =>
  listen<BrowserTaskStepEventPayload>('browser:task-step', ({ payload }) => handler(payload))

// ─── Automation (Phase 3) ─────────────────────────────────────────────
export interface AutomationActivity {
  id: string
  specId: string
  subscriptionId: string | null
  triggerSourceType: 'schedule' | 'file' | 'webhook' | 'webpage' | 'rss' | 'wecom' | 'custom' | 'manual' | string
  triggerPayloadJson: string
  status: 'queued' | 'running' | 'completed' | 'failed' | 'cancelled' | 'waiting_user' | 'filtered_out' | 'deferred_phase_2' | string
  errorText: string | null
  queuedAt: number
  startedAt: number | null
  completedAt: number | null
  durationMs: number
  llmIterations: number
  llmTokensIn: number
  llmTokensOut: number
  sessionId: string | null
  reportArtifactsJson: string
  reportText: string | null
  reportOutcome: string | null
  escalationId: string | null
  resumedFromActivityId: string | null
  resumedFromEscalationId: string | null
  workingDir: string
}

export const getAutomationActivity = (specId: string, limit?: number): Promise<AutomationActivity[]> =>
  invoke<AutomationActivity[]>('get_automation_activity', { specId, limit })

// ─── Humane Automation (Phase 1 spec § 7.3) ──────────────────────────

/** Typed row from the V20 automation_specs table. */
export interface HumaneSpecRow {
  id: string
  name: string
  version: string
  author: string
  description: string
  systemPrompt: string
  specFormat: string
  specYaml: string
  specJson: string
  userConfigValues: string
  permissionsGranted: string
  permissionsDenied: string
  status: 'active' | 'paused' | 'error' | 'uninstalled' | string
  enabled: boolean
  spaceId: string | null
  source: string
  sourceRef: string | null
  sourceVersion: string | null
  createdAt: number
  updatedAt: number
  lastRunAt: number | null
  lastRunOutcome: string | null
  triggerPhrase: string
  systemPromptOverride: string
}

/** Typed row from the V21 automation_escalations table. */
export interface EscalationRow {
  id: string
  specId: string
  activityId: string
  question: string
  choicesJson: string
  status: 'waiting' | 'resolved' | string
  userChoice: string | null
  userNote: string | null
  createdAt: number
  respondedAt: number | null
}

/** Upgraded: returns HumaneSpecRow[] (V20 schema). */
export const listAutomationsHumane = (): Promise<HumaneSpecRow[]> =>
  invoke<HumaneSpecRow[]>('list_automations')

export const installHumaneSpec = (yaml: string, sourceRef?: string): Promise<HumaneSpecRow> =>
  invoke<HumaneSpecRow>('install_humane_spec', { yaml, sourceRef })

export const importHumaneSpecFile = (path: string): Promise<HumaneSpecRow> =>
  invoke<HumaneSpecRow>('import_humane_spec_file', { path })

export const getAutomationSpec = (specId: string): Promise<HumaneSpecRow> =>
  invoke<HumaneSpecRow>('get_automation_spec', { specId })

export const updateAutomationUserConfig = (specId: string, values: Record<string, unknown>): Promise<void> =>
  invoke<void>('update_user_config', { specId, values })

export const setAutomationPermission = (specId: string, permission: string, granted: boolean): Promise<void> =>
  invoke<void>('set_automation_permission', { specId, permission, granted })

export const setAutomationEnabled = (specId: string, enabled: boolean): Promise<void> =>
  invoke<void>('set_automation_enabled', { specId, enabled })

export const uninstallAutomation = (specId: string): Promise<void> =>
  invoke<void>('uninstall_automation', { specId })

/** Upgraded: delegates to AppRuntimeService.execute_run. */
export const triggerAutomationManualHumane = (specId: string): Promise<void> =>
  invoke<void>('trigger_automation_manual', { specId })

export const stopAutomationRuns = (specId: string): Promise<number> =>
  invoke<number>('stop_automation_runs', { specId })

export const resolveEscalation = (escalationId: string, choice: string, note?: string): Promise<void> =>
  invoke<void>('resolve_escalation', { escalationId, choice, note })

export const listPendingEscalations = (specId?: string): Promise<EscalationRow[]> =>
  invoke<EscalationRow[]>('list_pending_escalations', { specId })

export const readAutomationMemory = (specId: string): Promise<string> =>
  invoke<string>('read_automation_memory', { specId })

export const compactAutomationMemory = (specId: string): Promise<string> =>
  invoke<string>('compact_automation_memory', { specId })

export interface SpecChannelBinding {
  specId: string
  channelInstanceId: string
  enabled: boolean
  channelName?: string
  channelType?: string
}

export const listSpecChannelBindings = (specId: string): Promise<SpecChannelBinding[]> =>
  invoke<SpecChannelBinding[]>('list_spec_channel_bindings', { specId })

export const updateSpecChannelBindings = (
  specId: string,
  bindings: SpecChannelBinding[]
): Promise<void> =>
  invoke<void>('update_spec_channel_bindings', { specId, bindings })

export const updateSpecImSettings = (
  specId: string,
  triggerPhrase: string | null,
  systemPromptOverride: string | null
): Promise<void> =>
  invoke<void>('update_spec_im_settings', { specId, triggerPhrase, systemPromptOverride })

// ─── Marketplace ──────────────────────────────────────────────────────

export interface EntryI18n {
  name?: string | null
  description?: string | null
  // serde serializes this as snake_case inside HashMap values (not top-level camelCase)
  system_prompt?: string | null
}

export interface MarketplaceItem {
  slug: string
  name: string
  version: string
  author: string
  description: string
  appType: string
  category: string
  icon: string | null
  tags: string[]
  sizeBytes: number | null
  minAppVersion: string | null
  locale: string | null
  i18n: Record<string, EntryI18n>
}

export interface MarketplaceQueryResult {
  items: MarketplaceItem[]
  total: number
  hasMore: boolean
}

export interface MarketplaceDetail {
  item: MarketplaceItem
  specYaml: string
  parsedSpecJson: unknown | null
  requiresMcps: string[]
  requiresSkills: string[]
  installedVersion: string | null
}

export interface MarketplaceUpdate {
  slug: string
  installedVersion: string
  latestVersion: string
}

export interface MarketplaceInstallProgress {
  phase: 'fetching_spec' | 'parsing' | 'installing' | 'activating' | 'complete' | string
  slug: string
  percent: number
  message: string | null
}

export const queryMarketplace = (
  search?: string,
  itemType?: string,
  category?: string,
  page?: number,
  pageSize?: number,
): Promise<MarketplaceQueryResult> =>
  invoke<MarketplaceQueryResult>('query_marketplace', {
    search, itemType, category, page, pageSize,
  })

export const getMarketplaceDetail = (slug: string): Promise<MarketplaceDetail> =>
  invoke<MarketplaceDetail>('get_marketplace_detail', { slug })

export const checkMarketplaceUpdates = (): Promise<MarketplaceUpdate[]> =>
  invoke<MarketplaceUpdate[]>('check_marketplace_updates')

export const refreshMarketplace = (): Promise<number> =>
  invoke<number>('refresh_marketplace')

// Discriminated union matching the Rust InstallOutcome enum
// (serde tag = "kind", rename_all = "camelCase").
export type InstallOutcome =
  | { kind: 'automation'; spec: HumaneSpecRow }
  | { kind: 'skill'; slug: string; installPath: string }
  | { kind: 'mcp'; slug: string; mcpServerId: string }

export const installMarketplaceHuman = (
  slug: string,
  spaceId?: string,
  userConfig?: Record<string, unknown>,
  progressChannel?: string,
): Promise<InstallOutcome> =>
  invoke<InstallOutcome>('install_marketplace_human', {
    slug, spaceId, userConfig, progressChannel,
  })

export interface StandaloneInstall {
  slug: string
  itemType: string // 'skill' | 'mcp'
  version: string
  installedAt: number
  mcpServerId: string | null
}

export const listStandaloneInstalls = (): Promise<StandaloneInstall[]> =>
  invoke<StandaloneInstall[]>('list_standalone_installs')

export const uninstallMarketplaceHuman = (slug: string): Promise<void> =>
  invoke<void>('uninstall_marketplace_human', { slug })

export const marketplaceCategoryCounts = (
  itemType?: string,
  search?: string,
): Promise<Record<string, number>> =>
  invoke<Record<string, number>>('marketplace_category_counts', { itemType, search })

// Deprecated — kept until Phase 3b removes. New code uses queryMarketplace + filter('automation')
export const listMarketplaceHumans = (registryUrl?: string): Promise<MarketplaceItem[]> =>
  invoke<MarketplaceItem[]>('list_marketplace_humans', { registryUrl })

export interface InstalledSkillBrief {
  skillId: string
  description: string | null
  installPath: string
  fileCount: number
}

export type CapabilityStatus = 'mapped' | 'missing'

export interface CapabilityCheck {
  mcpId: string
  status: CapabilityStatus
  mappedTo: string | null
}

export interface InstalledAutomation {
  slug: string
  name: string
  version: string
  icon: string | null
  category: string
  bundledSkills: InstalledSkillBrief[]
  requiredCapabilities: CapabilityCheck[]
}

export const listInstalledMarketplaceAutomations = (): Promise<InstalledAutomation[]> =>
  invoke<InstalledAutomation[]>('list_installed_marketplace_automations')

// ─── Badge ────────────────────────────────────────────────────────────
export const updateBadgeCount = (count: number): Promise<boolean> =>
  invoke<boolean>('update_badge_count', { count })

// ─── Permission Rules ─────────────────────────────────────────────────
export const listPermissionRules = (): Promise<PermissionRule[]> =>
  invoke<PermissionRule[]>('list_permission_rules')

export const createPermissionRule = (input: CreatePermissionRuleInput): Promise<PermissionRule> =>
  invoke<PermissionRule>('create_permission_rule', { input })

export const deletePermissionRule = (id: string): Promise<boolean> =>
  invoke<boolean>('delete_permission_rule', { id })

export const listPermissionAudit = (sessionId?: string, limit = 100): Promise<PermissionAuditEntry[]> =>
  invoke<PermissionAuditEntry[]>('list_permission_audit', { sessionId, limit })

// ─── Prompts (workspace uclaw.md + default mode prompts) ──────────────
export const readWorkspaceUclawMd = (): Promise<string> =>
  invoke<string>('read_workspace_uclaw_md')

export const writeWorkspaceUclawMd = (content: string): Promise<void> =>
  invoke<void>('write_workspace_uclaw_md', { content })

export const readDefaultPrompts = (): Promise<DefaultPromptsResponse> =>
  invoke<DefaultPromptsResponse>('read_default_prompts')

/**
 * Open `<active_workspace>/uclaw.md` in the OS default application (file
 * manager / text editor). Creates the file if it doesn't exist.
 * Errors propagate (caller should show toast).
 */
export const openWorkspaceUclawMdExternally = (): Promise<void> =>
  invoke<void>('open_workspace_uclaw_md_externally')

// ============================================================================
// Files Rail (W3)
// ============================================================================

interface BackendFileNode {
  path: string
  rel_path: string
  name: string
  kind: 'file' | 'directory'
  size: number
  mtime_ms: number
  is_ignored: boolean
}

const toTreeNode = (n: BackendFileNode): TreeNode => ({
  kind: n.kind,
  relPath: n.rel_path,
  name: n.name,
  size: n.size,
  mtimeMs: n.mtime_ms,
})

interface BackendMountRoot {
  id: string
  label: string
  path: string
  kind: 'workspace' | 'session' | 'attached_dir'
  editable: boolean
}

const toMountRoot = (m: BackendMountRoot): MountRoot => ({ ...m })

export async function filesRailListMounts(sessionId: string | null): Promise<MountRoot[]> {
  const raw = await invoke<BackendMountRoot[]>('files_rail_list_mounts', { sessionId })
  return raw.map(toMountRoot)
}

export async function filesRailReadDir(
  mountId: string,
  relPath: string,
  sessionId: string | null = null,
): Promise<TreeNode[]> {
  const raw = await invoke<BackendFileNode[]>('files_rail_read_dir', { mountId, relPath, sessionId })
  return raw.map(toTreeNode)
}

export async function filesRailWatchStart(
  mountId: string,
  sessionId: string | null = null,
): Promise<void> {
  await invoke<void>('files_rail_watch_start', { mountId, sessionId })
}

export async function filesRailWatchStop(mountId: string): Promise<void> {
  await invoke<void>('files_rail_watch_stop', { mountId })
}

// ============================================================================
// Preview (W4a)
// ============================================================================

interface BackendPreviewBytes {
  resolved_path: string
  /** Tauri serializes Vec<u8> as a number[] by default. */
  bytes: number[]
  size: number
  truncated: boolean
  mtime_ms: number
}

export interface PreviewBytes {
  resolvedPath: string
  /** Owned byte buffer for the file content. */
  bytes: Uint8Array
  size: number
  truncated: boolean
  mtimeMs: number
}

// ── STT ──────────────────────────────────────────────────────────────────

/** Pre-warm: download ONNX Runtime dylib if missing, set ORT_DYLIB_PATH.
 *  Idempotent — returns quickly when already cached.
 *  Listen to `stt:runtime_progress` for {phase, downloaded, total} updates. */
export const sttEnsureRuntime = (): Promise<string> =>
  invoke('stt_ensure_runtime')

export async function previewReadBytes(
  mountId: string,
  relPath: string,
  sessionId: string | null = null,
): Promise<PreviewBytes> {
  const raw = await invoke<BackendPreviewBytes>('preview_read_bytes', {
    mountId,
    relPath,
    sessionId,
  })
  return {
    resolvedPath: raw.resolved_path,
    bytes: new Uint8Array(raw.bytes),
    size: raw.size,
    truncated: raw.truncated,
    mtimeMs: raw.mtime_ms,
  }
}

// ── Global Shortcut ─────────────────────────────────────────────────────────

/**
 * 动态更新全局快捷键绑定。解除旧绑定并注册新的系统级全局快捷键。
 * @param shortcutId 快捷键 ID，如 "quick-memory-voice"
 * @param newCombo 新的组合键字符串，如 "Cmd+Shift+M"。空字符串表示移除绑定。
 */
export async function updateGlobalShortcut(shortcutId: string, newCombo: string): Promise<void> {
  await invoke<void>('update_global_shortcut', { shortcutId, newCombo })
}

// ============ 记忆碎片相关 ============

export interface ReviewStatus {
  reviewCount: number
  nextReviewAt: number | null
  completed: boolean
}

export interface FragmentItem {
  id: string
  title: string | null
  content: string
  source: string
  tags: string[]
  subtype?: string
  createdAt: number
  reviewStatus?: ReviewStatus
}

export interface FragmentSearchHit {
  id: string
  title: string | null
  snippet: string
  tags: string[]
  subtype?: string
  source: string
  createdAt: number
}

export interface ListFragmentsInput {
  spaceId?: string
  tag?: string
  limit?: number
  offset?: number
}

export interface DailySummaryItem {
  id: string
  summaryDate: string
  content: string
  fragmentCount: number
  fragmentIds: string[]
  createdAt: number
}

export const memoryGraphListFragments = (input: ListFragmentsInput): Promise<FragmentItem[]> =>
  invoke('memory_graph_list_fragments', { input })

export const searchFragments = (query: string): Promise<FragmentSearchHit[]> =>
  invoke('search_fragments', { query })

export const listDailySummaries = (limit?: number): Promise<DailySummaryItem[]> =>
  invoke('list_daily_summaries', { limit })

// ═══════════════════════════════════════════════════════════════════════════
// Symphony runtime — T14 frontend bindings.
//
// Schema mirrors `src-tauri/src/symphony/protocol/types.rs` + the DTOs in
// `tauri_commands.rs` (symphony_*). Keep these typed mirrors in sync when
// either side changes — there's no codegen step today.
// ═══════════════════════════════════════════════════════════════════════════

export type SymphonyNodeStatus =
  | 'pending'
  | 'ready'
  | 'running'
  | 'stalled'
  | 'succeeded'
  | 'failed'
  | 'cancelled'

export type SymphonyRunStatus =
  | 'queued'
  | 'running'
  | 'completed'
  | 'failed'
  | 'cancelled'
  | 'quota_exceeded'

export type SymphonyRunOutcome = 'succeeded' | 'partial' | 'failed'

export type SymphonyNodeKind = 'agent' | 'shell' | 'http'
export type SymphonyFailureMode = 'abort' | 'continue_others' | 'branch_only'

export interface SymphonyRetryPolicy {
  max_attempts: number
  max_backoff_ms?: number | null
}

export interface SymphonyNode {
  id: string
  label: string
  kind: SymphonyNodeKind
  prompt: string
  deps: string[]
  cost_cap_usd?: number | null
  max_iterations?: number | null
  retry: SymphonyRetryPolicy
  after_create_command?: string | null
  after_run_command?: string | null
  model?: string | null
}

export interface SymphonyEdge {
  from: string
  to: string
  label?: string | null
}

export interface SymphonyWorkflowDef {
  id: string
  name: string
  description?: string | null
  space_id?: string | null
  default_model?: string | null
  per_run_cost_cap_usd?: number | null
  max_concurrent_nodes?: number | null
  failure_mode: SymphonyFailureMode
  nodes: SymphonyNode[]
  edges: SymphonyEdge[]
}

export interface SymphonyWorkflowSummary {
  id: string
  name: string
  description: string | null
  spaceId: string | null
  currentVersion: number
  enabled: boolean
  createdAt: number
  updatedAt: number
}

export interface SymphonyWorkflowDetailDto {
  summary: SymphonyWorkflowSummary
  definition: SymphonyWorkflowDef
  definitionMd: string
}

export interface SymphonyRunRow {
  id: string
  workflowId: string
  workflowVersion: number
  status: SymphonyRunStatus
  outcome: SymphonyRunOutcome | null
  totalCostUsd: number
  queuedAt: number
  startedAt: number | null
  completedAt: number | null
}

export interface SymphonyNodeRunRow {
  id: string
  runId: string
  nodeId: string
  attempt: number
  status: SymphonyNodeStatus
  sessionId: string | null
  costUsd: number
  iterations: number
  lastHeartbeatMs: number | null
  errorText: string | null
  outputJson: string | null
}

export interface SymphonyRunDetail {
  run: SymphonyRunRow
  nodes: SymphonyNodeRunRow[]
}

export interface SymphonySaveResult {
  workflowId: string
  version: number
}

// ─── IPC events (listen via @tauri-apps/api/event) ─────────────────────────

export interface SymphonyNodeUpdateEvent {
  runId: string
  nodeId: string
  status: SymphonyNodeStatus
}

export interface SymphonyNodeLogEvent {
  runId: string
  nodeId: string
  role: 'assistant' | 'tool' | 'user'
  content: string
}

export interface SymphonyRunStartedEvent {
  runId: string
  workflowId: string
  startedAt: number
}

export interface SymphonyRunCompletedEvent {
  runId: string
  status: SymphonyRunStatus
  totalCostUsd: number
  succeeded: number
  failed: number
  cancelled: number
}

export interface SymphonyCostCapHitEvent {
  runId: string
  nodeId?: string
  scope: 'per_run' | 'per_day'
  usd: number
  capUsd: number
}

// ─── Tauri command wrappers ────────────────────────────────────────────────

export const symphonyListWorkflows = (): Promise<SymphonyWorkflowSummary[]> =>
  invoke('symphony_list_workflows')

export const symphonyGetWorkflow = (
  workflowId: string,
): Promise<SymphonyWorkflowDetailDto> =>
  invoke('symphony_get_workflow', { workflowId })

export const symphonySaveWorkflow = (
  definition: SymphonyWorkflowDef,
  definitionMd: string,
): Promise<SymphonySaveResult> =>
  invoke('symphony_save_workflow', { definition, definitionMd })

export const symphonyDeleteWorkflow = (workflowId: string): Promise<void> =>
  invoke('symphony_delete_workflow', { workflowId })

export const symphonyImportWorkflowMd = (
  source: string,
): Promise<SymphonySaveResult> =>
  invoke('symphony_import_workflow_md', { source })

export const symphonyExportWorkflowMd = (workflowId: string): Promise<string> =>
  invoke('symphony_export_workflow_md', { workflowId })

export const symphonyListRuns = (
  workflowId?: string,
): Promise<SymphonyRunRow[]> =>
  invoke('symphony_list_runs', { workflowId })

export const symphonyGetRun = (runId: string): Promise<SymphonyRunDetail> =>
  invoke('symphony_get_run', { runId })

export const symphonyTriggerRun = (
  workflowId: string,
  inputsJson?: string,
): Promise<string> =>
  invoke('symphony_trigger_run', { workflowId, inputsJson })

export const symphonyCancelRun = (runId: string): Promise<void> =>
  invoke('symphony_cancel_run', { runId })

export const symphonyGetNodeSessionId = (
  runId: string,
  nodeId: string,
): Promise<string | null> =>
  invoke('symphony_get_node_session_id', { runId, nodeId })

// ─── Symphony service health (audit P2b) ──────────────────────────────────

export type SymphonyServiceStatus =
  | { status: 'Stopped' }
  | { status: 'Starting' }
  | { status: 'Running' }
  | { status: 'Stopping' }
  | { status: 'Failed'; reason: string }

export interface SymphonyServiceHealthDto {
  /** True when memubot_config.symphony.enabled was false at boot. */
  disabled: boolean
  // Note: nested `health` keys are snake_case because `ServiceHealth` in
  // src-tauri/src/services/types.rs has no `#[serde(rename_all)]`. Only
  // the DTO wrapper itself uses camelCase.
  health: {
    name: string
    status: SymphonyServiceStatus
    uptime_secs: number | null
    last_error: string | null
    metrics: { active_runs?: number; stall_tracking_nodes?: number } & Record<
      string,
      unknown
    >
  } | null
}

export const symphonyGetServiceHealth = (): Promise<SymphonyServiceHealthDto> =>
  invoke('symphony_get_service_health')

// ── Plan-mode auto-suggest ────────────────────────────────────────────────────

/**
 * Report the user's decision on a plan-mode suggestion event.
 * @param eventId  UUID emitted by the backend alongside the IPC event.
 * @param outcome  One of accepted | skipped | silenced | aborted.
 * @param declineReason  Optional free-text reason (currently unused by backend, reserved).
 */
export async function respondPlanModeSuggest(
  eventId: string,
  outcome: 'accepted' | 'skipped' | 'silenced' | 'aborted',
  declineReason?: string,
): Promise<void> {
  await invoke('respond_plan_mode_suggest', { eventId, outcome, declineReason })
}
