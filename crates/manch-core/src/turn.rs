use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use manch_protocol::acp::{ContentBlock, SessionUpdate};
use manch_protocol::{AgentEvent, EventSink, Result, ToolInvocation};

/// Wraps the caller's sink for one sub-turn: streamed `Update`s pass through
/// live, host-tool `ToolCall`s are captured for dispatch (not forwarded — they
/// are host-side control events, not UI output), and the agent's own `Done` is
/// swallowed (the runtime emits a single final `Done` for the whole exchange).
pub(crate) struct InterceptSink {
    inner: Arc<dyn EventSink>,
    captured: Mutex<Vec<ToolInvocation>>,
    text: Mutex<String>,
}

impl InterceptSink {
    pub(crate) fn new(inner: Arc<dyn EventSink>) -> Self {
        Self {
            inner,
            captured: Mutex::new(Vec::new()),
            text: Mutex::new(String::new()),
        }
    }
    /// Drain the tool calls captured during the sub-turn.
    pub(crate) fn take_calls(&self) -> Vec<ToolInvocation> {
        std::mem::take(&mut self.captured.lock().unwrap())
    }
    /// Drain the assistant text accumulated during the sub-turn (`None` if the
    /// sub-turn emitted no text — e.g. a pure tool-call turn).
    pub(crate) fn take_text(&self) -> Option<String> {
        let mut guard = self.text.lock().unwrap();
        if guard.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut *guard))
        }
    }
}

#[async_trait]
impl EventSink for InterceptSink {
    async fn emit(&self, event: AgentEvent) -> Result<()> {
        match event {
            AgentEvent::ToolCall(inv) => {
                self.captured.lock().unwrap().push(inv);
                Ok(())
            }
            AgentEvent::Done(_) => Ok(()),
            // Token counts are UI output, not a host-side control event — forward
            // them so a consumer can show spend. Never a billing source: a
            // managed tier meters at its own proxy.
            AgentEvent::Usage(u) => self.inner.emit(AgentEvent::Usage(u)).await,
            // Accumulate assistant text for persistence, then forward live as
            // UI output. Non-text updates pass through untouched.
            AgentEvent::Update(u) => {
                if let SessionUpdate::AgentMessageChunk(chunk) = &u
                    && let ContentBlock::Text(t) = &chunk.content
                {
                    self.text.lock().unwrap().push_str(&t.text);
                }
                self.inner.emit(AgentEvent::Update(u)).await
            }
            // `AgentEvent` is `#[non_exhaustive]`, so this arm is required and
            // a new variant can no longer be caught here at compile time.
            //
            // Forwarding is the right default *for this type*: `InterceptSink`
            // is a pass-through that exists to observe tool calls and assistant
            // text on their way past. An event it does not recognise is not its
            // business, and dropping one here would make it invisible to
            // everything downstream. Forwarding at worst hands the UI something
            // it ignores; dropping loses it outright.
            other => self.inner.emit(other).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use manch_protocol::PromptHandler;
    use manch_protocol::acp::{ContentBlock, StopReason, TextContent};
    use manch_protocol::{
        AgentEvent, Entry, Error, EventSink, Extensions, MemoryStore, Role, Tier, Tool,
        ToolInvocation, TurnOutcome, Usage, acp,
    };

    use crate::Manch;
    use crate::MemStore;
    use crate::testing::{CollectSink, EchoTool, ScriptAgent};

    #[tokio::test]
    async fn intercept_sink_forwards_usage_to_the_caller() {
        // Token counts are UI output (a spend display), not a host-side control
        // event like a tool call, so they must reach the caller's sink.
        let inner = Arc::new(CollectSink::new());
        let sink = super::InterceptSink::new(inner.clone());
        let usage = Usage {
            input_tokens: Some(12),
            output_tokens: Some(4),
            ..Default::default()
        };
        sink.emit(AgentEvent::Usage(usage)).await.unwrap();
        assert!(matches!(
            inner.events().as_slice(),
            [AgentEvent::Usage(got)] if *got == usage
        ));
    }

    fn user_msg(text: &str) -> Vec<ContentBlock> {
        vec![ContentBlock::Text(TextContent::new(text.to_string()))]
    }

    /// Empty host context — these tests exercise the loop, not extensions.
    fn ext() -> Arc<Extensions> {
        Arc::new(Extensions::default())
    }

    /// Build an `AgentEvent::ToolCall` addressed to a registered tool by name.
    fn tool_call(name: &str) -> AgentEvent {
        AgentEvent::ToolCall(ToolInvocation {
            id: format!("call-{name}"),
            name: name.to_string(),
            arguments: serde_json::json!({ "x": 1 }),
            provider_meta: None,
        })
    }

    #[tokio::test]
    async fn a_tool_is_dispatched_by_its_schema_name() {
        // The registry keys on schema().name — never on anything the model
        // chose. `ToolInvocation` has no display field at all, so a name/title
        // mismatch is not even representable; what this pins is that dispatch
        // goes through the schema name and reaches the registered tool.
        let log = Arc::new(Mutex::new(Vec::new()));
        let store = Arc::new(MemStore::new());
        let manch = Manch::builder()
            .agent(Arc::new(ScriptAgent::new(
                "a",
                vec![
                    vec![AgentEvent::ToolCall(ToolInvocation {
                        id: "call-1".into(),
                        name: "search_patients".into(),
                        arguments: serde_json::json!({ "name": "Asha" }),
                        provider_meta: None,
                    })],
                    vec![AgentEvent::text_chunk("done")],
                ],
            )))
            .tool(Arc::new(EchoTool::new(
                "search_patients",
                Tier::Read,
                log.clone(),
            )))
            .memory(store.clone())
            .build()
            .unwrap();

        let sink = Arc::new(CollectSink::new());
        manch
            .handle("a", "s", user_msg("find asha"), ext(), sink)
            .await
            .unwrap();
        assert!(log.lock().unwrap().contains(&"search_patients".to_string()));
    }

    #[test]
    fn tier_is_declared_per_tool() {
        let log = Arc::new(Mutex::new(Vec::new()));
        assert_eq!(EchoTool::new("t", Tier::Draft, log).tier(), Tier::Draft);
    }

    #[tokio::test]
    async fn unknown_agent_is_not_found() {
        let manch = Manch::builder()
            .memory(Arc::new(MemStore::new()))
            .build()
            .unwrap();
        let sink = Arc::new(CollectSink::new());
        let err = manch
            .handle("nope", "s", user_msg("hi"), ext(), sink.clone())
            .await
            .unwrap_err();
        assert!(matches!(err, Error::NotFound(_)));
        assert!(sink.events().is_empty());
    }

    #[tokio::test]
    async fn text_turn_streams_through_and_ends_with_one_done() {
        let agent = ScriptAgent::new(
            "a",
            vec![vec![
                AgentEvent::text_chunk("hello"),
                AgentEvent::Done(StopReason::EndTurn),
            ]],
        );
        let store = Arc::new(MemStore::new());
        let manch = Manch::builder()
            .agent(Arc::new(agent))
            .memory(store.clone())
            .build()
            .unwrap();
        let sink = Arc::new(CollectSink::new());

        let outcome = manch
            .handle("a", "s", user_msg("hi"), ext(), sink.clone())
            .await
            .unwrap();

        assert!(matches!(
            outcome,
            TurnOutcome::Finished(StopReason::EndTurn)
        ));
        let evs = sink.events();
        // one text Update forwarded + exactly one final Done (agent's own Done swallowed).
        let updates = evs
            .iter()
            .filter(|e| matches!(e, AgentEvent::Update(_)))
            .count();
        let dones = evs
            .iter()
            .filter(|e| matches!(e, AgentEvent::Done(_)))
            .count();
        assert_eq!(updates, 1);
        assert_eq!(dones, 1);
        assert!(matches!(evs.last(), Some(AgentEvent::Done(_))));
        // the user message + the assistant's "hello" were both appended.
        assert_eq!(store.len(), 2);
    }

    #[tokio::test]
    async fn tool_call_is_dispatched_then_reprompted() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let echo = EchoTool::new("echo", Tier::Read, log.clone());
        // turn 1: emit a tool call. turn 2: finish with text + Done.
        let agent = ScriptAgent::new(
            "a",
            vec![
                vec![tool_call("echo")],
                vec![
                    AgentEvent::text_chunk("done"),
                    AgentEvent::Done(StopReason::EndTurn),
                ],
            ],
        );
        let store = Arc::new(MemStore::new());
        let manch = Manch::builder()
            .agent(Arc::new(agent))
            .tool(Arc::new(echo))
            .memory(store.clone())
            .build()
            .unwrap();
        let sink = Arc::new(CollectSink::new());

        manch
            .handle("a", "s", user_msg("hi"), ext(), sink.clone())
            .await
            .unwrap();

        assert_eq!(*log.lock().unwrap(), vec!["echo".to_string()]); // tool ran once
        let evs = sink.events();
        // caller never sees a raw ToolCall event; sees the turn-2 text + one Done.
        assert!(!evs.iter().any(|e| matches!(e, AgentEvent::ToolCall(_))));
        assert_eq!(
            evs.iter()
                .filter(|e| matches!(e, AgentEvent::Done(_)))
                .count(),
            1
        );
        // memory: user msg + assistant tool_call + tool result + assistant "done".
        assert_eq!(store.len(), 4);
    }

    #[tokio::test]
    async fn a_tool_call_is_persisted_before_its_result() {
        // Anthropic rejects a tool_result with no preceding tool_use, so a
        // second loop iteration would send invalid history if these were
        // stored the other way round. Assert the order, not the count.
        let log = Arc::new(Mutex::new(Vec::new()));
        let echo = EchoTool::new("echo", Tier::Read, log);
        let agent = ScriptAgent::new(
            "a",
            vec![
                vec![tool_call("echo")],
                vec![
                    AgentEvent::text_chunk("done"),
                    AgentEvent::Done(StopReason::EndTurn),
                ],
            ],
        );
        let store = Arc::new(MemStore::new());
        let manch = Manch::builder()
            .agent(Arc::new(agent))
            .tool(Arc::new(echo))
            .memory(store.clone())
            .build()
            .unwrap();
        let sink = Arc::new(CollectSink::new());

        manch
            .handle("a", "s", user_msg("hi"), ext(), sink)
            .await
            .unwrap();

        let entries = store.entries();
        let call_at = entries
            .iter()
            .position(|(_, e)| matches!(e, Entry::ToolCall(_)))
            .expect("the assistant's tool call must be persisted");
        let result_at = entries
            .iter()
            .position(|(_, e)| matches!(e, Entry::ToolResult { .. }))
            .expect("the tool result must be persisted");
        assert!(
            call_at < result_at,
            "tool_use must precede tool_result; got call at {call_at}, result at {result_at}"
        );
    }

    #[tokio::test]
    async fn an_unknown_tool_name_is_fed_back_so_the_model_can_correct_itself() {
        // Replaces `unknown_tool_is_not_found`, which pinned the opposite.
        // Naming a tool that was never offered is something models do, and it
        // is recoverable: told which tools exist, the model can reissue. Ending
        // the turn denies it that, exactly as a failing tool used to (#38).
        let agent = ScriptAgent::new(
            "a",
            vec![
                vec![tool_call("ghost")],
                vec![
                    AgentEvent::text_chunk("using the real one"),
                    AgentEvent::Done(StopReason::EndTurn),
                ],
            ],
        );
        let store = Arc::new(MemStore::new());
        let manch = Manch::builder()
            .agent(Arc::new(agent))
            .tool(Arc::new(EchoTool::new(
                "echo",
                Tier::Read,
                Arc::new(Mutex::new(Vec::new())),
            )))
            .memory(store.clone())
            .build()
            .unwrap();

        manch
            .handle(
                "a",
                "s",
                user_msg("hi"),
                ext(),
                Arc::new(CollectSink::new()),
            )
            .await
            .expect("an unknown tool name must not end the turn");

        let text = first_tool_result_text(&store.entries());
        assert!(
            text.contains("ghost"),
            "the model must be told which name it got wrong; got {text:?}"
        );
        assert!(
            text.contains("echo"),
            "and which tools it may actually call; got {text:?}"
        );
    }

    #[tokio::test]
    async fn a_policy_that_cannot_decide_still_ends_the_turn() {
        // The line drawn in #38 and extended here: a failure is answered when
        // there is a call to answer *and* the model can act on it. A policy
        // that errors is neither — there is no honest result to record, and
        // pretending otherwise would let a Draft tool past an undecided gate.
        use crate::testing::FailPolicy;
        let agent = ScriptAgent::new("a", vec![vec![tool_call("echo")]]);
        let manch = Manch::builder()
            .agent(Arc::new(agent))
            .tool(Arc::new(EchoTool::new(
                "echo",
                Tier::Draft,
                Arc::new(Mutex::new(Vec::new())),
            )))
            .permission_policy(Arc::new(FailPolicy))
            .memory(Arc::new(MemStore::new()))
            .build()
            .unwrap();
        let err = manch
            .handle(
                "a",
                "s",
                user_msg("hi"),
                ext(),
                Arc::new(CollectSink::new()),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Other(m) if m.contains("policy unavailable")));
    }

    /// The text of the first persisted `ToolResult` — what the model is
    /// actually told about a call that did not succeed.
    fn first_tool_result_text(entries: &[(Role, Entry)]) -> String {
        entries
            .iter()
            .find_map(|(_, e)| match e {
                Entry::ToolResult { content, .. } => Some(content.clone()),
                _ => None,
            })
            .expect("a tool result must be persisted")
            .iter()
            .filter_map(|c| match c {
                acp::ToolCallContent::Content(inner) => match &inner.content {
                    ContentBlock::Text(t) => Some(t.text.clone()),
                    _ => None,
                },
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[tokio::test]
    async fn a_failing_tool_is_reported_back_to_the_model_instead_of_ending_the_turn() {
        // Replaces `failing_tool_propagates_and_stops`, which pinned the
        // opposite behaviour. A tool that errors is not a host bug — it is
        // information the model can act on, exactly like the refusal a rejected
        // permission already records. Ending the turn instead denies it the
        // chance to retry or explain, which is #38.
        use crate::testing::FailTool;
        let agent = ScriptAgent::new(
            "a",
            vec![
                vec![tool_call("boom")],
                vec![
                    AgentEvent::text_chunk("could not do that"),
                    AgentEvent::Done(StopReason::EndTurn),
                ],
            ],
        );
        let store = Arc::new(MemStore::new());
        let manch = Manch::builder()
            .agent(Arc::new(agent))
            .tool(Arc::new(FailTool::new("boom")))
            .memory(store.clone())
            .build()
            .unwrap();
        let sink = Arc::new(CollectSink::new());

        let stop = manch
            .handle("a", "s", user_msg("hi"), ext(), sink.clone())
            .await
            .expect("a failing tool must not end the turn");
        assert_eq!(stop, TurnOutcome::Finished(StopReason::EndTurn));

        let entries = store.entries();
        let text = first_tool_result_text(&entries);
        assert!(
            text.contains("failed"),
            "the model must be told the call failed; got {text:?}"
        );

        // The UI still learns the call failed — the signal is not swallowed,
        // it is redirected.
        assert!(
            sink.events().iter().any(|e| matches!(
                e,
                AgentEvent::Update(acp::SessionUpdate::ToolCallUpdate(u))
                    if u.fields.status == Some(acp::ToolCallStatus::Failed)
            )),
            "the sink must still report the call as Failed"
        );
    }

    #[tokio::test]
    async fn unreadable_arguments_never_reach_the_tool_and_are_fed_back() {
        // `manch-llm` degrades an unparsable streamed argument accumulation to
        // `Value::Null` — deliberately, because `{}` is valid arguments for a
        // zero-argument tool and would let a garbled call execute. `Null` is
        // therefore a sentinel no model can send, and the call must be answered
        // rather than run.
        let log = Arc::new(Mutex::new(Vec::new()));
        let garbled = AgentEvent::ToolCall(ToolInvocation {
            id: "call-garbled".to_string(),
            name: "echo".to_string(),
            arguments: serde_json::Value::Null,
            provider_meta: None,
        });
        let agent = ScriptAgent::new(
            "a",
            vec![
                vec![garbled],
                vec![
                    AgentEvent::text_chunk("retrying"),
                    AgentEvent::Done(StopReason::EndTurn),
                ],
            ],
        );
        let store = Arc::new(MemStore::new());
        let manch = Manch::builder()
            .agent(Arc::new(agent))
            .tool(Arc::new(EchoTool::new("echo", Tier::Read, log.clone())))
            .memory(store.clone())
            .build()
            .unwrap();
        let sink = Arc::new(CollectSink::new());

        manch
            .handle("a", "s", user_msg("hi"), ext(), sink)
            .await
            .expect("unreadable arguments must not end the turn");

        assert!(
            log.lock().unwrap().is_empty(),
            "the tool must never be called with arguments that failed to parse"
        );
        let text = first_tool_result_text(&store.entries());
        assert!(
            text.contains("arguments"),
            "the model must be told its arguments were unreadable; got {text:?}"
        );
    }

    #[tokio::test]
    async fn a_tool_that_always_fails_still_terminates_at_the_iteration_cap() {
        // Feeding failures back makes them non-terminal, so the only thing left
        // bounding a model that retries forever is the step cap. Pin it.
        use crate::testing::FailTool;
        let turns: Vec<Vec<AgentEvent>> = (0..32).map(|_| vec![tool_call("boom")]).collect();
        let manch = Manch::builder()
            .agent(Arc::new(ScriptAgent::new("a", turns)))
            .tool(Arc::new(FailTool::new("boom")))
            .memory(Arc::new(MemStore::new()))
            .build()
            .unwrap();
        let sink = Arc::new(CollectSink::new());
        let err = manch
            .handle("a", "s", user_msg("hi"), ext(), sink)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Other(msg) if msg.contains("exceeded")));
    }

    #[tokio::test]
    async fn assistant_output_is_persisted() {
        let agent = ScriptAgent::new(
            "a",
            vec![
                vec![
                    AgentEvent::text_chunk("first reply"),
                    AgentEvent::Done(StopReason::EndTurn),
                ],
                vec![
                    AgentEvent::text_chunk("second reply"),
                    AgentEvent::Done(StopReason::EndTurn),
                ],
            ],
        );
        let store = Arc::new(MemStore::new());
        let manch = Manch::builder()
            .agent(Arc::new(agent))
            .memory(store.clone())
            .build()
            .unwrap();
        let sink = Arc::new(CollectSink::new());

        manch
            .handle("a", "s", user_msg("first"), ext(), sink.clone())
            .await
            .unwrap();

        // After turn 1: [User "first", Assistant "first reply"].
        let ctx = store.assemble_context("s").await.unwrap();
        assert_eq!(ctx.turns.len(), 2);
        assert_eq!(ctx.turns[0].role, Role::User);
        assert_eq!(ctx.turns[1].role, Role::Assistant);
        match &ctx.turns[1].entries[0] {
            manch_protocol::Entry::Block(ContentBlock::Text(t)) => {
                assert_eq!(t.text, "first reply")
            }
            _ => panic!("expected assistant text"),
        }

        manch
            .handle("a", "s", user_msg("second"), ext(), sink.clone())
            .await
            .unwrap();

        // Turn 2 sees turn 1's assistant reply: [User, Assistant, User, Assistant].
        let ctx2 = store.assemble_context("s").await.unwrap();
        assert_eq!(ctx2.turns.len(), 4);
        assert_eq!(ctx2.turns[3].role, Role::Assistant);
    }

    #[tokio::test]
    async fn endless_tool_calls_hit_the_iteration_cap() {
        // every turn emits a tool call → never terminates on its own.
        let turns: Vec<Vec<AgentEvent>> = (0..32).map(|_| vec![tool_call("echo")]).collect();
        let log = Arc::new(Mutex::new(Vec::new()));
        let manch = Manch::builder()
            .agent(Arc::new(ScriptAgent::new("a", turns)))
            .tool(Arc::new(EchoTool::new("echo", Tier::Read, log)))
            .memory(Arc::new(MemStore::new()))
            .build()
            .unwrap();
        let sink = Arc::new(CollectSink::new());
        let err = manch
            .handle("a", "s", user_msg("hi"), ext(), sink)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Other(msg) if msg.contains("exceeded")));
    }
}
