//! BYOK Gemini `generateContent` client (SSE streaming via `?alt=sse`).

use async_trait::async_trait;
use futures_util::StreamExt;
use manch_protocol::acp::StopReason;
use manch_protocol::{Agent, AgentEvent, Context, EventSink, Result, ToolSchema};

use crate::{ModelInfo, SseItem, drain_sse, ensure_crypto_provider, err, prompt_text};

const BASE: &str = "https://generativelanguage.googleapis.com/v1beta";
pub(crate) const FALLBACK_MODEL: &str = "gemini-3-flash";

pub struct GeminiAgent {
    api_key: String,
    model: String,
}

impl GeminiAgent {
    pub fn new(api_key: String, model: Option<String>) -> Self {
        Self {
            api_key,
            model: model.unwrap_or_else(|| FALLBACK_MODEL.to_string()),
        }
    }
}

/// Pure request body: a single user turn.
pub(crate) fn request_body(prompt: &str) -> serde_json::Value {
    serde_json::json!({ "contents": [{ "role": "user", "parts": [{ "text": prompt }] }] })
}

/// Parse one SSE line: concatenate the candidate's text parts, or surface an error. Pure.
pub(crate) fn parse_line(data: &str) -> Option<SseItem> {
    let v: serde_json::Value = serde_json::from_str(data).ok()?;
    if let Some(msg) = v
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
    {
        return Some(SseItem::Error(format!("gemini: {msg}")));
    }
    let parts = v
        .get("candidates")?
        .as_array()?
        .first()?
        .get("content")?
        .get("parts")?
        .as_array()?;
    let text: String = parts
        .iter()
        .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
        .collect();
    (!text.is_empty()).then_some(SseItem::Text(text))
}

/// Parse list-models response; ids drop the `models/` prefix. Pure.
pub(crate) fn parse_models(body: &serde_json::Value) -> Vec<ModelInfo> {
    body.get("models")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
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

fn fallback_model() -> ModelInfo {
    ModelInfo {
        id: FALLBACK_MODEL.to_string(),
        display_name: None,
    }
}

pub async fn list_models(api_key: &str) -> Result<Vec<ModelInfo>> {
    ensure_crypto_provider();
    let url = format!("{BASE}/models");
    let resp = reqwest::Client::new()
        .get(url)
        .header("x-goog-api-key", api_key)
        .send()
        .await;
    match resp {
        Ok(r) if r.status().is_success() => {
            let body: serde_json::Value = r.json().await.map_err(err)?;
            let models = parse_models(&body);
            Ok(if models.is_empty() {
                vec![fallback_model()]
            } else {
                models
            })
        }
        _ => Ok(vec![fallback_model()]),
    }
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
        sink: &dyn EventSink,
    ) -> Result<StopReason> {
        ensure_crypto_provider();
        let prompt = prompt_text(&ctx);
        let url = format!("{BASE}/models/{}:streamGenerateContent?alt=sse", self.model);
        let resp = reqwest::Client::new()
            .post(url)
            .header("x-goog-api-key", &self.api_key)
            .json(&request_body(&prompt))
            .send()
            .await
            .map_err(err)?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body: serde_json::Value = resp.json().await.map_err(err)?;
            let msg = body
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .map(|m| format!("gemini: {m}"))
                .unwrap_or_else(|| format!("gemini: HTTP {status}"));
            return Err(crate::err(msg));
        }

        let mut stream = resp.bytes_stream();
        let mut buf: Vec<u8> = Vec::new();
        while let Some(chunk) = stream.next().await {
            buf.extend_from_slice(&chunk.map_err(err)?);
            for item in drain_sse(&mut buf, parse_line) {
                match item {
                    SseItem::Text(t) => sink.emit(AgentEvent::text_chunk(t)).await?,
                    SseItem::Error(e) => return Err(crate::err(e)),
                }
            }
        }
        sink.emit(AgentEvent::Done(StopReason::EndTurn)).await?;
        Ok(StopReason::EndTurn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_body_has_user_part() {
        let body = request_body("hi");
        assert_eq!(body["contents"][0]["role"], "user");
        assert_eq!(body["contents"][0]["parts"][0]["text"], "hi");
    }

    #[test]
    fn parse_line_extracts_candidate_text() {
        let d = r#"{"candidates":[{"content":{"role":"model","parts":[{"text":"Hi"}]}}]}"#;
        assert!(matches!(parse_line(d), Some(crate::SseItem::Text(t)) if t == "Hi"));
    }

    #[test]
    fn parse_line_surfaces_error() {
        let d = r#"{"error":{"code":400,"message":"bad key"}}"#;
        assert!(matches!(parse_line(d), Some(crate::SseItem::Error(e)) if e == "gemini: bad key"));
    }

    #[test]
    fn parse_models_strips_models_prefix() {
        let body = serde_json::json!({
            "models": [{ "name": "models/gemini-3-flash", "displayName": "Gemini 3 Flash" }]
        });
        let models = parse_models(&body);
        assert_eq!(models[0].id, "gemini-3-flash");
        assert_eq!(models[0].display_name.as_deref(), Some("Gemini 3 Flash"));
    }
}
