// SPDX-License-Identifier: AGPL-3.0-or-later
//! `LocalMistralRsProvider` — implements `LlmProvider` over the in-process
//! `LocalLlmEngine`.  S1: complete() only; stream() returns Unsupported.

use std::sync::Arc;
use async_trait::async_trait;

use crate::agent::types::{
    ChatMessage, ContentBlock, MessageRole, RespondOutput, ResponseMetadata, StreamDelta,
    ToolDefinition, TokenUsage,
};
use crate::error::{Error, LlmError};
use crate::llm::provider::{CompletionConfig, LlmProvider};
use crate::local_llm::engine::LocalLlmEngine;

/// LlmProvider backed by the in-process mistralrs engine.
pub struct LocalMistralRsProvider {
    engine: Arc<LocalLlmEngine>,
    /// Model id to surface in ResponseMetadata (e.g. "minicpm5-1b").
    model_id: String,
}

impl LocalMistralRsProvider {
    /// Construct from an explicit engine handle (useful in tests).
    pub fn new(engine: Arc<LocalLlmEngine>, model_id: String) -> Self {
        Self { engine, model_id }
    }

    /// Construct from the global engine initialized by `init_local_engine`.
    /// Returns `Error::Internal` if the engine was never initialized.
    pub fn from_global(model_id: String) -> Result<Self, Error> {
        let engine = crate::local_llm::local_engine()
            .ok_or_else(|| Error::Internal(
                "local LLM engine not initialized; call init_local_engine at startup".into(),
            ))?;
        Ok(Self { engine, model_id })
    }

    /// Flatten a message list into (system_prompt, user_text).
    ///
    /// Rules:
    /// - All `MessageRole::System` messages are concatenated (newline-separated)
    ///   into the system prompt.
    /// - The last `MessageRole::User` message becomes the user turn.
    ///   If there are no user messages, falls back to the last assistant message,
    ///   and if that's also absent, uses an empty string.
    /// - Only `ContentBlock::Text` blocks are extracted; other block types are
    ///   skipped (tool_use / tool_result / thinking are not meaningful for the
    ///   local model's single-shot interface).
    fn split_messages(messages: &[ChatMessage]) -> (String, String) {
        let mut system_parts: Vec<String> = Vec::new();
        let mut last_user: Option<String> = None;

        for msg in messages {
            // Collect text content from all blocks in this message.
            let text: String = msg
                .content
                .iter()
                .filter_map(|b| {
                    if let ContentBlock::Text { text } = b {
                        Some(text.as_str())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");

            match msg.role {
                MessageRole::System => {
                    if !text.is_empty() {
                        system_parts.push(text);
                    }
                }
                MessageRole::User => {
                    last_user = Some(text);
                }
                MessageRole::Assistant => {
                    // Skip — we only need the last user turn for the local model.
                }
            }
        }

        let system = system_parts.join("\n");
        let user = last_user.unwrap_or_default();
        (system, user)
    }
}

#[async_trait]
impl LlmProvider for LocalMistralRsProvider {
    async fn complete(
        &self,
        messages: Vec<ChatMessage>,
        _tools: Vec<ToolDefinition>,
        config: &CompletionConfig,
    ) -> Result<RespondOutput, Error> {
        let (system, user) = Self::split_messages(&messages);

        let result = self
            .engine
            .complete(&system, &user, config.max_tokens, config.temperature)
            .await
            .map_err(|e| Error::Llm(LlmError::ApiRequestFailed(e.to_string())))?;

        Ok(RespondOutput::Text {
            text: result.text,
            thinking: None,
            thinking_signature: None,
            metadata: ResponseMetadata {
                model: result.model,
                finish_reason: Some("stop".to_string()),
                usage: Some(TokenUsage {
                    input_tokens: result.input_tokens,
                    output_tokens: result.output_tokens,
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
    ) -> Result<Box<dyn futures::Stream<Item = Result<StreamDelta, Error>> + Send + Unpin>, Error>
    {
        Err(Error::Internal(
            "LocalMistralRsProvider: streaming is not supported in S1 (planned for S4)".into(),
        ))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[tokio::test]
    async fn stream_is_unsupported_in_s1() {
        let engine = Arc::new(LocalLlmEngine::new(PathBuf::from("/tmp/x")));
        let provider = LocalMistralRsProvider::new(engine, "minicpm5-1b".into());
        let result = provider
            .stream(vec![], vec![], &CompletionConfig::default())
            .await;
        match result {
            Err(e) => {
                let msg = e.to_string();
                assert!(
                    msg.contains("streaming is not supported"),
                    "unexpected error message: {msg}"
                );
            }
            Ok(_) => panic!("stream() must return Err in S1"),
        }
    }

    #[test]
    fn split_messages_system_and_user() {
        let msgs = vec![
            ChatMessage::system("You are a helpful assistant."),
            ChatMessage::user("Hello, world!"),
        ];
        let (sys, usr) = LocalMistralRsProvider::split_messages(&msgs);
        assert_eq!(sys, "You are a helpful assistant.");
        assert_eq!(usr, "Hello, world!");
    }

    #[test]
    fn split_messages_no_user_returns_empty() {
        let msgs = vec![ChatMessage::system("sys only")];
        let (sys, usr) = LocalMistralRsProvider::split_messages(&msgs);
        assert_eq!(sys, "sys only");
        assert_eq!(usr, "");
    }

    #[test]
    fn split_messages_multiple_system_concatenated() {
        let msgs = vec![
            ChatMessage::system("Part one."),
            ChatMessage::system("Part two."),
            ChatMessage::user("question"),
        ];
        let (sys, _usr) = LocalMistralRsProvider::split_messages(&msgs);
        assert!(sys.contains("Part one."));
        assert!(sys.contains("Part two."));
    }

    #[test]
    fn split_messages_last_user_wins() {
        let msgs = vec![
            ChatMessage::user("first"),
            ChatMessage::assistant("reply"),
            ChatMessage::user("second"),
        ];
        let (_sys, usr) = LocalMistralRsProvider::split_messages(&msgs);
        assert_eq!(usr, "second");
    }
}
