//! BYOK OpenAI Chat Completions client (Codex BYOK path).

use std::sync::Arc;

use async_trait::async_trait;
use futures_util::StreamExt;
use manch_protocol::acp::StopReason;
use manch_protocol::{Agent, AgentEvent, Context, EventSink, Result, ToolSchema};

use crate::{ModelInfo, SseItem, drain_sse, ensure_crypto_provider, err, prompt_text};

const URL: &str = "https://api.openai.com/v1/chat/completions";
const MODELS_URL: &str = "https://api.openai.com/v1/models";
// Stable chat alias — resolves to the current GPT-5 chat snapshot and works with
// Chat Completions, so it won't rot like a pinned id. Only hit if list-models fails.
pub(crate) const FALLBACK_MODEL: &str = "gpt-5-chat-latest";

pub struct OpenAiAgent {
    api_key: String,
    model: String,
}

impl OpenAiAgent {
    pub fn new(api_key: String, model: Option<String>) -> Self {
        Self {
            api_key,
            model: model.unwrap_or_else(|| FALLBACK_MODEL.to_string()),
        }
    }
}

pub(crate) fn request_body(model: &str, prompt: &str) -> serde_json::Value {
    serde_json::json!({
        "model": model,
        "stream": true,
        "messages": [{ "role": "user", "content": prompt }],
    })
}

/// Parse one SSE line. `[DONE]` is the stream terminator (not JSON) → None. Pure.
pub(crate) fn parse_line(data: &str) -> Option<SseItem> {
    if data == "[DONE]" {
        return None;
    }
    let v: serde_json::Value = serde_json::from_str(data).ok()?;
    if let Some(msg) = v
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
    {
        return Some(SseItem::Error(format!("openai: {msg}")));
    }
    let content = v
        .get("choices")?
        .as_array()?
        .first()?
        .get("delta")?
        .get("content")?
        .as_str()?;
    (!content.is_empty()).then_some(SseItem::Text(content.to_string()))
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

fn fallback_model() -> ModelInfo {
    ModelInfo {
        id: FALLBACK_MODEL.to_string(),
        display_name: None,
    }
}

pub async fn list_models(api_key: &str) -> Result<Vec<ModelInfo>> {
    ensure_crypto_provider();
    let resp = reqwest::Client::new()
        .get(MODELS_URL)
        .bearer_auth(api_key)
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
impl Agent for OpenAiAgent {
    fn id(&self) -> &str {
        "openai"
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
            .bearer_auth(&self.api_key)
            .json(&request_body(&self.model, &prompt))
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
                .map(|m| format!("openai: {m}"))
                .unwrap_or_else(|| format!("openai: HTTP {status}"));
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
    fn request_body_streams_user_message() {
        let body = request_body("gpt-5", "hi");
        assert_eq!(body["model"], "gpt-5");
        assert_eq!(body["stream"], true);
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "hi");
    }

    #[test]
    fn parse_line_extracts_delta_content() {
        let d = r#"{"choices":[{"delta":{"content":"Hi"}}]}"#;
        assert!(matches!(parse_line(d), Some(crate::SseItem::Text(t)) if t == "Hi"));
    }

    #[test]
    fn parse_line_ignores_done_sentinel() {
        assert!(parse_line("[DONE]").is_none());
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
