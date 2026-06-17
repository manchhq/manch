# Dual-Path Hello World — Design Spec

**One question, two agent paths, one interface.**

## Goal

Ask *"What is the capital of India?"* in the desktop chat and get **New Delhi**,
proving the answer can travel through **either** agent path behind a single
interface:

1. **BYOK Anthropic** — Manch's own hand-rolled Anthropic Messages-API client
   over `reqwest`. **No `rig`.**
2. **Claude Code (BYOC)** — the `claude-agent-acp` adapter launched as a
   subprocess and driven over **ACP** (`agent-client-protocol` crate).

Same prompt, same `ChatAgent` interface, two implementations. This is the
README's "first milestone", doubled to prove the unified seam (plan B) holds
across a raw provider and an external CLI.

## Why this slice

- It rips out the one place Manch still depends on `rig` (`agent.rs`), executing
  the "thin hand-rolled clients, not a framework" decision now recorded in the
  README.
- It is the smallest change that exercises **both** agent paths, which is the
  thing the whole architecture is betting on.

## Architecture

- **UI-first, inline.** All logic stays in `apps/desktop/src-tauri` for now;
  extraction into `manch-core` / `manch-acp` comes later (per working style).
- **One interface, two impls.** A local trait:

  ```rust
  #[async_trait]
  trait ChatAgent: Send + Sync {
      async fn ask(&self, prompt: &str) -> Result<String, String>;
  }
  ```

  `AnthropicAgent` (BYOK) and `ClaudeCodeAgent` (ACP) both implement it. This is
  the deliberate, lightweight stand-in for `manch_protocol::Agent` — same idea
  (one interface), minus streaming / tools / `EventSink`, which this slice does
  not need. When `manch-core` is extracted, `ChatAgent` collapses into the real
  `Agent` trait; `ask` becomes `prompt(..)` streaming through an `EventSink`.
- **Non-streaming, no tools, no server.** Out of scope for this slice.

## Providers in this slice

| id | impl | key source |
|----|------|-----------|
| `anthropic` | `AnthropicAgent` (hand-rolled Messages API) | saved `anthropic` key |
| `claude-code` | `ClaudeCodeAgent` (ACP subprocess) | saved `claude-code` key, else falls back to the `anthropic` key |

Gemini is **dropped** from this slice (it was rig-backed). It returns later via
the README's provider roadmap (Gemini-native / OpenAI-compatible work).

## Anthropic client (hand-rolled)

- `POST https://api.anthropic.com/v1/messages`
- Headers: `x-api-key: <key>`, `anthropic-version: 2023-06-01`, `content-type: application/json`
- Body: `{ "model": "claude-opus-4-8", "max_tokens": 1024, "messages": [{ "role": "user", "content": <prompt> }] }`
- Success: response `.content[]` is an array of blocks; concatenate every
  `{ "type": "text", "text": ... }` block's text.
- Failure: non-2xx → surface `error.message` if present, else `"<status>: <body>"`.

Model id `claude-opus-4-8` is authoritative per the `claude-api` skill — do not change it.

## Claude Code over ACP

- Adapter: `@agentclientprotocol/claude-agent-acp` (canonical; the old
  `@zed-industries/claude-code-acp` is deprecated), launched via
  `npx -y @agentclientprotocol/claude-agent-acp@latest`.
- Auth: pass the resolved key as `ANTHROPIC_API_KEY` in the subprocess env. No
  ACP `authenticate` call needed.
- Crate: `agent-client-protocol = { version = "=0.14.0", features = ["unstable"] }`
  (match Zed's pin).
- Flow (client/host side): `initialize(V1)` → new session → `session/prompt` →
  collect streamed `AgentMessageChunk` text until the final `StopReason`.

> **API-accuracy caveat.** The 0.14 crate is a builder-based rewrite. The exact
> type/method names (`AcpAgent::from_args`, `Client.builder()`, `connect_with`,
> `build_session_cwd`, `read_to_string`, the `on_receive_*!` macros) are a
> research hypothesis and **must be verified against the actual crate** (docs.rs
> / source / compiler) when implementing the ACP task. If names differ, the ACP
> impl is the only place to adjust — it is isolated behind `ChatAgent`.

## Out of scope

Streaming, host-registered tools, the optional server, ACP permission/tool-call
UI, key encryption at rest, Gemini/other providers, `manch-core` extraction.

## Acceptance

1. Save an Anthropic key, ask "What is the capital of India?", select
   **Anthropic** → assistant answers **New Delhi**.
2. With Claude Code installed (`npx` available) and a key saved, select
   **Claude Code** → assistant answers **New Delhi** (routed through the ACP
   subprocess).
3. Bad key on either path → a red error bubble, not a crash.
4. `cargo test -p manch-desktop` passes (pure unit tests for request building +
   response parsing + provider/arg helpers).
5. `rig-core` no longer appears in `apps/desktop/src-tauri/Cargo.toml` or
   `Cargo.lock`.
