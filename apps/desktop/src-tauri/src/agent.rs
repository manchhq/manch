//! BYOK + BYOC agents behind one `ChatAgent` interface.
//!
//! Inline for this slice; collapses into `manch_protocol::Agent` when `manch-core`
//! is extracted (`ask` becomes a streaming `prompt` through an `EventSink`).
//! No `rig`: the Anthropic path is a hand-rolled Messages-API call over `reqwest`.

use async_trait::async_trait;
use manch_dto::StreamEvent;

/// Anthropic model id (authoritative per the claude-api skill — do NOT change).
const ANTHROPIC_MODEL: &str = "claude-opus-4-8";
const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const MAX_TOKENS: u32 = 1024;
const CLAUDE_CODE_PKG: &str = "@agentclientprotocol/claude-agent-acp@latest";

/// reqwest 0.13's `rustls-no-provider` feature ships no crypto backend, so a
/// rustls [`CryptoProvider`] must be installed process-wide before the first
/// `reqwest::Client` is built — otherwise construction panics. We install `ring`
/// (the same backend reqwest 0.12's `rustls-tls` used) exactly once; a second
/// call is a no-op, and an `Err` here means something already installed one.
fn ensure_crypto_provider() {
    use std::sync::OnceLock;
    static INSTALLED: OnceLock<()> = OnceLock::new();
    INSTALLED.get_or_init(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    Anthropic,
    ClaudeCode,
}

impl Provider {
    pub fn from_id(id: &str) -> Option<Provider> {
        match id {
            "anthropic" => Some(Provider::Anthropic),
            "claude-code" => Some(Provider::ClaudeCode),
            _ => None,
        }
    }
}

/// One interface, two implementations (plan B). Stand-in for `manch_protocol::Agent`.
#[async_trait]
pub trait ChatAgent: Send + Sync {
    /// Stream the answer to `prompt`, emitting `Token`/`Tool` events into `sink`
    /// and finishing with `Done` (or `Error`). Returns `Err` only for failures
    /// that occur before any event could be emitted.
    async fn stream(&self, prompt: &str, sink: &dyn EventSink) -> Result<(), String>;
}

/// Build the Anthropic Messages API request body. Pure.
fn anthropic_request_body(model: &str, prompt: &str) -> serde_json::Value {
    serde_json::json!({
        "model": model,
        "max_tokens": MAX_TOKENS,
        "stream": true,
        "messages": [{ "role": "user", "content": prompt }],
    })
}

/// Concatenate the `text` blocks of an Anthropic Messages response; surface
/// `error.message` when the body is an error. Pure.
fn parse_anthropic_text(body: &serde_json::Value) -> Result<String, String> {
    if let Some(err) = body.get("error") {
        let msg = err
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown error");
        return Err(format!("anthropic: {msg}"));
    }
    let content = body
        .get("content")
        .and_then(|c| c.as_array())
        .ok_or_else(|| "anthropic: response has no content array".to_string())?;
    let text: String = content
        .iter()
        .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
        .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
        .collect();
    if text.is_empty() {
        Err("anthropic: empty text response".to_string())
    } else {
        Ok(text)
    }
}

/// Spawn args for the Claude Code ACP adapter. A leading `NAME=value` token
/// (only when a key is supplied) becomes a subprocess env var; then the launch
/// command. BYOC: Claude Code authenticates itself, so the key is an optional
/// override — `None` means "use Claude Code's own login". Pure.
fn claude_code_args(api_key: Option<&str>) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(key) = api_key {
        args.push(format!("ANTHROPIC_API_KEY={key}"));
    }
    args.push("npx".into());
    args.push("-y".into());
    args.push(CLAUDE_CODE_PKG.into());
    args
}

/// Providers offerable in the UI: every saved (BYOK) one, plus `claude-code`,
/// which is always available because it brings its own auth (BYOC). Pure.
pub fn offerable_providers(mut saved: Vec<String>) -> Vec<String> {
    if !saved.iter().any(|p| p == "claude-code") {
        saved.push("claude-code".into());
    }
    saved.sort();
    saved.dedup();
    saved
}

/// A destination for streamed agent events. Production wraps a Tauri `Channel`;
/// tests use a `Vec` collector. Keeps the agents ignorant of Tauri specifics.
pub trait EventSink: Send + Sync {
    fn emit(&self, event: StreamEvent);
}

/// Merge one streamed chunk into `buf`, returning the newly-added text to emit
/// (`None` if nothing new). ACP adapters vary — pure deltas, cumulative
/// snapshots, and a trailing full-message repeat — this tolerates all three so
/// we never double-emit ("New Delhi.New Delhi."). Pure.
pub(crate) fn push_chunk(buf: &mut String, chunk: &str) -> Option<String> {
    if chunk.is_empty() {
        None
    } else if buf.is_empty() {
        buf.push_str(chunk);
        Some(chunk.to_string())
    } else if chunk.starts_with(buf.as_str()) {
        // cumulative snapshot: the delta is the suffix beyond what we have
        let delta = chunk[buf.len()..].to_string();
        *buf = chunk.to_string();
        (!delta.is_empty()).then_some(delta)
    } else if buf.ends_with(chunk) {
        None // trailing duplicate already present
    } else {
        buf.push_str(chunk);
        Some(chunk.to_string())
    }
}

/// Extract the text of one Anthropic streaming SSE `data:` payload; `None` for
/// any non-text event (`message_start`, `ping`, `content_block_stop`, …). Pure.
pub(crate) fn parse_sse_delta(data: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(data).ok()?;
    if v.get("type")?.as_str()? != "content_block_delta" {
        return None;
    }
    let delta = v.get("delta")?;
    if delta.get("type")?.as_str()? != "text_delta" {
        return None;
    }
    Some(delta.get("text")?.as_str()?.to_string())
}

/// Map an ACP tool-call status onto the stage's `running|done|error` vocabulary.
pub(crate) fn tool_status(
    status: agent_client_protocol::schema::v1::ToolCallStatus,
) -> &'static str {
    use agent_client_protocol::schema::v1::ToolCallStatus;
    match status {
        ToolCallStatus::Completed => "done",
        ToolCallStatus::Failed => "error",
        _ => "running", // Pending / InProgress / future
    }
}

/// BYOK Anthropic via a hand-rolled Messages-API call.
pub struct AnthropicAgent {
    api_key: String,
}

impl AnthropicAgent {
    pub fn new(api_key: String) -> Self {
        Self { api_key }
    }
}

#[async_trait]
impl ChatAgent for AnthropicAgent {
    async fn stream(&self, prompt: &str, sink: &dyn EventSink) -> Result<(), String> {
        use futures_util::StreamExt;
        ensure_crypto_provider();
        let resp = reqwest::Client::new()
            .post(ANTHROPIC_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .json(&anthropic_request_body(ANTHROPIC_MODEL, prompt))
            .send()
            .await
            .map_err(|e| e.to_string())?;

        // Error responses are JSON, not SSE — surface the message.
        if !resp.status().is_success() {
            let status = resp.status();
            let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
            let msg = parse_anthropic_text(&body)
                .err()
                .unwrap_or_else(|| format!("anthropic: HTTP {status}"));
            sink.emit(StreamEvent::Error { message: msg });
            return Ok(());
        }

        // SSE frames arrive as `\n`-separated lines; buffer partial lines across
        // network chunks and act only on `data:` payloads.
        let mut stream = resp.bytes_stream();
        let mut buf = String::new();
        while let Some(chunk) = stream.next().await {
            let bytes = chunk.map_err(|e| e.to_string())?;
            buf.push_str(&String::from_utf8_lossy(&bytes));
            while let Some(nl) = buf.find('\n') {
                let line = buf[..nl].trim().to_string();
                buf.drain(..=nl);
                if let Some(data) = line.strip_prefix("data:") {
                    let data = data.trim();
                    if let Some(text) = parse_sse_delta(data) {
                        sink.emit(StreamEvent::Token { text });
                    }
                }
            }
        }
        sink.emit(StreamEvent::Done);
        Ok(())
    }
}

/// BYOC Claude Code over ACP. Stub until Task 3 wires the subprocess.
pub struct ClaudeCodeAgent {
    /// Optional BYOK override; `None` means Claude Code uses its own login (BYOC).
    api_key: Option<String>,
}

impl ClaudeCodeAgent {
    pub fn new(api_key: Option<String>) -> Self {
        Self { api_key }
    }
}

#[async_trait]
impl ChatAgent for ClaudeCodeAgent {
    async fn stream(&self, prompt: &str, sink: &dyn EventSink) -> Result<(), String> {
        use std::collections::HashMap;
        use std::sync::{Arc, Mutex};

        use agent_client_protocol::schema::ProtocolVersion;
        use agent_client_protocol::schema::v1::{
            ContentBlock, ContentChunk, InitializeRequest, NewSessionRequest, PromptRequest,
            RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
            SelectedPermissionOutcome, SessionNotification, SessionUpdate, TextContent,
        };
        use agent_client_protocol::{self as acp, AcpAgent, Agent, Client, ConnectionTo};

        let agent = AcpAgent::from_args(claude_code_args(self.api_key.as_deref()))
            .map_err(|e| e.to_string())?;
        let prompt = prompt.to_string();

        // Forward events as they arrive. The handler is a `'static` async closure,
        // so it owns Arc clones. We collect StreamEvents into a queue drained after
        // the run (the `Client` handler cannot borrow `sink` directly), preserving
        // arrival order. `buf` dedups token chunks; `names` resolves tool titles
        // across ToolCallUpdate (which may omit the title).
        let queue: Arc<Mutex<Vec<StreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let q = queue.clone();
        let buf = Arc::new(Mutex::new(String::new()));
        let names: Arc<Mutex<HashMap<String, String>>> = Arc::new(Mutex::new(HashMap::new()));
        let nb = buf.clone();
        let nn = names.clone();

        let stop = Client
            .builder()
            .on_receive_notification(
                async move |n: SessionNotification, _cx| {
                    let mut out: Vec<StreamEvent> = Vec::new();
                    match n.update {
                        SessionUpdate::AgentMessageChunk(ContentChunk {
                            content: ContentBlock::Text(text),
                            ..
                        }) => {
                            if let Some(delta) = push_chunk(&mut nb.lock().unwrap(), &text.text) {
                                out.push(StreamEvent::Token { text: delta });
                            }
                        }
                        SessionUpdate::ToolCall(tc) => {
                            let id = tc.tool_call_id.0.to_string();
                            nn.lock().unwrap().insert(id.clone(), tc.title.clone());
                            out.push(StreamEvent::Tool {
                                id,
                                name: tc.title,
                                status: tool_status(tc.status).into(),
                                detail: None,
                            });
                        }
                        SessionUpdate::ToolCallUpdate(u) => {
                            let id = u.tool_call_id.0.to_string();
                            let name = u
                                .fields
                                .title
                                .clone()
                                .or_else(|| nn.lock().unwrap().get(&id).cloned())
                                .unwrap_or_default();
                            out.push(StreamEvent::Tool {
                                id,
                                name,
                                status: u
                                    .fields
                                    .status
                                    .map(tool_status)
                                    .unwrap_or("running")
                                    .into(),
                                detail: None,
                            });
                        }
                        _ => {}
                    }
                    if !out.is_empty() {
                        q.lock().unwrap().extend(out);
                    }
                    Ok(())
                },
                acp::on_receive_notification!(),
            )
            .on_receive_request(
                async move |request: RequestPermissionRequest, responder, _connection| match request
                    .options
                    .first()
                    .map(|opt| opt.option_id.clone())
                {
                    Some(id) => responder.respond(RequestPermissionResponse::new(
                        RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(id)),
                    )),
                    None => responder.respond(RequestPermissionResponse::new(
                        RequestPermissionOutcome::Cancelled,
                    )),
                },
                acp::on_receive_request!(),
            )
            .connect_with(agent, |connection: ConnectionTo<Agent>| async move {
                connection
                    .send_request(InitializeRequest::new(ProtocolVersion::V1))
                    .block_task()
                    .await?;
                let cwd = std::env::temp_dir().join("manch-acp-workspace");
                std::fs::create_dir_all(&cwd).ok();
                let session = connection
                    .send_request(NewSessionRequest::new(cwd))
                    .block_task()
                    .await?;
                let response = connection
                    .send_request(PromptRequest::new(
                        session.session_id,
                        vec![ContentBlock::Text(TextContent::new(prompt))],
                    ))
                    .block_task()
                    .await?;
                Ok(response.stop_reason)
            })
            .await
            .map_err(|e| e.to_string())?;

        // Drain queued events in arrival order, then finish.
        let events = std::mem::take(&mut *queue.lock().unwrap());
        let had_text = events
            .iter()
            .any(|e| matches!(e, StreamEvent::Token { .. }));
        for ev in events {
            sink.emit(ev);
        }
        if had_text {
            sink.emit(StreamEvent::Done);
        } else {
            sink.emit(StreamEvent::Error {
                message: format!("claude-code returned no text (stop reason: {stop:?})"),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_providers_parse() {
        assert_eq!(Provider::from_id("anthropic"), Some(Provider::Anthropic));
        assert_eq!(Provider::from_id("claude-code"), Some(Provider::ClaudeCode));
    }

    #[test]
    fn unknown_provider_is_none() {
        assert_eq!(Provider::from_id("gemini"), None);
        assert_eq!(Provider::from_id(""), None);
    }

    #[test]
    fn request_body_has_model_and_user_message() {
        let body = anthropic_request_body(ANTHROPIC_MODEL, "hi");
        assert_eq!(body["model"], "claude-opus-4-8");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "hi");
        assert!(body["max_tokens"].is_number());
        assert_eq!(body["stream"], true);
    }

    #[test]
    fn parses_concatenated_text_blocks() {
        let body = serde_json::json!({
            "content": [
                { "type": "text", "text": "New " },
                { "type": "text", "text": "Delhi" }
            ]
        });
        assert_eq!(parse_anthropic_text(&body).unwrap(), "New Delhi");
    }

    #[test]
    fn surfaces_error_message() {
        let body = serde_json::json!({ "error": { "message": "invalid x-api-key" } });
        assert_eq!(
            parse_anthropic_text(&body).unwrap_err(),
            "anthropic: invalid x-api-key"
        );
    }

    #[test]
    fn claude_code_args_without_key_is_just_npx() {
        let args = claude_code_args(None);
        assert_eq!(args[0], "npx");
        assert!(args.iter().any(|a| a.contains("claude-agent-acp")));
        assert!(!args.iter().any(|a| a.starts_with("ANTHROPIC_API_KEY=")));
    }

    #[test]
    fn claude_code_args_with_key_prepends_env() {
        let args = claude_code_args(Some("sk-test"));
        assert_eq!(args[0], "ANTHROPIC_API_KEY=sk-test");
        assert!(args.iter().any(|a| a == "npx"));
        assert!(args.iter().any(|a| a.contains("claude-agent-acp")));
    }

    #[test]
    fn claude_code_always_offered_byoc_brings_own_auth() {
        assert_eq!(offerable_providers(vec![]), vec!["claude-code".to_string()]);
        assert_eq!(
            offerable_providers(vec!["anthropic".into()]),
            vec!["anthropic".to_string(), "claude-code".to_string()]
        );
        assert_eq!(
            offerable_providers(vec!["claude-code".into()]),
            vec!["claude-code".to_string()]
        );
    }

    // --- push_chunk tests ---

    #[test]
    fn push_chunk_returns_full_first_chunk() {
        let mut b = String::new();
        assert_eq!(push_chunk(&mut b, "New "), Some("New ".to_string()));
        assert_eq!(b, "New ");
    }

    #[test]
    fn push_chunk_returns_only_the_delta_for_cumulative_snapshot() {
        let mut b = String::new();
        push_chunk(&mut b, "New");
        assert_eq!(
            push_chunk(&mut b, "New Delhi."),
            Some(" Delhi.".to_string())
        );
        assert_eq!(b, "New Delhi.");
    }

    #[test]
    fn push_chunk_appends_distinct_delta() {
        let mut b = String::new();
        push_chunk(&mut b, "New");
        assert_eq!(push_chunk(&mut b, " Delhi."), Some(" Delhi.".to_string()));
        assert_eq!(b, "New Delhi.");
    }

    #[test]
    fn push_chunk_drops_trailing_full_repeat() {
        let mut b = String::new();
        push_chunk(&mut b, "New");
        push_chunk(&mut b, " Delhi.");
        // after delta accumulation buf is already "New Delhi."; the full repeat is dropped
        assert_eq!(push_chunk(&mut b, "New Delhi."), None);
        // final identical repeat also yields no new delta
        assert_eq!(push_chunk(&mut b, "New Delhi."), None);
        assert_eq!(b, "New Delhi.");
    }

    // --- parse_sse_delta tests ---

    #[test]
    fn parse_sse_delta_extracts_text_delta() {
        let data =
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hi"}}"#;
        assert_eq!(parse_sse_delta(data), Some("Hi".to_string()));
    }

    #[test]
    fn parse_sse_delta_ignores_non_text_events() {
        assert_eq!(parse_sse_delta(r#"{"type":"message_start"}"#), None);
        assert_eq!(parse_sse_delta(r#"{"type":"ping"}"#), None);
        assert_eq!(parse_sse_delta("not json"), None);
    }

    // --- tool_status tests ---

    #[test]
    fn tool_status_maps_acp_to_stage_vocabulary() {
        use agent_client_protocol::schema::v1::ToolCallStatus;
        assert_eq!(tool_status(ToolCallStatus::Completed), "done");
        assert_eq!(tool_status(ToolCallStatus::Failed), "error");
        assert_eq!(tool_status(ToolCallStatus::InProgress), "running");
        assert_eq!(tool_status(ToolCallStatus::Pending), "running");
    }
}
