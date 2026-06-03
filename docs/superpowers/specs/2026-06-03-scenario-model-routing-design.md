# S0 — Make per-scenario model routing real

**Date:** 2026-06-03
**Status:** Design (approved in brainstorming, pending spec review)
**Author:** brainstorming session
**Sub-project of:** Local MiniCPM + desk-pet initiative (S0 → S1 → S2 → S3 → S4)

---

## Problem

The Settings → 智能 → 模型分配 panel lets the user assign a model+provider to five
scenarios (`chat`, `utility`, `utility_large`, `summarizer`, `compiler`). The config
plumbing is complete and persists to `~/.uclaw-pi/providers.json` under `role_models`,
**but the runtime only ever consults the `chat` role.** The other four roles are
cosmetic UI: no runtime code path reads `utility` / `utility_large` / `summarizer` /
`compiler`.

Evidence (investigation 2026-06-03):

- `ProviderService::get_chat_llm_config()` checks only `role == "chat"`, then falls back
  to `active_model` — `providers/service.rs:166`.
- Pi-engine prompt path resolves via `get_chat_llm_config()` — `tauri_commands.rs:1519`.
- Legacy agent path — `tauri_commands.rs:1732` — same.
- All Memory-OS passes (reflection, promotion, consolidation, daydream, wiki, entity
  synth, lint) go through `MemoryOsLlmClient::complete_text`, which hardcodes
  `get_chat_llm_config()` — `memory_graph/memory_os_llm.rs:161`.
- `get_role_models()` has exactly one caller: the UI bridge. Zero runtime consumers of
  the non-chat roles.

This blocks the larger initiative: S1 wants a local MiniCPM model assigned to the
`utility` (轻工具) and `summarizer` (记忆摘要) roles — exactly the roles that currently
route nowhere. S0 makes those roles real so a model assigned to them is actually called.

## Goal

Make the runtime honor the four currently-dead roles by routing the Memory-OS LLM passes
to the appropriate role, with a safe fallback chain. No UI changes (the picker already
works). No new failure modes. No change to the user-facing main agent turn (it correctly
uses `chat` today).

## Non-goals (YAGNI)

- Not wiring `utility_large` to a synthetic call site. The agent loop has no distinct
  "heavy reasoning tool" LLM call; inventing one is out of scope. `utility_large` stays
  reserved and falls back to `chat`.
- Not changing the pi-engine main-turn path or legacy turn path (both correctly use
  `chat`).
- Not building any Settings UI (the picker is done).
- Not reverting the in-progress `verify-temp` debugging tweaks (cadence/token changes in
  `engine_sink.rs` and `reflection_service.rs`). Those are reverted separately by the
  user when verification concludes. S0 only *augments* the existing `[VERIFY]` log.

## Design

### 1. One generic resolver

Add a single resolver on `ProviderService` and express the existing ones in terms of it:

```rust
// providers/service.rs

/// Resolve the LLM config for a given role with a graceful fallback chain.
/// Priority: role_models[role] → role_models["chat"] → active_model.
/// Returns (provider_id, model, api_key, base_url, api_override).
pub async fn get_role_llm_config(
    &self,
    role: &str,
) -> Option<(String, String, String, String, Option<ApiType>)>;
```

- `get_chat_llm_config()` is reimplemented as `get_role_llm_config("chat")` (preserves
  every existing caller's behavior exactly — pi engine, legacy turn, `tauri_commands`).
- `get_ingestion_llm_config()` is reimplemented as a thin adapter over
  `get_role_llm_config("ingestion")` (it returns a 4-tuple without `api_override`; keep
  that shape by dropping the 5th field, so its callers are untouched).
- Fallback chain rationale: an unassigned role degrades to the user's chat model, then to
  the global active model. A role can never *break* a call — worst case it behaves
  exactly like today.

Routing knowledge now lives in exactly one method.

### 2. Memory-OS cost_tag → role dispatch

`MemoryOsLlmClient::complete_text(cost_tag, …)` already receives a `cost_tag`. Add a pure
mapping function and resolve config per call:

```rust
// memory_graph/memory_os_llm.rs

/// Map a Memory-OS cost tag to the model role it should use.
fn role_for_cost_tag(tag: &str) -> &'static str {
    match tag {
        "memory_consolidation" | "memory_wiki"          => "summarizer",
        "memory_reflection"    | "memory_daydream"      => "compiler",
        "memory_promotion" | "memory_entity_synth" | "memory_lint" => "utility",
        _ => "chat",
    }
}
```

In `complete_text`, replace the hardcoded `get_chat_llm_config()` with:

```rust
let role = role_for_cost_tag(cost_tag);
let (provider_id, model, api_key, base_url, _) = self
    .provider_service
    .get_role_llm_config(role)
    .await
    .ok_or(MemoryOsLlmError::NoProvider)?;
```

| cost_tag | role | rationale |
|---|---|---|
| `memory_consolidation`, `memory_wiki` | `summarizer` | heavy text compression / 记忆摘要 |
| `memory_reflection`, `memory_daydream` | `compiler` | frequent background "fast" passes |
| `memory_promotion`, `memory_entity_synth`, `memory_lint` | `utility` | small one-shots / 轻工具 |
| (unmapped) | `chat` | safe default |

The table is the single tuning knob. New cost tags default to `chat` until explicitly
mapped — fail-safe, never fail-dead.

### 3. Observability

Augment the existing `[VERIFY] complete_text raw response` `tracing::warn!` in
`complete_text` with two fields: `role` (the dispatched role) and `resolved_model` (the
model the resolver actually returned). This proves, per pass, that routing is no longer
cosmetic, and dovetails with the verify-temp debugging already on this branch. The
`[VERIFY]` log itself is preserved (not reverted) per the branch decision.

### 4. Error handling

No new failure modes. The fallback chain guarantees a config whenever *any* model is
configured. If nothing at all is configured, behavior is identical to today
(`MemoryOsLlmError::NoProvider`).

### 5. Testing

Inline `#[cfg(test)]` unit tests (repo convention — `cargo test --lib`):

- `role_for_cost_tag`: table-driven — each known tag maps to its role; unknown → `chat`.
- `get_role_llm_config` fallback (extend existing `service.rs` tests):
  - role assigned → returns the role's model
  - role null, chat set → returns chat model
  - role null, chat null, active set → returns active model
  - nothing set → `None`
- Memory-OS dispatch (using the existing test mock in `memory_os_llm.rs`): assert
  `complete_text("memory_consolidation", …)` resolves the `summarizer` assignment rather
  than `chat` when the two differ.

## Files touched

| File | Change |
|---|---|
| `src-tauri/src/providers/service.rs` | Add `get_role_llm_config`; reimplement `get_chat_llm_config` + `get_ingestion_llm_config` on top of it; extend tests |
| `src-tauri/src/memory_graph/memory_os_llm.rs` | Add `role_for_cost_tag`; route `complete_text` through it; add `role`/`resolved_model` to `[VERIFY]` log; add dispatch test |

No migrations. No Tauri command changes. No frontend changes.

## Verification

- `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
- `cd src-tauri && cargo test --lib providers::service`
- `cd src-tauri && cargo test --lib memory_os_llm`
- Manual: assign distinct models to `summarizer` vs `chat` in Settings, trigger a
  consolidation pass, confirm the `[VERIFY]` log shows `role=summarizer` and the
  `resolved_model` matches the summarizer assignment.

## Impact / blast radius

- `get_chat_llm_config` is called by the pi engine, legacy agent path, and Memory-OS.
  Reimplementing it as `get_role_llm_config("chat")` is behavior-preserving — same
  priority order, same return shape. Run `gitnexus_impact` on `get_chat_llm_config`
  before editing and report the blast radius per repo policy.
- The only *behavioral* change is that Memory-OS passes now follow their mapped role
  instead of always `chat`. Because of the fallback chain, users who left the new roles
  unassigned see no change.

## Follow-on

S1 (local MiniCPM runtime) depends on S0: once routing is real, a `local-minicpm`
provider can be assigned to `summarizer` + `utility` and will actually be invoked for the
background memory work, saving tokens on the hosted chat model.
