# Dual-Path Hello World — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Answer "What is the capital of India?" through **both** a hand-rolled BYOK Anthropic client (no `rig`) and Claude Code over ACP, behind one `ChatAgent` interface.

**Architecture:** UI-first, inline in `apps/desktop/src-tauri`. A local `ChatAgent` trait with two impls (`AnthropicAgent`, `ClaudeCodeAgent`) — the lightweight stand-in for `manch_protocol::Agent`. Non-streaming, no tools, no server.

**Tech Stack:** Rust + Tauri v2, `reqwest` (rustls) for the Anthropic Messages API, `agent-client-protocol` `=0.14.0` for the Claude Code ACP subprocess, `rusqlite` (existing), `async-trait`.

**Spec:** `docs/superpowers/specs/2026-06-17-dual-path-hello-world-design.md`

## Global Constraints

- Edition 2024, `rust-version 1.85` (async closures available).
- Anthropic model id: `claude-opus-4-8` — authoritative per the `claude-api` skill, do NOT change.
- **No `rig`.** `rig-core` must be removed from the desktop crate and `Cargo.lock`.
- `reqwest` must use `rustls-tls` (no system OpenSSL): `default-features = false, features = ["json", "rustls-tls"]`.
- ACP pin matches Zed: `agent-client-protocol = { version = "=0.14.0", features = ["unstable"] }`.
- Providers this slice: `anthropic`, `claude-code`. Gemini is removed.

---

## Task 1: Dependency swap + `agent.rs` core (rig out, Anthropic in)

**Files:**
- Modify: `apps/desktop/src-tauri/Cargo.toml`
- Modify (rewrite): `apps/desktop/src-tauri/src/agent.rs`

**Interfaces:**
- Produces: `Provider` (`Anthropic`, `ClaudeCode`) + `Provider::from_id`; `trait ChatAgent { async fn ask(&self, &str) -> Result<String, String> }`; `AnthropicAgent::new(String)`; `ClaudeCodeAgent::new(String)` (stub in this task); pure fns `claude_code_args(&str) -> Vec<String>`, `claude_code_key(Option<String>, Option<String>) -> Option<String>`, `offerable_providers(Vec<String>) -> Vec<String>`.

- [ ] **Step 1: Swap dependencies**

In `apps/desktop/src-tauri/Cargo.toml`, delete the `rig-core` line and add the new deps. The `[dependencies]` section should become:

```toml
[dependencies]
tauri = { version = "2", features = [] }
serde = { workspace = true }
serde_json = { workspace = true }
async-trait = { workspace = true }
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
agent-client-protocol = { version = "=0.14.0", features = ["unstable"] }
rusqlite = { version = "0.40.1", features = ["bundled"] }
tokio = { workspace = true, features = ["rt-multi-thread", "macros"] }
```

- [ ] **Step 2: Rewrite `agent.rs` with the test module first (tests fail to compile)**

Replace the entire contents of `apps/desktop/src-tauri/src/agent.rs` with the tests + signatures only (impls come in Step 4):

```rust
//! BYOK + BYOC agents behind one `ChatAgent` interface.
//!
//! Inline for this slice; collapses into `manch_protocol::Agent` when `manch-core`
//! is extracted (`ask` becomes a streaming `prompt` through an `EventSink`).
//! No `rig`: the Anthropic path is a hand-rolled Messages-API call over `reqwest`.

use async_trait::async_trait;

/// Anthropic model id (authoritative per the claude-api skill — do NOT change).
const ANTHROPIC_MODEL: &str = "claude-opus-4-8";
const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const MAX_TOKENS: u32 = 1024;
const CLAUDE_CODE_PKG: &str = "@agentclientprotocol/claude-agent-acp@latest";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    Anthropic,
    ClaudeCode,
}

impl Provider {
    pub fn from_id(id: &str) -> Option<Provider> {
        match id {
            "anthropic" => Some(Provider::Anthropic),
            "claude-code" => Some(Provider::ClaudeCode),
            _ => None,
        }
    }
}

/// One interface, two implementations (plan B). Stand-in for `manch_protocol::Agent`.
#[async_trait]
pub trait ChatAgent: Send + Sync {
    async fn ask(&self, prompt: &str) -> Result<String, String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_providers_parse() {
        assert_eq!(Provider::from_id("anthropic"), Some(Provider::Anthropic));
        assert_eq!(Provider::from_id("claude-code"), Some(Provider::ClaudeCode));
    }

    #[test]
    fn unknown_provider_is_none() {
        assert_eq!(Provider::from_id("gemini"), None);
        assert_eq!(Provider::from_id(""), None);
    }

    #[test]
    fn request_body_has_model_and_user_message() {
        let body = anthropic_request_body(ANTHROPIC_MODEL, "hi");
        assert_eq!(body["model"], "claude-opus-4-8");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "hi");
        assert!(body["max_tokens"].is_number());
    }

    #[test]
    fn parses_concatenated_text_blocks() {
        let body = serde_json::json!({
            "content": [
                { "type": "text", "text": "New " },
                { "type": "text", "text": "Delhi" }
            ]
        });
        assert_eq!(parse_anthropic_text(&body).unwrap(), "New Delhi");
    }

    #[test]
    fn surfaces_error_message() {
        let body = serde_json::json!({ "error": { "message": "invalid x-api-key" } });
        assert_eq!(
            parse_anthropic_text(&body).unwrap_err(),
            "anthropic: invalid x-api-key"
        );
    }

    #[test]
    fn claude_code_args_include_key_and_npx() {
        let args = claude_code_args("sk-test");
        assert_eq!(args[0], "ANTHROPIC_API_KEY=sk-test");
        assert!(args.iter().any(|a| a == "npx"));
        assert!(args.iter().any(|a| a.contains("claude-agent-acp")));
    }

    #[test]
    fn claude_code_key_prefers_own_then_anthropic() {
        assert_eq!(
            claude_code_key(Some("own".into()), Some("ant".into())),
            Some("own".into())
        );
        assert_eq!(claude_code_key(None, Some("ant".into())), Some("ant".into()));
        assert_eq!(claude_code_key(None, None), None);
    }

    #[test]
    fn offers_claude_code_when_anthropic_present() {
        assert_eq!(
            offerable_providers(vec!["anthropic".into()]),
            vec!["anthropic".to_string(), "claude-code".to_string()]
        );
        assert_eq!(offerable_providers(vec![]), Vec::<String>::new());
        assert_eq!(
            offerable_providers(vec!["claude-code".into()]),
            vec!["claude-code".to_string()]
        );
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

```bash
cargo test -p manch-desktop agent::tests
```
Expected: FAIL to compile — `anthropic_request_body`, `parse_anthropic_text`, `claude_code_args`, `claude_code_key`, `offerable_providers` don't exist yet.

- [ ] **Step 4: Add the impls + pure helpers (insert before the `#[cfg(test)]` module)**

```rust
/// Build the Anthropic Messages API request body. Pure.
fn anthropic_request_body(model: &str, prompt: &str) -> serde_json::Value {
    serde_json::json!({
        "model": model,
        "max_tokens": MAX_TOKENS,
        "messages": [{ "role": "user", "content": prompt }],
    })
}

/// Concatenate the `text` blocks of an Anthropic Messages response; surface
/// `error.message` when the body is an error. Pure.
fn parse_anthropic_text(body: &serde_json::Value) -> Result<String, String> {
    if let Some(err) = body.get("error") {
        let msg = err
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown error");
        return Err(format!("anthropic: {msg}"));
    }
    let content = body
        .get("content")
        .and_then(|c| c.as_array())
        .ok_or_else(|| "anthropic: response has no content array".to_string())?;
    let text: String = content
        .iter()
        .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
        .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
        .collect();
    if text.is_empty() {
        Err("anthropic: empty text response".to_string())
    } else {
        Ok(text)
    }
}

/// Spawn args for the Claude Code ACP adapter. Leading `NAME=value` tokens are
/// env vars; then the launch command. Pure.
fn claude_code_args(api_key: &str) -> Vec<String> {
    vec![
        format!("ANTHROPIC_API_KEY={api_key}"),
        "npx".into(),
        "-y".into(),
        CLAUDE_CODE_PKG.into(),
    ]
}

/// Key for the claude-code path: its own saved key, else the anthropic key. Pure.
pub fn claude_code_key(own: Option<String>, anthropic: Option<String>) -> Option<String> {
    own.or(anthropic)
}

/// Providers offerable in the UI: every saved one, plus `claude-code` whenever
/// `anthropic` is present (it reuses the anthropic key). Pure.
pub fn offerable_providers(mut saved: Vec<String>) -> Vec<String> {
    if saved.iter().any(|p| p == "anthropic") && !saved.iter().any(|p| p == "claude-code") {
        saved.push("claude-code".into());
    }
    saved.sort();
    saved.dedup();
    saved
}

/// BYOK Anthropic via a hand-rolled Messages-API call.
pub struct AnthropicAgent {
    api_key: String,
}

impl AnthropicAgent {
    pub fn new(api_key: String) -> Self {
        Self { api_key }
    }
}

#[async_trait]
impl ChatAgent for AnthropicAgent {
    async fn ask(&self, prompt: &str) -> Result<String, String> {
        let resp = reqwest::Client::new()
            .post(ANTHROPIC_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .json(&anthropic_request_body(ANTHROPIC_MODEL, prompt))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let status = resp.status();
        let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            return Err(parse_anthropic_text(&body)
                .err()
                .unwrap_or_else(|| format!("anthropic: HTTP {status}")));
        }
        parse_anthropic_text(&body)
    }
}

/// BYOC Claude Code over ACP. Stub until Task 3 wires the subprocess.
pub struct ClaudeCodeAgent {
    api_key: String,
}

impl ClaudeCodeAgent {
    pub fn new(api_key: String) -> Self {
        Self { api_key }
    }
}

#[async_trait]
impl ChatAgent for ClaudeCodeAgent {
    async fn ask(&self, _prompt: &str) -> Result<String, String> {
        let _ = &self.api_key; // used in Task 3
        Err("claude-code path not wired yet".to_string())
    }
}
```

- [ ] **Step 5: Run the tests to verify they pass**

```bash
cargo test -p manch-desktop agent::tests
```
Expected: PASS (8 tests). If `agent` module is not yet declared in `lib.rs`, it already is (`mod agent;` exists) — the build covers it.

- [ ] **Step 6: Verify rig is gone**

```bash
grep -ri "rig" apps/desktop/src-tauri/Cargo.toml Cargo.lock || echo "rig fully removed"
cargo build -p manch-desktop
```
Expected: "rig fully removed" and a clean build. (`Cargo.lock` updates automatically.)

- [ ] **Step 7: Commit**

```bash
git add apps/desktop/src-tauri/Cargo.toml apps/desktop/src-tauri/src/agent.rs Cargo.lock
git commit -m "feat(desktop): hand-rolled Anthropic client + ChatAgent seam, drop rig"
```

---

## Task 2: Wire the Anthropic path end-to-end (commands, frontend)

**Files:**
- Modify (rewrite): `apps/desktop/src-tauri/src/commands.rs`
- Modify: `apps/desktop/src/lib/api.ts`

**Interfaces:**
- Consumes: `Provider`, `ChatAgent`, `AnthropicAgent`, `ClaudeCodeAgent`, `claude_code_key`, `offerable_providers` from Task 1; `Db::{get_key, list_providers, save_key}` (existing).
- Produces: commands `save_api_key`, `list_configured_providers`, `send_prompt` (unchanged names/signatures).

- [ ] **Step 1: Rewrite `commands.rs`**

Replace the entire contents of `apps/desktop/src-tauri/src/commands.rs` with:

```rust
//! Tauri commands the frontend invokes. Thin glue over `db` + `agent`.

use crate::agent::{
    claude_code_key, offerable_providers, AnthropicAgent, ChatAgent, ClaudeCodeAgent, Provider,
};
use crate::db::Db;
use tauri::State;

#[tauri::command]
pub fn save_api_key(state: State<Db>, provider: String, api_key: String) -> Result<(), String> {
    if Provider::from_id(&provider).is_none() {
        return Err(format!("unknown provider: {provider}"));
    }
    state.save_key(&provider, &api_key).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_configured_providers(state: State<Db>) -> Result<Vec<String>, String> {
    let saved = state.list_providers().map_err(|e| e.to_string())?;
    Ok(offerable_providers(saved))
}

#[tauri::command]
pub async fn send_prompt(
    state: State<'_, Db>,
    provider: String,
    text: String,
) -> Result<String, String> {
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
            let own = state.get_key("claude-code").map_err(|e| e.to_string())?;
            let ant = state.get_key("anthropic").map_err(|e| e.to_string())?;
            let key = claude_code_key(own, ant)
                .ok_or_else(|| "no API key saved for claude-code (or anthropic)".to_string())?;
            Box::new(ClaudeCodeAgent::new(key))
        }
    };
    agent.ask(&text).await
}
```

- [ ] **Step 2: Update the frontend provider list**

Replace the `Provider` type and `PROVIDERS` const in `apps/desktop/src/lib/api.ts` (leave the `invoke` wrappers unchanged):

```ts
export type Provider = "anthropic" | "claude-code";

export const PROVIDERS: { id: Provider; label: string }[] = [
  { id: "anthropic", label: "Anthropic (Claude · BYOK)" },
  { id: "claude-code", label: "Claude Code (ACP)" },
];
```

- [ ] **Step 3: Build + test + lint**

```bash
cargo build -p manch-desktop && cargo test -p manch-desktop
pnpm --filter @manch/desktop lint
```
Expected: build + 8 Rust tests pass; lint passes.

- [ ] **Step 4: Manual — Anthropic answers correctly (rig-free)**

```bash
pnpm --filter @manch/desktop tauri dev
```
Save an Anthropic key, select **Anthropic (Claude · BYOK)**, ask `What is the capital of India?` → assistant bubble answers **New Delhi**. (Selecting **Claude Code** here returns the "not wired yet" error — expected until Task 3.)

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src-tauri/src/commands.rs apps/desktop/src/lib/api.ts
git commit -m "feat(desktop): wire BYOK Anthropic path end-to-end via ChatAgent"
```

---

## Task 3: Claude Code over ACP (`ClaudeCodeAgent`)

**Files:**
- Modify: `apps/desktop/src-tauri/src/agent.rs` (replace the `ClaudeCodeAgent` `ask` stub)

**Interfaces:**
- Consumes: `claude_code_args` (Task 1), `agent-client-protocol` `=0.14.0`.
- Produces: a working `ChatAgent for ClaudeCodeAgent`.

> **VERIFY THE API FIRST.** The 0.14 crate is a builder-based rewrite; the names below are a research hypothesis from `agentclientprotocol/rust-sdk` tag `v0.14.0` (`examples/yolo_one_shot_client.rs`, `src/.../{acp_agent,session}.rs`). Before coding, confirm against the real crate:
> ```bash
> cargo doc -p agent-client-protocol --no-deps --open   # or browse docs.rs/agent-client-protocol/0.14.0
> ```
> Confirm: `AcpAgent::from_args`, `Client.builder().name(..).on_receive_request(.., on_receive_request!()).on_receive_notification(.., on_receive_notification!()).connect_with(agent, |conn| async {..})`, `conn.send_request(InitializeRequest::new(ProtocolVersion::V1)).block_task().await`, and `conn.build_session_cwd()?.block_task().run_until(|mut s| async { s.send_prompt(..)?; s.read_to_string().await }).await`. If a name differs, adjust — `ask` is the only place that changes.

- [ ] **Step 1: Replace the `ClaudeCodeAgent::ask` stub**

Swap the stub `impl ChatAgent for ClaudeCodeAgent` for:

```rust
#[async_trait]
impl ChatAgent for ClaudeCodeAgent {
    async fn ask(&self, prompt: &str) -> Result<String, String> {
        use agent_client_protocol as acp;
        use acp::schema::{
            InitializeRequest, ProtocolVersion, RequestPermissionOutcome,
            RequestPermissionRequest, RequestPermissionResponse, SelectedPermissionOutcome,
            SessionNotification,
        };

        let agent = acp::AcpAgent::from_args(claude_code_args(&self.api_key))
            .map_err(|e| e.to_string())?;
        let prompt = prompt.to_string();

        acp::Client
            .builder()
            .name("manch-desktop")
            // One-shot: auto-approve the first permission option, no UI.
            .on_receive_request(
                async move |req: RequestPermissionRequest, responder, _conn| {
                    let outcome = match req.options.first() {
                        Some(opt) => RequestPermissionOutcome::Selected(
                            SelectedPermissionOutcome::new(opt.option_id.clone()),
                        ),
                        None => RequestPermissionOutcome::Cancelled,
                    };
                    responder.respond(RequestPermissionResponse::new(outcome))
                },
                acp::on_receive_request!(),
            )
            // read_to_string handles assistant text; observer is a no-op for now.
            .on_receive_notification(
                async move |_n: SessionNotification, _cx| Ok(()),
                acp::on_receive_notification!(),
            )
            .connect_with(agent, move |connection| async move {
                connection
                    .send_request(InitializeRequest::new(ProtocolVersion::V1))
                    .block_task()
                    .await?;
                connection
                    .build_session_cwd()?
                    .block_task()
                    .run_until(move |mut session| async move {
                        session.send_prompt(&prompt)?;
                        session.read_to_string().await
                    })
                    .await
            })
            .await
            .map_err(|e| e.to_string())
    }
}
```

- [ ] **Step 2: Build (this is where API mismatches surface)**

```bash
cargo build -p manch-desktop
```
Expected: clean build. If the compiler rejects a method/type name, reconcile with the verified API from the VERIFY step (do not guess — read the crate). Re-run until green.

- [ ] **Step 3: Run unit tests (no regressions)**

```bash
cargo test -p manch-desktop
```
Expected: 8 tests pass (ACP `ask` is integration-tested manually in Task 4).

- [ ] **Step 4: Commit**

```bash
git add apps/desktop/src-tauri/src/agent.rs Cargo.lock
git commit -m "feat(desktop): drive Claude Code over ACP via ChatAgent"
```

---

## Task 4: Manual acceptance — both paths

**Files:** none (verification only).

**Prereqs:** `node`/`npx` on PATH; network access for `npx -y @agentclientprotocol/claude-agent-acp@latest`.

- [ ] **Step 1: Launch**

```bash
pnpm --filter @manch/desktop tauri dev
```

- [ ] **Step 2: BYOK Anthropic** — save an Anthropic key, select **Anthropic (Claude · BYOK)**, ask `What is the capital of India?` → **New Delhi**.

- [ ] **Step 3: Claude Code (ACP)** — select **Claude Code (ACP)** (offered automatically once an Anthropic key is saved), ask the same question → **New Delhi**, routed through the `claude-agent-acp` subprocess. First run may pause while `npx` fetches the adapter.

- [ ] **Step 4: Error path** — save a deliberately invalid key, ask again on each path → a red error bubble, not a crash.

- [ ] **Step 5: Record the result.** Both paths answering **New Delhi** behind the one `ChatAgent` interface = milestone met.

---

## Self-Review

**Spec coverage:**
- Rip out rig → Task 1 (Step 1 deps, Step 6 verify). ✅
- Hand-rolled Anthropic client (request build + response parse) → Task 1 (helpers + `AnthropicAgent`), unit-tested. ✅
- One interface / two impls (`ChatAgent`) → Task 1. ✅
- Providers `anthropic` + `claude-code`, claude-code key fallback, offerable list → Task 1 helpers + Task 2 commands. ✅
- Claude Code over ACP → Task 3. ✅
- Frontend provider list → Task 2. ✅
- Acceptance "capital of India" on both paths → Task 4. ✅
- Out of scope (streaming, tools, server, encryption, Gemini) → correctly absent.

**Placeholder scan:** No TBD/TODO. The one deferred specific is the exact ACP 0.14 API surface, explicitly gated behind a VERIFY step with the source/docs to confirm against (matches how the prior plan handled rig's API). Anthropic model id is concrete (`claude-opus-4-8`).

**Type consistency:** `Provider` variants (`Anthropic`, `ClaudeCode`) and `Provider::from_id` ids (`"anthropic"`, `"claude-code"`) match across `agent.rs`, `commands.rs`, `api.ts`. `ChatAgent::ask`, `AnthropicAgent::new`, `ClaudeCodeAgent::new`, `claude_code_key`, `offerable_providers` named identically wherever used. Command names (`save_api_key`, `list_configured_providers`, `send_prompt`) unchanged → `generate_handler!` in `lib.rs` needs no edit.

**Risk note:** ACP 0.14's builder/macro API is the only real unknown; it is isolated inside `ClaudeCodeAgent::ask`. The runtime mix (the crate's `async_process`/`futures` internals under Tauri's tokio runtime) is the second risk — if `connect_with(..).await` misbehaves under tokio, fall back to `tokio::task::spawn_blocking` around a dedicated executor. Both risks are contained to Task 3.
