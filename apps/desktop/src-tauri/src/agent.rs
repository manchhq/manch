//! BYOK + BYOC agents behind one `ChatAgent` interface.
//!
//! Inline for this slice; collapses into `manch_protocol::Agent` when `manch-core`
//! is extracted (`ask` becomes a streaming `prompt` through an `EventSink`).
//! No `rig`: the Anthropic path is a hand-rolled Messages-API call over `reqwest`.

use async_trait::async_trait;

/// Anthropic model id (authoritative per the claude-api skill — do NOT change).
const ANTHROPIC_MODEL: &str = "claude-opus-4-8";
const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const MAX_TOKENS: u32 = 1024;
const CLAUDE_CODE_PKG: &str = "@agentclientprotocol/claude-agent-acp@latest";

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
    async fn ask(&self, prompt: &str) -> Result<String, String>;
}

/// Build the Anthropic Messages API request body. Pure.
fn anthropic_request_body(model: &str, prompt: &str) -> serde_json::Value {
    serde_json::json!({
        "model": model,
        "max_tokens": MAX_TOKENS,
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
    async fn ask(&self, prompt: &str) -> Result<String, String> {
        let resp = reqwest::Client::new()
            .post(ANTHROPIC_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .json(&anthropic_request_body(ANTHROPIC_MODEL, prompt))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let status = resp.status();
        let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            return Err(parse_anthropic_text(&body)
                .err()
                .unwrap_or_else(|| format!("anthropic: HTTP {status}")));
        }
        parse_anthropic_text(&body)
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
    async fn ask(&self, prompt: &str) -> Result<String, String> {
        use std::sync::{Arc, Mutex};

        use agent_client_protocol::schema::{
            ContentBlock, ContentChunk, InitializeRequest, NewSessionRequest, PromptRequest,
            ProtocolVersion, RequestPermissionOutcome, RequestPermissionRequest,
            RequestPermissionResponse, SelectedPermissionOutcome, SessionNotification,
            SessionUpdate, TextContent,
        };
        use agent_client_protocol::{self as acp, AcpAgent, Agent, Client, ConnectionTo};

        // BYOC auth: `from_args` leading `NAME=value` tokens become subprocess env
        // vars; with `None` no key is injected and Claude Code uses its own login.
        let agent = AcpAgent::from_args(claude_code_args(self.api_key.as_deref()))
            .map_err(|e| e.to_string())?;
        let prompt = prompt.to_string();

        // Assistant text streams in as `AgentMessageChunk` notifications; collect it
        // in the notification handler (the pattern the crate's own example uses).
        let buf = Arc::new(Mutex::new(String::new()));
        let sink = buf.clone();

        let stop = Client
            .builder()
            .on_receive_notification(
                async move |n: SessionNotification, _cx| {
                    if let SessionUpdate::AgentMessageChunk(ContentChunk {
                        content: ContentBlock::Text(text),
                        ..
                    }) = n.update
                    {
                        eprintln!("[acp] message chunk (+{} chars)", text.text.len());
                        sink.lock().unwrap().push_str(&text.text);
                    }
                    Ok(())
                },
                acp::on_receive_notification!(),
            )
            // One-shot: auto-approve the first permission option, no UI.
            .on_receive_request(
                async move |request: RequestPermissionRequest, responder, _connection| {
                    match request.options.first().map(|opt| opt.option_id.clone()) {
                        Some(id) => responder.respond(RequestPermissionResponse::new(
                            RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(id)),
                        )),
                        None => responder.respond(RequestPermissionResponse::new(
                            RequestPermissionOutcome::Cancelled,
                        )),
                    }
                },
                acp::on_receive_request!(),
            )
            .connect_with(agent, |connection: ConnectionTo<Agent>| async move {
                connection
                    .send_request(InitializeRequest::new(ProtocolVersion::V1))
                    .block_task()
                    .await?;
                // Isolate the agent from Manch's own working dir: hand Claude Code a
                // dedicated, empty workspace so it does not read this project's files.
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

        let text = buf.lock().unwrap().clone();
        if text.trim().is_empty() {
            return Err(format!("claude-code returned no text (stop reason: {stop:?})"));
        }
        Ok(text)
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
}
