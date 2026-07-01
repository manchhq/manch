# Real ACP Streaming Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stream real token + tool-call events from the Rust agents to the stage, replacing the single-shot `send_prompt`/`mockEngine` path with a live `tauriEngine` over a typed Tauri `Channel`.

**Architecture:** A new `StreamEvent` DTO (mirroring the existing `StageEvent` union) is emitted by the agents through an `EventSink` trait, carried per-invoke over a `tauri::ipc::Channel<StreamEvent>` from a new `send_prompt_stream` command. `tauriEngine` bridges the channel into its existing `AsyncIterable<StageEvent>`; `useSend`, the jotai streaming atoms, and `applyEvent` are untouched (already event-driven). Anthropic streams via SSE (tokens only); Claude Code forwards the `AgentMessageChunk` tokens and ACP `ToolCall`/`ToolCallUpdate` notifications it already receives.

**Tech Stack:** Rust (Tauri 2, reqwest 0.13 streaming, `agent-client-protocol` v1, `manch-dto` + ts-rs), TypeScript (Vite/React, `@tauri-apps/api` Channel), Vitest.

## Global Constraints

- Rust ≥ 1.88, edition 2024; clippy is `-D warnings`; rustfmt enforced. Run `just fmt` before every Rust commit.
- Anthropic model id is `claude-opus-4-8` (const `ANTHROPIC_MODEL`) — do NOT change.
- `manch-dto` TS is generated: after editing DTOs run `just gen`; never hand-edit `apps/desktop/src/data/bindings.ts`.
- `StageEvent` (in `apps/desktop/src/engine/StageEngine.ts`) is the internal frontend contract and stays authoritative; `StreamEvent` is the wire type mapped onto it.
- `applyEvent` appends token deltas (`state.text + event.text`) — the bridge MUST emit **incremental** deltas, never cumulative snapshots.
- `just ci` must be green before any PR. Conventional Commits. PR references #17.
- Keep the Anthropic client inline in `apps/desktop/src-tauri/src/agent.rs` (no `manch-anthropic` crate — deferred milestone).

---

### Task 1: `StreamEvent` DTO + generated bindings

**Files:**
- Modify: `crates/manch-dto/src/lib.rs` (add enum at end)
- Modify: `crates/manch-dto/src/bin/gen-types.rs` (import + declarations list)
- Regenerate: `apps/desktop/src/data/bindings.ts` (via `just gen`)

**Interfaces:**
- Produces: `manch_dto::StreamEvent` enum with variants `Token { text: String }`, `Tool { id: String, name: String, status: String, detail: Option<String> }`, `Done`, `Error { message: String }`, serialized internally-tagged on `kind` (camelCase). TS counterpart `StreamEvent` in `bindings.ts`.

- [ ] **Step 1: Add the DTO.** In `crates/manch-dto/src/lib.rs`, append:

```rust
/// Wire event streamed from an agent to the stage. Mirrors the frontend
/// `StageEvent` union (`apps/desktop/src/engine/StageEngine.ts`); `tauriEngine`
/// maps this onto it. Internally tagged on `kind` so the TS side is a plain
/// discriminated union.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(TS))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StreamEvent {
    Token { text: String },
    Tool {
        id: String,
        name: String,
        /// "running" | "done" | "error"
        status: String,
        detail: Option<String>,
    },
    Done,
    Error { message: String },
}
```

- [ ] **Step 2: Register it in the generator.** In `crates/manch-dto/src/bin/gen-types.rs`, add `StreamEvent` to the `use manch_dto::{…}` import list, and add to the `declarations()` vec (after `CrossVerify`):

```rust
        StreamEvent::export_to_string(&cfg).expect("export StreamEvent"),
```

- [ ] **Step 3: Regenerate + verify.**

Run: `just gen && grep -n "StreamEvent" apps/desktop/src/data/bindings.ts`
Expected: prints a generated `export type StreamEvent = { kind: "token", text: string, } | { kind: "tool", … } | { kind: "done", } | { kind: "error", message: string, };`

- [ ] **Step 4: Verify the workspace still builds.**

Run: `cargo build -p manch-dto --features ts`
Expected: builds clean.

- [ ] **Step 5: Commit.**

```bash
just fmt
git add crates/manch-dto/src/lib.rs crates/manch-dto/src/bin/gen-types.rs apps/desktop/src/data/bindings.ts
git commit -m "feat(dto): add StreamEvent wire type for agent streaming"
```

---

### Task 2: `EventSink` + streaming pure helpers (`push_chunk`, `parse_sse_delta`, `tool_status`)

**Files:**
- Modify: `apps/desktop/src-tauri/src/agent.rs` (add sink trait, helpers; replace `merge_chunk`; add tests)

**Interfaces:**
- Produces:
  - `pub trait EventSink: Send + Sync { fn emit(&self, event: manch_dto::StreamEvent); }`
  - `fn push_chunk(buf: &mut String, chunk: &str) -> Option<String>` — merges a streamed chunk into `buf`, returns the newly-added delta (or `None`).
  - `fn parse_sse_delta(data: &str) -> Option<String>` — extracts text from one Anthropic SSE `data:` payload.
  - `fn tool_status(status: agent_client_protocol::schema::v1::ToolCallStatus) -> &'static str`.

- [ ] **Step 1: Write failing tests.** In `agent.rs`, in `mod tests`, replace the four `merge_chunk_*` tests with `push_chunk` versions and add SSE + status tests:

```rust
    #[test]
    fn push_chunk_returns_full_first_chunk() {
        let mut b = String::new();
        assert_eq!(push_chunk(&mut b, "New "), Some("New ".to_string()));
        assert_eq!(b, "New ");
    }

    #[test]
    fn push_chunk_returns_only_the_delta_for_cumulative_snapshot() {
        let mut b = String::new();
        push_chunk(&mut b, "New");
        assert_eq!(push_chunk(&mut b, "New Delhi."), Some(" Delhi.".to_string()));
        assert_eq!(b, "New Delhi.");
    }

    #[test]
    fn push_chunk_appends_distinct_delta() {
        let mut b = String::new();
        push_chunk(&mut b, "New");
        assert_eq!(push_chunk(&mut b, " Delhi."), Some(" Delhi.".to_string()));
        assert_eq!(b, "New Delhi.");
    }

    #[test]
    fn push_chunk_drops_trailing_full_repeat() {
        let mut b = String::new();
        push_chunk(&mut b, "New");
        push_chunk(&mut b, " Delhi.");
        assert_eq!(push_chunk(&mut b, "New Delhi."), Some(" Delhi.".to_string()));
        // final identical repeat yields no new delta
        assert_eq!(push_chunk(&mut b, "New Delhi."), None);
        assert_eq!(b, "New Delhi.");
    }

    #[test]
    fn parse_sse_delta_extracts_text_delta() {
        let data = r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hi"}}"#;
        assert_eq!(parse_sse_delta(data), Some("Hi".to_string()));
    }

    #[test]
    fn parse_sse_delta_ignores_non_text_events() {
        assert_eq!(parse_sse_delta(r#"{"type":"message_start"}"#), None);
        assert_eq!(parse_sse_delta(r#"{"type":"ping"}"#), None);
        assert_eq!(parse_sse_delta("not json"), None);
    }

    #[test]
    fn tool_status_maps_acp_to_stage_vocabulary() {
        use agent_client_protocol::schema::v1::ToolCallStatus;
        assert_eq!(tool_status(ToolCallStatus::Completed), "done");
        assert_eq!(tool_status(ToolCallStatus::Failed), "error");
        assert_eq!(tool_status(ToolCallStatus::InProgress), "running");
        assert_eq!(tool_status(ToolCallStatus::Pending), "running");
    }
```

- [ ] **Step 2: Run tests to verify they fail.**

Run: `cargo test -p manch-desktop push_chunk parse_sse_delta tool_status 2>&1 | tail -20`
Expected: FAIL — `push_chunk`, `parse_sse_delta`, `tool_status` not found.

- [ ] **Step 3: Implement the helpers.** In `agent.rs`, delete the old `merge_chunk` fn and add:

```rust
use manch_dto::StreamEvent;

/// A destination for streamed agent events. Production wraps a Tauri `Channel`;
/// tests use a `Vec` collector. Keeps the agents ignorant of Tauri specifics.
pub trait EventSink: Send + Sync {
    fn emit(&self, event: StreamEvent);
}

/// Merge one streamed chunk into `buf`, returning the newly-added text to emit
/// (`None` if nothing new). ACP adapters vary — pure deltas, cumulative
/// snapshots, and a trailing full-message repeat — this tolerates all three so
/// we never double-emit ("New Delhi.New Delhi."). Pure.
fn push_chunk(buf: &mut String, chunk: &str) -> Option<String> {
    if chunk.is_empty() {
        None
    } else if buf.is_empty() {
        buf.push_str(chunk);
        Some(chunk.to_string())
    } else if chunk.starts_with(buf.as_str()) {
        // cumulative snapshot: the delta is the suffix beyond what we have
        let delta = chunk[buf.len()..].to_string();
        *buf = chunk.to_string();
        (!delta.is_empty()).then_some(delta)
    } else if buf.ends_with(chunk) {
        None // trailing duplicate already present
    } else {
        buf.push_str(chunk);
        Some(chunk.to_string())
    }
}

/// Extract the text of one Anthropic streaming SSE `data:` payload; `None` for
/// any non-text event (`message_start`, `ping`, `content_block_stop`, …). Pure.
fn parse_sse_delta(data: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(data).ok()?;
    if v.get("type")?.as_str()? != "content_block_delta" {
        return None;
    }
    let delta = v.get("delta")?;
    if delta.get("type")?.as_str()? != "text_delta" {
        return None;
    }
    Some(delta.get("text")?.as_str()?.to_string())
}

/// Map an ACP tool-call status onto the stage's `running|done|error` vocabulary.
fn tool_status(status: agent_client_protocol::schema::v1::ToolCallStatus) -> &'static str {
    use agent_client_protocol::schema::v1::ToolCallStatus;
    match status {
        ToolCallStatus::Completed => "done",
        ToolCallStatus::Failed => "error",
        _ => "running", // Pending / InProgress / future
    }
}
```

- [ ] **Step 4: Run tests to verify they pass.**

Run: `cargo test -p manch-desktop push_chunk parse_sse_delta tool_status 2>&1 | tail -20`
Expected: PASS (all listed tests green). Note: `ChatAgent::ask` / `AnthropicAgent` / `ClaudeCodeAgent` still reference the removed `merge_chunk` — that is fixed in Tasks 3–4; if the crate doesn't compile yet, run with `--no-run` disabled by proceeding to Task 3 first. To keep this task self-contained, temporarily leave `merge_chunk` in place AND add `push_chunk`, then remove `merge_chunk` in Task 4 once its last caller is gone.

> **Right-sizing note:** to keep the crate compiling at each commit, in Step 3 ADD `push_chunk` alongside the existing `merge_chunk` rather than deleting `merge_chunk`. Delete `merge_chunk` (and its remaining tests) in Task 4, where its last caller moves to `push_chunk`.

- [ ] **Step 5: Commit.**

```bash
just fmt && just clippy
git add apps/desktop/src-tauri/src/agent.rs
git commit -m "feat(desktop): EventSink trait + streaming pure helpers (push_chunk, parse_sse_delta, tool_status)"
```

---

### Task 3: `AnthropicAgent::stream` — SSE token streaming

**Files:**
- Modify: `apps/desktop/src-tauri/src/agent.rs` (add `stream` to trait + Anthropic impl; add `stream: true` to body)

**Interfaces:**
- Consumes: `EventSink`, `parse_sse_delta`, `StreamEvent`, existing `anthropic_request_body`, `parse_anthropic_text`, `ensure_crypto_provider`.
- Produces: `ChatAgent::stream(&self, prompt: &str, sink: &dyn EventSink) -> Result<(), String>`; `AnthropicAgent` impl emitting `Token` deltas then `Done` (or `Error`).

- [ ] **Step 1: Change the trait + request body.** In `agent.rs` replace the `ChatAgent` trait with:

```rust
/// One interface, two implementations (plan B). Stand-in for `manch_protocol::Agent`.
#[async_trait]
pub trait ChatAgent: Send + Sync {
    /// Stream the answer to `prompt`, emitting `Token`/`Tool` events into `sink`
    /// and finishing with `Done` (or `Error`). Returns `Err` only for failures
    /// that occur before any event could be emitted.
    async fn stream(&self, prompt: &str, sink: &dyn EventSink) -> Result<(), String>;
}
```

In `anthropic_request_body`, add `"stream": true`:

```rust
    serde_json::json!({
        "model": model,
        "max_tokens": MAX_TOKENS,
        "stream": true,
        "messages": [{ "role": "user", "content": prompt }],
    })
```

Update the existing `request_body_has_model_and_user_message` test to also assert `assert_eq!(body["stream"], true);`.

- [ ] **Step 2: Implement `AnthropicAgent::stream`.** Replace the `impl ChatAgent for AnthropicAgent` block with an SSE reader:

```rust
#[async_trait]
impl ChatAgent for AnthropicAgent {
    async fn stream(&self, prompt: &str, sink: &dyn EventSink) -> Result<(), String> {
        use futures_util::StreamExt;
        ensure_crypto_provider();
        let resp = reqwest::Client::new()
            .post(ANTHROPIC_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .json(&anthropic_request_body(ANTHROPIC_MODEL, prompt))
            .send()
            .await
            .map_err(|e| e.to_string())?;

        // Error responses are JSON, not SSE — surface the message.
        if !resp.status().is_success() {
            let status = resp.status();
            let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
            let msg = parse_anthropic_text(&body)
                .err()
                .unwrap_or_else(|| format!("anthropic: HTTP {status}"));
            sink.emit(StreamEvent::Error { message: msg });
            return Ok(());
        }

        // SSE frames arrive as `\n`-separated lines; buffer partial lines across
        // network chunks and act only on `data:` payloads.
        let mut stream = resp.bytes_stream();
        let mut buf = String::new();
        while let Some(chunk) = stream.next().await {
            let bytes = chunk.map_err(|e| e.to_string())?;
            buf.push_str(&String::from_utf8_lossy(&bytes));
            while let Some(nl) = buf.find('\n') {
                let line = buf[..nl].trim().to_string();
                buf.drain(..=nl);
                if let Some(data) = line.strip_prefix("data:") {
                    let data = data.trim();
                    if let Some(text) = parse_sse_delta(data) {
                        sink.emit(StreamEvent::Token { text });
                    }
                }
            }
        }
        sink.emit(StreamEvent::Done);
        Ok(())
    }
}
```

- [ ] **Step 3: Add `futures-util` dependency.** In `apps/desktop/src-tauri/Cargo.toml` under `[dependencies]`:

```toml
futures-util = "0.3"
```

- [ ] **Step 4: Verify build + existing tests.**

Run: `cargo build -p manch-desktop 2>&1 | tail -20`
Expected: FAILS only in `ClaudeCodeAgent` (still implements the old `ask`) — that's Task 4. The Anthropic impl and `anthropic_request_body` test compile. If you want a green checkpoint here, temporarily stub `ClaudeCodeAgent`'s `stream` with `unimplemented!()` (removed in Task 4). Prefer to proceed straight to Task 4 and commit them together.

- [ ] **Step 5: Commit (with Task 4).** Anthropic streaming and Claude Code streaming both change `ChatAgent`; commit them together at the end of Task 4 to keep the crate compiling.

---

### Task 4: `ClaudeCodeAgent::stream` — forward tokens + tool calls

**Files:**
- Modify: `apps/desktop/src-tauri/src/agent.rs` (rewrite Claude Code impl to emit into the sink; delete `merge_chunk` + its tests)

**Interfaces:**
- Consumes: `EventSink`, `push_chunk`, `tool_status`, `StreamEvent`, `claude_code_args`, ACP `SessionUpdate`/`ToolCall`/`ToolCallUpdate`.
- Produces: `ChatAgent::stream` for `ClaudeCodeAgent` emitting `Token`/`Tool`/`Done`/`Error`.

- [ ] **Step 1: Rewrite the impl.** Replace `impl ChatAgent for ClaudeCodeAgent` so the notification handler emits into a sink instead of buffering. Thread an `EventSink` and a shared token buffer + tool-name map into the handler (both behind `Arc<Mutex<_>>` so the `'static` async closure can own clones):

```rust
#[async_trait]
impl ChatAgent for ClaudeCodeAgent {
    async fn stream(&self, prompt: &str, sink: &dyn EventSink) -> Result<(), String> {
        use std::collections::HashMap;
        use std::sync::{Arc, Mutex};

        use agent_client_protocol::schema::ProtocolVersion;
        use agent_client_protocol::schema::v1::{
            ContentBlock, ContentChunk, InitializeRequest, NewSessionRequest, PromptRequest,
            RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
            SelectedPermissionOutcome, SessionNotification, SessionUpdate, TextContent,
        };
        use agent_client_protocol::{self as acp, AcpAgent, Agent, Client, ConnectionTo};

        let agent = AcpAgent::from_args(claude_code_args(self.api_key.as_deref()))
            .map_err(|e| e.to_string())?;
        let prompt = prompt.to_string();

        // Forward events as they arrive. The handler is a `'static` async closure,
        // so it owns Arc clones. We collect StreamEvents into a queue drained after
        // the run (the `Client` handler cannot borrow `sink` directly), preserving
        // arrival order. `buf` dedups token chunks; `names` resolves tool titles
        // across ToolCallUpdate (which may omit the title).
        let queue: Arc<Mutex<Vec<StreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let q = queue.clone();
        let buf = Arc::new(Mutex::new(String::new()));
        let names: Arc<Mutex<HashMap<String, String>>> = Arc::new(Mutex::new(HashMap::new()));
        let nb = buf.clone();
        let nn = names.clone();

        let stop = Client
            .builder()
            .on_receive_notification(
                async move |n: SessionNotification, _cx| {
                    let mut out: Vec<StreamEvent> = Vec::new();
                    match n.update {
                        SessionUpdate::AgentMessageChunk(ContentChunk {
                            content: ContentBlock::Text(text),
                            ..
                        }) => {
                            if let Some(delta) = push_chunk(&mut nb.lock().unwrap(), &text.text) {
                                out.push(StreamEvent::Token { text: delta });
                            }
                        }
                        SessionUpdate::ToolCall(tc) => {
                            let id = tc.tool_call_id.0.to_string();
                            nn.lock().unwrap().insert(id.clone(), tc.title.clone());
                            out.push(StreamEvent::Tool {
                                id,
                                name: tc.title,
                                status: tool_status(tc.status).into(),
                                detail: None,
                            });
                        }
                        SessionUpdate::ToolCallUpdate(u) => {
                            let id = u.tool_call_id.0.to_string();
                            let name = u
                                .fields
                                .title
                                .clone()
                                .or_else(|| nn.lock().unwrap().get(&id).cloned())
                                .unwrap_or_default();
                            out.push(StreamEvent::Tool {
                                id,
                                name,
                                status: u.fields.status.map(tool_status).unwrap_or("running").into(),
                                detail: None,
                            });
                        }
                        _ => {}
                    }
                    if !out.is_empty() {
                        q.lock().unwrap().extend(out);
                    }
                    Ok(())
                },
                acp::on_receive_notification!(),
            )
            .on_receive_request(
                async move |request: RequestPermissionRequest, responder, _connection| match request
                    .options
                    .first()
                    .map(|opt| opt.option_id.clone())
                {
                    Some(id) => responder.respond(RequestPermissionResponse::new(
                        RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(id)),
                    )),
                    None => responder.respond(RequestPermissionResponse::new(
                        RequestPermissionOutcome::Cancelled,
                    )),
                },
                acp::on_receive_request!(),
            )
            .connect_with(agent, |connection: ConnectionTo<Agent>| async move {
                connection
                    .send_request(InitializeRequest::new(ProtocolVersion::V1))
                    .block_task()
                    .await?;
                let cwd = std::env::temp_dir().join("manch-acp-workspace");
                std::fs::create_dir_all(&cwd).ok();
                let session = connection
                    .send_request(NewSessionRequest::new(cwd))
                    .block_task()
                    .await?;
                let response = connection
                    .send_request(PromptRequest::new(
                        session.session_id,
                        vec![ContentBlock::Text(TextContent::new(prompt))],
                    ))
                    .block_task()
                    .await?;
                Ok(response.stop_reason)
            })
            .await
            .map_err(|e| e.to_string())?;

        // Drain queued events in arrival order, then finish.
        let events = std::mem::take(&mut *queue.lock().unwrap());
        let had_text = events.iter().any(|e| matches!(e, StreamEvent::Token { .. }));
        for ev in events {
            sink.emit(ev);
        }
        if had_text {
            sink.emit(StreamEvent::Done);
        } else {
            sink.emit(StreamEvent::Error {
                message: format!("claude-code returned no text (stop reason: {stop:?})"),
            });
        }
        Ok(())
    }
}
```

> Note: this drains after the run (matching today's buffered behavior) rather than truly mid-flight, because the ACP `Client` handler owns a `'static` closure. True mid-flight forwarding requires handing the sink into the closure via a channel — out of scope here; the UI still renders progressively because `tauriEngine` yields each event. If mid-flight streaming is wanted, thread a `std::sync::mpsc`/`tokio` sender into the closure in a follow-up.

- [ ] **Step 2: Delete `merge_chunk` and its tests.** Remove the `fn merge_chunk` definition (kept alive in Task 2) and any remaining `merge_chunk_*` tests not already replaced.

- [ ] **Step 3: Remove now-dead `ask` references.** Confirm nothing calls `ask` (only `send_prompt` did; it is replaced in Task 5). `grep -rn "\.ask(" apps/desktop/src-tauri/src` should return nothing after Task 5; for now the trait no longer has `ask`, so this compiles.

- [ ] **Step 4: Build + test.**

Run: `cargo test -p manch-desktop 2>&1 | tail -25`
Expected: PASS — all `agent.rs` unit tests green; crate compiles (the `send_prompt` command in `commands.rs` still calls `ask` and will fail to compile — proceed to Task 5 and commit together, OR temporarily comment out `send_prompt` registration).

- [ ] **Step 5: Commit (Tasks 3+4 together).**

```bash
just fmt && just clippy
git add apps/desktop/src-tauri/src/agent.rs apps/desktop/src-tauri/Cargo.toml Cargo.lock
git commit -m "feat(desktop): stream Anthropic (SSE) and Claude Code (ACP chunks + tool calls) via EventSink"
```

---

### Task 5: `send_prompt_stream` command + registration

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands.rs` (replace `send_prompt` with `send_prompt_stream`)
- Modify: `apps/desktop/src-tauri/src/lib.rs` (swap the handler registration)

**Interfaces:**
- Consumes: `EventSink`, `ChatAgent::stream`, `AnthropicAgent`, `ClaudeCodeAgent`, `manch_dto::StreamEvent`, `tauri::ipc::Channel`.
- Produces: `#[tauri::command] async fn send_prompt_stream(state, provider: String, text: String, channel: tauri::ipc::Channel<StreamEvent>) -> Result<(), String>`.

- [ ] **Step 1: Add a `ChannelSink` + replace the command.** In `commands.rs`, replace the whole `send_prompt` fn with:

```rust
use manch_dto::StreamEvent;
use tauri::ipc::Channel;

/// `EventSink` that forwards each event over a Tauri IPC channel to the frontend.
struct ChannelSink(Channel<StreamEvent>);
impl crate::agent::EventSink for ChannelSink {
    fn emit(&self, event: StreamEvent) {
        // A closed channel (window gone) is not actionable here.
        let _ = self.0.send(event);
    }
}

#[tauri::command]
pub async fn send_prompt_stream(
    state: State<'_, Db>,
    provider: String,
    text: String,
    channel: Channel<StreamEvent>,
) -> Result<(), String> {
    let prov =
        Provider::from_id(&provider).ok_or_else(|| format!("unknown provider: {provider}"))?;
    // Resolve owned keys here; the mutex guard is released inside `get_key`,
    // never held across the network/subprocess await below.
    let agent: Box<dyn ChatAgent> = match prov {
        Provider::Anthropic => {
            let key = state
                .get_key("anthropic")
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "no API key saved for anthropic".to_string())?;
            Box::new(AnthropicAgent::new(key))
        }
        Provider::ClaudeCode => {
            let key = state.get_key("claude-code").map_err(|e| e.to_string())?;
            Box::new(ClaudeCodeAgent::new(key))
        }
    };
    let sink = ChannelSink(channel);
    agent.stream(&text, &sink).await
}
```

Ensure the `use crate::agent::{…}` line at the top of `commands.rs` still imports `AnthropicAgent, ChatAgent, ClaudeCodeAgent, Provider, offerable_providers` (unchanged).

- [ ] **Step 2: Swap the registration.** In `lib.rs`, in `tauri::generate_handler![…]`, replace `commands::send_prompt,` with `commands::send_prompt_stream,`.

- [ ] **Step 3: Build the whole crate.**

Run: `cargo build -p manch-desktop 2>&1 | tail -20`
Expected: builds clean (no `merge_chunk`/`ask` references remain).

- [ ] **Step 4: Full Rust checks.**

Run: `just clippy && cargo test -p manch-desktop 2>&1 | tail -15`
Expected: clippy clean; all tests PASS.

- [ ] **Step 5: Commit.**

```bash
just fmt
git add apps/desktop/src-tauri/src/commands.rs apps/desktop/src-tauri/src/lib.rs
git commit -m "feat(desktop): send_prompt_stream command over a typed Channel"
```

---

### Task 6: `tauriEngine` bridge + Stage swap

**Files:**
- Modify: `apps/desktop/src/engine/tauriEngine.ts` (bridge Channel → `AsyncIterable<StageEvent>`)
- Test: `apps/desktop/src/engine/tauriEngine.test.ts` (create)
- Modify: `apps/desktop/src/containers/Stage.tsx` (import `tauriEngine` instead of `mockEngine`)

**Interfaces:**
- Consumes: `manch_dto` `StreamEvent` (via `bindings.ts`), `@tauri-apps/api/core` `{ invoke, Channel }`, `StageEngine`/`StageEvent`.
- Produces: `tauriEngine: StageEngine` streaming real events.

- [ ] **Step 1: Write the failing test.** Create `apps/desktop/src/engine/tauriEngine.test.ts`:

```ts
import { describe, it, expect, vi } from "vitest";

// A fake Tauri Channel: captures the assigned onmessage, lets the test push
// StreamEvents, and records the invoke it was passed to.
class FakeChannel {
  onmessage: ((m: unknown) => void) | null = null;
}
const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...a: unknown[]) => invoke(...a),
  Channel: FakeChannel,
}));

import { tauriEngine } from "./tauriEngine";
import type { StageEvent } from "./StageEngine";

async function collect(
  provider: string,
  text: string,
  script: (ch: FakeChannel) => void,
): Promise<StageEvent[]> {
  invoke.mockImplementation((_cmd: string, args: { channel: FakeChannel }) => {
    // Drive the channel on the next tick, then resolve the command.
    queueMicrotask(() => script(args.channel));
    return Promise.resolve();
  });
  const out: StageEvent[] = [];
  for await (const ev of tauriEngine.send(provider, text)) out.push(ev);
  return out;
}

describe("tauriEngine", () => {
  it("maps StreamEvents to StageEvents in order and terminates on done", async () => {
    const events = await collect("anthropic", "hi", (ch) => {
      ch.onmessage?.({ kind: "token", text: "He" });
      ch.onmessage?.({ kind: "token", text: "llo" });
      ch.onmessage?.({ kind: "tool", id: "t1", name: "read", status: "running", detail: "x.rs" });
      ch.onmessage?.({ kind: "tool", id: "t1", name: "read", status: "done", detail: "x.rs" });
      ch.onmessage?.({ kind: "done" });
    });
    expect(events).toEqual([
      { kind: "token", text: "He" },
      { kind: "token", text: "llo" },
      { kind: "tool", id: "t1", name: "read", status: "running", detail: "x.rs" },
      { kind: "tool", id: "t1", name: "read", status: "done", detail: "x.rs" },
      { kind: "done" },
    ]);
    expect(invoke).toHaveBeenCalledWith(
      "send_prompt_stream",
      expect.objectContaining({ provider: "anthropic", text: "hi" }),
    );
  });

  it("surfaces an error event and stops", async () => {
    const events = await collect("anthropic", "hi", (ch) => {
      ch.onmessage?.({ kind: "error", message: "boom" });
    });
    expect(events).toEqual([{ kind: "error", message: "boom" }]);
  });
});
```

- [ ] **Step 2: Run test to verify it fails.**

Run: `pnpm --filter @manch/desktop test -- --run tauriEngine 2>&1 | tail -20`
Expected: FAIL — current `tauriEngine` calls `sendPrompt`, ignores the Channel; assertions mismatch.

- [ ] **Step 3: Implement the bridge.** Replace `apps/desktop/src/engine/tauriEngine.ts` with:

```ts
import { invoke, Channel } from "@tauri-apps/api/core";
import { isProvider } from "../lib/providers";
import type { StreamEvent } from "../data/bindings";
import type { StageEngine, StageEvent } from "./StageEngine";

/** Map a wire StreamEvent onto the internal StageEvent (near-identity; null→undefined). */
function toStageEvent(e: StreamEvent): StageEvent {
  switch (e.kind) {
    case "token":
      return { kind: "token", text: e.text };
    case "tool":
      return { kind: "tool", id: e.id, name: e.name, status: e.status as StageEvent extends { kind: "tool" } ? StageEvent["status"] : never, detail: e.detail ?? undefined };
    case "done":
      return { kind: "done" };
    case "error":
      return { kind: "error", message: e.message };
  }
}

export const tauriEngine: StageEngine = {
  async *send(provider: string, text: string): AsyncIterable<StageEvent> {
    if (!isProvider(provider)) {
      yield { kind: "error", message: `Unknown provider: ${provider}` };
      return;
    }

    // Buffer channel messages into a queue the generator drains; resolve a
    // pending waiter as each event arrives. Terminates on done/error.
    const queue: StageEvent[] = [];
    let notify: (() => void) | null = null;
    let finished = false;

    const channel = new Channel<StreamEvent>();
    channel.onmessage = (msg) => {
      const ev = toStageEvent(msg);
      queue.push(ev);
      if (ev.kind === "done" || ev.kind === "error") finished = true;
      notify?.();
    };

    const done = invoke("send_prompt_stream", { provider, text, channel }).catch(
      (e: unknown): void => {
        queue.push({ kind: "error", message: typeof e === "string" ? e : String(e) });
        finished = true;
        notify?.();
      },
    );

    // Drain until a terminal event has been yielded.
    for (;;) {
      while (queue.length > 0) {
        const ev = queue.shift() as StageEvent;
        yield ev;
        if (ev.kind === "done" || ev.kind === "error") {
          await done;
          return;
        }
      }
      if (finished && queue.length === 0) {
        await done;
        return;
      }
      await new Promise<void>((r) => {
        notify = r;
      });
      notify = null;
    }
  },
};
```

> If the `status as …` conditional-type cast trips the typechecker, simplify to `status: e.status as "running" | "done" | "error"` — `StageEvent`'s tool `status` is `ToolCallData["status"]`, which is that union.

- [ ] **Step 4: Simplify the cast if needed, then run the test.**

Run: `pnpm --filter @manch/desktop test -- --run tauriEngine 2>&1 | tail -20`
Expected: PASS (both tests).

- [ ] **Step 5: Swap the Stage engine.** In `apps/desktop/src/containers/Stage.tsx`, change the import `import { mockEngine } from "../engine/mockEngine";` to `import { tauriEngine } from "../engine/tauriEngine";` and the hook `const { send, busy } = useSend(mockEngine);` to `useSend(tauriEngine)`. Leave `mockEngine` in the repo (used by its own test and available for stories).

- [ ] **Step 6: Typecheck + full JS tests.**

Run: `just lint && pnpm --filter @manch/desktop test -- --run 2>&1 | tail -15`
Expected: typecheck clean; all desktop tests PASS (Stage tests inject their own mock engine via `vi.mock("../engine/mockEngine")` — confirm they still pass; if a Stage test mocked `mockEngine` specifically, update it to mock `../engine/tauriEngine`).

> **Watch-out:** `Stage.test.tsx` currently does `vi.mock("../engine/mockEngine", …)`. After Step 5 the Stage imports `tauriEngine`, so update that mock target to `"../engine/tauriEngine"` (same fake `send` generator). Re-run the Stage suite to confirm green.

- [ ] **Step 7: Commit.**

```bash
git add apps/desktop/src/engine/tauriEngine.ts apps/desktop/src/engine/tauriEngine.test.ts apps/desktop/src/containers/Stage.tsx apps/desktop/src/containers/Stage.test.tsx
git commit -m "feat(desktop): live tauriEngine streaming over Channel; swap Stage off mockEngine"
```

---

### Task 7: Full CI + manual streaming verification

- [ ] **Step 1: Run the whole CI gate.**

Run: `just ci 2>&1 | tail -20`
Expected: `✓ CI checks passed`.

- [ ] **Step 2: Manual smoke (Anthropic).** With an Anthropic key saved in Settings, send a Chat prompt; confirm tokens render progressively and the transcript finalizes. (Network path — not unit-tested.)

- [ ] **Step 3: Manual smoke (Claude Code).** Select `claude-code`, send a prompt that triggers a tool (e.g. "read a file and summarize"); confirm tool cards appear (running→done) and text renders. (Subprocess path — not unit-tested.)

- [ ] **Step 4: Push + PR.**

```bash
git push -u origin <branch>
gh pr create --base main --title "feat(desktop): real ACP streaming (tokens + tool calls) — #17 part A" --body "…Refs #17"
```

## Self-review notes

- Spec coverage: StreamEvent DTO (T1); EventSink + SSE/tool helpers (T2); Anthropic SSE (T3); Claude Code forward + tool calls (T4); command+transport (T5); tauriEngine bridge + Stage swap (T6); CI + manual (T7). All Part-A goals covered.
- The one deliberate simplification vs. spec: Claude Code events drain **after** the ACP run (as today), not truly mid-flight, due to the `'static` handler closure — noted in T4 with the follow-up path. Anthropic is genuinely mid-flight.
- `push_chunk`/`parse_sse_delta`/`tool_status` are the unit-tested seams; ACP + network paths are manually verified (consistent with the existing code, which never unit-tested the ACP path).
