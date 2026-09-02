//! In-crate mocks for the runtime's unit tests. Not compiled outside `cfg(test)`.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use manch_protocol::acp::{
    Content, ContentBlock, StopReason, TextContent, ToolCallContent, ToolKind,
};
use manch_protocol::{
    Agent, AgentEvent, Context, EventSink, MemoryStore, Result, Tier, Tool, ToolContext, ToolSchema,
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

/// A `Tool` that records every call into a caller-owned log. The log is
/// injected rather than global so tests running in parallel cannot see each
/// other's calls.
pub struct EchoTool {
    name: String,
    tier: Tier,
    log: Arc<Mutex<Vec<String>>>,
}

impl EchoTool {
    pub fn new(name: &str, tier: Tier, log: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            name: name.to_string(),
            tier,
            log,
        }
    }
}

#[async_trait]
impl Tool for EchoTool {
    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name.clone(),
            description: String::new(),
            kind: ToolKind::Other,
            input_schema: serde_json::json!({ "type": "object" }),
        }
    }
    fn tier(&self) -> Tier {
        self.tier
    }
    async fn call(&self, _cx: &ToolContext, args: serde_json::Value) -> Result<ToolCallContent> {
        self.log.lock().unwrap().push(self.name.clone());
        Ok(ToolCallContent::Content(Content::new(ContentBlock::Text(
            TextContent::new(args.to_string()),
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
    fn tier(&self) -> Tier {
        Tier::Read
    }
    async fn call(&self, _cx: &ToolContext, _args: serde_json::Value) -> Result<ToolCallContent> {
        Err(manch_protocol::Error::Other("boom".to_string()))
    }
}

/// Wrap `s` as a standard-content [`ToolCallContent`] — the common case a
/// provider's tool result decodes into. Unused until the provider tests land
/// (Tasks 10-12).
#[allow(dead_code)]
pub fn text_content(s: &str) -> ToolCallContent {
    ToolCallContent::Content(Content::new(ContentBlock::Text(TextContent::new(
        s.to_string(),
    ))))
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
