# Memory P1 UI Backfill Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give P1's memory engine (`memory_core`) a complete, premium-quality UI: engine status + models toggle + download progress + index status in Settings, and "memories used this turn" in chat — with the backend IPC/events they need.

**Architecture:** `MemoryCore` gains an observable status (models, download progress, index coverage) updated by `models.rs` progress callbacks and the sync service, pushed to the UI through a `memory-core:status` event and readable via `memory_core_get_status`. The models toggle applies at runtime (load/unload, start download) instead of only at startup. The recall event carries MemoryCore hits. Frontend follows the existing local-model download pattern and the settings primitives, on theme tokens only.

**Tech Stack:** Rust (tauri v2 commands/events), React + TypeScript, Tailwind + CSS variables, shadcn/Radix primitives, `motion/react`, jotai, vitest + Testing Library, Playwright, `npm run dev:mock-tauri` for browser QA.

**Spec:** `docs/superpowers/specs/2026-11-09-unified-memory-design.md` (§4 读路径/控制, §5 P1 row). Recon: side-session notes `2026-11-09-p2-recon-frontend.md`.

## Global Constraints

- Every new Tauri command: defined in `src-tauri/src/commands/*.rs` (or `tauri_commands.rs`) AND registered in `main.rs` `generate_handler!` (forgetting the macro compiles but fails at runtime).
- Frontend colors only via theme tokens (CSS variables in `ui/src/styles/globals.css`, mapped in `ui/tailwind.config.js`). No raw Tailwind palette colors (`blue-500`, `amber-400`, …) in new or touched code; must look right in light, dark and all 10 named themes.
- Toggles use the existing `features/settings/components/primitives/SettingsToggle` (Radix Switch). Settings layout uses the existing primitives (SettingsSection/Row/Card).
- Copy is Simplified Chinese, concise, no jargon in primary text ("本地检索模型", not "bge-m3"); technical detail (model ids, sources, bytes) only in secondary text/tooltips.
- Motion via `motion/react`, ≤200 ms ease-out for state changes; respect `prefers-reduced-motion`.
- Every async surface has explicit loading (skeleton, not "加载中…" text), empty, error (with an action) and success states.
- Browser-mock parity: every new command/event is stubbed in `ui/src/lib/dev-tauri-mock.ts` so the UI renders fully at `http://127.0.0.1:9527` (`npm run dev:mock-tauri`).
- Backend verification: `cd src-tauri && cargo build`, `cargo test --lib <filter>`. Frontend: `cd ui && npx tsc --noEmit`, `npm test -- --run`.
- Explicit `git add`, one logical change per commit, no push/PR unless asked. Don't touch `crates/pi`.

## Review Focus

1. Download progress events arriving after the user turned models off, or out of order → UI must not show stale progress (generation/guard like `useLocalModel`'s `downloadActiveRef`). Test in Task 5.
2. Toggle flipped rapidly on/off/on while a 570 MB download is mid-flight → no duplicate downloads (per-spec mutex exists), no double model load, status ends consistent. Test in Task 2.
3. App started with models already on disk → status shows "就绪" immediately after load, no download card flash. Test in Task 5 (mock state).
4. Narrow settings window (≈720 px wide) and long Chinese/English mixed strings (source URLs, model ids) → no overflow; truncate with tooltip. Checked in Task 7.
5. A turn where only MemoryCore hits exist (graph recall empty) → the chat chip still appears. Test in Task 3.

---

### Task 1: Observable MemoryCore status + download progress

**Files:**
- Create: `src-tauri/src/memory_core/status.rs`
- Modify: `src-tauri/src/memory_core/mod.rs` (hold `RwLock<MemoryCoreStatus>`, expose `status()`/`update_status()`), `models.rs` (`ensure_model` progress callback), `service.rs` (update status: sync stats, models loaded, download phases; call a notifier)

**Interfaces:**
- Produces:
```rust
#[derive(Clone, Debug, Serialize, PartialEq)] #[serde(rename_all = "camelCase")]
pub struct MemoryCoreStatus {
    pub seq: u64,                          // monotonically increasing per status change; UI drops older events
    pub models_enabled: bool,
    pub mode: RetrievalMode,               // "hybrid" (embedder loaded) | "keyword" (BM25 only)
    pub embedder_loaded: bool, pub reranker_loaded: bool,
    pub download: Option<DownloadStatus>,   // None when nothing to download / done
    pub index: IndexStatus,
}
#[derive(Clone, Debug, Serialize, PartialEq)] #[serde(rename_all = "camelCase")]
pub struct DownloadStatus { pub phase: DownloadPhase /* queued|downloading|failed|done */, pub model: String /* spec id */,
    pub file: String, pub source: Option<String> /* host */, pub downloaded_bytes: u64, pub total_bytes: u64 /* sum over both specs */,
    pub error: Option<String> }
#[derive(Clone, Debug, Serialize, PartialEq)] #[serde(rename_all = "camelCase")]
pub struct IndexStatus { pub docs: u64, pub vectors: u64, pub pending_embeddings: u64, pub last_sync_at: Option<String> /* RFC3339 */ }
pub type StatusNotifier = Arc<dyn Fn(&MemoryCoreStatus) + Send + Sync>;
```
  `ensure_model(client, root, spec, on_progress: &(dyn Fn(ProgressTick) + Send + Sync))` — `ProgressTick { file, source_host, downloaded, total_for_file }`, called at most every 250 ms per file and once at file completion. `service::spawn(core, chunks_db_path, notifier: Option<StatusNotifier>)`; the notifier is invoked on every status change (throttled ≥250 ms except phase changes).
- `total_bytes` uses the manifest's known sizes (bge-m3 568,456,694; reranker 570,727,094 — verified 2026-11-09) for a stable overall percentage.

- [ ] **Step 1:** Failing tests: `MemoryCoreStatus` serializes camelCase with `mode: "keyword"` when no embedder; `update_status` changes trigger the notifier exactly once per change and not for identical status; index counts reflect `memory_search_docs` / `memory_search_vectors` / pending (docs without vector) on an in-memory DB.
- [ ] **Step 2:** RED → implement → GREEN (`cargo test --lib memory_core::status`, `memory_core::`).
- [ ] **Step 3:** Commit `feat(memory_core): observable engine status and download progress`.

### Task 2: Runtime models toggle + IPC commands

**Files:**
- Modify: `src-tauri/src/memory_core/service.rs` (controllable model task: `ServiceHandle { set_models_enabled(bool), retry_download() }`), `memory_core/mod.rs` (`unload_models()`), `src-tauri/src/commands/memory_core.rs` (new) + `commands/mod.rs`, `src-tauri/src/app.rs` (store `ServiceHandle` in AppState), `src-tauri/src/main.rs` (spawn with notifier that `emit("memory-core:status", status)`; register commands)

**Interfaces:**
- Commands: `memory_core_get_status() -> MemoryCoreStatus`; `memory_core_set_models_enabled(enabled: bool) -> MemoryCoreStatus` (persists `DbSettings.set_memory_core_models_enabled`, then: on → start/continue download + load; off → cancel pending download wait, unload models → keyword mode); `memory_core_retry_download() -> MemoryCoreStatus` (only when phase=failed).
- Event: `memory-core:status` with `MemoryCoreStatus` payload.

- [ ] **Step 1:** Failing tests (no app): `ServiceHandle` state machine — toggling off unloads (embedder None), toggling on when files are ready loads without downloading; rapid off/on/on yields one load (use the existing load mutex) — drive with a fake loader/downloader seam.
- [ ] **Step 2:** RED → implement → GREEN; `cargo build`.
- [ ] **Step 3:** Commit `feat(memory_core): runtime models toggle, status/retry commands, status event` (body: new commands registered in `generate_handler!`, AppState field).

### Task 3: "Memories used this turn" in the recall event

**Files:**
- Modify: `src-tauri/src/agent/memory_context.rs` (recall_event gains `coreHits` + `mode`; emitted also when graph recall is empty but core hits exist), `src-tauri/src/tauri_commands.rs` (pi agent path: ensure the event is emitted whenever `recall_event` is Some)

**Interfaces:** event `agent:memory-recall` payload adds `coreHits: [{ title: string, snippet: string (≤120 chars), source: "graph"|"chunk"|"item", score: number }]` and `mode: "hybrid"|"keyword"`; existing fields unchanged.

- [ ] **Step 1:** Failing test: `load_context` with a BM25-only `MemoryCore` holding one matching doc and an empty graph → `recall_event` is Some with 1 coreHit (title/snippet/source).
- [ ] **Step 2:** RED → implement → GREEN (`cargo test --lib memory_context`).
- [ ] **Step 3:** Commit `feat(memory): include MemoryCore hits in the recall event`.

### Task 4: Design foundations for memory UI

Load the `ui-ux-pro-max` skill (and `frontend-design` if available) before starting.

**Files:**
- Create: `docs/design/memory-ui.md` (visual language for all memory surfaces: type scale 11/12/13/15/20 px, spacing 4/8/12/16/24, radius tokens, elevation, motion durations, state patterns, copy tone; screenshots of the reference surfaces it builds on)
- Modify: `ui/src/styles/globals.css` + `ui/tailwind.config.js` (semantic tokens: `--memory-kind-{fact,preference,episode,lesson,entity,procedure}` and `-bg` variants, `--progress-track`, `--progress-fill`; defined for `:root`, `.dark`, and each named theme — derive from existing theme hues so they harmonize), `ui/src/components/chat/MemoryRecallChip.tsx` + `components/memory/FragmentCard.tsx` (replace hard-coded palette colors with the tokens)
- Create: `ui/src/components/memory/primitives/` — `KindBadge.tsx`, `ProgressBar.tsx` (determinate/indeterminate, token colors, reduced-motion aware), `StatPill.tsx`, `Skeleton` usage guidance

**Interfaces:** `<KindBadge kind="fact|preference|episode|lesson|entity|procedure" />`, `<ProgressBar value={0..1} indeterminate? label? />`, `<StatPill label value tone="neutral|success|warning|danger" />`.

- [ ] **Step 1:** Vitest: KindBadge renders a token class per kind (no raw palette classes — assert class names don't match `/(blue|green|red|amber|purple|cyan|orange)-\d/`); ProgressBar sets aria-valuenow and switches to indeterminate.
- [ ] **Step 2:** Implement; `npx tsc --noEmit`; `npm test -- --run`.
- [ ] **Step 3:** Commit `feat(ui): memory design tokens and primitives; drop hard-coded colors`.

### Task 5: Settings › 记忆 engine card

**Files:**
- Modify: `ui/src/atoms/settings-tab.ts`, `features/settings/components/SettingsNav.tsx` (rename 记忆召回 → 记忆; attention dot while downloading/failed), `SettingsPanel.tsx` (label), `MemoryRecallTab.tsx` (engine card on top; existing recall tuning moved under a collapsible 高级)
- Create: `features/settings/components/memory/MemoryEngineCard.tsx`, `hooks/useMemoryCoreStatus.ts`, bridge fns in `ui/src/lib/bridge/memory-core.ts`
- Modify: `ui/src/lib/dev-tauri-mock.ts` (stub the 3 commands + a streamed `memory-core:status` download sequence; a URL flag `?memoryCore=ready|downloading|failed|off|empty` to pick the mock state)

**Behavior:** card header: title "本地检索引擎", one-line explanation ("让助手按意思而不只是按字词找回记忆，模型在本机运行，不上传数据"), mode StatPill (混合检索 / 仅关键词), SettingsToggle "使用本地检索模型". States: ready (two model rows with ✓ and size), downloading (overall ProgressBar with percent + "x / 1.14 GB · 来自 镜像站" + per-model rows), failed (danger callout with reason + 重试), off (muted explanation of keyword-only mode), first-run empty index. Index section: docs, vector coverage ProgressBar (vectors/docs), pending, "上次同步 n 分钟前". Uses `useMemoryCoreStatus` = initial `memory_core_get_status` + `listen('memory-core:status')`, ignores events whose `seq` is ≤ the last applied one.

- [ ] **Step 1:** Vitest for MemoryEngineCard: each mock state renders its key text/role; toggling calls `memory_core_set_models_enabled`; a stale event (lower seq) is ignored.
- [ ] **Step 2:** Implement; tsc; tests.
- [ ] **Step 3:** Commit `feat(ui): memory engine settings — models toggle, download progress, index status`.

### Task 6: Chat — memories used this turn

**Files:** Modify `ui/src/components/chat/MemoryRecallChip.tsx` (+ its listener in `hooks/useGlobalAgentListeners.ts`), add test.

**Behavior:** compact chip "参考了 N 条记忆" (KindBadge-colored dot cluster); click → Popover listing core hits (title, 1–2 line snippet, source label 记忆/对话片段, relevance as a subtle bar), keyword-mode hint when `mode=keyword`; keyboard accessible; empty → no chip. (Deep link to the Memory Center comes in the P2 plan.)

- [ ] **Step 1:** Vitest: chip appears with coreHits only; popover lists them; hidden when none.
- [ ] **Step 2:** Implement; tsc; tests.
- [ ] **Step 3:** Commit `feat(ui): show memories used in a turn`.

### Task 7: Visual QA pass

- [ ] **Step 1:** `cd ui && npm run dev:mock-tauri`; with the browser pane, screenshot Settings › 记忆 in each `?memoryCore=` state × {light, dark, warm-paper, qingye} and at 1280 and ~720 px widths; screenshot the chat chip + popover. Save under `docs/design/screenshots/p1-ui/`.
- [ ] **Step 2:** Fix every defect found (overflow, contrast < WCAG AA for text, misaligned baselines, jank, missing states); re-shoot.
- [ ] **Step 3:** Playwright: add `e2e/memory-engine.spec.ts` covering the downloading → ready transition in the mock.
- [ ] **Step 4:** Commit `test(ui): memory engine visual QA + e2e` (include screenshots).
