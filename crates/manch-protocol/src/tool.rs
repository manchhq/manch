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

/// A model's request to run a host-registered [`Tool`], addressed by the
/// tool's `schema().name` — never by a display field. This is Manch's one
/// documented divergence from ACP (see the crate docs): ACP's `ToolCall` has
/// no `name` because ACP agents dispatch their own tools and only ever needed
/// a type for *reporting* a call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolInvocation {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
    /// Provider-specific data that must be handed back **verbatim** when this
    /// call is replayed to the same provider on a later turn. Captured when the
    /// call is parsed, echoed when the conversation is rebuilt, and never
    /// interpreted in between.
    ///
    /// It is deliberately opaque. Gemini's thinking models attach a
    /// `thoughtSignature` to a function call and reject the next turn if it is
    /// not returned; Anthropic's extended thinking has the same
    /// echo-it-back constraint. Naming either one here would teach
    /// `manch-protocol` a dialect's vocabulary and then be wrong for the other,
    /// so the provider owns the shape and Manch only carries it.
    ///
    /// **Not a general-purpose bag.** Anything that does not have to survive a
    /// round trip to satisfy a provider belongs somewhere else.
    ///
    /// `serde(default)` is load-bearing rather than tidiness: [`crate::Entry`]
    /// is `Deserialize`, so a durable [`crate::MemoryStore`] will already hold
    /// histories written before this field existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_meta: Option<serde_json::Value>,
}

/// Execution risk tier a [`Tool`] declares for itself. Governs auto-execution
/// vs. requiring caller confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// Read-only; safe to auto-execute.
    Read,
    /// Mutating; a caller may require confirmation before `call`.
    Draft,
}

/// **Extension point 2.** What an agent can *do*. **This is where domain products
/// plug in** (host-registered, BYOK path — see crate docs).
#[async_trait]
pub trait Tool: Send + Sync {
    /// The schema advertised to the model.
    fn schema(&self) -> ToolSchema;

    /// The execution risk tier this tool declares for itself. Required, with
    /// no default: a default of [`Tier::Read`] would hand auto-execution to
    /// any tool author who never considered the question.
    fn tier(&self) -> Tier;

    /// Preview what `call` would do, without doing it. Defaults to an empty
    /// proposal — inert, unlike a default `tier()`, which would be a
    /// permission grant.
    async fn propose(
        &self,
        _cx: &ToolContext,
        _args: &serde_json::Value,
    ) -> Result<Vec<ToolCallContent>> {
        Ok(vec![])
    }

    /// Execute the tool with model-supplied JSON arguments.
    async fn call(&self, cx: &ToolContext, args: serde_json::Value) -> Result<ToolCallContent>;
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

    #[test]
    fn an_invocation_stored_before_provider_meta_existed_still_deserialises() {
        // `Entry` is Deserialize and a durable MemoryStore will hold histories
        // written before this field existed. Without serde(default), adopting
        // it would break every stored conversation.
        let stored = r#"{"id":"c1","name":"search","arguments":{"q":"asha"}}"#;
        let inv: ToolInvocation = serde_json::from_str(stored).expect("old histories must load");
        assert_eq!(inv.id, "c1");
        assert!(inv.provider_meta.is_none());
    }

    #[test]
    fn an_invocation_without_provider_meta_does_not_serialise_the_key() {
        let inv = ToolInvocation {
            id: "c1".into(),
            name: "search".into(),
            arguments: serde_json::json!({}),
            provider_meta: None,
        };
        let json = serde_json::to_value(&inv).unwrap();
        assert!(json.get("provider_meta").is_none(), "got {json}");
    }
}
