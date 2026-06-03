# S1 — Local MiniCPM runtime (mistral.rs, in-process)

**Date:** 2026-06-03
**Status:** Design (approved in brainstorming, pending spec review)
**Sub-project of:** Local MiniCPM + desk-pet initiative (S0 ✓ → **S1** → S2 → S3 → S4)
**Depends on:** S0 (per-scenario model routing — PR #58). With S0 landed, a model
assigned to the `summarizer` / `utility` roles is actually invoked at runtime.

---

## Problem & goal

uClaw has no local LLM inference today (only memU FastEmbed for embeddings). Every
LLM call — including the cheap background Memory-OS passes (consolidation, reflection,
lint, …) — hits a hosted, paid model. We want a **Rust-native, in-process** local model
(`openbmb/MiniCPM5-1B-GGUF`, Apache-2.0) that can be assigned to the `summarizer` /
`utility` roles so background work runs locally at $0 token cost.

S1 delivers: (1) a verified in-process inference engine, (2) a `LocalMistralRsProvider`
that plugs into the existing `LlmProvider` abstraction, (3) lazy-load + idle-unload
lifecycle, (4) a **minimal single-source (ModelScope) downloader**, and (5) registry/UI
surfacing so the local model is assignable to roles.

## Decisions (from brainstorming)

| Decision | Choice | Rationale |
|---|---|---|
| Inference engine | **mistral.rs** (pure Rust, in-process API) | User wants Rust-native; mistral.rs has the strongest LoRA story (needed for S4 pet personas) and a Metal backend |
| Model | `openbmb/MiniCPM5-1B-GGUF`, **Q4_K_M** (688 MB) | Plain `LlamaForCausalLM` GGUF arch (OpenBMB: "works with vanilla llama.cpp, no custom kernels") → loads via mistral.rs generic GGUF/llama loader; Q4_K_M is the balanced 1B quant |
| Load policy | **Lazy load + idle unload** (10 min) | Don't pay RAM/startup unless used |
| Download source | **ModelScope only** (Q4_K_M) | User is in CN; ModelScope faster/more reliable. HF fallback + smart source = S2 |
| Engine handle | **Process-global singleton** (`OnceLock<Arc<LocalLlmEngine>>`) | `create_provider(&cfg)` is stateless; model is inherently single-instance — a singleton avoids threading `AppState` through every provider-creation call site |
| `stream` | **Not in S1** (only `complete`) | Memory-OS uses `complete`; streaming matters for the pet (S4) |

## Headline de-risk

MiniCPM5-text is **not** in mistral.rs's named support matrix (only multimodal
MiniCPM-O is). Because the arch is plain Llama, it *should* load via the generic
GGUF/llama path — but this is unverified. **Therefore Task 1 is a spike that must pass
before any other S1 work begins.** If mistral.rs cannot load MiniCPM5-1B-GGUF, S1 stops
and we revisit the engine choice (fallback: `llama-cpp-2` Rust bindings).

## Architecture

New module `src-tauri/src/local_llm/`:

```
local_llm/
  mod.rs        — module wiring + the global engine handle (OnceLock) + init fn
  engine.rs     — LocalLlmEngine: lazy load, warmup, idle-unload, async complete()
  provider.rs   — LocalMistralRsProvider: impl LlmProvider (complete; stream = unsupported in S1)
  download.rs   — minimal ModelScope downloader + paths
```

### Engine (`engine.rs`)

```rust
pub struct LocalLlmEngine {
    inner: tokio::sync::RwLock<EngineState>,
    model_path: std::path::PathBuf,        // ~/.uclaw-pi/models/minicpm5-1b/<file>.gguf
    idle_unload_after: std::time::Duration, // 10 min
}

enum EngineState {
    Unloaded,
    Loaded { model: LoadedMistralModel, last_used: std::time::Instant },
}
```

- `async fn complete(&self, system: &str, user: &str, max_tokens: u32, temperature: f32) -> Result<LocalCompletion, LocalLlmError>`:
  1. fast path: read lock, if `Loaded`, run inference + bump `last_used`.
  2. slow path: write lock, if `Unloaded`, check the GGUF exists (else `Err(ModelMissing)`),
     load via mistral.rs, warmup (generate 1 token), store `Loaded`, then run inference.
- `async fn ensure_unloaded_if_idle(&self)`: if `Loaded` and `last_used` older than
  `idle_unload_after`, transition to `Unloaded` (drop the model, free RAM).
- `fn is_loaded(&self) -> bool` for status surfacing.
- `LocalCompletion { text, input_tokens, output_tokens, model: String }` mirrors what the
  `LlmProvider` adapter needs.

mistral.rs specifics (validated/decided in the Task 1 spike): build the model via its
in-process Rust builder for a GGUF/quantized-llama model, Metal feature enabled on macOS.
The exact builder API + feature flags are pinned during the spike and recorded in the plan.

### Global handle (`mod.rs`)

```rust
static ENGINE: std::sync::OnceLock<std::sync::Arc<LocalLlmEngine>> = std::sync::OnceLock::new();

/// Initialize the local engine handle (called once at startup, after data_dir is known).
pub fn init_local_engine(data_dir: &std::path::Path) -> std::sync::Arc<LocalLlmEngine>;

/// Get the initialized engine, or None if init wasn't called.
pub fn local_engine() -> Option<std::sync::Arc<LocalLlmEngine>>;
```

Initialization does **not** load the model (lazy) — it only constructs the handle and
resolves the model path. Called in app startup (`app.rs`) alongside the other services.

### Provider (`provider.rs`)

```rust
pub struct LocalMistralRsProvider { engine: std::sync::Arc<LocalLlmEngine>, model_id: String }

#[async_trait]
impl LlmProvider for LocalMistralRsProvider {
    async fn complete(&self, messages, _tools, config) -> Result<RespondOutput> {
        // map ChatMessages → (system, user); ignore tools (1B text model);
        // call engine.complete(...); return RespondOutput::Text { text, metadata(usage) }
    }
    async fn stream(&self, ...) -> Result<...> {
        Err(/* unsupported in S1 — implemented in S4 for the pet */)
    }
}
```

### Routing seam (`llm/mod.rs`, `providers/registry.rs`, `providers/types.rs`)

- Add `ApiType::LocalMistralRs` variant.
- In `create_provider(&cfg)` (the existing `resolve_api`/dispatch): when `api == LocalMistralRs`,
  build `LocalMistralRsProvider` from the global `local_engine()` (error if not initialized).
- Register a built-in provider in `registry.rs`:
  ```rust
  KnownProvider {
      id: "local-minicpm".into(),
      display_name: "MiniCPM (本地)".into(),
      auth_type: AuthType::None,
      default_base_url: "".into(),            // unused for in-process
      default_api: ApiType::LocalMistralRs,
      supports_models: false,                 // static model list (see below)
  }
  ```
- `ProviderService::list_models` for `local-minicpm` returns a static `[minicpm5-1b]`
  (no network call) so it appears in Settings → 模型分配 and can be assigned to roles
  via the S0 `role_models` mechanism (`model_ref = "local-minicpm/minicpm5-1b"`).

### Download (`download.rs`)

- `model_dir(data_dir) -> ~/.uclaw-pi/models/minicpm5-1b/`, `model_file() -> <…>-Q4_K_M.gguf`.
- `async fn download_from_modelscope(progress: impl Fn(u64,u64)) -> Result<PathBuf>`:
  stream GET the Q4_K_M GGUF from ModelScope
  (`https://www.modelscope.cn/models/OpenBMB/MiniCPM5-1B-GGUF/resolve/master/<file>`),
  write to `<file>.gguf.part`, verify expected size, atomic rename to `<file>.gguf`.
  (Exact filename + URL confirmed during implementation against the live ModelScope repo.)
- Tauri command `download_local_model` (registered in `tauri_commands.rs` + `invoke_handler!`
  + `commands/`) that runs the download and emits progress events (`local-model:download-progress`).
  Manual trigger for S1; the auto first-launch wizard is S3.
- `is_local_model_present(data_dir) -> bool` Tauri command for the UI to gate the
  download button / show status.

## Data flow (summarizer routed to local)

```
Memory-OS consolidation pass
  → MemoryOsLlmClient::complete_text("memory_consolidation", …)        [S0]
  → role_for_cost_tag → "summarizer"                                    [S0]
  → ProviderService::get_role_llm_config("summarizer")                  [S0]
      → role_models["summarizer"] = "local-minicpm/minicpm5-1b"
  → create_provider(cfg{ api: LocalMistralRs })                         [S1]
  → LocalMistralRsProvider::complete → LocalLlmEngine::complete         [S1]
      → (lazy load + warmup if needed) → mistral.rs inference (Metal)   [S1]
  → RespondOutput::Text                                                  [S1]
```

## Error handling

- `LocalLlmError::ModelMissing` when the GGUF isn't on disk → surfaced to the caller; the
  Memory-OS path treats it like any provider error (the pass is skipped, logged). The UI
  uses `is_local_model_present` to prompt a download. No panics.
- Load failure (corrupt GGUF, unsupported arch) → `LocalLlmError::Load(String)`; if it
  happens for MiniCPM5 specifically, that's the Task 1 spike's job to catch first.
- Cost: local completions record `cost_usd = 0`. Confirm `agent::types::calculate_cost`
  returns 0 for an unknown/local model id (it already does for unknown ids); if not, add a
  guard so local inference never books spend.

## Testing

- **Task 1 spike** (gated, `#[ignore]`): with the GGUF manually placed, load via mistral.rs
  and assert a non-empty generation + that Metal is active. Run manually:
  `cargo test --lib local_llm::spike -- --ignored --nocapture`.
- **Engine state machine** (unit, no model): `Unloaded → ModelMissing` error when file
  absent; `ensure_unloaded_if_idle` transitions after a stubbed-old `last_used`. (Use a tiny
  fake model path; do not load a real model in CI.)
- **Provider adapter** (unit): `LocalMistralRsProvider::stream` returns the unsupported
  error in S1; `complete` maps a stubbed `LocalCompletion` → `RespondOutput::Text` with usage.
- **role_for_cost_tag / get_role_llm_config** already covered by S0.
- **Download** (unit): URL/path construction + atomic-rename logic against a local mock
  server (or a pure path/[]-builder unit test); do NOT hit the network in CI.

## Files

| File | Change |
|---|---|
| `src-tauri/Cargo.toml` | add `mistralrs` (+ `metal` feature on macOS); pin version in plan |
| `src-tauri/src/local_llm/mod.rs` | new — global engine handle + init |
| `src-tauri/src/local_llm/engine.rs` | new — `LocalLlmEngine` lifecycle |
| `src-tauri/src/local_llm/provider.rs` | new — `LocalMistralRsProvider` |
| `src-tauri/src/local_llm/download.rs` | new — ModelScope downloader + paths |
| `src-tauri/src/providers/types.rs` | add `ApiType::LocalMistralRs` |
| `src-tauri/src/providers/registry.rs` | register `local-minicpm` provider |
| `src-tauri/src/providers/service.rs` | static `list_models` for `local-minicpm` |
| `src-tauri/src/llm/mod.rs` | route `LocalMistralRs` in `create_provider` |
| `src-tauri/src/tauri_commands.rs` + `main.rs` | `download_local_model`, `is_local_model_present` commands + register; idle-unload reaper in Stage 3; `init_local_engine` at startup |
| `src-tauri/src/lib.rs` | `mod local_llm;` |

## Scope guardrails (YAGNI)

- **In S1:** engine + provider + lazy/idle lifecycle + minimal ModelScope download +
  registry/UI listing + role-assignability.
- **Not in S1:** streaming (S4), tools, HF fallback / network-aware source selection /
  rich progress UI (S2), first-launch onboarding wizard + env check + auto warmup (S3),
  desk-pet chat companion + LoRA persona adapters (S4).

## Risks

1. **mistral.rs may not load MiniCPM5-1B-GGUF** — mitigated by the Task 1 spike gate;
   fallback is `llama-cpp-2`.
2. **Build cost** — mistral.rs pulls candle + a large dep tree; first compile noticeably
   slower, bigger binary. Accepted (user chose pure Rust). Pin a specific version; consider
   a cargo feature to make the whole local-LLM stack optional at build time if compile time
   becomes painful.
3. **Metal maturity** — mistral.rs Metal is less exercised than CUDA; the spike validates it.
4. **tokio version overlap** with Tauri — verify mistral.rs's tokio is compatible during the
   spike; resolve via Cargo before deeper work.

## Follow-on

- **S2:** HF fallback + network-aware source selection + resumable download + progress UI.
- **S3:** first-launch onboarding (env check → download → warmup → auto-assign to roles).
- **S4:** desk-pet chat companion (floating bubble), streaming, LoRA persona adapter
  switch/import, porting MiniCPM-Desk-Pet characters into the existing pet roster.
