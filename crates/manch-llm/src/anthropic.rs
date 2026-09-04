//! BYOK Anthropic Messages API client.

use std::sync::Arc;

use async_trait::async_trait;
use manch_protocol::acp::{ContentBlock, StopReason, ToolCallContent};
use manch_protocol::{
    Agent, Context, Entry, EventSink, Result, Role, ToolInvocation, ToolSchema, Turn,
};

use crate::{ModelInfo, SseItem, ensure_crypto_provider, err, token_count, turn_text};

pub(crate) const DEFAULT_BASE: &str = "https://api.anthropic.com/v1";
const VERSION: &str = "2023-06-01";
const MAX_TOKENS: u32 = 1024;
pub(crate) const FALLBACK_MODEL: &str = "claude-opus-4-8"; // authoritative — do not change

/// BYOK Anthropic via a hand-rolled Messages-API call.
pub struct AnthropicAgent {
    api_key: String,
    model: String,
    base: String,
}

impl AnthropicAgent {
    pub fn new(api_key: String, model: Option<String>) -> Self {
        Self {
            api_key,
            model: model.unwrap_or_else(|| FALLBACK_MODEL.to_string()),
            base: crate::resolve_base("anthropic", None, DEFAULT_BASE),
        }
    }

    /// Point this agent at an alternative endpoint (a managed-tier proxy, or an
    /// Anthropic-compatible gateway). Wins over `MANCH_ANTHROPIC_BASE_URL`.
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

/// `{base}/messages` — the streaming Messages endpoint. Pure.
pub(crate) fn messages_url(base: &str) -> String {
    format!("{base}/messages")
}

/// `{base}/models` — the catalog endpoint, which must follow the same base so an
/// overridden endpoint does not list the vendor's catalog. Pure.
pub(crate) fn models_url(base: &str) -> String {
    format!("{base}/models")
}

/// Build the `tools` array Anthropic expects: `[{ name, description,
/// input_schema }]`. `input_schema` is already the JSON Schema a `Tool`
/// declares — only the envelope is Anthropic-specific. Pure.
pub(crate) fn tools_json(tools: &[ToolSchema]) -> serde_json::Value {
    serde_json::Value::Array(
        tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.input_schema,
                })
            })
            .collect(),
    )
}

/// Map one tool result's content blocks onto Anthropic's `tool_result`
/// content array. Only the text case is meaningful on today's `Tool` surface;
/// anything else (a future `Diff`/`Terminal` content kind) still serialises
/// rather than panicking, so an unexpected content kind degrades instead of
/// dropping the turn.
fn tool_result_content_json(content: &[ToolCallContent]) -> serde_json::Value {
    serde_json::Value::Array(
        content
            .iter()
            .map(|c| match c {
                ToolCallContent::Content(inner) => match &inner.content {
                    ContentBlock::Text(t) => serde_json::json!({ "type": "text", "text": t.text }),
                    other => serde_json::json!({
                        "type": "text",
                        "text": serde_json::to_string(other).unwrap_or_default(),
                    }),
                },
                other => serde_json::json!({
                    "type": "text",
                    "text": serde_json::to_string(other).unwrap_or_default(),
                }),
            })
            .collect(),
    )
}

/// Map one turn entry onto an Anthropic content block. `Entry::ToolCall`
/// becomes a `tool_use` block and `Entry::ToolResult` a `tool_result` block,
/// the pairing Anthropic requires to accept a second loop iteration built from
/// stored history.
///
/// `ContentBlock::Audio` and the two resource variants are still dropped:
/// Anthropic's Messages API has no input encoding for them, so there is nothing
/// truthful to map them onto. They are the only kinds that vanish here.
fn entry_json(entry: &Entry) -> Option<serde_json::Value> {
    match entry {
        Entry::Block(ContentBlock::Text(t)) => {
            Some(serde_json::json!({ "type": "text", "text": t.text }))
        }
        // ACP hands us base64 + a MIME type, which is exactly the pair
        // Anthropic's `base64` source wants — no transcoding in between.
        Entry::Block(ContentBlock::Image(i)) => Some(serde_json::json!({
            "type": "image",
            "source": {
                "type": "base64",
                "media_type": i.mime_type,
                "data": i.data,
            },
        })),
        Entry::Block(_) => None,
        Entry::ToolCall(ToolInvocation {
            id,
            name,
            arguments,
            ..
        }) => Some(serde_json::json!({
            "type": "tool_use",
            "id": id,
            "name": name,
            "input": arguments,
        })),
        Entry::ToolResult { id, content } => Some(serde_json::json!({
            "type": "tool_result",
            "tool_use_id": id,
            "content": tool_result_content_json(content),
        })),
    }
}

/// A turn's wire content: the plain string Anthropic expects when every entry
/// is ordinary text (unchanged from before this task), or a content-block
/// array once a turn carries a tool call or tool result.
fn turn_content(turn: &Turn) -> serde_json::Value {
    let all_text = turn
        .entries
        .iter()
        .all(|e| matches!(e, Entry::Block(ContentBlock::Text(_))));
    if all_text {
        return serde_json::Value::String(turn_text(turn));
    }
    serde_json::Value::Array(turn.entries.iter().filter_map(entry_json).collect())
}

/// Build the Messages API request body from role-tagged turns. Pure.
///
/// `tools` is omitted from the body entirely when empty — an empty `tools: []`
/// array is not the same fact to a model as no tools being registered.
pub(crate) fn request_body(model: &str, turns: &[Turn], tools: &[ToolSchema]) -> serde_json::Value {
    let messages: Vec<serde_json::Value> = turns
        .iter()
        .map(|t| {
            let role = match t.role {
                Role::User => "user",
                Role::Assistant => "assistant",
            };
            serde_json::json!({ "role": role, "content": turn_content(t) })
        })
        .collect();
    let mut body = serde_json::json!({
        "model": model,
        "max_tokens": MAX_TOKENS,
        "stream": true,
        "messages": messages,
    });
    if !tools.is_empty() {
        body["tools"] = tools_json(tools);
    }
    body
}

/// Parse one SSE `data:` payload into text or a surfaced error. Pure.
pub(crate) fn parse_line(data: &str) -> Vec<SseItem> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(data) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    match v.get("type").and_then(|t| t.as_str()) {
        Some("content_block_start") => {
            if let Some(index) = v.get("index").and_then(|i| i.as_u64())
                && let Some(cb) = v.get("content_block")
                && cb.get("type").and_then(|t| t.as_str()) == Some("tool_use")
                && let Some(id) = cb.get("id").and_then(|i| i.as_str())
                && let Some(name) = cb.get("name").and_then(|n| n.as_str())
            {
                out.push(SseItem::ToolCallStart {
                    index: index as u32,
                    id: id.to_string(),
                    name: name.to_string(),
                    provider_meta: None,
                });
            }
        }
        Some("content_block_delta") => {
            if let Some(delta) = v.get("delta")
                && delta.get("type").and_then(|t| t.as_str()) == Some("text_delta")
                && let Some(text) = delta.get("text").and_then(|t| t.as_str())
            {
                out.push(SseItem::Text(text.to_string()));
            }
            if let Some(index) = v.get("index").and_then(|i| i.as_u64())
                && let Some(delta) = v.get("delta")
                && delta.get("type").and_then(|t| t.as_str()) == Some("input_json_delta")
                && let Some(json) = delta.get("partial_json").and_then(|j| j.as_str())
            {
                out.push(SseItem::ToolCallArgs {
                    index: index as u32,
                    json: json.to_string(),
                });
            }
        }
        Some("content_block_stop") => {
            if let Some(index) = v.get("index").and_then(|i| i.as_u64()) {
                out.push(SseItem::ToolCallEnd {
                    index: index as u32,
                });
            }
        }
        Some("message_start") => {
            if let Some(u) = v.get("message").and_then(|m| m.get("usage")) {
                out.push(SseItem::Usage(manch_protocol::Usage {
                    input_tokens: token_count(u, "input_tokens"),
                    output_tokens: token_count(u, "output_tokens"),
                }));
            }
        }
        Some("message_delta") => {
            // The closing frame carries only the output total; input was reported
            // at message_start and must not be re-asserted as absent.
            if let Some(u) = v.get("usage") {
                out.push(SseItem::Usage(manch_protocol::Usage {
                    input_tokens: token_count(u, "input_tokens"),
                    output_tokens: token_count(u, "output_tokens"),
                }));
            }
        }
        Some("error") => {
            let msg = v
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("stream error");
            out.push(SseItem::Error(format!("anthropic: {msg}")));
        }
        _ => {}
    }
    out
}

/// Parse the list-models response into a catalog. Pure.
pub(crate) fn parse_models(body: &serde_json::Value) -> Vec<ModelInfo> {
    body.get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    Some(ModelInfo {
                        id: m.get("id")?.as_str()?.to_string(),
                        display_name: m
                            .get("display_name")
                            .and_then(|n| n.as_str())
                            .map(String::from),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Fetch the available models for this key (falls back to the default id on failure).
pub async fn list_models(api_key: &str) -> Result<Vec<ModelInfo>> {
    list_models_at(api_key, None).await
}

/// As [`list_models`], against an explicit base (falling back to the env
/// override, then the vendor default).
pub async fn list_models_at(api_key: &str, base: Option<&str>) -> Result<Vec<ModelInfo>> {
    ensure_crypto_provider();
    let base = crate::resolve_base("anthropic", base, DEFAULT_BASE);
    let resp = reqwest::Client::new()
        .get(models_url(&base))
        .header("x-api-key", api_key)
        .header("anthropic-version", VERSION)
        .send()
        .await;
    crate::list_models_with(resp, FALLBACK_MODEL, parse_models).await
}

#[async_trait]
impl Agent for AnthropicAgent {
    fn id(&self) -> &str {
        "anthropic"
    }

    async fn prompt(
        &self,
        ctx: Context,
        tools: &[ToolSchema],
        sink: Arc<dyn EventSink>,
    ) -> Result<StopReason> {
        ensure_crypto_provider();
        let resp = reqwest::Client::new()
            .post(messages_url(&self.base))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", VERSION)
            .json(&request_body(&self.model, &ctx.turns, tools))
            .send()
            .await
            .map_err(err)?;

        if !resp.status().is_success() {
            return Err(crate::http_error("anthropic", resp).await);
        }
        crate::stream_sse(resp, &sink, parse_line).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use manch_protocol::acp::{ContentBlock, ImageContent, TextContent};
    use manch_protocol::{Entry, Role, Turn};

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
        let body = request_body("claude-opus-4-8", &[u("hi")], &[]);
        assert_eq!(body["model"], "claude-opus-4-8");
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
    fn tools_json_uses_the_anthropic_envelope() {
        use manch_protocol::acp::ToolKind;

        let s = ToolSchema {
            name: "search".into(),
            description: "find".into(),
            kind: ToolKind::Other,
            input_schema: serde_json::json!({ "type": "object" }),
        };
        let v = tools_json(&[s]);
        assert_eq!(v[0]["name"], "search");
        assert_eq!(v[0]["description"], "find");
        assert_eq!(v[0]["input_schema"]["type"], "object");
    }

    #[test]
    fn request_body_encodes_a_tool_use_and_its_result() {
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
        let body = request_body("m", &turns, &[]);
        assert_eq!(body["messages"][0]["content"][0]["type"], "tool_use");
        assert_eq!(body["messages"][0]["content"][0]["id"], "c1");
        assert_eq!(body["messages"][0]["content"][0]["name"], "search");
        assert_eq!(body["messages"][1]["content"][0]["type"], "tool_result");
        assert_eq!(body["messages"][1]["content"][0]["tool_use_id"], "c1");
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
        assert_eq!(body["tools"][0]["name"], "search");
    }

    #[test]
    fn parse_line_extracts_text_delta() {
        let d =
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hi"}}"#;
        assert!(matches!(parse_line(d).as_slice(), [crate::SseItem::Text(t)] if t == "Hi"));
    }

    #[test]
    fn parse_line_surfaces_stream_error() {
        let d = r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#;
        assert!(
            matches!(parse_line(d).as_slice(), [crate::SseItem::Error(e)] if e == "anthropic: Overloaded")
        );
    }

    #[test]
    fn parse_line_reports_input_tokens_from_message_start() {
        let d =
            r#"{"type":"message_start","message":{"usage":{"input_tokens":12,"output_tokens":1}}}"#;
        assert!(matches!(
            parse_line(d).as_slice(),
            [crate::SseItem::Usage(u)] if u.input_tokens == Some(12)
        ));
    }

    #[test]
    fn parse_line_reports_output_tokens_from_message_delta() {
        let d = r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":25}}"#;
        assert!(matches!(
            parse_line(d).as_slice(),
            [crate::SseItem::Usage(u)] if u.output_tokens == Some(25) && u.input_tokens.is_none()
        ));
    }

    #[test]
    fn parse_line_starts_a_tool_call_on_content_block_start() {
        let d = r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"c1","name":"search"}}"#;
        assert!(matches!(parse_line(d).as_slice(),
            [crate::SseItem::ToolCallStart { index: 0, id, name, .. }] if id == "c1" && name == "search"));
    }

    #[test]
    fn parse_line_accumulates_input_json_delta_fragments() {
        let d = r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"a\":"}}"#;
        assert!(matches!(parse_line(d).as_slice(),
            [crate::SseItem::ToolCallArgs { index: 0, json }] if json == "{\"a\":"));
    }

    #[test]
    fn parse_line_ends_a_tool_call_on_content_block_stop() {
        let d = r#"{"type":"content_block_stop","index":0}"#;
        assert!(matches!(
            parse_line(d).as_slice(),
            [crate::SseItem::ToolCallEnd { index: 0 }]
        ));
    }

    #[test]
    fn parse_models_reads_id_and_display_name() {
        let body = serde_json::json!({
            "data": [{ "id": "claude-opus-4-8", "display_name": "Claude Opus 4.8" }]
        });
        let models = parse_models(&body);
        assert_eq!(models[0].id, "claude-opus-4-8");
        assert_eq!(models[0].display_name.as_deref(), Some("Claude Opus 4.8"));
    }

    #[test]
    fn new_defaults_to_the_vendor_base() {
        assert_eq!(AnthropicAgent::new("k".into(), None).base, DEFAULT_BASE);
    }

    #[test]
    fn base_is_readable_so_a_caller_can_thread_it_to_list_models_at() {
        // `list_models` is a free function: an agent pointed at a proxy cannot
        // redirect the catalog by itself. The caller must read the base back
        // off the agent and pass it to `list_models_at`, which needs an
        // accessor to be possible at all.
        let agent = AnthropicAgent::new("k".into(), None).base_url("https://proxy.internal/v9");
        assert_eq!(agent.base(), "https://proxy.internal/v9");
    }

    #[test]
    fn base_url_overrides_the_default() {
        let a = AnthropicAgent::new("k".into(), None).base_url("https://proxy.internal/v1");
        assert_eq!(a.base, "https://proxy.internal/v1");
    }

    #[test]
    fn urls_derive_from_the_base() {
        assert_eq!(messages_url("https://p/v1"), "https://p/v1/messages");
        assert_eq!(models_url("https://p/v1"), "https://p/v1/models");
    }

    #[test]
    fn new_uses_fallback_when_model_none() {
        let a = AnthropicAgent::new("k".into(), None);
        assert_eq!(a.model, FALLBACK_MODEL);
    }

    fn image(mime: &str, data: &str) -> Entry {
        Entry::Block(ContentBlock::Image(ImageContent::new(
            data.to_string(),
            mime.to_string(),
        )))
    }

    #[test]
    fn an_image_block_reaches_anthropic_as_a_base64_image_source() {
        let turn = Turn {
            role: Role::User,
            entries: vec![
                Entry::Block(ContentBlock::Text(TextContent::new(
                    "read this".to_string(),
                ))),
                image("image/png", "AAAA"),
            ],
        };
        let body = request_body("claude-opus-4-8", &[turn], &[]);
        let content = &body["messages"][0]["content"];
        // The prose alongside the image must survive as well.
        assert_eq!(content[0]["text"], "read this");
        assert_eq!(
            content[1],
            serde_json::json!({
                "type": "image",
                "source": { "type": "base64", "media_type": "image/png", "data": "AAAA" },
            })
        );
    }

    /// Regression guard, not a RED test: an all-text turn must keep serialising
    /// as a bare string, byte for byte as before multimodal support landed.
    #[test]
    fn a_text_only_turn_still_serialises_as_a_bare_string() {
        let body = request_body("claude-opus-4-8", &[u("hi")], &[]);
        assert_eq!(body["messages"][0]["content"], serde_json::json!("hi"));
    }
}
