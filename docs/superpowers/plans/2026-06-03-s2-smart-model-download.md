# Smart Model Download (S2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development or superpowers:executing-plans. Steps use `- [ ]`.
> **Status:** task-level roadmap. Implement AFTER S1 (#60) merges; at execution, detail-fill each task's code against the merged `local_llm/download.rs` and live repo URLs/SHAs (the exact SHA256 + redirect behavior are facts to look up, not invent).

**Goal:** Make local-model download robust + network-aware: concurrent HF/ModelScope latency probe, HTTP Range resume, SHA256 verification, and quant selection (Q4_K_M default).

**Architecture:** Refactor S1's single-file `local_llm/download.rs` into a `local_llm/download/` submodule (`source`, `resume`, `verify`, `quant`, `mod`). Backend Tauri commands gain source/quant/cancel; UI gains a source-aware progress bar + advanced quant selector.

**Spec:** `docs/superpowers/specs/2026-06-03-s2-smart-model-download-design.md`

**Branch:** `pi/s2-smart-model-download` — stacked on S1 (base `pi/local-minicpm-runtime-impl`) until S1 merges, then rebase onto main.

---

## File structure

| File | Responsibility |
|---|---|
| `src-tauri/src/local_llm/download/mod.rs` | public `download_model(quant, prefer, on_progress)` + `DownloadError` + re-exports (replaces `download.rs`) |
| `src-tauri/src/local_llm/download/source.rs` | `Source{ModelScope,HuggingFace}`; URL builders; `probe_fastest` |
| `src-tauri/src/local_llm/download/resume.rs` | Range-aware streaming to `.part` |
| `src-tauri/src/local_llm/download/verify.rs` | size + SHA256 verify |
| `src-tauri/src/local_llm/download/quant.rs` | `Quant{Q4KM,Q8_0,F16}` → filename/size/sha |
| `src-tauri/src/local_llm/paths.rs` | `model_file_for(quant)`, quant-aware `is_model_present` |
| `src-tauri/src/commands/local_llm.rs` | extend `download_local_model(quant,source)`; add `probe_download_sources`, `cancel_download` |
| `ui/src/features/settings/components/...MiniCPM...` | quant selector + source-aware progress |

## Tasks (ordered; detail-fill code at execution)

### Task 1: `quant.rs` — quant model (pure, fully testable now)
- `enum Quant { Q4KM, Q8_0, F16 }`; `fn filename(self) -> &str` (e.g. `MiniCPM5-1B-Q4_K_M.gguf`); `fn expected_size(self) -> u64`; `fn sha256(self) -> &str` (look up live). `Default = Q4KM`.
- Unit tests: each variant → filename/size; round-trip a serde string repr for settings storage.
- Commit.

### Task 2: `paths.rs` quant-awareness
- `model_file_for(quant) -> PathBuf`; `is_model_present(data_dir, quant) -> bool`. Keep a back-compat `MODEL_FILE` = Q4KM filename or migrate callers (engine reads the active quant from a setting).
- Tests: path per quant; presence false when absent. Commit.

### Task 3: `source.rs` — source URLs + latency probe
- `enum Source { ModelScope, HuggingFace }`; `fn file_url(self, quant) -> String`; `async fn reachable_latency(self, quant) -> Option<Duration>` (1-byte Range GET or HEAD, short timeout).
- `async fn probe_fastest(quant, timeout) -> Result<Source>` — `join` both, pick lowest reachable.
- Tests: URL builders per source×quant (unit); probe picks the lower of two injected latencies (inject via a trait or a test seam, not real network).
- Commit.

### Task 4: `resume.rs` — Range resume
- `async fn stream_with_resume(url, part_path, on_progress, total_hint) -> Result<()>` — read existing `.part` len → `Range: bytes=<len>-`; append; progress includes the pre-existing bytes. Handle 200 (no-range support → restart from 0) vs 206.
- Tests: Range header from a fixture `.part` length; 200-vs-206 branch via a local mock server (`#[ignore]` if it needs a port) or a pure header-builder unit.
- Commit.

### Task 5: `verify.rs` — size + SHA256
- `async fn verify(path, quant) -> Result<()>` — size check + streaming SHA256 (use the `sha2` crate; confirm it's a dep or add it).
- Tests: a tiny fixture file with known sha passes; tampered fails. Commit.

### Task 6: `mod.rs` — orchestrate `download_model`
- Resolve source (prefer|probe) → resume-stream → on transport error retry on the *other* source (≤2) → verify → atomic rename. Typed errors (`NoSource`, `Checksum`, `Incomplete`).
- Tests: orchestration with mocked source/resume/verify seams (success; failover; checksum-fail cleanup). Commit.

### Task 7: Tauri commands + cancel
- Extend `download_local_model(quant, source)`; add `probe_download_sources() -> {fastest, latencies}` and `cancel_download` (a shared `CancellationToken` / atomic flag the stream loop checks). Register all in `generate_handler!`. Emit `local-model:download-progress {downloaded,total,source,phase}`.
- Commit.

### Task 8: UI — quant selector + source-aware progress
- Settings → MiniCPM: advanced quant dropdown (writes setting); progress bar with source label + phase + cancel button; wire to the new commands/events. Follow existing settings component patterns.
- `npx tsc --noEmit` clean; vitest for the progress component if feasible. Commit.

## Verification
- `cargo test --lib local_llm::download` (all submodule unit tests).
- Manual: throttle/block one source → confirm probe picks the other; kill mid-download → confirm resume continues; corrupt `.part` → checksum catches it.

## Notes
- Stacked on S1: when S1 merges, rebase onto main (S1 commits drop). `sha2` may be a new dep → commit Cargo.lock with it.
