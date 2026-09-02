use std::collections::HashMap;
use std::sync::Arc;

use manch_protocol::{
    Agent, AskOncePolicy, Channel, Error, MemoryStore, PermissionPolicy, Result, Tool,
};

use crate::Manch;

/// Fluent builder for [`Manch`]. Registers agents/tools/channels by their id and
/// the sole required dependency, a [`MemoryStore`]. Duplicate ids: last wins.
#[derive(Default)]
pub struct ManchBuilder {
    agents: HashMap<String, Arc<dyn Agent>>,
    tools: HashMap<String, Arc<dyn Tool>>,
    channels: HashMap<String, Arc<dyn Channel>>,
    memory: Option<Arc<dyn MemoryStore>>,
    policy: Option<Arc<dyn PermissionPolicy>>,
    max_tool_iters: Option<usize>,
}

impl ManchBuilder {
    pub fn agent(mut self, agent: Arc<dyn Agent>) -> Self {
        self.agents.insert(agent.id().to_string(), agent);
        self
    }
    pub fn tool(mut self, tool: Arc<dyn Tool>) -> Self {
        self.tools.insert(tool.schema().name, tool);
        self
    }
    pub fn channel(mut self, channel: Arc<dyn Channel>) -> Self {
        self.channels.insert(channel.id().to_string(), channel);
        self
    }
    pub fn memory(mut self, memory: Arc<dyn MemoryStore>) -> Self {
        self.memory = Some(memory);
        self
    }
    /// Sets the [`PermissionPolicy`] deciding whether (and how) a human is
    /// asked before a `Draft`-tier tool executes. Defaults to
    /// [`AskOncePolicy`] — Manch does not decide permission posture for
    /// consumers it has not met, so the shipped default always asks rather
    /// than silently allowing.
    pub fn permission_policy(mut self, policy: Arc<dyn PermissionPolicy>) -> Self {
        self.policy = Some(policy);
        self
    }
    /// Caps prompt → tool → re-prompt cycles within one turn. Defaults to
    /// [`crate::DEFAULT_MAX_TOOL_ITERS`]. Rejects `0`, which would mean a turn
    /// that can never call a tool.
    ///
    /// **This budget is per continuation, not per conversation.** A turn that
    /// suspends for a human decision and is resumed through
    /// [`Manch::approve`](crate::Manch::approve) starts a fresh budget — a
    /// human decision is not a model loop, so a person willing to keep
    /// approving should not be cut off by a cap meant to stop a model spinning.
    /// The consequence is that total steps across a suspend/resume cycle are
    /// bounded by the *human*, not by this number. If a hard ceiling per
    /// conversation is needed, count approvals on the calling side.
    pub fn max_tool_iters(mut self, n: usize) -> Self {
        self.max_tool_iters = Some(n);
        self
    }
    pub fn build(self) -> Result<Manch> {
        let memory = self
            .memory
            .ok_or_else(|| Error::Other("Manch::builder() requires a MemoryStore".to_string()))?;
        let max_tool_iters = self.max_tool_iters.unwrap_or(crate::DEFAULT_MAX_TOOL_ITERS);
        if max_tool_iters == 0 {
            return Err(Error::Other(
                "max_tool_iters must be at least 1; 0 means a turn that can never call a tool"
                    .to_string(),
            ));
        }
        Ok(Manch {
            agents: Arc::new(self.agents),
            tools: Arc::new(self.tools),
            channels: Arc::new(self.channels),
            memory,
            policy: self.policy.unwrap_or_else(|| Arc::new(AskOncePolicy)),
            max_tool_iters,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use manch_protocol::Tier;

    use crate::Manch;
    use crate::MemStore;
    use crate::testing::{EchoTool, ScriptAgent};

    #[test]
    fn the_step_cap_defaults_to_eight() {
        let manch = Manch::builder()
            .memory(Arc::new(MemStore::new()))
            .build()
            .unwrap();
        assert_eq!(manch.max_tool_iters, crate::DEFAULT_MAX_TOOL_ITERS);
    }

    #[test]
    fn max_tool_iters_rejects_zero() {
        // A cap of 0 means a turn that can never dispatch a tool — almost
        // certainly a mistake, and silently unreachable behaviour if allowed.
        let err = Manch::builder()
            .memory(Arc::new(MemStore::new()))
            .max_tool_iters(0)
            .build()
            .unwrap_err();
        assert!(
            err.to_string().contains('0') || err.to_string().contains("zero"),
            "got: {err}"
        );
    }

    #[test]
    fn build_requires_a_memory_store() {
        let err = Manch::builder()
            .agent(Arc::new(ScriptAgent::new("a", vec![])))
            .build()
            .unwrap_err();
        assert!(matches!(err, manch_protocol::Error::Other(_)));
    }

    #[test]
    fn build_succeeds_and_registers_by_id() {
        let manch = Manch::builder()
            .agent(Arc::new(ScriptAgent::new("a", vec![])))
            .tool(Arc::new(EchoTool::new(
                "echo",
                Tier::Read,
                Arc::new(Mutex::new(Vec::new())),
            )))
            .memory(Arc::new(MemStore::new()))
            .build()
            .unwrap();
        assert!(manch.agents.contains_key("a"));
        assert!(manch.tools.contains_key("echo"));
    }
}
