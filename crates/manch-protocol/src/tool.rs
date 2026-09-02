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
