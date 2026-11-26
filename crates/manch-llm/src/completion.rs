//! Collect the existing streaming contract without owning tools or persistence.

use async_trait::async_trait;
use manch_protocol::acp::{ContentBlock, SessionUpdate, StopReason};
use manch_protocol::{
    Agent, AgentEvent, Context, Error, EventSink, ProviderMetadata, Result, ToolInvocation,
    ToolSchema, Usage,
};
use std::sync::{Arc, Mutex};

/// Progress retained even when transport, cancellation or parsing interrupts a round.
#[derive(Debug, Clone, Default)]
pub struct PartialCompletion {
    pub text: String,
    pub thoughts: String,
    pub tool_calls: Vec<ToolInvocation>,
    pub usage: Usage,
    pub metadata: ProviderMetadata,
    pub stop_reason: Option<StopReason>,
}

/// A provider round with a recognized terminal status. Inspect `stop_reason`:
/// MaxTokens and Refusal are terminal outcomes, not complete usable answers.
#[derive(Debug, Clone)]
pub struct Completion {
    pub text: String,
    pub thoughts: String,
    pub tool_calls: Vec<ToolInvocation>,
    pub usage: Usage,
    /// Only reported/derivable when the provider distinguishes visible output.
    /// Raw provider output and reasoning counts remain independently available.
    pub visible_output_tokens: Option<u64>,
    pub metadata: ProviderMetadata,
    pub stop_reason: StopReason,
}

/// Errors retain inspectable partial work, but Debug/Display never print it.
pub struct CompletionFailure {
    pub error: Error,
    pub partial: Box<PartialCompletion>,
}
impl std::fmt::Debug for CompletionFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompletionFailure")
            .field("error", &self.error)
            .field("usage", &self.partial.usage)
            .finish_non_exhaustive()
    }
}
impl std::fmt::Display for CompletionFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.error, f)
    }
}
impl std::error::Error for CompletionFailure {}

/// A single-round sink. Retain an Arc to inspect usage after dropping a prompt
/// future at a host-owned deadline. Create a fresh collector for each round.
#[derive(Default)]
pub struct CompletionCollector(Mutex<PartialCompletion>);
impl CompletionCollector {
    pub fn snapshot(&self) -> PartialCompletion {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}
#[async_trait]
impl EventSink for CompletionCollector {
    async fn emit(&self, event: AgentEvent) -> Result<()> {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        match event {
            AgentEvent::Update(SessionUpdate::AgentMessageChunk(chunk)) => {
                if let ContentBlock::Text(text) = chunk.content {
                    state.text.push_str(&text.text);
                }
            }
            AgentEvent::Update(SessionUpdate::AgentThoughtChunk(chunk)) => {
                if let ContentBlock::Text(text) = chunk.content {
                    state.thoughts.push_str(&text.text);
                }
            }
            AgentEvent::ToolCall(call) => state.tool_calls.push(call),
            AgentEvent::Usage(usage) => {
                macro_rules! merge {($($field:ident),*)=>{$(if usage.$field.is_some(){state.usage.$field=usage.$field;})*};}
                merge!(
                    input_tokens,
                    output_tokens,
                    total_tokens,
                    thought_tokens,
                    cached_read_tokens,
                    cached_write_tokens
                );
            }
            AgentEvent::Metadata(meta) => {
                macro_rules! merge {($($field:ident),*)=>{$(if meta.$field.is_some(){state.metadata.$field=meta.$field;})*};}
                merge!(
                    provider,
                    requested_model,
                    reported_model,
                    response_id,
                    request_id
                );
            }
            AgentEvent::Done(stop) => state.stop_reason = Some(stop),
            _ => {}
        }
        Ok(())
    }
}

/// Run exactly one model invocation through the existing streaming path.
pub async fn collect_completion(
    agent: &dyn Agent,
    context: Context,
    tools: &[ToolSchema],
) -> std::result::Result<Completion, CompletionFailure> {
    let sink = Arc::new(CompletionCollector::default());
    let result = agent.prompt(context, tools, sink.clone()).await;
    let partial = sink.snapshot();
    let stop = match result {
        Ok(stop) if partial.stop_reason == Some(stop) => stop,
        Ok(_) => {
            return Err(CompletionFailure {
                error: crate::incomplete(),
                partial: Box::new(partial),
            });
        }
        Err(error) => {
            return Err(CompletionFailure {
                error,
                partial: Box::new(partial),
            });
        }
    };
    let visible_output_tokens = match partial.metadata.provider.as_deref() {
        Some("gemini") => partial.usage.output_tokens,
        Some("openai" | "fireworks") => partial
            .usage
            .output_tokens
            .zip(partial.usage.thought_tokens)
            .and_then(|(out, thought)| out.checked_sub(thought)),
        _ => None,
    };
    Ok(Completion {
        text: partial.text,
        thoughts: partial.thoughts,
        tool_calls: partial.tool_calls,
        usage: partial.usage,
        visible_output_tokens,
        metadata: partial.metadata,
        stop_reason: stop,
    })
}
