use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use manch_protocol::acp::{ContentBlock, SessionUpdate, ToolCall};
use manch_protocol::{AgentEvent, EventSink, Result};

/// Wraps the caller's sink for one sub-turn: streamed `Update`s pass through
/// live, host-tool `ToolCall`s are captured for dispatch (not forwarded — they
/// are host-side control events, not UI output), and the agent's own `Done` is
/// swallowed (the runtime emits a single final `Done` for the whole exchange).
pub(crate) struct InterceptSink {
    inner: Arc<dyn EventSink>,
    captured: Mutex<Vec<ToolCall>>,
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
    pub(crate) fn take_calls(&self) -> Vec<ToolCall> {
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
            AgentEvent::ToolCall(tc) => {
                self.captured.lock().unwrap().push(tc);
                Ok(())
            }
            AgentEvent::Done(_) => Ok(()),
            // Explicit (not a catch-all) so that if `AgentEvent` grows a new
            // variant, this match fails to compile instead of silently
            // forwarding an unrecognized event to the caller as UI output.
            //
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
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use manch_protocol::PromptHandler;
    use manch_protocol::acp::{ContentBlock, StopReason, TextContent, ToolCall};
    use manch_protocol::{AgentEvent, Error, MemoryStore, Role};

    use crate::Manch;
    use crate::testing::{CollectSink, ScriptAgent, VecStore};

    fn user_msg(text: &str) -> Vec<ContentBlock> {
        vec![ContentBlock::Text(TextContent::new(text.to_string()))]
    }

    /// Build an `AgentEvent::ToolCall` addressed to a registered tool by name.
    fn tool_call(name: &str) -> AgentEvent {
        let mut tc = ToolCall::new(format!("call-{name}"), name.to_string());
        tc.raw_input = Some(serde_json::json!({ "x": 1 }));
        AgentEvent::ToolCall(tc)
    }

    #[tokio::test]
    async fn unknown_agent_is_not_found() {
        let manch = Manch::builder()
            .memory(Arc::new(VecStore::new()))
            .build()
            .unwrap();
        let sink = Arc::new(CollectSink::new());
        let err = manch
            .handle("nope", "s", user_msg("hi"), sink.clone())
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
        let store = Arc::new(VecStore::new());
        let manch = Manch::builder()
            .agent(Arc::new(agent))
            .memory(store.clone())
            .build()
            .unwrap();
        let sink = Arc::new(CollectSink::new());

        let stop = manch
            .handle("a", "s", user_msg("hi"), sink.clone())
            .await
            .unwrap();

        assert!(matches!(stop, StopReason::EndTurn));
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
        use crate::testing::EchoTool;
        let echo = EchoTool::new("echo");
        let calls = echo.calls.clone();
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
        let store = Arc::new(VecStore::new());
        let manch = Manch::builder()
            .agent(Arc::new(agent))
            .tool(Arc::new(echo))
            .memory(store.clone())
            .build()
            .unwrap();
        let sink = Arc::new(CollectSink::new());

        manch
            .handle("a", "s", user_msg("hi"), sink.clone())
            .await
            .unwrap();

        assert_eq!(*calls.lock().unwrap(), 1); // tool ran once
        let evs = sink.events();
        // caller never sees a raw ToolCall event; sees the turn-2 text + one Done.
        assert!(!evs.iter().any(|e| matches!(e, AgentEvent::ToolCall(_))));
        assert_eq!(
            evs.iter()
                .filter(|e| matches!(e, AgentEvent::Done(_)))
                .count(),
            1
        );
        // memory: user msg + tool result + assistant "done" appended.
        assert_eq!(store.len(), 3);
    }

    #[tokio::test]
    async fn unknown_tool_is_not_found() {
        let agent = ScriptAgent::new("a", vec![vec![tool_call("ghost")]]);
        let manch = Manch::builder()
            .agent(Arc::new(agent))
            .memory(Arc::new(VecStore::new()))
            .build()
            .unwrap();
        let sink = Arc::new(CollectSink::new());
        let err = manch
            .handle("a", "s", user_msg("hi"), sink)
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
            .memory(Arc::new(VecStore::new()))
            .build()
            .unwrap();
        let sink = Arc::new(CollectSink::new());
        let err = manch
            .handle("a", "s", user_msg("hi"), sink)
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
        let store = Arc::new(VecStore::new());
        let manch = Manch::builder()
            .agent(Arc::new(agent))
            .memory(store.clone())
            .build()
            .unwrap();
        let sink = Arc::new(CollectSink::new());

        manch
            .handle("a", "s", user_msg("first"), sink.clone())
            .await
            .unwrap();

        // After turn 1: [User "first", Assistant "first reply"].
        let ctx = store.assemble_context("s").await.unwrap();
        assert_eq!(ctx.turns.len(), 2);
        assert_eq!(ctx.turns[0].role, Role::User);
        assert_eq!(ctx.turns[1].role, Role::Assistant);
        match &ctx.turns[1].blocks[0] {
            ContentBlock::Text(t) => assert_eq!(t.text, "first reply"),
            _ => panic!("expected assistant text"),
        }

        manch
            .handle("a", "s", user_msg("second"), sink.clone())
            .await
            .unwrap();

        // Turn 2 sees turn 1's assistant reply: [User, Assistant, User, Assistant].
        let ctx2 = store.assemble_context("s").await.unwrap();
        assert_eq!(ctx2.turns.len(), 4);
        assert_eq!(ctx2.turns[3].role, Role::Assistant);
    }

    #[tokio::test]
    async fn endless_tool_calls_hit_the_iteration_cap() {
        use crate::testing::EchoTool;
        // every turn emits a tool call → never terminates on its own.
        let turns: Vec<Vec<AgentEvent>> = (0..32).map(|_| vec![tool_call("echo")]).collect();
        let manch = Manch::builder()
            .agent(Arc::new(ScriptAgent::new("a", turns)))
            .tool(Arc::new(EchoTool::new("echo")))
            .memory(Arc::new(VecStore::new()))
            .build()
            .unwrap();
        let sink = Arc::new(CollectSink::new());
        let err = manch
            .handle("a", "s", user_msg("hi"), sink)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Other(msg) if msg.contains("exceeded")));
    }
}
