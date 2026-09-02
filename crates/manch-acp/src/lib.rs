//! Framework-agnostic ACP host — one generic subprocess agent parameterized by a launch spec.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use agent_client_protocol::schema::v1::ToolCallStatus;
use async_trait::async_trait;
use manch_protocol::acp::{self, SessionUpdate, StopReason};
use manch_protocol::{
    Agent, AgentEvent, AskOncePolicy, Context, EventSink, Extensions, PermissionDecision,
    PermissionPolicy, Result, Role, ToolContext, ToolInvocation, ToolSchema, kind_of,
};

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
    policy: Arc<dyn PermissionPolicy>,
}

impl AcpCliAgent {
    /// Defaults to [`AskOncePolicy`] — deny-by-default until a consumer opts
    /// into its own [`PermissionPolicy`] via [`Self::with_policy`].
    pub fn new(id: &'static str, api_key: Option<String>, spec: LaunchSpec) -> Self {
        Self {
            id,
            api_key,
            spec,
            policy: Arc::new(AskOncePolicy),
        }
    }

    /// Overrides the default [`AskOncePolicy`] with a consumer-supplied
    /// [`PermissionPolicy`] (e.g. one backed by a remembered-decision store).
    pub fn with_policy(mut self, policy: Arc<dyn PermissionPolicy>) -> Self {
        self.policy = policy;
        self
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

/// Decides the outcome of an incoming `session/request_permission` from an
/// external ACP agent, via `policy`.
///
/// The ACP path has no `ToolInvocation` of its own: Manch is the ACP
/// *client* here, and the external agent owns and dispatches its own tools
/// — nothing on this path ever looks a tool up by name. So this synthesises
/// a `ToolInvocation` purely so `policy` has something to inspect: `id` is
/// the agent's `tool_call_id`, and `name` is the agent's own display title
/// for the call (`tool_call.fields.title`). That title is an arbitrary
/// string the *agent* chose to show a human, not a registry key — do not
/// mistake it for a dispatch name.
///
/// Deny-by-default: `Resolved(outcome)` is returned as-is, but `Ask(options)`
/// means "a human should decide", and there is no human on this code path
/// inside the library, so the first reject-kind option is selected — never
/// an allow — falling back to `Cancelled` if the agent offered no
/// reject-kind option at all.
pub(crate) async fn decide_permission(
    policy: Arc<dyn PermissionPolicy>,
    req: acp::RequestPermissionRequest,
) -> Result<acp::RequestPermissionOutcome> {
    let session_id = req.session_id.0.to_string();
    let tool_call_id = req.tool_call.tool_call_id.0.to_string();
    let cx = ToolContext::new(
        session_id,
        tool_call_id.clone(),
        Arc::new(Extensions::default()),
    );
    let inv = ToolInvocation {
        id: tool_call_id,
        name: req.tool_call.fields.title.clone().unwrap_or_default(),
        arguments: serde_json::Value::Null,
    };

    match policy.decide(&cx, &inv).await? {
        PermissionDecision::Resolved(outcome) => Ok(outcome),
        PermissionDecision::Ask(options) => Ok(options
            .into_iter()
            .find(|opt| {
                matches!(
                    kind_of(&opt.option_id),
                    Some(acp::PermissionOptionKind::RejectOnce)
                        | Some(acp::PermissionOptionKind::RejectAlways)
                )
            })
            .map(|opt| {
                acp::RequestPermissionOutcome::Selected(acp::SelectedPermissionOutcome::new(
                    opt.option_id,
                ))
            })
            .unwrap_or(acp::RequestPermissionOutcome::Cancelled)),
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
            SessionNotification,
        };
        use agent_client_protocol::{self as acp, AcpAgent, Client, ConnectionTo};

        let agent = AcpAgent::from_args(self.argv()).map_err(err)?;
        // Isolate each session's ACP workspace by session id (was a single shared
        // temp dir). `ctx` is owned, so the blocks move rather than clone.
        let cwd = std::env::temp_dir().join(format!("manch-acp-{}", ctx.session_id));
        // ACP's PromptRequest is role-less and the external agent owns its own
        // in-session history, so send only the current (trailing) user turn —
        // never feed the agent its own prior assistant replies as user input.
        // Single-turn (#5) has exactly one user turn, so this is unchanged there.
        // Tool calls/results have no slot in ACP's role-less ContentBlock
        // vocabulary and don't apply on this path anyway (host tools are
        // BYOK-only — see crate docs), so only `Entry::Block` entries are sent.
        let blocks = ctx
            .turns
            .into_iter()
            .rev()
            .find(|t| t.role == Role::User)
            .map(|t| {
                t.entries
                    .into_iter()
                    .filter_map(|e| match e {
                        manch_protocol::Entry::Block(b) => Some(b),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let id = self.id;
        let policy = self.policy.clone();

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
                async move |request: RequestPermissionRequest, responder, _connection| {
                    // Deny-by-default: any failure to decide (including the
                    // policy itself erroring) falls back to `Cancelled`,
                    // never to an allow.
                    let outcome = decide_permission(policy.clone(), request)
                        .await
                        .unwrap_or(RequestPermissionOutcome::Cancelled);
                    responder.respond(RequestPermissionResponse::new(outcome))
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

    /// Records the `name` of every `ToolInvocation` it is asked to decide,
    /// and always defers to a human (`Ask`) — never resolves on its own.
    #[derive(Default)]
    struct RecordingPolicy {
        seen: Mutex<Vec<String>>,
    }

    impl RecordingPolicy {
        fn seen(&self) -> Vec<String> {
            self.seen.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl PermissionPolicy for RecordingPolicy {
        async fn decide(
            &self,
            _cx: &ToolContext,
            inv: &ToolInvocation,
        ) -> Result<PermissionDecision> {
            self.seen.lock().unwrap().push(inv.name.clone());
            Ok(PermissionDecision::Ask(manch_protocol::once_options()))
        }
    }

    fn allow_always_option() -> acp::PermissionOption {
        acp::PermissionOption::new(
            acp::PermissionOptionId::new("allow_always"),
            "Always allow",
            acp::PermissionOptionKind::AllowAlways,
        )
    }

    fn reject_once_option() -> acp::PermissionOption {
        acp::PermissionOption::new(
            acp::PermissionOptionId::new("reject_once"),
            "Reject",
            acp::PermissionOptionKind::RejectOnce,
        )
    }

    fn request_with_options(opts: Vec<acp::PermissionOption>) -> acp::RequestPermissionRequest {
        acp::RequestPermissionRequest::new(
            acp::SessionId::new("s1"),
            acp::ToolCallUpdate::new(
                acp::ToolCallId::new("tc1"),
                acp::ToolCallUpdateFields::new().title(Some("Edit file /etc/hosts".to_string())),
            ),
            opts,
        )
    }

    #[tokio::test]
    async fn the_acp_handler_does_not_auto_approve_by_default() {
        // A policy that records what it was asked, and refuses.
        let policy = Arc::new(RecordingPolicy::default());
        let decided = decide_permission(
            policy.clone(),
            request_with_options(vec![allow_always_option(), reject_once_option()]),
        )
        .await
        .unwrap();

        assert_eq!(policy.seen(), vec!["Edit file /etc/hosts".to_string()]);
        match decided {
            acp::RequestPermissionOutcome::Selected(s) => {
                assert_eq!(s.option_id.0.as_ref(), "reject_once")
            }
            acp::RequestPermissionOutcome::Cancelled => {
                panic!("expected a decision, not a cancellation")
            }
            _ => panic!("unexpected RequestPermissionOutcome variant"),
        }
        // The old behaviour would have selected options.first() — allow_always.
    }

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
