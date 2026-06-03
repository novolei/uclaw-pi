# Local MiniCPM Runtime (S1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Run `openbmb/MiniCPM5-1B-GGUF` in-process via the pure-Rust `mistralrs` crate, exposed as a `local-minicpm` provider assignable to the S0 `summarizer`/`utility` roles, with lazy-load + idle-unload lifecycle and a minimal ModelScope downloader.

**Architecture:** A new `src-tauri/src/local_llm/` module holds a process-global `LocalLlmEngine` singleton (`OnceLock`) that lazily loads the GGUF on first use and unloads it after idle. A `LocalMistralRsProvider` implements the existing `LlmProvider` trait and delegates to the engine. A new `ApiType::LocalMistralRs` routes to it in `create_provider`. The Memory-OS path is fixed to thread the resolved `api_override` (today dropped), so a role assigned to `local-minicpm` actually reaches the local provider.

**Tech Stack:** Rust, Tauri, tokio, `mistralrs` (Metal feature on macOS), `reqwest` (already a dep) for the ModelScope download. `cargo build` / `cargo test --lib`.

**Spec:** `docs/superpowers/specs/2026-06-03-local-minicpm-runtime-design.md`

**Branch:** `pi/local-minicpm-runtime` (off `origin/main`; spec already committed here).

**Pre-flight (repo policy):** Before editing `create_provider` / `get_role_llm_config` consumers, run `gitnexus_impact` on the touched symbols (`create_provider`, `complete_text`) and report blast radius. Add the new Tauri commands in BOTH `tauri_commands.rs` and the `invoke_handler!` macro in `main.rs` (forgetting the macro compiles but fails at runtime). Register the idle-unload reaper in the `[Stage 3]` block of `main.rs`.

---

## ⚠️ Hard gate

**Task 1 is a spike that must pass before Tasks 3–6.** If `mistralrs` cannot load `MiniCPM5-1B-GGUF`, STOP and escalate — the fallback is `llama-cpp-2` (a different engine), which would change Tasks 3–4. Task 2 (download + paths) and Task 5's analysis are engine-independent and may proceed in parallel, but no engine integration lands until the spike is green.

---

## File Structure

| File | Responsibility | Change |
|---|---|---|
| `src-tauri/Cargo.toml` | deps | add `mistralrs` (+ `metal` feature on macOS via target cfg) |
| `src-tauri/src/local_llm/mod.rs` | module wiring + global engine handle | new |
| `src-tauri/src/local_llm/paths.rs` | model dir/file path helpers + presence check | new |
| `src-tauri/src/local_llm/download.rs` | ModelScope Q4_K_M downloader | new |
| `src-tauri/src/local_llm/engine.rs` | `LocalLlmEngine`: lazy load, warmup, idle-unload, `complete` | new |
| `src-tauri/src/local_llm/provider.rs` | `LocalMistralRsProvider: LlmProvider` | new |
| `src-tauri/src/lib.rs` | `pub mod local_llm;` | modify |
| `src-tauri/src/providers/types.rs` | `ApiType::LocalMistralRs` | modify |
| `src-tauri/src/providers/registry.rs` | register `local-minicpm` | modify |
| `src-tauri/src/providers/service.rs` | static `list_models` for `local-minicpm` | modify |
| `src-tauri/src/llm/mod.rs` | route `LocalMistralRs` in `create_provider` | modify |
| `src-tauri/src/memory_graph/memory_os_llm.rs` | thread resolved `api_override` (fix dropped api) | modify |
| `src-tauri/src/tauri_commands.rs` + `main.rs` + `commands/` | `download_local_model`, `is_local_model_present`; startup init; idle reaper | modify |

---

## Task 1: Spike — verify `mistralrs` loads MiniCPM5-1B-GGUF on Metal (HARD GATE)

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/local_llm/mod.rs`, `src-tauri/src/local_llm/spike_test.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add the dependency.** In `src-tauri/Cargo.toml` add (pin the latest published version; verify on crates.io at implementation time — record the exact version you used in a comment):

```toml
# Local in-process LLM inference (S1). Metal on macOS; CPU elsewhere.
mistralrs = { version = "0.6", default-features = false }

[target.'cfg(target_os = "macos")'.dependencies]
mistralrs = { version = "0.6", features = ["metal"] }
```

If `mistralrs`'s exact crate name / version / feature flags differ, adjust here and record what actually worked. Confirm `cargo tree -p mistralrs` resolves and the workspace's `tokio` version is compatible (no duplicate incompatible tokio).

- [ ] **Step 2: Create the module entry.** `src-tauri/src/local_llm/mod.rs`:

```rust
//! Local in-process LLM runtime (S1): MiniCPM5-1B-GGUF via mistralrs.
#[cfg(test)]
mod spike_test;
```

Add `pub mod local_llm;` to `src-tauri/src/lib.rs` (next to the other `pub mod` lines).

- [ ] **Step 3: Write the spike integration test** (`#[ignore]` — needs the real GGUF + is slow). `src-tauri/src/local_llm/spike_test.rs`:

```rust
//! HARD-GATE spike: prove mistralrs can load MiniCPM5-1B-GGUF and generate.
//! Run manually after placing the GGUF at the path below:
//!   cargo test --lib local_llm::spike_test -- --ignored --nocapture
//! Record the exact working mistralrs API (builder + request + response field
//! names) in this file's comments — Tasks 3-4 depend on it.

#[tokio::test]
#[ignore = "needs MiniCPM5-1B-GGUF on disk; slow; run manually"]
async fn spike_loads_minicpm5_and_generates() {
    // Place the Q4_K_M GGUF here for the spike:
    let dir = format!("{}/.uclaw-pi/models/minicpm5-1b", std::env::var("HOME").unwrap());
    let gguf = "MiniCPM5-1B-Q4_K_M.gguf"; // adjust to the real filename you downloaded

    // NOTE: this is the *expected* mistralrs ~0.6 high-level API. Adjust to the
    // real signatures of the version you pinned in Step 1, then update these
    // comments with the confirmed API so Tasks 3-4 can copy it verbatim.
    use mistralrs::{GgufModelBuilder, TextMessageRole, TextMessages};

    let model = GgufModelBuilder::new(dir, vec![gguf.to_string()])
        .build()
        .await
        .expect("mistralrs failed to load MiniCPM5-1B-GGUF — SPIKE FAILED, escalate to llama-cpp-2");

    let messages = TextMessages::new()
        .add_message(TextMessageRole::System, "You are concise.")
        .add_message(TextMessageRole::User, "Reply with exactly: ok");

    let resp = model.send_chat_request(messages).await.expect("generation failed");
    let text = resp.choices[0].message.content.clone().unwrap_or_default();
    eprintln!("[SPIKE] generated: {text:?}; usage: {:?}", resp.usage);
    assert!(!text.trim().is_empty(), "model produced empty output");
}
```

- [ ] **Step 4: Run the spike** (after downloading the GGUF — you can use Task 2's downloader once it exists, or `huggingface-cli`/`modelscope` manually):

Run: `cd src-tauri && cargo test --lib local_llm::spike_test -- --ignored --nocapture`
Expected: PASS, with a non-empty `[SPIKE] generated:` line. On Metal, expect GPU use (watch Activity Monitor / mistralrs logs).

- [ ] **Step 5: Record the confirmed API.** Edit the comments at the top of `spike_test.rs` to document the EXACT working API: the builder type + chained methods, how max_tokens/temperature are set (likely a `RequestBuilder` with sampling params), the response text field path, and the usage field names (e.g. `resp.usage.completion_tokens`). Tasks 3-4 copy this verbatim.

- [ ] **Step 6: Commit.**

```bash
# Cargo.lock SHOULD be committed here — it changed legitimately (new mistralrs
# dep) and pins the dependency for reproducibility. This is the one place in
# this feature where staging Cargo.lock is correct.
git add Cargo.lock src-tauri/Cargo.toml src-tauri/src/local_llm/mod.rs src-tauri/src/local_llm/spike_test.rs src-tauri/src/lib.rs
git commit -m "spike(local-llm): verify mistralrs loads MiniCPM5-1B-GGUF (Metal)"
```

**If the spike FAILS:** stop, report BLOCKED with the error, and do not proceed. The fallback engine (`llama-cpp-2`) requires re-planning Tasks 3-4.

---

## Task 2: Model paths + minimal ModelScope downloader (engine-independent)

**Files:**
- Create: `src-tauri/src/local_llm/paths.rs`, `src-tauri/src/local_llm/download.rs`
- Modify: `src-tauri/src/local_llm/mod.rs`

- [ ] **Step 1: Write the failing path tests.** Create `src-tauri/src/local_llm/paths.rs`:

```rust
//! Filesystem layout for the local MiniCPM model.
use std::path::{Path, PathBuf};

/// The Q4_K_M GGUF filename we download/expect. (Single quant for S1.)
pub const MODEL_FILE: &str = "MiniCPM5-1B-Q4_K_M.gguf";

/// Directory holding the local model, under the uClaw data dir:
/// `<data_dir>/models/minicpm5-1b/`.
pub fn model_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("models").join("minicpm5-1b")
}

/// Full path to the GGUF file.
pub fn model_file_path(data_dir: &Path) -> PathBuf {
    model_dir(data_dir).join(MODEL_FILE)
}

/// True iff the GGUF is present and non-empty.
pub fn is_model_present(data_dir: &Path) -> bool {
    std::fs::metadata(model_file_path(data_dir))
        .map(|m| m.is_file() && m.len() > 0)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_under_data_dir_models() {
        let d = Path::new("/tmp/uclaw-x");
        assert_eq!(model_dir(d), Path::new("/tmp/uclaw-x/models/minicpm5-1b"));
        assert_eq!(model_file_path(d), Path::new("/tmp/uclaw-x/models/minicpm5-1b/MiniCPM5-1B-Q4_K_M.gguf"));
    }

    #[test]
    fn absent_model_reports_false() {
        assert!(!is_model_present(Path::new("/tmp/uclaw-does-not-exist-zzz")));
    }
}
```

Add to `src-tauri/src/local_llm/mod.rs`: `pub mod paths;` and `pub mod download;`.

- [ ] **Step 2: Run path tests, verify PASS.**
Run: `cd src-tauri && cargo test --lib local_llm::paths 2>&1 | tail -8`
Expected: PASS (2 tests). (These compile without `mistralrs` touching them.)

- [ ] **Step 3: Write the downloader.** Create `src-tauri/src/local_llm/download.rs`:

```rust
//! Minimal single-source (ModelScope) downloader for the MiniCPM Q4_K_M GGUF.
//! Smart source selection / HF fallback / resumability is deferred to S2.
use std::path::{Path, PathBuf};
use crate::local_llm::paths::{model_dir, model_file_path, MODEL_FILE};

/// ModelScope raw-file URL for the Q4_K_M GGUF.
/// (Confirm the exact path segment against the live repo at implementation time.)
pub fn modelscope_url() -> String {
    format!(
        "https://www.modelscope.cn/models/OpenBMB/MiniCPM5-1B-GGUF/resolve/master/{MODEL_FILE}"
    )
}

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("http error: {0}")]
    Http(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("download incomplete: got {got} bytes")]
    Incomplete { got: u64 },
}

/// Download the GGUF to `<data_dir>/models/minicpm5-1b/`, streaming to a
/// `.part` file then atomically renaming. `on_progress(downloaded, total)` is
/// called as bytes arrive (`total` is 0 if the server omits Content-Length).
pub async fn download_from_modelscope(
    data_dir: &Path,
    on_progress: impl Fn(u64, u64) + Send,
) -> Result<PathBuf, DownloadError> {
    use futures::StreamExt;
    use tokio::io::AsyncWriteExt;

    let dir = model_dir(data_dir);
    tokio::fs::create_dir_all(&dir).await.map_err(|e| DownloadError::Io(e.to_string()))?;
    let final_path = model_file_path(data_dir);
    let part_path = dir.join(format!("{MODEL_FILE}.part"));

    let resp = reqwest::get(modelscope_url()).await.map_err(|e| DownloadError::Http(e.to_string()))?;
    let resp = resp.error_for_status().map_err(|e| DownloadError::Http(e.to_string()))?;
    let total = resp.content_length().unwrap_or(0);

    let mut file = tokio::fs::File::create(&part_path).await.map_err(|e| DownloadError::Io(e.to_string()))?;
    let mut downloaded: u64 = 0;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| DownloadError::Http(e.to_string()))?;
        file.write_all(&chunk).await.map_err(|e| DownloadError::Io(e.to_string()))?;
        downloaded += chunk.len() as u64;
        on_progress(downloaded, total);
    }
    file.flush().await.map_err(|e| DownloadError::Io(e.to_string()))?;
    drop(file);

    if total > 0 && downloaded < total {
        return Err(DownloadError::Incomplete { got: downloaded });
    }
    tokio::fs::rename(&part_path, &final_path).await.map_err(|e| DownloadError::Io(e.to_string()))?;
    Ok(final_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_targets_modelscope_q4km() {
        let u = modelscope_url();
        assert!(u.starts_with("https://www.modelscope.cn/models/OpenBMB/MiniCPM5-1B-GGUF/"));
        assert!(u.ends_with("MiniCPM5-1B-Q4_K_M.gguf"));
    }
}
```

Confirm `thiserror` and `futures` are already workspace deps (they are — used elsewhere). If not, add them.

- [ ] **Step 4: Run, verify PASS + build.**
Run: `cd src-tauri && cargo test --lib local_llm::download 2>&1 | tail -8` → PASS.
Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` → no output.

- [ ] **Step 5: Commit.**

```bash
git add src-tauri/src/local_llm/paths.rs src-tauri/src/local_llm/download.rs src-tauri/src/local_llm/mod.rs
git commit -m "feat(local-llm): model paths + minimal ModelScope downloader"
```

---

## Task 3: `LocalLlmEngine` — lazy load, warmup, idle-unload

**Files:**
- Create: `src-tauri/src/local_llm/engine.rs`
- Modify: `src-tauri/src/local_llm/mod.rs`

- [ ] **Step 1: Write the state-machine tests first** (no real model — exercise the Unloaded/missing/idle logic with a non-existent path). Create `src-tauri/src/local_llm/engine.rs` with the type + tests; the real `mistralrs` load lives behind a method the tests don't call:

```rust
//! In-process MiniCPM engine: lazy load + warmup + idle unload.
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use crate::local_llm::paths::{is_model_present, model_file_path};

#[derive(Debug, thiserror::Error)]
pub enum LocalLlmError {
    #[error("local model file not present; download it first")]
    ModelMissing,
    #[error("failed to load model: {0}")]
    Load(String),
    #[error("inference failed: {0}")]
    Inference(String),
}

/// One completed local generation.
pub struct LocalCompletion {
    pub text: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub model: String,
}

enum EngineState {
    Unloaded,
    Loaded { model: LoadedModel, last_used: Instant },
}

/// Opaque handle to the loaded mistralrs model. Defined in Step 3.
pub(crate) struct LoadedModel {
    inner: mistralrs::Model,
}

pub struct LocalLlmEngine {
    state: RwLock<EngineState>,
    data_dir: PathBuf,
    idle_unload_after: Duration,
    model_id: String,
}

impl LocalLlmEngine {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            state: RwLock::new(EngineState::Unloaded),
            data_dir,
            idle_unload_after: Duration::from_secs(10 * 60),
            model_id: "minicpm5-1b".to_string(),
        }
    }

    pub async fn is_loaded(&self) -> bool {
        matches!(&*self.state.read().await, EngineState::Loaded { .. })
    }

    /// Unload the model if it has been idle longer than `idle_unload_after`.
    pub async fn unload_if_idle(&self) {
        let mut s = self.state.write().await;
        if let EngineState::Loaded { last_used, .. } = &*s {
            if last_used.elapsed() >= self.idle_unload_after {
                *s = EngineState::Unloaded;
                tracing::info!("local_llm: unloaded idle MiniCPM model to free RAM");
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn with_idle_window(data_dir: PathBuf, idle: Duration) -> Self {
        let mut e = Self::new(data_dir);
        e.idle_unload_after = idle;
        e
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fresh_engine_is_unloaded() {
        let e = LocalLlmEngine::new(PathBuf::from("/tmp/uclaw-none"));
        assert!(!e.is_loaded().await);
    }

    #[tokio::test]
    async fn complete_errors_when_model_missing() {
        let e = LocalLlmEngine::new(PathBuf::from("/tmp/uclaw-does-not-exist-zzz"));
        let r = e.complete("sys", "usr", 16, 0.3).await;
        assert!(matches!(r, Err(LocalLlmError::ModelMissing)));
    }

    #[tokio::test]
    async fn unload_if_idle_noop_when_unloaded() {
        let e = LocalLlmEngine::with_idle_window(PathBuf::from("/tmp/x"), Duration::from_secs(0));
        e.unload_if_idle().await; // must not panic on Unloaded
        assert!(!e.is_loaded().await);
    }
}
```

- [ ] **Step 2: Run, verify FAIL** (no `complete` / `LoadedModel::load` yet → compile error):
Run: `cd src-tauri && cargo test --lib local_llm::engine 2>&1 | tail -15`
Expected: FAIL — `no method named complete` (and `LoadedModel` has no constructor used by `complete`).

- [ ] **Step 3: Implement `complete` + the mistralrs load**, copying the API confirmed by the Task 1 spike. Add to `impl LocalLlmEngine` and define `LoadedModel`:

```rust
impl LoadedModel {
    /// Load the GGUF via mistralrs. Uses the API pinned by the Task 1 spike;
    /// adapt the builder/feature calls to that confirmed signature.
    async fn load(data_dir: &std::path::Path) -> Result<Self, LocalLlmError> {
        use mistralrs::GgufModelBuilder;
        let dir = crate::local_llm::paths::model_dir(data_dir);
        let inner = GgufModelBuilder::new(
            dir.to_string_lossy().to_string(),
            vec![crate::local_llm::paths::MODEL_FILE.to_string()],
        )
        .build()
        .await
        .map_err(|e| LocalLlmError::Load(e.to_string()))?;
        Ok(Self { inner })
    }

    async fn generate(&self, system: &str, user: &str, max_tokens: u32, temperature: f32)
        -> Result<(String, u32, u32), LocalLlmError>
    {
        use mistralrs::{TextMessageRole, RequestBuilder};
        // API confirmed by the Task 1 spike (mistralrs 0.8.1).
        // CRITICAL: MiniCPM5 is a REASONING model — its chat template emits
        // `<think>…</think>` before the answer. With a short max_tokens budget
        // the whole budget is spent inside `<think>` and `content` comes back
        // EMPTY (this is the same empty-completion failure mode seen on hosted
        // reasoning models). For the short single-shot memory-OS completions we
        // MUST disable thinking. (S4's pet may re-enable it with headroom +
        // a `<think>` stripper.)
        let req = RequestBuilder::new()
            .add_message(TextMessageRole::System, system)
            .add_message(TextMessageRole::User, user)
            .set_sampler_max_len(max_tokens as usize)
            .set_sampler_temperature(temperature as f64)
            .enable_thinking(false);
        let resp = self.inner.send_chat_request(req).await
            .map_err(|e| LocalLlmError::Inference(e.to_string()))?;
        let text = resp.choices.first()
            .and_then(|c| c.message.content.clone())
            .unwrap_or_default();
        // usage fields are `usize` in mistralrs 0.8.1 (spike-confirmed).
        let usage = resp.usage;
        Ok((text, usage.prompt_tokens as u32, usage.completion_tokens as u32))
    }
}

impl LocalLlmEngine {
    /// Run one completion. Lazily loads + warms the model on first use; drops
    /// to `ModelMissing` if the GGUF isn't on disk.
    pub async fn complete(&self, system: &str, user: &str, max_tokens: u32, temperature: f32)
        -> Result<LocalCompletion, LocalLlmError>
    {
        if !is_model_present(&self.data_dir) {
            let _ = model_file_path(&self.data_dir);
            return Err(LocalLlmError::ModelMissing);
        }
        // Fast path: already loaded.
        {
            let mut s = self.state.write().await;
            if let EngineState::Loaded { model, last_used } = &mut *s {
                let (text, it, ot) = model.generate(system, user, max_tokens, temperature).await?;
                *last_used = Instant::now();
                return Ok(LocalCompletion { text, input_tokens: it, output_tokens: ot, model: self.model_id.clone() });
            }
        }
        // Slow path: load + warmup, then generate.
        let model = LoadedModel::load(&self.data_dir).await?;
        // Warmup: a 1-token generation to JIT kernels (ignore output/errors).
        let _ = model.generate("", "ok", 1, 0.0).await;
        let (text, it, ot) = model.generate(system, user, max_tokens, temperature).await?;
        let mut s = self.state.write().await;
        *s = EngineState::Loaded { model, last_used: Instant::now() };
        Ok(LocalCompletion { text, input_tokens: it, output_tokens: ot, model: self.model_id.clone() })
    }
}
```

Add `pub mod engine;` to `src-tauri/src/local_llm/mod.rs`.

- [ ] **Step 4: Run, verify PASS + build.**
Run: `cd src-tauri && cargo test --lib local_llm::engine 2>&1 | tail -12` → 3 tests PASS (they never load a real model).
Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` → no output.
If the `mistralrs` request/response field names differ from the spike, fix `generate` to match — the 3 tests only exercise the missing/unloaded paths so they pass regardless, but the build must be clean.

- [ ] **Step 5: Commit.**

```bash
git add src-tauri/src/local_llm/engine.rs src-tauri/src/local_llm/mod.rs
git commit -m "feat(local-llm): LocalLlmEngine lazy-load + warmup + idle-unload"
```

---

## Task 4: `LocalMistralRsProvider` + routing + registry + model listing

**Files:**
- Create: `src-tauri/src/local_llm/provider.rs`
- Modify: `src-tauri/src/local_llm/mod.rs`, `src-tauri/src/providers/types.rs`, `src-tauri/src/llm/mod.rs`, `src-tauri/src/providers/registry.rs`, `src-tauri/src/providers/service.rs`

- [ ] **Step 1: Add the global engine handle** to `src-tauri/src/local_llm/mod.rs`:

```rust
pub mod engine;
pub mod paths;
pub mod download;
pub mod provider;

use std::sync::{Arc, OnceLock};
use engine::LocalLlmEngine;

static ENGINE: OnceLock<Arc<LocalLlmEngine>> = OnceLock::new();

/// Initialize the global local engine (called once at startup). Does NOT load
/// the model — only constructs the handle + resolves paths (lazy).
pub fn init_local_engine(data_dir: &std::path::Path) -> Arc<LocalLlmEngine> {
    let e = Arc::new(LocalLlmEngine::new(data_dir.to_path_buf()));
    let _ = ENGINE.set(e.clone());
    e
}

/// Get the initialized engine, or None if `init_local_engine` wasn't called.
pub fn local_engine() -> Option<Arc<LocalLlmEngine>> {
    ENGINE.get().cloned()
}
```

- [ ] **Step 2: Add the `ApiType` variant.** In `src-tauri/src/providers/types.rs`, inside `pub enum ApiType`, add:

```rust
    /// In-process local inference via mistralrs (S1: MiniCPM5-1B-GGUF).
    #[serde(rename = "local-mistralrs")]
    LocalMistralRs,
```

Then `cargo build` and fix any non-exhaustive `match` on `ApiType` the compiler flags (grep `match .*api` / `ApiType::` across `src-tauri/src`; most sites use a catch-all and won't break).

- [ ] **Step 3: Write the provider** (`src-tauri/src/local_llm/provider.rs`):

```rust
//! LlmProvider adapter over the in-process LocalLlmEngine (S1).
use async_trait::async_trait;
use std::sync::Arc;
use crate::agent::types::{ChatMessage, RespondOutput, ResponseMetadata, StreamDelta, TokenUsage, ToolDefinition};
use crate::error::Error;
use crate::llm::provider::{CompletionConfig, LlmProvider};
use crate::local_llm::engine::LocalLlmEngine;

pub struct LocalMistralRsProvider {
    engine: Arc<LocalLlmEngine>,
    model_id: String,
}

impl LocalMistralRsProvider {
    /// Build from the process-global engine. Errors if startup init didn't run.
    pub fn from_global(model_id: String) -> Result<Self, Error> {
        let engine = crate::local_llm::local_engine()
            .ok_or_else(|| Error::Internal("local LLM engine not initialized".into()))?;
        Ok(Self { engine, model_id })
    }
}

/// Flatten ChatMessages into (system, user) for the small text model.
/// System messages are concatenated; all non-system messages are joined as the
/// user turn (the 1B model serves single-shot memory tasks, not multi-turn tools).
fn split_messages(messages: &[ChatMessage]) -> (String, String) {
    let mut system = String::new();
    let mut user = String::new();
    for m in messages {
        let role = m.role.as_str();
        let content = m.text_content(); // see note below
        if role == "system" {
            if !system.is_empty() { system.push('\n'); }
            system.push_str(&content);
        } else {
            if !user.is_empty() { user.push('\n'); }
            user.push_str(&content);
        }
    }
    (system, user)
}

#[async_trait]
impl LlmProvider for LocalMistralRsProvider {
    async fn complete(
        &self,
        messages: Vec<ChatMessage>,
        _tools: Vec<ToolDefinition>,
        config: &CompletionConfig,
    ) -> Result<RespondOutput, Error> {
        let (system, user) = split_messages(&messages);
        let out = self.engine
            .complete(&system, &user, config.max_tokens, config.temperature)
            .await
            .map_err(|e| Error::Internal(format!("local inference: {e}")))?;
        Ok(RespondOutput::Text {
            text: out.text,
            thinking: None,
            thinking_signature: None,
            metadata: ResponseMetadata {
                model: out.model,
                finish_reason: Some("stop".into()),
                usage: Some(TokenUsage {
                    input_tokens: out.input_tokens,
                    output_tokens: out.output_tokens,
                    cache_read_tokens: 0,
                    cache_creation_tokens: 0,
                    reasoning_output_tokens: 0,
                }),
            },
        })
    }

    async fn stream(
        &self,
        _messages: Vec<ChatMessage>,
        _tools: Vec<ToolDefinition>,
        _config: &CompletionConfig,
    ) -> Result<Box<dyn futures::Stream<Item = Result<StreamDelta, Error>> + Send + Unpin>, Error> {
        // Streaming is implemented in S4 (desk-pet). Memory-OS uses `complete`.
        Err(Error::Internal("local provider streaming not supported in S1".into()))
    }
}
```

NOTE on `m.role` / `m.text_content()`: inspect `ChatMessage` in `src-tauri/src/agent/types.rs` and use its real field/accessor names (the codebase already uses `ChatMessage::system(..)` / `ChatMessage::user(..)` constructors in `memory_os_llm.rs`). If there's no `text_content()` helper, read the content field directly. Adjust `split_messages` to the actual shape — do not invent accessors.

Add `pub mod provider;` is already in Step 1's `mod.rs`.

- [ ] **Step 4: Route it in `create_provider`.** In `src-tauri/src/llm/mod.rs`, change the `match` in `create_provider` to add the local arm BEFORE the `_` catch-all:

```rust
    match resolve_api(&config.provider, config.api.clone()) {
        ApiType::AnthropicMessages => Ok(Arc::new(AnthropicProvider::new(
            config.api_key.clone(),
            config.base_url.clone(),
        ))),
        ApiType::LocalMistralRs => Ok(Arc::new(
            crate::local_llm::provider::LocalMistralRsProvider::from_global(config.model.clone())?,
        )),
        _ => Ok(Arc::new(OpenAIProvider::new(
            config.api_key.clone(),
            config.base_url.clone(),
        ))),
    }
```

- [ ] **Step 5: Register the provider + static model listing.**
In `src-tauri/src/providers/registry.rs`, add a `KnownProvider` entry to the vec returned by the registry (place it near `ollama`/local providers):

```rust
    KnownProvider {
        id: "local-minicpm".into(),
        display_name: "MiniCPM (本地)".into(),
        auth_type: AuthType::None,
        default_base_url: "".into(),
        default_api: ApiType::LocalMistralRs,
        service_category: ServiceCategory::Api,
        geo_category: ProviderCategory::Local,
        supports_models: false,
    },
```

(Confirm `AuthType::None` exists — the enum has `ApiKey | OAuth | None` per the provider types. If the variant is spelled differently, use the real one.)

In `src-tauri/src/providers/service.rs`, make `list_models` return a static model for `local-minicpm` without a network call. Find the `list_models` method and add an early branch:

```rust
        if provider_id == "local-minicpm" {
            return Ok(vec![Model { id: "minicpm5-1b".to_string(), /* ..fill remaining fields with sensible defaults like the anthropic static list does.. */ }]);
        }
```

Match the exact `Model` struct shape used by `list_anthropic_models()` (copy its field initialization style — context_window/max_tokens etc.). Read that function and mirror it.

- [ ] **Step 6: Build + provider unit test.** Add to `provider.rs` a `#[cfg(test)] mod tests` that verifies `stream` returns the unsupported error and `from_global` errors when the engine isn't initialized:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stream_is_unsupported_in_s1() {
        // engine not initialized in tests → from_global errors, so build a
        // provider directly with a fresh engine to test stream().
        let engine = std::sync::Arc::new(
            crate::local_llm::engine::LocalLlmEngine::new(std::path::PathBuf::from("/tmp/x")),
        );
        let p = LocalMistralRsProvider { engine, model_id: "minicpm5-1b".into() };
        let r = p.stream(vec![], vec![], &CompletionConfig::default()).await;
        assert!(r.is_err());
    }
}
```

(For this test to construct the struct, its fields are crate-visible within the module — they are, since the test is in the same file.)

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` → no output.
Run: `cd src-tauri && cargo test --lib local_llm 2>&1 | tail -12` → all PASS.

- [ ] **Step 7: Commit.**

```bash
git add src-tauri/src/local_llm/ src-tauri/src/providers/types.rs src-tauri/src/llm/mod.rs src-tauri/src/providers/registry.rs src-tauri/src/providers/service.rs
git commit -m "feat(local-llm): LocalMistralRsProvider + ApiType::LocalMistralRs routing + local-minicpm registry"
```

---

## Task 5: Thread the resolved `api_override` through the Memory-OS path (critical fix)

**Why:** `MemoryOsLlmClient::complete_text` resolves the role's config via `get_role_llm_config` (S0) but **discards the 5th tuple element (`api_override`)** and passes `None` to `llm_config_from_provider` (the existing `// TODO(Task 2): effective api`). Without this fix, a role assigned to `local-minicpm` would resolve `provider="local-minicpm"` but `api=None` → `resolve_api` returns `OpenAiCompletions` → the local provider is never built. This task closes that gap.

**Files:**
- Modify: `src-tauri/src/memory_graph/memory_os_llm.rs`

- [ ] **Step 1: Write a failing test** proving the api flows through. In the `#[cfg(test)] mod tests` of `memory_os_llm.rs`, add a unit test on a small helper we will extract. First, the test:

```rust
    #[test]
    fn effective_api_is_carried_into_llm_config() {
        // The config built for a resolved local-minicpm role must keep the
        // LocalMistralRs api so create_provider routes to the local provider.
        let cfg = crate::llm::llm_config_from_provider(
            "local-minicpm", "minicpm5-1b", "", "", 256, 0.3,
            Some(crate::providers::types::ApiType::LocalMistralRs),
        );
        assert_eq!(cfg.api, Some(crate::providers::types::ApiType::LocalMistralRs));
        assert_eq!(cfg.provider, "local-minicpm");
    }
```

(This documents the contract; it passes immediately since `llm_config_from_provider` already accepts `api`. The real change is the caller below — Step 3 — which the build + the existing routing tests cover.)

- [ ] **Step 2: Run it (PASS) — it locks the contract.**
Run: `cd src-tauri && cargo test --lib memory_os_llm::tests::effective_api 2>&1 | tail -6` → PASS.

- [ ] **Step 3: Capture and forward the api.** In `complete_text`, change the resolver destructure to bind the 5th element and pass it through. Replace:

```rust
        let role = role_for_cost_tag(cost_tag);
        let (provider_id, model, api_key, base_url, _) = self
            .provider_service
            .get_role_llm_config(role)
            .await
            .ok_or(MemoryOsLlmError::NoProvider)?;
```

with:

```rust
        let role = role_for_cost_tag(cost_tag);
        let (provider_id, model, api_key, base_url, api) = self
            .provider_service
            .get_role_llm_config(role)
            .await
            .ok_or(MemoryOsLlmError::NoProvider)?;
```

Then in the `llm_config_from_provider(...)` call below, replace the `None, // TODO(Task 2): effective api` argument with `api`:

```rust
        let cfg = llm_config_from_provider(
            &provider_id,
            &model,
            &api_key,
            &base_url,
            max_tokens,
            0.3, // memory-os synthesis prefers determinism over flair
            api, // effective api — carries ApiType::LocalMistralRs etc. (S1)
        );
```

- [ ] **Step 4: Cost guard — confirm local inference books $0.** Verify `calculate_cost("minicpm5-1b", 100, 50)` returns `0.0` (unknown model → zero pricing). Add a quick test in `memory_os_llm.rs` tests:

```rust
    #[test]
    fn local_model_costs_zero() {
        assert_eq!(crate::agent::types::calculate_cost("minicpm5-1b", 1000, 1000), 0.0);
    }
```

Run: `cd src-tauri && cargo test --lib memory_os_llm::tests::local_model_costs_zero 2>&1 | tail -6`.
If it does NOT return 0.0 (i.e. `model_pricing` has a nonzero default), add an early `if model == "minicpm5-1b" { /* skip cost record */ }` guard in `record_cost`'s caller, or ensure `model_pricing` defaults unknown ids to zero. Report which path you took.

- [ ] **Step 5: Build + full module test.**
Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` → no output.
Run: `cd src-tauri && cargo test --lib memory_os_llm 2>&1 | tail -12` → all PASS.

- [ ] **Step 6: Commit.**

```bash
git add src-tauri/src/memory_graph/memory_os_llm.rs
git commit -m "fix(memory): thread resolved api_override into Memory-OS LlmConfig

complete_text dropped the 5th tuple element from get_role_llm_config and
passed api=None, so a role assigned to local-minicpm resolved to the
OpenAI wire protocol and never reached the local provider. Forward the
effective api (closes the old TODO). Local model books \$0 cost."
```

---

## Task 6: Tauri commands + startup wiring + idle reaper

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs` (or `src-tauri/src/commands/` — match where provider commands live), `src-tauri/src/main.rs`, `src-tauri/src/app.rs`

- [ ] **Step 1: Add the two Tauri commands.** In the module where provider commands live (e.g. `commands/provider.rs` per the codebase's `set_role_model` location), add:

```rust
/// True iff the local MiniCPM GGUF is downloaded.
#[tauri::command]
pub async fn is_local_model_present(state: State<'_, AppState>) -> Result<bool, Error> {
    Ok(crate::local_llm::paths::is_model_present(&state.data_dir))
}

/// Download the MiniCPM Q4_K_M GGUF from ModelScope, emitting progress events
/// on `local-model:download-progress` ({ downloaded, total }).
#[tauri::command]
pub async fn download_local_model(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<String, Error> {
    use tauri::Emitter;
    let data_dir = state.data_dir.clone();
    let path = crate::local_llm::download::download_from_modelscope(&data_dir, |downloaded, total| {
        let _ = app.emit("local-model:download-progress", serde_json::json!({ "downloaded": downloaded, "total": total }));
    })
    .await
    .map_err(|e| Error::Internal(format!("download failed: {e}")))?;
    Ok(path.to_string_lossy().to_string())
}
```

Confirm `AppState` exposes `data_dir` (it's set at startup from `uclaw_home_pathbuf()`); if the field name differs, use the real one. Confirm the Tauri `Emitter` import path for this Tauri version.

- [ ] **Step 2: Register the commands in the `invoke_handler!` macro** in `src-tauri/src/main.rs` (add `is_local_model_present, download_local_model` to the handler list — REQUIRED or they fail at runtime).

- [ ] **Step 3: Initialize the engine at startup.** In `src-tauri/src/app.rs`, near where `ProviderService::new(&data_dir)` is constructed, add:

```rust
    // Local LLM engine handle (lazy — does not load the model here).
    crate::local_llm::init_local_engine(&data_dir);
```

- [ ] **Step 4: Register the idle-unload reaper** in the `[Stage 3]` background-services block of `src-tauri/src/main.rs`:

```rust
    // Local LLM idle-unload reaper: drop the MiniCPM model after 10 min idle.
    tokio::spawn(async {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            tick.tick().await;
            if let Some(engine) = crate::local_llm::local_engine() {
                engine.unload_if_idle().await;
            }
        }
    });
```

- [ ] **Step 5: Build + confirm commands compile/register.**
Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` → no output.
Run: `cd src-tauri && cargo test --lib 2>&1 | tail -8` → existing + new tests PASS (the new commands have no unit test; they're integration-wired).

- [ ] **Step 6: Commit.**

```bash
git add src-tauri/src/commands/ src-tauri/src/tauri_commands.rs src-tauri/src/main.rs src-tauri/src/app.rs
git commit -m "feat(local-llm): download/presence Tauri commands + startup init + idle reaper"
```

---

## Final verification

- [ ] **Full build + targeted tests.**
Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` → no output.
Run: `cd src-tauri && cargo test --lib local_llm providers::service memory_os_llm 2>&1 | tail -20` → all PASS.

- [ ] **Spike re-run (manual, after downloading via the new command).**
1. Launch the app (or call the command via a test harness) → `download_local_model` pulls the Q4_K_M GGUF to `~/.uclaw-pi/models/minicpm5-1b/`.
2. Re-run the Task 1 spike: `cargo test --lib local_llm::spike_test -- --ignored --nocapture` → PASS.

- [ ] **End-to-end manual.**
1. In Settings → 智能 → 模型分配, assign `摘要模型 (summarizer)` to `MiniCPM (本地) / minicpm5-1b`.
2. Trigger a consolidation pass; confirm the S0 `memory_os_llm: routed completion to role` info log shows `role=summarizer resolved_model=minicpm5-1b`, the local engine loads (first call), and the cost record is $0.
3. Idle > 10 min → confirm the `local_llm: unloaded idle MiniCPM model` log fires.

- [ ] **Symbol-drift check before PR.** `gitnexus_detect_changes()` — confirm only the planned symbols changed.

---

## Notes / scope guardrails

- **Streaming, tools, LoRA persona swap → S4.** HF fallback / smart source / progress UI → S2. Onboarding wizard → S3.
- The `mistralrs` raw API in Tasks 3-4 follows the version pinned by the Task 1 spike — adapt the builder/request/response calls to that confirmed signature; the surrounding structure (build-once handle, `complete` mapping to `RespondOutput::Text`, `stream` unsupported) is fixed.
- Do not stage `Cargo.lock` opportunistically; it changes legitimately here because `Cargo.toml` gained `mistralrs` — in THIS feature `Cargo.lock` SHOULD be committed (in Task 1's commit) so the dependency is reproducible. Stage it explicitly in Task 1 only.

### Spike outcomes (Task 1, done — mistralrs 0.8.1, gate GREEN)

- **Confirmed API** is recorded verbatim in `src-tauri/src/local_llm/spike_test.rs`; Tasks 3-4 copy it. `GgufModelBuilder::new(dir, vec![file]).build().await`; `RequestBuilder::new().add_message(role,txt).set_sampler_max_len(usize).set_sampler_temperature(f64).enable_thinking(bool)`; `model.send_chat_request(req)`; text at `resp.choices[0].message.content: Option<String>`; usage `resp.usage.prompt_tokens/.completion_tokens: usize`.
- **MiniCPM5 is a reasoning model** (`<think>…</think>`). Short budgets → empty `content`. Tasks 3-4 call `.enable_thinking(false)` (already baked into Task 3's `generate`).
- **Adjacent dep change (out-of-S1-scope but mandatory):** mistralrs's `smallvec ^2.0.0-alpha.12` requirement conflicted with STT's exact-pinned `ort 2.0.0-rc.10` (`smallvec =2.0.0-alpha.10`). Resolution: bump `ort` rc.10 → **rc.12** and `ndarray` 0.16 → **0.17**, migrating `src-tauri/src/stt/openflow/onnx_inference.rs` to the new `ort` API (`Session::inputs/outputs` now methods; tighter `TensorRef` bounds). All 26 STT tests pass. **Review focus:** this touches the STT subsystem's runtime deps — the final review + manual STT smoke must confirm no STT regression. Ideally this would be its own commit/PR; it was bundled into the spike commit because it's a hard prerequisite for mistralrs to compile at all.
- **Build-env prereqs (not committed, needed on CI/other machines):** Xcode Metal Toolchain component (`xcodebuild -downloadComponent MetalToolchain`) for mistralrs's `.metal` kernel precompile; and the `pyembed`/`bunembed`/`gbrain-source` resource symlinks for the Tauri build.
