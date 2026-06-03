# S2 — Smart model download (HF + ModelScope, network-aware)

**Date:** 2026-06-03
**Status:** Design (decisions locked in brainstorming; pending review)
**Sub-project of:** Local MiniCPM initiative (S0 ✓ #58 · S1 ✓ #60 · **S2** · S3 · S4)
**Depends on:** S1 (`local_llm/download.rs` exists with `download_from_modelscope` + paths + the `download_local_model` Tauri command).

---

## Problem & goal

S1 ships a deliberately minimal downloader: ModelScope-only, no resume, size-only check, single quant, basic progress event. S2 makes the download **robust and network-aware** so first-time setup "just works" regardless of region or a flaky connection.

## Decisions (brainstorming)

| Topic | Choice |
|---|---|
| Source selection | **Concurrent latency probe** of both HF + ModelScope; start the download from whichever responds faster; on hard failure, fail over to the other. |
| Resume | **HTTP Range** resume of the `.part` file. |
| Integrity | **Checksum** verification (SHA256 from repo metadata) after download, in addition to size. |
| Quant | Default **Q4_K_M**; advanced switch to **Q8_0 / F16** in Settings → MiniCPM. |

## Architecture

Extend `local_llm/download.rs` into a small submodule `local_llm/download/`:

```
local_llm/download/
  mod.rs       — public API: download_model(spec, source_pref, on_progress) + re-exports
  source.rs    — Source enum {ModelScope, HuggingFace}; URL builders; latency probe + pick
  resume.rs    — Range-aware streaming to .part (read existing length, send Range header)
  verify.rs    — size + SHA256 verification against repo metadata
  quant.rs     — Quant enum {Q4KM, Q8_0, F16} → filename + expected size/sha
```

- `Source::probe_fastest(timeout) -> Source` — fire a `HEAD` (or 1-byte Range GET) at both sources' file URLs concurrently (`tokio::select!`/`join`), return the lower-latency reachable one; if only one reachable, return it; if neither, `Err`.
- `download_model(quant: Quant, prefer: Option<Source>, on_progress) -> Result<PathBuf>`:
  1. resolve source (explicit `prefer`, else `probe_fastest`).
  2. determine resume offset = existing `.part` length; open file in append mode.
  3. stream with `Range: bytes=<offset>-`; on chunk, `on_progress(downloaded, total)`.
  4. on transport error mid-stream: retry up to N times with the *other* source (Range continues from current offset); preserve partial.
  5. on completion: `verify` (size + SHA256); atomic rename to final path.
- Quant metadata (`quant.rs`): filename, expected byte size, and SHA256 per quant, sourced from the repos' file listings. (The exact SHA values are filled at implementation time by reading the live repo metadata — they are facts to look up, not invent.)

### HF source specifics
- URL: `https://huggingface.co/openbmb/MiniCPM5-1B-GGUF/resolve/main/<file>` (+ optional mirror `hf-mirror.com` if HF is blocked — probe covers this).
- ModelScope URL: as in S1 (`resolve/master/<file>`).

### UI (Settings → MiniCPM + onboarding progress)
- Progress: keep the S1 event `local-model:download-progress` `{downloaded,total}`, add `{source, phase}` (probing | downloading | verifying). A small progress bar + source label + cancel.
- Quant selector (advanced): a dropdown writing the chosen quant to a setting consumed by `download_model` + `paths::MODEL_FILE` resolution (the model-file path becomes quant-dependent).

## Backend ↔ frontend
- Tauri commands: extend `download_local_model` to take an optional `quant` + `source` arg; add `probe_download_sources() -> { fastest, latencies }` for the UI to show which source it'll use; add `cancel_download`.
- `paths.rs`: `MODEL_FILE` becomes `model_file_for(quant)`; `is_model_present` checks the active quant.

## Error handling
- Both sources unreachable → typed `DownloadError::NoSource`; UI shows "check network / try later".
- Checksum mismatch → delete the bad file, `DownloadError::Checksum`; offer retry.
- Resume offset > remote size (stale .part) → discard `.part`, restart.

## Testing
- `source.rs`: URL builders per source×quant (unit); probe logic with a mock (inject two fake latencies → picks lower).
- `resume.rs`: Range header construction from an existing `.part` length (unit); against a local mock HTTP server serving partial content.
- `verify.rs`: size+sha pass/fail (unit, with a tiny fixture file + known sha).
- `quant.rs`: enum→filename/size mapping (unit).
- No network in CI; the live download is a manual `#[ignore]` test.

## Scope guardrails
- **In S2:** dual source + latency probe + Range resume + checksum + quant selection + progress/cancel UI.
- **Not S2:** onboarding flow (S3), pet (S4). Torrent/mirror lists beyond HF-mirror. Background auto-update of the model.

## Risks
- ModelScope/HF `resolve` URL formats + redirect behavior differ; probe must handle redirects. Confirm with `curl -IL` at implementation.
- SHA metadata availability: if a repo doesn't expose per-file SHA conveniently, fall back to size-only for that source + log (don't block).
