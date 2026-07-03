//! Framework-agnostic ACP host — one generic subprocess agent parameterized by a launch spec.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use agent_client_protocol::schema::v1::ToolCallStatus;
use async_trait::async_trait;
use manch_protocol::acp::{SessionUpdate, StopReason};
use manch_protocol::{Agent, AgentEvent, Context, EventSink, Result, ToolSchema};

const CLAUDE_CODE_PKG: &str = "@agentclientprotocol/claude-agent-acp@latest";
const CODEX_PKG: &str = "@zed-industries/codex-acp";
const GEMINI_CLI_PKG: &str = "@google/gemini-cli";

/// A per-CLI subprocess launch recipe. `args` is the launch command; `key_env`,
/// when set and given a key, becomes a leading `NAME=value` subprocess env var.
pub struct LaunchSpec {
    pub args: Vec<String>,
    pub key_env: Option<&'static str>,
}

/// Wraps an external ACP agent (subprocess) as a `manch_protocol::Agent`.
pub struct AcpCliAgent {
    id: &'static str,
    api_key: Option<String>,
    pub spec: LaunchSpec,
}

impl AcpCliAgent {
    pub fn new(id: &'static str, api_key: Option<String>, spec: LaunchSpec) -> Self {
        Self { id, api_key, spec }
    }

    /// Full argv passed to the ACP host: a leading `NAME=value` env token (only
    /// when this agent takes a key override AND one was supplied), then the
    /// launch command.
    pub(crate) fn argv(&self) -> Vec<String> {
        let mut argv = Vec::new();
        if let (Some(env), Some(key)) = (self.spec.key_env, self.api_key.as_deref()) {
            argv.push(format!("{env}={key}"));
        }
        argv.extend(self.spec.args.iter().cloned());
        argv
    }
}

pub fn claude_code(api_key: Option<String>) -> AcpCliAgent {
    AcpCliAgent::new(
        "claude-code",
        api_key,
        LaunchSpec {
            args: vec!["npx".into(), "-y".into(), CLAUDE_CODE_PKG.into()],
            key_env: Some("ANTHROPIC_API_KEY"),
        },
    )
}

pub fn codex(api_key: Option<String>) -> AcpCliAgent {
    AcpCliAgent::new(
        "codex",
        api_key,
        LaunchSpec {
            args: vec!["npx".into(), "-y".into(), CODEX_PKG.into()],
            key_env: Some("OPENAI_API_KEY"),
        },
    )
}

pub fn gemini_cli(api_key: Option<String>) -> AcpCliAgent {
    AcpCliAgent::new(
        "gemini-cli",
        api_key,
        LaunchSpec {
            args: vec![
                "npx".into(),
                "-y".into(),
                GEMINI_CLI_PKG.into(),
                "--experimental-acp".into(),
            ],
            key_env: Some("GEMINI_API_KEY"),
        },
    )
}

/// Merge one streamed chunk into `buf`, returning the newly-added text (`None`
/// if nothing new). Tolerates pure deltas, cumulative snapshots, and trailing
/// full-message repeats so we never double-emit. Pure.
pub(crate) fn push_chunk(buf: &mut String, chunk: &str) -> Option<String> {
    if chunk.is_empty() {
        None
    } else if buf.is_empty() {
        buf.push_str(chunk);
        Some(chunk.to_string())
    } else if chunk.starts_with(buf.as_str()) {
        let delta = chunk[buf.len()..].to_string();
        *buf = chunk.to_string();
        (!delta.is_empty()).then_some(delta)
    } else if buf.ends_with(chunk) {
        None
    } else {
        buf.push_str(chunk);
        Some(chunk.to_string())
    }
}

/// Map an ACP tool-call status onto the `running|done|error` vocabulary.
pub fn tool_status(status: ToolCallStatus) -> &'static str {
    match status {
        ToolCallStatus::Completed => "done",
        ToolCallStatus::Failed => "error",
        _ => "running",
    }
}

#[async_trait]
impl Agent for AcpCliAgent {
    fn id(&self) -> &str {
        self.id
    }

    async fn prompt(
        &self,
        ctx: Context,
        _tools: &[ToolSchema],
        sink: Arc<dyn EventSink>,
    ) -> Result<StopReason> {
        use std::collections::HashMap;

        use agent_client_protocol::schema::ProtocolVersion;
        use agent_client_protocol::schema::v1::{
            ContentBlock, ContentChunk, InitializeRequest, NewSessionRequest, PromptRequest,
            RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
            SelectedPermissionOutcome, SessionNotification,
        };
        use agent_client_protocol::{self as acp, AcpAgent, Client, ConnectionTo};

        let agent = AcpAgent::from_args(self.argv()).map_err(err)?;
        // Isolate each session's ACP workspace by session id (was a single shared
        // temp dir). `ctx` is owned, so the blocks move rather than clone.
        let cwd = std::env::temp_dir().join(format!("manch-acp-{}", ctx.session_id));
        let blocks = ctx.blocks;
        let id = self.id;

        // The 'static notification handler owns a clone of the sink and emits
        // live as events arrive — no post-turn buffering, so partial text
        // survives a mid-turn error. `emitted` preserves the "no text" error.
        // Mutex guards below are scoped and dropped *before* every `.await`
        // (a std Mutex guard is not Send and must not cross an await point).
        let text_buf = Arc::new(Mutex::new(String::new()));
        let names: Arc<Mutex<HashMap<String, String>>> = Arc::new(Mutex::new(HashMap::new()));
        let emitted = Arc::new(AtomicBool::new(false));
        let (hsink, htext, hnames, hemitted) = (
            sink.clone(),
            text_buf.clone(),
            names.clone(),
            emitted.clone(),
        );

        let stop = Client
            .builder()
            .on_receive_notification(
                async move |n: SessionNotification, _cx| {
                    match n.update {
                        SessionUpdate::AgentMessageChunk(ContentChunk {
                            content: ContentBlock::Text(t),
                            ..
                        }) => {
                            let delta = push_chunk(&mut htext.lock().unwrap(), &t.text);
                            if let Some(delta) = delta {
                                hemitted.store(true, Ordering::Relaxed);
                                let _ = hsink.emit(AgentEvent::text_chunk(delta)).await;
                            }
                        }
                        SessionUpdate::ToolCall(tc) => {
                            hnames
                                .lock()
                                .unwrap()
                                .insert(tc.tool_call_id.0.to_string(), tc.title.clone());
                            hemitted.store(true, Ordering::Relaxed);
                            let _ = hsink
                                .emit(AgentEvent::Update(SessionUpdate::ToolCall(tc)))
                                .await;
                        }
                        SessionUpdate::ToolCallUpdate(mut u) => {
                            if u.fields.title.is_none() {
                                u.fields.title = hnames
                                    .lock()
                                    .unwrap()
                                    .get(&u.tool_call_id.0.to_string())
                                    .cloned();
                            }
                            hemitted.store(true, Ordering::Relaxed);
                            let _ = hsink
                                .emit(AgentEvent::Update(SessionUpdate::ToolCallUpdate(u)))
                                .await;
                        }
                        _ => {}
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
            .connect_with(agent, |connection: ConnectionTo<acp::Agent>| async move {
                connection
                    .send_request(InitializeRequest::new(ProtocolVersion::V1))
                    .block_task()
                    .await?;
                std::fs::create_dir_all(&cwd).ok();
                let session = connection
                    .send_request(NewSessionRequest::new(cwd))
                    .block_task()
                    .await?;
                let response = connection
                    .send_request(PromptRequest::new(session.session_id, blocks))
                    .block_task()
                    .await?;
                Ok(response.stop_reason)
            })
            .await
            .map_err(err)?;

        if emitted.load(Ordering::Relaxed) {
            sink.emit(AgentEvent::Done(stop)).await?;
            Ok(stop)
        } else {
            Err(manch_protocol::Error::Other(format!(
                "{id} returned no text (stop reason: {stop:?})"
            )))
        }
    }
}

fn err(e: impl ToString) -> manch_protocol::Error {
    manch_protocol::Error::Other(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_code_without_key_is_just_npx() {
        let s = claude_code(None).spec;
        assert_eq!(s.args[0], "npx");
        assert!(s.args.iter().any(|a| a.contains("claude-agent-acp")));
    }

    #[test]
    fn codex_launches_zed_adapter() {
        let s = codex(None).spec;
        assert_eq!(s.args[0], "npx");
        assert!(
            s.args
                .iter()
                .any(|a| a.contains("@zed-industries/codex-acp"))
        );
        assert_eq!(s.key_env, Some("OPENAI_API_KEY"));
    }

    #[test]
    fn gemini_cli_passes_experimental_acp() {
        let s = gemini_cli(None).spec;
        assert!(s.args.iter().any(|a| a == "--experimental-acp"));
        assert_eq!(s.key_env, Some("GEMINI_API_KEY"));
    }

    #[test]
    fn launch_argv_prepends_env_when_key_present() {
        let agent = claude_code(Some("sk-test".into()));
        let argv = agent.argv();
        assert_eq!(argv[0], "ANTHROPIC_API_KEY=sk-test");
    }

    #[test]
    fn push_chunk_returns_only_cumulative_delta() {
        let mut b = String::new();
        push_chunk(&mut b, "New");
        assert_eq!(
            push_chunk(&mut b, "New Delhi."),
            Some(" Delhi.".to_string())
        );
    }

    #[test]
    fn tool_status_maps_acp_vocabulary() {
        use agent_client_protocol::schema::v1::ToolCallStatus;
        assert_eq!(tool_status(ToolCallStatus::Completed), "done");
        assert_eq!(tool_status(ToolCallStatus::Failed), "error");
        assert_eq!(tool_status(ToolCallStatus::InProgress), "running");
    }
}
