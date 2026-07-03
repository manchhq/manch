use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use manch_protocol::acp::ToolCall;
use manch_protocol::{AgentEvent, EventSink, Result};

/// Wraps the caller's sink for one sub-turn: streamed `Update`s pass through
/// live, host-tool `ToolCall`s are captured for dispatch (not forwarded — they
/// are host-side control events, not UI output), and the agent's own `Done` is
/// swallowed (the runtime emits a single final `Done` for the whole exchange).
pub(crate) struct InterceptSink {
    inner: Arc<dyn EventSink>,
    captured: Mutex<Vec<ToolCall>>,
}

impl InterceptSink {
    pub(crate) fn new(inner: Arc<dyn EventSink>) -> Self {
        Self {
            inner,
            captured: Mutex::new(Vec::new()),
        }
    }
    /// Drain the tool calls captured during the sub-turn.
    pub(crate) fn take_calls(&self) -> Vec<ToolCall> {
        std::mem::take(&mut self.captured.lock().unwrap())
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
            update => self.inner.emit(update).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use manch_protocol::PromptHandler;
    use manch_protocol::acp::{ContentBlock, StopReason, TextContent};
    use manch_protocol::{AgentEvent, Error};

    use crate::Manch;
    use crate::testing::{CollectSink, ScriptAgent, VecStore};

    fn user_msg(text: &str) -> Vec<ContentBlock> {
        vec![ContentBlock::Text(TextContent::new(text.to_string()))]
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
        // the inbound user message was appended to memory.
        assert_eq!(store.len(), 1);
    }
}
