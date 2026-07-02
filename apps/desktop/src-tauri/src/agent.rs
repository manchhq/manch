//! Desktop glue: map `manch_protocol::AgentEvent` → `manch_dto::StreamEvent`,
//! and resolve a provider id to a concrete agent. All provider logic now lives
//! in `manch-llm` (BYOK) and `manch-acp` (CLI).

use async_trait::async_trait;
use manch_acp::tool_status;
use manch_dto::StreamEvent;
use manch_protocol::acp::{ContentBlock, SessionUpdate};
use manch_protocol::{AgentEvent, EventSink};
use tauri::ipc::Channel;

use crate::db::Db;

/// Provider ids the desktop understands (BYOK + CLI).
pub const BYOK: [&str; 3] = ["anthropic", "gemini", "openai"];
pub const CLI: [&str; 3] = ["claude-code", "gemini-cli", "codex"];

pub fn is_known_provider(id: &str) -> bool {
    BYOK.contains(&id) || CLI.contains(&id)
}

/// Providers offerable in the UI: every saved one, plus the always-available
/// BYOC CLIs (they bring their own auth).
pub fn offerable_providers(mut saved: Vec<String>) -> Vec<String> {
    for cli in CLI {
        if !saved.iter().any(|p| p == cli) {
            saved.push(cli.to_string());
        }
    }
    saved.sort();
    saved.dedup();
    saved
}

/// `EventSink` that maps ACP events to `StreamEvent` and forwards them over a
/// Tauri IPC channel. `emitted` gates nothing here — the agent decides Done/Err.
pub struct ChannelSink(pub Channel<StreamEvent>);

impl ChannelSink {
    pub fn send_error(&self, message: String) {
        let _ = self.0.send(StreamEvent::Error { message });
    }
}

#[async_trait]
impl EventSink for ChannelSink {
    async fn emit(&self, event: AgentEvent) -> manch_protocol::Result<()> {
        match event {
            AgentEvent::Update(SessionUpdate::AgentMessageChunk(chunk)) => {
                if let ContentBlock::Text(t) = chunk.content {
                    let _ = self.0.send(StreamEvent::Token { text: t.text });
                }
            }
            AgentEvent::Update(SessionUpdate::ToolCall(tc)) => {
                let _ = self.0.send(StreamEvent::Tool {
                    id: tc.tool_call_id.0.to_string(),
                    name: tc.title,
                    status: tool_status(tc.status).into(),
                    detail: None,
                });
            }
            AgentEvent::Update(SessionUpdate::ToolCallUpdate(u)) => {
                let _ = self.0.send(StreamEvent::Tool {
                    id: u.tool_call_id.0.to_string(),
                    name: u.fields.title.unwrap_or_default(),
                    status: u.fields.status.map(tool_status).unwrap_or("running").into(),
                    detail: None,
                });
            }
            AgentEvent::Done(_) => {
                let _ = self.0.send(StreamEvent::Done);
            }
            _ => {}
        }
        Ok(())
    }
}

/// Resolve a provider id to a concrete agent, pulling keys/model from the DB.
pub fn resolve_agent(provider: &str, db: &Db) -> Result<Box<dyn manch_protocol::Agent>, String> {
    let byok = |p: &str| -> Result<(String, Option<String>), String> {
        let key = db
            .get_key(p)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("no API key saved for {p}"))?;
        let model = db.get_model(p).map_err(|e| e.to_string())?;
        Ok((key, model))
    };
    match provider {
        "anthropic" => {
            let (k, m) = byok("anthropic")?;
            Ok(Box::new(manch_llm::AnthropicAgent::new(k, m)))
        }
        "gemini" => {
            let (k, m) = byok("gemini")?;
            Ok(Box::new(manch_llm::GeminiAgent::new(k, m)))
        }
        "openai" => {
            let (k, m) = byok("openai")?;
            Ok(Box::new(manch_llm::OpenAiAgent::new(k, m)))
        }
        "claude-code" => Ok(Box::new(manch_acp::claude_code(
            db.get_key("claude-code").map_err(|e| e.to_string())?,
        ))),
        "gemini-cli" => Ok(Box::new(manch_acp::gemini_cli(
            db.get_key("gemini-cli").map_err(|e| e.to_string())?,
        ))),
        "codex" => Ok(Box::new(manch_acp::codex(
            db.get_key("codex").map_err(|e| e.to_string())?,
        ))),
        _ => Err(format!("unknown provider: {provider}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_agents_always_offered() {
        let out = offerable_providers(vec!["anthropic".into()]);
        assert!(out.contains(&"anthropic".to_string()));
        assert!(out.contains(&"claude-code".to_string()));
        assert!(out.contains(&"codex".to_string()));
        assert!(out.contains(&"gemini-cli".to_string()));
    }

    #[test]
    fn known_providers() {
        assert!(is_known_provider("gemini"));
        assert!(is_known_provider("codex"));
        assert!(!is_known_provider("nope"));
    }
}
