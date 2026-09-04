//! Tier resolution and batch buffering for the tool loop.
//!
//! Two rules live here, and both exist because a tool call is not a pure
//! function of the model's output:
//!
//! 1. **Tier gates dispatch.** A [`Tier::Read`] call executes when the model
//!    asks. A [`Tier::Draft`] call consults the runtime's
//!    [`PermissionPolicy`](manch_protocol::PermissionPolicy) first, and
//!    suspends the turn when that policy wants a human.
//! 2. **A batch is all-or-nothing.** Results are held in a [`Buffer`] and
//!    appended only once the whole batch has resolved. [`MemoryStore`] is
//!    append-only with no compensating delete, so atomicity has to come from
//!    not writing yet rather than from rolling back.

use std::sync::Arc;

use manch_protocol::{
    AgentEvent, Entry, Error, EventSink, Extensions, MemoryStore, PermissionDecision, Result, Role,
    Tier, Tool, ToolContext, ToolInvocation, acp, kind_of,
};

use crate::Manch;

/// Entries produced by one batch of tool calls, withheld from the
/// [`MemoryStore`] until the batch's fate is known.
///
/// A batch resolves one of three ways: it completes (flush), it suspends
/// (flush — the calls that already ran have final results), or a call errors
/// (drop, so a mid-batch failure leaves no partial record).
#[derive(Default)]
/// Buffers one batch's calls and results separately, so a flush writes every
/// call before any result.
pub(crate) struct Buffer {
    calls: Vec<Entry>,
    results: Vec<Entry>,
}

impl Buffer {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Record a dispatched call and the content it produced.
    ///
    /// Calls and results are kept in separate lists and written calls-first on
    /// [`Buffer::flush`], for two reasons that both matter.
    ///
    /// Every call precedes every result, because Anthropic rejects a
    /// `tool_result` with no preceding `tool_use` — stored history would be
    /// invalid input for the next loop iteration otherwise.
    ///
    /// And the calls of one batch stay *adjacent*, so `coalesce_turns` folds
    /// them into a single assistant turn with their results in a single user
    /// turn. Interleaving them per invocation produced four alternating turns
    /// for a two-call batch, which misrepresents a parallel batch as a sequence
    /// — and Gemini rejects it outright, because it attaches a thought
    /// signature to only the *first* call of a batch, so a lone second call in
    /// a turn of its own has none.
    pub(crate) fn record(&mut self, inv: &ToolInvocation, content: Vec<acp::ToolCallContent>) {
        self.calls.push(Entry::ToolCall(inv.clone()));
        self.results.push(Entry::ToolResult {
            id: inv.id.clone(),
            content,
        });
    }

    /// Append everything buffered, in order. Consumes the buffer, so it cannot
    /// be flushed twice.
    pub(crate) async fn flush(self, memory: &dyn MemoryStore, session_id: &str) -> Result<()> {
        for entry in self.calls {
            memory.append(session_id, Role::Assistant, entry).await?;
        }
        for entry in self.results {
            memory.append(session_id, Role::User, entry).await?;
        }
        Ok(())
    }
}

/// How a batch of invocations resolved.
pub(crate) enum Batch {
    /// Every invocation ran (or was refused by a policy that had already
    /// decided). The loop continues.
    Completed,
    /// A `Draft` invocation needs a human. Invocations queued *after* it were
    /// dropped, not run: the model re-decides them on resume, when it can see
    /// the approved result. Silently running them after a human paused the turn
    /// would defeat the pause.
    /// Boxed because this variant dwarfs the other two — and unlike
    /// `TurnOutcome`, `Batch` is crate-internal, so the indirection ripples
    /// nowhere.
    Suspended {
        request: Box<acp::RequestPermissionRequest>,
        pending: ToolInvocation,
    },
    /// A decision cancelled the turn.
    Cancelled,
}

/// How a single permission outcome was applied.
pub(crate) enum Applied {
    /// An allow-kind outcome; the tool ran.
    Ran,
    /// A reject-kind outcome; a refusal was recorded for the model to read.
    Refused,
    /// The turn was cancelled before the human answered.
    Cancelled,
}

/// The `ToolResult` content recorded when a call is refused.
///
/// The model must see *something* addressed to the call id — a `tool_use` with
/// no matching `tool_result` is invalid history — and it must read as a
/// refusal, not as a tool that ran and returned nothing.
fn refusal() -> acp::ToolCallContent {
    acp::ToolCallContent::Content(acp::Content::new(acp::ContentBlock::Text(
        acp::TextContent::new("The user rejected this tool call.".to_string()),
    )))
}

/// The `ToolResult` content recorded when a call could not succeed.
///
/// Same invariant as [`refusal`] — a `tool_use` with no matching `tool_result`
/// is invalid history, so the model must see *something* addressed to the call
/// id. Unlike a refusal, this is also the model's only chance to learn *why*
/// and try again, so the reason travels with it.
fn failure(reason: &str) -> acp::ToolCallContent {
    acp::ToolCallContent::Content(acp::Content::new(acp::ContentBlock::Text(
        acp::TextContent::new(format!("This tool call failed: {reason}")),
    )))
}

/// The reason recorded for a call whose arguments never parsed.
///
/// `manch-llm` degrades an unparsable streamed accumulation to
/// `serde_json::Value::Null` rather than aborting the stream. `Null` is not a
/// value a model can send as an argument object, so it is an unambiguous
/// sentinel — which is why this can be detected here without widening
/// `ToolInvocation` or `ToolAccum::apply`'s return type.
const UNREADABLE_ARGUMENTS: &str = "its arguments were not valid JSON and could not be read. Reissue the call with      well-formed JSON arguments.";

/// Report a host tool's progress in ACP's own `ToolCallUpdate` vocabulary, so a
/// UI renders host-registered tools exactly as it renders an ACP agent's own.
async fn report(
    sink: &Arc<dyn EventSink>,
    inv: &ToolInvocation,
    kind: acp::ToolKind,
    status: acp::ToolCallStatus,
    content: Option<Vec<acp::ToolCallContent>>,
) -> Result<()> {
    sink.emit(AgentEvent::Update(acp::SessionUpdate::ToolCallUpdate(
        update(inv, kind, status, content),
    )))
    .await
}

/// Build the `ToolCallUpdate` describing `inv` at `status`.
fn update(
    inv: &ToolInvocation,
    kind: acp::ToolKind,
    status: acp::ToolCallStatus,
    content: Option<Vec<acp::ToolCallContent>>,
) -> acp::ToolCallUpdate {
    acp::ToolCallUpdate::new(
        inv.id.clone(),
        acp::ToolCallUpdateFields::new()
            .kind(kind)
            .status(status)
            .title(inv.name.clone())
            .content(content)
            .raw_input(inv.arguments.clone()),
    )
}

/// Dispatch `inv` and buffer the pair it produces.
///
/// `Pending` is not emitted here: it means "awaiting approval", which a call
/// reaching this function never is — either it is `Read`, or a human has
/// already answered.
pub(crate) async fn execute(
    tool: &dyn Tool,
    cx: &ToolContext,
    inv: &ToolInvocation,
    sink: &Arc<dyn EventSink>,
    buf: &mut Buffer,
) -> Result<()> {
    let kind = tool.schema().kind;
    report(sink, inv, kind, acp::ToolCallStatus::InProgress, None).await?;
    match tool.call(cx, inv.arguments.clone()).await {
        Ok(content) => {
            report(
                sink,
                inv,
                kind,
                acp::ToolCallStatus::Completed,
                Some(vec![content.clone()]),
            )
            .await?;
            buf.record(inv, vec![content]);
            Ok(())
        }
        // A tool that errors is information for the model, not a host fault:
        // it is the same situation as a refused permission, which is already
        // recorded as a `ToolResult` rather than ending the turn. Reporting
        // `Failed` to the sink keeps the UI (and the host) informed; the reason
        // additionally goes back to the model so it can retry or explain.
        //
        // Errors from `report` itself still propagate — a sink that cannot be
        // written to is a host fault, and nothing can be fed back through it.
        Err(e) => {
            let content = failure(&e.to_string());
            report(
                sink,
                inv,
                kind,
                acp::ToolCallStatus::Failed,
                Some(vec![content.clone()]),
            )
            .await?;
            buf.record(inv, vec![content]);
            Ok(())
        }
    }
}

/// Apply a permission outcome to `inv`: run it, record the refusal, or report
/// that the turn was cancelled.
///
/// Shared by the two paths that can hold an outcome — a policy that resolved
/// one itself mid-batch, and a human's answer arriving at
/// [`Manch::approve`](crate::Manch::approve) — so both interpret ACP's
/// vocabulary identically.
pub(crate) async fn apply(
    tool: &dyn Tool,
    cx: &ToolContext,
    inv: &ToolInvocation,
    outcome: &acp::RequestPermissionOutcome,
    sink: &Arc<dyn EventSink>,
    buf: &mut Buffer,
) -> Result<Applied> {
    let selected = match outcome {
        acp::RequestPermissionOutcome::Cancelled => return Ok(Applied::Cancelled),
        acp::RequestPermissionOutcome::Selected(s) => s,
        // `RequestPermissionOutcome` is `#[non_exhaustive]`: a future ACP
        // outcome we cannot interpret must not be read as consent.
        other => {
            return Err(Error::Other(format!(
                "unrecognised permission outcome: {other:?}"
            )));
        }
    };
    // An id Manch does not recognise is never coerced into one of ACP's kinds.
    // A policy free to invent its own option ids is equally free to resolve
    // them itself, which is what `PermissionDecision::Resolved` is for.
    let kind = kind_of(&selected.option_id).ok_or_else(|| {
        Error::Other(format!(
            "permission option '{}' is not one of ACP's kinds; a policy offering its own \
             option ids must resolve them itself",
            selected.option_id.0
        ))
    })?;
    match kind {
        acp::PermissionOptionKind::AllowOnce | acp::PermissionOptionKind::AllowAlways => {
            execute(tool, cx, inv, sink, buf).await?;
            Ok(Applied::Ran)
        }
        acp::PermissionOptionKind::RejectOnce | acp::PermissionOptionKind::RejectAlways => {
            report(
                sink,
                inv,
                tool.schema().kind,
                acp::ToolCallStatus::Failed,
                Some(vec![refusal()]),
            )
            .await?;
            buf.record(inv, vec![refusal()]);
            Ok(Applied::Refused)
        }
        // `PermissionOptionKind` is `#[non_exhaustive]`; deny by default.
        other => Err(Error::Other(format!(
            "unrecognised permission option kind: {other:?}"
        ))),
    }
}

impl Manch {
    /// Resolve the tool the model addressed. Keyed on `schema().name`, never on
    /// a display field.
    pub(crate) fn tool_for(&self, inv: &ToolInvocation) -> Result<Arc<dyn Tool>> {
        self.tools
            .get(&inv.name)
            .cloned()
            .ok_or_else(|| Error::NotFound(inv.name.clone()))
    }

    /// What to tell a model that named a tool which is not registered.
    ///
    /// Names the wrong name *and* the real ones: "unknown tool" alone leaves
    /// the model guessing, and it will usually guess the same way twice. Sorted
    /// so the sentence is stable across runs rather than following `HashMap`
    /// iteration order.
    fn unknown_tool_reason(&self, name: &str) -> String {
        let mut available: Vec<&str> = self.tools.keys().map(String::as_str).collect();
        available.sort_unstable();
        if available.is_empty() {
            return format!("there is no tool named '{name}'; no tools are registered.");
        }
        format!(
            "there is no tool named '{name}'. Available tools: {}.",
            available.join(", ")
        )
    }

    /// Run one batch of invocations in order, buffering their results.
    ///
    /// Returns without flushing on error — the caller drops the buffer, so a
    /// mid-batch failure leaves no partial record.
    pub(crate) async fn run_batch(
        &self,
        session_id: &str,
        ext: &Arc<Extensions>,
        sink: &Arc<dyn EventSink>,
        calls: Vec<ToolInvocation>,
        buf: &mut Buffer,
    ) -> Result<Batch> {
        for inv in calls {
            // A name that was never offered is something models do, and it is
            // recoverable — told what exists, the model can reissue. Answering
            // it is the same call as answering a failed tool (#38): there *is*
            // a call to answer, and the model is the one that can fix it.
            //
            // `Manch::approve` still treats this as an error, and should: there
            // the invocation was already resolved once, so failing on resume
            // means the registry changed underneath a pending approval.
            let Ok(tool) = self.tool_for(&inv) else {
                let content = failure(&self.unknown_tool_reason(&inv.name));
                report(
                    sink,
                    &inv,
                    acp::ToolKind::default(),
                    acp::ToolCallStatus::Failed,
                    Some(vec![content.clone()]),
                )
                .await?;
                buf.record(&inv, vec![content]);
                continue;
            };
            let cx = ToolContext::new(session_id, &inv.id, ext.clone());
            // Checked before the tier split on purpose: a `Draft` call whose
            // arguments are unreadable must not reach a human either. Asking
            // someone to approve an action nobody can describe is worse than
            // not asking, and `propose` would be rendering `null`.
            if inv.arguments.is_null() {
                let content = failure(UNREADABLE_ARGUMENTS);
                report(
                    sink,
                    &inv,
                    tool.schema().kind,
                    acp::ToolCallStatus::Failed,
                    Some(vec![content.clone()]),
                )
                .await?;
                buf.record(&inv, vec![content]);
                continue;
            }
            match tool.tier() {
                Tier::Read => execute(tool.as_ref(), &cx, &inv, sink, buf).await?,
                Tier::Draft => match self.policy.decide(&cx, &inv).await? {
                    PermissionDecision::Resolved(outcome) => {
                        match apply(tool.as_ref(), &cx, &inv, &outcome, sink, buf).await? {
                            Applied::Ran | Applied::Refused => {}
                            Applied::Cancelled => return Ok(Batch::Cancelled),
                        }
                    }
                    PermissionDecision::Ask(options) => {
                        // `propose` previews the action without performing it,
                        // so the human is shown what they are approving.
                        let proposal = tool.propose(&cx, &inv.arguments).await?;
                        let pending = update(
                            &inv,
                            tool.schema().kind,
                            acp::ToolCallStatus::Pending,
                            Some(proposal),
                        );
                        sink.emit(AgentEvent::Update(acp::SessionUpdate::ToolCallUpdate(
                            pending.clone(),
                        )))
                        .await?;
                        return Ok(Batch::Suspended {
                            request: Box::new(acp::RequestPermissionRequest::new(
                                session_id.to_string(),
                                pending,
                                options,
                            )),
                            pending: inv,
                        });
                    }
                },
            }
        }
        Ok(Batch::Completed)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use manch_protocol::acp::{ContentBlock, StopReason, TextContent};
    use manch_protocol::{
        AgentEvent, Approver, Entry, Extensions, MemoryStore, PermissionDecision, PermissionPolicy,
        PromptHandler, Result, Tier, ToolContext, ToolInvocation, TurnOutcome, acp,
    };
    use serde_json::json;

    use crate::testing::{CollectSink, EchoTool, FailTool, ScriptAgent};
    use crate::{Manch, MemStore};

    // ── helpers ─────────────────────────────────────────────────────────────

    fn user_msg(text: &str) -> Vec<ContentBlock> {
        vec![ContentBlock::Text(TextContent::new(text.to_string()))]
    }

    fn inv(id: &str, name: &str) -> ToolInvocation {
        ToolInvocation {
            id: id.to_string(),
            name: name.to_string(),
            arguments: json!({}),
            provider_meta: None,
        }
    }

    fn tool_call(id: &str, name: &str, args: serde_json::Value) -> AgentEvent {
        AgentEvent::ToolCall(ToolInvocation {
            id: id.to_string(),
            name: name.to_string(),
            arguments: args,
            provider_meta: None,
        })
    }

    fn ext() -> Arc<Extensions> {
        Arc::new(Extensions::default())
    }

    fn sink() -> Arc<CollectSink> {
        Arc::new(CollectSink::new())
    }

    fn allow_once() -> acp::RequestPermissionOutcome {
        acp::RequestPermissionOutcome::Selected(acp::SelectedPermissionOutcome::new(
            acp::PermissionOptionId::new("allow_once"),
        ))
    }

    fn reject_once() -> acp::RequestPermissionOutcome {
        acp::RequestPermissionOutcome::Selected(acp::SelectedPermissionOutcome::new(
            acp::PermissionOptionId::new("reject_once"),
        ))
    }

    /// A policy that has already decided: allow, without asking anyone.
    struct AlwaysAllowPolicy;

    #[async_trait::async_trait]
    impl PermissionPolicy for AlwaysAllowPolicy {
        async fn decide(
            &self,
            _cx: &ToolContext,
            _inv: &ToolInvocation,
        ) -> Result<PermissionDecision> {
            Ok(PermissionDecision::Resolved(allow_once()))
        }
    }

    /// Build a runtime whose agent replays `turns`, with an `EchoTool` at
    /// `tier` registered for every distinct tool name those turns address.
    /// Every tool writes into the one returned log, so a test can assert on
    /// exactly what ran and in what order.
    fn manch_from(
        turns: Vec<Vec<AgentEvent>>,
        tier: Tier,
        policy: Option<Arc<dyn PermissionPolicy>>,
    ) -> (Manch, Arc<Mutex<Vec<String>>>, Arc<MemStore>) {
        let log = Arc::new(Mutex::new(Vec::new()));
        let store = Arc::new(MemStore::new());
        let mut names: Vec<String> = Vec::new();
        for turn in &turns {
            for ev in turn {
                if let AgentEvent::ToolCall(i) = ev
                    && !names.contains(&i.name)
                {
                    names.push(i.name.clone());
                }
            }
        }
        let mut builder = Manch::builder()
            .agent(Arc::new(ScriptAgent::new("a", turns)))
            .memory(store.clone());
        for name in names {
            builder = builder.tool(Arc::new(EchoTool::new(&name, tier, log.clone())));
        }
        if let Some(policy) = policy {
            builder = builder.permission_policy(policy);
        }
        (builder.build().unwrap(), log, store)
    }

    fn manch_with(
        events: Vec<AgentEvent>,
        tier: Tier,
    ) -> (Manch, Arc<Mutex<Vec<String>>>, Arc<MemStore>) {
        manch_from(vec![events], tier, None)
    }

    fn manch_with_script(
        turns: Vec<Vec<AgentEvent>>,
        tier: Tier,
    ) -> (Manch, Arc<Mutex<Vec<String>>>, Arc<MemStore>) {
        manch_from(turns, tier, None)
    }

    fn manch_with_policy(
        policy: Arc<dyn PermissionPolicy>,
        tier: Tier,
    ) -> (Manch, Arc<Mutex<Vec<String>>>, Arc<MemStore>) {
        manch_from(
            vec![vec![tool_call("c1", "draft_prescription", json!({}))]],
            tier,
            Some(policy),
        )
    }

    /// One turn emitting two calls, the second of which errors.
    fn manch_with_failing_second_call() -> (Manch, Arc<Mutex<Vec<String>>>, Arc<MemStore>) {
        let log = Arc::new(Mutex::new(Vec::new()));
        let store = Arc::new(MemStore::new());
        let manch = Manch::builder()
            .agent(Arc::new(ScriptAgent::new(
                "a",
                vec![vec![
                    tool_call("c1", "list_appointments", json!({})),
                    tool_call("c2", "boom", json!({})),
                ]],
            )))
            .tool(Arc::new(EchoTool::new(
                "list_appointments",
                Tier::Read,
                log.clone(),
            )))
            .tool(Arc::new(FailTool::new("boom")))
            .memory(store.clone())
            .build()
            .unwrap();
        (manch, log, store)
    }

    /// A model that only ever emits tool calls — it never terminates on its own.
    fn manch_always_calls_a_tool() -> (Manch, Arc<Mutex<Vec<String>>>, Arc<MemStore>) {
        let turns: Vec<Vec<AgentEvent>> = (0..32)
            .map(|n| vec![tool_call(&format!("c{n}"), "list_appointments", json!({}))])
            .collect();
        manch_from(turns, Tier::Read, None)
    }

    // ── tests ───────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn a_read_tool_executes_without_asking() {
        let (m, log, _store) = manch_with(
            vec![tool_call("c1", "list_appointments", json!({}))],
            Tier::Read,
        );
        let out = m
            .handle("a", "s", user_msg("today?"), ext(), sink())
            .await
            .unwrap();
        assert!(matches!(out, TurnOutcome::Finished(_)));
        assert!(
            log.lock()
                .unwrap()
                .contains(&"list_appointments".to_string())
        );
    }

    #[tokio::test]
    async fn a_draft_tool_suspends_and_does_not_execute() {
        let (m, log, _store) = manch_with(
            vec![tool_call("c1", "draft_prescription", json!({}))],
            Tier::Draft,
        );
        let out = m
            .handle("a", "s", user_msg("add metformin"), ext(), sink())
            .await
            .unwrap();
        match out {
            TurnOutcome::AwaitingApproval { pending, .. } => {
                assert_eq!(pending.name, "draft_prescription")
            }
            TurnOutcome::Finished(_) => panic!("a Draft tool must not complete the turn"),
        }
        assert!(
            log.lock().unwrap().is_empty(),
            "nothing may run before approval"
        );
    }

    #[tokio::test]
    async fn approving_runs_the_stored_invocation_without_re_prompting() {
        // The model is scripted to propose a DIFFERENT action on a second prompt.
        // If approve() re-prompted, the wrong tool would run.
        let (m, log, _store) = manch_with_script(
            vec![
                vec![AgentEvent::ToolCall(inv("c1", "draft_prescription"))],
                vec![AgentEvent::ToolCall(inv("c2", "draft_something_else"))],
            ],
            Tier::Draft,
        );
        let TurnOutcome::AwaitingApproval { pending, .. } = m
            .handle("a", "s", user_msg("go"), ext(), sink())
            .await
            .unwrap()
        else {
            panic!("expected suspension")
        };

        m.approve("a", "s", ext(), pending, allow_once(), sink())
            .await
            .unwrap();
        let ran = log.lock().unwrap().clone();
        assert_eq!(
            ran,
            vec!["draft_prescription".to_string()],
            "the approved action ran, and nothing else did"
        );
    }

    #[tokio::test]
    async fn host_extensions_reach_the_tool_that_runs() {
        // The per-call host-context extension point, end to end: whatever the
        // caller put in `Extensions` must be visible to `Tool::call` through
        // `cx.get::<T>()`. Without this, `ext.clone()` in run_batch could be
        // replaced by a fresh default and every other test would still pass.
        #[derive(Clone, PartialEq, Debug)]
        struct Scope(&'static str);

        /// Records whatever `Scope` it saw in its context (None if absent).
        struct ScopeTool(Arc<Mutex<Vec<Option<Scope>>>>);

        #[async_trait]
        impl manch_protocol::Tool for ScopeTool {
            fn schema(&self) -> manch_protocol::ToolSchema {
                manch_protocol::ToolSchema {
                    name: "peek".to_string(),
                    description: String::new(),
                    kind: acp::ToolKind::Other,
                    input_schema: json!({ "type": "object" }),
                }
            }
            fn tier(&self) -> Tier {
                Tier::Read
            }
            async fn call(
                &self,
                cx: &ToolContext,
                _args: serde_json::Value,
            ) -> Result<acp::ToolCallContent> {
                self.0.lock().unwrap().push(cx.get::<Scope>().cloned());
                Ok(acp::ToolCallContent::Content(acp::Content::new(
                    ContentBlock::Text(TextContent::new("ok".to_string())),
                )))
            }
        }

        let seen = Arc::new(Mutex::new(Vec::new()));
        let m = Manch::builder()
            .agent(Arc::new(ScriptAgent::new(
                "a",
                vec![vec![tool_call("c1", "peek", json!({}))], vec![]],
            )))
            .memory(Arc::new(MemStore::new()))
            .tool(Arc::new(ScopeTool(seen.clone())))
            .build()
            .unwrap();

        let mut extensions = Extensions::default();
        extensions.insert(Scope("clinic-42"));
        m.handle("a", "s", user_msg("go"), Arc::new(extensions), sink())
            .await
            .unwrap();

        assert_eq!(
            seen.lock().unwrap().clone(),
            vec![Some(Scope("clinic-42"))],
            "the host's Extensions must reach Tool::call"
        );
    }

    #[tokio::test]
    async fn an_unrecognised_option_id_is_an_error_and_runs_nothing() {
        // Deny-by-default on the resume path. `Manch::approve` is what a
        // stateless server calls with whatever the client sent back, so an
        // option id outside ACP's vocabulary must never be coerced into a
        // kind — least of all an allow. It must error, and the pending tool
        // must not run.
        let (m, log, _store) = manch_with(
            vec![tool_call("c1", "draft_prescription", json!({}))],
            Tier::Draft,
        );
        let TurnOutcome::AwaitingApproval { pending, .. } = m
            .handle("a", "s", user_msg("go"), ext(), sink())
            .await
            .unwrap()
        else {
            panic!("expected suspension")
        };

        let yolo = acp::RequestPermissionOutcome::Selected(acp::SelectedPermissionOutcome::new(
            acp::PermissionOptionId::new("yolo"),
        ));
        let err = m
            .approve("a", "s", ext(), pending, yolo, sink())
            .await
            .expect_err("an unrecognised option id is not consent");
        assert!(
            err.to_string().contains("yolo"),
            "the error should name the offending id, got: {err}"
        );
        assert!(log.lock().unwrap().is_empty(), "the tool must not have run");
    }

    #[tokio::test]
    async fn a_rejected_call_reaches_the_model_as_a_result() {
        let (m, log, store) = manch_with(
            vec![tool_call("c1", "draft_prescription", json!({}))],
            Tier::Draft,
        );
        let TurnOutcome::AwaitingApproval { pending, .. } = m
            .handle("a", "s", user_msg("go"), ext(), sink())
            .await
            .unwrap()
        else {
            panic!("expected suspension")
        };

        m.approve("a", "s", ext(), pending, reject_once(), sink())
            .await
            .unwrap();
        assert!(log.lock().unwrap().is_empty());
        let ctx = store.assemble_context("s").await.unwrap();
        assert!(
            ctx.turns
                .iter()
                .flat_map(|t| &t.entries)
                .any(|e| matches!(e, Entry::ToolResult { .. })),
            "the refusal is recorded so the model can respond to it"
        );
    }

    #[tokio::test]
    async fn a_mid_batch_error_appends_nothing_from_that_batch() {
        // The atomicity invariant, unchanged: when a call in a batch raises an
        // error the turn cannot answer, the whole batch is discarded so no
        // `tool_use` is persisted without its `tool_result`.
        //
        // The vehicle has moved twice. A *failing tool* no longer errors, and
        // neither does an *unregistered name* — both are answered and fed back
        // now. What is left is a host fault the model cannot act on: a policy
        // that cannot decide. There is no honest result to record for it.
        let store = Arc::new(MemStore::new());
        let m = Manch::builder()
            .agent(Arc::new(ScriptAgent::new(
                "a",
                vec![vec![
                    tool_call("c1", "list_appointments", json!({})),
                    tool_call("c2", "book", json!({})),
                ]],
            )))
            .tool(Arc::new(EchoTool::new(
                "list_appointments",
                Tier::Read,
                Arc::new(Mutex::new(Vec::new())),
            )))
            .tool(Arc::new(EchoTool::new(
                "book",
                Tier::Draft,
                Arc::new(Mutex::new(Vec::new())),
            )))
            .permission_policy(Arc::new(crate::testing::FailPolicy))
            .memory(store.clone())
            .build()
            .unwrap();

        let err = m
            .handle("a", "s", user_msg("go"), ext(), sink())
            .await
            .unwrap_err();
        assert!(matches!(err, manch_protocol::Error::Other(m) if m.contains("policy unavailable")));

        let ctx = store.assemble_context("s").await.unwrap();
        assert!(
            !ctx.turns
                .iter()
                .flat_map(|t| &t.entries)
                .any(|e| matches!(e, Entry::ToolResult { .. })),
            "buffered results are discarded when any call in the batch errors"
        );
    }

    #[tokio::test]
    async fn a_mid_batch_tool_failure_records_both_calls_rather_than_discarding() {
        // The other half of the same invariant. Now that a tool failure is
        // answered instead of raised, every call in the batch ends up with a
        // result — the success *and* the failure — so history stays valid and
        // the model can see exactly which call went wrong.
        let (m, _log, store) = manch_with_failing_second_call();
        m.handle("a", "s", user_msg("go"), ext(), sink())
            .await
            .expect("a failing tool no longer ends the turn");

        let ctx = store.assemble_context("s").await.unwrap();
        let results: Vec<_> = ctx
            .turns
            .iter()
            .flat_map(|t| &t.entries)
            .filter_map(|e| match e {
                Entry::ToolResult { id, .. } => Some(id.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            results,
            vec!["c1".to_string(), "c2".to_string()],
            "both calls must be answered, in issue order"
        );
    }

    /// One turn emitting two calls: `first` at `first_tier`, then `second` at
    /// `second_tier`. Both write to the returned log.
    fn manch_with_mixed_batch(
        first: (&str, Tier),
        second: (&str, Tier),
    ) -> (Manch, Arc<Mutex<Vec<String>>>, Arc<MemStore>) {
        let log = Arc::new(Mutex::new(Vec::new()));
        let store = Arc::new(MemStore::new());
        let manch = Manch::builder()
            .agent(Arc::new(ScriptAgent::new(
                "a",
                vec![vec![
                    tool_call("c1", first.0, json!({})),
                    tool_call("c2", second.0, json!({})),
                ]],
            )))
            .tool(Arc::new(EchoTool::new(first.0, first.1, log.clone())))
            .tool(Arc::new(EchoTool::new(second.0, second.1, log.clone())))
            .memory(store.clone())
            .build()
            .unwrap();
        (manch, log, store)
    }

    #[tokio::test]
    async fn invocations_queued_after_a_suspension_are_dropped() {
        // A human paused the turn. Running the calls the model had queued behind
        // the suspending one would defeat the pause; the model re-decides them on
        // resume, when it can see the approved result.
        let (m, log, _store) = manch_with_mixed_batch(
            ("draft_prescription", Tier::Draft),
            ("list_appointments", Tier::Read),
        );
        let out = m
            .handle("a", "s", user_msg("go"), ext(), sink())
            .await
            .unwrap();
        assert!(matches!(out, TurnOutcome::AwaitingApproval { .. }));
        assert!(
            log.lock().unwrap().is_empty(),
            "the call queued behind the suspension must not have run"
        );
    }

    #[tokio::test]
    async fn calls_that_ran_before_a_suspension_are_recorded() {
        // The suspension flushes rather than discarding: those calls really ran,
        // so their results are final and the model must see them on resume.
        let (m, log, store) = manch_with_mixed_batch(
            ("list_appointments", Tier::Read),
            ("draft_prescription", Tier::Draft),
        );
        let out = m
            .handle("a", "s", user_msg("go"), ext(), sink())
            .await
            .unwrap();
        assert!(matches!(out, TurnOutcome::AwaitingApproval { .. }));
        assert_eq!(*log.lock().unwrap(), vec!["list_appointments".to_string()]);
        let ctx = store.assemble_context("s").await.unwrap();
        assert!(
            ctx.turns
                .iter()
                .flat_map(|t| &t.entries)
                .any(|e| matches!(e, Entry::ToolResult { .. })),
            "the result of the call that already ran is appended, not discarded"
        );
    }

    #[tokio::test]
    async fn a_resolved_policy_decision_skips_the_human() {
        let (m, log, _store) = manch_with_policy(Arc::new(AlwaysAllowPolicy), Tier::Draft);
        let out = m
            .handle("a", "s", user_msg("go"), ext(), sink())
            .await
            .unwrap();
        assert!(matches!(out, TurnOutcome::Finished(_)));
        assert!(
            log.lock()
                .unwrap()
                .contains(&"draft_prescription".to_string())
        );
    }

    #[tokio::test]
    async fn a_tool_call_reports_its_status_to_the_ui() {
        // A UI must render host tools the same way it renders an ACP agent's own
        // tools, so the runtime emits ACP ToolCallUpdate transitions.
        let (m, _log, _store) = manch_with(
            vec![tool_call("c1", "list_appointments", json!({}))],
            Tier::Read,
        );
        let sink = Arc::new(CollectSink::new());
        m.handle("a", "s", user_msg("today?"), ext(), sink.clone())
            .await
            .unwrap();

        let statuses: Vec<acp::ToolCallStatus> = sink
            .events()
            .into_iter()
            .filter_map(|e| match e {
                AgentEvent::Update(acp::SessionUpdate::ToolCallUpdate(u)) => u.fields.status,
                _ => None,
            })
            .collect();
        assert_eq!(
            statuses,
            vec![
                acp::ToolCallStatus::InProgress,
                acp::ToolCallStatus::Completed
            ]
        );
    }

    #[tokio::test]
    async fn a_parallel_batch_is_persisted_as_one_call_turn_then_one_result_turn() {
        // Two calls in one model turn must stay in one model turn. Interleaving
        // them as call/result/call/result coalesces into FOUR alternating turns,
        // which misrepresents what the model did — and Gemini rejects it
        // outright, because it attaches a thought signature to only the first
        // call of a batch, leaving a lone second call unsigned in a turn of its
        // own.
        let log = Arc::new(Mutex::new(Vec::new()));
        let store = Arc::new(MemStore::new());
        let m = Manch::builder()
            .agent(Arc::new(ScriptAgent::new(
                "a",
                vec![
                    vec![
                        tool_call("c1", "whoami", json!({})),
                        tool_call("c2", "bed_count", json!({})),
                    ],
                    vec![AgentEvent::text_chunk("kaayantar, 23 beds")],
                ],
            )))
            .tool(Arc::new(EchoTool::new("whoami", Tier::Read, log.clone())))
            .tool(Arc::new(EchoTool::new(
                "bed_count",
                Tier::Read,
                log.clone(),
            )))
            .memory(store.clone())
            .build()
            .unwrap();

        m.handle(
            "a",
            "s",
            user_msg("which clinic and how many beds?"),
            ext(),
            sink(),
        )
        .await
        .unwrap();

        let shape: Vec<(manch_protocol::Role, &'static str)> = store
            .entries()
            .into_iter()
            .map(|(role, e)| {
                (
                    role,
                    match e {
                        Entry::Block(_) => "block",
                        Entry::ToolCall(_) => "call",
                        Entry::ToolResult { .. } => "result",
                    },
                )
            })
            .collect();

        let calls: Vec<usize> = shape
            .iter()
            .enumerate()
            .filter(|(_, (_, k))| *k == "call")
            .map(|(i, _)| i)
            .collect();
        let results: Vec<usize> = shape
            .iter()
            .enumerate()
            .filter(|(_, (_, k))| *k == "result")
            .map(|(i, _)| i)
            .collect();

        assert_eq!(calls.len(), 2, "both calls must be persisted: {shape:?}");
        assert_eq!(
            results.len(),
            2,
            "both results must be persisted: {shape:?}"
        );
        assert_eq!(
            calls[1],
            calls[0] + 1,
            "the two calls must be adjacent so they coalesce into ONE turn: {shape:?}"
        );
        assert!(
            calls[1] < results[0],
            "every call must precede every result, or the batch splits into \
             alternating turns: {shape:?}"
        );
    }

    #[tokio::test]
    async fn a_custom_step_cap_is_respected() {
        // Pins that the configured cap is USED, not merely stored: a model that
        // only ever calls tools must stop after exactly `max_tool_iters` cycles.
        let log = Arc::new(Mutex::new(Vec::new()));
        let turns: Vec<Vec<AgentEvent>> = (0..32)
            .map(|n| vec![tool_call(&format!("c{n}"), "list_appointments", json!({}))])
            .collect();
        let m = Manch::builder()
            .agent(Arc::new(ScriptAgent::new("a", turns)))
            .tool(Arc::new(EchoTool::new(
                "list_appointments",
                Tier::Read,
                log.clone(),
            )))
            .memory(Arc::new(MemStore::new()))
            .max_tool_iters(2)
            .build()
            .unwrap();

        let err = m
            .handle("a", "s", user_msg("go"), ext(), sink())
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains('2'),
            "cap should name itself: {err}"
        );
        assert_eq!(
            log.lock().unwrap().len(),
            2,
            "the tool ran more times than the cap allows"
        );
    }

    #[tokio::test]
    async fn the_loop_terminates_when_a_model_calls_tools_forever() {
        // MAX_TOOL_ITERS = 8. A model that only ever emits tool calls must not spin.
        let (m, _log, _store) = manch_always_calls_a_tool();
        let err = m
            .handle("a", "s", user_msg("go"), ext(), sink())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("exceeded"), "got: {err}");
    }

    #[tokio::test]
    async fn handle_with_approver_drives_a_draft_call_to_completion() {
        struct AlwaysAllow;
        #[async_trait]
        impl Approver for AlwaysAllow {
            async fn approve(
                &self,
                _req: acp::RequestPermissionRequest,
            ) -> Result<acp::RequestPermissionOutcome> {
                Ok(acp::RequestPermissionOutcome::Selected(
                    acp::SelectedPermissionOutcome::new(acp::PermissionOptionId::new("allow_once")),
                ))
            }
        }
        let (m, log, _store) = manch_with(
            vec![tool_call("c1", "draft_prescription", json!({}))],
            Tier::Draft,
        );
        let stop = m
            .handle_with_approver("a", "s", user_msg("go"), ext(), &AlwaysAllow, sink())
            .await
            .unwrap();
        assert_eq!(stop, StopReason::EndTurn);
        assert!(
            log.lock()
                .unwrap()
                .contains(&"draft_prescription".to_string())
        );
    }

    #[tokio::test]
    async fn handle_with_approver_stops_when_the_approver_cancels() {
        struct Cancels;
        #[async_trait]
        impl Approver for Cancels {
            async fn approve(
                &self,
                _req: acp::RequestPermissionRequest,
            ) -> Result<acp::RequestPermissionOutcome> {
                Ok(acp::RequestPermissionOutcome::Cancelled)
            }
        }
        let (m, log, _store) = manch_with(
            vec![tool_call("c1", "draft_prescription", json!({}))],
            Tier::Draft,
        );
        let stop = m
            .handle_with_approver("a", "s", user_msg("go"), ext(), &Cancels, sink())
            .await
            .unwrap();
        assert_eq!(stop, StopReason::Cancelled);
        assert!(log.lock().unwrap().is_empty());
    }
}
