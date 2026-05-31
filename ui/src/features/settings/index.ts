// Public surface of the settings feature — other features import ONLY from here,
// never from internal files (code-organization ADR 2026-05-31, rule 1).
//
// Components/hooks are added to this barrel as they migrate (plan P1–P3:
// docs/superpowers/plans/2026-05-31-frontend-settings-feature-migration.md).
export { settingsBridge } from '../../lib/bridge/settings'

// P1 — SystemTab migrated + split into system/ cards + hooks.
export { SystemTab } from './components/SystemTab'

// P2 — GeneralTab migrated (thin composer; sub-sections migrate later).
export { GeneralTab } from './components/GeneralTab'

// P2 — AgentSettings migrated (presentation; plan-mode persist in a hook).
export { AgentSettings } from './components/AgentSettings'

// P2 — PromptsSettings migrated (presentation; load/save IPC in a hook).
export { PromptsSettings } from './components/PromptsSettings'

// P3 — BrowserRuntimeSettings migrated (thin shell; side effects in a hook,
// split into browser-runtime/ cards + lib/browser-runtime-format helpers).
export { BrowserRuntimeSettings } from './components/BrowserRuntimeSettings'

// P3 — AppearanceSettings migrated (theme cards; atom-helper side effects, no IPC).
export { AppearanceSettings } from './components/AppearanceSettings'

// P3 — PromptSettings migrated (thin shell; CRUD side effects in a hook, rows
// split into prompts/PromptRow). Distinct from the P2 PromptsSettings tab above.
export { PromptSettings } from './components/PromptSettings'

// P3 — GeneralSettings migrated (thin shell; language load/persist in a hook).
export { GeneralSettings } from './components/GeneralSettings'

// P3a — ToolSettings migrated (active-manifest load in a hook). The skill-tag
// editor is now a migrated sibling (see WorkspaceSkillTagsEditor below).
export { ToolSettings } from './components/ToolSettings'

// P3a — ModelSettings migrated (role→model load + optimistic write in a hook;
// dropdown open/outside-click UI state stays in the component).
export { ModelSettings } from './components/ModelSettings'

// P3a — PermissionsSettings migrated + split (328 → thin shell composing
// permissions/ cards; all IPC + draft state in usePermissionsSettings; the
// sandbox sub-panel stays under components/settings/ for now).
export { PermissionsSettings } from './components/PermissionsSettings'

// P3a — EmbeddingEndpointSection migrated (config load/save in a hook; IPC stays
// in the @/lib/embedding-endpoint gbrain/memU helper). Consumed by SystemTab.
export { EmbeddingEndpointSection } from './components/EmbeddingEndpointSection'

// P3a — StreamSkillThresholdsSection migrated (load/save in a hook; IPC stays in
// the @/lib/stream-skill-thresholds Bundle-26/27 helper). Consumed by SystemTab.
export { StreamSkillThresholdsSection } from './components/StreamSkillThresholdsSection'

// P3a — FoldDeltaThresholdSection migrated (load/save in a hook; IPC stays in the
// @/lib/fold-delta-threshold Bundle-17-B helper). Consumed by SystemTab.
export { FoldDeltaThresholdSection } from './components/FoldDeltaThresholdSection'

// P3a — DeveloperOptionsSection migrated (run state machine + setup-script event
// subscriptions in a hook; the `@tauri-apps/api/event` `listen` moved behind
// settingsBridge.onSetupScript* wrappers). Consumed by SystemTab.
export { DeveloperOptionsSection } from './components/DeveloperOptionsSection'

// ── IM / channel cluster (Settings → 机器人 / 渠道). Migrated out of
// components/settings/ ; raw `invoke` moved behind settingsBridge.*; oversized
// rows split into im-channels/ accordion parts + hooks. ──

// WechatIlinkBindingPanel migrated (QR-binding state machine in
// useWechatIlinkBinding; only the canvas draw effect stays in the component).
export { WechatIlinkBindingPanel } from './components/im-channels/WechatIlinkBindingPanel'

// ImChannelAccordionRow migrated + split (602 → shell + im-channels/accordion/
// ChannelTypeFields + useImChannelAccordionForm + lib/im-channel-format).
export { ImChannelAccordionRow } from './components/im-channels/ImChannelAccordionRow'

// ImChannelForm migrated + split (315 → shell + im-channels/form/
// ImChannelFormFields + useImChannelForm). The flat add/edit form (distinct
// from the accordion row); currently has no consumer in-tree.
export { ImChannelForm } from './components/im-channels/ImChannelForm'

// ImChannelsSettings migrated (Settings → 机器人 tabbed list; data + IPC + the
// realtime status subscription in useImChannelsSettings, pure-UI tab state in
// the component). Consumed by SettingsPanel.
export { ImChannelsSettings } from './components/im-channels/ImChannelsSettings'

// ChannelSettings migrated + split (455 → list shell + channels/ProviderDetail +
// useChannelSettings + useProviderDetail). Model-provider config panel; IPC stays
// in the typed @/lib/tauri-bridge provider helpers. Consumed by ConnectivityTab.
// ProviderDetail is re-exported for consumers/tests that import it by name.
export { ChannelSettings, ProviderDetail } from './components/channels/ChannelSettings'

// ChannelForm migrated (provider quick-configure modal; config load + submit in
// useChannelForm). No in-tree consumer renders it today; kept for completeness.
export { ChannelForm } from './components/channels/ChannelForm'

// Platform stubs migrated (1:1 moves, no IPC): BotHubSettings is the Bot-Hub
// marketplace placeholder; WeChat/Feishu/DingTalk are Proma-only integrations
// that render null in uClaw. None have in-tree consumers today.
export { BotHubSettings } from './components/channels/BotHubSettings'
export { default as WeChatSettings } from './components/channels/WeChatSettings'
export { default as FeishuSettings } from './components/channels/FeishuSettings'
export { default as DingTalkSettings } from './components/channels/DingTalkSettings'

// ── Persona cluster (Settings → Agent 人格). Migrated out of components/settings/ ;
// config load + optimistic writes moved into hooks; IPC stays in the typed
// @/lib/persona domain helpers. Consumed by AgentSettings (relative sibling). ──

// PersonaStudio migrated (config load + optimistic voice write in usePersonaStudio;
// clampVoice/clampSlider moved with the write path).
export { PersonaStudio } from './components/PersonaStudio'

// PersonaBondTimeline migrated + split (560 → thin shell composing persona-bond/
// section lists; all state + the six mutating actions in usePersonaBondTimeline;
// the typed @/lib/persona domain helpers stay in the hook).
export { PersonaBondTimeline } from './components/PersonaBondTimeline'

// ── Memory cluster (Settings → 记忆 / 记忆召回). Migrated out of
// components/settings/ ; the boot-node load + config CRUD side effects moved into
// hooks; IPC stays in the typed @/lib/tauri-bridge memory-graph + recall helpers. ──

// MemorySettings migrated (boot-node load + toggle state in useMemorySettings).
// No in-tree consumer renders it today; kept for completeness.
export { MemorySettings } from './components/MemorySettings'

// MemoryRecallSettings migrated + split (474 → thin shell composing memory-recall/
// section cards; all state + IPC in useMemoryRecallSettings; the typed
// @/lib/tauri-bridge get/patchMemoryRecallConfig helpers stay in the hook).
export { MemoryRecallSettings } from './components/MemoryRecallSettings'

// MemoryRecallTab migrated (记忆召回配置 page; thin wrapper around
// MemoryRecallSettings). Consumed by components/settings/SettingsPanel.
export { MemoryRecallTab } from './components/MemoryRecallTab'

// ── Skill cluster (used from the Kaleidoscope Skills module, not Settings nav).
// Migrated out of components/settings/ ; side effects moved into hooks; IPC stays
// in the typed @/lib/tauri-bridge skill helpers. Consumers import from this barrel. ──

// SkillEvolutionTab migrated (version-history load + selection in useSkillVersions).
// Consumed by views/Kaleidoscope/.../SkillDetail.
export { SkillEvolutionTab } from './components/SkillEvolutionTab'

// SkillConsolidationDialog migrated (expanded/applying state machine + apply
// action in useSkillConsolidation). Consumed by views/Kaleidoscope/.../SkillsModule.
export { SkillConsolidationDialog } from './components/SkillConsolidationDialog'

// WorkspaceSkillTagsEditor migrated (workspace-tag data + persist in
// useWorkspaceSkillTags; the @/atoms/workspace reads + typed @/lib/tauri-bridge
// tag helpers stay in the hook). Consumed by ToolSettings (relative sibling import).
export { WorkspaceSkillTagsEditor } from './components/WorkspaceSkillTagsEditor'
