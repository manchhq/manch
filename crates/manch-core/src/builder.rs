use std::collections::HashMap;
use std::sync::Arc;

use manch_protocol::{Agent, Channel, Error, MemoryStore, Result, Tool};

use crate::Manch;

/// Fluent builder for [`Manch`]. Registers agents/tools/channels by their id and
/// the sole required dependency, a [`MemoryStore`]. Duplicate ids: last wins.
#[derive(Default)]
pub struct ManchBuilder {
    agents: HashMap<String, Arc<dyn Agent>>,
    tools: HashMap<String, Arc<dyn Tool>>,
    channels: HashMap<String, Arc<dyn Channel>>,
    memory: Option<Arc<dyn MemoryStore>>,
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
    pub fn build(self) -> Result<Manch> {
        let memory = self
            .memory
            .ok_or_else(|| Error::Other("Manch::builder() requires a MemoryStore".to_string()))?;
        Ok(Manch {
            agents: Arc::new(self.agents),
            tools: Arc::new(self.tools),
            channels: Arc::new(self.channels),
            memory,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::Manch;
    use crate::testing::{EchoTool, ScriptAgent, VecStore};

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
            .tool(Arc::new(EchoTool::new("echo")))
            .memory(Arc::new(VecStore::new()))
            .build()
            .unwrap();
        assert!(manch.agents.contains_key("a"));
        assert!(manch.tools.contains_key("echo"));
    }
}
