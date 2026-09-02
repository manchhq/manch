//! BYOK OpenAI Chat Completions client (Codex BYOK path).

use std::sync::Arc;

use async_trait::async_trait;
use manch_protocol::acp::StopReason;
use manch_protocol::{Agent, Context, EventSink, Result, Role, ToolSchema, Turn};

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
}

/// `{base}/chat/completions`. Pure.
pub(crate) fn completions_url(base: &str) -> String {
    format!("{base}/chat/completions")
}

/// `{base}/models`. Pure.
pub(crate) fn models_url(base: &str) -> String {
    format!("{base}/models")
}

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
        "stream": true,
        // Chat Completions reports no usage at all in stream mode unless this
        // is set; without it AgentEvent::Usage would never fire for OpenAI.
        "stream_options": { "include_usage": true },
        "messages": messages,
    })
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
    if let Some(u) = v.get("usage") {
        out.push(SseItem::Usage(manch_protocol::Usage {
            input_tokens: token_count(u, "prompt_tokens"),
            output_tokens: token_count(u, "completion_tokens"),
        }));
    }
    let content = v
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|c| c.first())
        .and_then(|c| c.get("delta"))
        .and_then(|d| d.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or_default();
    if !content.is_empty() {
        out.push(SseItem::Text(content.to_string()));
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
        _tools: &[ToolSchema],
        sink: Arc<dyn EventSink>,
    ) -> Result<StopReason> {
        ensure_crypto_provider();
        let resp = reqwest::Client::new()
            .post(completions_url(&self.base))
            .bearer_auth(&self.api_key)
            .json(&request_body(&self.model, &ctx.turns))
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
        let body = request_body("gpt-5", &[u("hi")]);
        assert_eq!(body["model"], "gpt-5");
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
    fn request_body_opts_into_streamed_usage() {
        // Chat Completions reports no usage in stream mode unless asked.
        let body = request_body("gpt-5-chat-latest", &[u("hi")]);
        assert_eq!(body["stream_options"]["include_usage"], true);
    }

    #[test]
    fn new_defaults_to_the_vendor_base() {
        assert_eq!(OpenAiAgent::new("k".into(), None).base, DEFAULT_BASE);
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
