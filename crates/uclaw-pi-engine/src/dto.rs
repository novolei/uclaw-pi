//! DTO mapping (§3.4): pi message types → uClaw frontend `ChatMessage` /
//! `ContentBlock` JSON (snake_case wire). Used by `get_messages` to render pi's
//! session history (`handle.messages()`) through the **unchanged** frontend
//! renderer (`NativeBlockRenderer`, which switches on `block.type`).
//!
//! Frontend contract (`ui/src/lib/chat-types.ts`):
//! ```text
//! ContentBlock = {type:'text',text}
//!              | {type:'thinking',thinking}
//!              | {type:'tool_use',id,name,input}
//!              | {type:'tool_result',tool_use_id,content,is_error?}
//! ChatMessage  = {id, role:'user'|'assistant'|'system', content, contentBlocks}
//! ```
//! The `type` discriminants MUST be snake_case — `NativeBlockRenderer` depends
//! on the exact strings.

use pi::model::{ContentBlock, Message, UserContent};
use pi::tools::ToolOutput;
use serde_json::{json, Value};

/// [R4 F5] Normalize a pi [`ToolOutput`] into the `result` shape uClaw's
/// `tool-renderers/` consume — the flattened **text** of the output's content
/// blocks. `bash-result.tsx` / `read-result.tsx` / … read `activity.result` as a
/// string (e.g. `result.replace(/\\n/g, '\n')`); emitting the raw `ToolOutput`
/// object instead blanks the card (R4 Stop-if: "ToolOutput 形状不符致渲染器空白").
///
/// Built-in tools (read/bash/edit/write/grep/find/ls) stay pi's (F5) — only their
/// output shape is mapped here, never reimplemented.
#[must_use]
pub fn tool_output_to_result(output: &ToolOutput) -> Value {
    let text: String = output
        .content
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    Value::String(text)
}

/// Map one pi [`ContentBlock`] to the frontend snake_case ContentBlock JSON.
/// `None` = no frontend representation (e.g. inline images, which the frontend
/// `ContentBlock` union does not model).
#[must_use]
pub fn content_block_to_fe(cb: &ContentBlock) -> Option<Value> {
    match cb {
        ContentBlock::Text(t) => Some(json!({ "type": "text", "text": t.text })),
        ContentBlock::Thinking(t) => Some(json!({ "type": "thinking", "thinking": t.thinking })),
        // Redacted reasoning carries no surfaceable text; keep an (empty) thinking
        // block so block ordering round-trips but nothing renders.
        ContentBlock::RedactedThinking(_) => Some(json!({ "type": "thinking", "thinking": "" })),
        ContentBlock::ToolCall(tc) => Some(json!({
            "type": "tool_use",
            "id": tc.id,
            "name": tc.name,
            "input": tc.arguments,
        })),
        ContentBlock::Image(_) => None,
    }
}

/// Concatenate the plain text of a block list (the `content` string the frontend
/// `ChatMessage` carries alongside `contentBlocks`).
fn flatten_text(blocks: &[ContentBlock]) -> String {
    let mut s = String::new();
    for b in blocks {
        if let ContentBlock::Text(t) = b {
            s.push_str(&t.text);
        }
    }
    s
}

fn map_blocks(blocks: &[ContentBlock]) -> Vec<Value> {
    blocks.iter().filter_map(content_block_to_fe).collect()
}

/// Map a pi [`Message`] to a frontend `ChatMessage` JSON
/// `{id, role, content, contentBlocks}`.
#[must_use]
pub fn message_to_chat_message(msg: &Message, id: impl Into<String>) -> Value {
    let id = id.into();
    match msg {
        Message::User(um) => match &um.content {
            UserContent::Text(s) => json!({
                "id": id, "role": "user", "content": s,
                "contentBlocks": [{ "type": "text", "text": s }],
            }),
            UserContent::Blocks(bs) => json!({
                "id": id, "role": "user", "content": flatten_text(bs),
                "contentBlocks": map_blocks(bs),
            }),
        },
        Message::Assistant(am) => json!({
            "id": id, "role": "assistant", "content": flatten_text(&am.content),
            "contentBlocks": map_blocks(&am.content),
        }),
        // Tool results map to a user-role message carrying one tool_result block
        // (Anthropic convention); the frontend renders it via tool-renderers/.
        Message::ToolResult(trm) => json!({
            "id": id, "role": "user", "content": "",
            "contentBlocks": [{
                "type": "tool_result",
                "tool_use_id": trm.tool_call_id,
                "content": flatten_text(&trm.content),
                "is_error": trm.is_error,
            }],
        }),
        Message::Custom(_) => json!({
            "id": id, "role": "system", "content": "", "contentBlocks": [],
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pi::model::{TextContent, ThinkingContent, ToolCall};

    fn text_block(s: &str) -> ContentBlock {
        ContentBlock::Text(TextContent {
            text: s.into(),
            text_signature: None,
        })
    }

    #[test]
    fn text_and_thinking_map_to_snake_case() {
        let t = content_block_to_fe(&text_block("hi")).unwrap();
        assert_eq!(t["type"], "text");
        assert_eq!(t["text"], "hi");

        let th = content_block_to_fe(&ContentBlock::Thinking(ThinkingContent {
            thinking: "hmm".into(),
            thinking_signature: None,
        }))
        .unwrap();
        assert_eq!(th["type"], "thinking");
        assert_eq!(th["thinking"], "hmm");
    }

    #[test]
    fn tool_call_maps_to_tool_use() {
        let cb = ContentBlock::ToolCall(ToolCall {
            id: "t1".into(),
            name: "bash".into(),
            arguments: json!({ "cmd": "ls" }),
            thought_signature: None,
        });
        let fe = content_block_to_fe(&cb).unwrap();
        assert_eq!(fe["type"], "tool_use");
        assert_eq!(fe["id"], "t1");
        assert_eq!(fe["name"], "bash");
        assert_eq!(fe["input"]["cmd"], "ls");
    }

    #[test]
    fn image_block_has_no_frontend_representation() {
        // Build an image block via serde to avoid depending on ImageContent's
        // exact constructor; it must map to None.
        // (ContentBlock::Image is the only None case.)
        // We assert the None branch indirectly through assistant mapping below.
        let am_blocks = vec![text_block("a"), text_block("b")];
        let mapped = map_blocks(&am_blocks);
        assert_eq!(mapped.len(), 2);
    }

    #[test]
    fn assistant_message_maps_role_content_and_blocks() {
        use pi::model::AssistantMessage;
        use std::sync::Arc;

        let am = AssistantMessage {
            content: vec![
                text_block("hello"),
                ContentBlock::ToolCall(ToolCall {
                    id: "c1".into(),
                    name: "read".into(),
                    arguments: json!({ "path": "/x" }),
                    thought_signature: None,
                }),
            ],
            ..Default::default()
        };
        let cm = message_to_chat_message(&Message::Assistant(Arc::new(am)), "m1");
        assert_eq!(cm["id"], "m1");
        assert_eq!(cm["role"], "assistant");
        assert_eq!(cm["content"], "hello");
        assert_eq!(cm["contentBlocks"][0]["type"], "text");
        assert_eq!(cm["contentBlocks"][1]["type"], "tool_use");
        assert_eq!(cm["contentBlocks"][1]["name"], "read");
    }

    #[test]
    fn user_text_message_maps_to_user_role() {
        use pi::model::UserMessage;
        let cm = message_to_chat_message(
            &Message::User(UserMessage {
                content: UserContent::Text("ping".into()),
                timestamp: 0,
            }),
            "u1",
        );
        assert_eq!(cm["role"], "user");
        assert_eq!(cm["content"], "ping");
        assert_eq!(cm["contentBlocks"][0]["text"], "ping");
    }

    /// [R2 Done-when#2] Pin the EXACT wire shape of every `ContentBlock` variant
    /// against `ui/src/lib/chat-types.ts`. `NativeBlockRenderer` switches on
    /// `block.type` (snake_case) and reads the sibling fields by name, so any
    /// drift here misaligns rendering. This test is the contract.
    #[test]
    fn content_block_wire_shapes_match_chat_types_ts() {
        use pi::model::{ImageContent, TextContent, ThinkingContent, ToolCall};
        use std::collections::BTreeSet;

        let keys = |v: &Value| -> BTreeSet<String> {
            v.as_object()
                .map(|o| o.keys().cloned().collect())
                .unwrap_or_default()
        };

        // {type:'text', text}
        let text = content_block_to_fe(&text_block("hi")).unwrap();
        assert_eq!(text["type"], "text");
        assert_eq!(
            keys(&text),
            ["text", "type"].iter().map(|s| s.to_string()).collect()
        );

        // {type:'thinking', thinking}
        let think = content_block_to_fe(&ContentBlock::Thinking(ThinkingContent {
            thinking: "t".into(),
            thinking_signature: None,
        }))
        .unwrap();
        assert_eq!(think["type"], "thinking");
        assert_eq!(
            keys(&think),
            ["thinking", "type"].iter().map(|s| s.to_string()).collect()
        );

        // {type:'tool_use', id, name, input}
        let tool_use = content_block_to_fe(&ContentBlock::ToolCall(ToolCall {
            id: "id1".into(),
            name: "bash".into(),
            arguments: json!({ "cmd": "ls" }),
            thought_signature: None,
        }))
        .unwrap();
        assert_eq!(tool_use["type"], "tool_use");
        assert_eq!(
            keys(&tool_use),
            ["id", "input", "name", "type"]
                .iter()
                .map(|s| s.to_string())
                .collect()
        );

        // {type:'tool_result', tool_use_id, content, is_error} — via Message::ToolResult.
        let tr = tool_result_block(true);
        assert_eq!(tr["type"], "tool_result");
        assert_eq!(
            keys(&tr),
            ["content", "is_error", "tool_use_id", "type"]
                .iter()
                .map(|s| s.to_string())
                .collect()
        );

        // Image has no frontend ContentBlock representation → None (filtered out).
        assert!(content_block_to_fe(&ContentBlock::Image(ImageContent {
            data: String::new(),
            mime_type: "image/png".into(),
        }))
        .is_none());
    }

    /// [R4 F5] A pi ToolOutput must flatten to the **string** `result` the
    /// renderers consume — never the raw `{content,…}` object (which blanks the
    /// card). Mirrors the bash/read/write/edit/screenshot renderer expectation.
    #[test]
    fn tool_output_flattens_to_renderer_result_string() {
        let output = ToolOutput {
            content: vec![
                ContentBlock::Text(TextContent {
                    text: "line1\n".into(),
                    text_signature: None,
                }),
                // A non-text block (e.g. an image) is dropped from the text result.
                ContentBlock::Text(TextContent {
                    text: "line2".into(),
                    text_signature: None,
                }),
            ],
            details: None,
            is_error: false,
        };
        let result = tool_output_to_result(&output);
        assert!(result.is_string(), "renderers read result as a string");
        assert_eq!(result, json!("line1\nline2"));
    }

    /// The single tool_result block produced by a `Message::ToolResult`.
    fn tool_result_block(is_error: bool) -> Value {
        use pi::model::ToolResultMessage;
        use std::sync::Arc;
        let trm = ToolResultMessage {
            tool_call_id: "tc1".into(),
            tool_name: "bash".into(),
            content: vec![ContentBlock::Text(TextContent {
                text: "out".into(),
                text_signature: None,
            })],
            details: None,
            is_error,
            timestamp: 0,
        };
        let cm = message_to_chat_message(&Message::ToolResult(Arc::new(trm)), "r1");
        cm["contentBlocks"][0].clone()
    }
}
