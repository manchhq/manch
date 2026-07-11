//! BYOK provider clients for Manch — direct provider HTTP/SSE, no execution surface.
//! Each provider implements `manch_protocol::Agent` and emits ACP event vocabulary.

use std::sync::Arc;

use futures_util::StreamExt;
use manch_protocol::acp::{ContentBlock, StopReason};
use manch_protocol::{AgentEvent, EventSink, Result, Turn};

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

/// One parsed SSE line: streamed text, or a surfaced error message.
pub(crate) enum SseItem {
    Text(String),
    Error(String),
}

/// Drain complete `\n`-terminated lines from `buf`, applying `parse` to each
/// line's trimmed `data:` payload. Any trailing partial line is retained in
/// `buf`. Splitting on the ASCII `\n` byte keeps multibyte UTF-8 sequences
/// (Devanagari, CJK, emoji) whole across network chunk boundaries.
pub(crate) fn drain_sse(
    buf: &mut Vec<u8>,
    parse: impl Fn(&str) -> Option<SseItem>,
) -> Vec<SseItem> {
    let mut out = Vec::new();
    while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
        let line_bytes: Vec<u8> = buf.drain(..=pos).collect();
        let line = String::from_utf8_lossy(&line_bytes);
        let line = line.trim();
        if let Some(data) = line.strip_prefix("data:")
            && let Some(item) = parse(data.trim())
        {
            out.push(item);
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

/// Shared SSE streaming loop: decode byte chunks (splitting on `\n` so multibyte
/// UTF-8 stays whole), drain complete lines through `parse`, emit text live,
/// surface a parsed stream error, and emit `Done` when the stream ends.
pub(crate) async fn stream_sse(
    resp: reqwest::Response,
    sink: &Arc<dyn EventSink>,
    parse: impl Fn(&str) -> Option<SseItem>,
) -> Result<StopReason> {
    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = stream.next().await {
        buf.extend_from_slice(&chunk.map_err(err)?);
        for item in drain_sse(&mut buf, &parse) {
            match item {
                SseItem::Text(t) => sink.emit(AgentEvent::text_chunk(t)).await?,
                SseItem::Error(e) => return Err(err(e)),
            }
        }
    }
    sink.emit(AgentEvent::Done(StopReason::EndTurn)).await?;
    Ok(StopReason::EndTurn)
}

/// Fetch selectable models for a BYOK provider id. Unknown / disabled providers
/// yield `NotFound`. Each provider degrades to its fallback model on fetch failure.
pub async fn list_models(provider: &str, api_key: &str) -> manch_protocol::Result<Vec<ModelInfo>> {
    match provider {
        #[cfg(feature = "anthropic")]
        "anthropic" => anthropic::list_models(api_key).await,
        #[cfg(feature = "gemini")]
        "gemini" => gemini::list_models(api_key).await,
        #[cfg(feature = "openai")]
        "openai" => openai::list_models(api_key).await,
        _ => Err(manch_protocol::Error::NotFound(provider.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drain_sse_extracts_data_lines_and_leaves_partial() {
        let mut buf = b"data: {\"t\":1}\ndata: partial".to_vec();
        let items = drain_sse(&mut buf, |d| Some(SseItem::Text(d.to_string())));
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
}
