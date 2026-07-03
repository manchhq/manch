//! BYOK Anthropic Messages API client.

use std::sync::Arc;

use async_trait::async_trait;
use manch_protocol::acp::StopReason;
use manch_protocol::{Agent, Context, EventSink, Result, ToolSchema};

use crate::{ModelInfo, SseItem, ensure_crypto_provider, err, prompt_text};

const URL: &str = "https://api.anthropic.com/v1/messages";
const MODELS_URL: &str = "https://api.anthropic.com/v1/models";
const VERSION: &str = "2023-06-01";
const MAX_TOKENS: u32 = 1024;
pub(crate) const FALLBACK_MODEL: &str = "claude-opus-4-8"; // authoritative — do not change

/// BYOK Anthropic via a hand-rolled Messages-API call.
pub struct AnthropicAgent {
    api_key: String,
    model: String,
}

impl AnthropicAgent {
    pub fn new(api_key: String, model: Option<String>) -> Self {
        Self {
            api_key,
            model: model.unwrap_or_else(|| FALLBACK_MODEL.to_string()),
        }
    }
}

/// Build the Messages API request body. Pure.
pub(crate) fn request_body(model: &str, prompt: &str) -> serde_json::Value {
    serde_json::json!({
        "model": model,
        "max_tokens": MAX_TOKENS,
        "stream": true,
        "messages": [{ "role": "user", "content": prompt }],
    })
}

/// Parse one SSE `data:` payload into text or a surfaced error. Pure.
pub(crate) fn parse_line(data: &str) -> Option<SseItem> {
    let v: serde_json::Value = serde_json::from_str(data).ok()?;
    match v.get("type")?.as_str()? {
        "content_block_delta" => {
            let delta = v.get("delta")?;
            if delta.get("type")?.as_str()? == "text_delta" {
                return Some(SseItem::Text(delta.get("text")?.as_str()?.to_string()));
            }
            None
        }
        "error" => {
            let msg = v
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("stream error");
            Some(SseItem::Error(format!("anthropic: {msg}")))
        }
        _ => None,
    }
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
    ensure_crypto_provider();
    let resp = reqwest::Client::new()
        .get(MODELS_URL)
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
        let prompt = prompt_text(&ctx);
        let resp = reqwest::Client::new()
            .post(URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", VERSION)
            .json(&request_body(&self.model, &prompt))
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

    #[test]
    fn request_body_has_model_and_user_message() {
        let body = request_body("claude-opus-4-8", "hi");
        assert_eq!(body["model"], "claude-opus-4-8");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "hi");
        assert_eq!(body["stream"], true);
    }

    #[test]
    fn parse_line_extracts_text_delta() {
        let d =
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hi"}}"#;
        assert!(matches!(parse_line(d), Some(crate::SseItem::Text(t)) if t == "Hi"));
    }

    #[test]
    fn parse_line_surfaces_stream_error() {
        let d = r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#;
        assert!(
            matches!(parse_line(d), Some(crate::SseItem::Error(e)) if e == "anthropic: Overloaded")
        );
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
    fn new_uses_fallback_when_model_none() {
        let a = AnthropicAgent::new("k".into(), None);
        assert_eq!(a.model, FALLBACK_MODEL);
    }
}
