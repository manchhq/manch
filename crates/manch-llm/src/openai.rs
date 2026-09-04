//! BYOK OpenAI Chat Completions client (Codex BYOK path).

use std::sync::Arc;

use async_trait::async_trait;
use manch_protocol::acp::{ContentBlock, StopReason, ToolCallContent};
use manch_protocol::{
    Agent, Context, Entry, EventSink, Result, Role, ToolInvocation, ToolSchema, Turn,
};

use crate::{ModelInfo, ModelKind, SseItem, nested_token_count, token_count, turn_text};

pub(crate) const DEFAULT_BASE: &str = "https://api.openai.com/v1";
// Stable chat alias — resolves to the current GPT-5 chat snapshot and works with
// Chat Completions, so it won't rot like a pinned id. Only hit if list-models fails.
pub(crate) const FALLBACK_MODEL: &str = "gpt-5-chat-latest";

pub struct OpenAiAgent {
    api_key: String,
    model: String,
    base: String,
    /// What this agent calls itself. `"openai"` for the vendor; an
    /// OpenAI-compatible provider built through [`OpenAiAgent::compatible`]
    /// reports its own id, so a host routing on `Agent::id` is not told
    /// Fireworks is OpenAI.
    id: &'static str,
    max_output_tokens: u32,
    http: crate::http::Http,
}

impl OpenAiAgent {
    pub fn new(api_key: String, model: Option<String>) -> Self {
        Self {
            api_key,
            model: model.unwrap_or_else(|| FALLBACK_MODEL.to_string()),
            base: crate::resolve_base("openai", None, DEFAULT_BASE),
            id: "openai",
            max_output_tokens: crate::DEFAULT_MAX_OUTPUT_TOKENS,
            http: crate::http::Http::default(),
        }
    }
    /// Time allowed to establish a connection. See
    /// [`DEFAULT_CONNECT_TIMEOUT`](crate::DEFAULT_CONNECT_TIMEOUT).
    #[must_use]
    pub fn connect_timeout(mut self, d: std::time::Duration) -> Self {
        self.http = self.http.with_connect_timeout(d);
        self
    }

    /// Maximum time between two reads on a live response — a stall detector,
    /// not a deadline for the turn. See
    /// [`DEFAULT_READ_TIMEOUT`](crate::DEFAULT_READ_TIMEOUT).
    #[must_use]
    pub fn read_timeout(mut self, d: std::time::Duration) -> Self {
        self.http = self.http.with_read_timeout(d);
        self
    }

    /// Retries after the first attempt, on 429 and 5xx. `0` disables retrying.
    /// See [`DEFAULT_MAX_RETRIES`](crate::DEFAULT_MAX_RETRIES).
    #[must_use]
    pub fn max_retries(mut self, n: u32) -> Self {
        self.http = self.http.with_max_retries(n);
        self
    }

    /// Cap the model's output, in tokens. Defaults to
    /// [`DEFAULT_MAX_OUTPUT_TOKENS`](crate::DEFAULT_MAX_OUTPUT_TOKENS).
    #[must_use]
    pub fn max_output_tokens(mut self, n: u32) -> Self {
        self.max_output_tokens = n;
        self
    }

    #[cfg(test)]
    pub(crate) fn max_output_tokens_for_test(&self) -> u32 {
        self.max_output_tokens
    }

    /// An **OpenAI-compatible** provider: same wire format, its own id, base
    /// and default model. Fireworks, Together, Groq, OpenRouter and a local
    /// vLLM all speak this dialect, and none of them serve OpenAI's catalogue
    /// or answer to OpenAI's model ids.
    ///
    /// This exists so a *host* need not know any of that. Before it, a
    /// consumer had to supply the base URL, hardcode a default model id in
    /// its own crate (OpenAI's `gpt-5-chat-latest` 404s on Fireworks), and
    /// remember that the catalogue needs redirecting too — three provider
    /// facts leaking into product code. Which model suits which *task* is the
    /// host's call; *which string names a model on Fireworks* is ours.
    ///
    /// `env_key` names the `MANCH_{KEY}_BASE_URL` override, so each compatible
    /// provider is redirectable independently of OpenAI itself.
    pub fn compatible(
        id: &'static str,
        env_key: &str,
        api_key: String,
        model: Option<String>,
        default_base: &str,
        fallback_model: &str,
    ) -> Self {
        Self {
            api_key,
            model: model.unwrap_or_else(|| fallback_model.to_string()),
            base: crate::resolve_base(env_key, None, default_base),
            id,
            max_output_tokens: crate::DEFAULT_MAX_OUTPUT_TOKENS,
            http: crate::http::Http::default(),
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

    /// The resolved model id. Test-only accessor: `model` is private and
    /// `compatible`'s fallback behaviour is worth asserting from a sibling
    /// module.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn model_for_test(&self) -> &str {
        &self.model
    }
}

/// Test-only: lets a sibling module assert its fallback is *not* this one.
#[cfg(test)]
pub(crate) fn fallback_model_for_test() -> &'static str {
    FALLBACK_MODEL
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

/// A turn's `content` value: the bare string OpenAI has always been sent when
/// every content block is text, or an array of typed parts once the turn
/// carries a non-text block.
///
/// The API accepts both shapes, so the string is kept for the common case
/// deliberately — switching every existing request to an array to enable a rare
/// one would be a regression surface for no gain.
///
/// `ContentBlock::Audio` and the two resource variants are still dropped:
/// Chat Completions encodes audio as `input_audio` with a bare format name
/// (`"wav"`, `"mp3"`), not a MIME type, so mapping it is a different problem
/// than this one and inventing the translation here would be a guess.
fn turn_content(turn: &Turn) -> serde_json::Value {
    let has_non_text_block = turn
        .entries
        .iter()
        .any(|e| matches!(e, Entry::Block(b) if !matches!(b, ContentBlock::Text(_))));
    if !has_non_text_block {
        return serde_json::Value::String(turn_text(turn));
    }
    serde_json::Value::Array(
        turn.entries
            .iter()
            .filter_map(|e| match e {
                Entry::Block(ContentBlock::Text(t)) => {
                    Some(serde_json::json!({ "type": "text", "text": t.text }))
                }
                // Chat Completions takes an image as a URL, and a data URL is
                // how base64 bytes become one.
                Entry::Block(ContentBlock::Image(i)) => Some(serde_json::json!({
                    "type": "image_url",
                    "image_url": { "url": format!("data:{};base64,{}", i.mime_type, i.data) },
                })),
                _ => None,
            })
            .collect(),
    )
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
                ..
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
                turn_content(turn)
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
        messages.push(serde_json::json!({ "role": role, "content": turn_content(turn) }));
    }

    messages
}

/// Build the Chat Completions request body from role-tagged turns. Pure.
///
/// `tools` is omitted from the body entirely when empty — an empty `tools: []`
/// array is not the same fact to a model as no tools being registered.
pub(crate) fn request_body(
    model: &str,
    turns: &[Turn],
    tools: &[ToolSchema],
    max_output_tokens: u32,
    id: &str,
) -> serde_json::Value {
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
    // Which parameter names the output cap depends on who is answering.
    //
    // OpenAI itself deprecated `max_tokens` and rejects it outright on the
    // reasoning models, so the vendor gets `max_completion_tokens`. The
    // compatible providers a BYOK user actually brings a key for — Together,
    // OpenRouter, Groq, a local vLLM — universally understand `max_tokens` and
    // are far less consistent about the newer name. Sending the right one per
    // dialect costs three lines; sending the wrong one costs an uncapped turn
    // or a 400, and neither is visible from the host.
    if id == "openai" {
        body["max_completion_tokens"] = max_output_tokens.into();
    } else {
        body["max_tokens"] = max_output_tokens.into();
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
            total_tokens: token_count(u, "total_tokens"),
            // The only two counts any provider nests. Reading them off the top
            // level silently yields `None` on every request.
            thought_tokens: nested_token_count(u, "completion_tokens_details", "reasoning_tokens"),
            cached_read_tokens: nested_token_count(u, "prompt_tokens_details", "cached_tokens"),
            // Prompt caching is automatic here; nothing reports a write.
            cached_write_tokens: None,
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
    // `finish_reason` closes the turn. `"tool_calls"` is deliberately *not*
    // treated as a tool-call signal: it arrives on a frame that carries no
    // index, and OpenAI never marks an individual call finished on its own —
    // `ToolAccum::flush` closes whatever is still open when the stream ends
    // (Task 10). It still maps to `EndTurn` like any other ordinary finish.
    if let Some(reason) = v
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|c| c.first())
        .and_then(|c| c.get("finish_reason"))
        .and_then(|r| r.as_str())
    {
        out.push(SseItem::Stop(crate::stop_reason(reason)));
    }
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
                    provider_meta: None,
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
                        // `/v1/models` carries no capability field whatsoever;
                        // `kind` is asserted only because `is_chat_model` has
                        // already curated the list down to chat families.
                        kind: Some(ModelKind::Chat),
                        ..ModelInfo::new(id)
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
    let base = crate::resolve_base("openai", base, DEFAULT_BASE);
    let resp = crate::http::shared()
        .client()
        .get(models_url(&base))
        .bearer_auth(api_key)
        .send()
        .await;
    crate::list_models_with(resp, FALLBACK_MODEL, parse_models).await
}

#[async_trait]
impl Agent for OpenAiAgent {
    fn id(&self) -> &str {
        self.id
    }

    async fn prompt(
        &self,
        ctx: Context,
        tools: &[ToolSchema],
        sink: Arc<dyn EventSink>,
    ) -> Result<StopReason> {
        let resp = self
            .http
            .send(
                self.http
                    .client()
                    .post(completions_url(&self.base))
                    .bearer_auth(&self.api_key)
                    .json(&request_body(
                        &self.model,
                        &ctx.turns,
                        tools,
                        self.max_output_tokens,
                        self.id,
                    )),
            )
            .await?;

        if !resp.status().is_success() {
            return Err(crate::http_error("openai", resp).await);
        }
        crate::stream_sse(resp, &sink, parse_line).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use manch_protocol::acp::{ContentBlock, ImageContent, TextContent};
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
        let body = request_body(
            "gpt-5",
            &[u("hi")],
            &[],
            crate::DEFAULT_MAX_OUTPUT_TOKENS,
            "openai",
        );
        assert_eq!(body["model"], "gpt-5");
        assert_eq!(body["stream"], true);
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "hi");
    }

    #[test]
    fn request_body_preserves_assistant_role() {
        let body = request_body(
            "m",
            &[u("q1"), a("a1"), u("q2")],
            &[],
            crate::DEFAULT_MAX_OUTPUT_TOKENS,
            "openai",
        );
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
        let body = request_body(
            "gpt-5-chat-latest",
            &[u("hi")],
            &[],
            crate::DEFAULT_MAX_OUTPUT_TOKENS,
            "openai",
        );
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
            [crate::SseItem::ToolCallStart { index: 0, id, name, .. }] if id == "c1" && name == "search"));
    }

    #[test]
    fn parse_line_reads_argument_fragments_from_later_deltas() {
        let d = r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"q\":"}}]}}]}"#;
        assert!(matches!(parse_line(d).as_slice(),
            [crate::SseItem::ToolCallArgs { index: 0, json }] if json == "{\"q\":"));
    }

    #[test]
    fn a_finish_reason_frame_closes_the_turn_but_never_a_single_tool_call() {
        // Replaces `parse_line_emits_no_end_marker_for_openai`, which asserted
        // this frame yielded *nothing*. It now yields the turn's stop reason —
        // that is the whole point of #64, since discarding it is what made a
        // truncated turn look like a finished one.
        //
        // The original claim it was really protecting still holds and is what
        // is asserted here: OpenAI never marks an individual call finished, and
        // a pure `parse_line` has no memory of which indexes are open, so no
        // `ToolCallEnd` may be inferred. `ToolAccum::flush` closes them when the
        // stream ends.
        let d = r#"{"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#;
        let items = parse_line(d);
        assert!(
            !items
                .iter()
                .any(|i| matches!(i, crate::SseItem::ToolCallEnd { .. })),
            "a finish_reason frame must never close an individual tool call"
        );
        assert!(items.iter().any(|i| matches!(
            i,
            crate::SseItem::Stop(manch_protocol::acp::StopReason::EndTurn)
        )));
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
                    provider_meta: None,
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
        let body = request_body("m", &turns, &[], crate::DEFAULT_MAX_OUTPUT_TOKENS, "openai");
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
                        provider_meta: None,
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
        let body = request_body("m", &turns, &[], crate::DEFAULT_MAX_OUTPUT_TOKENS, "openai");
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
        let body = request_body(
            "m",
            &[u("hi")],
            &[],
            crate::DEFAULT_MAX_OUTPUT_TOKENS,
            "openai",
        );
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
        let body = request_body(
            "m",
            &[u("hi")],
            &[s],
            crate::DEFAULT_MAX_OUTPUT_TOKENS,
            "openai",
        );
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

    #[test]
    fn an_image_block_reaches_openai_as_a_data_url_image_part() {
        let turn = Turn {
            role: Role::User,
            entries: vec![
                Entry::Block(ContentBlock::Text(TextContent::new(
                    "read this".to_string(),
                ))),
                Entry::Block(ContentBlock::Image(ImageContent::new(
                    "AAAA".to_string(),
                    "image/png".to_string(),
                ))),
            ],
        };
        let body = request_body(
            "gpt-5-chat-latest",
            &[turn],
            &[],
            crate::DEFAULT_MAX_OUTPUT_TOKENS,
            "openai",
        );
        let content = &body["messages"][0]["content"];
        assert_eq!(
            content[0],
            serde_json::json!({ "type": "text", "text": "read this" })
        );
        assert_eq!(
            content[1],
            serde_json::json!({
                "type": "image_url",
                "image_url": { "url": "data:image/png;base64,AAAA" },
            })
        );
    }

    /// Regression guard, not a RED test: OpenAI accepts both a bare string and a
    /// parts array, and every existing all-text call must keep the string.
    #[test]
    fn a_text_only_turn_still_serialises_content_as_a_bare_string() {
        let body = request_body(
            "gpt-5-chat-latest",
            &[u("hi")],
            &[],
            crate::DEFAULT_MAX_OUTPUT_TOKENS,
            "openai",
        );
        assert_eq!(body["messages"][0]["content"], serde_json::json!("hi"));
    }

    #[test]
    fn openai_capabilities_read_as_unknown_not_false() {
        let body = serde_json::json!({ "data": [{ "id": "gpt-5" }] });
        let m = &parse_models(&body)[0];
        assert_eq!(m.supports_tools, None);
        assert_eq!(m.context_window, None);
        assert_eq!(m.kind, Some(ModelKind::Chat));
    }

    #[test]
    fn parse_line_reports_the_openai_cache_and_reasoning_breakdown() {
        // Captured from a live OpenAI-dialect stream (Fireworks) on 2026-09-04.
        // Both breakdowns are nested one level down, unlike every other count.
        let d = r#"{"choices":[],"usage":{"prompt_tokens":90,"completion_tokens":23,"total_tokens":113,"prompt_tokens_details":{"cached_tokens":64},"completion_tokens_details":{"reasoning_tokens":7}}}"#;
        let u = match parse_line(d).into_iter().find_map(|i| match i {
            crate::SseItem::Usage(u) => Some(u),
            _ => None,
        }) {
            Some(u) => u,
            None => panic!("no usage item"),
        };
        assert_eq!(u.total_tokens, Some(113));
        assert_eq!(u.cached_read_tokens, Some(64));
        assert_eq!(u.thought_tokens, Some(7));
        // OpenAI caches automatically; there is no write count to report.
        assert_eq!(u.cached_write_tokens, None);
    }

    #[test]
    fn openai_proper_gets_max_completion_tokens() {
        // `max_tokens` is rejected outright by OpenAI's reasoning models, so
        // the vendor gets the parameter that still works on all of them.
        let a = OpenAiAgent::new("k".into(), None);
        let body = request_body(
            "gpt-5-chat-latest",
            &[u("hi")],
            &[],
            a.max_output_tokens_for_test(),
            a.id(),
        );
        assert_eq!(
            body["max_completion_tokens"],
            crate::DEFAULT_MAX_OUTPUT_TOKENS
        );
        assert!(body.get("max_tokens").is_none());
    }

    #[test]
    fn a_compatible_provider_gets_max_tokens() {
        // Together, OpenRouter and friends universally understand `max_tokens`;
        // `max_completion_tokens` is far less evenly supported. Breadth wins on
        // the path a BYOK user actually brings a key for.
        let a = crate::fireworks::agent("k".into(), None);
        let body = request_body("m", &[u("hi")], &[], a.max_output_tokens_for_test(), a.id());
        assert_eq!(body["max_tokens"], crate::DEFAULT_MAX_OUTPUT_TOKENS);
        assert!(body.get("max_completion_tokens").is_none());
    }

    #[test]
    fn a_truncated_turn_reports_max_tokens_not_end_turn() {
        let d = r#"{"choices":[{"delta":{},"finish_reason":"length"}]}"#;
        assert!(parse_line(d).iter().any(|i| matches!(
            i,
            crate::SseItem::Stop(manch_protocol::acp::StopReason::MaxTokens)
        )));
    }
}
