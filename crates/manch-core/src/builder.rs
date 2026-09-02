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
    pub fn build(self) -> Result<Manch> {
        let memory = self
            .memory
            .ok_or_else(|| Error::Other("Manch::builder() requires a MemoryStore".to_string()))?;
        Ok(Manch {
            agents: Arc::new(self.agents),
            tools: Arc::new(self.tools),
            channels: Arc::new(self.channels),
            memory,
            policy: self.policy.unwrap_or_else(|| Arc::new(AskOncePolicy)),
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
