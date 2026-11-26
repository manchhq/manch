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

// Fireworks rides OpenAI's wire format, so it cannot be enabled without it.
#[cfg(feature = "fireworks")]
pub mod fireworks;

mod completion;
mod generation;
mod http;
pub use completion::{
    Completion, CompletionCollector, CompletionFailure, PartialCompletion, collect_completion,
};
pub use generation::{
    GenerationOptions, ModelPolicy, ReasoningEffort, ReasoningKind, ReasoningOptions, ThinkingLevel,
};
pub use http::{DEFAULT_CONNECT_TIMEOUT, DEFAULT_MAX_RETRIES, DEFAULT_READ_TIMEOUT};

/// What a model is *for*. Derived from whatever the provider publishes, so it
/// is deliberately coarse: the question it answers is "would offering this in a
/// chat picker be a mistake", not "what family is this".
///
/// `#[non_exhaustive]` from birth — catalogues gain kinds far faster than this
/// crate ships breaking releases (see #40 for the cost of learning that late).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ModelKind {
    /// Prompt-and-respond. The only kind an [`Agent`](manch_protocol::Agent) can drive.
    Chat,
    /// Produces vectors, not prose. Fireworks reports these as chat-capable,
    /// which they are not in any useful sense.
    Embedding,
    /// Generates images rather than text.
    Image,
}

/// A model advertised by a provider's list-models endpoint.
///
/// Everything past `id` is `Option` because providers disagree wildly about
/// what they publish — Fireworks returns capability flags and a context length,
/// Gemini returns token limits and a thinking flag, and OpenAI and Anthropic
/// return essentially nothing. **An unknown capability reads as `None`, never
/// as `false`**: a host that cannot tell "does not support tools" from "was not
/// told" will route on the difference and be wrong.
///
/// `#[non_exhaustive]` because this is an output type — callers read it, and
/// catalogues will keep publishing more than they do today. Build one with
/// [`ModelInfo::new`].
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[non_exhaustive]
pub struct ModelInfo {
    pub id: String,
    pub display_name: Option<String>,
    /// Maximum input the model accepts, in tokens.
    pub context_window: Option<u32>,
    /// Maximum the model will generate in one response, in tokens.
    pub max_output_tokens: Option<u32>,
    /// Whether the model can be offered host-registered tools.
    pub supports_tools: Option<bool>,
    /// Whether the model accepts image content blocks (see the multimodal
    /// mapping in each provider module). A host should know this *before*
    /// sending a page, not from the provider's error.
    pub supports_image_input: Option<bool>,
    /// Whether this is a thinking/reasoning model. Not cosmetic: Gemini's
    /// thinking models require `thoughtSignature` to be echoed back, which is a
    /// behavioural difference a host currently cannot see coming.
    pub reasoning: Option<bool>,
    pub kind: Option<ModelKind>,
}

impl ModelInfo {
    /// A model known only by its id — every capability unknown.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            display_name: None,
            context_window: None,
            max_output_tokens: None,
            supports_tools: None,
            supports_image_input: None,
            reasoning: None,
            kind: None,
        }
    }
}

/// One fact parsed out of an SSE line: streamed text, provider token counts, a
/// surfaced error message, or one fragment of a streamed tool call.
///
/// A tool call arrives across several SSE frames (start, N argument-JSON
/// fragments, end), but `parse_line` is a pure function of one line and must
/// stay that way — so parsers emit these fragments and [`ToolAccum`], owned by
/// [`stream_sse`] for the life of the stream, assembles them.
#[derive(Debug)]
pub(crate) enum SseItem {
    Text(String),
    Thought(String),
    Metadata(manch_protocol::ProviderMetadata),
    Usage(Usage),
    Error(String),
    Closed,
    ToolFinish,
    /// A tool call has begun at `index` (the provider's stream slot).
    ToolCallStart {
        index: u32,
        id: String,
        name: String,
        /// Provider data that must be echoed back verbatim when this call is
        /// replayed on a later turn. Opaque here — the provider that captured
        /// it owns its shape. `None` for dialects that need nothing.
        provider_meta: Option<serde_json::Value>,
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
    /// The provider's own reason for ending the turn, translated into ACP's
    /// vocabulary. Consumed by [`stream_items`] rather than becoming an event:
    /// exactly one `AgentEvent::Done` is emitted per turn, and this decides
    /// what it carries.
    Stop(StopReason),
}

/// Output cap applied when a host sets none.
///
/// Roughly 6,000 words — enough for a drafted contract section or a long
/// analysis, which the previous Anthropic-only constant of 1024 (~750 words)
/// cut off mid-sentence. Chosen to be accepted by every current frontier model:
/// Anthropic *requires* `max_tokens` and rejects a value above the model's
/// ceiling, so the default has to clear the lowest ceiling in use rather than
/// the highest.
///
/// A host that knows better should say so — [`ModelInfo::max_output_tokens`]
/// carries the model's real ceiling where the provider publishes it (Gemini
/// does), so the intended flow is to read it and pass it to the agent's
/// `max_output_tokens` builder rather than to raise this constant.
pub const DEFAULT_MAX_OUTPUT_TOKENS: u32 = 8192;

/// Translate a provider's own stop/finish reason into ACP's [`StopReason`].
///
/// The three dialects spell the same facts differently — Anthropic
/// `"max_tokens"`, OpenAI `"length"`, Gemini `"MAX_TOKENS"` — and all three
/// were previously discarded, so a truncated turn was indistinguishable from a
/// complete one. Unknown values remain unknown and make a stream incomplete.
pub(crate) fn stop_reason(raw: &str) -> Option<StopReason> {
    match raw {
        "max_tokens" | "length" | "MAX_TOKENS" => Some(StopReason::MaxTokens),
        // Anthropic reports a refusal outright; OpenAI and Gemini report the
        // filter that produced it. ACP has one word for all of them.
        "refusal" | "content_filter" | "SAFETY" | "PROHIBITED_CONTENT" | "BLOCKLIST"
        | "IMAGE_SAFETY" => Some(StopReason::Refusal),
        "STOP" | "stop" | "end_turn" | "tool_calls" | "tool_use" | "stop_sequence" => {
            Some(StopReason::EndTurn)
        }
        _ => None,
    }
}

pub(crate) fn stop_item(raw: &str) -> SseItem {
    if matches!(raw, "tool_calls" | "tool_use") {
        return SseItem::ToolFinish;
    }
    stop_reason(raw)
        .map(SseItem::Stop)
        .unwrap_or_else(|| SseItem::Error("unrecognized provider finish status".into()))
}

pub(crate) fn incomplete() -> manch_protocol::Error {
    provider_failure(
        "provider",
        None,
        manch_protocol::ProviderFailureKind::Incomplete,
    )
}

/// Assembles streamed tool-call fragments into complete [`AgentEvent::ToolCall`]s.
///
/// Owned by [`stream_sse`] for the life of one stream — never by a parser,
/// which must stay a pure function of one line. Keyed by the provider's
/// stream `index` because a model may open several tool calls before closing
/// any of them (interleaved calls).
pub(crate) struct ToolAccum {
    /// index -> (id, name, accumulated argument JSON, provider round-trip data).
    open: HashMap<u32, (String, String, String, Option<serde_json::Value>)>,
    order: Vec<u32>,
    ended: std::collections::HashSet<u32>,
    next_id: u64,
    namespace: String,
}

impl Default for ToolAccum {
    fn default() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static ROUND: AtomicU64 = AtomicU64::new(0);
        let time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        Self {
            open: HashMap::new(),
            order: Vec::new(),
            ended: std::collections::HashSet::new(),
            next_id: 0,
            namespace: format!("{time:x}-{:x}", ROUND.fetch_add(1, Ordering::Relaxed)),
        }
    }
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
            SseItem::ToolCallStart {
                index,
                id,
                name,
                provider_meta,
            } => {
                let id = if id.is_empty() {
                    format!("manch-call-{}-{}", self.namespace, self.next_id)
                } else {
                    id
                };
                self.next_id += 1;
                if !self.open.contains_key(&index) {
                    self.order.push(index);
                }
                self.open
                    .insert(index, (id, name, String::new(), provider_meta));
                None
            }
            SseItem::ToolCallArgs { index, json } => {
                if let Some(entry) = self.open.get_mut(&index) {
                    entry.2.push_str(&json);
                }
                None
            }
            SseItem::ToolCallEnd { index } => {
                if !self.open.contains_key(&index) {
                    return None;
                }
                self.ended.insert(index);
                let first = *self.order.first()?;
                if !self.ended.contains(&first) {
                    return None;
                }
                self.order.remove(0);
                self.ended.remove(&first);
                let (id, name, json, provider_meta) = self.open.remove(&first)?;
                Some(AgentEvent::ToolCall(ToolInvocation {
                    id,
                    name,
                    arguments: parse_tool_arguments(&json),
                    provider_meta,
                }))
            }
            _ => None,
        }
    }

    /// Complete every call still open when the stream ends (OpenAI never
    /// marks an individual call finished; the stream ending is the only
    /// signal). Drains the map, so calling `flush` twice cannot replay a call.
    pub(crate) fn flush(&mut self) -> Vec<AgentEvent> {
        self.ended.clear();
        std::mem::take(&mut self.order)
            .into_iter()
            .filter_map(|index| self.open.remove(&index))
            .map(|(id, name, json, provider_meta)| {
                AgentEvent::ToolCall(ToolInvocation {
                    id,
                    name,
                    arguments: parse_tool_arguments(&json),
                    provider_meta,
                })
            })
            .collect()
    }
}

/// Parse a tool call's accumulated argument JSON. An empty accumulation means
/// "no arguments" and becomes `{}`; a malformed accumulation cannot panic —
/// the internal sentinel is `Value::Null`. The stream driver rejects it before
/// emitting a ToolCall or declaring completion.
///
/// **`Null` is load-bearing.** `manch-core` reads it as "these arguments never
/// parsed" and answers the call with an error the model can act on instead of
/// dispatching it (`dispatch::UNREADABLE_ARGUMENTS`). It works as a sentinel
/// precisely because no model can send `null` as an argument *object* — which
/// is also why `{}` was rejected as the degrade target: `{}` is valid arguments
/// for a zero-argument tool, so degrading garbage to it would execute a call
/// nobody could read. Changing this value silently re-enables that.
fn parse_tool_arguments(json: &str) -> serde_json::Value {
    if json.trim().is_empty() {
        return serde_json::json!({});
    }
    serde_json::from_str(json).unwrap_or(serde_json::Value::Null)
}

/// Drain complete SSE events (blank-line delimited), joining their data lines.
/// Retain incomplete events, including a terminal JSON line without its event
/// delimiter, so EOF cannot turn a truncated response into success.
pub(crate) fn drain_sse(buf: &mut Vec<u8>, parse: impl Fn(&str) -> Vec<SseItem>) -> Vec<SseItem> {
    let mut out = Vec::new();
    loop {
        let mut line_start = 0;
        let mut end = None;
        for (index, byte) in buf.iter().enumerate() {
            if *byte == b'\n' {
                let line = &buf[line_start..index];
                if line.is_empty() || line == b"\r" {
                    end = Some(index + 1);
                    break;
                }
                line_start = index + 1;
            }
        }
        let Some(end) = end else {
            break;
        };
        let event: Vec<u8> = buf.drain(..end).collect();
        let Ok(event) = std::str::from_utf8(&event) else {
            out.push(SseItem::Error("invalid provider UTF-8".into()));
            continue;
        };
        let data: Vec<_> = event
            .lines()
            .filter_map(|line| {
                line.strip_prefix("data:")
                    .map(|data| data.strip_prefix(' ').unwrap_or(data))
            })
            .collect();
        if !data.is_empty() {
            out.extend(parse(&data.join("\n")));
        }
    }
    out
}

/// Bytes plus media type from a block that carries binary content: an
/// `Image`, or a `Resource` holding a blob (which is how a PDF arrives —
/// ACP has no document variant of its own).
///
/// `None` for text, for a `ResourceLink` (a URI whose bytes we do not hold),
/// and for a blob with no declared media type. That last one is deliberately
/// not guessed: sending a PDF as whatever we assumed fails somewhere far less
/// legible than here.
pub(crate) fn binary_block(block: &ContentBlock) -> Option<(&str, &str)> {
    match block {
        ContentBlock::Image(i) => Some((i.mime_type.as_str(), i.data.as_str())),
        ContentBlock::Resource(r) => match &r.resource {
            manch_protocol::acp::EmbeddedResourceResource::BlobResourceContents(b) => {
                Some((b.mime_type.as_deref()?, b.blob.as_str()))
            }
            _ => None,
        },
        _ => None,
    }
}

/// Reject content that would otherwise disappear during provider mapping.
/// Diagnostics intentionally never include MIME strings, URIs or document data.
pub(crate) fn unsupported_resource(
    turns: &[Turn],
    links: bool,
    accepts: fn(&str) -> bool,
) -> Option<String> {
    for turn in turns {
        for entry in &turn.entries {
            if turn.role == manch_protocol::Role::System
                && !matches!(entry, Entry::Block(ContentBlock::Text(_)))
            {
                return Some("system instructions support text only".into());
            }
            let valid=match entry {
                Entry::Block(ContentBlock::Text(_))=>true,
                Entry::Block(ContentBlock::Image(image))=>image.mime_type.starts_with("image/") && accepts(&image.mime_type),
                Entry::Block(block @ ContentBlock::Resource(_))=>binary_block(block).is_some_and(|(mime,_)|accepts(mime)),
                Entry::Block(ContentBlock::ResourceLink(link))=>links && !link.name.is_empty() && link.mime_type.as_deref().is_some_and(accepts) && reqwest::Url::parse(&link.uri).is_ok_and(|url|matches!(url.scheme(),"https"|"gs")),
                Entry::Block(_)=>false,
                Entry::ToolResult{content,..}=>content.iter().all(|c|matches!(c,manch_protocol::acp::ToolCallContent::Content(c) if matches!(c.content,ContentBlock::Text(_)))),
                Entry::ToolCall(_)=>true,
            };
            if !valid {
                return Some("content cannot be represented by this provider".into());
            }
        }
    }
    None
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

/// Discard untrusted decoder/stream detail and retain a safe classification.
pub(crate) fn err(_error: impl ToString) -> manch_protocol::Error {
    provider_failure(
        "provider",
        None,
        manch_protocol::ProviderFailureKind::Stream,
    )
}

pub(crate) fn provider_failure(
    provider: &str,
    status: Option<u16>,
    kind: manch_protocol::ProviderFailureKind,
) -> manch_protocol::Error {
    manch_protocol::Error::Provider {
        provider: provider.into(),
        status,
        kind,
    }
}

/// Build a `ModelInfo` for a provider's fallback id.
pub(crate) fn fallback_model(id: &str) -> ModelInfo {
    ModelInfo::new(id)
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

/// Classify HTTP failures without reading or retaining untrusted response prose.
pub(crate) async fn http_error(provider: &str, resp: reqwest::Response) -> manch_protocol::Error {
    let status = resp.status();
    use manch_protocol::ProviderFailureKind as K;
    let kind = match status.as_u16() {
        401 | 403 => K::Authentication,
        429 => K::RateLimit,
        500..=599 => K::Server,
        _ => K::InvalidRequest,
    };
    provider_failure(provider, Some(status.as_u16()), kind)
}

/// Read one token count out of a provider's usage object. Absent or non-numeric
/// fields yield `None` rather than zero — "not reported" and "zero tokens" are
/// different facts, and a consumer summing these must be able to tell them apart.
pub(crate) fn token_count(usage: &serde_json::Value, key: &str) -> Option<u64> {
    usage.get(key)?.as_u64()
}

/// Read a token count nested one level down, as the OpenAI dialect reports its
/// cache and reasoning breakdowns (`prompt_tokens_details.cached_tokens`,
/// `completion_tokens_details.reasoning_tokens`). Reading either off the top
/// level yields a well-typed `None` on every request, which is why it has its
/// own helper rather than an inline `get`.
pub(crate) fn nested_token_count(usage: &serde_json::Value, outer: &str, key: &str) -> Option<u64> {
    token_count(usage.get(outer)?, key)
}

/// Map one parsed SSE item to the event it emits. An `Error` item ends the turn
/// rather than producing an event. Pure — split from [`stream_sse`] so the
/// mapping is unit-testable without a live HTTP response.
pub(crate) fn item_to_event(item: SseItem) -> Result<AgentEvent> {
    match item {
        SseItem::Text(t) => Ok(AgentEvent::text_chunk(t)),
        SseItem::Thought(t) => Ok(AgentEvent::Update(
            manch_protocol::acp::SessionUpdate::AgentThoughtChunk(
                manch_protocol::acp::ContentChunk::new(ContentBlock::Text(
                    manch_protocol::acp::TextContent::new(t),
                )),
            ),
        )),
        SseItem::Usage(u) => Ok(AgentEvent::Usage(u)),
        SseItem::Metadata(meta) => Ok(AgentEvent::Metadata(meta)),
        SseItem::Error(e) => Err(err(e)),
        // `stream_items` intercepts this before it can reach here.
        SseItem::Stop(_) | SseItem::Closed | SseItem::ToolFinish => {
            unreachable!("terminal markers are consumed by stream_items")
        }
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
    // EOF alone is not proof of completion; require a known terminal verdict.
    let mut stop = None;
    let mut closed = false;
    let mut calls = 0;
    let mut ids = std::collections::HashSet::new();
    while let Some(chunk) = chunks.next().await {
        buf.extend_from_slice(&chunk.map_err(http::transport_err)?);
        for item in drain_sse(&mut buf, &parse) {
            if closed {
                return Err(incomplete());
            }
            if stop.is_some()
                && !matches!(
                    &item,
                    SseItem::Usage(_) | SseItem::Metadata(_) | SseItem::Closed
                )
            {
                return Err(incomplete());
            }
            match &item {
                SseItem::ToolCallStart { index, .. } => {
                    if accum.open.contains_key(index) {
                        return Err(incomplete());
                    }
                    calls += 1;
                }
                SseItem::ToolCallArgs { index, .. }
                    if !accum.open.contains_key(index) || accum.ended.contains(index) =>
                {
                    return Err(incomplete());
                }
                _ => {}
            }
            match item {
                SseItem::ToolCallStart { .. }
                | SseItem::ToolCallArgs { .. }
                | SseItem::ToolCallEnd { .. } => {
                    if let Some(ev) = accum.apply(item) {
                        emit_valid_call(sink, ev, &mut ids).await?;
                    }
                }
                // Recorded, not emitted — exactly one `Done` closes a turn, and
                // it is sent below once the stream is fully drained.
                SseItem::Stop(s) => stop = Some(s),
                SseItem::Closed => closed = true,
                SseItem::ToolFinish => {
                    if calls == 0 {
                        return Err(incomplete());
                    }
                    stop = Some(StopReason::EndTurn);
                }
                other => sink.emit(item_to_event(other)?).await?,
            }
        }
    }
    if buf.iter().any(|b| !b.is_ascii_whitespace()) {
        return Err(incomplete());
    }
    let stop = stop.ok_or_else(incomplete)?;
    for ev in accum.flush() {
        emit_valid_call(sink, ev, &mut ids).await?;
    }
    sink.emit(AgentEvent::Done(stop)).await?;
    Ok(stop)
}

async fn emit_valid_call(
    sink: &Arc<dyn EventSink>,
    event: AgentEvent,
    ids: &mut std::collections::HashSet<String>,
) -> Result<()> {
    if let AgentEvent::ToolCall(call) = &event
        && (call.name.is_empty() || !call.arguments.is_object() || !ids.insert(call.id.clone()))
    {
        return Err(incomplete());
    }
    sink.emit(event).await
}

/// Thin wrapper: drive [`stream_items`] over a live HTTP response's byte stream.
pub(crate) async fn stream_sse(
    resp: reqwest::Response,
    sink: &Arc<dyn EventSink>,
    parse: impl Fn(&str) -> Vec<SseItem>,
) -> Result<StopReason> {
    let request_id = resp
        .headers()
        .get("x-request-id")
        .or_else(|| resp.headers().get("request-id"))
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    if request_id.is_some() {
        sink.emit(AgentEvent::Metadata(manch_protocol::ProviderMetadata {
            request_id,
            ..Default::default()
        }))
        .await?;
    }
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
        // Fireworks has its own catalogue *and* its own capability fields, so
        // it routes to its own module rather than OpenAI's despite sharing the
        // request dialect.
        #[cfg(feature = "fireworks")]
        "fireworks" => fireworks::list_models_at(api_key, base).await,
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
    fn interleaved_call_ends_cannot_reorder_issue_order() {
        let mut accum = ToolAccum::default();
        for index in [0, 1] {
            accum.apply(SseItem::ToolCallStart {
                index,
                id: format!("c{index}"),
                name: "echo".into(),
                provider_meta: None,
            });
        }
        assert!(accum.apply(SseItem::ToolCallEnd { index: 1 }).is_none());
        let AgentEvent::ToolCall(first) = accum.apply(SseItem::ToolCallEnd { index: 0 }).unwrap()
        else {
            panic!()
        };
        assert_eq!(first.id, "c0");
        let remaining = accum.flush();
        assert!(matches!(remaining.as_slice(),[AgentEvent::ToolCall(second)] if second.id=="c1"));
    }

    #[test]
    fn multiline_sse_data_is_one_complete_event() {
        let mut bytes = b"data: first\r\ndata: second\r\n\r\n".to_vec();
        let items = drain_sse(&mut bytes, |data| vec![SseItem::Text(data.to_owned())]);
        assert!(matches!(items.as_slice(),[SseItem::Text(text)] if text=="first\nsecond"));
    }

    #[test]
    fn generated_call_ids_do_not_repeat_between_rounds() {
        let mut ids = Vec::new();
        for _ in 0..2 {
            let mut accum = ToolAccum::default();
            accum.apply(SseItem::ToolCallStart {
                index: 0,
                id: String::new(),
                name: "echo".into(),
                provider_meta: None,
            });
            let AgentEvent::ToolCall(call) =
                accum.apply(SseItem::ToolCallEnd { index: 0 }).unwrap()
            else {
                panic!()
            };
            ids.push(call.id);
        }
        assert_ne!(ids[0], ids[1]);
    }

    #[test]
    fn invalid_utf8_is_not_silently_replaced() {
        let mut bytes = b"data: ".to_vec();
        bytes.extend_from_slice(&[0xff, b'\n', b'\n']);
        let items = drain_sse(&mut bytes, |text| vec![SseItem::Text(text.to_owned())]);
        assert!(matches!(items.first(), Some(SseItem::Error(_))));
    }

    #[test]
    fn openai_flush_preserves_start_order() {
        let mut accum = ToolAccum::default();
        for index in (0..32).rev() {
            accum.apply(SseItem::ToolCallStart {
                index,
                id: format!("c{index}"),
                name: "search".into(),
                provider_meta: None,
            });
        }
        let ids: Vec<_> = accum
            .flush()
            .into_iter()
            .map(|ev| match ev {
                AgentEvent::ToolCall(call) => call.id,
                _ => panic!(),
            })
            .collect();
        assert_eq!(
            ids,
            (0..32)
                .rev()
                .map(|index| format!("c{index}"))
                .collect::<Vec<_>>()
        );
    }

    #[cfg(feature = "gemini")]
    #[tokio::test]
    async fn parallel_calls_keep_issue_order_and_distinct_ids() {
        let collect = CollectSink::default();
        let sink: Arc<dyn EventSink> = Arc::new(collect.clone());
        let chunks = futures_util::stream::iter(vec![Ok(bytes::Bytes::from_static(
            b"data: {\"candidates\":[{\"content\":{\"parts\":[{\"functionCall\":{\"name\":\"search\",\"args\":{\"q\":1}}}]}}]}\n\ndata: {\"candidates\":[{\"content\":{\"parts\":[{\"functionCall\":{\"name\":\"search\",\"args\":{\"q\":2}}}]},\"finishReason\":\"STOP\"}]}\n\n"
        ))]);
        stream_items(chunks, &sink, crate::gemini::parse_line)
            .await
            .unwrap();
        let calls: Vec<_> = collect
            .events()
            .into_iter()
            .filter_map(|ev| match ev {
                AgentEvent::ToolCall(call) => Some(call),
                _ => None,
            })
            .collect();
        assert_eq!(calls.len(), 2);
        assert_ne!(calls[0].id, calls[1].id);
        assert_eq!(calls[0].arguments, serde_json::json!({"q":1}));
        assert_eq!(calls[1].arguments, serde_json::json!({"q":2}));
    }

    #[test]
    fn drain_sse_extracts_data_lines_and_leaves_partial() {
        let mut buf = b"data: {\"t\":1}\n\ndata: partial".to_vec();
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
                name: "search".into(),
                provider_meta: None,
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
            provider_meta: None,
        });
        acc.apply(SseItem::ToolCallStart {
            index: 1,
            id: "b".into(),
            name: "two".into(),
            provider_meta: None,
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
            provider_meta: None,
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
            provider_meta: None,
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
            provider_meta: None,
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
            b"data: start\n\ndata: args\n\ndata: done\n\n",
        ))]);
        let parse = |data: &str| -> Vec<SseItem> {
            match data {
                "start" => vec![SseItem::ToolCallStart {
                    index: 0,
                    id: "c1".into(),
                    name: "search".into(),
                    provider_meta: None,
                }],
                "done" => vec![SseItem::Stop(StopReason::EndTurn)],
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
                    provider_meta: None,
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
            thought_tokens: Some(42),
            ..Default::default()
        };
        let ev = item_to_event(SseItem::Usage(u)).unwrap();
        assert!(matches!(ev, AgentEvent::Usage(got) if got == u));
    }

    #[test]
    fn item_to_event_turns_an_error_item_into_a_turn_error() {
        let e = item_to_event(SseItem::Error("openai: boom".into())).unwrap_err();
        assert!(matches!(
            e,
            manch_protocol::Error::Provider {
                kind: manch_protocol::ProviderFailureKind::Stream,
                ..
            }
        ));
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

    #[tokio::test]
    async fn a_stream_that_reports_truncation_returns_max_tokens() {
        // The gap a `parse_line` test cannot cover: emitting `SseItem::Stop` is
        // useless unless `stream_items` carries it into the `Done` event and the
        // returned `StopReason`. Making the recorded value unused still passed
        // every per-provider test, which is what this is here to prevent.
        let collect = CollectSink::default();
        let sink: Arc<dyn EventSink> = Arc::new(collect.clone());
        let chunks = futures_util::stream::iter(vec![Ok(bytes::Bytes::from_static(
            b"data: text\n\ndata: cut\n\n",
        ))]);
        let parse = |data: &str| -> Vec<SseItem> {
            match data {
                "text" => vec![SseItem::Text("half a sen".into())],
                "cut" => vec![SseItem::Stop(StopReason::MaxTokens)],
                _ => vec![],
            }
        };

        let returned = stream_items(chunks, &sink, parse).await.unwrap();
        assert_eq!(returned, StopReason::MaxTokens);
        assert!(
            collect
                .events()
                .iter()
                .any(|e| matches!(e, AgentEvent::Done(StopReason::MaxTokens))),
            "the host learns about truncation through Done, not just the return"
        );
    }

    #[tokio::test]
    async fn a_stream_that_says_nothing_is_incomplete() {
        let collect = CollectSink::default();
        let sink: Arc<dyn EventSink> = Arc::new(collect.clone());
        let chunks = futures_util::stream::iter(Vec::<
            std::result::Result<bytes::Bytes, reqwest::Error>,
        >::new());
        assert!(stream_items(chunks, &sink, |_| vec![]).await.is_err());
        assert!(collect.events().is_empty());
    }
}
