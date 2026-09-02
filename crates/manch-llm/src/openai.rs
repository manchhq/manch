//! BYOK OpenAI Chat Completions client (Codex BYOK path).

use std::sync::Arc;

use async_trait::async_trait;
use manch_protocol::acp::{ContentBlock, StopReason, ToolCallContent};
use manch_protocol::{
    Agent, Context, Entry, EventSink, Result, Role, ToolInvocation, ToolSchema, Turn,
};

use crate::{ModelInfo, SseItem, ensure_crypto_provider, err, token_count, turn_text};

pub(crate) const DEFAULT_BASE: &str = "https://api.openai.com/v1";
// Stable chat alias — resolves to the current GPT-5 chat snapshot and works with
// Chat Completions, so it won't rot like a pinned id. Only hit if list-models fails.
pub(crate) const FALLBACK_MODEL: &str = "gpt-5-chat-latest";

pub struct OpenAiAgent {
    api_key: String,
    model: String,
    base: String,
}

impl OpenAiAgent {
    pub fn new(api_key: String, model: Option<String>) -> Self {
        Self {
            api_key,
            model: model.unwrap_or_else(|| FALLBACK_MODEL.to_string()),
            base: crate::resolve_base("openai", None, DEFAULT_BASE),
        }
    }

    /// Point this agent at an OpenAI-compatible endpoint — a managed-tier proxy,
    /// or a third party such as Fireworks. Wins over `MANCH_OPENAI_BASE_URL`.
    #[must_use]
    pub fn base_url(mut self, base: impl Into<String>) -> Self {
        let base = base.into();
        self.base = crate::pick_base(Some(&base), None, &self.base);
        self
    }

    /// The endpoint this agent resolved to. Thread it into
    /// [`list_models_at`] so a redirected agent lists ITS catalog rather than
    /// the vendor's — nothing does that automatically, because `list_models`
    /// is a free function with no agent to read a base from.
    #[must_use]
    pub fn base(&self) -> &str {
        &self.base
    }
}

/// `{base}/chat/completions`. Pure.
pub(crate) fn completions_url(base: &str) -> String {
    format!("{base}/chat/completions")
}

/// `{base}/models`. Pure.
pub(crate) fn models_url(base: &str) -> String {
    format!("{base}/models")
}

/// Build the `tools` array OpenAI expects: `[{ type: "function", function: {
/// name, description, parameters } }]`. `parameters` is already the JSON
/// Schema a `Tool` declares — only the envelope is OpenAI-specific. Pure.
pub(crate) fn tools_json(tools: &[ToolSchema]) -> serde_json::Value {
    serde_json::Value::Array(
        tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.input_schema,
                    },
                })
            })
            .collect(),
    )
}

/// Flatten one tool result's content blocks into the plain string OpenAI's
/// `tool` message expects. Only the text case is meaningful on today's `Tool`
/// surface; anything else (a future `Diff`/`Terminal` content kind) still
/// serialises rather than panicking, so an unexpected content kind degrades
/// instead of dropping the result.
fn tool_result_text(content: &[ToolCallContent]) -> String {
    content
        .iter()
        .map(|c| match c {
            ToolCallContent::Content(inner) => match &inner.content {
                ContentBlock::Text(t) => t.text.clone(),
                other => serde_json::to_string(other).unwrap_or_default(),
            },
            other => serde_json::to_string(other).unwrap_or_default(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Map one turn onto the OpenAI message(s) it becomes. Usually one message,
/// but a turn holding tool results expands to one `tool`-role message per
/// result, since each carries its own `tool_call_id` — OpenAI has no
/// equivalent of Anthropic's single message with a content-block array.
fn turn_messages(turn: &Turn) -> Vec<serde_json::Value> {
    let role = match turn.role {
        Role::User => "user",
        Role::Assistant => "assistant",
    };

    let tool_calls: Vec<serde_json::Value> = turn
        .entries
        .iter()
        .filter_map(|e| match e {
            Entry::ToolCall(ToolInvocation {
                id,
                name,
                arguments,
            }) => Some(serde_json::json!({
                "id": id,
                "type": "function",
                "function": {
                    "name": name,
                    // OpenAI's function.arguments is a JSON string, not an object.
                    "arguments": serde_json::to_string(arguments).unwrap_or_default(),
                },
            })),
            _ => None,
        })
        .collect();

    let mut messages = Vec::new();
    let has_block = turn.entries.iter().any(|e| matches!(e, Entry::Block(_)));

    // A narrated call ("let me look that up", then the call) rides on ONE
    // assistant message. OpenAI requires the `tool` replies to *immediately*
    // follow the message carrying `tool_calls`, so emitting the narration as a
    // separate assistant message would both wedge an illegal message in between
    // and reverse the real order. An assistant message may carry `content` and
    // `tool_calls` together — that is the shape the API itself returns.
    let absorbed = !tool_calls.is_empty();
    if absorbed {
        messages.push(serde_json::json!({
            "role": "assistant",
            "content": if has_block {
                serde_json::Value::String(turn_text(turn))
            } else {
                serde_json::Value::Null
            },
            "tool_calls": tool_calls,
        }));
    }

    for entry in &turn.entries {
        if let Entry::ToolResult { id, content } = entry {
            messages.push(serde_json::json!({
                "role": "tool",
                "tool_call_id": id,
                "content": tool_result_text(content),
            }));
        }
    }

    if (has_block && !absorbed) || messages.is_empty() {
        messages.push(serde_json::json!({ "role": role, "content": turn_text(turn) }));
    }

    messages
}

/// Build the Chat Completions request body from role-tagged turns. Pure.
///
/// `tools` is omitted from the body entirely when empty — an empty `tools: []`
/// array is not the same fact to a model as no tools being registered.
pub(crate) fn request_body(model: &str, turns: &[Turn], tools: &[ToolSchema]) -> serde_json::Value {
    let messages: Vec<serde_json::Value> = turns.iter().flat_map(turn_messages).collect();
    let mut body = serde_json::json!({
        "model": model,
        "stream": true,
        // Chat Completions reports no usage at all in stream mode unless this
        // is set; without it AgentEvent::Usage would never fire for OpenAI.
        "stream_options": { "include_usage": true },
        "messages": messages,
    });
    if !tools.is_empty() {
        body["tools"] = tools_json(tools);
    }
    body
}

/// Parse one SSE line. `[DONE]` is the stream terminator (not JSON) → None. Pure.
pub(crate) fn parse_line(data: &str) -> Vec<SseItem> {
    if data == "[DONE]" {
        return Vec::new();
    }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(data) else {
        return Vec::new();
    };
    if let Some(msg) = v
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
    {
        return vec![SseItem::Error(format!("openai: {msg}"))];
    }
    let mut out = Vec::new();
    // `stream_options.include_usage` makes OpenAI send `"usage": null` on every
    // non-final chunk, so `get("usage")` is Some(Null) there — filter it out or
    // an empty Usage event fires per frame. Anthropic guards by frame type and
    // Gemini by key presence; only this parser sees a present-but-null key.
    if let Some(u) = v.get("usage").filter(|u| !u.is_null()) {
        out.push(SseItem::Usage(manch_protocol::Usage {
            input_tokens: token_count(u, "prompt_tokens"),
            output_tokens: token_count(u, "completion_tokens"),
        }));
    }
    let delta = v
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|c| c.first())
        .and_then(|c| c.get("delta"));
    let content = delta
        .and_then(|d| d.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or_default();
    if !content.is_empty() {
        out.push(SseItem::Text(content.to_string()));
    }
    // `finish_reason: "tool_calls"` is deliberately ignored: it arrives on a
    // frame that carries no index, and OpenAI never marks an individual call
    // finished on its own — ToolAccum::flush closes whatever is still open
    // when the stream ends (Task 10).
    if let Some(tool_calls) = delta
        .and_then(|d| d.get("tool_calls"))
        .and_then(|t| t.as_array())
    {
        for tc in tool_calls {
            let Some(index) = tc.get("index").and_then(|i| i.as_u64()) else {
                continue;
            };
            let index = index as u32;
            // `id` and `function.name` arrive only on the first delta for a
            // given index; later deltas for that index carry only
            // `function.arguments` fragments.
            if let (Some(id), Some(name)) = (
                tc.get("id").and_then(|i| i.as_str()),
                tc.get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|n| n.as_str()),
            ) {
                out.push(SseItem::ToolCallStart {
                    index,
                    id: id.to_string(),
                    name: name.to_string(),
                });
            }
            if let Some(args) = tc
                .get("function")
                .and_then(|f| f.get("arguments"))
                .and_then(|a| a.as_str())
                && !args.is_empty()
            {
                out.push(SseItem::ToolCallArgs {
                    index,
                    json: args.to_string(),
                });
            }
        }
    }
    out
}

pub(crate) fn parse_models(body: &serde_json::Value) -> Vec<ModelInfo> {
    body.get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    let id = m.get("id")?.as_str()?;
                    is_chat_model(id).then(|| ModelInfo {
                        id: id.to_string(),
                        display_name: None,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `/v1/models` returns every model with no capability field — embeddings, TTS,
/// image, transcription, moderation — any of which errors if picked for chat.
/// OpenAI has no machine-readable "is chat" flag, so this is a curated id
/// heuristic: keep the `gpt-*`/`chatgpt-*`/`o1|o3|o4` reasoning + chat families,
/// drop everything whose id names a non-chat modality. Revisit when the model
/// lineup shifts.
fn is_chat_model(id: &str) -> bool {
    const NON_CHAT: [&str; 9] = [
        "embedding",
        "tts",
        "whisper",
        "audio",
        "transcribe",
        "dall-e",
        "image",
        "moderation",
        "realtime",
    ];
    if NON_CHAT.iter().any(|marker| id.contains(marker)) {
        return false;
    }
    id.starts_with("gpt-")
        || id.starts_with("chatgpt")
        || id.starts_with("o1")
        || id.starts_with("o3")
        || id.starts_with("o4")
}

pub async fn list_models(api_key: &str) -> Result<Vec<ModelInfo>> {
    list_models_at(api_key, None).await
}

/// As [`list_models`], against an explicit base (falling back to the env
/// override, then the vendor default).
pub async fn list_models_at(api_key: &str, base: Option<&str>) -> Result<Vec<ModelInfo>> {
    ensure_crypto_provider();
    let base = crate::resolve_base("openai", base, DEFAULT_BASE);
    let resp = reqwest::Client::new()
        .get(models_url(&base))
        .bearer_auth(api_key)
        .send()
        .await;
    crate::list_models_with(resp, FALLBACK_MODEL, parse_models).await
}

#[async_trait]
impl Agent for OpenAiAgent {
    fn id(&self) -> &str {
        "openai"
    }

    async fn prompt(
        &self,
        ctx: Context,
        tools: &[ToolSchema],
        sink: Arc<dyn EventSink>,
    ) -> Result<StopReason> {
        ensure_crypto_provider();
        let resp = reqwest::Client::new()
            .post(completions_url(&self.base))
            .bearer_auth(&self.api_key)
            .json(&request_body(&self.model, &ctx.turns, tools))
            .send()
            .await
            .map_err(err)?;

        if !resp.status().is_success() {
            return Err(crate::http_error("openai", resp).await);
        }
        crate::stream_sse(resp, &sink, parse_line).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use manch_protocol::acp::{ContentBlock, TextContent};
    use manch_protocol::{Entry, Role, ToolInvocation, Turn};

    fn u(text: &str) -> Turn {
        Turn {
            role: Role::User,
            entries: vec![Entry::Block(ContentBlock::Text(TextContent::new(
                text.to_string(),
            )))],
        }
    }
    fn a(text: &str) -> Turn {
        Turn {
            role: Role::Assistant,
            entries: vec![Entry::Block(ContentBlock::Text(TextContent::new(
                text.to_string(),
            )))],
        }
    }

    #[test]
    fn request_body_maps_single_user_turn() {
        let body = request_body("gpt-5", &[u("hi")], &[]);
        assert_eq!(body["model"], "gpt-5");
        assert_eq!(body["stream"], true);
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "hi");
    }

    #[test]
    fn request_body_preserves_assistant_role() {
        let body = request_body("m", &[u("q1"), a("a1"), u("q2")], &[]);
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][1]["role"], "assistant");
        assert_eq!(body["messages"][1]["content"], "a1");
        assert_eq!(body["messages"][2]["role"], "user");
    }

    #[test]
    fn parse_line_extracts_delta_content() {
        let d = r#"{"choices":[{"delta":{"content":"Hi"}}]}"#;
        assert!(matches!(parse_line(d).as_slice(), [crate::SseItem::Text(t)] if t == "Hi"));
    }

    #[test]
    fn parse_line_reports_usage_from_the_final_chunk() {
        let d = r#"{"choices":[],"usage":{"prompt_tokens":9,"completion_tokens":4}}"#;
        assert!(matches!(
            parse_line(d).as_slice(),
            [crate::SseItem::Usage(u)] if u.input_tokens == Some(9) && u.output_tokens == Some(4)
        ));
    }

    #[test]
    fn parse_line_ignores_a_null_usage_field() {
        // With stream_options.include_usage, OpenAI sends "usage": null on
        // every non-final chunk. `v.get("usage")` is Some(Null) there, so an
        // unguarded read fires an empty Usage event per frame.
        let d = r#"{"choices":[{"delta":{"content":"Hi"}}],"usage":null}"#;
        assert!(
            !parse_line(d)
                .iter()
                .any(|i| matches!(i, crate::SseItem::Usage(_))),
            "a null usage field is not a usage report"
        );
    }

    #[test]
    fn request_body_opts_into_streamed_usage() {
        // Chat Completions reports no usage in stream mode unless asked.
        let body = request_body("gpt-5-chat-latest", &[u("hi")], &[]);
        assert_eq!(body["stream_options"]["include_usage"], true);
    }

    #[test]
    fn tools_json_uses_the_openai_function_envelope() {
        use manch_protocol::acp::ToolKind;

        let s = ToolSchema {
            name: "search".into(),
            description: "find".into(),
            kind: ToolKind::Other,
            input_schema: serde_json::json!({ "type": "object" }),
        };
        let v = tools_json(&[s]);
        assert_eq!(v[0]["type"], "function");
        assert_eq!(v[0]["function"]["name"], "search");
        assert_eq!(v[0]["function"]["parameters"]["type"], "object");
    }

    #[test]
    fn parse_line_starts_a_tool_call_from_a_delta_with_id_and_name() {
        let d = r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c1","function":{"name":"search","arguments":""}}]}}]}"#;
        assert!(matches!(parse_line(d).as_slice(),
            [crate::SseItem::ToolCallStart { index: 0, id, name }] if id == "c1" && name == "search"));
    }

    #[test]
    fn parse_line_reads_argument_fragments_from_later_deltas() {
        let d = r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"q\":"}}]}}]}"#;
        assert!(matches!(parse_line(d).as_slice(),
            [crate::SseItem::ToolCallArgs { index: 0, json }] if json == "{\"q\":"));
    }

    #[test]
    fn parse_line_emits_no_end_marker_for_openai() {
        // OpenAI never marks an individual call finished, and a pure parse_line has
        // no memory of which indexes are open. ToolAccum::flush closes them when
        // the stream ends (Task 10).
        let d = r#"{"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#;
        assert!(parse_line(d).is_empty());
    }

    #[test]
    fn request_body_encodes_tool_calls_and_a_tool_role_result() {
        let turns = vec![
            Turn {
                role: Role::Assistant,
                entries: vec![Entry::ToolCall(ToolInvocation {
                    id: "c1".into(),
                    name: "search".into(),
                    arguments: serde_json::json!({"q":"asha"}),
                })],
            },
            Turn {
                role: Role::User,
                entries: vec![Entry::ToolResult {
                    id: "c1".into(),
                    content: vec![crate::text_content("2 matches")],
                }],
            },
        ];
        let body = request_body("m", &turns, &[]);
        assert_eq!(body["messages"][0]["tool_calls"][0]["id"], "c1");
        assert_eq!(
            body["messages"][0]["tool_calls"][0]["function"]["name"],
            "search"
        );
        assert_eq!(body["messages"][1]["role"], "tool");
        assert_eq!(body["messages"][1]["tool_call_id"], "c1");
    }

    #[test]
    fn request_body_keeps_a_narrated_tool_call_in_one_assistant_message() {
        // A model that says "let me look that up" before calling persists as
        // [Block, ToolCall] in one turn. OpenAI requires the `tool` replies to
        // immediately follow the assistant message carrying `tool_calls`, so the
        // narration has to ride along on that same message rather than become a
        // second assistant message wedged in between.
        let turns = vec![
            Turn {
                role: Role::Assistant,
                entries: vec![
                    Entry::Block(ContentBlock::Text(TextContent::new(
                        "Let me look that up.".to_string(),
                    ))),
                    Entry::ToolCall(ToolInvocation {
                        id: "c1".into(),
                        name: "search".into(),
                        arguments: serde_json::json!({"q":"asha"}),
                    }),
                ],
            },
            Turn {
                role: Role::User,
                entries: vec![Entry::ToolResult {
                    id: "c1".into(),
                    content: vec![crate::text_content("2 matches")],
                }],
            },
        ];
        let body = request_body("m", &turns, &[]);
        let messages = body["messages"].as_array().expect("messages array");
        assert_eq!(
            messages.len(),
            2,
            "narration must not become a third message: {messages:#?}"
        );
        assert_eq!(messages[0]["role"], "assistant");
        assert_eq!(messages[0]["content"], "Let me look that up.");
        assert_eq!(messages[0]["tool_calls"][0]["id"], "c1");
        assert_eq!(
            messages[1]["role"], "tool",
            "the tool reply must be the very next message"
        );
        assert_eq!(messages[1]["tool_call_id"], "c1");
    }

    #[test]
    fn request_body_omits_the_tools_key_when_no_tools_are_registered() {
        let body = request_body("m", &[u("hi")], &[]);
        assert!(
            body.get("tools").is_none(),
            "an empty tools array changes model behaviour"
        );
    }

    #[test]
    fn request_body_includes_tools_when_provided() {
        use manch_protocol::acp::ToolKind;

        let s = ToolSchema {
            name: "search".into(),
            description: "find".into(),
            kind: ToolKind::Other,
            input_schema: serde_json::json!({ "type": "object" }),
        };
        let body = request_body("m", &[u("hi")], &[s]);
        assert_eq!(body["tools"][0]["function"]["name"], "search");
    }

    #[test]
    fn new_defaults_to_the_vendor_base() {
        assert_eq!(OpenAiAgent::new("k".into(), None).base, DEFAULT_BASE);
    }

    #[test]
    fn base_is_readable_so_a_caller_can_thread_it_to_list_models_at() {
        // `list_models` is a free function: an agent pointed at a proxy cannot
        // redirect the catalog by itself. The caller must read the base back
        // off the agent and pass it to `list_models_at`, which needs an
        // accessor to be possible at all.
        let agent = OpenAiAgent::new("k".into(), None).base_url("https://proxy.internal/v9");
        assert_eq!(agent.base(), "https://proxy.internal/v9");
    }

    #[test]
    fn base_url_overrides_the_default() {
        let a =
            OpenAiAgent::new("k".into(), None).base_url("https://api.fireworks.ai/inference/v1");
        assert_eq!(a.base, "https://api.fireworks.ai/inference/v1");
    }

    #[test]
    fn urls_derive_from_the_base() {
        let b = "https://api.fireworks.ai/inference/v1";
        assert_eq!(
            completions_url(b),
            "https://api.fireworks.ai/inference/v1/chat/completions"
        );
        assert_eq!(
            models_url(b),
            "https://api.fireworks.ai/inference/v1/models"
        );
    }

    #[test]
    fn parse_line_ignores_done_sentinel() {
        assert!(parse_line("[DONE]").is_empty());
    }

    #[test]
    fn parse_models_reads_data_ids() {
        let body = serde_json::json!({ "data": [{ "id": "gpt-5" }, { "id": "o4-mini" }] });
        let models = parse_models(&body);
        assert_eq!(models[0].id, "gpt-5");
        assert_eq!(models[1].id, "o4-mini");
    }

    #[test]
    fn parse_models_curates_out_non_chat_models() {
        let body = serde_json::json!({ "data": [
            { "id": "gpt-5" },
            { "id": "text-embedding-3-large" },
            { "id": "dall-e-3" },
            { "id": "whisper-1" },
            { "id": "gpt-4o-transcribe" },
            { "id": "o3-mini" },
            { "id": "omni-moderation-latest" },
        ] });
        let ids: Vec<_> = parse_models(&body).into_iter().map(|m| m.id).collect();
        assert_eq!(ids, vec!["gpt-5", "o3-mini"]);
    }

    #[test]
    fn new_uses_fallback_when_model_none() {
        let a = OpenAiAgent::new("k".into(), None);
        assert_eq!(a.model, FALLBACK_MODEL);
    }
}
