# manch-llm + manch-acp Provider Crates Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract the inline BYOK + ACP agent logic into two reusable crates (`manch-llm`, `manch-acp`), add Gemini + Codex on both paths, with fetched/user-selectable BYOK models.

**Architecture:** Both crates implement the existing `manch_protocol::Agent` trait and emit ACP event vocabulary (`AgentEvent`). `manch-llm` = direct provider HTTP clients (Anthropic/Gemini/OpenAI) behind Cargo features, no execution surface. `manch-acp` = one generic ACP-subprocess agent parameterized by a per-CLI launch spec. The desktop maps `AgentEvent` → `manch_dto::StreamEvent` at the UI edge.

**Tech Stack:** Rust (edition 2024), `reqwest` + `rustls` (BYOK HTTP/SSE), `agent-client-protocol` (ACP), `rusqlite` (desktop), Tauri, React 19 (desktop frontend).

**Design spec:** `docs/superpowers/specs/2026-07-02-manch-llm-acp-provider-crates-design.md`

## Global Constraints

- Rust ≥ 1.88, edition 2024. Clippy is `-D warnings`; rustfmt enforced. Gate every task with the commands shown; final gate is `just ci`.
- **Both new crates are `publish = false`** (names/APIs still churning).
- **`manch-llm` has no execution surface:** no `std::process`, no filesystem-write APIs. Ever.
- **Speak ACP's vocabulary** — do not define parallel content/event enums. Emit `manch_protocol::AgentEvent` / `acp::SessionUpdate`.
- **No `rig`.** Hand-rolled `reqwest` clients; per-provider differences are consts/data.
- Anthropic model id `claude-opus-4-8` is authoritative — do not change it.
- Conventional Commits (`feat:`, `refactor:`, `docs:`, …). Commit at the end of every task.
- All work on branch `feat/manch-llm-acp-provider-crates` (already created).

## File Structure

**Created:**
- `crates/manch-llm/Cargo.toml`, `crates/manch-llm/src/lib.rs` (shared helpers + dispatch), `src/anthropic.rs`, `src/gemini.rs`, `src/openai.rs`
- `crates/manch-acp/Cargo.toml`, `crates/manch-acp/src/lib.rs` (generic `AcpCliAgent` + builders)

**Modified:**
- `Cargo.toml` (root) — add both crates to `members`
- `crates/manch-protocol/src/lib.rs` — re-export `TextContent`; add `AgentEvent::text_chunk`
- `apps/desktop/src-tauri/Cargo.toml` — swap inline deps for the two crates
- `apps/desktop/src-tauri/src/agent.rs` — reduce to the desktop-only mapping (`ChannelSink`, `tool_status`, `resolve_agent`); delete the moved provider code
- `apps/desktop/src-tauri/src/commands.rs` — `send_prompt_stream` builds `Context`; add `list_models` command; widen provider validation
- `apps/desktop/src-tauri/src/db.rs` — persist selected model per provider
- `apps/desktop/src/*` — model dropdown (exact files located in Task 10)

---

### Task 1: Scaffold both crates and register them in the workspace

**Files:**
- Create: `crates/manch-llm/Cargo.toml`, `crates/manch-llm/src/lib.rs`
- Create: `crates/manch-acp/Cargo.toml`, `crates/manch-acp/src/lib.rs`
- Modify: `Cargo.toml` (root)

**Interfaces:**
- Produces: two compiling library crates `manch-llm`, `manch-acp`.

- [ ] **Step 1: Add both crates to the workspace members**

In root `Cargo.toml`, change the members line to:

```toml
members = ["crates/manch-protocol", "crates/manch-dto", "crates/manch-llm", "crates/manch-acp", "apps/server", "apps/desktop/src-tauri"]
```

- [ ] **Step 2: Create `crates/manch-llm/Cargo.toml`**

```toml
[package]
name = "manch-llm"
description = "BYOK provider clients for Manch (Anthropic, Gemini, OpenAI), implementing manch_protocol::Agent."
version = "0.0.1"
edition.workspace = true
license.workspace = true
repository.workspace = true
rust-version.workspace = true
publish = false

[features]
default = ["anthropic", "gemini", "openai"]
anthropic = []
gemini = []
openai = []

[dependencies]
manch-protocol = { workspace = true }
async-trait = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
futures-util = "0.3"
reqwest = { version = "0.13", default-features = false, features = ["json", "rustls-no-provider", "stream"] }
rustls = { version = "0.23", default-features = false, features = ["ring"] }
```

- [ ] **Step 3: Create `crates/manch-llm/src/lib.rs` (placeholder)**

```rust
//! BYOK provider clients for Manch — direct provider HTTP/SSE, no execution surface.
//! Each provider implements `manch_protocol::Agent` and emits ACP event vocabulary.
```

- [ ] **Step 4: Create `crates/manch-acp/Cargo.toml`**

```toml
[package]
name = "manch-acp"
description = "Framework-agnostic ACP host for Manch — wraps external CLI agents (Claude Code, Gemini CLI, Codex) as manch_protocol::Agent."
version = "0.0.1"
edition.workspace = true
license.workspace = true
repository.workspace = true
rust-version.workspace = true
publish = false

[dependencies]
manch-protocol = { workspace = true }
async-trait = { workspace = true }
agent-client-protocol = { workspace = true, features = ["unstable"] }
tokio = { workspace = true, features = ["rt-multi-thread", "macros"] }
```

- [ ] **Step 5: Create `crates/manch-acp/src/lib.rs` (placeholder)**

```rust
//! Framework-agnostic ACP host — one generic subprocess agent parameterized by launch spec.
```

- [ ] **Step 6: Verify both crates build**

Run: `cargo build -p manch-llm -p manch-acp`
Expected: PASS (two empty crates compile).

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml crates/manch-llm crates/manch-acp
git commit -m "feat: scaffold manch-llm and manch-acp crates"
```

---

### Task 2: manch-protocol — re-export `TextContent` and add `AgentEvent::text_chunk`

**Files:**
- Modify: `crates/manch-protocol/src/lib.rs`

**Interfaces:**
- Produces: `manch_protocol::acp::TextContent`; `manch_protocol::AgentEvent::text_chunk(impl Into<String>) -> AgentEvent`. Both `manch-llm` and `manch-acp` construct message chunks through this one helper (DRY, hides ACP construction).

- [ ] **Step 1: Write the failing test**

Add to `crates/manch-protocol/src/lib.rs` (in a `#[cfg(test)] mod tests` — create the module if absent):

```rust
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p manch-protocol text_chunk`
Expected: FAIL — `text_chunk` not found / `TextContent` not in scope.

- [ ] **Step 3: Add the re-export and the helper**

In the `pub mod acp { pub use ... }` block, add `TextContent`, `ContentChunk`, and `AgentMessageChunk`'s carrier as needed — extend the `use` list to include `TextContent`:

```rust
pub mod acp {
    pub use agent_client_protocol::schema::v1::{
        ContentBlock, ContentChunk, PromptRequest, PromptResponse, SessionNotification,
        SessionUpdate, StopReason, TextContent, ToolCall, ToolCallContent, ToolCallStatus,
        ToolCallUpdate, ToolCallUpdateFields, ToolKind,
    };
}
```

Add the constructor to `AgentEvent` (below its definition):

```rust
impl AgentEvent {
    /// Convenience: an agent message text chunk in ACP vocabulary. The one place
    /// BYOK and ACP agents construct streamed text, so the ACP wrapping lives here.
    pub fn text_chunk(text: impl Into<String>) -> AgentEvent {
        use acp::{AgentMessageChunkNamespace as _, ContentBlock, ContentChunk, SessionUpdate, TextContent};
        AgentEvent::Update(SessionUpdate::AgentMessageChunk(ContentChunk::new(
            ContentBlock::Text(TextContent::new(text.into())),
        )))
    }
}
```

> NOTE: `ContentChunk::new` / `TextContent::new` / the `AgentMessageChunk` variant shape come from `agent-client-protocol` v1. If a constructor name differs, match the crate's actual API (it is already a dependency; `TextContent::new(..)` is used today in `apps/desktop/src-tauri/src/agent.rs:378`). Remove the bogus `AgentMessageChunkNamespace` import — it is a placeholder to force you to check the real path; the correct form is `SessionUpdate::AgentMessageChunk(ContentChunk::new(...))`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p manch-protocol text_chunk`
Expected: PASS.

- [ ] **Step 5: Confirm no wider breakage**

Run: `cargo build -p manch-protocol && cargo clippy -p manch-protocol -- -D warnings`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/manch-protocol/src/lib.rs
git commit -m "feat(protocol): re-export TextContent and add AgentEvent::text_chunk"
```

---

### Task 3: manch-llm shared core (helpers used by all providers)

**Files:**
- Modify: `crates/manch-llm/src/lib.rs`

**Interfaces:**
- Produces:
  - `pub struct ModelInfo { pub id: String, pub display_name: Option<String> }` (`Clone`, `serde::Serialize`, `Debug`, `PartialEq`)
  - `pub(crate) enum SseItem { Text(String), Error(String) }`
  - `pub(crate) fn drain_sse(buf: &mut Vec<u8>, parse: impl Fn(&str) -> Option<SseItem>) -> Vec<SseItem>`
  - `pub(crate) fn prompt_text(ctx: &manch_protocol::Context) -> String`
  - `pub(crate) fn ensure_crypto_provider()`
  - `pub(crate) fn err(e: impl ToString) -> manch_protocol::Error`

- [ ] **Step 1: Write the failing tests**

Replace `crates/manch-llm/src/lib.rs` with the module doc + these tests appended:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use manch_protocol::acp::{ContentBlock, TextContent};
    use manch_protocol::Context;

    #[test]
    fn drain_sse_extracts_data_lines_and_leaves_partial() {
        let mut buf = b"data: {\"t\":1}\ndata: partial".to_vec();
        let items = drain_sse(&mut buf, |d| Some(SseItem::Text(d.to_string())));
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], SseItem::Text(s) if s == "{\"t\":1}"));
        assert_eq!(String::from_utf8_lossy(&buf), "data: partial"); // partial retained
    }

    #[test]
    fn prompt_text_joins_user_text_blocks() {
        let ctx = Context {
            session_id: "s1".into(),
            blocks: vec![
                ContentBlock::Text(TextContent::new("hello".to_string())),
                ContentBlock::Text(TextContent::new("world".to_string())),
            ],
        };
        assert_eq!(prompt_text(&ctx), "hello\nworld");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p manch-llm`
Expected: FAIL — `drain_sse` / `prompt_text` not defined.

- [ ] **Step 3: Implement the shared core**

Prepend to `crates/manch-llm/src/lib.rs` (above the tests):

```rust
//! BYOK provider clients for Manch — direct provider HTTP/SSE, no execution surface.
//! Each provider implements `manch_protocol::Agent` and emits ACP event vocabulary.

use manch_protocol::Context;
use manch_protocol::acp::ContentBlock;

/// A model advertised by a provider's list-models endpoint.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub display_name: Option<String>,
}

/// One parsed SSE line: streamed text, or a surfaced error message.
pub(crate) enum SseItem {
    Text(String),
    Error(String),
}

/// Drain complete `\n`-terminated lines from `buf`, applying `parse` to each
/// line's trimmed `data:` payload. Any trailing partial line is retained in
/// `buf`. Splitting on the ASCII `\n` byte keeps multibyte UTF-8 sequences
/// (Devanagari, CJK, emoji) whole across network chunk boundaries.
pub(crate) fn drain_sse(buf: &mut Vec<u8>, parse: impl Fn(&str) -> Option<SseItem>) -> Vec<SseItem> {
    let mut out = Vec::new();
    while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
        let line_bytes: Vec<u8> = buf.drain(..=pos).collect();
        let line = String::from_utf8_lossy(&line_bytes);
        let line = line.trim();
        if let Some(data) = line.strip_prefix("data:") {
            if let Some(item) = parse(data.trim()) {
                out.push(item);
            }
        }
    }
    out
}

/// Flatten a `Context` into a single prompt string (concatenated user text
/// blocks). Placeholder until real memory/multi-turn assembly lands.
pub(crate) fn prompt_text(ctx: &Context) -> String {
    ctx.blocks
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Install the `ring` rustls crypto provider once (reqwest `rustls-no-provider`
/// ships no backend; first `Client` build would panic otherwise).
pub(crate) fn ensure_crypto_provider() {
    use std::sync::OnceLock;
    static INSTALLED: OnceLock<()> = OnceLock::new();
    INSTALLED.get_or_init(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// Map any error into `manch_protocol::Error::Other`.
pub(crate) fn err(e: impl ToString) -> manch_protocol::Error {
    manch_protocol::Error::Other(e.to_string())
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p manch-llm`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/manch-llm/src/lib.rs
git commit -m "feat(llm): shared BYOK core — SSE framing, prompt flattening, ModelInfo"
```

---

### Task 4: manch-llm Anthropic provider

**Files:**
- Create: `crates/manch-llm/src/anthropic.rs`
- Modify: `crates/manch-llm/src/lib.rs` (add `#[cfg(feature = "anthropic")] mod anthropic; pub use`)

**Interfaces:**
- Consumes: `SseItem`, `drain_sse`, `prompt_text`, `ensure_crypto_provider`, `err`, `ModelInfo` (Task 3); `AgentEvent::text_chunk`, `acp::StopReason` (Task 2); `manch_protocol::{Agent, Context, EventSink, ToolSchema}`.
- Produces: `pub struct AnthropicAgent`; `AnthropicAgent::new(api_key: String, model: Option<String>) -> Self`; `impl manch_protocol::Agent`; `pub async fn list_models(api_key: &str) -> manch_protocol::Result<Vec<ModelInfo>>`.

- [ ] **Step 1: Write the failing tests**

Create `crates/manch-llm/src/anthropic.rs` with pure-fn tests first (adapt the existing tests in `apps/desktop/src-tauri/src/agent.rs:414-442,515-536`):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_body_has_model_and_user_message() {
        let body = request_body("claude-opus-4-8", "hi");
        assert_eq!(body["model"], "claude-opus-4-8");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "hi");
        assert_eq!(body["stream"], true);
    }

    #[test]
    fn parse_line_extracts_text_delta() {
        let d = r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hi"}}"#;
        assert!(matches!(parse_line(d), Some(crate::SseItem::Text(t)) if t == "Hi"));
    }

    #[test]
    fn parse_line_surfaces_stream_error() {
        let d = r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#;
        assert!(matches!(parse_line(d), Some(crate::SseItem::Error(e)) if e == "anthropic: Overloaded"));
    }

    #[test]
    fn parse_models_reads_id_and_display_name() {
        let body = serde_json::json!({
            "data": [{ "id": "claude-opus-4-8", "display_name": "Claude Opus 4.8" }]
        });
        let models = parse_models(&body);
        assert_eq!(models[0].id, "claude-opus-4-8");
        assert_eq!(models[0].display_name.as_deref(), Some("Claude Opus 4.8"));
    }

    #[test]
    fn new_uses_fallback_when_model_none() {
        let a = AnthropicAgent::new("k".into(), None);
        assert_eq!(a.model, FALLBACK_MODEL);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p manch-llm --features anthropic anthropic::`
Expected: FAIL — module/functions not defined.

- [ ] **Step 3: Implement the Anthropic provider**

Prepend to `crates/manch-llm/src/anthropic.rs`:

```rust
//! BYOK Anthropic Messages API client.

use async_trait::async_trait;
use futures_util::StreamExt;
use manch_protocol::acp::StopReason;
use manch_protocol::{Agent, AgentEvent, Context, EventSink, Result, ToolSchema};

use crate::{drain_sse, ensure_crypto_provider, err, prompt_text, ModelInfo, SseItem};

const URL: &str = "https://api.anthropic.com/v1/messages";
const MODELS_URL: &str = "https://api.anthropic.com/v1/models";
const VERSION: &str = "2023-06-01";
const MAX_TOKENS: u32 = 1024;
pub(crate) const FALLBACK_MODEL: &str = "claude-opus-4-8"; // authoritative — do not change

/// BYOK Anthropic via a hand-rolled Messages-API call.
pub struct AnthropicAgent {
    api_key: String,
    model: String,
}

impl AnthropicAgent {
    pub fn new(api_key: String, model: Option<String>) -> Self {
        Self { api_key, model: model.unwrap_or_else(|| FALLBACK_MODEL.to_string()) }
    }
}

/// Build the Messages API request body. Pure.
pub(crate) fn request_body(model: &str, prompt: &str) -> serde_json::Value {
    serde_json::json!({
        "model": model,
        "max_tokens": MAX_TOKENS,
        "stream": true,
        "messages": [{ "role": "user", "content": prompt }],
    })
}

/// Parse one SSE `data:` payload into text or a surfaced error. Pure.
pub(crate) fn parse_line(data: &str) -> Option<SseItem> {
    let v: serde_json::Value = serde_json::from_str(data).ok()?;
    match v.get("type")?.as_str()? {
        "content_block_delta" => {
            let delta = v.get("delta")?;
            if delta.get("type")?.as_str()? == "text_delta" {
                return Some(SseItem::Text(delta.get("text")?.as_str()?.to_string()));
            }
            None
        }
        "error" => {
            let msg = v.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str())
                .unwrap_or("stream error");
            Some(SseItem::Error(format!("anthropic: {msg}")))
        }
        _ => None,
    }
}

/// Surface `error.message` from a non-stream error body. Pure.
fn error_message(body: &serde_json::Value) -> Option<String> {
    let msg = body.get("error")?.get("message")?.as_str()?;
    Some(format!("anthropic: {msg}"))
}

/// Parse the list-models response into a catalog. Pure.
pub(crate) fn parse_models(body: &serde_json::Value) -> Vec<ModelInfo> {
    body.get("data").and_then(|d| d.as_array()).map(|arr| {
        arr.iter().filter_map(|m| {
            Some(ModelInfo {
                id: m.get("id")?.as_str()?.to_string(),
                display_name: m.get("display_name").and_then(|n| n.as_str()).map(String::from),
            })
        }).collect()
    }).unwrap_or_default()
}

/// Fetch the available models for this key (falls back to the default id on failure).
pub async fn list_models(api_key: &str) -> Result<Vec<ModelInfo>> {
    ensure_crypto_provider();
    let resp = reqwest::Client::new()
        .get(MODELS_URL)
        .header("x-api-key", api_key)
        .header("anthropic-version", VERSION)
        .send().await;
    match resp {
        Ok(r) if r.status().is_success() => {
            let body: serde_json::Value = r.json().await.map_err(err)?;
            let models = parse_models(&body);
            Ok(if models.is_empty() { vec![fallback_model()] } else { models })
        }
        _ => Ok(vec![fallback_model()]),
    }
}

fn fallback_model() -> ModelInfo {
    ModelInfo { id: FALLBACK_MODEL.to_string(), display_name: None }
}

#[async_trait]
impl Agent for AnthropicAgent {
    fn id(&self) -> &str { "anthropic" }

    async fn prompt(&self, ctx: Context, _tools: &[ToolSchema], sink: &dyn EventSink) -> Result<StopReason> {
        ensure_crypto_provider();
        let prompt = prompt_text(&ctx);
        let resp = reqwest::Client::new()
            .post(URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", VERSION)
            .json(&request_body(&self.model, &prompt))
            .send().await.map_err(err)?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body: serde_json::Value = resp.json().await.map_err(err)?;
            return Err(crate::err(error_message(&body).unwrap_or_else(|| format!("anthropic: HTTP {status}"))));
        }

        let mut stream = resp.bytes_stream();
        let mut buf: Vec<u8> = Vec::new();
        while let Some(chunk) = stream.next().await {
            buf.extend_from_slice(&chunk.map_err(err)?);
            for item in drain_sse(&mut buf, parse_line) {
                match item {
                    SseItem::Text(t) => sink.emit(AgentEvent::text_chunk(t)).await?,
                    SseItem::Error(e) => return Err(crate::err(e)),
                }
            }
        }
        sink.emit(AgentEvent::Done(StopReason::EndTurn)).await?;
        Ok(StopReason::EndTurn)
    }
}
```

> NOTE: Confirm the `StopReason` variant name against `agent-client-protocol` v1 (`EndTurn` is the expected "completed" reason). If the enum uses a different spelling, use that.

- [ ] **Step 4: Register the module**

In `crates/manch-llm/src/lib.rs`, add after the `use` lines:

```rust
#[cfg(feature = "anthropic")]
pub mod anthropic;
#[cfg(feature = "anthropic")]
pub use anthropic::AnthropicAgent;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p manch-llm --features anthropic`
Expected: PASS.

- [ ] **Step 6: Clippy**

Run: `cargo clippy -p manch-llm --features anthropic -- -D warnings`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/manch-llm/src/anthropic.rs crates/manch-llm/src/lib.rs
git commit -m "feat(llm): Anthropic BYOK agent implementing manch_protocol::Agent"
```

---

### Task 5: manch-llm Gemini provider

**Files:**
- Create: `crates/manch-llm/src/gemini.rs`
- Modify: `crates/manch-llm/src/lib.rs`

**Interfaces:**
- Consumes: same shared core as Task 4.
- Produces: `pub struct GeminiAgent`; `GeminiAgent::new(api_key: String, model: Option<String>)`; `impl Agent`; `pub async fn list_models(api_key: &str) -> Result<Vec<ModelInfo>>`.

- [ ] **Step 1: Write the failing tests**

Create `crates/manch-llm/src/gemini.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_body_has_user_part() {
        let body = request_body("hi");
        assert_eq!(body["contents"][0]["role"], "user");
        assert_eq!(body["contents"][0]["parts"][0]["text"], "hi");
    }

    #[test]
    fn parse_line_extracts_candidate_text() {
        let d = r#"{"candidates":[{"content":{"role":"model","parts":[{"text":"Hi"}]}}]}"#;
        assert!(matches!(parse_line(d), Some(crate::SseItem::Text(t)) if t == "Hi"));
    }

    #[test]
    fn parse_line_surfaces_error() {
        let d = r#"{"error":{"code":400,"message":"bad key"}}"#;
        assert!(matches!(parse_line(d), Some(crate::SseItem::Error(e)) if e == "gemini: bad key"));
    }

    #[test]
    fn parse_models_strips_models_prefix() {
        let body = serde_json::json!({
            "models": [{ "name": "models/gemini-3-flash", "displayName": "Gemini 3 Flash" }]
        });
        let models = parse_models(&body);
        assert_eq!(models[0].id, "gemini-3-flash");
        assert_eq!(models[0].display_name.as_deref(), Some("Gemini 3 Flash"));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p manch-llm --features gemini gemini::`
Expected: FAIL.

- [ ] **Step 3: Implement the Gemini provider**

Prepend to `crates/manch-llm/src/gemini.rs`:

```rust
//! BYOK Gemini `generateContent` client (SSE streaming via `?alt=sse`).

use async_trait::async_trait;
use futures_util::StreamExt;
use manch_protocol::acp::StopReason;
use manch_protocol::{Agent, AgentEvent, Context, EventSink, Result, ToolSchema};

use crate::{drain_sse, ensure_crypto_provider, err, prompt_text, ModelInfo, SseItem};

const BASE: &str = "https://generativelanguage.googleapis.com/v1beta";
pub(crate) const FALLBACK_MODEL: &str = "gemini-3-flash";

pub struct GeminiAgent {
    api_key: String,
    model: String,
}

impl GeminiAgent {
    pub fn new(api_key: String, model: Option<String>) -> Self {
        Self { api_key, model: model.unwrap_or_else(|| FALLBACK_MODEL.to_string()) }
    }
}

/// Pure request body: a single user turn.
pub(crate) fn request_body(prompt: &str) -> serde_json::Value {
    serde_json::json!({ "contents": [{ "role": "user", "parts": [{ "text": prompt }] }] })
}

/// Parse one SSE line: concatenate the candidate's text parts, or surface an error. Pure.
pub(crate) fn parse_line(data: &str) -> Option<SseItem> {
    let v: serde_json::Value = serde_json::from_str(data).ok()?;
    if let Some(msg) = v.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str()) {
        return Some(SseItem::Error(format!("gemini: {msg}")));
    }
    let parts = v.get("candidates")?.as_array()?.first()?
        .get("content")?.get("parts")?.as_array()?;
    let text: String = parts.iter().filter_map(|p| p.get("text").and_then(|t| t.as_str())).collect();
    (!text.is_empty()).then_some(SseItem::Text(text))
}

/// Parse list-models response; ids drop the `models/` prefix. Pure.
pub(crate) fn parse_models(body: &serde_json::Value) -> Vec<ModelInfo> {
    body.get("models").and_then(|m| m.as_array()).map(|arr| {
        arr.iter().filter_map(|m| {
            let name = m.get("name")?.as_str()?;
            Some(ModelInfo {
                id: name.strip_prefix("models/").unwrap_or(name).to_string(),
                display_name: m.get("displayName").and_then(|n| n.as_str()).map(String::from),
            })
        }).collect()
    }).unwrap_or_default()
}

fn fallback_model() -> ModelInfo { ModelInfo { id: FALLBACK_MODEL.to_string(), display_name: None } }

pub async fn list_models(api_key: &str) -> Result<Vec<ModelInfo>> {
    ensure_crypto_provider();
    let url = format!("{BASE}/models");
    let resp = reqwest::Client::new().get(url).header("x-goog-api-key", api_key).send().await;
    match resp {
        Ok(r) if r.status().is_success() => {
            let body: serde_json::Value = r.json().await.map_err(err)?;
            let models = parse_models(&body);
            Ok(if models.is_empty() { vec![fallback_model()] } else { models })
        }
        _ => Ok(vec![fallback_model()]),
    }
}

#[async_trait]
impl Agent for GeminiAgent {
    fn id(&self) -> &str { "gemini" }

    async fn prompt(&self, ctx: Context, _tools: &[ToolSchema], sink: &dyn EventSink) -> Result<StopReason> {
        ensure_crypto_provider();
        let prompt = prompt_text(&ctx);
        let url = format!("{BASE}/models/{}:streamGenerateContent?alt=sse", self.model);
        let resp = reqwest::Client::new()
            .post(url)
            .header("x-goog-api-key", &self.api_key)
            .json(&request_body(&prompt))
            .send().await.map_err(err)?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body: serde_json::Value = resp.json().await.map_err(err)?;
            let msg = body.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str())
                .map(|m| format!("gemini: {m}")).unwrap_or_else(|| format!("gemini: HTTP {status}"));
            return Err(crate::err(msg));
        }

        let mut stream = resp.bytes_stream();
        let mut buf: Vec<u8> = Vec::new();
        while let Some(chunk) = stream.next().await {
            buf.extend_from_slice(&chunk.map_err(err)?);
            for item in drain_sse(&mut buf, parse_line) {
                match item {
                    SseItem::Text(t) => sink.emit(AgentEvent::text_chunk(t)).await?,
                    SseItem::Error(e) => return Err(crate::err(e)),
                }
            }
        }
        sink.emit(AgentEvent::Done(StopReason::EndTurn)).await?;
        Ok(StopReason::EndTurn)
    }
}
```

- [ ] **Step 4: Register the module**

In `crates/manch-llm/src/lib.rs`:

```rust
#[cfg(feature = "gemini")]
pub mod gemini;
#[cfg(feature = "gemini")]
pub use gemini::GeminiAgent;
```

- [ ] **Step 5: Run tests + clippy**

Run: `cargo test -p manch-llm --features gemini && cargo clippy -p manch-llm --features gemini -- -D warnings`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/manch-llm/src/gemini.rs crates/manch-llm/src/lib.rs
git commit -m "feat(llm): Gemini BYOK agent (generateContent SSE)"
```

---

### Task 6: manch-llm OpenAI provider (Codex BYOK)

**Files:**
- Create: `crates/manch-llm/src/openai.rs`
- Modify: `crates/manch-llm/src/lib.rs`

**Interfaces:**
- Consumes: shared core.
- Produces: `pub struct OpenAiAgent`; `OpenAiAgent::new(api_key: String, model: Option<String>)`; `impl Agent`; `pub async fn list_models(api_key: &str) -> Result<Vec<ModelInfo>>`.

- [ ] **Step 1: Write the failing tests**

Create `crates/manch-llm/src/openai.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_body_streams_user_message() {
        let body = request_body("gpt-5", "hi");
        assert_eq!(body["model"], "gpt-5");
        assert_eq!(body["stream"], true);
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "hi");
    }

    #[test]
    fn parse_line_extracts_delta_content() {
        let d = r#"{"choices":[{"delta":{"content":"Hi"}}]}"#;
        assert!(matches!(parse_line(d), Some(crate::SseItem::Text(t)) if t == "Hi"));
    }

    #[test]
    fn parse_line_ignores_done_sentinel() {
        assert!(parse_line("[DONE]").is_none());
    }

    #[test]
    fn parse_models_reads_data_ids() {
        let body = serde_json::json!({ "data": [{ "id": "gpt-5" }, { "id": "o4-mini" }] });
        let models = parse_models(&body);
        assert_eq!(models[0].id, "gpt-5");
        assert_eq!(models[1].id, "o4-mini");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p manch-llm --features openai openai::`
Expected: FAIL.

- [ ] **Step 3: Implement the OpenAI provider**

Prepend to `crates/manch-llm/src/openai.rs`:

```rust
//! BYOK OpenAI Chat Completions client (Codex BYOK path).

use async_trait::async_trait;
use futures_util::StreamExt;
use manch_protocol::acp::StopReason;
use manch_protocol::{Agent, AgentEvent, Context, EventSink, Result, ToolSchema};

use crate::{drain_sse, ensure_crypto_provider, err, prompt_text, ModelInfo, SseItem};

const URL: &str = "https://api.openai.com/v1/chat/completions";
const MODELS_URL: &str = "https://api.openai.com/v1/models";
pub(crate) const FALLBACK_MODEL: &str = "gpt-5"; // confirm current default at impl time

pub struct OpenAiAgent {
    api_key: String,
    model: String,
}

impl OpenAiAgent {
    pub fn new(api_key: String, model: Option<String>) -> Self {
        Self { api_key, model: model.unwrap_or_else(|| FALLBACK_MODEL.to_string()) }
    }
}

pub(crate) fn request_body(model: &str, prompt: &str) -> serde_json::Value {
    serde_json::json!({
        "model": model,
        "stream": true,
        "messages": [{ "role": "user", "content": prompt }],
    })
}

/// Parse one SSE line. `[DONE]` is the stream terminator (not JSON) → None. Pure.
pub(crate) fn parse_line(data: &str) -> Option<SseItem> {
    if data == "[DONE]" { return None; }
    let v: serde_json::Value = serde_json::from_str(data).ok()?;
    if let Some(msg) = v.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str()) {
        return Some(SseItem::Error(format!("openai: {msg}")));
    }
    let content = v.get("choices")?.as_array()?.first()?.get("delta")?.get("content")?.as_str()?;
    (!content.is_empty()).then_some(SseItem::Text(content.to_string()))
}

pub(crate) fn parse_models(body: &serde_json::Value) -> Vec<ModelInfo> {
    body.get("data").and_then(|d| d.as_array()).map(|arr| {
        arr.iter().filter_map(|m| Some(ModelInfo {
            id: m.get("id")?.as_str()?.to_string(),
            display_name: None,
        })).collect()
    }).unwrap_or_default()
}

fn fallback_model() -> ModelInfo { ModelInfo { id: FALLBACK_MODEL.to_string(), display_name: None } }

pub async fn list_models(api_key: &str) -> Result<Vec<ModelInfo>> {
    ensure_crypto_provider();
    let resp = reqwest::Client::new().get(MODELS_URL).bearer_auth(api_key).send().await;
    match resp {
        Ok(r) if r.status().is_success() => {
            let body: serde_json::Value = r.json().await.map_err(err)?;
            let models = parse_models(&body);
            Ok(if models.is_empty() { vec![fallback_model()] } else { models })
        }
        _ => Ok(vec![fallback_model()]),
    }
}

#[async_trait]
impl Agent for OpenAiAgent {
    fn id(&self) -> &str { "openai" }

    async fn prompt(&self, ctx: Context, _tools: &[ToolSchema], sink: &dyn EventSink) -> Result<StopReason> {
        ensure_crypto_provider();
        let prompt = prompt_text(&ctx);
        let resp = reqwest::Client::new()
            .post(URL)
            .bearer_auth(&self.api_key)
            .json(&request_body(&self.model, &prompt))
            .send().await.map_err(err)?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body: serde_json::Value = resp.json().await.map_err(err)?;
            let msg = body.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str())
                .map(|m| format!("openai: {m}")).unwrap_or_else(|| format!("openai: HTTP {status}"));
            return Err(crate::err(msg));
        }

        let mut stream = resp.bytes_stream();
        let mut buf: Vec<u8> = Vec::new();
        while let Some(chunk) = stream.next().await {
            buf.extend_from_slice(&chunk.map_err(err)?);
            for item in drain_sse(&mut buf, parse_line) {
                match item {
                    SseItem::Text(t) => sink.emit(AgentEvent::text_chunk(t)).await?,
                    SseItem::Error(e) => return Err(crate::err(e)),
                }
            }
        }
        sink.emit(AgentEvent::Done(StopReason::EndTurn)).await?;
        Ok(StopReason::EndTurn)
    }
}
```

- [ ] **Step 4: Register the module + the public dispatch**

In `crates/manch-llm/src/lib.rs`, add the module lines and a top-level `list_models` dispatch:

```rust
#[cfg(feature = "openai")]
pub mod openai;
#[cfg(feature = "openai")]
pub use openai::OpenAiAgent;

/// Fetch selectable models for a BYOK provider id. Unknown / disabled providers
/// yield `NotFound`. Each provider degrades to its fallback model on fetch failure.
pub async fn list_models(provider: &str, api_key: &str) -> manch_protocol::Result<Vec<ModelInfo>> {
    match provider {
        #[cfg(feature = "anthropic")]
        "anthropic" => anthropic::list_models(api_key).await,
        #[cfg(feature = "gemini")]
        "gemini" => gemini::list_models(api_key).await,
        #[cfg(feature = "openai")]
        "openai" => openai::list_models(api_key).await,
        _ => Err(manch_protocol::Error::NotFound(provider.to_string())),
    }
}
```

- [ ] **Step 5: Write the failing dispatch test**

Add to the `tests` module in `crates/manch-llm/src/lib.rs`:

```rust
    #[tokio::test]
    async fn list_models_rejects_unknown_provider() {
        let e = super::list_models("nope", "k").await.unwrap_err();
        assert!(matches!(e, manch_protocol::Error::NotFound(_)));
    }
```

Add `tokio` as a dev-dependency in `crates/manch-llm/Cargo.toml`:

```toml
[dev-dependencies]
tokio = { workspace = true, features = ["macros", "rt"] }
```

- [ ] **Step 6: Run all tests with all features + clippy**

Run: `cargo test -p manch-llm --all-features && cargo clippy -p manch-llm --all-features -- -D warnings`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/manch-llm/src/openai.rs crates/manch-llm/src/lib.rs crates/manch-llm/Cargo.toml
git commit -m "feat(llm): OpenAI BYOK agent (Codex path) + list_models dispatch"
```

---

### Task 7: manch-acp generic CLI agent + launch builders

**Files:**
- Modify: `crates/manch-acp/src/lib.rs`

Adapt the ACP machinery from `apps/desktop/src-tauri/src/agent.rs` — `push_chunk` (129-146), `tool_status` (178-187), and the `ClaudeCodeAgent::stream` body (267-395). Generalize the launch args (93-106) into a `LaunchSpec`.

**Interfaces:**
- Consumes: `AgentEvent::text_chunk` (Task 2); `manch_protocol::{Agent, Context, EventSink, ToolSchema, acp}`.
- Produces:
  - `pub struct LaunchSpec { pub args: Vec<String>, pub key_env: Option<&'static str> }`
  - `pub struct AcpCliAgent` with `AcpCliAgent::new(id: &'static str, spec: LaunchSpec) -> Self` and `impl Agent`
  - `pub fn claude_code(api_key: Option<String>) -> AcpCliAgent`
  - `pub fn gemini_cli(api_key: Option<String>) -> AcpCliAgent`
  - `pub fn codex(api_key: Option<String>) -> AcpCliAgent`
  - `pub(crate) fn push_chunk(buf: &mut String, chunk: &str) -> Option<String>`
  - `pub(crate) fn tool_status(status: acp::ToolCallStatus) -> &'static str`

- [ ] **Step 1: Write the failing tests**

Add to `crates/manch-acp/src/lib.rs` (a `#[cfg(test)] mod tests`), porting the arg/`push_chunk`/`tool_status` tests from `agent.rs:445-511,544-553`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_code_without_key_is_just_npx() {
        let s = claude_code(None).spec;
        assert_eq!(s.args[0], "npx");
        assert!(s.args.iter().any(|a| a.contains("claude-agent-acp")));
    }

    #[test]
    fn codex_launches_zed_adapter() {
        let s = codex(None).spec;
        assert_eq!(s.args[0], "npx");
        assert!(s.args.iter().any(|a| a.contains("@zed-industries/codex-acp")));
        assert_eq!(s.key_env, Some("OPENAI_API_KEY"));
    }

    #[test]
    fn gemini_cli_passes_experimental_acp() {
        let s = gemini_cli(None).spec;
        assert!(s.args.iter().any(|a| a == "--experimental-acp"));
        assert_eq!(s.key_env, Some("GEMINI_API_KEY"));
    }

    #[test]
    fn launch_argv_prepends_env_when_key_present() {
        let agent = claude_code(Some("sk-test".into()));
        let argv = agent.argv();
        assert_eq!(argv[0], "ANTHROPIC_API_KEY=sk-test");
    }

    #[test]
    fn push_chunk_returns_only_cumulative_delta() {
        let mut b = String::new();
        push_chunk(&mut b, "New");
        assert_eq!(push_chunk(&mut b, "New Delhi."), Some(" Delhi.".to_string()));
    }

    #[test]
    fn tool_status_maps_acp_vocabulary() {
        use agent_client_protocol::schema::v1::ToolCallStatus;
        assert_eq!(tool_status(ToolCallStatus::Completed), "done");
        assert_eq!(tool_status(ToolCallStatus::Failed), "error");
        assert_eq!(tool_status(ToolCallStatus::InProgress), "running");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p manch-acp`
Expected: FAIL — types not defined.

- [ ] **Step 3: Implement `LaunchSpec`, builders, `push_chunk`, `tool_status`**

Prepend to `crates/manch-acp/src/lib.rs`:

```rust
//! Framework-agnostic ACP host — one generic subprocess agent parameterized by a launch spec.

use agent_client_protocol::schema::v1::ToolCallStatus;

const CLAUDE_CODE_PKG: &str = "@agentclientprotocol/claude-agent-acp@latest";
const CODEX_PKG: &str = "@zed-industries/codex-acp";
const GEMINI_CLI_PKG: &str = "@google/gemini-cli";

/// A per-CLI subprocess launch recipe. `args` is the launch command; `key_env`,
/// when set and given a key, becomes a leading `NAME=value` subprocess env var.
pub struct LaunchSpec {
    pub args: Vec<String>,
    pub key_env: Option<&'static str>,
}

/// Wraps an external ACP agent (subprocess) as a `manch_protocol::Agent`.
pub struct AcpCliAgent {
    id: &'static str,
    api_key: Option<String>,
    pub spec: LaunchSpec,
}

impl AcpCliAgent {
    pub fn new(id: &'static str, api_key: Option<String>, spec: LaunchSpec) -> Self {
        Self { id, api_key, spec }
    }

    /// Full argv passed to the ACP host: a leading `NAME=value` env token (only
    /// when this agent takes a key override AND one was supplied), then the
    /// launch command.
    pub(crate) fn argv(&self) -> Vec<String> {
        let mut argv = Vec::new();
        if let (Some(env), Some(key)) = (self.spec.key_env, self.api_key.as_deref()) {
            argv.push(format!("{env}={key}"));
        }
        argv.extend(self.spec.args.iter().cloned());
        argv
    }
}

pub fn claude_code(api_key: Option<String>) -> AcpCliAgent {
    AcpCliAgent::new("claude-code", api_key, LaunchSpec {
        args: vec!["npx".into(), "-y".into(), CLAUDE_CODE_PKG.into()],
        key_env: Some("ANTHROPIC_API_KEY"),
    })
}

pub fn codex(api_key: Option<String>) -> AcpCliAgent {
    AcpCliAgent::new("codex", api_key, LaunchSpec {
        args: vec!["npx".into(), "-y".into(), CODEX_PKG.into()],
        key_env: Some("OPENAI_API_KEY"),
    })
}

pub fn gemini_cli(api_key: Option<String>) -> AcpCliAgent {
    AcpCliAgent::new("gemini-cli", api_key, LaunchSpec {
        args: vec!["npx".into(), "-y".into(), GEMINI_CLI_PKG.into(), "--experimental-acp".into()],
        key_env: Some("GEMINI_API_KEY"),
    })
}

/// Merge one streamed chunk into `buf`, returning the newly-added text (`None`
/// if nothing new). Tolerates pure deltas, cumulative snapshots, and trailing
/// full-message repeats so we never double-emit. Pure.
pub(crate) fn push_chunk(buf: &mut String, chunk: &str) -> Option<String> {
    if chunk.is_empty() {
        None
    } else if buf.is_empty() {
        buf.push_str(chunk);
        Some(chunk.to_string())
    } else if chunk.starts_with(buf.as_str()) {
        let delta = chunk[buf.len()..].to_string();
        *buf = chunk.to_string();
        (!delta.is_empty()).then_some(delta)
    } else if buf.ends_with(chunk) {
        None
    } else {
        buf.push_str(chunk);
        Some(chunk.to_string())
    }
}

/// Map an ACP tool-call status onto the `running|done|error` vocabulary.
pub(crate) fn tool_status(status: ToolCallStatus) -> &'static str {
    match status {
        ToolCallStatus::Completed => "done",
        ToolCallStatus::Failed => "error",
        _ => "running",
    }
}
```

- [ ] **Step 4: Run the non-async tests**

Run: `cargo test -p manch-acp`
Expected: PASS (the builder / push_chunk / tool_status tests).

- [ ] **Step 5: Implement `impl Agent for AcpCliAgent`**

Adapt `agent.rs:267-395`. Key changes vs. today:
- Signature is `manch_protocol::Agent::prompt(&self, ctx, _tools, sink)` returning `Result<StopReason>`.
- Build the prompt blocks from `ctx.blocks` (pass them straight into `PromptRequest`).
- The notification handler emits `AgentEvent` (not `StreamEvent`): text via `push_chunk` → `AgentEvent::text_chunk(delta)`; `ToolCall` forwarded as `AgentEvent::Update(SessionUpdate::ToolCall(tc))` after recording its title; `ToolCallUpdate` with its `fields.title` filled from the recorded name, forwarded as `AgentEvent::Update(SessionUpdate::ToolCallUpdate(u))`.
- Track an `emitted` flag; after the run, `sink.emit(AgentEvent::Done(stop)).await?` if anything was emitted, else return `Err(manch_protocol::Error::Other(format!("{id} returned no text (stop reason: {stop:?})")))`.
- The `EventSink` is `&dyn manch_protocol::EventSink` (async); the `'static` notification handler needs an owned emitter. Since `emit` is async and the handler closure is sync, buffer emissions: push `AgentEvent`s into an `Arc<Mutex<Vec<AgentEvent>>>` from the handler, and after `connect_with` returns, drain the vec and `sink.emit(..).await?` each in order. (This preserves ordering and keeps the async boundary outside the sync ACP callback.)

Append:

```rust
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use manch_protocol::acp::{SessionUpdate, StopReason};
use manch_protocol::{Agent, AgentEvent, Context, EventSink, Result, ToolSchema};

#[async_trait]
impl Agent for AcpCliAgent {
    fn id(&self) -> &str { self.id }

    async fn prompt(&self, ctx: Context, _tools: &[ToolSchema], sink: &dyn EventSink) -> Result<StopReason> {
        use std::collections::HashMap;
        use agent_client_protocol::schema::ProtocolVersion;
        use agent_client_protocol::schema::v1::{
            ContentChunk, InitializeRequest, NewSessionRequest, PromptRequest,
            RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
            SelectedPermissionOutcome, SessionNotification, ContentBlock,
        };
        use agent_client_protocol::{self as acp, AcpAgent, Agent as _, Client, ConnectionTo};

        let agent = AcpAgent::from_args(self.argv()).map_err(err)?;
        let blocks = ctx.blocks.clone();
        let id = self.id;

        // Buffer AgentEvents from the 'static sync notification handler; drain
        // and async-emit them after the connection completes (emit is async).
        let out: Arc<Mutex<Vec<AgentEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let text_buf = Arc::new(Mutex::new(String::new()));
        let names: Arc<Mutex<HashMap<String, String>>> = Arc::new(Mutex::new(HashMap::new()));
        let (hout, htext, hnames) = (out.clone(), text_buf.clone(), names.clone());

        let stop = Client
            .builder()
            .on_receive_notification(
                async move |n: SessionNotification, _cx| {
                    match n.update {
                        SessionUpdate::AgentMessageChunk(ContentChunk { content: ContentBlock::Text(t), .. }) => {
                            if let Some(delta) = push_chunk(&mut htext.lock().unwrap(), &t.text) {
                                hout.lock().unwrap().push(AgentEvent::text_chunk(delta));
                            }
                        }
                        SessionUpdate::ToolCall(tc) => {
                            hnames.lock().unwrap().insert(tc.tool_call_id.0.to_string(), tc.title.clone());
                            hout.lock().unwrap().push(AgentEvent::Update(SessionUpdate::ToolCall(tc)));
                        }
                        SessionUpdate::ToolCallUpdate(mut u) => {
                            if u.fields.title.is_none() {
                                u.fields.title = hnames.lock().unwrap().get(&u.tool_call_id.0.to_string()).cloned();
                            }
                            hout.lock().unwrap().push(AgentEvent::Update(SessionUpdate::ToolCallUpdate(u)));
                        }
                        _ => {}
                    }
                    Ok(())
                },
                acp::on_receive_notification!(),
            )
            .on_receive_request(
                async move |request: RequestPermissionRequest, responder, _connection| match request
                    .options.first().map(|opt| opt.option_id.clone())
                {
                    Some(id) => responder.respond(RequestPermissionResponse::new(
                        RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(id)))),
                    None => responder.respond(RequestPermissionResponse::new(RequestPermissionOutcome::Cancelled)),
                },
                acp::on_receive_request!(),
            )
            .connect_with(agent, |connection: ConnectionTo<acp::Agent>| async move {
                connection.send_request(InitializeRequest::new(ProtocolVersion::V1)).block_task().await?;
                let cwd = std::env::temp_dir().join("manch-acp-workspace");
                std::fs::create_dir_all(&cwd).ok();
                let session = connection.send_request(NewSessionRequest::new(cwd)).block_task().await?;
                let response = connection
                    .send_request(PromptRequest::new(session.session_id, blocks))
                    .block_task().await?;
                Ok(response.stop_reason)
            })
            .await
            .map_err(err)?;

        let events = std::mem::take(&mut *out.lock().unwrap());
        let emitted = !events.is_empty();
        for ev in events {
            sink.emit(ev).await?;
        }
        if emitted {
            sink.emit(AgentEvent::Done(stop)).await?;
            Ok(stop)
        } else {
            Err(manch_protocol::Error::Other(format!("{id} returned no text (stop reason: {stop:?})")))
        }
    }
}

fn err(e: impl ToString) -> manch_protocol::Error {
    manch_protocol::Error::Other(e.to_string())
}
```

> NOTE: This block is adapted verbatim from `agent.rs:267-395`; keep imports/types matching `agent-client-protocol` v1 exactly (the desktop currently compiles against it). The only behavioral change is buffering events then async-emitting, and filling `ToolCallUpdate.fields.title` from the recorded names so the desktop mapping always has a name.

- [ ] **Step 6: Build + clippy (async path is compile-checked, not unit-tested — it needs a live subprocess)**

Run: `cargo build -p manch-acp && cargo clippy -p manch-acp -- -D warnings && cargo test -p manch-acp`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/manch-acp/src/lib.rs
git commit -m "feat(acp): generic AcpCliAgent + claude-code/gemini-cli/codex launch specs"
```

---

### Task 8: Desktop — persist selected model per provider

**Files:**
- Modify: `apps/desktop/src-tauri/src/db.rs`

**Interfaces:**
- Consumes: existing `Db` (mutex'd `Connection`), `provider_keys` table pattern.
- Produces: `Db::set_model(&self, provider: &str, model: &str) -> rusqlite::Result<()>`; `Db::get_model(&self, provider: &str) -> rusqlite::Result<Option<String>>`.

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` in `db.rs`:

```rust
#[test]
fn model_round_trips_per_provider() {
    let db = Db::open_in_memory().unwrap();
    assert_eq!(db.get_model("anthropic").unwrap(), None);
    db.set_model("anthropic", "claude-opus-4-8").unwrap();
    db.set_model("gemini", "gemini-3-flash").unwrap();
    assert_eq!(db.get_model("anthropic").unwrap().as_deref(), Some("claude-opus-4-8"));
    assert_eq!(db.get_model("gemini").unwrap().as_deref(), Some("gemini-3-flash"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p manch-desktop model_round_trips`
Expected: FAIL — `set_model` not found.

- [ ] **Step 3: Add the table + methods**

In `Db::init`, add a table (additive; `CREATE TABLE IF NOT EXISTS` is the existing migration idiom):

```rust
        conn.execute(
            "CREATE TABLE IF NOT EXISTS provider_models (
                 provider TEXT PRIMARY KEY,
                 model    TEXT NOT NULL
             )",
            [],
        )?;
```

Add the methods (next to `save_key`/`get_key`):

```rust
    pub fn set_model(&self, provider: &str, model: &str) -> rusqlite::Result<()> {
        let conn = self.0.lock().unwrap();
        conn.execute(
            "INSERT INTO provider_models (provider, model) VALUES (?1, ?2)
             ON CONFLICT(provider) DO UPDATE SET model = excluded.model",
            rusqlite::params![provider, model],
        )?;
        Ok(())
    }

    pub fn get_model(&self, provider: &str) -> rusqlite::Result<Option<String>> {
        let conn = self.0.lock().unwrap();
        conn.query_row(
            "SELECT model FROM provider_models WHERE provider = ?1",
            rusqlite::params![provider],
            |r| r.get(0),
        )
        .optional()
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p manch-desktop model_round_trips`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src-tauri/src/db.rs
git commit -m "feat(desktop): persist selected model per provider"
```

---

### Task 9: Desktop — rewire onto the crates, delete inline provider code

**Files:**
- Modify: `apps/desktop/src-tauri/Cargo.toml`
- Modify: `apps/desktop/src-tauri/src/agent.rs` (reduce to desktop mapping + factory)
- Modify: `apps/desktop/src-tauri/src/commands.rs`

**Interfaces:**
- Consumes: `manch_llm::{AnthropicAgent, GeminiAgent, OpenAiAgent, list_models}`, `manch_acp::{claude_code, gemini_cli, codex}`, `manch_protocol::{Agent, AgentEvent, Context, EventSink, acp}`.
- Produces: `resolve_agent(provider, &Db) -> Result<Box<dyn manch_protocol::Agent>, String>`; `ChannelSink` (async `EventSink`); Tauri commands `send_prompt_stream`, `list_models`, updated `save_api_key`/`list_configured_providers`.

- [ ] **Step 1: Swap dependencies**

In `apps/desktop/src-tauri/Cargo.toml`, remove `futures-util`, `reqwest`, `rustls`, `agent-client-protocol` from `[dependencies]` and add:

```toml
manch-protocol = { workspace = true }
manch-llm = { path = "../../../crates/manch-llm" }
manch-acp = { path = "../../../crates/manch-acp" }
```

Keep `serde`, `serde_json`, `async-trait`, `rusqlite`, `tokio`, `tauri`, `manch-dto`.

- [ ] **Step 2: Replace `agent.rs` with the desktop-only mapping + factory**

Delete everything in `agent.rs` except a new, small module. Replace the file contents with:

```rust
//! Desktop glue: map `manch_protocol::AgentEvent` → `manch_dto::StreamEvent`,
//! and resolve a provider id to a concrete agent. All provider logic now lives
//! in `manch-llm` (BYOK) and `manch-acp` (CLI).

use std::sync::Mutex;

use async_trait::async_trait;
use manch_dto::StreamEvent;
use manch_protocol::acp::{ContentBlock, SessionUpdate, ToolCallStatus};
use manch_protocol::{AgentEvent, EventSink};
use tauri::ipc::Channel;

use crate::db::Db;

/// Provider ids the desktop understands (BYOK + CLI).
pub const BYOK: [&str; 3] = ["anthropic", "gemini", "openai"];
pub const CLI: [&str; 3] = ["claude-code", "gemini-cli", "codex"];

pub fn is_known_provider(id: &str) -> bool {
    BYOK.contains(&id) || CLI.contains(&id)
}

/// Providers offerable in the UI: every saved one, plus the always-available
/// BYOC CLIs (they bring their own auth).
pub fn offerable_providers(mut saved: Vec<String>) -> Vec<String> {
    for cli in CLI {
        if !saved.iter().any(|p| p == cli) {
            saved.push(cli.to_string());
        }
    }
    saved.sort();
    saved.dedup();
    saved
}

fn tool_status(status: ToolCallStatus) -> &'static str {
    match status {
        ToolCallStatus::Completed => "done",
        ToolCallStatus::Failed => "error",
        _ => "running",
    }
}

/// `EventSink` that maps ACP events to `StreamEvent` and forwards them over a
/// Tauri IPC channel. `emitted` gates nothing here — the agent decides Done/Err.
pub struct ChannelSink(pub Channel<StreamEvent>);

#[async_trait]
impl EventSink for ChannelSink {
    async fn emit(&self, event: AgentEvent) -> manch_protocol::Result<()> {
        match event {
            AgentEvent::Update(SessionUpdate::AgentMessageChunk(chunk)) => {
                if let ContentBlock::Text(t) = chunk.content {
                    let _ = self.0.send(StreamEvent::Token { text: t.text });
                }
            }
            AgentEvent::Update(SessionUpdate::ToolCall(tc)) => {
                let _ = self.0.send(StreamEvent::Tool {
                    id: tc.tool_call_id.0.to_string(),
                    name: tc.title,
                    status: tool_status(tc.status).into(),
                    detail: None,
                });
            }
            AgentEvent::Update(SessionUpdate::ToolCallUpdate(u)) => {
                let _ = self.0.send(StreamEvent::Tool {
                    id: u.tool_call_id.0.to_string(),
                    name: u.fields.title.unwrap_or_default(),
                    status: u.fields.status.map(tool_status).unwrap_or("running").into(),
                    detail: None,
                });
            }
            AgentEvent::Done(_) => {
                let _ = self.0.send(StreamEvent::Done);
            }
            _ => {}
        }
        Ok(())
    }
}

/// Resolve a provider id to a concrete agent, pulling keys/model from the DB.
pub fn resolve_agent(provider: &str, db: &Db) -> Result<Box<dyn manch_protocol::Agent>, String> {
    let byok = |p: &str| -> Result<(String, Option<String>), String> {
        let key = db.get_key(p).map_err(|e| e.to_string())?
            .ok_or_else(|| format!("no API key saved for {p}"))?;
        let model = db.get_model(p).map_err(|e| e.to_string())?;
        Ok((key, model))
    };
    match provider {
        "anthropic" => { let (k, m) = byok("anthropic")?; Ok(Box::new(manch_llm::AnthropicAgent::new(k, m))) }
        "gemini"    => { let (k, m) = byok("gemini")?;    Ok(Box::new(manch_llm::GeminiAgent::new(k, m))) }
        "openai"    => { let (k, m) = byok("openai")?;    Ok(Box::new(manch_llm::OpenAiAgent::new(k, m))) }
        "claude-code" => Ok(Box::new(manch_acp::claude_code(db.get_key("claude-code").map_err(|e| e.to_string())?))),
        "gemini-cli"  => Ok(Box::new(manch_acp::gemini_cli(db.get_key("gemini-cli").map_err(|e| e.to_string())?))),
        "codex"       => Ok(Box::new(manch_acp::codex(db.get_key("codex").map_err(|e| e.to_string())?))),
        _ => Err(format!("unknown provider: {provider}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_agents_always_offered() {
        let out = offerable_providers(vec!["anthropic".into()]);
        assert!(out.contains(&"anthropic".to_string()));
        assert!(out.contains(&"claude-code".to_string()));
        assert!(out.contains(&"codex".to_string()));
        assert!(out.contains(&"gemini-cli".to_string()));
    }

    #[test]
    fn known_providers() {
        assert!(is_known_provider("gemini"));
        assert!(is_known_provider("codex"));
        assert!(!is_known_provider("nope"));
    }
}
```

- [ ] **Step 3: Update `commands.rs`**

Rewrite the imports and the agent-facing commands. Replace the current `use crate::agent::{...}` line with:

```rust
use crate::agent::{ChannelSink, is_known_provider, offerable_providers, resolve_agent};
use manch_protocol::{Context, EventSink};
```

Replace `save_api_key`'s validation, `list_configured_providers`, and `send_prompt_stream`, and add `list_models`:

```rust
#[tauri::command]
pub fn save_api_key(state: State<Db>, provider: String, api_key: String) -> Result<(), String> {
    if !is_known_provider(&provider) {
        return Err(format!("unknown provider: {provider}"));
    }
    state.save_key(&provider, &api_key).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_configured_providers(state: State<Db>) -> Result<Vec<String>, String> {
    let saved = state.list_providers().map_err(|e| e.to_string())?;
    Ok(offerable_providers(saved))
}

/// Fetch selectable models for a BYOK provider (needs a saved key).
#[tauri::command]
pub async fn list_models(
    state: State<'_, Db>,
    provider: String,
) -> Result<Vec<manch_llm::ModelInfo>, String> {
    let key = state.get_key(&provider).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no API key saved for {provider}"))?;
    manch_llm::list_models(&provider, &key).await.map_err(|e| e.to_string())
}

/// Persist the user's chosen model for a provider.
#[tauri::command]
pub fn set_model(state: State<Db>, provider: String, model: String) -> Result<(), String> {
    state.set_model(&provider, &model).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn send_prompt_stream(
    state: State<'_, Db>,
    provider: String,
    text: String,
    channel: Channel<StreamEvent>,
) -> Result<(), String> {
    let agent = resolve_agent(&provider, &state)?;
    let ctx = Context {
        session_id: "desktop".to_string(),
        blocks: vec![manch_protocol::acp::ContentBlock::Text(
            manch_protocol::acp::TextContent::new(text),
        )],
    };
    let sink = ChannelSink(channel);
    match agent.prompt(ctx, &[], &sink).await {
        Ok(_) => Ok(()),
        Err(e) => {
            // Surface the failure to the UI, then swallow (stream already closed on the happy path).
            let _ = EventSink::emit(&sink, manch_protocol::AgentEvent::Update(
                manch_protocol::acp::SessionUpdate::AgentMessageChunk(
                    manch_protocol::acp::ContentChunk::new(
                        manch_protocol::acp::ContentBlock::Text(
                            manch_protocol::acp::TextContent::new(String::new())))))).await;
            channel_error(&sink, e.to_string());
            Ok(())
        }
    }
}
```

> The error path must send `StreamEvent::Error`. `ChannelSink` only maps `AgentEvent`, which has no error variant — so add a tiny direct sender rather than routing through `emit`. Replace the error arm above with a direct call:

```rust
        Err(e) => {
            sink.send_error(e.to_string());
            Ok(())
        }
```

and add to `ChannelSink` in `agent.rs`:

```rust
impl ChannelSink {
    pub fn send_error(&self, message: String) {
        let _ = self.0.send(StreamEvent::Error { message });
    }
}
```

(Delete the placeholder `channel_error`/empty-chunk block — it was scaffolding to make the requirement explicit.)

- [ ] **Step 4: Register the new commands**

In `apps/desktop/src-tauri/src/lib.rs`, add `commands::list_models` and `commands::set_model` to the `tauri::generate_handler![...]` list (next to the existing `send_prompt_stream`).

- [ ] **Step 5: Build, test, clippy the desktop crate**

Run: `cargo test -p manch-desktop && cargo clippy -p manch-desktop -- -D warnings`
Expected: PASS. If `manch_protocol` isn't a direct dep yet, it is now referenced — ensure the `manch-protocol` line from Step 1 is present.

- [ ] **Step 6: Commit**

```bash
git add apps/desktop/src-tauri
git commit -m "refactor(desktop): rewire onto manch-llm + manch-acp; delete inline ChatAgent"
```

---

### Task 10: Desktop frontend — model dropdown

**Files:**
- Locate first: `rg -l "send_prompt_stream|list_configured_providers" apps/desktop/src` to find the provider-picker component.
- Modify: the settings/provider component (e.g. `apps/desktop/src/…/Settings.tsx`) and the chat send path.

**Interfaces:**
- Consumes: Tauri commands `list_models(provider) -> ModelInfo[]`, `set_model(provider, model)`, `save_api_key`.
- `ModelInfo` shape (from `manch_llm::ModelInfo`): `{ id: string; display_name: string | null }`.

- [ ] **Step 1: Locate the components**

Run: `rg -n "list_configured_providers|save_api_key|send_prompt_stream" apps/desktop/src`
Note the file(s) that own provider selection and API-key entry.

- [ ] **Step 2: Add a model dropdown for BYOK providers**

In the provider settings component, when the selected provider is one of `anthropic`/`gemini`/`openai` and a key is saved, call `invoke('list_models', { provider })`, render a `<select>` of the returned models (label = `display_name ?? id`, value = `id`), and on change call `invoke('set_model', { provider, model })`. Show a graceful fallback (single-option select) when the list has one entry. Do not render the dropdown for CLI providers (`claude-code`/`gemini-cli`/`codex`).

Example (adapt to the existing component's framework/styling):

```tsx
const BYOK = ["anthropic", "gemini", "openai"];
const [models, setModels] = useState<{ id: string; display_name: string | null }[]>([]);

useEffect(() => {
  if (!BYOK.includes(provider) || !hasKey) return;
  invoke<{ id: string; display_name: string | null }[]>("list_models", { provider })
    .then(setModels)
    .catch(() => setModels([]));
}, [provider, hasKey]);

{BYOK.includes(provider) && models.length > 0 && (
  <select onChange={(e) => invoke("set_model", { provider, model: e.target.value })}>
    {models.map((m) => (
      <option key={m.id} value={m.id}>{m.display_name ?? m.id}</option>
    ))}
  </select>
)}
```

- [ ] **Step 3: Typecheck the frontend**

Run: `just lint`
Expected: PASS (TS typecheck clean).

- [ ] **Step 4: JS/UI tests (if the touched component has a test)**

Run: `just test-js`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src
git commit -m "feat(desktop): model dropdown fed by provider list-models"
```

---

### Task 11: Full CI gate + cleanup

**Files:** none (verification only), plus any fixups the gate surfaces.

- [ ] **Step 1: Regenerate bindings (DTOs unchanged, but keep CI parity)**

Run: `just gen`
Expected: PASS (no unexpected diff; `ModelInfo` is returned as a plain command result, not a ts-rs DTO, so no `bindings.ts` change is required — the frontend types it inline).

- [ ] **Step 2: Run the full CI pipeline**

Run: `just ci`
Expected: PASS — `gen → fmt-check → clippy → test-rust → lint → test-js → build-js` all green.

- [ ] **Step 3: Fix anything the gate surfaces**

If clippy/fmt/tests fail, fix inline and re-run `just ci` until green. Commit fixes with `fix:`/`chore:` as appropriate.

- [ ] **Step 4: Final verification of the invariant**

Run: `rg -n "std::process|std::fs::(write|create|remove)|Command" crates/manch-llm/src`
Expected: no matches — confirms `manch-llm` has no execution/fs-write surface.

- [ ] **Step 5: Final commit (if any fixups)**

```bash
git add -A
git commit -m "chore: ci green for manch-llm + manch-acp extraction"
```

---

## Self-Review

**Spec coverage:**
- manch-llm hybrid crate + features → Tasks 1, 3–6. ✅
- manch-acp generic CLI agent + 3 launch specs → Task 7. ✅
- Contract in manch-protocol (already existed) + `text_chunk`/`TextContent` additions → Task 2. ✅
- Anthropic/Gemini/OpenAI wire dialects + pure fns → Tasks 4–6. ✅
- Model fetch + user-select + persistence → Tasks 6 (dispatch), 8 (db), 9 (commands), 10 (UI). ✅
- Desktop rewiring, delete inline ChatAgent, AgentEvent→StreamEvent mapping, one-block Context → Task 9. ✅
- `publish = false` → Task 1 Cargo.toml. ✅
- manch-llm no-execution invariant → Task 11 Step 4. ✅
- CLI model selection stays agent-owned (no ModelCatalog on CLI) → Task 7 (no list_models). ✅
- Testing (pure-fn per dialect, launch-spec, mapping) → Tasks 4–7, 9. ✅

**Placeholder scan:** The two `NOTE:` blocks (Task 2 constructor path, Task 4/7 `StopReason` variant) are deliberate "verify against the ACP crate" pointers, each with the concrete expected form and a real reference line — not open TODOs.

**Type consistency:** `AnthropicAgent::new(String, Option<String>)`, `GeminiAgent::new(..)`, `OpenAiAgent::new(..)` consistent across Tasks 4–6 and 9; `manch_llm::list_models(provider, key)` (Task 6) matches the `commands.rs` call (Task 9); `ModelInfo { id, display_name }` consistent (Tasks 3, 9, 10); `AcpCliAgent`/`claude_code`/`gemini_cli`/`codex` consistent (Tasks 7, 9); `tool_status`/`push_chunk` moved to manch-acp (Task 7), `tool_status` for the desktop mapping re-defined locally in `agent.rs` (Task 9) — intentional, both small and private.
