//! Live provider smoke tests. **Not run by CI.**
//!
//! Every other test in this crate is a pure unit test over request-body
//! encoding and SSE parsing. That is the right shape for those concerns, and it
//! is also why a fallback model that no provider recognises shipped: the body
//! was encoded exactly as documented and the API rejected it anyway. Encoding
//! correctly and being *accepted* are different properties, and only one of them
//! can be tested offline.
//!
//! These tests close that gap. They are `#[ignore]`d so they never run in CI and
//! cost nothing on a normal `cargo test`. Run them deliberately before a
//! release, or after touching a provider:
//!
//! ```sh
//! export ANTHROPIC_API_KEY=…  OPENAI_API_KEY=…  GEMINI_API_KEY=…
//! cargo test -p manch-llm --test live -- --ignored
//! ```
//!
//! A missing key is a hard failure rather than a skip. A test that passes
//! without testing anything is worse than no test — it manufactures confidence.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use manch_llm::{AnthropicAgent, GeminiAgent, OpenAiAgent};
use manch_protocol::acp::{ContentBlock, SessionUpdate, TextContent, ToolKind};
use manch_protocol::{
    Agent, AgentEvent, Context, Entry, EventSink, Result, Role, ToolSchema, Turn,
};

/// Records everything a turn emits, so a test can assert on what actually
/// arrived rather than on a mock's expectations.
#[derive(Default)]
struct Collector {
    events: Mutex<Vec<AgentEvent>>,
}

#[async_trait]
impl EventSink for Collector {
    async fn emit(&self, event: AgentEvent) -> Result<()> {
        self.events.lock().unwrap().push(event);
        Ok(())
    }
}

impl Collector {
    /// Concatenated assistant text across the turn.
    fn text(&self) -> String {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                AgentEvent::Update(SessionUpdate::AgentMessageChunk(c)) => match &c.content {
                    ContentBlock::Text(t) => Some(t.text.clone()),
                    _ => None,
                },
                _ => None,
            })
            .collect()
    }

    /// Every host-tool invocation the model asked for.
    fn tool_calls(&self) -> Vec<manch_protocol::ToolInvocation> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                AgentEvent::ToolCall(inv) => Some(inv.clone()),
                _ => None,
            })
            .collect()
    }
}

/// Read a key, failing loudly when it is absent. These tests are `#[ignore]`d,
/// so reaching this function means someone asked for them on purpose.
fn key(var: &str) -> String {
    std::env::var(var).unwrap_or_else(|_| {
        panic!("{var} is not set — live tests need real credentials. See this file's docs.")
    })
}

fn ask(prompt: &str) -> Context {
    Context {
        session_id: "live-smoke".to_string(),
        turns: vec![Turn {
            role: Role::User,
            entries: vec![Entry::Block(ContentBlock::Text(TextContent::new(
                prompt.to_string(),
            )))],
        }],
    }
}

/// A tool the model has every reason to call, with one required argument so a
/// call that arrives can be checked for more than its name.
fn weather_tool() -> ToolSchema {
    ToolSchema {
        name: "get_current_weather".to_string(),
        description: "Get the current weather in a given city.".to_string(),
        kind: ToolKind::Fetch,
        input_schema: serde_json::json!({
            "type": "object",
            "properties": { "city": { "type": "string", "description": "City name" } },
            "required": ["city"]
        }),
    }
}

/// Assert a turn produced usable prose. Proves the default model is one the
/// provider actually recognises, the endpoint resolves, and the stream parses.
async fn assert_default_agent_answers(agent: impl Agent, provider: &str) {
    let sink = Arc::new(Collector::default());
    agent
        .prompt(ask("Reply with exactly: pong"), &[], sink.clone())
        .await
        .unwrap_or_else(|e| panic!("{provider}: prompt failed — {e}"));

    let text = sink.text();
    assert!(
        !text.trim().is_empty(),
        "{provider}: the turn produced no assistant text at all"
    );
}

/// Assert the model was told a tool exists and chose to call it. This is the
/// path that has never run against a real provider: schema serialised into the
/// dialect's envelope, model emits a call, fragments reassembled into a
/// `ToolInvocation`.
async fn assert_tool_is_offered_and_called(agent: impl Agent, provider: &str) {
    let sink = Arc::new(Collector::default());
    agent
        .prompt(
            ask("What is the current weather in Ahmedabad? Use the tool."),
            &[weather_tool()],
            sink.clone(),
        )
        .await
        .unwrap_or_else(|e| panic!("{provider}: prompt failed — {e}"));

    let calls = sink.tool_calls();
    assert_eq!(
        calls.len(),
        1,
        "{provider}: expected exactly one tool call, got {calls:?}"
    );
    let call = &calls[0];
    assert_eq!(call.name, "get_current_weather", "{provider}: wrong tool");
    assert!(
        !call.id.is_empty(),
        "{provider}: the invocation carries no id, so a result cannot be paired to it"
    );
    assert!(
        call.arguments
            .get("city")
            .and_then(|c| c.as_str())
            .is_some(),
        "{provider}: arguments did not reassemble into an object with `city` — got {}",
        call.arguments
    );
}

// ── Anthropic ───────────────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "live: needs ANTHROPIC_API_KEY"]
async fn anthropic_default_agent_answers() {
    assert_default_agent_answers(
        AnthropicAgent::new(key("ANTHROPIC_API_KEY"), None),
        "anthropic",
    )
    .await;
}

#[tokio::test]
#[ignore = "live: needs ANTHROPIC_API_KEY"]
async fn anthropic_offers_a_tool_and_the_model_calls_it() {
    assert_tool_is_offered_and_called(
        AnthropicAgent::new(key("ANTHROPIC_API_KEY"), None),
        "anthropic",
    )
    .await;
}

// ── OpenAI ──────────────────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "live: needs OPENAI_API_KEY"]
async fn openai_default_agent_answers() {
    assert_default_agent_answers(OpenAiAgent::new(key("OPENAI_API_KEY"), None), "openai").await;
}

#[tokio::test]
#[ignore = "live: needs OPENAI_API_KEY"]
async fn openai_offers_a_tool_and_the_model_calls_it() {
    // OpenAI never marks an individual tool call finished, so this also proves
    // the end-of-stream flush completes the call rather than dropping it.
    assert_tool_is_offered_and_called(OpenAiAgent::new(key("OPENAI_API_KEY"), None), "openai")
        .await;
}

// ── Gemini ──────────────────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "live: needs GEMINI_API_KEY"]
async fn gemini_default_agent_answers() {
    // This is the test that would have caught the unusable default model: the
    // request body was encoded correctly and the API rejected the model name.
    assert_default_agent_answers(GeminiAgent::new(key("GEMINI_API_KEY"), None), "gemini").await;
}

#[tokio::test]
#[ignore = "live: needs GEMINI_API_KEY"]
async fn gemini_offers_a_tool_and_the_model_calls_it() {
    // Gemini supplies no call id, so the non-empty-id assertion also covers the
    // synthesised-id path.
    assert_tool_is_offered_and_called(GeminiAgent::new(key("GEMINI_API_KEY"), None), "gemini")
        .await;
}
