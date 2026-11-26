//! One authenticated client, two model rounds, host-owned tool results.
//! This example makes provider calls only when explicitly run with credentials.

#[cfg(feature = "gemini")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use manch_llm::{GeminiAgent, collect_completion};
    use manch_protocol::acp::{
        Content, ContentBlock, StopReason, TextContent, ToolCallContent, ToolKind,
    };
    use manch_protocol::{Context, Entry, Role, ToolSchema, Turn};

    let key = std::env::var("GEMINI_API_KEY")?;
    let model = std::env::var("MANCH_MODEL")?;
    // Construct once; credentials remain private to the agent. Choose the model
    // in the host. An endpoint override can point this at a local fixture/proxy.
    let agent = GeminiAgent::new(key, Some(model)).max_retries(0);
    let transport = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .read_timeout(std::time::Duration::from_secs(120))
        .retry(reqwest::retry::never())
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let agent = agent.with_http_client(transport);
    let tools = vec![ToolSchema {
        name: "echo".into(),
        description: "Return the supplied value".into(),
        kind: ToolKind::Other,
        input_schema: serde_json::json!({"type":"object","properties":{"value":{"type":"string"}},"required":["value"]}),
    }];
    let mut context = Context {
        session_id: "host-round".into(),
        turns: vec![
            Turn {
                role: Role::System,
                entries: vec![Entry::Block(ContentBlock::Text(TextContent::new(
                    "Call echo with value hello once, then summarize its result.",
                )))],
            },
            Turn {
                role: Role::User,
                entries: vec![Entry::Block(ContentBlock::Text(TextContent::new("Begin")))],
            },
        ],
    };
    let first = collect_completion(&agent, context.clone(), &tools).await?;
    if first.stop_reason != StopReason::EndTurn || first.tool_calls.is_empty() {
        return Err("the first round did not complete with a tool request".into());
    }
    let mut assistant = Vec::new();
    if !first.text.is_empty() {
        assistant.push(Entry::Block(ContentBlock::Text(TextContent::new(
            first.text,
        ))));
    }
    assistant.extend(first.tool_calls.iter().cloned().map(Entry::ToolCall));
    context.turns.push(Turn {
        role: Role::Assistant,
        entries: assistant,
    });
    let mut results = Vec::new();
    for call in first.tool_calls {
        // This validation and execution belongs to the host. A real host applies
        // its own authorization, checkpoints and budget policy here.
        if call.name != "echo" {
            return Err("unexpected host tool".into());
        }
        let value = call
            .arguments
            .get("value")
            .and_then(|v| v.as_str())
            .ok_or("invalid echo arguments")?;
        results.push(Entry::ToolResult {
            id: call.id,
            content: vec![ToolCallContent::Content(Content::new(ContentBlock::Text(
                TextContent::new(value),
            )))],
            raw_output: None,
        });
    }
    context.turns.push(Turn {
        role: Role::User,
        entries: results,
    });
    let second = collect_completion(&agent, context, &[]).await?;
    if second.stop_reason != StopReason::EndTurn || !second.tool_calls.is_empty() {
        return Err("the second round did not complete normally".into());
    }
    println!("{}", second.text);
    Ok(())
}

#[cfg(not(feature = "gemini"))]
fn main() {
    eprintln!("Enable the gemini feature to run this example.");
}
