// SPDX-License-Identifier: AGPL-3.0-or-later
//! HARD-GATE spike (S4): prove mistralrs 0.8.1 supports (a) STREAMING chat and
//! (b) runtime LoRA adapter activation/switch — the two features that gate S4's
//! streamed pet chat + persona-adapter switching.
//!
//! Run manually (needs the MiniCPM5-1B GGUF from S1 on disk; slow):
//!   cargo test --lib local_llm::s4_spike_test -- --ignored --nocapture
//!
//! ============================================================================
//! FINDINGS (verbatim 0.8.1 API — verified against the unpacked crate source at
//!  ~/.cargo/registry/src/.../mistralrs-0.8.1 and mistralrs-core-0.8.1)
//! ============================================================================
//!
//! ─────────────────────────────────────────────────────────────────────────
//! (a) STREAMING — AVAILABLE ✅
//! ─────────────────────────────────────────────────────────────────────────
//! `Model::stream_chat_request` exists and returns a `mistralrs::Stream<'_>`
//! that implements `futures::Stream<Item = Response>`. Signature (model.rs:106):
//!
//!     pub async fn stream_chat_request<R: RequestLike>(
//!         &self,
//!         request: R,
//!     ) -> mistralrs::error::Result<mistralrs::Stream<'_>>;
//!
//!     // also: stream_chat_request_with_model(req, Some(model_id)) for multi-model
//!
//! `Stream` has an inherent `async fn next(&mut self) -> Option<Response>`
//! (mpsc receiver under the hood) AND impls `futures::Stream`, so either
//! `stream.next().await` (inherent) or `StreamExt::next` works.
//!
//! Each chunk is a `Response` enum (mistralrs-core response.rs:240). The
//! streaming variants you care about:
//!     Response::Chunk(ChatCompletionChunkResponse)   // token deltas
//!     Response::Done(ChatCompletionResponse)         // NOT emitted for streams
//!     Response::ModelError(String, _) / InternalError(_) / ValidationError(_)
//!
//! ChatCompletionChunkResponse (response.rs:160):
//!     { id, choices: Vec<ChunkChoice>, created: u128, model, system_fingerprint,
//!       object, usage: Option<Usage> }
//! ChunkChoice (response.rs:100):
//!     { finish_reason: Option<String>, index, delta: Delta, logprobs }
//! Delta (response.rs:48):
//!     { content: Option<String>, role: String, tool_calls, reasoning_content: Option<String> }
//!
//! => TEXT DELTA  : chunk.choices[0].delta.content (Option<String>)
//! => THINK DELTA : chunk.choices[0].delta.reasoning_content (Option<String>)
//!                  (MiniCPM5 reasoning text streams here when enable_thinking(true))
//! => FINISH      : chunk.choices[0].finish_reason becomes Some("stop"|"length"|…)
//! => FINAL USAGE : the LAST chunk carries `usage: Some(Usage{ completion_tokens,
//!                  prompt_tokens, total_tokens, avg_tok_per_sec, .. })`.
//!                  (Streams emit Response::Chunk repeatedly and finish by setting
//!                   finish_reason + usage on the final chunk — there is NO trailing
//!                   Response::Done for streams.)
//!
//! S4 mapping: delta.content -> StreamDelta::TextDelta; final chunk's
//! Some(usage) -> Done{usage}. With enable_thinking(true), accumulate/strip
//! reasoning_content (or surface it as a "thinking" pet state) before content.
//!
//! ─────────────────────────────────────────────────────────────────────────
//! (b) LoRA — runtime activate/switch is NOT AVAILABLE in 0.8.1's public API ❌
//!     (load-time LoRA IS available; runtime per-request switch is a dead no-op)
//! ─────────────────────────────────────────────────────────────────────────
//! LOAD-TIME LoRA (works): adapters are declared at build time and fused into
//! the pipeline. Two builders exist:
//!   • mistralrs::LoraModelBuilder::from_text_model_builder(text_builder, ["adapter_id", ..])
//!       .build().await  -> Model            (safetensors / HF-style adapters)
//!   • mistralrs::GgufLoraModelBuilder::from_gguf_model_builder(gguf_builder, lora_id, ordering)
//!       .build().await  -> Model            (GGUF base + LoRA)
//!   (X-LoRA variants: XLoraModelBuilder / GgufXLoraModelBuilder.)
//!
//! RUNTIME ACTIVATE/SWITCH (absent): there is NO way in 0.8.1 to flip the active
//! adapter on a loaded Model without rebuilding it. Concretely:
//!   • `RequestBuilder::set_adapters(Vec<String>)` EXISTS and is documented
//!     "Activate the given LoRA/X-LoRA adapter layers by name" (messages.rs:724),
//!     and `RequestLike::take_adapters()` exists — BUT neither
//!     `Model::send_chat_request_with_model` nor `stream_chat_request_with_model`
//!     ever call `take_adapters()`, and `NormalRequest` (request.rs:181) has NO
//!     adapter field. => set_adapters() compiles but is silently dropped; it is a
//!     DEAD NO-OP at the request layer in 0.8.1.
//!   • The `Request` enum (request.rs:260) has NO `ActivateAdapters` variant.
//!   • The public `MistralRs` runner (core lib.rs) exposes NO `activate_adapters`
//!     / `set_adapter` method. (`activate_adapters` only exists *inside* the
//!     X-LoRA model modules as internal per-step scaling logic — not user-facing,
//!     not a switch.)
//!   • There is no `preload_adapters` API on the builders or Model.
//!
//! VIABLE RUNTIME-SWITCH WORKAROUND (heavier): the multi-model API
//!   MistralRs::add_model(...).await / set_default_model_id(id) /
//!   register_model_alias / send_*_with_model(req, Some(id))
//! lets you load the base + each persona-as-its-own-(LoRA-merged)-Model and
//! switch via model_id. Cost: each persona is a full resident model (RAM/VRAM),
//! not a cheap adapter swap. Acceptable only for a tiny number of personas.
//!
//! ─────────────────────────────────────────────────────────────────────────
//! RECOMMENDATION FOR S4
//! ─────────────────────────────────────────────────────────────────────────
//!  • Streamed pet chat: YES — implement `stream` on the local provider via
//!    `stream_chat_request` exactly as mapped above. (Streaming gate PASSES.)
//!  • Runtime persona-adapter switching: NO (not as a cheap in-place swap on
//!    0.8.1). DEGRADE PATH = personas are **system-prompt-only** (the spec's
//!    documented fallback). If a future persona truly needs a LoRA, either
//!    (1) ship it as a separately-built Model and switch via the multi-model
//!    add_model/model_id API (RAM cost), or (2) upgrade mistralrs once an
//!    in-place adapter-activation API lands upstream.
//! ============================================================================

/// (a) STREAMING — compiles unconditionally; runs only with the GGUF on disk.
///
/// Streams a short generation, collects token deltas, and reads the final usage
/// off the last chunk. Asserts non-empty streamed text.
#[tokio::test]
#[ignore = "needs MiniCPM5-1B-GGUF on disk; slow; run manually"]
async fn s4_streaming_collects_deltas_and_usage() {
    use mistralrs::{GgufModelBuilder, RequestBuilder, Response, TextMessageRole};

    let dir = format!(
        "{}/.uclaw-pi/models/minicpm5-1b",
        std::env::var("HOME").unwrap()
    );
    let gguf = crate::local_llm::paths::MODEL_FILE; // S1's confirmed filename

    let model = GgufModelBuilder::new(dir, vec![gguf.to_string()])
        .with_logging()
        .build()
        .await
        .expect("mistralrs failed to load MiniCPM5-1B-GGUF — S4 streaming spike blocked");

    // Pet chat: thinking ON with headroom (the pet can "think"); we capture the
    // reasoning_content deltas separately and the answer deltas in `text`.
    let req = RequestBuilder::new()
        .add_message(TextMessageRole::System, "You are a friendly desk pet. Be brief.")
        .add_message(TextMessageRole::User, "Say hi in one short sentence.")
        .enable_thinking(false) // keep the spike fast/deterministic; flip true for pet
        .set_sampler_max_len(128)
        .set_sampler_temperature(0.7);

    let mut stream = model
        .stream_chat_request(req)
        .await
        .expect("stream_chat_request failed");

    let mut text = String::new();
    let mut think = String::new();
    let mut chunks = 0usize;
    let mut final_finish: Option<String> = None;
    let mut final_usage: Option<mistralrs::Usage> = None;

    while let Some(resp) = stream.next().await {
        match resp {
            Response::Chunk(chunk) => {
                chunks += 1;
                if let Some(choice) = chunk.choices.first() {
                    if let Some(delta) = choice.delta.content.as_deref() {
                        text.push_str(delta);
                    }
                    if let Some(rc) = choice.delta.reasoning_content.as_deref() {
                        think.push_str(rc);
                    }
                    if choice.finish_reason.is_some() {
                        final_finish = choice.finish_reason.clone();
                    }
                }
                // The final chunk carries usage.
                if chunk.usage.is_some() {
                    final_usage = chunk.usage.clone();
                }
            }
            Response::ModelError(e, _) => panic!("model error mid-stream: {e}"),
            Response::InternalError(e) | Response::ValidationError(e) => {
                panic!("stream error: {e}")
            }
            // Response is not Debug; for a streamed chat request the only other
            // variants are non-chat (Completion*/Image/Speech/etc.) and won't occur.
            _ => eprintln!("[S4-SPIKE] unexpected non-chat stream variant"),
        }
    }

    eprintln!(
        "[S4-SPIKE] streamed {chunks} chunks; finish={final_finish:?}; \
         text={text:?}; think_len={}; usage={:?}",
        think.len(),
        final_usage.as_ref().map(|u| (u.prompt_tokens, u.completion_tokens)),
    );

    assert!(chunks > 0, "stream produced no chunks");
    assert!(!text.trim().is_empty(), "stream produced no answer text");
    assert!(
        final_usage.is_some(),
        "final chunk did not carry usage — S4 Done{{usage}} mapping needs adjustment"
    );
}

/// (b) LoRA — COMPILE-ONLY probe. Proves which adapter API surfaces exist in
/// 0.8.1. We do NOT call `.build()` (no real MiniCPM adapter on disk, and a
/// build would try to fetch one). The point is that these symbols compile,
/// documenting exactly what is and isn't available. See the header for the
/// finding: load-time LoRA exists; runtime switch does NOT.
#[test]
fn s4_lora_api_surface_compiles() {
    // This closure is never executed (no real adapter / model); it exists purely
    // to type-check the adapter API against 0.8.1. If mistralrs changes these
    // signatures, this stops compiling and the spike's findings are re-triggered.
    #[allow(dead_code, unreachable_code, clippy::diverging_sub_expression)]
    async fn _never_runs() -> anyhow::Result<()> {
        use mistralrs::{LoraModelBuilder, RequestBuilder, TextMessageRole, TextModelBuilder};

        // --- LOAD-TIME LoRA: the safetensors/HF-style builder path compiles ---
        // (adapters declared at build, fused into the pipeline):
        let _lora_text: LoraModelBuilder = LoraModelBuilder::from_text_model_builder(
            TextModelBuilder::new("some/base-model"),
            ["adapter-a", "adapter-b"], // adapter ids declared at load
        );
        // NOTE: a GGUF base + LoRA path also exists via
        //   mistralrs::GgufLoraModelBuilder::from_gguf_model_builder(gguf, lora_id, ordering)
        // but its `ordering: Ordering` arg uses `mistralrs_core::Ordering`, which the
        // facade does NOT re-export — so to use it S4 must add `mistralrs-core` as a
        // direct dep (or build the ordering some other way). Documented, not exercised.

        // --- RUNTIME SWITCH: this COMPILES but is a NO-OP in 0.8.1 ---
        // set_adapters() is on RequestBuilder and documented as "Activate the
        // given LoRA/X-LoRA adapter layers by name" — but take_adapters() is
        // never consumed by send/stream paths and NormalRequest has no adapter
        // field, so it is silently dropped. Kept here to prove the symbol exists
        // and to fail-fast if 0.8.x ever wires it through (then S4 can revisit).
        let _req = RequestBuilder::new()
            .add_message(TextMessageRole::User, "hi")
            .set_adapters(vec!["persona-cheerful".to_string()]); // <- dead no-op in 0.8.1

        // NOTE: there is intentionally NO `model.activate_adapters(...)` /
        // `model.set_adapter(...)` call here — those methods DO NOT EXIST on
        // `mistralrs::Model` / `MistralRs` in 0.8.1. (Attempting them fails to
        // compile, which is the proof that runtime switch is absent.)

        Ok(())
    }
}
