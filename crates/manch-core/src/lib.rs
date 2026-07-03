//! # manch-core
//!
//! The Manch runtime: Agent/Tool/Channel registries + the prompt/tool loop.
//! Framework-free, domain-free — the seam gate. Implements
//! [`manch_protocol::PromptHandler`] over registered [`manch_protocol::Agent`]s,
//! [`manch_protocol::Tool`]s, and a [`manch_protocol::MemoryStore`].

#[cfg(test)]
mod testing;

mod builder;
mod turn;

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
pub use builder::ManchBuilder;
use manch_protocol::acp::{ContentBlock, StopReason};
use manch_protocol::{
    Agent, AgentEvent, Channel, Error, EventSink, MemoryStore, PromptHandler, Result, Tool,
    ToolSchema,
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
            self.memory.append(session_id, block).await?;
        }

        let schemas: Vec<ToolSchema> = self.tools.values().map(|t| t.schema()).collect();

        // Task 3 interim: both arms below return unconditionally, so the loop
        // never actually iterates yet — clippy flags that correctly. Task 4
        // makes the "calls non-empty" arm `continue` after dispatch instead of
        // returning, so the loop becomes real; until then this is intentional
        // scaffolding, not a bug, so it's allowed rather than restructured.
        #[allow(clippy::never_loop)]
        for _ in 0..MAX_TOOL_ITERS {
            let ctx = self.memory.assemble_context(session_id).await?;
            let intercept = Arc::new(InterceptSink::new(sink.clone()));
            let stop = agent.prompt(ctx, &schemas, intercept.clone()).await?;

            let calls = intercept.take_calls();
            if calls.is_empty() {
                sink.emit(AgentEvent::Done(stop)).await?;
                return Ok(stop);
            }

            // Task 4 fills in dispatch here; until then, a captured call would
            // loop with no progress, so treat "no dispatch implemented" as done.
            sink.emit(AgentEvent::Done(stop)).await?;
            return Ok(stop);
        }

        Err(Error::Other(format!(
            "tool-call loop exceeded {MAX_TOOL_ITERS} iterations"
        )))
    }
}
