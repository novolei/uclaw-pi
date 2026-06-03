# First-Launch Onboarding (S3) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development or superpowers:executing-plans. Steps use `- [ ]`.
> **Status:** task-level roadmap. Implement AFTER S1 (#60) + S2 merge. At execution, detail-fill code against the merged `local_llm` module and the real `OnboardingView` step machine (inspect it first).

**Goal:** A skippable onboarding step that env-checks the machine, downloads MiniCPM (S2), warms it up, and auto-assigns it to `summarizer`+`utility`.

**Architecture:** New `local_llm/preflight.rs` (env checks) + Tauri commands (`check_local_model_environment`, `warmup_local_model`, `assign_local_model_to_roles`). New `LocalModelStep.tsx` inserted into `OnboardingView`; orchestration extracted to a `useLocalModelSetup` hook reused by Settings → MiniCPM.

**Spec:** `docs/superpowers/specs/2026-06-03-s3-first-launch-onboarding-design.md`

**Branch:** `pi/s3-first-launch-onboarding` — stacked on S2 until merges, then rebase onto main.

---

## File structure

| File | Responsibility |
|---|---|
| `src-tauri/src/local_llm/preflight.rs` | `EnvReport`, `check_environment(quant)` (disk/RAM/Metal/network) |
| `src-tauri/src/commands/local_llm.rs` | `check_local_model_environment`, `warmup_local_model`, `assign_local_model_to_roles` |
| `ui/src/components/onboarding/steps/LocalModelStep.tsx` | the onboarding step UI + state machine |
| `ui/src/hooks/useLocalModelSetup.ts` | shared orchestration (check→download→warmup→assign) |
| `ui/src/components/onboarding/OnboardingView.tsx` | insert the step + persist done/skip flag |
| `ui/src/features/settings/components/...MiniCPM...` | "运行首启检查/下载" re-entry using the hook |

## Tasks (ordered; detail-fill at execution)

### Task 1: `preflight.rs` env checks (backend, mostly pure)
- `EnvReport` + `CheckStatus`. `check_environment(quant)`: disk free (via `sysinfo`/`statvfs`) vs `quant.expected_size()*1.5`; RAM via `sysinfo`; Metal via `cfg(macos)` + a cheap device probe (or reuse mistralrs availability); network via S2 `probe_download_sources`.
- Tests: `disk_ok`/`ram_ok` threshold logic with injected numbers; serialization. (Confirm `sysinfo` dep.)
- Commit.

### Task 2: backend commands
- `check_local_model_environment(quant) -> EnvReport`; `warmup_local_model()` (calls `engine.complete("","ok",1,0.0)` — load+JIT, ignore output); `assign_local_model_to_roles()` (calls `ProviderService::set_role_model("summarizer", Some("local-minicpm/minicpm5-1b"))` and same for `"utility"`). Register all in `generate_handler!`.
- Test: `assign_*` sets both roles (extend provider-service tests, or an integration check). Commit.

### Task 3: `useLocalModelSetup` hook
- Encapsulates the bridge calls + the state machine (`intro|checking|report|downloading|warming|done|skipped|blocked`), subscribing to `local-model:download-progress`. Returns state + actions (`runChecks`, `downloadAndEnable`, `skip`).
- vitest with mocked bridge: order of calls (check→download→warmup→assign); disk-fail → blocked; skip → flag set. Commit.

### Task 4: `LocalModelStep.tsx`
- Renders the env checklist (✅/⚠️/❌ + guidance), progress bar, `下载并启用`/`跳过` buttons; consumes `useLocalModelSetup`. Disk-Fail disables download; Metal/network warns-but-proceeds.
- vitest: renders report; blocked state hides download; happy path. Commit.

### Task 5: wire into `OnboardingView`
- **Inspect the real step enum first.** Insert `LocalModelStep` after the API-Key step; add a persisted `onboarding.localModelDone|skipped` flag (where existing onboarding completion is stored) so it doesn't re-prompt. Don't break existing nav.
- vitest: step appears in sequence; completing/skipping advances + sets flag. Commit.

### Task 6: Settings → MiniCPM re-entry
- Add a "运行首启检查 / 下载本地模型" panel reusing `useLocalModelSetup`. Follow existing settings component patterns.
- `npx tsc --noEmit` clean. Commit.

## Verification
- `cargo test --lib local_llm::preflight` + `cargo build` clean.
- `ui`: `npx tsc --noEmit`, vitest for the hook + step.
- Manual: fresh profile → onboarding shows the step; happy path downloads, warms, auto-assigns (verify `role_models` in `providers.json` + the S0 routing log); skip path proceeds cloud-only; re-run from Settings works.

## Notes
- Stacked on S2; rebase onto main when S1+S2 merge.
- `sysinfo` likely a new dep → commit Cargo.lock with it.
