//! HARD-GATE spike: prove mistralrs can load MiniCPM5-1B-GGUF and generate.
//!
//! Run manually after placing the GGUF at the path below:
//!   cargo test --lib local_llm::spike_test -- --ignored --nocapture
//!
//! ============================================================================
//! CONFIRMED mistralrs API (Tasks 3-4 copy this VERBATIM)
//! ----------------------------------------------------------------------------
//! Pinned crate:  mistralrs = "0.8.1"   (latest stable on crates.io 2026-06-03;
//!                macOS gets the `metal` feature via the [target.'cfg(macos)']
//!                block in src-tauri/Cargo.toml). Backend: candle 0.10.
//!
//! tokio compatibility: mistralrs 0.8.1 requires tokio ^1.49.0; the workspace
//! resolves tokio 1.52.3 → COMPATIBLE, single tokio in the graph. (reqwest:
//! mistralrs pulls reqwest 0.13, the app uses 0.12; semver-major-incompatible
//! so they coexist as two separate crates — not a conflict.)
//!
//! --- Build a GGUF model in-process ---
//!   use mistralrs::GgufModelBuilder;
//!   // new(model_id: impl ToString, files: Vec<impl ToString>)
//!   let model: mistralrs::Model = GgufModelBuilder::new(dir, vec![file])
//!       // optional: .with_logging()         // mistralrs tracing
//!       // optional: .with_throughput_logging()
//!       // optional: .with_force_cpu()        // CPU fallback
//!       .build()                              // -> anyhow::Result<Model>
//!       .await?;
//!
//! --- Send a chat request with system+user + max_tokens/temperature ---
//!   use mistralrs::{RequestBuilder, TextMessageRole};
//!   let req = RequestBuilder::new()
//!       .add_message(TextMessageRole::System, system_str)   // add_message(role, impl ToString) -> Self
//!       .add_message(TextMessageRole::User,   user_str)
//!       .set_sampler_max_len(max_tokens as usize)           // usize
//!       .set_sampler_temperature(temperature as f64);       // f64
//!   // (RequestBuilder impls RequestLike; Model::send_chat_request<R: RequestLike>.
//!   //  For zero-config determinism, TextMessages also impls RequestLike and can
//!   //  be passed directly, but it has no sampler setters — use RequestBuilder
//!   //  whenever max_tokens/temperature must be set.)
//!   let resp: mistralrs::ChatCompletionResponse = model.send_chat_request(req).await?;
//!
//! --- Read generated text + token usage from the response ---
//!   // ChatCompletionResponse { id, choices: Vec<Choice>, created, model,
//!   //                          system_fingerprint, object, usage: Usage }
//!   // Choice { finish_reason: String, index: usize, message: ResponseMessage, .. }
//!   // ResponseMessage { content: Option<String>, role: String, tool_calls, .. }
//!   let text: String = resp.choices.first()
//!       .and_then(|c| c.message.content.clone())
//!       .unwrap_or_default();
//!   // Usage { completion_tokens: usize, prompt_tokens: usize, total_tokens: usize,
//!   //         avg_tok_per_sec: f32, .. }  — all token counts are usize.
//!   let input_tokens:  u32 = resp.usage.prompt_tokens     as u32;
//!   let output_tokens: u32 = resp.usage.completion_tokens as u32;
//!
//! ⚠️  CRITICAL for Tasks 3-4 — MiniCPM5 is a REASONING model.
//! The MiniCPM5-1B-GGUF ships an embedded Qwen-style chat template
//! (`<|im_start|>` … `<|im_start|>assistant`) that, by default, emits a
//! `<think> … </think>` block BEFORE the answer. The response `content` is the
//! text AFTER `</think>`. Consequences the engine layer MUST handle:
//!   • For short single-shot memory tasks (summarizer/utility) call
//!     `RequestBuilder::enable_thinking(false)` — otherwise a small max_tokens is
//!     spent entirely inside `<think>` and `content` comes back EMPTY (observed:
//!     max_len=32 → finish_reason="length", completion_tokens=32, text="").
//!   • With `enable_thinking(false)` + max_len=256 the spike got a clean
//!     finish_reason="stop", completion_tokens=2, text="ok". ✅
//!   • Alternatively, give large headroom and strip the `<think>…</think>`
//!     prefix yourself. The simplest robust default for S1 memory roles is
//!     `enable_thinking(false)`.
//!
//! CONFIRMED on hardware (Tier C, 2026-06-03, Apple Silicon / Metal):
//!   • general.architecture: llama (plain LlamaForCausalLM) → loads fine.
//!   • Metal GPU IS used: load log "Layers 0-23: metal[...]".
//!   • DType BF16; 24 layers; vocab 130560; gpt2/BPE tokenizer.
//!   • GGUF filename on ModelScope: MiniCPM5-1B-Q4_K_M.gguf (688 MB).
//! ============================================================================

#[tokio::test]
#[ignore = "needs MiniCPM5-1B-GGUF on disk; slow; run manually"]
async fn spike_loads_minicpm5_and_generates() {
    // Place the Q4_K_M GGUF here for the spike. NOTE: confirm the real filename
    // against the live ModelScope repo (Tier C) and update `gguf` to match.
    let dir = format!(
        "{}/.uclaw-pi/models/minicpm5-1b",
        std::env::var("HOME").unwrap()
    );
    let gguf = "MiniCPM5-1B-Q4_K_M.gguf"; // adjust to the real filename you downloaded

    use mistralrs::{GgufModelBuilder, RequestBuilder, TextMessageRole};

    let model = GgufModelBuilder::new(dir, vec![gguf.to_string()])
        .with_logging()
        .build()
        .await
        .expect(
            "mistralrs failed to load MiniCPM5-1B-GGUF — SPIKE FAILED, escalate to llama-cpp-2",
        );

    // MiniCPM5 is a reasoning model: its GGUF chat template emits a <think>…</think>
    // block before the answer. With a tiny max_len the whole budget is spent inside
    // <think> and the post-think `content` is empty. Give it real headroom (and
    // disable thinking) so the spike sees actual answer text.
    let req = RequestBuilder::new()
        .add_message(TextMessageRole::System, "You are concise.")
        .add_message(TextMessageRole::User, "Reply with exactly: ok")
        .enable_thinking(false)
        .set_sampler_max_len(256)
        .set_sampler_temperature(0.7);

    let resp = model
        .send_chat_request(req)
        .await
        .expect("generation failed");

    let choice = resp.choices.first();
    let text = choice
        .and_then(|c| c.message.content.clone())
        .unwrap_or_default();
    eprintln!(
        "[SPIKE] finish_reason={:?} role={:?} text={text:?}; prompt_tokens={} completion_tokens={}",
        choice.map(|c| c.finish_reason.clone()),
        choice.map(|c| c.message.role.clone()),
        resp.usage.prompt_tokens,
        resp.usage.completion_tokens
    );

    // Fallback diagnostic: the bare chat() helper (single user turn, default sampler).
    let bare = model.chat("Reply with exactly: ok").await;
    eprintln!("[SPIKE] bare chat() -> {bare:?}");

    assert!(!text.trim().is_empty(), "model produced empty output");
}
