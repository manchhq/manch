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
use manch_protocol::acp::{
    Content, ContentBlock, ImageContent, SessionUpdate, TextContent, ToolCallContent, ToolKind,
};
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
    // Exact equality is deliberate. A Gemini 3 diagnostic named a call
    // `default_api:clinic_fact`, and if that prefix ever appears on the wire
    // rather than only in error text, `Manch::tool_for` cannot resolve it —
    // the registry is keyed on `schema().name`. This assertion is where that
    // would surface.
    assert_eq!(
        call.name, "get_current_weather",
        "{provider}: tool name did not round-trip. A namespace prefix here (e.g. \
         `default_api:get_current_weather`) would also break dispatch, which \
         resolves against the bare schema name."
    );
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

/// Drive a full two-turn tool loop: ask, take the model's call, hand back a
/// result, and ask again.
///
/// **The second turn is the point.** The first always succeeds, which is
/// exactly why offline tests and a single-turn live test both missed that
/// Gemini's thinking models attach a `thoughtSignature` to a function call and
/// reject the follow-up unless it is returned verbatim. Anything a provider
/// requires to be echoed shows up here and nowhere earlier.
///
/// Runs on the DEFAULT model deliberately: whatever `FALLBACK_MODEL` resolves
/// to is what a caller who did not choose will get.
async fn assert_a_tool_loop_completes_two_turns(agent: impl Agent, provider: &str) {
    let first = Arc::new(Collector::default());
    let question = "What is the current weather in Ahmedabad? Use the tool.";
    agent
        .prompt(ask(question), &[weather_tool()], first.clone())
        .await
        .unwrap_or_else(|e| panic!("{provider}: first turn failed — {e}"));

    let calls = first.tool_calls();
    assert_eq!(
        calls.len(),
        1,
        "{provider}: expected one call, got {calls:?}"
    );
    let call = calls[0].clone();

    // Replay the exchange the way `manch-core` would persist it: the user's
    // question, the assistant's call, then the result addressed back to it.
    let replay = Context {
        session_id: "live-smoke".to_string(),
        turns: vec![
            Turn {
                role: Role::User,
                entries: vec![Entry::Block(ContentBlock::Text(TextContent::new(
                    question.to_string(),
                )))],
            },
            Turn {
                role: Role::Assistant,
                entries: vec![Entry::ToolCall(call.clone())],
            },
            Turn {
                role: Role::User,
                entries: vec![Entry::ToolResult {
                    id: call.id.clone(),
                    content: vec![ToolCallContent::Content(Content::new(ContentBlock::Text(
                        TextContent::new("18 degrees Celsius and sunny".to_string()),
                    )))],
                }],
            },
        ],
    };

    let second = Arc::new(Collector::default());
    agent
        .prompt(replay, &[weather_tool()], second.clone())
        .await
        .unwrap_or_else(|e| {
            panic!(
                "{provider}: SECOND turn failed — the provider rejected the replayed history. \
                 If it names a missing signature, the call's provider_meta was not echoed. \
                 Error: {e}"
            )
        });

    assert!(
        !second.text().trim().is_empty(),
        "{provider}: the second turn produced no answer from the tool result"
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

#[tokio::test]
#[ignore = "live: needs ANTHROPIC_API_KEY"]
async fn anthropic_completes_a_two_turn_tool_loop() {
    assert_a_tool_loop_completes_two_turns(
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

#[tokio::test]
#[ignore = "live: needs OPENAI_API_KEY"]
async fn openai_completes_a_two_turn_tool_loop() {
    assert_a_tool_loop_completes_two_turns(OpenAiAgent::new(key("OPENAI_API_KEY"), None), "openai")
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

#[tokio::test]
#[ignore = "live: needs GEMINI_API_KEY"]
async fn gemini_completes_a_two_turn_tool_loop() {
    // The regression test for the thought-signature echo. On a thinking model
    // this fails with "Function call is missing a thought_signature in
    // functionCall parts" unless the captured provider_meta is sent back.
    assert_a_tool_loop_completes_two_turns(GeminiAgent::new(key("GEMINI_API_KEY"), None), "gemini")
        .await;
}

/// Two tools, so one question can force two calls in a single assistant turn.
fn clinic_tools() -> Vec<ToolSchema> {
    vec![
        ToolSchema {
            name: "whoami".to_string(),
            description: "Which clinic the caller is in.".to_string(),
            kind: ToolKind::Fetch,
            input_schema: serde_json::json!({ "type": "object", "properties": {} }),
        },
        ToolSchema {
            name: "bed_count".to_string(),
            description: "How many beds this clinic has.".to_string(),
            kind: ToolKind::Fetch,
            input_schema: serde_json::json!({ "type": "object", "properties": {} }),
        },
    ]
}

#[tokio::test]
#[ignore = "live: needs GEMINI_API_KEY"]
async fn gemini_completes_a_two_turn_loop_with_parallel_calls() {
    // Parallel calls are a different shape from a single call, and Gemini
    // attaches a thoughtSignature to only the FIRST call of a batch. Both calls
    // must therefore go back in ONE model turn with both results in ONE user
    // turn — splitting them into alternating turns puts the second call in a
    // turn of its own, where it has no signature and is rejected.
    let agent = GeminiAgent::new(key("GEMINI_API_KEY"), None);
    let tools = clinic_tools();
    let question = "Which clinic am I in, and how many beds does it have? Call both tools.";

    let first = Arc::new(Collector::default());
    agent
        .prompt(ask(question), &tools, first.clone())
        .await
        .unwrap_or_else(|e| panic!("gemini: first turn failed — {e}"));

    let calls = first.tool_calls();
    assert_eq!(
        calls.len(),
        2,
        "expected two parallel calls, got {calls:?} — re-run if the model chose not to"
    );

    let mut assistant = Vec::new();
    if !first.text().trim().is_empty() {
        assistant.push(Entry::Block(ContentBlock::Text(TextContent::new(
            first.text(),
        ))));
    }
    let mut results = Vec::new();
    for c in &calls {
        assistant.push(Entry::ToolCall(c.clone()));
        results.push(Entry::ToolResult {
            id: c.id.clone(),
            content: vec![ToolCallContent::Content(Content::new(ContentBlock::Text(
                TextContent::new("kaayantar, 23 beds".to_string()),
            )))],
        });
    }

    let replay = Context {
        session_id: "live-smoke".to_string(),
        turns: vec![
            Turn {
                role: Role::User,
                entries: vec![Entry::Block(ContentBlock::Text(TextContent::new(
                    question.to_string(),
                )))],
            },
            Turn {
                role: Role::Assistant,
                entries: assistant,
            },
            Turn {
                role: Role::User,
                entries: results,
            },
        ],
    };

    let second = Arc::new(Collector::default());
    agent
        .prompt(replay, &tools, second.clone())
        .await
        .unwrap_or_else(|e| panic!("gemini: SECOND turn failed with parallel calls — {e}"));
    assert!(!second.text().trim().is_empty());
}

// ---------------------------------------------------------------------------
// Vision: an image block has to survive the trip to the provider.
//
// The encoding half of this is unit-tested per provider, but "we serialised a
// data URL" and "the model saw pixels" are the same two different properties
// this file exists to separate — a provider that ignores an unrecognised
// content part returns a perfectly successful 200 describing nothing.
// ---------------------------------------------------------------------------

/// A 16×16 solid crimson PNG. Inline rather than a fixture file so the test is
/// self-contained and the bytes cannot drift.
const RED_PNG_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAABAAAAAQCAIAAACQkWg2AAAAFklEQVR42mO4IyJCEmIY1TCqYfhqAAACcQQQFd0BdQAAAABJRU5ErkJggg==";

/// One user turn carrying prose *and* an image, which is the ordering a real
/// OCR prompt has: an instruction, then the page it applies to.
fn ask_about_the_image(prompt: &str) -> Context {
    Context {
        session_id: "live-vision".to_string(),
        turns: vec![Turn {
            role: Role::User,
            entries: vec![
                Entry::Block(ContentBlock::Text(TextContent::new(prompt.to_string()))),
                Entry::Block(ContentBlock::Image(ImageContent::new(
                    RED_PNG_B64.to_string(),
                    "image/png".to_string(),
                ))),
            ],
        }],
    }
}

/// Asking for the colour is the cheapest question a model cannot answer without
/// the pixels: a turn where the image was silently dropped either errors or
/// answers about nothing, and neither one says "red".
async fn assert_an_image_reaches_the_model(agent: impl Agent, provider: &str) {
    let sink = Arc::new(Collector::default());
    agent
        .prompt(
            ask_about_the_image(
                "What colour is this image? Answer with a single word, no punctuation.",
            ),
            &[],
            sink.clone(),
        )
        .await
        .unwrap_or_else(|e| panic!("{provider}: prompt failed — {e}"));

    let text = sink.text().to_lowercase();
    assert!(
        text.contains("red") || text.contains("crimson"),
        "{provider}: the model did not describe the image it was sent — got {text:?}"
    );
}

#[tokio::test]
#[ignore = "live: needs ANTHROPIC_API_KEY"]
async fn anthropic_receives_an_image_block() {
    let agent = AnthropicAgent::new(key("ANTHROPIC_API_KEY"), None);
    assert_an_image_reaches_the_model(agent, "anthropic").await;
}

#[tokio::test]
#[ignore = "live: needs OPENAI_API_KEY"]
async fn openai_receives_an_image_block() {
    let agent = OpenAiAgent::new(key("OPENAI_API_KEY"), None);
    assert_an_image_reaches_the_model(agent, "openai").await;
}

#[tokio::test]
#[ignore = "live: needs GEMINI_API_KEY"]
async fn gemini_receives_an_image_block() {
    let agent = GeminiAgent::new(key("GEMINI_API_KEY"), None);
    assert_an_image_reaches_the_model(agent, "gemini").await;
}

// ---------------------------------------------------------------------------
// Catalogue: a provider's model list has to survive parsing.
//
// Unit tests pin the parser against a captured body, which cannot catch the
// failure that actually shipped — Fireworks borrowed OpenAI's parser, whose id
// heuristic matched none of its ids, so a perfectly healthy 200 with 25 models
// in it silently became a one-element fallback list.
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "live: needs FIREWORKS_API_KEY"]
async fn fireworks_returns_its_real_catalogue_not_just_the_fallback() {
    let models = manch_llm::fireworks::list_models(&key("FIREWORKS_API_KEY"))
        .await
        .expect("fireworks: list_models failed");

    assert!(
        models.len() > 1,
        "fireworks: got {} model(s) — a single entry means the catalogue was \
         filtered away and the fallback substituted, not that the account has \
         one model",
        models.len()
    );
    assert!(
        models.iter().any(|m| m.context_window.is_some()),
        "fireworks: not one model reported a context window, so the capability \
         fields are being parsed away"
    );
    assert!(
        models.iter().any(|m| m.supports_image_input == Some(true)),
        "fireworks: no vision-capable model found; the catalogue advertises \
         several, so this flag is not surviving the parse"
    );
}
