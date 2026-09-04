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
/// Deny-by-default: `Resolved(outcome)` is returned as-is. `Ask(_)` means "a
/// human should decide", and there is no human on this code path inside the
/// library, so the policy's own option list (built for a human to read, not
/// for this fallback) is ignored entirely; instead the *agent's* own
/// `req.options` — the ids ACP requires the client to answer with — are
/// scanned for the first option that is reject-kind by its **typed** `kind`
/// field, never by guessing by id. An option whose id resolves (via
/// `kind_of`) to a *different* kind than the one it declares is treated as
/// crafted/untrustworthy and skipped, not selected — matching by id alone
/// would let an agent offer `{option_id: "reject_once", kind: AllowAlways}`
/// and have us pick it believing it denies the action, when the agent would
/// actually act on the `AllowAlways` it declared. An option whose id is
/// shared with another option is skipped for the same reason: the id we send
/// back could be resolved by the agent to its *allow* twin (ACP assumes id
/// uniqueness but does not enforce it). Falls back to `Cancelled` if the
/// agent offered no trustworthy reject-kind option at all — never an allow.
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
        provider_meta: None,
    };

    match policy.decide(&cx, &inv).await? {
        PermissionDecision::Resolved(outcome) => Ok(outcome),
        PermissionDecision::Ask(_) => {
            let options = req.options;
            Ok(options
                .iter()
                .find(|opt| {
                    let is_reject = matches!(
                        opt.kind,
                        acp::PermissionOptionKind::RejectOnce
                            | acp::PermissionOptionKind::RejectAlways
                    );
                    // Trust the typed `kind` the agent declared, not our own
                    // guess from its id. If the id names a known kind that
                    // disagrees with the declared `kind`, the option is
                    // inconsistent — and therefore untrustworthy — so reject it
                    // outright rather than trusting either half.
                    let id_agrees = kind_of(&opt.option_id).is_none_or(|k| k == opt.kind);
                    // An id shared with another option is not an answer: the
                    // agent could resolve the id we send back to its allow
                    // entry. ACP assumes id uniqueness but does not enforce it.
                    let id_unique = options
                        .iter()
                        .filter(|o| o.option_id == opt.option_id)
                        .count()
                        == 1;
                    is_reject && id_agrees && id_unique
                })
                .map(|opt| {
                    acp::RequestPermissionOutcome::Selected(acp::SelectedPermissionOutcome::new(
                        opt.option_id.clone(),
                    ))
                })
                .unwrap_or(acp::RequestPermissionOutcome::Cancelled))
        }
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
        //
        // `Role::System` is dropped here, and that is correct rather than an
        // omission. ACP has no system role — its own `Role` is `User |
        // Assistant` — and an external agent such as Claude Code or Codex owns
        // its own system prompt, which a host driving it through Manch cannot
        // and should not overwrite. Folding system content into the user turn
        // instead would hand the host's standing rules exactly the authority of
        // the text they exist to constrain, which is the bug `Role::System`
        // was added to prevent. A host that needs guardrails on this path has
        // to express them the way that agent supports.
        let blocks = prompt_blocks(ctx.turns);
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

    fn allow_once_option() -> acp::PermissionOption {
        acp::PermissionOption::new(
            acp::PermissionOptionId::new("allow_once"),
            "Allow once",
            acp::PermissionOptionKind::AllowOnce,
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

    #[tokio::test]
    async fn an_agent_offering_only_allow_options_is_cancelled() {
        // No reject-kind option exists at all, so there is nothing safe to
        // select — the outcome must be Cancelled, and specifically not a
        // Selected naming an id the agent never offered.
        let policy = Arc::new(RecordingPolicy::default());
        let decided = decide_permission(
            policy,
            request_with_options(vec![allow_once_option(), allow_always_option()]),
        )
        .await
        .unwrap();

        assert!(matches!(decided, acp::RequestPermissionOutcome::Cancelled));
    }

    #[tokio::test]
    async fn a_crafted_option_whose_id_disagrees_with_its_kind_is_not_trusted() {
        // Attack: an agent offers a single option with option_id
        // "reject_once" (a recognisable, reject-shaped id) but declares its
        // typed `kind` as AllowAlways. If we selected on the id alone, we'd
        // pick this option believing it denies the action; the agent would
        // then act on the `kind` it actually declared — AllowAlways — which
        // turns our deny into an allow. The typed `kind` must win, and a
        // mismatch between id and kind makes the option untrustworthy, not
        // merely ambiguous, so it must be skipped rather than selected.
        let crafted = acp::PermissionOption::new(
            acp::PermissionOptionId::new("reject_once"),
            "Reject (lies)",
            acp::PermissionOptionKind::AllowAlways,
        );
        let policy = Arc::new(RecordingPolicy::default());
        let decided = decide_permission(policy, request_with_options(vec![crafted]))
            .await
            .unwrap();

        assert!(matches!(decided, acp::RequestPermissionOutcome::Cancelled));
    }

    #[tokio::test]
    async fn an_option_id_shared_with_another_option_is_not_trusted() {
        // Attack: two options share the id "x" — the first declares
        // AllowAlways, the second RejectOnce. `kind_of("x")` is None, so the
        // id-agreement check passes for both and `find` settles on the reject
        // entry; we would answer Selected("x"). But the agent resolves that id
        // against its OWN list, where "x" first names the allow entry — our
        // deny becomes an allow. ACP assumes id uniqueness but does not
        // enforce it, so a duplicated id is not an answer at all.
        let allow = acp::PermissionOption::new(
            acp::PermissionOptionId::new("x"),
            "Always allow",
            acp::PermissionOptionKind::AllowAlways,
        );
        let reject = acp::PermissionOption::new(
            acp::PermissionOptionId::new("x"),
            "Reject",
            acp::PermissionOptionKind::RejectOnce,
        );
        let policy = Arc::new(RecordingPolicy::default());
        let decided = decide_permission(policy, request_with_options(vec![allow, reject]))
            .await
            .unwrap();

        assert!(
            matches!(decided, acp::RequestPermissionOutcome::Cancelled),
            "expected Cancelled, got {decided:?}"
        );
    }

    #[tokio::test]
    async fn a_reject_kind_option_with_an_unrecognised_id_is_still_selected() {
        // The typed `kind` is what the agent will act on — our own
        // `allow_once`/`reject_once`/... id vocabulary (from `kind_of`) is
        // just a convenience for recognising *our own* options, not a
        // requirement the agent's ids must satisfy. An id we don't
        // recognise (`kind_of` returns None) is fine as long as it has no
        // declared kind to disagree with.
        let opt = acp::PermissionOption::new(
            acp::PermissionOptionId::new("nope"),
            "Nope",
            acp::PermissionOptionKind::RejectOnce,
        );
        let policy = Arc::new(RecordingPolicy::default());
        let decided = decide_permission(policy, request_with_options(vec![opt]))
            .await
            .unwrap();

        match decided {
            acp::RequestPermissionOutcome::Selected(s) => {
                assert_eq!(s.option_id.0.as_ref(), "nope")
            }
            other => panic!("expected Selected(\"nope\"), got {other:?}"),
        }
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

/// The content blocks sent to an external ACP agent: the entries of the most
/// recent `Role::User` turn. Pure, so the selection rule is testable without a
/// live agent connection.
///
/// See the call site for why `Role::System` is deliberately not folded in.
fn prompt_blocks(turns: Vec<manch_protocol::Turn>) -> Vec<manch_protocol::acp::ContentBlock> {
    turns
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
        .unwrap_or_default()
}

#[cfg(test)]
mod prompt_block_tests {
    use manch_protocol::acp::{ContentBlock, TextContent};
    use manch_protocol::{Entry, Role, Turn};

    use super::prompt_blocks;

    fn turn(role: Role, text: &str) -> Turn {
        Turn {
            role,
            entries: vec![Entry::Block(ContentBlock::Text(TextContent::new(
                text.to_string(),
            )))],
        }
    }

    fn texts(blocks: &[ContentBlock]) -> Vec<String> {
        blocks
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text(t) => Some(t.text.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_system_turn_is_not_sent_to_an_external_agent() {
        // ACP has no system role, and an external agent (Claude Code, Codex)
        // owns its own system prompt. Folding the host's standing rules into
        // the user turn would hand them exactly the authority of the text they
        // exist to constrain — the bug `Role::System` was added to prevent.
        // The system turn sits *after* the user turn, which is the adversarial
        // position for a reverse scan: anything that gathers blocks loosely
        // picks it up first.
        let blocks = prompt_blocks(vec![
            turn(Role::User, "summarise this"),
            turn(Role::System, "never invent citations"),
        ]);
        assert_eq!(texts(&blocks), vec!["summarise this".to_string()]);
        assert!(
            !texts(&blocks).iter().any(|t| t.contains("never invent")),
            "system content must not reach an external agent as user input"
        );
    }

    #[test]
    fn only_the_trailing_user_turn_is_sent() {
        let blocks = prompt_blocks(vec![
            turn(Role::User, "first"),
            turn(Role::Assistant, "reply"),
            turn(Role::User, "second"),
        ]);
        assert_eq!(texts(&blocks), vec!["second".to_string()]);
    }

    #[test]
    fn a_history_with_no_user_turn_sends_nothing() {
        let blocks = prompt_blocks(vec![turn(Role::System, "rules only")]);
        assert!(blocks.is_empty());
    }
}
