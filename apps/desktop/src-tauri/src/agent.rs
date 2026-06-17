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

/// Spawn args for the Claude Code ACP adapter. Leading `NAME=value` tokens are
/// env vars; then the launch command. Pure.
fn claude_code_args(api_key: &str) -> Vec<String> {
    vec![
        format!("ANTHROPIC_API_KEY={api_key}"),
        "npx".into(),
        "-y".into(),
        CLAUDE_CODE_PKG.into(),
    ]
}

/// Key for the claude-code path: its own saved key, else the anthropic key. Pure.
pub fn claude_code_key(own: Option<String>, anthropic: Option<String>) -> Option<String> {
    own.or(anthropic)
}

/// Providers offerable in the UI: every saved one, plus `claude-code` whenever
/// `anthropic` is present (it reuses the anthropic key). Pure.
pub fn offerable_providers(mut saved: Vec<String>) -> Vec<String> {
    if saved.iter().any(|p| p == "anthropic") && !saved.iter().any(|p| p == "claude-code") {
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
    api_key: String,
}

impl ClaudeCodeAgent {
    pub fn new(api_key: String) -> Self {
        Self { api_key }
    }
}

#[async_trait]
impl ChatAgent for ClaudeCodeAgent {
    async fn ask(&self, _prompt: &str) -> Result<String, String> {
        let _ = &self.api_key; // used in Task 3
        let _ = claude_code_args; // referenced in Task 3
        Err("claude-code path not wired yet".to_string())
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
    fn claude_code_args_include_key_and_npx() {
        let args = claude_code_args("sk-test");
        assert_eq!(args[0], "ANTHROPIC_API_KEY=sk-test");
        assert!(args.iter().any(|a| a == "npx"));
        assert!(args.iter().any(|a| a.contains("claude-agent-acp")));
    }

    #[test]
    fn claude_code_key_prefers_own_then_anthropic() {
        assert_eq!(
            claude_code_key(Some("own".into()), Some("ant".into())),
            Some("own".into())
        );
        assert_eq!(claude_code_key(None, Some("ant".into())), Some("ant".into()));
        assert_eq!(claude_code_key(None, None), None);
    }

    #[test]
    fn offers_claude_code_when_anthropic_present() {
        assert_eq!(
            offerable_providers(vec!["anthropic".into()]),
            vec!["anthropic".to_string(), "claude-code".to_string()]
        );
        assert_eq!(offerable_providers(vec![]), Vec::<String>::new());
        assert_eq!(
            offerable_providers(vec!["claude-code".into()]),
            vec!["claude-code".to_string()]
        );
    }
}
