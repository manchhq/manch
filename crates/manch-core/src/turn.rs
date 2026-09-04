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
        ToolInvocation, TurnOutcome, Usage,
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
    async fn unknown_tool_is_not_found() {
        let agent = ScriptAgent::new("a", vec![vec![tool_call("ghost")]]);
        let manch = Manch::builder()
            .agent(Arc::new(agent))
            .memory(Arc::new(MemStore::new()))
            .build()
            .unwrap();
        let sink = Arc::new(CollectSink::new());
        let err = manch
            .handle("a", "s", user_msg("hi"), ext(), sink)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::NotFound(name) if name == "ghost"));
    }

    #[tokio::test]
    async fn failing_tool_propagates_and_stops() {
        use crate::testing::FailTool;
        let agent = ScriptAgent::new("a", vec![vec![tool_call("boom")]]);
        let manch = Manch::builder()
            .agent(Arc::new(agent))
            .tool(Arc::new(FailTool::new("boom")))
            .memory(Arc::new(MemStore::new()))
            .build()
            .unwrap();
        let sink = Arc::new(CollectSink::new());
        let err = manch
            .handle("a", "s", user_msg("hi"), ext(), sink)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Other(_)));
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
