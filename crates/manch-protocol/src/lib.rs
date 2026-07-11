//! # manch-protocol
//!
//! The contracts for [Manch](https://github.com/manchhq/manch): the four traits
//! every consumer implements to extend the substrate — [`Agent`], [`Tool`],
//! [`Channel`], and [`MemoryStore`] — plus the shared message/event vocabulary.
//!
//! ## We build on ACP, we do not reinvent it
//!
//! The content and event vocabulary (text/image/resource blocks, tool-call
//! reporting, stop reasons, session updates) is already an open standard: the
//! [Agent Client Protocol](https://agentclientprotocol.com). Manch **re-exports**
//! those types from the official [`agent_client_protocol`] crate rather than
//! defining parallel ones. See [`acp`].
//!
//! ## The one place Manch and ACP differ: who owns tools
//!
//! ACP's model is **agent-owned tools**: an external agent (Claude Code, Gemini
//! CLI, …) runs its own tools and merely *reports* them via [`acp::ToolCall`] /
//! [`acp::ToolCallUpdate`]; the ACP *client* only authorizes/executes a fixed set
//! of client-side operations (filesystem, terminal) and grants permission. There
//! is **no mechanism in ACP for a host to register tool schemas the agent must
//! call.**
//!
//! Manch's [`Tool`] extension point is the opposite — it is **host-registered**.
//! This is deliberate, and it applies to exactly one of Manch's two agent paths:
//!
//! | Agent path | Who owns the tool loop | Does [`Tool`] apply? |
//! |------------|------------------------|----------------------|
//! | **BYOK / in-process** (raw model API: Claude, GPT, Gemini, Ollama) | `manch-core` runs `prompt → tool → re-prompt` and must *supply* tool schemas and *dispatch* calls | **Yes** — this is what [`Tool`] is for. |
//! | **ACP-hosted** (external agent over the wire via `manch-acp`) | the external agent owns its own loop; Manch is the ACP *client* and bridges events | **No** — Manch surfaces the agent's own [`acp::ToolCall`] reports; it does not inject host tools. |
//!
//! In both paths the *reporting* vocabulary is ACP's, so a UI renders tool
//! activity identically regardless of which path produced it.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Re-exported Agent Client Protocol vocabulary. Manch speaks ACP's types; it
/// does not define parallel content/event enums.
pub mod acp {
    pub use agent_client_protocol::schema::v1::{
        ContentBlock, ContentChunk, PromptRequest, PromptResponse, SessionNotification,
        SessionUpdate, StopReason, TextContent, ToolCall, ToolCallContent, ToolCallStatus,
        ToolCallUpdate, ToolCallUpdateFields, ToolKind,
    };
}

use acp::{ContentBlock, StopReason, ToolCall, ToolCallContent, ToolKind};

/// The error type returned across Manch's trait boundaries.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A requested agent / tool / channel id was not registered.
    #[error("not found: {0}")]
    NotFound(String),
    /// A tool received arguments it could not parse or validate.
    #[error("invalid tool arguments: {0}")]
    InvalidArguments(String),
    /// The underlying agent, transport, or store failed.
    #[error("{0}")]
    Other(String),
}

/// Convenience alias for fallible Manch operations.
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    User,
    Assistant,
}

/// One role-attributed span of the conversation: contiguous same-role blocks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Turn {
    pub role: Role,
    pub blocks: Vec<ContentBlock>,
}

/// Context assembled by a [`MemoryStore`] and handed to an [`Agent`] for a turn.
///
/// Role lives here, not in [`ContentBlock`]: ACP keeps author in its *streaming*
/// vocabulary, so a stored block can't say who spoke. Persistence and assembly
/// are Manch's seam, so the role dimension is Manch's to add.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Context {
    pub session_id: String,
    /// The conversation as role-tagged turns, oldest first.
    pub turns: Vec<Turn>,
}

/// Fold an ordered `(role, block)` log into [`Turn`]s by merging runs of the
/// same role. The one place turn-grouping lives, so every [`MemoryStore`]
/// coalesces identically.
pub fn coalesce_turns(items: impl IntoIterator<Item = (Role, ContentBlock)>) -> Vec<Turn> {
    let mut turns: Vec<Turn> = Vec::new();
    for (role, block) in items {
        match turns.last_mut() {
            Some(last) if last.role == role => last.blocks.push(block),
            _ => turns.push(Turn {
                role,
                blocks: vec![block],
            }),
        }
    }
    turns
}

/// A streamed unit of progress from an [`Agent`] during a turn.
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// A streamed update in ACP's own vocabulary (content chunk, tool-call
    /// status, plan, …). Forwarded verbatim to the originating [`Channel`]/UI.
    Update(acp::SessionUpdate),
    /// **BYOK path only.** The model has requested a host-registered tool; the
    /// runtime must dispatch it via [`Tool::call`] and re-prompt with the result.
    ToolCall(ToolCall),
    /// The turn finished.
    Done(StopReason),
}

impl AgentEvent {
    /// Convenience: an agent message text chunk in ACP vocabulary. The one place
    /// BYOK and ACP agents construct streamed text, so the ACP wrapping lives here.
    pub fn text_chunk(text: impl Into<String>) -> AgentEvent {
        use acp::{ContentChunk, SessionUpdate, TextContent};
        AgentEvent::Update(SessionUpdate::AgentMessageChunk(ContentChunk::new(
            ContentBlock::Text(TextContent::new(text.into())),
        )))
    }
}

/// Receives [`AgentEvent`]s as a turn streams. Implemented by the runtime; passed
/// down into [`Agent::prompt`].
#[async_trait]
pub trait EventSink: Send + Sync {
    async fn emit(&self, event: AgentEvent) -> Result<()>;
}

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

// ── The four extension points ───────────────────────────────────────────────

/// **Extension point 1.** How a model/agent is invoked and streams events back.
///
/// Implementations: a BYOK provider (Claude/GPT/Gemini), an ACP child process
/// (via `manch-acp`), a local Ollama model.
#[async_trait]
pub trait Agent: Send + Sync {
    /// Stable id used to address this agent in the registry.
    fn id(&self) -> &str;

    /// Run one turn. `tools` is the set of host-registered tools offered to the
    /// model (empty / ignored on the ACP-hosted path — see crate docs). Progress
    /// is streamed through `sink`; the final [`StopReason`] is also returned.
    async fn prompt(
        &self,
        context: Context,
        tools: &[ToolSchema],
        sink: Arc<dyn EventSink>,
    ) -> Result<StopReason>;
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

/// **Extension point 3.** How the outside world reaches an agent. ACP deliberately
/// does not cover transport/ingress, so this is wholly Manch's.
///
/// Implementations: CLI, Telegram, webhook.
#[async_trait]
pub trait Channel: Send + Sync {
    /// Stable id used to address this channel in the registry.
    fn id(&self) -> &str;

    /// Run the channel's ingress loop, forwarding inbound prompts to `handler`
    /// and streaming results back out over the channel's own transport.
    async fn serve(&self, handler: Arc<dyn PromptHandler>) -> Result<()>;
}

/// **Extension point 4.** How sessions persist and how context is assembled. ACP
/// deliberately does not cover persistence, so this is wholly Manch's.
///
/// Implementations: SQLite default (`manch-memory`); swap for Postgres or a
/// retrieval-backed strategy.
#[async_trait]
pub trait MemoryStore: Send + Sync {
    /// Append a role-tagged content block to a session's append-only history.
    async fn append(&self, session_id: &str, role: Role, block: ContentBlock) -> Result<()>;

    /// Assemble the context for the next turn. **The seam** — retrieval,
    /// summarisation, and compaction all live behind this one method.
    async fn assemble_context(&self, session_id: &str) -> Result<Context>;
}

/// The runtime surface a [`Channel`] calls to drive a turn. Implemented by
/// `manch-core`; lives here so [`Channel`] implementations need not depend on the
/// runtime crate.
#[async_trait]
pub trait PromptHandler: Send + Sync {
    /// Drive one turn for `agent_id` in `session_id` with the inbound `message`,
    /// streaming progress to `sink`.
    async fn handle(
        &self,
        agent_id: &str,
        session_id: &str,
        message: Vec<ContentBlock>,
        sink: Arc<dyn EventSink>,
    ) -> Result<StopReason>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use acp::SessionUpdate;

    #[test]
    fn text_chunk_wraps_delta_as_agent_message_chunk() {
        let ev = AgentEvent::text_chunk("New Delhi");
        match ev {
            AgentEvent::Update(SessionUpdate::AgentMessageChunk(chunk)) => match chunk.content {
                acp::ContentBlock::Text(t) => assert_eq!(t.text, "New Delhi"),
                _ => panic!("expected text content"),
            },
            _ => panic!("expected AgentMessageChunk update"),
        }
    }

    #[test]
    fn coalesce_merges_runs_of_same_role() {
        use acp::{ContentBlock, TextContent};
        let b = |s: &str| ContentBlock::Text(TextContent::new(s.to_string()));
        let turns = coalesce_turns([
            (Role::User, b("a")),
            (Role::User, b("b")),
            (Role::Assistant, b("c")),
            (Role::User, b("d")),
        ]);
        assert_eq!(turns.len(), 3);
        assert_eq!(turns[0].role, Role::User);
        assert_eq!(turns[0].blocks.len(), 2);
        assert_eq!(turns[1].role, Role::Assistant);
        assert_eq!(turns[2].role, Role::User);
        assert_eq!(turns[2].blocks.len(), 1);
    }
}
