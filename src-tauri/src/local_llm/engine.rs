// SPDX-License-Identifier: AGPL-3.0-or-later
//! In-process MiniCPM engine: lazy load + warmup + idle unload.
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use crate::local_llm::paths::is_model_present;

#[derive(Debug, thiserror::Error)]
pub enum LocalLlmError {
    #[error("local model file not present; download it first")]
    ModelMissing,
    #[error("failed to load model: {0}")]
    Load(String),
    #[error("inference failed: {0}")]
    Inference(String),
}

pub struct LocalCompletion {
    pub text: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub model: String,
}

/// Generation knobs shared by `complete` and `stream`.
///
/// `thinking` toggles MiniCPM5's `<think>` reasoning block. The S1/S3 memory
/// path keeps it `false` (a `<think>` block eats the whole budget on short
/// single-shot tasks → empty content). The S4 pet can flip it on per call.
#[derive(Debug, Clone, Copy)]
pub struct EngineGenerateOpts {
    pub thinking: bool,
    pub max_tokens: u32,
    pub temperature: f32,
}

/// One streamed item from `LocalLlmEngine::stream` / `generate_stream`.
#[derive(Debug, Clone)]
pub enum LocalStreamChunk {
    /// An answer-text delta (`delta.content`).
    Text(String),
    /// A final marker carrying finish_reason + token usage off the last chunk.
    Done {
        finish_reason: Option<String>,
        input_tokens: u32,
        output_tokens: u32,
    },
}

/// Strip a single leading `<think>…</think>` block from `text`, returning the
/// remaining answer text (trimmed of leading whitespace after the block).
///
/// Rules:
/// - No `<think>` present → returns the input unchanged.
/// - `<think>…</think>` present → returns everything after the first `</think>`,
///   with leading whitespace trimmed.
/// - `<think>` present but never closed → treated as all-reasoning; returns "".
pub(crate) fn strip_think(text: &str) -> &str {
    let trimmed = text.trim_start();
    let Some(rest) = trimmed.strip_prefix("<think>") else {
        return text;
    };
    match rest.find("</think>") {
        Some(idx) => rest[idx + "</think>".len()..].trim_start(),
        None => "",
    }
}

enum EngineState {
    Unloaded,
    Loaded { model: LoadedModel, last_used: Instant },
}

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

    /// Force-unload the model now, regardless of idle time. Used when the
    /// active quant changes so the next `complete`/`stream` reloads the new
    /// GGUF. No-op if already unloaded.
    pub async fn unload(&self) {
        let mut s = self.state.write().await;
        if matches!(&*s, EngineState::Loaded { .. }) {
            *s = EngineState::Unloaded;
            tracing::info!("local_llm: force-unloaded MiniCPM model (quant change)");
        }
    }

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

// ---------------------------------------------------------------------------
// LoadedModel: wraps mistralrs::Model with load + generate
// ---------------------------------------------------------------------------

impl LoadedModel {
    async fn load(data_dir: &std::path::Path) -> Result<Self, LocalLlmError> {
        use mistralrs::GgufModelBuilder;
        // Load the user-selected quant (persisted in providers.json, seeded into
        // the local_llm active-quant static at startup). Defaults to Q4_K_M.
        let quant = crate::local_llm::active_quant();
        let dir = crate::local_llm::paths::model_dir(data_dir);
        let inner = GgufModelBuilder::new(
            dir.to_string_lossy().to_string(),
            vec![quant.filename().to_string()],
        )
        .with_logging()
        .build()
        .await
        .map_err(|e| LocalLlmError::Load(e.to_string()))?;
        Ok(Self { inner })
    }

    async fn generate(
        &self,
        system: &str,
        user: &str,
        opts: EngineGenerateOpts,
    ) -> Result<(String, u32, u32), LocalLlmError> {
        use mistralrs::{TextMessageRole, RequestBuilder};
        // enable_thinking: MiniCPM5 is a reasoning model; for short single-shot
        // memory tasks the <think> block would eat the whole budget and return
        // empty content, so the memory path passes thinking:false. (Spike-
        // confirmed.) The S4 pet may re-enable per call via opts.
        let req = RequestBuilder::new()
            .add_message(TextMessageRole::System, system)
            .add_message(TextMessageRole::User, user)
            .set_sampler_max_len(opts.max_tokens as usize)
            .set_sampler_temperature(opts.temperature as f64)
            .enable_thinking(opts.thinking);
        let resp = self
            .inner
            .send_chat_request(req)
            .await
            .map_err(|e| LocalLlmError::Inference(e.to_string()))?;
        let raw = resp
            .choices
            .first()
            .and_then(|c| c.message.content.clone())
            .unwrap_or_default();
        // MiniCPM5 is a reasoning model: even with thinking disabled it can wrap
        // output in a leading <think>…</think> block. Strip it so callers (the
        // session-title JSON parser, memory synthesis) get the clean answer
        // instead of think-prefixed/garbled text. (If the model spent its whole
        // budget inside <think>, this yields "" → the caller's graceful fallback.)
        let text = strip_think(&raw).to_string();
        let usage = resp.usage;
        Ok((text, usage.prompt_tokens as u32, usage.completion_tokens as u32))
    }

    /// Stream a generation, pumping `LocalStreamChunk` items onto `tx` as they
    /// arrive. The mistralrs `Stream<'_>` borrows `&self.inner`, so this is
    /// driven entirely inside one `&self` call (the caller holds the engine
    /// write lock across the whole pump — serialized, same as `generate`).
    async fn generate_stream(
        &self,
        system: &str,
        user: &str,
        opts: EngineGenerateOpts,
        tx: &tokio::sync::mpsc::UnboundedSender<LocalStreamChunk>,
    ) -> Result<(), LocalLlmError> {
        use mistralrs::{RequestBuilder, Response, TextMessageRole};
        let req = RequestBuilder::new()
            .add_message(TextMessageRole::System, system)
            .add_message(TextMessageRole::User, user)
            .set_sampler_max_len(opts.max_tokens as usize)
            .set_sampler_temperature(opts.temperature as f64)
            .enable_thinking(opts.thinking);

        let mut stream = self
            .inner
            .stream_chat_request(req)
            .await
            .map_err(|e| LocalLlmError::Inference(e.to_string()))?;

        let mut finish_reason: Option<String> = None;
        let mut input_tokens = 0u32;
        let mut output_tokens = 0u32;

        while let Some(resp) = stream.next().await {
            match resp {
                Response::Chunk(chunk) => {
                    if let Some(choice) = chunk.choices.first() {
                        // Pet keeps thinking off; reasoning_content is ignored
                        // here. (If thinking is later enabled, accumulate
                        // choice.delta.reasoning_content separately.)
                        if let Some(delta) = choice.delta.content.as_deref() {
                            if !delta.is_empty() {
                                // Receiver gone (cancelled) → stop pumping.
                                if tx.send(LocalStreamChunk::Text(delta.to_string())).is_err() {
                                    return Ok(());
                                }
                            }
                        }
                        if choice.finish_reason.is_some() {
                            finish_reason = choice.finish_reason.clone();
                        }
                    }
                    // The final chunk carries usage.
                    if let Some(usage) = chunk.usage.as_ref() {
                        input_tokens = usage.prompt_tokens as u32;
                        output_tokens = usage.completion_tokens as u32;
                    }
                }
                Response::ModelError(e, _) => {
                    return Err(LocalLlmError::Inference(e));
                }
                Response::InternalError(e) | Response::ValidationError(e) => {
                    return Err(LocalLlmError::Inference(e.to_string()));
                }
                // Non-chat variants won't occur for a streamed chat request.
                _ => {}
            }
        }

        let _ = tx.send(LocalStreamChunk::Done {
            finish_reason,
            input_tokens,
            output_tokens,
        });
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// LocalLlmEngine::complete — lazy-load + warmup + idle-aware dispatch
// ---------------------------------------------------------------------------

impl LocalLlmEngine {
    pub async fn complete(
        &self,
        system: &str,
        user: &str,
        max_tokens: u32,
        temperature: f32,
    ) -> Result<LocalCompletion, LocalLlmError> {
        if !is_model_present(&self.data_dir, crate::local_llm::active_quant()) {
            return Err(LocalLlmError::ModelMissing);
        }

        // Fast path: model already loaded — generate under the write lock so
        // last_used is updated atomically. The write lock is brief; the real
        // latency is inside generate() (GPU/CPU bound), not in the lock itself.
        // Memory/role path preserves S1/S3 behavior: thinking:false.
        let opts = EngineGenerateOpts {
            thinking: false,
            max_tokens,
            temperature,
        };

        {
            let mut s = self.state.write().await;
            if let EngineState::Loaded { model, last_used } = &mut *s {
                let (text, it, ot) = model.generate(system, user, opts).await?;
                *last_used = Instant::now();
                return Ok(LocalCompletion {
                    text,
                    input_tokens: it,
                    output_tokens: ot,
                    model: self.model_id.clone(),
                });
            }
        }

        // Slow path: load + warmup, then generate.
        let model = LoadedModel::load(&self.data_dir).await?;
        // Warmup: one cheap forward pass to prime Metal/CUDA kernels.
        let _ = model
            .generate("", "ok", EngineGenerateOpts { thinking: false, max_tokens: 1, temperature: 0.0 })
            .await;
        let (text, it, ot) = model.generate(system, user, opts).await?;
        let mut s = self.state.write().await;
        *s = EngineState::Loaded {
            model,
            last_used: Instant::now(),
        };
        Ok(LocalCompletion {
            text,
            input_tokens: it,
            output_tokens: ot,
            model: self.model_id.clone(),
        })
    }

    /// Stream a generation. Returns the resolved model id plus a receiver of
    /// [`LocalStreamChunk`] (text deltas, then a single `Done`). Lazy-loads +
    /// warms up like `complete`. The pump runs in a spawned task that holds the
    /// engine write lock for the whole stream — serialized, same as `complete`
    /// (the lock is released when the stream finishes / the receiver is dropped).
    ///
    /// `Err` is returned synchronously only for the up-front guards (model
    /// missing / load failure); per-chunk inference errors close the receiver
    /// without a trailing `Done`.
    pub async fn stream(
        self: std::sync::Arc<Self>,
        system: String,
        user: String,
        opts: EngineGenerateOpts,
    ) -> Result<
        (
            String,
            tokio::sync::mpsc::UnboundedReceiver<LocalStreamChunk>,
        ),
        LocalLlmError,
    > {
        if !is_model_present(&self.data_dir, crate::local_llm::active_quant()) {
            return Err(LocalLlmError::ModelMissing);
        }

        // Ensure the model is loaded BEFORE spawning the pump so a load failure
        // surfaces synchronously (the caller can emit "本地模型未就绪").
        {
            let mut s = self.state.write().await;
            if matches!(&*s, EngineState::Unloaded) {
                let model = LoadedModel::load(&self.data_dir).await?;
                let _ = model
                    .generate(
                        "",
                        "ok",
                        EngineGenerateOpts { thinking: false, max_tokens: 1, temperature: 0.0 },
                    )
                    .await;
                *s = EngineState::Loaded {
                    model,
                    last_used: Instant::now(),
                };
            }
        }

        let model_id = self.model_id.clone();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<LocalStreamChunk>();

        // Pump under the write lock (held for the whole stream → serialized).
        let engine = self.clone();
        tokio::spawn(async move {
            let mut s = engine.state.write().await;
            if let EngineState::Loaded { model, last_used } = &mut *s {
                if let Err(e) = model.generate_stream(&system, &user, opts, &tx).await {
                    tracing::warn!(error = %e, "local_llm: stream pump failed");
                    // Receiver sees the channel close (no Done) → caller errors.
                }
                *last_used = Instant::now();
            }
        });

        Ok((model_id, rx))
    }
}

// ---------------------------------------------------------------------------
// Tests — none of these load a real model
// ---------------------------------------------------------------------------

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

    #[test]
    fn strip_think_no_block_returns_input() {
        assert_eq!(strip_think("hello world"), "hello world");
        assert_eq!(strip_think("  no think here"), "  no think here");
    }

    #[test]
    fn strip_think_removes_leading_block() {
        assert_eq!(
            strip_think("<think>reasoning here</think>answer"),
            "answer"
        );
        assert_eq!(
            strip_think("<think>\nmulti\nline\n</think>\n\nThe answer."),
            "The answer."
        );
    }

    #[test]
    fn strip_think_handles_leading_whitespace_before_tag() {
        assert_eq!(strip_think("  \n<think>x</think>hi"), "hi");
    }

    #[test]
    fn strip_think_unclosed_block_returns_empty() {
        assert_eq!(strip_think("<think>still thinking..."), "");
    }

    #[test]
    fn strip_think_only_strips_leading_block() {
        // A <think> not at the start is left intact.
        assert_eq!(
            strip_think("answer first <think>late</think>"),
            "answer first <think>late</think>"
        );
    }

    #[tokio::test]
    async fn unload_if_idle_noop_when_unloaded() {
        let e =
            LocalLlmEngine::with_idle_window(PathBuf::from("/tmp/x"), Duration::from_secs(0));
        e.unload_if_idle().await;
        assert!(!e.is_loaded().await);
    }
}
