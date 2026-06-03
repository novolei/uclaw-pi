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
        let dir = crate::local_llm::paths::model_dir(data_dir);
        let inner = GgufModelBuilder::new(
            dir.to_string_lossy().to_string(),
            vec![crate::local_llm::paths::MODEL_FILE.to_string()],
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
        max_tokens: u32,
        temperature: f32,
    ) -> Result<(String, u32, u32), LocalLlmError> {
        use mistralrs::{TextMessageRole, RequestBuilder};
        // enable_thinking(false): MiniCPM5 is a reasoning model; for short
        // single-shot memory tasks the <think> block would eat the whole budget
        // and return empty content. (Spike-confirmed; S4 pet may re-enable.)
        let req = RequestBuilder::new()
            .add_message(TextMessageRole::System, system)
            .add_message(TextMessageRole::User, user)
            .set_sampler_max_len(max_tokens as usize)
            .set_sampler_temperature(temperature as f64)
            .enable_thinking(false);
        let resp = self
            .inner
            .send_chat_request(req)
            .await
            .map_err(|e| LocalLlmError::Inference(e.to_string()))?;
        let text = resp
            .choices
            .first()
            .and_then(|c| c.message.content.clone())
            .unwrap_or_default();
        let usage = resp.usage;
        Ok((text, usage.prompt_tokens as u32, usage.completion_tokens as u32))
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
        if !is_model_present(&self.data_dir) {
            return Err(LocalLlmError::ModelMissing);
        }

        // Fast path: model already loaded — generate under the write lock so
        // last_used is updated atomically. The write lock is brief; the real
        // latency is inside generate() (GPU/CPU bound), not in the lock itself.
        {
            let mut s = self.state.write().await;
            if let EngineState::Loaded { model, last_used } = &mut *s {
                let (text, it, ot) =
                    model.generate(system, user, max_tokens, temperature).await?;
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
        let _ = model.generate("", "ok", 1, 0.0).await;
        let (text, it, ot) = model.generate(system, user, max_tokens, temperature).await?;
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

    #[tokio::test]
    async fn unload_if_idle_noop_when_unloaded() {
        let e =
            LocalLlmEngine::with_idle_window(PathBuf::from("/tmp/x"), Duration::from_secs(0));
        e.unload_if_idle().await;
        assert!(!e.is_loaded().await);
    }
}
