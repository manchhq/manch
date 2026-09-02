use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::Result;
use crate::acp::{ToolCallContent, ToolKind};

/// Describes a host-registered [`Tool`] to the model (BYOK path). Mirrors the
/// shape an LLM tool-use API expects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    /// ACP's tool taxonomy, so UIs categorise host tools and agent-owned tools alike.
    pub kind: ToolKind,
    /// JSON Schema for the tool's arguments.
    pub input_schema: serde_json::Value,
}

/// **Extension point 2.** What an agent can *do*. **This is where domain products
/// plug in** (host-registered, BYOK path — see crate docs).
#[async_trait]
pub trait Tool: Send + Sync {
    /// The schema advertised to the model.
    fn schema(&self) -> ToolSchema;

    /// Execute the tool with model-supplied JSON arguments.
    async fn call(&self, args: serde_json::Value) -> Result<ToolCallContent>;
}

/// Type-keyed storage for host-supplied context values passed to a tool at invocation.
///
/// A library cannot know its consumers' context types, and a type parameter would
/// propagate through `Manch`, `ManchBuilder`, `PromptHandler`, and `Channel` for
/// consumers who want no context at all, while preventing one runtime from hosting
/// tools with different contexts. This opaque type-keyed map decouples the host's
/// context shape from the framework's trait boundaries — each tool reads back only
/// the values it cares about, and the host supplies only what each invocation needs.
#[derive(Default)]
pub struct Extensions(
    std::collections::HashMap<std::any::TypeId, Box<dyn std::any::Any + Send + Sync>>,
);

impl Extensions {
    pub fn insert<T: std::any::Any + Send + Sync>(&mut self, value: T) -> &mut Self {
        self.0.insert(std::any::TypeId::of::<T>(), Box::new(value));
        self
    }
    pub fn get<T: std::any::Any + Send + Sync>(&self) -> Option<&T> {
        self.0
            .get(&std::any::TypeId::of::<T>())?
            .downcast_ref::<T>()
    }
}

/// Per-invocation context passed to a tool. Contains session and invocation ids
/// and host-supplied extensions.
pub struct ToolContext {
    pub session_id: String,
    pub invocation_id: String,
    extensions: std::sync::Arc<Extensions>,
}

impl ToolContext {
    pub fn new(
        session_id: impl Into<String>,
        invocation_id: impl Into<String>,
        extensions: std::sync::Arc<Extensions>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            invocation_id: invocation_id.into(),
            extensions,
        }
    }
    pub fn get<T: std::any::Any + Send + Sync>(&self) -> Option<&T> {
        self.extensions.get::<T>()
    }
}

#[cfg(test)]
mod context_tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct Scope(&'static str);
    struct Other;

    #[test]
    fn extensions_returns_the_value_that_was_inserted() {
        let mut ext = Extensions::default();
        ext.insert(Scope("clinic-7"));
        assert_eq!(ext.get::<Scope>(), Some(&Scope("clinic-7")));
    }

    #[test]
    fn extensions_returns_none_for_a_type_never_inserted() {
        let ext = Extensions::default();
        assert!(ext.get::<Other>().is_none());
    }

    #[test]
    fn inserting_the_same_type_twice_keeps_the_last_value() {
        let mut ext = Extensions::default();
        ext.insert(Scope("first"));
        ext.insert(Scope("second"));
        assert_eq!(ext.get::<Scope>(), Some(&Scope("second")));
    }

    #[test]
    fn tool_context_exposes_its_own_fields_and_the_host_extensions() {
        let mut ext = Extensions::default();
        ext.insert(Scope("clinic-7"));
        let cx = ToolContext::new("session-1", "call-1", std::sync::Arc::new(ext));
        assert_eq!(cx.session_id, "session-1");
        assert_eq!(cx.invocation_id, "call-1");
        assert_eq!(cx.get::<Scope>(), Some(&Scope("clinic-7")));
    }
}
