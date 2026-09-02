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
        Content, ContentBlock, ContentChunk, PermissionOption, PermissionOptionId,
        PermissionOptionKind, PromptRequest, PromptResponse, RequestPermissionOutcome,
        RequestPermissionRequest, RequestPermissionResponse, SelectedPermissionOutcome, SessionId,
        SessionNotification, SessionUpdate, StopReason, TextContent, ToolCall, ToolCallContent,
        ToolCallId, ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields, ToolKind,
    };
}

use acp::{ContentBlock, StopReason};

mod memory;
mod permission;
mod tool;

pub use memory::{Entry, MemoryStore, Turn, coalesce_turns};
pub use permission::{AskOncePolicy, PermissionDecision, PermissionPolicy, kind_of, once_options};
pub use tool::{Extensions, Tier, Tool, ToolContext, ToolInvocation, ToolSchema};

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

/// Token counts reported by a provider for a turn. Both fields are optional
/// because the three provider dialects report them at different moments and not
/// always together — Anthropic sends input at `message_start` and output at
/// `message_delta`, Gemini repeats a running total per chunk, OpenAI sends one
/// final block. Manch forwards what it is told and does not accumulate; a
/// consumer that needs a running total sums the events it receives.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
}

/// A streamed unit of progress from an [`Agent`] during a turn.
///
/// `ToolInvocation` is intentionally small (id/name/args); `acp::SessionUpdate`
/// is ACP's own, much larger type. Boxing `Update`'s payload to close that gap
/// would ripple `Box`/deref through every call site across the workspace that
/// matches on it — out of proportion to the lint. Sizes are known at each
/// match, so the variance costs nothing but the enum's own stack slot.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// A streamed update in ACP's own vocabulary (content chunk, tool-call
    /// status, plan, …). Forwarded verbatim to the originating [`Channel`]/UI.
    Update(acp::SessionUpdate),
    /// **BYOK path only.** The model has requested a host-registered tool; the
    /// runtime must dispatch it via [`Tool::call`] and re-prompt with the result.
    ToolCall(ToolInvocation),
    /// Provider-reported token counts. Emitted as they arrive, so a turn may
    /// produce several. Never trusted for billing — a managed tier meters at its
    /// own proxy rather than believing a client-side total.
    Usage(Usage),
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

/// How a turn ended — or why it stopped early.
///
/// A [`Tier::Draft`] tool may not execute until a human has approved it, and a
/// human decision has no bounded duration: it can outlive an HTTP request, a
/// websocket, or the process itself. So the turn *suspends* rather than
/// awaiting inline. The caller holds the returned `request` (the question to
/// put to the human), `pending` (the exact action being asked about) and
/// `agent_id` for as long as the decision takes, then resumes with
/// `Manch::approve`.
///
/// Suspend/resume is the primitive because it is the more general of the two
/// shapes: a blocking approver is constructible from it (await a decision, then
/// resume), while stateless suspension is *not* constructible from a blocking
/// await. `manch-core` ships a blocking convenience wrapper on top.
// `RequestPermissionRequest` embeds ACP's own `ToolCallUpdate`, so
// `AwaitingApproval` is much larger than `Finished(StopReason)`. Boxing it
// would push a `Box`/deref through every consumer that matches on the outcome —
// out of proportion to the lint, and the same call made for `AgentEvent` and
// `Entry`. Sizes are known at each match, so the variance costs nothing but the
// enum's own stack slot.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum TurnOutcome {
    /// The turn ran to completion, with ACP's own reason.
    Finished(StopReason),
    /// A [`Tier::Draft`] tool is waiting on a human decision. Nothing about it
    /// has executed yet.
    AwaitingApproval {
        /// The permission question, in ACP's own `session/request_permission`
        /// vocabulary, so a UI renders it exactly as it renders an ACP agent's.
        request: acp::RequestPermissionRequest,
        /// The invocation the human is being shown. Resuming dispatches *this*
        /// value rather than re-prompting the model, so the action that runs is
        /// exactly the action that was approved.
        pending: ToolInvocation,
        /// The agent that proposed `pending`, so the resumed turn re-prompts
        /// the same one. Carried here rather than remembered by the runtime:
        /// the suspension may cross a process boundary, and a runtime that held
        /// per-session state between `handle` and `approve` would not survive
        /// it.
        agent_id: String,
    },
}

/// The runtime surface a [`Channel`] calls to drive a turn. Implemented by
/// `manch-core`; lives here so [`Channel`] implementations need not depend on the
/// runtime crate.
#[async_trait]
pub trait PromptHandler: Send + Sync {
    /// Drive one turn for `agent_id` in `session_id` with the inbound `message`,
    /// streaming progress to `sink`.
    ///
    /// `ext` is the host-supplied context handed to every [`Tool`] this turn
    /// dispatches. It is passed per call, not held by the runtime, so a
    /// request-scoped value (a tenant, a grant, a connection) belongs to the
    /// request that supplied it.
    ///
    /// Returns [`TurnOutcome`]: either the turn finished, or it suspended
    /// awaiting a human's permission decision.
    async fn handle(
        &self,
        agent_id: &str,
        session_id: &str,
        message: Vec<ContentBlock>,
        ext: Arc<Extensions>,
        sink: Arc<dyn EventSink>,
    ) -> Result<TurnOutcome>;
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
}
