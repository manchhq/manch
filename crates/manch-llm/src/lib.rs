//! BYOK provider clients for Manch — direct provider HTTP/SSE, no execution surface.
//! Each provider implements `manch_protocol::Agent` and emits ACP event vocabulary.

use std::collections::HashMap;
use std::sync::Arc;

use futures_util::StreamExt;
use manch_protocol::acp::{ContentBlock, StopReason};
use manch_protocol::{AgentEvent, Entry, EventSink, Result, ToolInvocation, Turn, Usage};

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

/// One fact parsed out of an SSE line: streamed text, provider token counts, a
/// surfaced error message, or one fragment of a streamed tool call.
///
/// A tool call arrives across several SSE frames (start, N argument-JSON
/// fragments, end), but `parse_line` is a pure function of one line and must
/// stay that way — so parsers emit these fragments and [`ToolAccum`], owned by
/// [`stream_sse`] for the life of the stream, assembles them.
pub(crate) enum SseItem {
    Text(String),
    Usage(Usage),
    Error(String),
    /// A tool call has begun at `index` (the provider's stream slot).
    ToolCallStart {
        index: u32,
        id: String,
        name: String,
    },
    /// One fragment of the call's argument JSON at `index`, to be concatenated
    /// in arrival order.
    ToolCallArgs {
        index: u32,
        json: String,
    },
    /// The call at `index` is complete; its accumulated JSON is ready to parse.
    ToolCallEnd {
        index: u32,
    },
}

/// Assembles streamed tool-call fragments into complete [`AgentEvent::ToolCall`]s.
///
/// Owned by [`stream_sse`] for the life of one stream — never by a parser,
/// which must stay a pure function of one line. Keyed by the provider's
/// stream `index` because a model may open several tool calls before closing
/// any of them (interleaved calls).
#[derive(Default)]
pub(crate) struct ToolAccum {
    /// index -> (id, name, accumulated argument JSON).
    open: HashMap<u32, (String, String, String)>,
}

impl ToolAccum {
    /// Feed one fragment. `ToolCallStart`/`ToolCallArgs` return `None` — a call
    /// is only ever reported once assembled. `ToolCallEnd` removes the open
    /// entry and returns the completed `AgentEvent::ToolCall`, parsing the
    /// accumulated JSON (an empty accumulation means "no arguments", not
    /// malformed, so it becomes `{}`). A malformed accumulation cannot panic —
    /// `AgentEvent` has no error variant to carry a parse failure, so it
    /// degrades to `Value::Null` rather than aborting the stream over one
    /// unparsable call.
    pub(crate) fn apply(&mut self, item: SseItem) -> Option<AgentEvent> {
        match item {
            SseItem::ToolCallStart { index, id, name } => {
                self.open.insert(index, (id, name, String::new()));
                None
            }
            SseItem::ToolCallArgs { index, json } => {
                if let Some(entry) = self.open.get_mut(&index) {
                    entry.2.push_str(&json);
                }
                None
            }
            SseItem::ToolCallEnd { index } => {
                let (id, name, json) = self.open.remove(&index)?;
                Some(AgentEvent::ToolCall(ToolInvocation {
                    id,
                    name,
                    arguments: parse_tool_arguments(&json),
                }))
            }
            SseItem::Text(_) | SseItem::Usage(_) | SseItem::Error(_) => None,
        }
    }

    /// Complete every call still open when the stream ends (OpenAI never
    /// marks an individual call finished; the stream ending is the only
    /// signal). Drains the map, so calling `flush` twice cannot replay a call.
    pub(crate) fn flush(&mut self) -> Vec<AgentEvent> {
        self.open
            .drain()
            .map(|(_, (id, name, json))| {
                AgentEvent::ToolCall(ToolInvocation {
                    id,
                    name,
                    arguments: parse_tool_arguments(&json),
                })
            })
            .collect()
    }
}

/// Parse a tool call's accumulated argument JSON. An empty accumulation means
/// "no arguments" and becomes `{}`; a malformed accumulation cannot panic —
/// `AgentEvent` has no error variant to carry a parse failure through, so it
/// degrades to `Value::Null` rather than aborting the stream over one
/// unparsable call.
fn parse_tool_arguments(json: &str) -> serde_json::Value {
    if json.trim().is_empty() {
        return serde_json::json!({});
    }
    serde_json::from_str(json).unwrap_or(serde_json::Value::Null)
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

/// Concatenate a turn's text blocks into one string. Non-text blocks and
/// non-`Block` entries (a tool call is not prose) are ignored.
///
/// All three providers now wire `ToolCall`/`ToolResult` onto their own request
/// shapes (`anthropic::turn_content`, `gemini::turn_parts`,
/// `openai::turn_messages`), so this is the prose half of that mapping — the
/// text a turn carries alongside its calls — not a fallback for providers
/// that lack tool support.
pub(crate) fn turn_text(turn: &Turn) -> String {
    turn.entries
        .iter()
        .filter_map(|e| match e {
            Entry::Block(ContentBlock::Text(t)) => Some(t.text.as_str()),
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
        SseItem::ToolCallStart { .. }
        | SseItem::ToolCallArgs { .. }
        | SseItem::ToolCallEnd { .. } => {
            unreachable!(
                "tool call fragments are routed through ToolAccum in stream_sse, never item_to_event"
            )
        }
    }
}

/// Drive the SSE loop over an arbitrary chunk source: decode byte chunks
/// (splitting on `\n` so multibyte UTF-8 stays whole), drain complete lines
/// through `parse`, emit text live, surface a parsed stream error, assemble
/// streamed tool calls via a stream-lifetime [`ToolAccum`], **flush it before
/// emitting `Done`** so any call still open when the stream ends is still
/// reported, and emit `Done` last.
///
/// Split from [`stream_sse`] so the end-of-stream flush-before-`Done` ordering
/// is unit-testable without a live HTTP response — a synthetic chunk stream
/// (e.g. `futures_util::stream::iter`) exercises the same loop. This ordering
/// is load-bearing, not incidental: OpenAI (Task 11) never marks an individual
/// tool call finished, so this flush is the *only* signal that completes one.
pub(crate) async fn stream_items<S>(
    mut chunks: S,
    sink: &Arc<dyn EventSink>,
    parse: impl Fn(&str) -> Vec<SseItem>,
) -> Result<StopReason>
where
    S: futures_util::Stream<Item = std::result::Result<bytes::Bytes, reqwest::Error>> + Unpin,
{
    let mut buf: Vec<u8> = Vec::new();
    let mut accum = ToolAccum::default();
    while let Some(chunk) = chunks.next().await {
        buf.extend_from_slice(&chunk.map_err(err)?);
        for item in drain_sse(&mut buf, &parse) {
            match item {
                SseItem::ToolCallStart { .. }
                | SseItem::ToolCallArgs { .. }
                | SseItem::ToolCallEnd { .. } => {
                    if let Some(ev) = accum.apply(item) {
                        sink.emit(ev).await?;
                    }
                }
                other => sink.emit(item_to_event(other)?).await?,
            }
        }
    }
    for ev in accum.flush() {
        sink.emit(ev).await?;
    }
    sink.emit(AgentEvent::Done(StopReason::EndTurn)).await?;
    Ok(StopReason::EndTurn)
}

/// Thin wrapper: drive [`stream_items`] over a live HTTP response's byte stream.
pub(crate) async fn stream_sse(
    resp: reqwest::Response,
    sink: &Arc<dyn EventSink>,
    parse: impl Fn(&str) -> Vec<SseItem>,
) -> Result<StopReason> {
    stream_items(resp.bytes_stream(), sink, parse).await
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

/// Wrap `s` as a standard-content [`acp::ToolCallContent`] — the common case a
/// provider's tool result decodes into. A test fixture shared by every
/// provider's test module (`crate::text_content`), not part of the feature
/// under test.
#[cfg(test)]
pub(crate) fn text_content(s: &str) -> manch_protocol::acp::ToolCallContent {
    use manch_protocol::acp::{Content, TextContent, ToolCallContent};
    ToolCallContent::Content(Content::new(ContentBlock::Text(TextContent::new(
        s.to_string(),
    ))))
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

    #[test]
    fn tool_accum_assembles_a_call_from_fragments() {
        let mut acc = ToolAccum::default();
        assert!(
            acc.apply(SseItem::ToolCallStart {
                index: 0,
                id: "c1".into(),
                name: "search".into()
            })
            .is_none()
        );
        assert!(
            acc.apply(SseItem::ToolCallArgs {
                index: 0,
                json: "{\"na".into()
            })
            .is_none()
        );
        assert!(
            acc.apply(SseItem::ToolCallArgs {
                index: 0,
                json: "me\":\"Asha\"}".into()
            })
            .is_none()
        );
        let ev = acc
            .apply(SseItem::ToolCallEnd { index: 0 })
            .expect("a completed call");
        match ev {
            AgentEvent::ToolCall(inv) => {
                assert_eq!(inv.id, "c1");
                assert_eq!(inv.name, "search");
                assert_eq!(inv.arguments, serde_json::json!({ "name": "Asha" }));
            }
            _ => panic!("expected a tool call"),
        }
    }

    #[test]
    fn tool_accum_keeps_two_interleaved_calls_apart() {
        let mut acc = ToolAccum::default();
        acc.apply(SseItem::ToolCallStart {
            index: 0,
            id: "a".into(),
            name: "one".into(),
        });
        acc.apply(SseItem::ToolCallStart {
            index: 1,
            id: "b".into(),
            name: "two".into(),
        });
        acc.apply(SseItem::ToolCallArgs {
            index: 1,
            json: "{\"x\":2}".into(),
        });
        acc.apply(SseItem::ToolCallArgs {
            index: 0,
            json: "{\"x\":1}".into(),
        });
        let first = acc.apply(SseItem::ToolCallEnd { index: 0 }).unwrap();
        match first {
            AgentEvent::ToolCall(i) => assert_eq!(i.arguments, serde_json::json!({"x":1})),
            _ => panic!(),
        }
    }

    #[test]
    fn tool_accum_flushes_calls_left_open_at_stream_end() {
        // OpenAI never marks an individual call finished, so the stream ending is
        // the only signal. A pure parse_line cannot know which indexes are open.
        let mut acc = ToolAccum::default();
        acc.apply(SseItem::ToolCallStart {
            index: 0,
            id: "c1".into(),
            name: "search".into(),
        });
        acc.apply(SseItem::ToolCallArgs {
            index: 0,
            json: "{\"q\":1}".into(),
        });
        let flushed = acc.flush();
        assert_eq!(flushed.len(), 1);
        assert!(
            acc.flush().is_empty(),
            "flushing twice must not replay the call"
        );
    }

    #[test]
    fn tool_accum_treats_empty_arguments_as_an_empty_object() {
        let mut acc = ToolAccum::default();
        acc.apply(SseItem::ToolCallStart {
            index: 0,
            id: "c".into(),
            name: "n".into(),
        });
        let ev = acc.apply(SseItem::ToolCallEnd { index: 0 }).unwrap();
        match ev {
            AgentEvent::ToolCall(i) => assert_eq!(i.arguments, serde_json::json!({})),
            _ => panic!(),
        }
    }

    #[test]
    fn tool_accum_treats_malformed_arguments_as_null_not_empty_object() {
        // Distinct from the empty-accumulation case above: a genuinely
        // unparsable accumulation must not panic, but it also must not be
        // silently treated as "no arguments" (`{}`) — that would hide a real
        // wire-format problem behind a value that looks intentional.
        let mut acc = ToolAccum::default();
        acc.apply(SseItem::ToolCallStart {
            index: 0,
            id: "c".into(),
            name: "n".into(),
        });
        acc.apply(SseItem::ToolCallArgs {
            index: 0,
            json: "{not valid json".into(),
        });
        let ev = acc.apply(SseItem::ToolCallEnd { index: 0 }).unwrap();
        match ev {
            AgentEvent::ToolCall(i) => assert_eq!(i.arguments, serde_json::Value::Null),
            _ => panic!(),
        }
    }

    /// A local `EventSink` that records every emitted event for order
    /// assertions. `manch-llm` has no reason to depend on `manch-core`'s test
    /// mocks (a different crate), so this is deliberately small and local.
    #[derive(Clone, Default)]
    struct CollectSink {
        events: std::sync::Arc<std::sync::Mutex<Vec<AgentEvent>>>,
    }

    impl CollectSink {
        fn events(&self) -> Vec<AgentEvent> {
            self.events.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl EventSink for CollectSink {
        async fn emit(&self, event: AgentEvent) -> Result<()> {
            self.events.lock().unwrap().push(event);
            Ok(())
        }
    }

    #[tokio::test]
    async fn a_tool_call_left_open_at_stream_end_is_emitted_before_done() {
        // OpenAI never marks an individual call finished, so end-of-stream
        // flush is the only thing that completes one. A call emitted after
        // Done (or never flushed at all) is lost.
        let collect = CollectSink::default();
        let sink: Arc<dyn EventSink> = Arc::new(collect.clone());
        let chunks = futures_util::stream::iter(vec![Ok(bytes::Bytes::from_static(
            b"data: start\ndata: args\n",
        ))]);
        let parse = |data: &str| -> Vec<SseItem> {
            match data {
                "start" => vec![SseItem::ToolCallStart {
                    index: 0,
                    id: "c1".into(),
                    name: "search".into(),
                }],
                "args" => vec![SseItem::ToolCallArgs {
                    index: 0,
                    json: "{\"q\":1}".into(),
                }],
                _ => vec![],
            }
        };
        stream_items(chunks, &sink, parse).await.unwrap();

        let events = collect.events();
        let call_at = events
            .iter()
            .position(|e| matches!(e, AgentEvent::ToolCall(_)))
            .expect("the open tool call must be emitted at end of stream");
        let done_at = events
            .iter()
            .position(|e| matches!(e, AgentEvent::Done(_)))
            .expect("Done must be emitted");
        assert!(
            call_at < done_at,
            "the flushed tool call must precede Done; got call at {call_at}, done at {done_at}"
        );
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
        use manch_protocol::{Entry, Role, Turn};

        let turn = Turn {
            role: Role::User,
            entries: vec![
                Entry::Block(ContentBlock::Text(TextContent::new("hello".to_string()))),
                Entry::Block(ContentBlock::Text(TextContent::new("world".to_string()))),
            ],
        };
        assert_eq!(turn_text(&turn), "hello\nworld");
    }

    #[test]
    fn turn_text_ignores_non_block_entries() {
        // A tool call is not prose; it must not leak into the text of a turn.
        use manch_protocol::acp::{ContentBlock, TextContent};
        use manch_protocol::{Entry, Role, ToolInvocation, Turn};

        let turn = Turn {
            role: Role::Assistant,
            entries: vec![
                Entry::Block(ContentBlock::Text(TextContent::new("hello".to_string()))),
                Entry::ToolCall(ToolInvocation {
                    id: "c1".into(),
                    name: "t".into(),
                    arguments: serde_json::Value::Null,
                }),
            ],
        };
        assert_eq!(turn_text(&turn), "hello");
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
