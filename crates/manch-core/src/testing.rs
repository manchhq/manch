//! In-crate mocks for the runtime's unit tests. Not compiled outside `cfg(test)`.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use manch_protocol::acp::{ContentBlock, StopReason, TextContent, ToolCallContent, ToolKind};
use manch_protocol::{
    Agent, AgentEvent, Context, EventSink, MemoryStore, Result, Tool, ToolSchema,
};
use manch_protocol::{Role, coalesce_turns};

/// An `Agent` that replays a pre-scripted list of event batches — one batch per
/// `prompt()` call. Each batch is emitted in order; the call returns `EndTurn`.
pub struct ScriptAgent {
    id: &'static str,
    turns: Mutex<std::collections::VecDeque<Vec<AgentEvent>>>,
}

impl ScriptAgent {
    pub fn new(id: &'static str, turns: Vec<Vec<AgentEvent>>) -> Self {
        Self {
            id,
            turns: Mutex::new(turns.into_iter().collect()),
        }
    }
}

#[async_trait]
impl Agent for ScriptAgent {
    fn id(&self) -> &str {
        self.id
    }
    async fn prompt(
        &self,
        _ctx: Context,
        _tools: &[ToolSchema],
        sink: Arc<dyn EventSink>,
    ) -> Result<StopReason> {
        let batch = self.turns.lock().unwrap().pop_front().unwrap_or_default();
        for ev in batch {
            sink.emit(ev).await?;
        }
        Ok(StopReason::EndTurn)
    }
}

/// A `Tool` that echoes a fixed text result and counts its invocations.
pub struct EchoTool {
    name: &'static str,
    pub calls: Arc<Mutex<usize>>,
}

impl EchoTool {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            calls: Arc::new(Mutex::new(0)),
        }
    }
}

#[async_trait]
impl Tool for EchoTool {
    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name.to_string(),
            description: "echo".to_string(),
            kind: ToolKind::default(),
            input_schema: serde_json::json!({ "type": "object" }),
        }
    }
    async fn call(&self, _args: serde_json::Value) -> Result<ToolCallContent> {
        *self.calls.lock().unwrap() += 1;
        Ok(ToolCallContent::from(ContentBlock::Text(TextContent::new(
            "echoed".to_string(),
        ))))
    }
}

/// A `Tool` whose `call` always errors — for the failure-path test.
pub struct FailTool {
    name: &'static str,
}

impl FailTool {
    pub fn new(name: &'static str) -> Self {
        Self { name }
    }
}

#[async_trait]
impl Tool for FailTool {
    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name.to_string(),
            description: "always fails".to_string(),
            kind: ToolKind::default(),
            input_schema: serde_json::json!({ "type": "object" }),
        }
    }
    async fn call(&self, _args: serde_json::Value) -> Result<ToolCallContent> {
        Err(manch_protocol::Error::Other("boom".to_string()))
    }
}

/// A `MemoryStore` backed by an in-memory Vec of role-tagged blocks.
/// `assemble_context` coalesces them into turns — the "dumbest strategy" #3
/// will also ship first.
pub struct VecStore {
    entries: Mutex<Vec<(Role, ContentBlock)>>,
}

impl VecStore {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
        }
    }
    /// Number of raw appended blocks (not turns).
    pub fn len(&self) -> usize {
        self.entries.lock().unwrap().len()
    }
}

#[async_trait]
impl MemoryStore for VecStore {
    async fn append(&self, _session_id: &str, role: Role, block: ContentBlock) -> Result<()> {
        self.entries.lock().unwrap().push((role, block));
        Ok(())
    }
    async fn assemble_context(&self, session_id: &str) -> Result<Context> {
        Ok(Context {
            session_id: session_id.to_string(),
            turns: coalesce_turns(self.entries.lock().unwrap().iter().cloned()),
        })
    }
}

/// An `EventSink` that records every emitted event for assertions.
#[derive(Clone, Default)]
pub struct CollectSink {
    pub events: Arc<Mutex<Vec<AgentEvent>>>,
}

impl CollectSink {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn events(&self) -> Vec<AgentEvent> {
        self.events.lock().unwrap().clone()
    }
}

#[async_trait]
impl EventSink for CollectSink {
    async fn emit(&self, event: AgentEvent) -> Result<()> {
        self.events.lock().unwrap().push(event);
        Ok(())
    }
}

#[cfg(test)]
mod smoke {
    use super::*;

    #[tokio::test]
    async fn mocks_are_usable() {
        let sink = Arc::new(CollectSink::new());
        let agent = ScriptAgent::new("m", vec![vec![AgentEvent::text_chunk("hi")]]);
        agent
            .prompt(
                Context {
                    session_id: "s".into(),
                    turns: vec![],
                },
                &[],
                sink.clone(),
            )
            .await
            .unwrap();
        assert_eq!(sink.events().len(), 1);
    }
}
