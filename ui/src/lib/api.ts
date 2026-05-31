import { invoke } from "@tauri-apps/api/core";
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
  RemoteConfig,
  PairDeviceRequest,
  PairDeviceResponse,
  AuthStatusResponse,
  CreateApiTokenRequest,
  CreateApiTokenResponse,
  HealthCheckResponse,
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
} from "./types";

export const apiClient = {
  // ─── Bootstrap ───────────────────────────────────────────────────────
  async getSettings(): Promise<Settings> {
    return invoke("get_settings");
  },
  async patchSettings(input: PatchSettingsInput): Promise<Settings> {
    return invoke("patch_settings", { input });
  },
  async getPlatform(): Promise<PlatformInfo> {
    return invoke("get_platform");
  },
  async getVersion(): Promise<VersionInfo> {
    return invoke("get_version");
  },
  async getBootstrapStatus(): Promise<BootstrapStatus> {
    return invoke("get_bootstrap_status");
  },

  // ─── Chat ────────────────────────────────────────────────────────────
  async sendMessage(input: SendMessageInput): Promise<SendMessageResponse> {
    return invoke("send_message", { input });
  },
  async createConversation(input: CreateConversationInput): Promise<ConversationResponse> {
    return invoke("create_conversation", { input });
  },
  async listConversations(): Promise<ConversationResponse[]> {
    return invoke("list_conversations");
  },
  async getMessages(input: GetMessagesInput): Promise<Message[]> {
    return invoke("get_messages", { input });
  },
  async deleteConversation(id: string): Promise<boolean> {
    return invoke("delete_conversation", { id });
  },
  async approveToolCall(input: ApproveToolCallInput): Promise<void> {
    return invoke("approve_tool_call", { input });
  },

  // ─── Spaces ──────────────────────────────────────────────────────────
  async createSpace(input: CreateSpaceInput): Promise<SpaceSummary> {
    return invoke("create_space", { input });
  },
  async listSpaces(): Promise<SpaceSummary[]> {
    return invoke("list_spaces");
  },
  async deleteSpace(id: string): Promise<boolean> {
    return invoke("delete_space", { id });
  },

  // ─── LLM Config ──────────────────────────────────────────────────────
  async getLlmConfig(): Promise<LlmConfigResponse> {
    return invoke("get_llm_config");
  },
  async updateLlmConfig(input: LlmConfigInput): Promise<LlmConfigResponse> {
    return invoke("update_llm_config", { input });
  },

  // ─── Artifacts ───────────────────────────────────────────────────────
  async listArtifacts(): Promise<ArtifactNode[]> {
    return invoke("list_artifacts");
  },
  async readArtifact(path: string): Promise<ArtifactContentResponse> {
    return invoke("read_artifact", { input: { path } });
  },
  async writeArtifact(path: string, content: string): Promise<ArtifactContentResponse> {
    return invoke("write_artifact", { input: { path, content } });
  },
  async deleteArtifact(path: string): Promise<boolean> {
    return invoke("delete_artifact", { path });
  },
  async listArtifactsTree(input: ListArtifactTreeInput): Promise<ArtifactTreeNodeResponse[]> {
    return invoke("list_artifacts_tree", { input });
  },
  async loadArtifactChildren(input: LoadArtifactChildrenInput): Promise<ArtifactTreeNodeResponse[]> {
    return invoke("load_artifact_children", { input });
  },
  async createArtifact(input: CreateArtifactInput): Promise<ArtifactTreeNodeResponse> {
    return invoke("create_artifact", { input });
  },
  async renameArtifact(input: RenameArtifactInput): Promise<boolean> {
    return invoke("rename_artifact", { input });
  },
  async moveArtifact(input: MoveArtifactInput): Promise<boolean> {
    return invoke("move_artifact", { input });
  },
  async deleteArtifactRecursive(spaceId: string, path: string): Promise<boolean> {
    return invoke("delete_artifact_recursive", { spaceId, path });
  },
  async detectFileType(path: string): Promise<DetectFileTypeResponse> {
    return invoke("detect_file_type", { path });
  },
  async toggleStarConversation(conversationId: string): Promise<ToggleStarResponse> {
    return invoke("toggle_star_conversation", { input: { conversationId } });
  },

  // ─── Search ───────────────────────────────────────────────────────────
  async searchWorkspace(input: SearchInput): Promise<SearchResult[]> {
    return invoke("search_workspace", { input });
  },
  async searchConversations(input: SearchInput): Promise<SearchResult[]> {
    return invoke("search_conversations", { input });
  },
  async searchAll(input: SearchInput): Promise<SearchResult[]> {
    return invoke("search_all", { input });
  },

  // ─── Providers ───────────────────────────────────────────────────────
  async listProviders(): Promise<ProviderInfo[]> {
    return invoke("list_providers");
  },
  async listConfiguredProviders(): Promise<string[]> {
    return invoke("list_configured_providers");
  },
  async getProviderConfig(providerId: string): Promise<ProviderConfigResponse | null> {
    return invoke("get_provider_config", { providerId });
  },
  async configureProvider(input: ProviderConfigInput): Promise<void> {
    return invoke("configure_provider", { input });
  },
  async configureProviderWithModels(input: ProviderConfigureInput): Promise<void> {
    const { modelIds, ...config } = input;
    return invoke("configure_provider_with_models", { providerConfig: config, modelIds });
  },
  async removeProviderConfig(providerId: string): Promise<void> {
    return invoke("remove_provider_config", { providerId });
  },
  async testProviderConnection(input: TestConnectionInput): Promise<TestResultInfo> {
    return invoke("test_provider_connection", { input });
  },
  async listProviderModels(input: ListModelsInput): Promise<ModelInfo[]> {
    return invoke("list_provider_models", { input });
  },
  async getConfiguredModels(providerId: string): Promise<string[]> {
    return invoke("get_configured_models", { providerId });
  },
  async getAllConfiguredModels(): Promise<[string, string[]][]> {
    return invoke("get_all_configured_models");
  },
  async getActiveModel(): Promise<ModelSelectionInfo | null> {
    return invoke("get_active_model");
  },
  async setActiveModel(providerId: string, modelId: string): Promise<void> {
    return invoke("set_active_model", { providerId, modelId });
  },

  // ─── Learned Skills ─────────────────────────────────────────────────
  async listLearnedSkills(spaceId: string = 'default'): Promise<LearnedSkill[]> {
    return invoke('list_learned_skills', { spaceId });
  },
  async getLearnedSkill(skillId: string): Promise<LearnedSkill> {
    return invoke('get_learned_skill', { skillId });
  },
  async toggleLearnedSkill(skillId: string, enabled: boolean): Promise<void> {
    return invoke('toggle_learned_skill', { skillId, enabled });
  },
  async deleteLearnedSkill(skillId: string): Promise<void> {
    return invoke('delete_learned_skill', { skillId });
  },
};
// ─── Remote HTTP API Client ─────────────────────────────────────────────

const DEFAULT_REMOTE_URL = "http://127.0.0.1:19528";

export const remoteClient = {
  config: { baseUrl: DEFAULT_REMOTE_URL, token: null as string | null } as RemoteConfig,

  setToken(token: string | null) {
    this.config.token = token;
    if (typeof localStorage !== "undefined") {
      if (token) {
        localStorage.setItem("uclaw_remote_token", token);
      } else {
        localStorage.removeItem("uclaw_remote_token");
      }
    }
  },

  loadToken() {
    if (typeof localStorage !== "undefined") {
      this.config.token = localStorage.getItem("uclaw_remote_token");
    }
  },

  // Internal request helper
  async _request<T>(method: string, path: string, body?: unknown): Promise<T> {
    const headers: Record<string, string> = {
      "Content-Type": "application/json",
    };
    if (this.config.token) {
      headers["Authorization"] = `Bearer ${this.config.token}`;
    }

    const res = await fetch(`${this.config.baseUrl}/api${path}`, {
      method,
      headers,
      body: body ? JSON.stringify(body) : undefined,
    });

    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: res.statusText }));
      throw new Error(err.error || `HTTP ${res.status}`);
    }

    return res.json();
  },

  // Health
  async health(): Promise<HealthCheckResponse> {
    return this._request("GET", "/health");
  },

  // Auth
  async pairDevice(deviceName: string): Promise<PairDeviceResponse> {
    const result = await this._request<PairDeviceResponse>("POST", "/auth/pair", {
      deviceName,
    } as PairDeviceRequest);
    this.setToken(result.token);
    return result;
  },

  async authStatus(): Promise<AuthStatusResponse> {
    return this._request("GET", "/auth/status");
  },

  async createApiToken(input: CreateApiTokenRequest): Promise<CreateApiTokenResponse> {
    return this._request("POST", "/auth/token", input);
  },

  // Conversations
  async listConversations(): Promise<ConversationResponse[]> {
    return this._request("GET", "/conversations");
  },

  async createConversation(input: CreateConversationInput): Promise<ConversationResponse> {
    return this._request("POST", "/conversations", input);
  },

  async getMessages(conversationId: string): Promise<Message[]> {
    return this._request("GET", `/messages?conversation_id=${conversationId}`);
  },

  async deleteConversation(id: string): Promise<boolean> {
    return this._request("DELETE", `/conversations/${encodeURIComponent(id)}`);
  },

  // Artifacts
  async listArtifacts(): Promise<ArtifactNode[]> {
    return this._request("GET", "/artifacts");
  },

  async readArtifact(path: string): Promise<ArtifactContentResponse> {
    return this._request("GET", `/artifacts/${encodeURIComponent(path)}`);
  },

  async writeArtifact(path: string, content: string): Promise<ArtifactContentResponse> {
    return this._request("POST", "/artifacts", { path, content });
  },

  async deleteArtifact(path: string): Promise<boolean> {
    return this._request("DELETE", `/artifacts/${encodeURIComponent(path)}`);
  },

  // WebSocket
  connectWebSocket(): WebSocket {
    const url = this.config.baseUrl.replace("http", "ws");
    const wsUrl = `${url}/api/ws${this.config.token ? `?token=${this.config.token}` : ""}`;
    return new WebSocket(wsUrl);
  },

  // ─── Dev / Testing ────────────────────────────────────────────────────
  async triggerProactiveScenario(scenarioName: string): Promise<any> {
    return invoke("trigger_proactive_scenario", { scenarioName });
  },
};
