//! BYOK Gemini `generateContent` client (SSE streaming via `?alt=sse`).

use std::sync::Arc;

use async_trait::async_trait;
use manch_protocol::acp::StopReason;
use manch_protocol::{Agent, Context, EventSink, Result, Role, ToolSchema, Turn};

use crate::{ModelInfo, SseItem, ensure_crypto_provider, err, token_count, turn_text};

pub(crate) const DEFAULT_BASE: &str = "https://generativelanguage.googleapis.com/v1beta";
pub(crate) const FALLBACK_MODEL: &str = "gemini-3-flash";

pub struct GeminiAgent {
    api_key: String,
    model: String,
    base: String,
}

impl GeminiAgent {
    pub fn new(api_key: String, model: Option<String>) -> Self {
        Self {
            api_key,
            model: model.unwrap_or_else(|| FALLBACK_MODEL.to_string()),
            base: crate::resolve_base("gemini", None, DEFAULT_BASE),
        }
    }

    /// Point this agent at an alternative endpoint. Wins over `MANCH_GEMINI_BASE_URL`.
    #[must_use]
    pub fn base_url(mut self, base: impl Into<String>) -> Self {
        let base = base.into();
        self.base = crate::pick_base(Some(&base), None, &self.base);
        self
    }
}

/// `{base}/models/{model}:streamGenerateContent?alt=sse`. Pure.
pub(crate) fn stream_url(base: &str, model: &str) -> String {
    format!("{base}/models/{model}:streamGenerateContent?alt=sse")
}

/// `{base}/models`. Pure.
pub(crate) fn models_url(base: &str) -> String {
    format!("{base}/models")
}

/// Pure request body: role-tagged turns as Gemini `contents`.
///
/// Only `Entry::Block` is mapped (via `turn_text`); `Entry::ToolCall` and
/// `Entry::ToolResult` are not yet encoded onto Gemini's `functionCall` /
/// `functionResponse` wire shape — that lands in Task 12.
pub(crate) fn request_body(turns: &[Turn]) -> serde_json::Value {
    let contents: Vec<serde_json::Value> = turns
        .iter()
        .map(|t| {
            let role = match t.role {
                Role::User => "user",
                Role::Assistant => "model",
            };
            serde_json::json!({ "role": role, "parts": [{ "text": turn_text(t) }] })
        })
        .collect();
    serde_json::json!({ "contents": contents })
}

/// Parse one SSE line: concatenate the candidate's text parts, or surface an error. Pure.
pub(crate) fn parse_line(data: &str) -> Vec<SseItem> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(data) else {
        return Vec::new();
    };
    if let Some(msg) = v
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
    {
        return vec![SseItem::Error(format!("gemini: {msg}"))];
    }
    let mut out = Vec::new();
    let text: String = v
        .get("candidates")
        .and_then(|c| c.as_array())
        .and_then(|c| c.first())
        .and_then(|c| c.get("content"))
        .and_then(|c| c.get("parts"))
        .and_then(|p| p.as_array())
        .map(|parts| {
            parts
                .iter()
                .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                .collect()
        })
        .unwrap_or_default();
    if !text.is_empty() {
        out.push(SseItem::Text(text));
    }
    if let Some(u) = v.get("usageMetadata") {
        out.push(SseItem::Usage(manch_protocol::Usage {
            input_tokens: token_count(u, "promptTokenCount"),
            output_tokens: token_count(u, "candidatesTokenCount"),
        }));
    }
    out
}

/// Parse list-models response; ids drop the `models/` prefix. Only models that
/// advertise `streamGenerateContent` are kept — the raw list also contains
/// embedding/TTS/embedContent-only models that error at prompt time. Pure.
pub(crate) fn parse_models(body: &serde_json::Value) -> Vec<ModelInfo> {
    body.get("models")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|m| supports_streaming(m))
                .filter_map(|m| {
                    let name = m.get("name")?.as_str()?;
                    Some(ModelInfo {
                        id: name.strip_prefix("models/").unwrap_or(name).to_string(),
                        display_name: m
                            .get("displayName")
                            .and_then(|n| n.as_str())
                            .map(String::from),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// True if a list-models entry advertises `streamGenerateContent` — i.e. it's a
/// chat model this provider can actually prompt (excludes embedding/TTS models).
fn supports_streaming(model: &serde_json::Value) -> bool {
    model
        .get("supportedGenerationMethods")
        .and_then(|m| m.as_array())
        .is_some_and(|methods| {
            methods
                .iter()
                .any(|m| m.as_str() == Some("streamGenerateContent"))
        })
}

pub async fn list_models(api_key: &str) -> Result<Vec<ModelInfo>> {
    list_models_at(api_key, None).await
}

/// As [`list_models`], against an explicit base (falling back to the env
/// override, then the vendor default).
pub async fn list_models_at(api_key: &str, base: Option<&str>) -> Result<Vec<ModelInfo>> {
    ensure_crypto_provider();
    let base = crate::resolve_base("gemini", base, DEFAULT_BASE);
    let resp = reqwest::Client::new()
        .get(models_url(&base))
        .header("x-goog-api-key", api_key)
        .send()
        .await;
    crate::list_models_with(resp, FALLBACK_MODEL, parse_models).await
}

#[async_trait]
impl Agent for GeminiAgent {
    fn id(&self) -> &str {
        "gemini"
    }

    async fn prompt(
        &self,
        ctx: Context,
        _tools: &[ToolSchema],
        sink: Arc<dyn EventSink>,
    ) -> Result<StopReason> {
        ensure_crypto_provider();
        let resp = reqwest::Client::new()
            .post(stream_url(&self.base, &self.model))
            .header("x-goog-api-key", &self.api_key)
            .json(&request_body(&ctx.turns))
            .send()
            .await
            .map_err(err)?;

        if !resp.status().is_success() {
            return Err(crate::http_error("gemini", resp).await);
        }
        crate::stream_sse(resp, &sink, parse_line).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use manch_protocol::acp::{ContentBlock, TextContent};
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
        let body = request_body(&[u("hi")]);
        assert_eq!(body["contents"][0]["role"], "user");
        assert_eq!(body["contents"][0]["parts"][0]["text"], "hi");
    }

    #[test]
    fn request_body_maps_assistant_to_model_role() {
        let body = request_body(&[u("q1"), a("a1")]);
        assert_eq!(body["contents"][1]["role"], "model");
        assert_eq!(body["contents"][1]["parts"][0]["text"], "a1");
    }

    #[test]
    fn parse_line_extracts_candidate_text() {
        let d = r#"{"candidates":[{"content":{"role":"model","parts":[{"text":"Hi"}]}}]}"#;
        assert!(matches!(parse_line(d).as_slice(), [crate::SseItem::Text(t)] if t == "Hi"));
    }

    #[test]
    fn parse_line_reports_usage_alongside_text_in_one_chunk() {
        // Gemini repeats usageMetadata on chunks that also carry text, so one
        // frame must be able to yield two items.
        let d = r#"{"candidates":[{"content":{"parts":[{"text":"Hi"}]}}],"usageMetadata":{"promptTokenCount":5,"candidatesTokenCount":7}}"#;
        let items = parse_line(d);
        assert!(matches!(items.as_slice(),
            [crate::SseItem::Text(t), crate::SseItem::Usage(u)]
                if t == "Hi" && u.input_tokens == Some(5) && u.output_tokens == Some(7)));
    }

    #[test]
    fn new_defaults_to_the_vendor_base() {
        assert_eq!(GeminiAgent::new("k".into(), None).base, DEFAULT_BASE);
    }

    #[test]
    fn base_url_overrides_the_default() {
        let g = GeminiAgent::new("k".into(), None).base_url("https://proxy.internal/v1beta");
        assert_eq!(g.base, "https://proxy.internal/v1beta");
    }

    #[test]
    fn urls_derive_from_the_base() {
        assert_eq!(
            stream_url("https://p/v1beta", "gemini-3-flash"),
            "https://p/v1beta/models/gemini-3-flash:streamGenerateContent?alt=sse"
        );
        assert_eq!(models_url("https://p/v1beta"), "https://p/v1beta/models");
    }

    #[test]
    fn parse_line_surfaces_error() {
        let d = r#"{"error":{"code":400,"message":"bad key"}}"#;
        assert!(
            matches!(parse_line(d).as_slice(), [crate::SseItem::Error(e)] if e == "gemini: bad key")
        );
    }

    #[test]
    fn parse_models_strips_models_prefix() {
        let body = serde_json::json!({
            "models": [{
                "name": "models/gemini-3-flash",
                "displayName": "Gemini 3 Flash",
                "supportedGenerationMethods": ["generateContent", "streamGenerateContent"]
            }]
        });
        let models = parse_models(&body);
        assert_eq!(models[0].id, "gemini-3-flash");
        assert_eq!(models[0].display_name.as_deref(), Some("Gemini 3 Flash"));
    }

    #[test]
    fn parse_models_drops_non_streaming_models() {
        let body = serde_json::json!({
            "models": [
                {
                    "name": "models/gemini-3-flash",
                    "supportedGenerationMethods": ["streamGenerateContent"]
                },
                {
                    "name": "models/text-embedding-004",
                    "supportedGenerationMethods": ["embedContent"]
                },
                { "name": "models/legacy-no-methods" }
            ]
        });
        let ids: Vec<_> = parse_models(&body).into_iter().map(|m| m.id).collect();
        assert_eq!(ids, vec!["gemini-3-flash"]);
    }

    #[test]
    fn new_uses_fallback_when_model_none() {
        let g = GeminiAgent::new("k".into(), None);
        assert_eq!(g.model, FALLBACK_MODEL);
    }
}
