//! # manch-core
//!
//! The Manch runtime: Agent/Tool/Channel registries + the prompt/tool loop.
//! Framework-free, domain-free — the seam gate. Implements
//! [`manch_protocol::PromptHandler`] over registered [`manch_protocol::Agent`]s,
//! [`manch_protocol::Tool`]s, and a [`manch_protocol::MemoryStore`].

#[cfg(test)]
mod testing;

mod builder;
mod dispatch;
mod store;
mod turn;

pub use store::MemStore;

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
pub use builder::ManchBuilder;
use dispatch::{Applied, Batch, Buffer};
use manch_protocol::acp;
use manch_protocol::acp::{ContentBlock, StopReason, TextContent};
use manch_protocol::{
    Agent, AgentEvent, Approver, Channel, Entry, Error, EventSink, Extensions, MemoryStore,
    PermissionPolicy, PromptHandler, Result, Role, Tool, ToolContext, ToolInvocation, ToolSchema,
    TurnOutcome,
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
    /// Decides whether (and how) a human is asked before a `Draft`-tier tool
    /// executes. Manch ships a seam ([`PermissionPolicy`]) and a safe default
    /// (always ask), not a permission policy of its own.
    ///
    /// Consulted by [`Manch::run_batch`] before any `Draft`-tier tool is
    /// dispatched.
    pub(crate) policy: Arc<dyn PermissionPolicy>,
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

    /// Resolve a registered agent, or say which id was missing.
    fn agent_for(&self, agent_id: &str) -> Result<Arc<dyn Agent>> {
        self.agents
            .get(agent_id)
            .cloned()
            .ok_or_else(|| Error::NotFound(agent_id.to_string()))
    }

    /// Resume a turn suspended by [`TurnOutcome::AwaitingApproval`].
    ///
    /// `pending` is dispatched **as given**. The model is never re-prompted to
    /// choose an action here, so the action that runs is exactly the action the
    /// human was shown — a re-prompt could yield a different one, which would
    /// make the confirmation meaningless.
    ///
    /// `ext` is supplied again rather than replayed from the call that
    /// suspended: the suspension may cross a process boundary, and a
    /// request-scoped grant should be rebuilt from the request that resumes the
    /// turn, not from the one that proposed it.
    ///
    /// An allow-kind outcome dispatches `pending`; a reject-kind records the
    /// refusal as a `ToolResult` so the model can respond to it;
    /// [`acp::RequestPermissionOutcome::Cancelled`] ends the turn. In the first
    /// two cases the loop then continues — re-prompting *after* a result is not
    /// re-deciding, because the approved action has already run. That
    /// continuation gets a fresh [`MAX_TOOL_ITERS`] budget: a human decision is
    /// not a loop iteration, and the cap exists to stop a model spinning
    /// unattended, which a resumed turn by definition is not.
    pub async fn approve(
        &self,
        agent_id: &str,
        session_id: &str,
        ext: Arc<Extensions>,
        pending: ToolInvocation,
        outcome: acp::RequestPermissionOutcome,
        sink: Arc<dyn EventSink>,
    ) -> Result<TurnOutcome> {
        let agent = self.agent_for(agent_id)?;
        let tool = self.tool_for(&pending)?;
        let cx = ToolContext::new(session_id, &pending.id, ext.clone());

        // Same rule as a batch: nothing is written until the outcome has been
        // applied without error.
        let mut buf = Buffer::new();
        let applied =
            dispatch::apply(tool.as_ref(), &cx, &pending, &outcome, &sink, &mut buf).await?;
        buf.flush(self.memory.as_ref(), session_id).await?;

        if matches!(applied, Applied::Cancelled) {
            sink.emit(AgentEvent::Done(StopReason::Cancelled)).await?;
            return Ok(TurnOutcome::Finished(StopReason::Cancelled));
        }

        self.drive(agent, agent_id, session_id, ext, sink).await
    }

    /// Drive prompt → tool → re-prompt until the turn finishes or suspends.
    async fn drive(
        &self,
        agent: Arc<dyn Agent>,
        agent_id: &str,
        session_id: &str,
        ext: Arc<Extensions>,
        sink: Arc<dyn EventSink>,
    ) -> Result<TurnOutcome> {
        let schemas: Vec<ToolSchema> = self.tools.values().map(|t| t.schema()).collect();

        for _ in 0..MAX_TOOL_ITERS {
            let ctx = self.memory.assemble_context(session_id).await?;
            let intercept = Arc::new(InterceptSink::new(sink.clone()));
            let stop = agent.prompt(ctx, &schemas, intercept.clone()).await?;

            if let Some(text) = intercept.take_text() {
                self.memory
                    .append(
                        session_id,
                        Role::Assistant,
                        Entry::Block(ContentBlock::Text(TextContent::new(text))),
                    )
                    .await?;
            }

            let calls = intercept.take_calls();
            if calls.is_empty() {
                sink.emit(AgentEvent::Done(stop)).await?;
                return Ok(TurnOutcome::Finished(stop));
            }

            // Results are buffered, never appended as they are produced. The `?`
            // below drops the buffer, so a call that errors mid-batch leaves no
            // partial record of the calls that ran before it.
            let mut buf = Buffer::new();
            let batch = self
                .run_batch(session_id, &ext, &sink, calls, &mut buf)
                .await?;
            // Past this point the batch resolved, so what it did buffer is final
            // and gets written — including on suspension, where the calls before
            // the suspending one have already run.
            buf.flush(self.memory.as_ref(), session_id).await?;
            match batch {
                Batch::Completed => {}
                Batch::Cancelled => {
                    sink.emit(AgentEvent::Done(StopReason::Cancelled)).await?;
                    return Ok(TurnOutcome::Finished(StopReason::Cancelled));
                }
                Batch::Suspended { request, pending } => {
                    return Ok(TurnOutcome::AwaitingApproval {
                        request: *request,
                        pending,
                        agent_id: agent_id.to_string(),
                    });
                }
            }
        }

        Err(Error::Other(format!(
            "tool-call loop exceeded {MAX_TOOL_ITERS} iterations"
        )))
    }

    /// Blocking convenience over [`Manch::handle`] / [`Manch::approve`] for a
    /// consumer that can hold a call open across a human decision — a desktop
    /// or CLI app, not a stateless server.
    ///
    /// This is a loop over the primitive, not a second control path: it holds
    /// no state that `handle`/`approve` do not already hold, and on
    /// suspension it passes back the exact `agent_id` and `pending` that
    /// [`TurnOutcome::AwaitingApproval`] handed it — never a reconstructed,
    /// cloned-and-rebuilt, or looked-up substitute — so the action that runs
    /// is exactly the action the [`Approver`] was shown. A consumer that
    /// needs to survive a process boundary (a stateless server that cannot
    /// hold a request open across a human's decision) uses `handle` /
    /// `approve` directly instead.
    pub async fn handle_with_approver(
        &self,
        agent_id: &str,
        session_id: &str,
        message: Vec<ContentBlock>,
        ext: Arc<Extensions>,
        approver: &dyn Approver,
        sink: Arc<dyn EventSink>,
    ) -> Result<StopReason> {
        let mut outcome = self
            .handle(agent_id, session_id, message, ext.clone(), sink.clone())
            .await?;
        loop {
            match outcome {
                TurnOutcome::Finished(stop) => return Ok(stop),
                TurnOutcome::AwaitingApproval {
                    agent_id,
                    request,
                    pending,
                } => {
                    let decision = approver.approve(request).await?;
                    // `agent_id` and `pending` come back out of the outcome
                    // itself, not from anything reconstructed or looked up:
                    // the action that runs is exactly the action the
                    // `Approver` was shown.
                    outcome = self
                        .approve(
                            &agent_id,
                            session_id,
                            ext.clone(),
                            pending,
                            decision,
                            sink.clone(),
                        )
                        .await?;
                }
            }
        }
    }
}

#[async_trait]
impl PromptHandler for Manch {
    // Persistence: inbound user blocks (User), the agent's own streamed text
    // (Assistant, accumulated per sub-turn in `InterceptSink`), the assistant's
    // tool_use request (Assistant, `Entry::ToolCall`), and the host-tool's
    // result (User, `Entry::ToolResult` — Anthropic's "tool_result lives in a
    // user turn" shape). The `ToolCall` is appended *before* its `ToolResult`
    // so a stored history is valid input for a second loop iteration —
    // Anthropic rejects a `tool_result` with no preceding `tool_use`.
    async fn handle(
        &self,
        agent_id: &str,
        session_id: &str,
        message: Vec<ContentBlock>,
        ext: Arc<Extensions>,
        sink: Arc<dyn EventSink>,
    ) -> Result<TurnOutcome> {
        let agent = self.agent_for(agent_id)?;

        for block in message {
            self.memory
                .append(session_id, Role::User, Entry::Block(block))
                .await?;
        }

        self.drive(agent, agent_id, session_id, ext, sink).await
    }
}
