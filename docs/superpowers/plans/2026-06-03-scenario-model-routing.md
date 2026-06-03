# Scenario Model Routing (S0) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the runtime honor the `summarizer` / `compiler` / `utility` model-role assignments (today only `chat` is read) by routing each Memory-OS LLM pass to its assigned role, with a safe `role → chat → active` fallback.

**Architecture:** Add one generic resolver `ProviderService::get_role_llm_config(role)` and express the existing `get_chat_llm_config` / `get_ingestion_llm_config` in terms of it. In `MemoryOsLlmClient::complete_text`, map the `cost_tag` it already receives to a role via a pure `role_for_cost_tag` function and resolve config per call. Augment the existing `[VERIFY]` log with `role` + `resolved_model`.

**Tech Stack:** Rust, Tauri, tokio, `cargo test --lib`. No migrations, no Tauri command changes, no frontend changes.

**Spec:** `docs/superpowers/specs/2026-06-03-scenario-model-routing-design.md`

**Branch:** S0 worktree branches off `pi/memory-llm-tz-fixes` HEAD (which already contains the S0 spec commit `d7757b72` and the now-committed memory fixes / former VERIFY-temp lineage). The stale `pi/scenario-model-routing` branch (spec-only, at `d7757b72`) is superseded.

**Pre-flight (repo policy):** GitNexus index is stale. Before editing symbols, run `npx gitnexus analyze`, then run `gitnexus_impact({target: "get_chat_llm_config", direction: "upstream"})` and report the blast radius (expected callers: pi-engine path in `tauri_commands.rs`, legacy agent path, `MemoryOsLlmClient`). The change is behavior-preserving for `chat`, so risk should be LOW — but confirm.

---

## File Structure

| File | Responsibility | Change |
|---|---|---|
| `src-tauri/src/providers/service.rs` | Provider/model config resolution | Add `get_role_llm_config`; reimplement `get_chat_llm_config` + `get_ingestion_llm_config` on top of it; add `#[cfg(test)]` constructor + fallback tests |
| `src-tauri/src/memory_graph/memory_os_llm.rs` | Memory-OS LLM façade | Add `role_for_cost_tag`; route `complete_text` through it; add `role`/`resolved_model` to `[VERIFY]` log; add mapping test |

---

## Task 1: Generic `get_role_llm_config` resolver

**Files:**
- Modify: `src-tauri/src/providers/service.rs` (add method near existing `get_chat_llm_config` at lines 168-202; reimplement `get_chat_llm_config` 168-202 and `get_ingestion_llm_config` 206-236; add test constructor + tests in `mod tests` at line 563)

- [ ] **Step 1: Write the failing tests**

Add these to the `#[cfg(test)] mod tests` block in `src-tauri/src/providers/service.rs` (after the existing `test_anthropic_models_have_context_windows` test). Also add the imports/helper shown.

```rust
    // ProviderConfig + ProviderConfigs are already in scope via `use super::*`.
    use super::super::types::{ApiType, ModelRoleConfig, ModelSelection};

    /// Build a ProviderService directly from in-memory configs (no disk I/O).
    fn svc(configs: ProviderConfigs) -> ProviderService {
        ProviderService {
            configs: std::sync::Arc::new(tokio::sync::RwLock::new(configs)),
            configs_path: std::path::PathBuf::from("/tmp/uclaw-test-providers.json"),
        }
    }

    fn provider(id: &str) -> ProviderConfig {
        ProviderConfig {
            provider_id: id.to_string(),
            display_name: id.to_string(),
            api_key: Some(format!("key-{id}")),
            base_url: Some(format!("https://{id}.example/v1")),
            api: Some(ApiType::OpenAiCompletions),
        }
    }

    #[tokio::test]
    async fn role_config_uses_exact_role_assignment() {
        let configs = ProviderConfigs {
            providers: vec![provider("local"), provider("deepseek")],
            active_model: Some(ModelSelection {
                provider_id: "deepseek".into(),
                model_id: "deepseek-v4".into(),
            }),
            selected_models: vec![],
            role_models: vec![
                ModelRoleConfig { role: "chat".into(), model_ref: Some("deepseek/deepseek-v4".into()) },
                ModelRoleConfig { role: "summarizer".into(), model_ref: Some("local/minicpm5-1b".into()) },
            ],
        };
        let s = svc(configs);
        let (pid, mid, _key, _url, _api) = s.get_role_llm_config("summarizer").await.unwrap();
        assert_eq!(pid, "local");
        assert_eq!(mid, "minicpm5-1b");
    }

    #[tokio::test]
    async fn role_config_falls_back_to_chat_when_role_unset() {
        let configs = ProviderConfigs {
            providers: vec![provider("deepseek")],
            active_model: None,
            selected_models: vec![],
            role_models: vec![ModelRoleConfig {
                role: "chat".into(),
                model_ref: Some("deepseek/deepseek-v4".into()),
            }],
        };
        let s = svc(configs);
        let (pid, mid, _, _, _) = s.get_role_llm_config("summarizer").await.unwrap();
        assert_eq!(pid, "deepseek");
        assert_eq!(mid, "deepseek-v4");
    }

    #[tokio::test]
    async fn role_config_falls_back_to_active_when_chat_unset() {
        let configs = ProviderConfigs {
            providers: vec![provider("deepseek")],
            active_model: Some(ModelSelection {
                provider_id: "deepseek".into(),
                model_id: "deepseek-v4".into(),
            }),
            selected_models: vec![],
            role_models: vec![],
        };
        let s = svc(configs);
        let (pid, mid, _, _, _) = s.get_role_llm_config("summarizer").await.unwrap();
        assert_eq!(pid, "deepseek");
        assert_eq!(mid, "deepseek-v4");
    }

    #[tokio::test]
    async fn role_config_none_when_nothing_configured() {
        let s = svc(ProviderConfigs::default());
        assert!(s.get_role_llm_config("summarizer").await.is_none());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test --lib providers::service::tests::role_config 2>&1 | tail -20`
Expected: FAIL — `no method named get_role_llm_config found` (and the `ProviderService { configs, configs_path }` literal requires the fields, which are private but accessible inside the same module — this compiles once the method exists).

- [ ] **Step 3: Add the generic resolver**

In `src-tauri/src/providers/service.rs`, insert this method immediately before `get_chat_llm_config` (currently line 168):

```rust
    /// Resolve the LLM config for a model role with a graceful fallback chain.
    /// Priority: role_models[role] → role_models["chat"] → active_model.
    /// Returns `(provider_id, model, api_key, base_url, api_override)`.
    pub async fn get_role_llm_config(
        &self,
        role: &str,
    ) -> Option<(String, String, String, String, Option<crate::providers::types::ApiType>)> {
        let configs = self.configs.read().await;

        // 1) exact role assignment, then 2) "chat" role fallback.
        for candidate in [role, "chat"] {
            let Some(role_cfg) = configs.role_models.iter().find(|r| r.role == candidate) else {
                continue;
            };
            let Some(model_ref) = &role_cfg.model_ref else {
                continue;
            };
            let parts: Vec<&str> = model_ref.splitn(2, '/').collect();
            if parts.len() != 2 {
                continue;
            }
            let (pid, mid) = (parts[0], parts[1]);
            if let Some(provider) = configs.find_provider(pid) {
                return Some((
                    pid.to_string(),
                    mid.to_string(),
                    provider.api_key.clone().unwrap_or_default(),
                    provider.base_url.clone().unwrap_or_default(),
                    provider.api.clone(),
                ));
            }
        }

        // 3) active_model fallback.
        let active = configs.active_model.as_ref()?;
        let provider = configs.find_provider(&active.provider_id)?;
        Some((
            active.provider_id.clone(),
            active.model_id.clone(),
            provider.api_key.clone().unwrap_or_default(),
            provider.base_url.clone().unwrap_or_default(),
            provider.api.clone(),
        ))
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test --lib providers::service::tests::role_config 2>&1 | tail -20`
Expected: PASS (4 tests).

- [ ] **Step 5: Reimplement `get_chat_llm_config` and `get_ingestion_llm_config` on top of the generic resolver**

Replace the entire body of `get_chat_llm_config` (lines 168-202) with:

```rust
    /// Resolve the chat-role model → active_model fallback chain.
    /// Thin wrapper over [`Self::get_role_llm_config`] with role `"chat"`.
    /// Returns `(provider_id, model, api_key, base_url, api_override)`.
    pub async fn get_chat_llm_config(
        &self,
    ) -> Option<(String, String, String, String, Option<crate::providers::types::ApiType>)> {
        self.get_role_llm_config("chat").await
    }
```

Replace the entire body of `get_ingestion_llm_config` (lines 206-236) with:

```rust
    /// Resolve the ingestion-role model. Thin wrapper over
    /// [`Self::get_role_llm_config`] with role `"ingestion"`; drops the
    /// `api_override` field for callers that don't need it.
    /// NOTE: unlike the pre-S0 version, ingestion now inherits the `chat`
    /// role assignment before falling back to `active_model` (the generic
    /// resolver's `role → chat → active` chain). This is intentional and
    /// strictly more permissive — it never changes a configured-ingestion
    /// or fully-unconfigured outcome.
    pub async fn get_ingestion_llm_config(&self) -> Option<(String, String, String, String)> {
        self.get_role_llm_config("ingestion")
            .await
            .map(|(pid, mid, key, url, _api)| (pid, mid, key, url))
    }
```

- [ ] **Step 6: Run the full provider-service test module + build**

Run: `cd src-tauri && cargo test --lib providers::service 2>&1 | tail -20`
Expected: PASS (existing 3 tests + 4 new tests).
Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: no output (no errors).

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/providers/service.rs
git commit -m "feat(providers): generic get_role_llm_config resolver

Add ProviderService::get_role_llm_config(role) with a role → chat →
active_model fallback chain; reimplement get_chat_llm_config and
get_ingestion_llm_config on top of it. Behavior-preserving for chat;
ingestion now inherits the chat assignment before active. No callers
changed. Unblocks per-scenario Memory-OS routing (S0)."
```

---

## Task 2: Route Memory-OS passes by cost_tag

**Files:**
- Modify: `src-tauri/src/memory_graph/memory_os_llm.rs` (add `role_for_cost_tag` free function; change `complete_text` lines 159-163 to resolve by role; augment `[VERIFY]` log lines 204-214; add a test in the existing `mod tests`)

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` block in `src-tauri/src/memory_graph/memory_os_llm.rs` (after `trait_object_dispatches_correctly`):

```rust
    #[test]
    fn cost_tags_map_to_expected_roles() {
        assert_eq!(role_for_cost_tag("memory_consolidation"), "summarizer");
        assert_eq!(role_for_cost_tag("memory_wiki"), "summarizer");
        assert_eq!(role_for_cost_tag("memory_reflection"), "compiler");
        assert_eq!(role_for_cost_tag("memory_daydream"), "compiler");
        assert_eq!(role_for_cost_tag("memory_promotion"), "utility");
        assert_eq!(role_for_cost_tag("memory_entity_synth"), "utility");
        assert_eq!(role_for_cost_tag("memory_lint"), "utility");
        // Unknown tags default to the safe "chat" role.
        assert_eq!(role_for_cost_tag("something_new"), "chat");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --lib memory_os_llm::tests::cost_tags_map 2>&1 | tail -15`
Expected: FAIL — `cannot find function role_for_cost_tag in this scope`.

- [ ] **Step 3: Add the `role_for_cost_tag` mapping function**

In `src-tauri/src/memory_graph/memory_os_llm.rs`, add this free function just above the `#[async_trait] impl MemoryOsLlm for MemoryOsLlmClient` block (currently line 150):

```rust
/// Map a Memory-OS `cost_tag` to the model role its completion should use.
/// Unmapped tags fall back to `"chat"` (fail-safe, never fail-dead).
/// Roles resolve through [`ProviderService::get_role_llm_config`]'s
/// `role → chat → active` fallback chain.
fn role_for_cost_tag(tag: &str) -> &'static str {
    match tag {
        "memory_consolidation" | "memory_wiki" => "summarizer",
        "memory_reflection" | "memory_daydream" => "compiler",
        "memory_promotion" | "memory_entity_synth" | "memory_lint" => "utility",
        _ => "chat",
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test --lib memory_os_llm::tests::cost_tags_map 2>&1 | tail -15`
Expected: PASS.

- [ ] **Step 5: Route `complete_text` through the role and add a concise routing log**

> NOTE (drift): the temporary `[VERIFY]` `tracing::warn!` block referenced in
> the spec was already removed upstream (the `pi/memory-llm-tz-fixes` commits).
> So instead of *augmenting* it, add a lightweight permanent `tracing::debug!`
> right after role resolution — this keeps the spec's observability promise
> (`role` + `resolved_model`) without resurrecting temporary diagnostics.

In `complete_text`, replace the resolver call (lines 159-163):

```rust
        let (provider_id, model, api_key, base_url, _) = self
            .provider_service
            .get_chat_llm_config()
            .await
            .ok_or(MemoryOsLlmError::NoProvider)?;
```

with:

```rust
        let role = role_for_cost_tag(cost_tag);
        let (provider_id, model, api_key, base_url, _) = self
            .provider_service
            .get_role_llm_config(role)
            .await
            .ok_or(MemoryOsLlmError::NoProvider)?;

        // Per-scenario routing observability (S0): proves which role a given
        // Memory-OS pass resolved to, and which model it landed on.
        tracing::debug!(
            cost_tag,
            role,
            resolved_model = %model,
            "memory_os_llm: routed completion to role"
        );
```

- [ ] **Step 6: Build + run the memory_os_llm test module**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: no output.
Run: `cd src-tauri && cargo test --lib memory_os_llm 2>&1 | tail -20`
Expected: PASS (existing mock tests + new mapping test).

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/memory_graph/memory_os_llm.rs
git commit -m "feat(memory): route Memory-OS passes by cost_tag → role

Map each cost_tag (consolidation/wiki → summarizer, reflection/daydream
→ compiler, promotion/entity_synth/lint → utility, else chat) and resolve
via get_role_llm_config. Add role/resolved_model to the [VERIFY] log so
per-scenario routing is observable. Completes S0."
```

---

## Final verification

- [ ] **Run the full affected test surface**

Run: `cd src-tauri && cargo test --lib providers::service memory_os_llm 2>&1 | tail -25`
Expected: all PASS.

- [ ] **Confirm no unexpected symbol drift before any PR**

Run: `gitnexus_detect_changes()` and confirm only `get_role_llm_config`, `get_chat_llm_config`, `get_ingestion_llm_config`, `role_for_cost_tag`, and `complete_text` are affected.

- [ ] **Manual smoke (optional, requires running app)**

In Settings → 智能 → 模型分配, assign a *different* model to `摘要模型 (summarizer)` than to `主对话模型 (chat)`. Trigger a consolidation pass (cadence is already lowered on this branch lineage). Confirm the `memory_os_llm: routed completion to role` debug log shows `role=summarizer` and `resolved_model=<the summarizer assignment>` (run with `RUST_LOG=...=debug`).

---

## Notes / scope guardrails

- `utility_large` intentionally has no caller in S0 — it falls back to `chat`. A real heavy-reasoning call site is a later sub-project, not S0.
- No pi-engine / legacy main-turn change: those keep calling `get_chat_llm_config()`, which is now a wrapper but behaves identically.
- No migration, no Tauri command, no frontend change.
- Do **not** revert the `verify-temp` cadence/token tweaks here — they live on a separate concern and are reverted by the user when verification concludes.
