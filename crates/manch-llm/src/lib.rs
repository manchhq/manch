//! BYOK provider clients for Manch — direct provider HTTP/SSE, no execution surface.
//! Each provider implements `manch_protocol::Agent` and emits ACP event vocabulary.

use std::sync::Arc;

use futures_util::StreamExt;
use manch_protocol::acp::{ContentBlock, StopReason};
use manch_protocol::{AgentEvent, EventSink, Result, Turn, Usage};

#[cfg(feature = "anthropic")]
pub mod anthropic;
#[cfg(feature = "anthropic")]
pub use anthropic::AnthropicAgent;

#[cfg(feature = "gemini")]
pub mod gemini;
#[cfg(feature = "gemini")]
pub use gemini::GeminiAgent;

#[cfg(feature = "openai")]
pub mod openai;
#[cfg(feature = "openai")]
pub use openai::OpenAiAgent;

/// A model advertised by a provider's list-models endpoint.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub display_name: Option<String>,
}

/// One fact parsed out of an SSE line: streamed text, provider token counts, or
/// a surfaced error message.
pub(crate) enum SseItem {
    Text(String),
    Usage(Usage),
    Error(String),
}

/// Drain complete `\n`-terminated lines from `buf`, applying `parse` to each
/// line's trimmed `data:` payload. Any trailing partial line is retained in
/// `buf`. Splitting on the ASCII `\n` byte keeps multibyte UTF-8 sequences
/// (Devanagari, CJK, emoji) whole across network chunk boundaries.
pub(crate) fn drain_sse(buf: &mut Vec<u8>, parse: impl Fn(&str) -> Vec<SseItem>) -> Vec<SseItem> {
    let mut out = Vec::new();
    while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
        let line_bytes: Vec<u8> = buf.drain(..=pos).collect();
        let line = String::from_utf8_lossy(&line_bytes);
        let line = line.trim();
        if let Some(data) = line.strip_prefix("data:") {
            out.extend(parse(data.trim()));
        }
    }
    out
}

/// Concatenate a turn's text blocks into one string. Non-text blocks are
/// ignored — multimodal message mapping is future work.
pub(crate) fn turn_text(turn: &Turn) -> String {
    turn.blocks
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Install the `ring` rustls crypto provider once (reqwest `rustls-no-provider`
/// ships no backend; first `Client` build would panic otherwise).
pub(crate) fn ensure_crypto_provider() {
    use std::sync::OnceLock;
    static INSTALLED: OnceLock<()> = OnceLock::new();
    INSTALLED.get_or_init(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// Pure base-URL precedence: an explicit override wins, then an environment
/// override, then the provider default. Blank overrides are ignored (a var that
/// is set-but-empty must not blank the endpoint), and one trailing slash is
/// trimmed so `…/v1` and `…/v1/` behave identically.
pub(crate) fn pick_base(explicit: Option<&str>, env: Option<&str>, default: &str) -> String {
    let chosen = [explicit, env]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|s| !s.is_empty())
        .unwrap_or(default);
    chosen.trim_end_matches('/').to_string()
}

/// Resolve a provider's base URL, reading `MANCH_{PROVIDER}_BASE_URL` as the
/// environment override. An explicit value still wins, so one process can point
/// different agents at different proxies — which an env var alone cannot express.
pub(crate) fn resolve_base(provider: &str, explicit: Option<&str>, default: &str) -> String {
    let key = format!("MANCH_{}_BASE_URL", provider.to_uppercase());
    let env = std::env::var(&key).ok();
    pick_base(explicit, env.as_deref(), default)
}

/// Map any error into `manch_protocol::Error::Other`.
pub(crate) fn err(e: impl ToString) -> manch_protocol::Error {
    manch_protocol::Error::Other(e.to_string())
}

/// Build a `ModelInfo` for a provider's fallback id.
pub(crate) fn fallback_model(id: &str) -> ModelInfo {
    ModelInfo {
        id: id.to_string(),
        display_name: None,
    }
}

/// Shared list-models flow: on a 2xx body, parse with the provider's `parse`
/// (empty → the fallback); on any failure, degrade to the single fallback model.
pub(crate) async fn list_models_with(
    resp: reqwest::Result<reqwest::Response>,
    fallback_id: &str,
    parse: impl Fn(&serde_json::Value) -> Vec<ModelInfo>,
) -> Result<Vec<ModelInfo>> {
    match resp {
        Ok(r) if r.status().is_success() => {
            let body: serde_json::Value = r.json().await.map_err(err)?;
            let models = parse(&body);
            Ok(if models.is_empty() {
                vec![fallback_model(fallback_id)]
            } else {
                models
            })
        }
        _ => Ok(vec![fallback_model(fallback_id)]),
    }
}

/// Extract a human error message from a response body. Surfaces `error.message`
/// when the body is JSON; otherwise `"{provider}: HTTP {status}"`. Pure — split
/// from [`http_error`] so the fallback behaviour is unit-testable. A proxy's
/// HTML 502/504 body simply fails to parse and takes the fallback branch.
pub(crate) fn error_message(provider: &str, status: reqwest::StatusCode, body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("error")?.get("message")?.as_str().map(String::from))
        .map(|m| format!("{provider}: {m}"))
        .unwrap_or_else(|| format!("{provider}: HTTP {status}"))
}

/// Turn a non-2xx response into an error. Reads the body as **text first** — a
/// proxy's HTML 502/504 isn't JSON, and calling `.json()` on it would mask the
/// real status behind a generic decode error. Consumes `resp`.
pub(crate) async fn http_error(provider: &str, resp: reqwest::Response) -> manch_protocol::Error {
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    err(error_message(provider, status, &text))
}

/// Read one token count out of a provider's usage object. Absent or non-numeric
/// fields yield `None` rather than zero — "not reported" and "zero tokens" are
/// different facts, and a consumer summing these must be able to tell them apart.
pub(crate) fn token_count(usage: &serde_json::Value, key: &str) -> Option<u32> {
    usage.get(key)?.as_u64().and_then(|n| u32::try_from(n).ok())
}

/// Map one parsed SSE item to the event it emits. An `Error` item ends the turn
/// rather than producing an event. Pure — split from [`stream_sse`] so the
/// mapping is unit-testable without a live HTTP response.
pub(crate) fn item_to_event(item: SseItem) -> Result<AgentEvent> {
    match item {
        SseItem::Text(t) => Ok(AgentEvent::text_chunk(t)),
        SseItem::Usage(u) => Ok(AgentEvent::Usage(u)),
        SseItem::Error(e) => Err(err(e)),
    }
}

/// Shared SSE streaming loop: decode byte chunks (splitting on `\n` so multibyte
/// UTF-8 stays whole), drain complete lines through `parse`, emit text live,
/// surface a parsed stream error, and emit `Done` when the stream ends.
pub(crate) async fn stream_sse(
    resp: reqwest::Response,
    sink: &Arc<dyn EventSink>,
    parse: impl Fn(&str) -> Vec<SseItem>,
) -> Result<StopReason> {
    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = stream.next().await {
        buf.extend_from_slice(&chunk.map_err(err)?);
        for item in drain_sse(&mut buf, &parse) {
            sink.emit(item_to_event(item)?).await?;
        }
    }
    sink.emit(AgentEvent::Done(StopReason::EndTurn)).await?;
    Ok(StopReason::EndTurn)
}

/// Fetch selectable models for a BYOK provider id. Unknown / disabled providers
/// yield `NotFound`. Each provider degrades to its fallback model on fetch failure.
pub async fn list_models(provider: &str, api_key: &str) -> manch_protocol::Result<Vec<ModelInfo>> {
    list_models_at(provider, api_key, None).await
}

/// As [`list_models`], against an explicit base URL. `None` falls back to
/// `MANCH_{PROVIDER}_BASE_URL` and then the vendor default, so a caller that
/// does not care passes `None` and a managed tier passes its proxy per call.
pub async fn list_models_at(
    provider: &str,
    api_key: &str,
    base: Option<&str>,
) -> manch_protocol::Result<Vec<ModelInfo>> {
    match provider {
        #[cfg(feature = "anthropic")]
        "anthropic" => anthropic::list_models_at(api_key, base).await,
        #[cfg(feature = "gemini")]
        "gemini" => gemini::list_models_at(api_key, base).await,
        #[cfg(feature = "openai")]
        "openai" => openai::list_models_at(api_key, base).await,
        _ => Err(manch_protocol::Error::NotFound(provider.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drain_sse_extracts_data_lines_and_leaves_partial() {
        let mut buf = b"data: {\"t\":1}\ndata: partial".to_vec();
        let items = drain_sse(&mut buf, |d| vec![SseItem::Text(d.to_string())]);
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], SseItem::Text(s) if s == "{\"t\":1}"));
        assert_eq!(String::from_utf8_lossy(&buf), "data: partial"); // partial retained
    }

    #[tokio::test]
    async fn list_models_rejects_unknown_provider() {
        let e = super::list_models("nope", "k").await.unwrap_err();
        assert!(matches!(e, manch_protocol::Error::NotFound(_)));
    }

    #[test]
    fn error_message_surfaces_json_error() {
        let msg = error_message(
            "openai",
            reqwest::StatusCode::BAD_REQUEST,
            r#"{"error":{"message":"bad key"}}"#,
        );
        assert_eq!(msg, "openai: bad key");
    }

    #[test]
    fn error_message_falls_back_on_non_json_body() {
        // A proxy's HTML 502 must not be masked by a JSON decode error.
        let msg = error_message(
            "gemini",
            reqwest::StatusCode::BAD_GATEWAY,
            "<html>502 Bad Gateway</html>",
        );
        assert_eq!(msg, "gemini: HTTP 502 Bad Gateway");
    }

    #[test]
    fn fallback_model_has_no_display_name() {
        let m = fallback_model("gpt-5-chat-latest");
        assert_eq!(m.id, "gpt-5-chat-latest");
        assert_eq!(m.display_name, None);
    }

    #[test]
    fn turn_text_joins_multiple_text_blocks_with_newline() {
        use manch_protocol::acp::{ContentBlock, TextContent};
        use manch_protocol::{Role, Turn};

        let turn = Turn {
            role: Role::User,
            blocks: vec![
                ContentBlock::Text(TextContent::new("hello".to_string())),
                ContentBlock::Text(TextContent::new("world".to_string())),
            ],
        };
        assert_eq!(turn_text(&turn), "hello\nworld");
    }

    #[test]
    fn item_to_event_maps_text_to_a_message_chunk() {
        let ev = item_to_event(SseItem::Text("Hi".into())).unwrap();
        assert!(matches!(ev, AgentEvent::Update(_)));
    }

    #[test]
    fn item_to_event_maps_usage_through_verbatim() {
        let u = Usage {
            input_tokens: Some(10),
            output_tokens: Some(3),
        };
        let ev = item_to_event(SseItem::Usage(u)).unwrap();
        assert!(matches!(ev, AgentEvent::Usage(got) if got == u));
    }

    #[test]
    fn item_to_event_turns_an_error_item_into_a_turn_error() {
        let e = item_to_event(SseItem::Error("openai: boom".into())).unwrap_err();
        assert!(e.to_string().contains("boom"));
    }

    #[test]
    fn pick_base_prefers_explicit_over_env() {
        let b = pick_base(
            Some("https://proxy/v1"),
            Some("https://env/v1"),
            "https://d/v1",
        );
        assert_eq!(b, "https://proxy/v1");
    }

    #[test]
    fn pick_base_uses_env_when_no_explicit() {
        let b = pick_base(None, Some("https://env/v1"), "https://d/v1");
        assert_eq!(b, "https://env/v1");
    }

    #[test]
    fn pick_base_falls_back_to_default() {
        assert_eq!(pick_base(None, None, "https://d/v1"), "https://d/v1");
    }

    #[test]
    fn pick_base_ignores_blank_override() {
        // A set-but-empty MANCH_*_BASE_URL must not blank the endpoint.
        assert_eq!(
            pick_base(Some(""), Some("  "), "https://d/v1"),
            "https://d/v1"
        );
    }

    #[test]
    fn pick_base_trims_trailing_slash() {
        assert_eq!(
            pick_base(Some("https://proxy/v1/"), None, "https://d/v1"),
            "https://proxy/v1"
        );
    }
}
