//! BYOK Gemini `generateContent` client (SSE streaming via `?alt=sse`).

use std::sync::Arc;

use async_trait::async_trait;
use manch_protocol::acp::{ContentBlock, StopReason, ToolCallContent};
use manch_protocol::{
    Agent, Context, Entry, EventSink, Result, Role, ToolInvocation, ToolSchema, Turn,
};

use crate::{ModelInfo, SseItem, ensure_crypto_provider, err, token_count, turn_text};

pub(crate) const DEFAULT_BASE: &str = "https://generativelanguage.googleapis.com/v1beta";
// Stable alias — resolves to the current flash snapshot, so it won't rot like a
// pinned id. Only hit if list-models fails. Matches the reasoning behind
// `openai::FALLBACK_MODEL`.
//
// The previous value, `gemini-3-flash`, was not a model Google offers in any
// form: the catalogue is versioned `3.1`/`3.5`/`3.6`/…, and the unversioned
// names are the `-latest` aliases. It failed at request time rather than at
// construction, so a consumer met it in production. Nothing offline could catch
// that — the request body was encoded exactly as documented and was rejected on
// the model name alone — which is what `tests/live.rs` exists to cover.
pub(crate) const FALLBACK_MODEL: &str = "gemini-flash-latest";

pub struct GeminiAgent {
    api_key: String,
    model: String,
    base: String,
}

impl GeminiAgent {
    pub fn new(api_key: String, model: Option<String>) -> Self {
        Self {
            api_key,
            model: model.unwrap_or_else(|| FALLBACK_MODEL.to_string()),
            base: crate::resolve_base("gemini", None, DEFAULT_BASE),
        }
    }

    /// Point this agent at an alternative endpoint. Wins over `MANCH_GEMINI_BASE_URL`.
    #[must_use]
    pub fn base_url(mut self, base: impl Into<String>) -> Self {
        let base = base.into();
        self.base = crate::pick_base(Some(&base), None, &self.base);
        self
    }

    /// The endpoint this agent resolved to. Thread it into
    /// [`list_models_at`] so a redirected agent lists ITS catalog rather than
    /// the vendor's — nothing does that automatically, because `list_models`
    /// is a free function with no agent to read a base from.
    #[must_use]
    pub fn base(&self) -> &str {
        &self.base
    }
}

/// `{base}/models/{model}:streamGenerateContent?alt=sse`. Pure.
pub(crate) fn stream_url(base: &str, model: &str) -> String {
    format!("{base}/models/{model}:streamGenerateContent?alt=sse")
}

/// `{base}/models`. Pure.
pub(crate) fn models_url(base: &str) -> String {
    format!("{base}/models")
}

/// Build the `tools` array Gemini expects: a single entry wrapping
/// `functionDeclarations`, each `{ name, description, parameters }`.
/// `parameters` is already the JSON Schema a `Tool` declares — only the
/// envelope is Gemini-specific. Pure.
pub(crate) fn tools_json(tools: &[ToolSchema]) -> serde_json::Value {
    let declarations: Vec<serde_json::Value> = tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "name": t.name,
                "description": t.description,
                "parameters": t.input_schema,
            })
        })
        .collect();
    serde_json::json!([{ "functionDeclarations": declarations }])
}

/// Flatten one tool result's content blocks into the plain string wrapped
/// into Gemini's `functionResponse.response`. Only the text case is
/// meaningful on today's `Tool` surface; anything else (a future
/// `Diff`/`Terminal` content kind) still serialises rather than panicking, so
/// an unexpected content kind degrades instead of dropping the result.
fn tool_result_text(content: &[ToolCallContent]) -> String {
    content
        .iter()
        .map(|c| match c {
            ToolCallContent::Content(inner) => match &inner.content {
                ContentBlock::Text(t) => t.text.clone(),
                other => serde_json::to_string(other).unwrap_or_default(),
            },
            other => serde_json::to_string(other).unwrap_or_default(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Resolve a `ToolResult.id` back to the tool name Gemini's wire needs, by
/// scanning the immediately preceding assistant turn's `Entry::ToolCall`
/// entries for a matching id.
///
/// This lookup is exact — Manch's own turns still carry the id that pairs a
/// result with its call. The genuine ambiguity is downstream of this
/// function: Gemini's `functionResponse` carries only a `name`, no id, so if
/// the same tool was called twice in one turn, the two resulting response
/// parts are indistinguishable to Gemini once encoded. Nothing on this side
/// can repair that — the best this function (and `request_body`) can do is
/// preserve issue order, so response N lines up positionally with call N of
/// that name.
fn resolve_tool_name(turns: &[Turn], result_turn_index: usize, id: &str) -> Option<String> {
    let assistant_turn = result_turn_index
        .checked_sub(1)
        .and_then(|i| turns.get(i))?;
    assistant_turn.entries.iter().find_map(|e| match e {
        Entry::ToolCall(ToolInvocation {
            id: call_id, name, ..
        }) if call_id == id => Some(name.clone()),
        _ => None,
    })
}

/// Map one turn entry onto a Gemini `parts` entry. `turns`/`turn_index` are
/// needed only to resolve a `ToolResult`'s id back to its tool name (see
/// [`resolve_tool_name`]).
fn entry_part(entry: &Entry, turns: &[Turn], turn_index: usize) -> Option<serde_json::Value> {
    match entry {
        Entry::Block(ContentBlock::Text(t)) => Some(serde_json::json!({ "text": t.text })),
        Entry::Block(_) => None,
        Entry::ToolCall(ToolInvocation {
            name,
            arguments,
            provider_meta,
            ..
        }) => {
            let mut part = serde_json::json!({
                "functionCall": { "name": name, "args": arguments },
            });
            // Whatever was captured alongside this call goes back exactly where
            // it came from — as sibling keys of `functionCall`. Merged rather
            // than nested, and omitted entirely when there is nothing: an
            // explicit null is not the same as an absent key, and non-thinking
            // models must see the body they saw before.
            if let (Some(serde_json::Value::Object(meta)), Some(obj)) =
                (provider_meta, part.as_object_mut())
            {
                for (k, v) in meta {
                    obj.insert(k.clone(), v.clone());
                }
            }
            Some(part)
        }
        Entry::ToolResult { id, content } => {
            let name = resolve_tool_name(turns, turn_index, id).unwrap_or_else(|| id.clone());
            Some(serde_json::json!({
                "functionResponse": {
                    "name": name,
                    "response": { "result": tool_result_text(content) },
                },
            }))
        }
    }
}

/// A turn's `parts` array: the single merged `{ "text": .. }` part Gemini
/// expects when every entry is ordinary text (unchanged from before this
/// task), or one part per entry once a turn carries a tool call or result.
fn turn_parts(turn: &Turn, turns: &[Turn], turn_index: usize) -> Vec<serde_json::Value> {
    let all_text = turn
        .entries
        .iter()
        .all(|e| matches!(e, Entry::Block(ContentBlock::Text(_))));
    if all_text {
        return vec![serde_json::json!({ "text": turn_text(turn) })];
    }
    turn.entries
        .iter()
        .filter_map(|e| entry_part(e, turns, turn_index))
        .collect()
}

/// Pure request body: role-tagged turns as Gemini `contents`, with
/// `Entry::ToolCall`/`Entry::ToolResult` encoded onto `functionCall` /
/// `functionResponse` parts.
///
/// `tools` is omitted from the body entirely when empty — an empty `tools: []`
/// array is not the same fact to a model as no tools being registered.
///
/// Encoding a `ToolResult` requires resolving its id back to a tool *name*
/// (see [`resolve_tool_name`]) because Gemini keys `functionResponse` by name,
/// not id. When the same tool is called twice in one turn, the resulting wire
/// representation is genuinely ambiguous — two `functionResponse` parts with
/// the same name, and no id to tell them apart — which is a limitation of
/// Gemini's wire format, not something this function can resolve. Preserving
/// issue order (never reordering or merging) is the best available reading of
/// a wire format that cannot express the distinction.
pub(crate) fn request_body(turns: &[Turn], tools: &[ToolSchema]) -> serde_json::Value {
    let contents: Vec<serde_json::Value> = turns
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let role = match t.role {
                Role::User => "user",
                Role::Assistant => "model",
            };
            serde_json::json!({ "role": role, "parts": turn_parts(t, turns, i) })
        })
        .collect();
    let mut body = serde_json::json!({ "contents": contents });
    if !tools.is_empty() {
        body["tools"] = tools_json(tools);
    }
    body
}

/// Parse one SSE line: concatenate the candidate's text parts, or surface an error. Pure.
pub(crate) fn parse_line(data: &str) -> Vec<SseItem> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(data) else {
        return Vec::new();
    };
    if let Some(msg) = v
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
    {
        return vec![SseItem::Error(format!("gemini: {msg}"))];
    }
    let mut out = Vec::new();
    let parts = v
        .get("candidates")
        .and_then(|c| c.as_array())
        .and_then(|c| c.first())
        .and_then(|c| c.get("content"))
        .and_then(|c| c.get("parts"))
        .and_then(|p| p.as_array());

    if let Some(parts) = parts {
        let text: String = parts
            .iter()
            .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
            .collect();
        if !text.is_empty() {
            out.push(SseItem::Text(text));
        }

        // A `functionCall` arrives complete in a single part — unlike
        // Anthropic/OpenAI, there is no fragmentation across frames — so one
        // part yields a start/args/end triple in one shot, and Gemini
        // supplies no call id, so one is synthesised from the part's
        // position so a repeat call in the same turn still gets a distinct id.
        for (index, part) in parts.iter().enumerate() {
            let Some(call) = part.get("functionCall") else {
                continue;
            };
            let index = index as u32;
            let name = call
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or_default()
                .to_string();
            let args = call.get("args").cloned().unwrap_or(serde_json::json!({}));
            let id = format!("gemini-{name}-{index}");
            // Thinking models attach a `thoughtSignature` to the call and reject
            // the NEXT turn unless it comes back verbatim. It is a sibling of
            // `functionCall` on the part, not a field inside it. Captured under
            // its own key so the rebuild can echo it without Manch's protocol
            // ever learning what it means.
            let provider_meta = part
                .get("thoughtSignature")
                .map(|sig| serde_json::json!({ "thoughtSignature": sig }));
            out.push(SseItem::ToolCallStart {
                index,
                id,
                name: name.clone(),
                provider_meta,
            });
            out.push(SseItem::ToolCallArgs {
                index,
                json: args.to_string(),
            });
            out.push(SseItem::ToolCallEnd { index });
        }
    }

    if let Some(u) = v.get("usageMetadata") {
        out.push(SseItem::Usage(manch_protocol::Usage {
            input_tokens: token_count(u, "promptTokenCount"),
            output_tokens: token_count(u, "candidatesTokenCount"),
        }));
    }
    out
}

/// Parse list-models response; ids drop the `models/` prefix. Only models that
/// advertise `streamGenerateContent` are kept — the raw list also contains
/// embedding/TTS/embedContent-only models that error at prompt time. Pure.
pub(crate) fn parse_models(body: &serde_json::Value) -> Vec<ModelInfo> {
    body.get("models")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|m| supports_streaming(m))
                .filter_map(|m| {
                    let name = m.get("name")?.as_str()?;
                    Some(ModelInfo {
                        id: name.strip_prefix("models/").unwrap_or(name).to_string(),
                        display_name: m
                            .get("displayName")
                            .and_then(|n| n.as_str())
                            .map(String::from),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// True if a list-models entry advertises `streamGenerateContent` — i.e. it's a
/// chat model this provider can actually prompt (excludes embedding/TTS models).
fn supports_streaming(model: &serde_json::Value) -> bool {
    model
        .get("supportedGenerationMethods")
        .and_then(|m| m.as_array())
        .is_some_and(|methods| {
            methods
                .iter()
                .any(|m| m.as_str() == Some("streamGenerateContent"))
        })
}

pub async fn list_models(api_key: &str) -> Result<Vec<ModelInfo>> {
    list_models_at(api_key, None).await
}

/// As [`list_models`], against an explicit base (falling back to the env
/// override, then the vendor default).
pub async fn list_models_at(api_key: &str, base: Option<&str>) -> Result<Vec<ModelInfo>> {
    ensure_crypto_provider();
    let base = crate::resolve_base("gemini", base, DEFAULT_BASE);
    let resp = reqwest::Client::new()
        .get(models_url(&base))
        .header("x-goog-api-key", api_key)
        .send()
        .await;
    crate::list_models_with(resp, FALLBACK_MODEL, parse_models).await
}

#[async_trait]
impl Agent for GeminiAgent {
    fn id(&self) -> &str {
        "gemini"
    }

    async fn prompt(
        &self,
        ctx: Context,
        tools: &[ToolSchema],
        sink: Arc<dyn EventSink>,
    ) -> Result<StopReason> {
        ensure_crypto_provider();
        let resp = reqwest::Client::new()
            .post(stream_url(&self.base, &self.model))
            .header("x-goog-api-key", &self.api_key)
            .json(&request_body(&ctx.turns, tools))
            .send()
            .await
            .map_err(err)?;

        if !resp.status().is_success() {
            return Err(crate::http_error("gemini", resp).await);
        }
        crate::stream_sse(resp, &sink, parse_line).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use manch_protocol::acp::{ContentBlock, TextContent};
    use manch_protocol::{Entry, Role, Turn};

    fn u(text: &str) -> Turn {
        Turn {
            role: Role::User,
            entries: vec![Entry::Block(ContentBlock::Text(TextContent::new(
                text.to_string(),
            )))],
        }
    }
    fn a(text: &str) -> Turn {
        Turn {
            role: Role::Assistant,
            entries: vec![Entry::Block(ContentBlock::Text(TextContent::new(
                text.to_string(),
            )))],
        }
    }

    #[test]
    fn request_body_maps_single_user_turn() {
        let body = request_body(&[u("hi")], &[]);
        assert_eq!(body["contents"][0]["role"], "user");
        assert_eq!(body["contents"][0]["parts"][0]["text"], "hi");
    }

    #[test]
    fn request_body_maps_assistant_to_model_role() {
        let body = request_body(&[u("q1"), a("a1")], &[]);
        assert_eq!(body["contents"][1]["role"], "model");
        assert_eq!(body["contents"][1]["parts"][0]["text"], "a1");
    }

    #[test]
    fn tools_json_wraps_declarations_in_a_single_tools_entry() {
        use manch_protocol::acp::ToolKind;

        let s = ToolSchema {
            name: "search".into(),
            description: "find".into(),
            kind: ToolKind::Other,
            input_schema: serde_json::json!({ "type": "object" }),
        };
        let v = tools_json(&[s]);
        assert_eq!(v[0]["functionDeclarations"][0]["name"], "search");
        assert_eq!(
            v[0]["functionDeclarations"][0]["parameters"]["type"],
            "object"
        );
    }

    #[test]
    fn parse_line_emits_a_complete_call_as_start_args_and_end() {
        // Gemini sends the whole call in one part, so one frame yields all three
        // fragments and ToolAccum completes it immediately.
        let d = r#"{"candidates":[{"content":{"parts":[{"functionCall":{"name":"search","args":{"q":"asha"}}}]}}]}"#;
        let items = parse_line(d);
        assert!(matches!(items.as_slice(), [
            crate::SseItem::ToolCallStart { name, .. },
            crate::SseItem::ToolCallArgs { .. },
            crate::SseItem::ToolCallEnd { .. }
        ] if name == "search"));
    }

    #[test]
    fn parse_line_captures_a_thought_signature_from_the_part() {
        // Gemini 3 attaches a thoughtSignature as a SIBLING of functionCall on
        // the part, and rejects the next turn if it is not handed back.
        let d = r#"{"candidates":[{"content":{"parts":[{"functionCall":{"name":"search","args":{"q":"asha"}},"thoughtSignature":"sig-abc"}]}}]}"#;
        match parse_line(d).as_slice() {
            [crate::SseItem::ToolCallStart { provider_meta, .. }, ..] => {
                let meta = provider_meta
                    .as_ref()
                    .expect("the signature must be captured, not dropped");
                assert_eq!(meta["thoughtSignature"], "sig-abc");
            }
            other => panic!("expected a tool call start, got {other:?}"),
        }
    }

    #[test]
    fn parse_line_leaves_provider_meta_unset_when_there_is_no_signature() {
        // 2.5 is not a thinking model and sends none; nothing must be invented.
        let d = r#"{"candidates":[{"content":{"parts":[{"functionCall":{"name":"search","args":{}}}]}}]}"#;
        match parse_line(d).as_slice() {
            [crate::SseItem::ToolCallStart { provider_meta, .. }, ..] => {
                assert!(provider_meta.is_none())
            }
            other => panic!("expected a tool call start, got {other:?}"),
        }
    }

    #[test]
    fn request_body_echoes_provider_meta_beside_the_function_call() {
        // The round trip that Gemini 3 requires: whatever was captured on the
        // way in is emitted verbatim on the way back out.
        let turns = vec![Turn {
            role: Role::Assistant,
            entries: vec![Entry::ToolCall(ToolInvocation {
                id: "c1".into(),
                name: "search".into(),
                arguments: serde_json::json!({"q":"asha"}),
                provider_meta: Some(serde_json::json!({"thoughtSignature": "sig-abc"})),
            })],
        }];
        let part = &request_body(&turns, &[])["contents"][0]["parts"][0];
        assert_eq!(part["functionCall"]["name"], "search");
        assert_eq!(
            part["thoughtSignature"], "sig-abc",
            "the signature must sit beside functionCall, not inside it"
        );
    }

    #[test]
    fn request_body_omits_the_signature_key_entirely_when_absent() {
        // An explicit null may be rejected, and 2.5 must be unchanged.
        let turns = vec![Turn {
            role: Role::Assistant,
            entries: vec![Entry::ToolCall(ToolInvocation {
                id: "c1".into(),
                name: "search".into(),
                arguments: serde_json::json!({}),
                provider_meta: None,
            })],
        }];
        let part = &request_body(&turns, &[])["contents"][0]["parts"][0];
        assert!(
            part.get("thoughtSignature").is_none(),
            "absent means absent, not null: {part}"
        );
    }

    #[test]
    fn a_gemini_call_gets_a_synthesised_id() {
        let d = r#"{"candidates":[{"content":{"parts":[{"functionCall":{"name":"search","args":{}}}]}}]}"#;
        match parse_line(d).as_slice() {
            [crate::SseItem::ToolCallStart { id, .. }, ..] => assert!(
                !id.is_empty(),
                "Gemini supplies no id; Manch must synthesise one so results can be paired"
            ),
            _ => panic!("expected a tool call start"),
        }
    }

    #[test]
    fn request_body_encodes_function_call_and_response_parts() {
        let turns = vec![
            Turn {
                role: Role::Assistant,
                entries: vec![Entry::ToolCall(ToolInvocation {
                    id: "c1".into(),
                    name: "search".into(),
                    arguments: serde_json::json!({"q":"asha"}),
                    provider_meta: None,
                })],
            },
            Turn {
                role: Role::User,
                entries: vec![Entry::ToolResult {
                    id: "c1".into(),
                    content: vec![crate::text_content("2 matches")],
                }],
            },
        ];
        let body = request_body(&turns, &[]);
        assert_eq!(
            body["contents"][0]["parts"][0]["functionCall"]["name"],
            "search"
        );
        assert_eq!(
            body["contents"][1]["parts"][0]["functionResponse"]["name"], "search",
            "Gemini keys results by name, not by id"
        );
    }

    #[test]
    fn request_body_keeps_two_calls_to_one_tool_in_issue_order() {
        // Gemini's functionResponse carries only a name, no id, so two calls to
        // the same tool in one turn are indistinguishable on the wire. Issue
        // order is the only alignment left: response N must line up with call N.
        let turns = vec![
            Turn {
                role: Role::Assistant,
                entries: vec![
                    Entry::ToolCall(ToolInvocation {
                        id: "c1".into(),
                        name: "search".into(),
                        arguments: serde_json::json!({"q":"asha"}),
                        provider_meta: None,
                    }),
                    Entry::ToolCall(ToolInvocation {
                        id: "c2".into(),
                        name: "search".into(),
                        arguments: serde_json::json!({"q":"bhim"}),
                        provider_meta: None,
                    }),
                ],
            },
            Turn {
                role: Role::User,
                entries: vec![
                    Entry::ToolResult {
                        id: "c1".into(),
                        content: vec![crate::text_content("2 matches")],
                    },
                    Entry::ToolResult {
                        id: "c2".into(),
                        content: vec![crate::text_content("5 matches")],
                    },
                ],
            },
        ];
        let body = request_body(&turns, &[]);
        let calls = body["contents"][0]["parts"]
            .as_array()
            .expect("call parts")
            .clone();
        assert_eq!(calls.len(), 2, "one part per call: {calls:#?}");
        assert_eq!(calls[0]["functionCall"]["args"]["q"], "asha");
        assert_eq!(calls[1]["functionCall"]["args"]["q"], "bhim");

        let responses = body["contents"][1]["parts"]
            .as_array()
            .expect("response parts")
            .clone();
        assert_eq!(
            responses.len(),
            2,
            "both responses must be emitted, not merged: {responses:#?}"
        );
        assert_eq!(responses[0]["functionResponse"]["name"], "search");
        assert_eq!(responses[1]["functionResponse"]["name"], "search");
        assert_eq!(
            responses[0]["functionResponse"]["response"]["result"], "2 matches",
            "response N must line up with call N"
        );
        assert_eq!(
            responses[1]["functionResponse"]["response"]["result"], "5 matches",
            "response N must line up with call N"
        );
    }

    #[test]
    fn request_body_omits_the_tools_key_when_no_tools_are_registered() {
        let body = request_body(&[u("hi")], &[]);
        assert!(
            body.get("tools").is_none(),
            "an empty tools array changes model behaviour"
        );
    }

    #[test]
    fn request_body_includes_tools_when_provided() {
        use manch_protocol::acp::ToolKind;

        let s = ToolSchema {
            name: "search".into(),
            description: "find".into(),
            kind: ToolKind::Other,
            input_schema: serde_json::json!({ "type": "object" }),
        };
        let body = request_body(&[u("hi")], &[s]);
        assert_eq!(
            body["tools"][0]["functionDeclarations"][0]["name"],
            "search"
        );
    }

    #[test]
    fn parse_line_extracts_candidate_text() {
        let d = r#"{"candidates":[{"content":{"role":"model","parts":[{"text":"Hi"}]}}]}"#;
        assert!(matches!(parse_line(d).as_slice(), [crate::SseItem::Text(t)] if t == "Hi"));
    }

    #[test]
    fn parse_line_reports_usage_alongside_text_in_one_chunk() {
        // Gemini repeats usageMetadata on chunks that also carry text, so one
        // frame must be able to yield two items.
        let d = r#"{"candidates":[{"content":{"parts":[{"text":"Hi"}]}}],"usageMetadata":{"promptTokenCount":5,"candidatesTokenCount":7}}"#;
        let items = parse_line(d);
        assert!(matches!(items.as_slice(),
            [crate::SseItem::Text(t), crate::SseItem::Usage(u)]
                if t == "Hi" && u.input_tokens == Some(5) && u.output_tokens == Some(7)));
    }

    #[test]
    fn new_defaults_to_the_vendor_base() {
        assert_eq!(GeminiAgent::new("k".into(), None).base, DEFAULT_BASE);
    }

    #[test]
    fn base_is_readable_so_a_caller_can_thread_it_to_list_models_at() {
        // `list_models` is a free function: an agent pointed at a proxy cannot
        // redirect the catalog by itself. The caller must read the base back
        // off the agent and pass it to `list_models_at`, which needs an
        // accessor to be possible at all.
        let agent = GeminiAgent::new("k".into(), None).base_url("https://proxy.internal/v9");
        assert_eq!(agent.base(), "https://proxy.internal/v9");
    }

    #[test]
    fn base_url_overrides_the_default() {
        let g = GeminiAgent::new("k".into(), None).base_url("https://proxy.internal/v1beta");
        assert_eq!(g.base, "https://proxy.internal/v1beta");
    }

    #[test]
    fn urls_derive_from_the_base() {
        assert_eq!(
            stream_url("https://p/v1beta", "gemini-flash-latest"),
            "https://p/v1beta/models/gemini-flash-latest:streamGenerateContent?alt=sse"
        );
        assert_eq!(models_url("https://p/v1beta"), "https://p/v1beta/models");
    }

    #[test]
    fn parse_line_surfaces_error() {
        let d = r#"{"error":{"code":400,"message":"bad key"}}"#;
        assert!(
            matches!(parse_line(d).as_slice(), [crate::SseItem::Error(e)] if e == "gemini: bad key")
        );
    }

    #[test]
    fn parse_models_strips_models_prefix() {
        let body = serde_json::json!({
            "models": [{
                "name": "models/gemini-flash-latest",
                "displayName": "Gemini 3 Flash",
                "supportedGenerationMethods": ["generateContent", "streamGenerateContent"]
            }]
        });
        let models = parse_models(&body);
        assert_eq!(models[0].id, "gemini-flash-latest");
        assert_eq!(models[0].display_name.as_deref(), Some("Gemini 3 Flash"));
    }

    #[test]
    fn parse_models_drops_non_streaming_models() {
        let body = serde_json::json!({
            "models": [
                {
                    "name": "models/gemini-flash-latest",
                    "supportedGenerationMethods": ["streamGenerateContent"]
                },
                {
                    "name": "models/text-embedding-004",
                    "supportedGenerationMethods": ["embedContent"]
                },
                { "name": "models/legacy-no-methods" }
            ]
        });
        let ids: Vec<_> = parse_models(&body).into_iter().map(|m| m.id).collect();
        assert_eq!(ids, vec!["gemini-flash-latest"]);
    }

    #[test]
    fn new_uses_fallback_when_model_none() {
        let g = GeminiAgent::new("k".into(), None);
        assert_eq!(g.model, FALLBACK_MODEL);
    }
}
