//! # manch-core
//!
//! The Manch runtime: Agent/Tool/Channel registries + the prompt/tool loop.
//! Framework-free, domain-free — the seam gate. Implements
//! [`manch_protocol::PromptHandler`] over registered [`manch_protocol::Agent`]s,
//! [`manch_protocol::Tool`]s, and a [`manch_protocol::MemoryStore`].

#[cfg(test)]
mod testing;

mod builder;
mod store;
mod turn;

pub use store::MemStore;

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
pub use builder::ManchBuilder;
use manch_protocol::acp::{ContentBlock, StopReason, TextContent};
use manch_protocol::{
    Agent, AgentEvent, Channel, Entry, Error, EventSink, Extensions, MemoryStore, PromptHandler,
    Result, Role, Tool, ToolContext, ToolSchema,
};
use turn::InterceptSink;

/// Cap on prompt→tool→re-prompt cycles, guarding against a model that loops on
/// tool calls forever.
const MAX_TOOL_ITERS: usize = 8;

/// The Manch runtime. Cheap to clone (every field is `Arc`), so a `Channel` can
/// hold one and drive turns from its ingress loop.
#[derive(Clone)]
pub struct Manch {
    pub(crate) agents: Arc<HashMap<String, Arc<dyn Agent>>>,
    pub(crate) tools: Arc<HashMap<String, Arc<dyn Tool>>>,
    pub(crate) channels: Arc<HashMap<String, Arc<dyn Channel>>>,
    pub(crate) memory: Arc<dyn MemoryStore>,
}

impl std::fmt::Debug for Manch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Manch")
            .field("agents", &self.agents.len())
            .field("tools", &self.tools.len())
            .field("channels", &self.channels.len())
            .finish()
    }
}

impl Manch {
    /// Start building a runtime. A [`MemoryStore`] is required; agents, tools,
    /// and channels are optional and registered by their id.
    pub fn builder() -> ManchBuilder {
        ManchBuilder::default()
    }
}

#[async_trait]
impl PromptHandler for Manch {
    // Persistence: inbound user blocks (User), the agent's own streamed text
    // (Assistant, accumulated per sub-turn in `InterceptSink`), the assistant's
    // tool_use request (Assistant, `Entry::ToolCall`), and the host-tool's
    // result (User, `Entry::ToolResult` — Anthropic's "tool_result lives in a
    // user turn" shape). The `ToolCall` is appended *before* its `ToolResult`
    // so a stored history is valid input for a second loop iteration —
    // Anthropic rejects a `tool_result` with no preceding `tool_use`.
    async fn handle(
        &self,
        agent_id: &str,
        session_id: &str,
        message: Vec<ContentBlock>,
        sink: Arc<dyn EventSink>,
    ) -> Result<StopReason> {
        let agent = self
            .agents
            .get(agent_id)
            .ok_or_else(|| Error::NotFound(agent_id.to_string()))?
            .clone();

        for block in message {
            self.memory
                .append(session_id, Role::User, Entry::Block(block))
                .await?;
        }

        let schemas: Vec<ToolSchema> = self.tools.values().map(|t| t.schema()).collect();

        for _ in 0..MAX_TOOL_ITERS {
            let ctx = self.memory.assemble_context(session_id).await?;
            let intercept = Arc::new(InterceptSink::new(sink.clone()));
            let stop = agent.prompt(ctx, &schemas, intercept.clone()).await?;

            if let Some(text) = intercept.take_text() {
                self.memory
                    .append(
                        session_id,
                        Role::Assistant,
                        Entry::Block(ContentBlock::Text(TextContent::new(text))),
                    )
                    .await?;
            }

            let calls = intercept.take_calls();
            if calls.is_empty() {
                sink.emit(AgentEvent::Done(stop)).await?;
                return Ok(stop);
            }

            // Edge case (untested; only single-call turns are exercised today):
            // if a turn emits multiple tool calls and a later one errors, the
            // earlier results in this batch are already appended to memory
            // before the `?` below propagates the error.
            for inv in calls {
                let tool = self
                    .tools
                    .get(&inv.name)
                    .ok_or_else(|| Error::NotFound(inv.name.clone()))?;
                // Persist the request before dispatching it, so a mid-call error
                // still leaves a valid tool_use/tool_result pairing in history.
                self.memory
                    .append(session_id, Role::Assistant, Entry::ToolCall(inv.clone()))
                    .await?;
                // Task 7 threads the caller's Extensions through `handle`; until then the
                // context carries only what Manch itself knows.
                let cx = ToolContext::new(session_id, &inv.id, Arc::new(Extensions::default()));
                let result = tool.call(&cx, inv.arguments.clone()).await?;
                self.memory
                    .append(
                        session_id,
                        Role::User,
                        Entry::ToolResult {
                            id: inv.id.clone(),
                            content: vec![result],
                        },
                    )
                    .await?;
            }
        }

        Err(Error::Other(format!(
            "tool-call loop exceeded {MAX_TOOL_ITERS} iterations"
        )))
    }
}
