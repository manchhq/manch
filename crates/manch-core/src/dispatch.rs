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
pub(crate) struct Buffer(Vec<(Role, Entry)>);

impl Buffer {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Record a dispatched call and the content it produced.
    ///
    /// The `ToolCall` is pushed before its `ToolResult` and the two are never
    /// separated: Anthropic rejects a `tool_result` with no preceding
    /// `tool_use`, so stored history would be invalid input for the next loop
    /// iteration if they were written the other way round.
    pub(crate) fn record(&mut self, inv: &ToolInvocation, content: Vec<acp::ToolCallContent>) {
        self.0.push((Role::Assistant, Entry::ToolCall(inv.clone())));
        self.0.push((
            Role::User,
            Entry::ToolResult {
                id: inv.id.clone(),
                content,
            },
        ));
    }

    /// Append everything buffered, in order. Consumes the buffer, so it cannot
    /// be flushed twice.
    pub(crate) async fn flush(self, memory: &dyn MemoryStore, session_id: &str) -> Result<()> {
        for (role, entry) in self.0 {
            memory.append(session_id, role, entry).await?;
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
        Err(e) => {
            report(sink, inv, kind, acp::ToolCallStatus::Failed, None).await?;
            Err(e)
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
            let tool = self.tool_for(&inv)?;
            let cx = ToolContext::new(session_id, &inv.id, ext.clone());
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
    async fn a_mid_batch_failure_appends_nothing_from_that_batch() {
        // Two calls in one turn; the second fails. The first must leave no record.
        let (m, _log, store) = manch_with_failing_second_call();
        let _ = m
            .handle("a", "s", user_msg("go"), ext(), sink())
            .await
            .unwrap_err();
        let ctx = store.assemble_context("s").await.unwrap();
        assert!(
            !ctx.turns
                .iter()
                .flat_map(|t| &t.entries)
                .any(|e| matches!(e, Entry::ToolResult { .. })),
            "buffered results are discarded when any call in the batch errors"
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
