# S3 — First-launch onboarding for the local model

**Date:** 2026-06-03
**Status:** Design (decisions locked in brainstorming; pending review)
**Sub-project of:** Local MiniCPM initiative (S0 ✓ #58 · S1 ✓ #60 · S2 · **S3** · S4)
**Depends on:** S1 (engine + `download_local_model` + `is_local_model_present` + role assignment) and S2 (robust `download_model` + `probe_download_sources`). S3 orchestrates them into a guided first-run flow.

---

## Problem & goal

Deliver the user's "无需手动配置:首次启动会引导完成环境检查、模型下载和模型预热" — a guided, skippable step in the existing onboarding that env-checks the machine, downloads MiniCPM (via S2), warms it up, and auto-assigns it to the cheap background roles.

## Decisions (brainstorming)

| Topic | Choice |
|---|---|
| Entry point | **Add one step to the existing `OnboardingView`** (Welcome → API Key → **Local Model (optional)** → Theme → Done). |
| After warmup | **Auto-assign** `local-minicpm/minicpm5-1b` to the `summarizer` + `utility` roles (editable later in Settings). |
| Skippable | **Yes** — users can skip local-model setup (cloud-only is fine); re-runnable from Settings → MiniCPM. |
| Env checks | **Disk space · RAM · Metal/GPU · network reachability** (all four). |

## Architecture

### Backend: an env-check command + a one-call setup orchestrator
`local_llm/preflight.rs` (new):
```rust
pub struct EnvReport {
    pub disk_free_bytes: u64,
    pub disk_ok: bool,            // >= quant size * 1.5 headroom
    pub ram_total_bytes: u64,
    pub ram_ok: bool,            // >= ~2 GB free heuristic for 1B Q4
    pub metal_available: bool,    // macOS only; false → CPU fallback (still ok)
    pub network: NetworkReport,   // per-source reachability + chosen source (from S2 probe)
}
pub enum CheckStatus { Ok, Warn, Fail }
```
- `check_environment(quant) -> EnvReport` — disk via `statvfs`/`sysinfo`; RAM via `sysinfo`; Metal via a cheap mistralrs/metal availability probe (or `cfg(target_os="macos")` + device query); network via S2's `probe_download_sources`.
- Tauri commands: `check_local_model_environment(quant) -> EnvReport`; reuse S2 `download_local_model` (progress events) and `warmup_local_model()` (calls `engine.complete` once with a trivial prompt to load+JIT); `assign_local_model_to_roles()` (sets `role_models[summarizer|utility] = "local-minicpm/minicpm5-1b"` via `ProviderService::set_role_model`).

### Frontend: a new onboarding step component
`ui/src/components/onboarding/steps/LocalModelStep.tsx`:
- Sub-states: `intro → checking → (report) → downloading → warming → done | skipped | blocked`.
- Renders the `EnvReport` as a checklist (✅/⚠️/❌ per item) with human guidance (e.g. "需要 ~1GB 磁盘", "未检测到 Metal,将用 CPU(较慢)").
- **Disk Fail** blocks download (hard requirement); RAM/Metal/network warns but allows proceed (Metal-absent → CPU; network-absent → "稍后在设置里下载"). 
- Buttons: `下载并启用` (runs download → warmup → auto-assign), `跳过`.
- Wire into `OnboardingView`'s step machine + the `View`/step enum; persist an `onboarding.localModelDone|skipped` flag so it doesn't re-prompt.

### Re-entry from Settings
Settings → MiniCPM gets a "运行首启检查 / 下载本地模型" affordance that reuses the same `LocalModelStep` logic (extract the orchestration into a hook `useLocalModelSetup` shared by both surfaces).

## Data flow
```
Onboarding LocalModelStep
  → check_local_model_environment(quant)            [S3 backend]
  → (disk ok) download_local_model(quant, source)   [S2] ──progress──▶ UI bar
  → warmup_local_model()                            [S1 engine.complete]
  → assign_local_model_to_roles()                   [S0 set_role_model ×2]
  → persist onboarding.localModelDone               [frontend settings]
```

## Error handling
- Disk Fail → block download, show required vs free, offer skip.
- Download error (from S2 typed errors) → surface message + retry/skip; never leave onboarding stuck.
- Warmup error (e.g. corrupt model) → mark not-done, suggest re-download; don't auto-assign.
- Auto-assign is best-effort; failure logs + leaves roles unchanged (user can assign in Settings).

## Testing
- `preflight.rs`: `disk_ok`/`ram_ok` threshold logic (unit, injected numbers); `EnvReport` serialization.
- Command wiring smoke (no real download).
- Frontend: `LocalModelStep` state machine via vitest (mock the bridge commands): intro→checking→report; disk-fail blocks; skip path sets flag; happy path calls download→warmup→assign in order.

## Scope guardrails
- **In S3:** env-check + the onboarding step + skip + auto-assign + Settings re-entry hook.
- **Not S3:** the download internals (S2), the pet (S4), multi-model management UI.

## Risks
- Cross-platform env checks: `sysinfo` covers disk/RAM portably; Metal check is macOS-specific (gate with `cfg`). Confirm `sysinfo` is/should-be a dep.
- Onboarding step-machine shape: must read the real `OnboardingView` step enum at implementation (it currently has Welcome/API Key/Theme/Completion) and insert without breaking existing navigation.
