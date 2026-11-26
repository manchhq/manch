//! Required acceptance fixtures: no network beyond loopback and no provider keys.
#![cfg(feature = "gemini")]

use async_trait::async_trait;
use manch_llm::GeminiAgent;
use manch_protocol::acp::{ContentBlock, TextContent};
use manch_protocol::{Agent, AgentEvent, Context, Entry, EventSink, Role, Turn};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Default)]
struct Sink(Mutex<Vec<AgentEvent>>);
#[async_trait]
impl EventSink for Sink {
    async fn emit(&self, event: AgentEvent) -> manch_protocol::Result<()> {
        self.0.lock().unwrap().push(event);
        Ok(())
    }
}

fn context() -> Context {
    Context {
        session_id: "fixture".into(),
        turns: vec![Turn {
            role: Role::User,
            entries: vec![Entry::Block(ContentBlock::Text(TextContent::new("hi")))],
        }],
    }
}
fn text_sse(text: &str) -> String {
    format!(
        "data: {}\n\n",
        serde_json::json!({"candidates":[{"content":{"parts":[{"text":text}]},"finishReason":"STOP"}]})
    )
}

struct Fixture {
    base: String,
    requests: Arc<Mutex<Vec<String>>>,
    connections: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Fixture {
    async fn new(replies: Vec<(u16, String)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let connections = Arc::new(AtomicUsize::new(0));
        let seen = requests.clone();
        let count = connections.clone();
        let replies = Arc::new(replies);
        let task = tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    break;
                };
                count.fetch_add(1, Ordering::SeqCst);
                let seen = seen.clone();
                let replies = replies.clone();
                tokio::spawn(async move {
                    let mut buf = Vec::new();
                    loop {
                        let boundary = loop {
                            if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                                break pos + 4;
                            }
                            let mut chunk = [0; 4096];
                            match sock.read(&mut chunk).await {
                                Ok(0) | Err(_) => return,
                                Ok(n) => buf.extend_from_slice(&chunk[..n]),
                            }
                        };
                        let head = String::from_utf8_lossy(&buf[..boundary]);
                        let len: usize = head
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .and_then(|n| n.trim().parse().ok())
                            })
                            .unwrap_or(0);
                        while buf.len() < boundary + len {
                            let mut chunk = [0; 4096];
                            match sock.read(&mut chunk).await {
                                Ok(0) | Err(_) => return,
                                Ok(n) => buf.extend_from_slice(&chunk[..n]),
                            }
                        }
                        let request =
                            String::from_utf8(buf.drain(..boundary + len).collect()).unwrap();
                        let index = {
                            let mut seen = seen.lock().unwrap();
                            let n = seen.len();
                            seen.push(request);
                            n
                        };
                        let (status, body) = replies
                            .get(index)
                            .unwrap_or_else(|| replies.last().unwrap());
                        if *status == 0 {
                            tokio::time::sleep(Duration::from_secs(10)).await;
                            return;
                        }
                        let redirect = if *status == 302 {
                            format!("Location: {body}\r\n")
                        } else {
                            String::new()
                        };
                        let response = format!(
                            "HTTP/1.1 {status} Fixture\r\nContent-Type: text/event-stream\r\n{redirect}x-request-id: request-1\r\nContent-Length: {}\r\n\r\n{body}",
                            body.len()
                                + if *status == 299 || *status == 298 {
                                    20
                                } else {
                                    0
                                }
                        );
                        if sock.write_all(response.as_bytes()).await.is_err() {
                            return;
                        }
                        if *status == 299 {
                            return;
                        }
                        if *status == 298 {
                            tokio::time::sleep(Duration::from_secs(10)).await;
                            return;
                        }
                    }
                });
            }
        });
        Self {
            base,
            requests,
            connections,
            task,
        }
    }
    fn bodies(&self) -> Vec<serde_json::Value> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .map(|r| serde_json::from_str(r.split_once("\r\n\r\n").unwrap().1).unwrap())
            .collect()
    }
}

#[tokio::test]
async fn injected_transport_is_reused_across_rounds() {
    let f = Fixture::new(vec![(200, text_sse("ok"))]).await;
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        "x-host-transport",
        reqwest::header::HeaderValue::from_static("injected"),
    );
    // Existing construction installs the crypto provider, as real callers may.
    let agent = GeminiAgent::new("test-key".into(), Some("chosen-model".into()));
    let client = reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .unwrap();
    let agent = agent
        .base_url(&f.base)
        .with_http_client(client)
        .max_retries(0);
    for _ in 0..2 {
        agent
            .prompt(context(), &[], Arc::new(Sink::default()))
            .await
            .unwrap();
    }
    assert_eq!(f.connections.load(Ordering::SeqCst), 1);
    assert!(
        f.requests
            .lock()
            .unwrap()
            .iter()
            .all(|r| r.contains("x-host-transport: injected"))
    );
}

#[tokio::test]
async fn provider_failures_are_classified_and_redacted() {
    for (status, kind) in [
        (401, "Authentication"),
        (403, "Authentication"),
        (429, "RateLimit"),
        (500, "Server"),
    ] {
        let f = Fixture::new(vec![(
            status,
            r#"{"error":{"message":"secret-key private-document"}}"#.into(),
        )])
        .await;
        let error = GeminiAgent::new("secret-key".into(), None)
            .base_url(&f.base)
            .max_retries(0)
            .prompt(context(), &[], Arc::new(Sink::default()))
            .await
            .unwrap_err();
        let public = format!("{error:?} {error}");
        assert!(
            !public.contains("secret-key") && !public.contains("private-document"),
            "{public}"
        );
        assert!(
            public.contains(kind),
            "missing failure category {kind}: {public}"
        );
    }
}

#[tokio::test]
async fn header_timeout_does_not_replay_unknown_acceptance() {
    let f = Fixture::new(vec![(0, String::new())]).await;
    let error = GeminiAgent::new("k".into(), None)
        .base_url(&f.base)
        .read_timeout(Duration::from_millis(30))
        .max_retries(2)
        .prompt(context(), &[], Arc::new(Sink::default()))
        .await
        .unwrap_err();
    assert!(matches!(error, manch_protocol::Error::Timeout(_)));
    assert_eq!(f.requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn streamed_errors_and_bad_urls_do_not_expose_inputs() {
    let f = Fixture::new(vec![(
        200,
        "data: {\"error\":{\"message\":\"secret-key private-document\"}}\n\n".into(),
    )])
    .await;
    for base in [f.base.as_str(), "http://[private-document-secret-key"] {
        let error = GeminiAgent::new("secret-key".into(), None)
            .base_url(base)
            .max_retries(0)
            .prompt(context(), &[], Arc::new(Sink::default()))
            .await
            .unwrap_err();
        let public = format!("{error:?} {error}");
        assert!(
            !public.contains("secret-key") && !public.contains("private-document"),
            "{public}"
        );
    }
}

#[tokio::test]
async fn collector_keeps_latest_known_usage_fields_and_identity() {
    let frame = r#"data: {"responseId":"response-1","modelVersion":"reported-model","candidates":[{"content":{"parts":[{"text":"ok"}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":21,"candidatesTokenCount":3,"thoughtsTokenCount":158,"totalTokenCount":182}}

data: {"usageMetadata":{"candidatesTokenCount":3,"totalTokenCount":182}}

"#;
    let f = Fixture::new(vec![(200, frame.into())]).await;
    let agent = GeminiAgent::new("k".into(), Some("requested-model".into())).base_url(&f.base);
    let result = manch_llm::collect_completion(&agent, context(), &[])
        .await
        .unwrap();
    assert_eq!(result.text, "ok");
    assert_eq!(result.usage.input_tokens, Some(21));
    assert_eq!(result.usage.output_tokens, Some(3));
    assert_eq!(result.usage.thought_tokens, Some(158));
    assert_eq!(result.usage.total_tokens, Some(182));
    assert_eq!(result.usage.cached_write_tokens, None);
    assert_eq!(result.metadata.provider.as_deref(), Some("gemini"));
    assert_eq!(
        result.metadata.requested_model.as_deref(),
        Some("requested-model")
    );
    assert_eq!(
        result.metadata.reported_model.as_deref(),
        Some("reported-model")
    );
    assert_eq!(result.metadata.request_id.as_deref(), Some("request-1"));
    assert_eq!(result.metadata.response_id.as_deref(), Some("response-1"));
    assert!(
        !serde_json::to_string(&f.bodies()[0])
            .unwrap()
            .contains("\"k\"")
    );
}

#[tokio::test]
async fn missing_or_invalid_terminal_state_is_incomplete() {
    for body in [
        String::new(),
        "data: not-json\n\n".into(),
        "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"partial\"}]}}]}\n\n".into(),
        "data: {\"candidates\":[{\"finishReason\":\"NEW_UNKNOWN_REASON\"}]}\n\n".into(),
        format!("{}data: {{\"unfinished\":", text_sse("partial")),
    ] {
        let f = Fixture::new(vec![(200, body)]).await;
        let result = manch_llm::collect_completion(
            &GeminiAgent::new("k".into(), None).base_url(&f.base),
            context(),
            &[],
        )
        .await;
        assert!(result.is_err(), "incomplete stream accepted: {result:?}");
    }
}

#[tokio::test]
async fn collector_preserves_partial_usage_on_failure() {
    let body = "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"private-document\"}]}}],\"usageMetadata\":{\"promptTokenCount\":21}}\n\n";
    let f = Fixture::new(vec![(299, body.into())]).await;
    let agent = GeminiAgent::new("k".into(), None)
        .base_url(&f.base)
        .max_retries(2);
    let failure = manch_llm::collect_completion(&agent, context(), &[])
        .await
        .unwrap_err();
    assert_eq!(failure.partial.text, "private-document");
    assert_eq!(failure.partial.usage.input_tokens, Some(21));
    assert_eq!(failure.partial.stop_reason, None);
    assert!(!format!("{failure:?} {failure}").contains("private-document"));
    assert_eq!(f.requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn host_cancellation_keeps_caller_owned_usage_snapshot() {
    let f = Fixture::new(vec![(
        298,
        "data: {\"usageMetadata\":{\"promptTokenCount\":21}}\n\n".into(),
    )])
    .await;
    let agent = GeminiAgent::new("k".into(), None).base_url(&f.base);
    let collector = Arc::new(manch_llm::CompletionCollector::default());
    assert!(
        tokio::time::timeout(
            Duration::from_millis(100),
            agent.prompt(context(), &[], collector.clone())
        )
        .await
        .is_err()
    );
    assert_eq!(collector.snapshot().usage.input_tokens, Some(21));
    assert_eq!(collector.snapshot().stop_reason, None);
    assert_eq!(f.requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn truncation_and_refusal_remain_distinct_from_completion() {
    use manch_protocol::acp::StopReason;
    for (wire, expected, text) in [
        ("MAX_TOKENS", StopReason::MaxTokens, ""),
        ("MAX_TOKENS", StopReason::MaxTokens, "partial"),
        ("SAFETY", StopReason::Refusal, ""),
    ] {
        let f=Fixture::new(vec![(200,format!("data: {}\n\n",serde_json::json!({"candidates":[{"content":{"parts":[{"text":text}]},"finishReason":wire}]})))]).await;
        let result = manch_llm::collect_completion(
            &GeminiAgent::new("k".into(), None).base_url(&f.base),
            context(),
            &[],
        )
        .await
        .unwrap();
        assert_eq!(result.stop_reason, expected);
        assert_eq!(result.text, text);
    }
}

#[tokio::test]
async fn typed_gemini_settings_reach_the_configured_model() {
    use manch_llm::{GenerationOptions, ModelPolicy, ReasoningOptions};
    let f = Fixture::new(vec![(200, text_sse("ok"))]).await;
    let policy = ModelPolicy {
        temperature: Some(true),
        top_p: Some(true),
        top_k: Some(true),
        reasoning: vec![ReasoningOptions::GeminiBudget(0).kind()],
        max_output_tokens: Some(32768),
        ..ModelPolicy::new("gemini", "chosen-model")
    };
    let agent = GeminiAgent::new("k".into(), Some("chosen-model".into()))
        .base_url(&f.base)
        .generation_options(
            GenerationOptions {
                max_output_tokens: Some(16384),
                temperature: Some(0.2),
                top_p: Some(0.9),
                top_k: Some(20),
                reasoning: Some(ReasoningOptions::GeminiBudget(1024)),
            },
            policy,
        )
        .unwrap();
    manch_llm::collect_completion(&agent, context(), &[])
        .await
        .unwrap();
    let body = &f.bodies()[0];
    assert_eq!(
        body["generationConfig"],
        serde_json::json!({"maxOutputTokens":16384,"temperature":0.2,"topP":0.9,"topK":20,"thinkingConfig":{"thinkingBudget":1024}})
    );
    assert!(
        f.requests.lock().unwrap()[0]
            .starts_with("POST /models/chosen-model:streamGenerateContent?alt=sse ")
    );
}

#[tokio::test]
async fn unsupported_or_unknown_requested_option_fails_before_http() {
    use manch_llm::{GenerationOptions, ModelPolicy, ReasoningOptions};
    let options = [
        GenerationOptions {
            temperature: Some(0.2),
            ..Default::default()
        },
        GenerationOptions {
            top_p: Some(f64::NAN),
            ..Default::default()
        },
        GenerationOptions {
            top_k: Some(0),
            ..Default::default()
        },
        GenerationOptions {
            max_output_tokens: Some(0),
            ..Default::default()
        },
        GenerationOptions {
            max_output_tokens: Some(32769),
            ..Default::default()
        },
        GenerationOptions {
            reasoning: Some(ReasoningOptions::CompatibleBudget(1024)),
            ..Default::default()
        },
    ];
    for option in options {
        let result = GeminiAgent::new("k".into(), Some("chosen-model".into())).generation_options(
            option,
            ModelPolicy {
                max_output_tokens: Some(32768),
                ..ModelPolicy::new("gemini", "chosen-model")
            },
        );
        assert!(result.is_err());
    }
    assert!(
        GeminiAgent::new("k".into(), Some("chosen-model".into()))
            .generation_options(
                GenerationOptions::default(),
                ModelPolicy::new("gemini", "wrong-model")
            )
            .is_err()
    );
    let f = Fixture::new(vec![(200, text_sse("ok"))]).await;
    let agent = GeminiAgent::new("k".into(), Some("chosen-model".into()))
        .base_url(&f.base)
        .generation_options(
            GenerationOptions::default(),
            ModelPolicy {
                max_output_tokens: Some(100),
                ..ModelPolicy::new("gemini", "chosen-model")
            },
        )
        .unwrap()
        .max_output_tokens(101);
    assert!(
        agent
            .prompt(context(), &[], Arc::new(Sink::default()))
            .await
            .is_err()
    );
    assert!(f.requests.lock().unwrap().is_empty());
}

#[cfg(feature = "openai")]
#[tokio::test]
async fn openai_and_fireworks_settings_use_their_own_wire_fields() {
    use manch_llm::{
        GenerationOptions, ModelPolicy, OpenAiAgent, ReasoningEffort, ReasoningOptions,
    };
    let response = "data: {\"id\":\"o1\",\"model\":\"reported\",\"choices\":[{\"delta\":{\"reasoning_content\":\"private\",\"content\":\"ok\"},\"finish_reason\":\"stop\"}],\"usage\":{\"completion_tokens\":12,\"completion_tokens_details\":{\"reasoning_tokens\":10}}}\n\ndata: [DONE]\n\n";
    for (provider, reasoning, key, value) in [
        (
            "openai",
            ReasoningOptions::OpenAiEffort(ReasoningEffort::Low),
            "max_completion_tokens",
            serde_json::json!("low"),
        ),
        (
            "fireworks",
            ReasoningOptions::CompatibleBudget(8192),
            "max_tokens",
            serde_json::json!(8192),
        ),
    ] {
        let f = Fixture::new(vec![(200, response.into())]).await;
        let policy = ModelPolicy {
            reasoning: vec![reasoning.kind()],
            ..ModelPolicy::new(provider, "chosen-model")
        };
        let agent = OpenAiAgent::compatible(
            provider,
            provider,
            "k".into(),
            Some("chosen-model".into()),
            &f.base,
            "fallback",
        )
        .generation_options(
            GenerationOptions {
                max_output_tokens: Some(16384),
                reasoning: Some(reasoning),
                ..Default::default()
            },
            policy,
        )
        .unwrap();
        let result = manch_llm::collect_completion(&agent, context(), &[])
            .await
            .unwrap();
        let body = &f.bodies()[0];
        assert_eq!(body["model"], "chosen-model");
        assert_eq!(body[key], 16384);
        assert_eq!(body["reasoning_effort"], value);
        assert!(body.get("top_k").is_none());
        assert_eq!(result.metadata.provider.as_deref(), Some(provider));
        assert_eq!(result.text, "ok");
        assert_eq!(result.visible_output_tokens, Some(2));
        assert_eq!(result.thoughts, "private");
    }
}

fn link(uri: &str, mime: Option<&str>, name: &str) -> ContentBlock {
    // Use ACP's serialized contract so the test also covers its wire representation.
    serde_json::from_value(
        serde_json::json!({"type":"resource_link","uri":uri,"mimeType":mime,"name":name}),
    )
    .unwrap()
}

#[tokio::test]
async fn prepared_documents_preserve_order_and_labels() {
    let f = Fixture::new(vec![(200, text_sse("ok"))]).await;
    let mut ctx = context();
    ctx.turns[0].entries.extend([
        Entry::Block(serde_json::from_value(serde_json::json!({"type":"resource","resource":{"uri":"inline","mimeType":"application/pdf","blob":"cGRm"}})).unwrap()),
        Entry::Block(link("https://files.example/a",Some("application/pdf"),"source A")),
        Entry::Block(link("https://files.example/b",Some("image/png"),"source B")),
        Entry::Block(ContentBlock::Text(TextContent::new("after documents"))),
    ]);
    manch_llm::collect_completion(
        &GeminiAgent::new("k".into(), None).base_url(&f.base),
        ctx,
        &[],
    )
    .await
    .unwrap();
    assert_eq!(
        f.bodies()[0]["contents"][0]["parts"],
        serde_json::json!([
            {"text":"hi"},{"inlineData":{"mimeType":"application/pdf","data":"cGRm"}},
            {"fileData":{"fileUri":"https://files.example/a","mimeType":"application/pdf","displayName":"source A"}},
            {"fileData":{"fileUri":"https://files.example/b","mimeType":"image/png","displayName":"source B"}},
            {"text":"after documents"}
        ])
    );
}

#[tokio::test]
async fn unsupported_blocks_fail_before_http() {
    let audio: ContentBlock = serde_json::from_value(
        serde_json::json!({"type":"audio","data":"YWJj","mimeType":"audio/wav"}),
    )
    .unwrap();
    for block in [
        audio,
        link("https://private-document", None, "private-document"),
        link(
            "https://private-document",
            Some("video/mp4"),
            "private-document",
        ),
        link("", Some("application/pdf"), "private-document"),
    ] {
        let f = Fixture::new(vec![(200, text_sse("ok"))]).await;
        let mut ctx = context();
        ctx.turns[0].entries.push(Entry::Block(block));
        let error = GeminiAgent::new("k".into(), None)
            .base_url(&f.base)
            .prompt(ctx, &[], Arc::new(Sink::default()))
            .await
            .unwrap_err();
        assert!(matches!(error, manch_protocol::Error::Unsupported(_)));
        assert!(!format!("{error:?}").contains("private-document"));
        assert!(f.requests.lock().unwrap().is_empty());
    }
    let mut ctx = context();
    ctx.turns[0].role = Role::System;
    ctx.turns[0].entries.push(Entry::Block(link(
        "https://files.example/a",
        Some("application/pdf"),
        "a",
    )));
    assert!(
        GeminiAgent::new("k".into(), None)
            .prompt(ctx, &[], Arc::new(Sink::default()))
            .await
            .is_err()
    );
}

#[cfg(feature = "openai")]
#[tokio::test]
async fn native_document_references_are_not_portable() {
    let f = Fixture::new(vec![(200, text_sse("ok"))]).await;
    let mut ctx = context();
    ctx.turns[0].entries.push(Entry::Block(link(
        "https://files.example/a",
        Some("application/pdf"),
        "a",
    )));
    let result = manch_llm::OpenAiAgent::new("k".into(), None)
        .base_url(&f.base)
        .prompt(ctx, &[], Arc::new(Sink::default()))
        .await;
    assert!(matches!(result, Err(manch_protocol::Error::Unsupported(_))));
    assert!(f.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn host_supplies_ordered_results_and_signature_on_the_next_round() {
    use manch_protocol::ToolSchema;
    use manch_protocol::acp::{Content, ToolCallContent, ToolKind};
    let calls = r#"data: {"candidates":[{"content":{"parts":[{"functionCall":{"id":"native-a","name":"echo","args":{"value":"first"}},"thoughtSignature":"signature-a"},{"functionCall":{"id":"native-b","name":"echo","args":{"value":"second"}}}]},"finishReason":"STOP"}]}

"#;
    let f = Fixture::new(vec![(200, calls.into()), (200, text_sse("done"))]).await;
    let agent = GeminiAgent::new("k".into(), Some("chosen-model".into())).base_url(&f.base);
    let tools = vec![ToolSchema {
        name: "echo".into(),
        description: "echo a value".into(),
        kind: ToolKind::Other,
        input_schema: serde_json::json!({"type":"object","properties":{"value":{"type":"string"}}}),
    }];
    let mut ctx = context();
    let first = manch_llm::collect_completion(&agent, ctx.clone(), &tools)
        .await
        .unwrap();
    assert_eq!(
        first
            .tool_calls
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        ["native-a", "native-b"]
    );
    // Save/reload using the protocol's existing history format.
    ctx.turns.push(Turn {
        role: Role::Assistant,
        entries: first
            .tool_calls
            .iter()
            .cloned()
            .map(Entry::ToolCall)
            .collect(),
    });
    ctx.turns.push(Turn {
        role: Role::User,
        entries: first
            .tool_calls
            .iter()
            .map(|call| Entry::ToolResult {
                id: call.id.clone(),
                content: vec![ToolCallContent::Content(Content::new(ContentBlock::Text(
                    TextContent::new(call.arguments["value"].as_str().unwrap()),
                )))],
                raw_output: None,
            })
            .collect(),
    });
    let ctx: Context = serde_json::from_str(&serde_json::to_string(&ctx).unwrap()).unwrap();
    let second = manch_llm::collect_completion(&agent, ctx, &tools)
        .await
        .unwrap();
    assert_eq!(second.text, "done");
    let body = &f.bodies()[1];
    assert_eq!(
        body["contents"][1]["parts"][0]["thoughtSignature"],
        "signature-a"
    );
    assert_eq!(
        body["contents"][2]["parts"][0]["functionResponse"],
        serde_json::json!({"id":"native-a","name":"echo","response":{"result":"first"}})
    );
    assert_eq!(
        body["contents"][2]["parts"][1]["functionResponse"],
        serde_json::json!({"id":"native-b","name":"echo","response":{"result":"second"}})
    );
    assert_eq!(f.requests.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn invalid_tool_and_system_content_cannot_disappear() {
    use manch_protocol::acp::{Content, ToolCallContent};
    let f = Fixture::new(vec![(200, text_sse("ok"))]).await;
    let mut ctx = context();
    ctx.turns[0].entries.push(Entry::ToolResult {
        id: "c1".into(),
        content: vec![ToolCallContent::Content(Content::new(link(
            "https://files.example/a",
            Some("application/pdf"),
            "a",
        )))],
        raw_output: None,
    });
    assert!(
        GeminiAgent::new("k".into(), None)
            .base_url(&f.base)
            .prompt(ctx, &[], Arc::new(Sink::default()))
            .await
            .is_err()
    );
    assert!(f.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn default_transport_does_not_forward_keys_through_redirects() {
    let destination = Fixture::new(vec![(200, text_sse("ok"))]).await;
    let redirect = Fixture::new(vec![(302, destination.base.clone())]).await;
    let result = GeminiAgent::new("secret-key".into(), None)
        .base_url(&redirect.base)
        .max_retries(0)
        .prompt(context(), &[], Arc::new(Sink::default()))
        .await;
    assert!(result.is_err());
    assert!(destination.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn unknown_finish_retains_same_frame_text_and_usage() {
    let f=Fixture::new(vec![(200,"data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"partial\"}]},\"finishReason\":\"NEW_REASON\"}],\"usageMetadata\":{\"totalTokenCount\":21}}\n\n".into())]).await;
    let failure = manch_llm::collect_completion(
        &GeminiAgent::new("k".into(), None).base_url(&f.base),
        context(),
        &[],
    )
    .await
    .unwrap_err();
    assert_eq!(failure.partial.text, "partial");
    assert_eq!(failure.partial.usage.total_tokens, Some(21));
}

#[tokio::test]
async fn prompt_level_block_is_a_refusal_without_candidates() {
    let f=Fixture::new(vec![(200,"data: {\"responseId\":\"blocked-1\",\"promptFeedback\":{\"blockReason\":\"SAFETY\"},\"usageMetadata\":{\"promptTokenCount\":21}}\n\n".into())]).await;
    let result = manch_llm::collect_completion(
        &GeminiAgent::new("k".into(), None).base_url(&f.base),
        context(),
        &[],
    )
    .await
    .unwrap();
    assert_eq!(result.stop_reason, manch_protocol::acp::StopReason::Refusal);
    assert_eq!(result.usage.input_tokens, Some(21));
    assert_eq!(result.metadata.response_id.as_deref(), Some("blocked-1"));
}

#[cfg(feature = "openai")]
#[tokio::test]
async fn malformed_openai_fragments_and_restarted_streams_fail() {
    let finish = "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n";
    for delta in [
        serde_json::json!({"index":0,"function":{"arguments":"{}"}}),
        serde_json::json!({"id":"c1","function":{"name":"echo","arguments":"{}"}}),
        serde_json::json!({"index":0,"id":"c1","function":{"arguments":"{}"}}),
        serde_json::json!({"index":0,"id":"c1","function":{"name":"echo","arguments":"{"}}),
    ] {
        let body = format!(
            "data: {}\n\n{finish}",
            serde_json::json!({"choices":[{"delta":{"tool_calls":[delta]}}]})
        );
        let f = Fixture::new(vec![(200, body)]).await;
        assert!(
            manch_llm::collect_completion(
                &manch_llm::OpenAiAgent::new("k".into(), None).base_url(&f.base),
                context(),
                &[]
            )
            .await
            .is_err()
        );
    }
    let f = Fixture::new(vec![(200, format!("data: [DONE]\n\n{finish}"))]).await;
    assert!(
        manch_llm::collect_completion(
            &manch_llm::OpenAiAgent::new("k".into(), None).base_url(&f.base),
            context(),
            &[]
        )
        .await
        .is_err()
    );
}

#[cfg(feature = "anthropic")]
#[tokio::test]
async fn unknown_anthropic_finish_retains_usage() {
    let body = "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"unknown\"},\"usage\":{\"output_tokens\":21}}\n\n";
    let f = Fixture::new(vec![(200, body.into())]).await;
    let failure = manch_llm::collect_completion(
        &manch_llm::AnthropicAgent::new("k".into(), None).base_url(&f.base),
        context(),
        &[],
    )
    .await
    .unwrap_err();
    assert_eq!(failure.partial.usage.output_tokens, Some(21));
}

#[tokio::test]
async fn terminal_sse_event_requires_its_blank_delimiter() {
    let f = Fixture::new(vec![(
        200,
        text_sse("partial").trim_end_matches('\n').to_string() + "\n",
    )])
    .await;
    assert!(
        manch_llm::collect_completion(
            &GeminiAgent::new("k".into(), None).base_url(&f.base),
            context(),
            &[]
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn split_result_turns_use_the_nearest_call_name_and_metadata() {
    use manch_protocol::ToolInvocation;
    use manch_protocol::acp::{Content, ToolCallContent};
    let call = |name: &str, meta| {
        Entry::ToolCall(ToolInvocation {
            id: "reused".into(),
            name: name.into(),
            arguments: serde_json::json!({}),
            provider_meta: meta,
        })
    };
    let result = || Entry::ToolResult {
        id: "reused".into(),
        content: vec![ToolCallContent::Content(Content::new(ContentBlock::Text(
            TextContent::new("result"),
        )))],
        raw_output: None,
    };
    let mut ctx = context();
    ctx.turns.extend([
        Turn {
            role: Role::Assistant,
            entries: vec![call(
                "old",
                Some(serde_json::json!({"callId":"old-native"})),
            )],
        },
        Turn {
            role: Role::User,
            entries: vec![result()],
        },
        Turn {
            role: Role::Assistant,
            entries: vec![call("current", None)],
        },
        Turn {
            role: Role::User,
            entries: vec![Entry::Block(ContentBlock::Text(TextContent::new(
                "between",
            )))],
        },
        Turn {
            role: Role::User,
            entries: vec![result()],
        },
    ]);
    let f = Fixture::new(vec![(200, text_sse("ok"))]).await;
    manch_llm::collect_completion(
        &GeminiAgent::new("k".into(), None).base_url(&f.base),
        ctx,
        &[],
    )
    .await
    .unwrap();
    let body = &f.bodies()[0];
    let response = &body["contents"][5]["parts"][0]["functionResponse"];
    assert_eq!(response["name"], "current");
    assert!(
        response.get("id").is_none(),
        "must not inherit the old native ID: {response}"
    );
}

#[cfg(feature = "openai")]
#[tokio::test]
async fn non_string_openai_arguments_are_not_replaced_with_empty_object() {
    let body = "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c1\",\"function\":{\"name\":\"echo\",\"arguments\":7}}]},\"finish_reason\":\"tool_calls\"}]}\n\n";
    let f = Fixture::new(vec![(200, body.into())]).await;
    assert!(
        manch_llm::collect_completion(
            &manch_llm::OpenAiAgent::new("k".into(), None).base_url(&f.base),
            context(),
            &[]
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn malformed_call_retains_same_frame_usage() {
    let body = "data: {\"candidates\":[{\"content\":{\"parts\":[{\"functionCall\":{\"name\":\"echo\",\"args\":7}}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"totalTokenCount\":21}}\n\n";
    let f = Fixture::new(vec![(200, body.into())]).await;
    let failure = manch_llm::collect_completion(
        &GeminiAgent::new("k".into(), None).base_url(&f.base),
        context(),
        &[],
    )
    .await
    .unwrap_err();
    assert_eq!(failure.partial.usage.total_tokens, Some(21));
}

#[cfg(feature = "fireworks")]
#[tokio::test]
async fn existing_compatible_budget_survives_typed_output_policy() {
    use manch_llm::{GenerationOptions, ModelPolicy};
    let f = Fixture::new(vec![(
        200,
        "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\n\n"
            .into(),
    )])
    .await;
    let agent = manch_llm::fireworks::agent("k".into(), Some("chosen".into()))
        .base_url(&f.base)
        .compatible_reasoning_budget_tokens(std::num::NonZeroU32::new(4096).unwrap())
        .generation_options(
            GenerationOptions {
                max_output_tokens: Some(16384),
                ..Default::default()
            },
            ModelPolicy::new("fireworks", "chosen"),
        )
        .unwrap();
    manch_llm::collect_completion(&agent, context(), &[])
        .await
        .unwrap();
    assert_eq!(f.bodies()[0]["reasoning_effort"], 4096);
    assert_eq!(f.bodies()[0]["max_tokens"], 16384);
}

#[cfg(feature = "fireworks")]
#[tokio::test]
async fn conflicting_legacy_and_typed_reasoning_controls_are_explicit() {
    use manch_llm::{GenerationOptions, ModelPolicy, ReasoningOptions};
    let budget = std::num::NonZeroU32::new(4096).unwrap();
    let policy = ModelPolicy {
        reasoning: vec![ReasoningOptions::CompatibleBudget(2048).kind()],
        ..ModelPolicy::new("fireworks", "chosen")
    };
    let options = GenerationOptions {
        reasoning: Some(ReasoningOptions::CompatibleBudget(2048)),
        ..Default::default()
    };
    assert!(
        manch_llm::fireworks::agent("k".into(), Some("chosen".into()))
            .compatible_reasoning_budget_tokens(budget)
            .generation_options(options.clone(), policy.clone())
            .is_err()
    );
    let f = Fixture::new(vec![(200, text_sse("ok"))]).await;
    let agent = manch_llm::fireworks::agent("k".into(), Some("chosen".into()))
        .base_url(&f.base)
        .generation_options(options, policy)
        .unwrap()
        .compatible_reasoning_budget_tokens(budget);
    assert!(
        agent
            .prompt(context(), &[], Arc::new(Sink::default()))
            .await
            .is_err()
    );
    assert!(f.requests.lock().unwrap().is_empty());
}
