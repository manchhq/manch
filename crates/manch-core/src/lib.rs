//! # manch-core
//!
//! The Manch runtime: Agent/Tool/Channel registries + the prompt/tool loop.
//! Framework-free, domain-free — the seam gate. Implements
//! [`manch_protocol::PromptHandler`] over registered [`manch_protocol::Agent`]s,
//! [`manch_protocol::Tool`]s, and a [`manch_protocol::MemoryStore`].

#[cfg(test)]
mod testing;

mod builder;

use std::collections::HashMap;
use std::sync::Arc;

pub use builder::ManchBuilder;
use manch_protocol::{Agent, Channel, MemoryStore, Tool};

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
