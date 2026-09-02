//! BYOK Anthropic Messages API client.

use std::sync::Arc;

use async_trait::async_trait;
use manch_protocol::acp::StopReason;
use manch_protocol::{Agent, Context, EventSink, Result, Role, ToolSchema, Turn};

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

/// Build the Messages API request body from role-tagged turns. Pure.
pub(crate) fn request_body(model: &str, turns: &[Turn]) -> serde_json::Value {
    let messages: Vec<serde_json::Value> = turns
        .iter()
        .map(|t| {
            let role = match t.role {
                Role::User => "user",
                Role::Assistant => "assistant",
            };
            serde_json::json!({ "role": role, "content": turn_text(t) })
        })
        .collect();
    serde_json::json!({
        "model": model,
        "max_tokens": MAX_TOKENS,
        "stream": true,
        "messages": messages,
    })
}

/// Parse one SSE `data:` payload into text or a surfaced error. Pure.
pub(crate) fn parse_line(data: &str) -> Vec<SseItem> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(data) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    match v.get("type").and_then(|t| t.as_str()) {
        Some("content_block_delta") => {
            if let Some(delta) = v.get("delta")
                && delta.get("type").and_then(|t| t.as_str()) == Some("text_delta")
                && let Some(text) = delta.get("text").and_then(|t| t.as_str())
            {
                out.push(SseItem::Text(text.to_string()));
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
        _tools: &[ToolSchema],
        sink: Arc<dyn EventSink>,
    ) -> Result<StopReason> {
        ensure_crypto_provider();
        let resp = reqwest::Client::new()
            .post(messages_url(&self.base))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", VERSION)
            .json(&request_body(&self.model, &ctx.turns))
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
    use manch_protocol::acp::{ContentBlock, TextContent};
    use manch_protocol::{Role, Turn};

    fn u(text: &str) -> Turn {
        Turn {
            role: Role::User,
            blocks: vec![ContentBlock::Text(TextContent::new(text.to_string()))],
        }
    }
    fn a(text: &str) -> Turn {
        Turn {
            role: Role::Assistant,
            blocks: vec![ContentBlock::Text(TextContent::new(text.to_string()))],
        }
    }

    #[test]
    fn request_body_maps_single_user_turn() {
        let body = request_body("claude-opus-4-8", &[u("hi")]);
        assert_eq!(body["model"], "claude-opus-4-8");
        assert_eq!(body["stream"], true);
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "hi");
    }

    #[test]
    fn request_body_preserves_assistant_role() {
        let body = request_body("m", &[u("q1"), a("a1"), u("q2")]);
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][1]["role"], "assistant");
        assert_eq!(body["messages"][1]["content"], "a1");
        assert_eq!(body["messages"][2]["role"], "user");
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
}
