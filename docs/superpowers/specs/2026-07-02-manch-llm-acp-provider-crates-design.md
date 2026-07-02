# manch-llm + manch-acp: reusable provider crates (Gemini + Codex, BYOK + CLI)

- **Date:** 2026-07-02
- **Status:** Approved (design), pending implementation plan
- **Builds on:** `docs/superpowers/specs/2026-06-17-dual-path-hello-world-design.md` (the inline Anthropic BYOK + Claude Code ACP slice this extracts)

## Motivation

Add **Gemini** and **Codex** support — each on **both** paths (BYOK direct API + CLI over ACP) —
by carrying forward the existing Anthropic two-path design, and **extract that logic out of
the desktop app into reusable, publishable crates** so it can be consumed by other projects
(e.g. NYAYVAANI, pi-health).

Today both paths live inline in `apps/desktop/src-tauri/src/agent.rs` behind a local
`ChatAgent` trait that emits `manch_dto::StreamEvent`. This work moves them into crates that
implement the **already-existing** `manch_protocol::Agent` contract.

## The two-path split (settled, non-negotiable rationale)

Manch keeps **BYOK** and **CLI** as separate paths on purpose:

- **BYOK path** = a **direct provider API client** (Anthropic Messages, Gemini
  `generateContent`, OpenAI Chat Completions). It does **not** speak the ACP *wire protocol*;
  it only **emits ACP's event/content *types*** so the UI is uniform. File/shell tooling on
  this path is done by **Rust-implemented host tools** (host-registered
  `manch_protocol::Tool`, dispatched by the future `manch-core`), **never** by delegating to a
  CLI. That tool layer is **out of scope here** — this extraction preserves today's behavior
  (stream text; `tools` argument ignored).
- **CLI path** = an external agent (claude-code / gemini-cli / codex) driven over the **ACP
  wire protocol**; the agent brings its own tool loop and its own file/shell tools.

This matches how Zed and AionUi work (direct built-in BYOK agent + external CLI agents over
ACP; neither routes BYOK through ACP). See memory `byok-cli-two-path-rationale`.

## Key decisions

| Decision | Choice |
|----------|--------|
| Crate layout | **Hybrid**: `manch-llm` (BYOK, Cargo features per provider) + `manch-acp` (CLI over ACP). Both **`publish = false` for now** — names/APIs still churning; release via `release-plz` later. |
| Contract home | **`manch-protocol`** — already defines `Agent`, `EventSink`, `AgentEvent`. No new contract needed. |
| Completion layer | **No `rig`** — hand-rolled thin `reqwest` clients; per-provider quirks are **data, not branches**. |
| Wire dialects | Anthropic Messages · OpenAI Chat Completions (Codex BYOK) · Gemini `generateContent` — 3 dialects. |
| `manch-llm` execution surface | **None.** Forbids `std::process` and filesystem-write. Safe by construction. |
| Scope | **Full**: both crates, all 6 agents, desktop rewired onto `manch_protocol::Agent`, inline `ChatAgent` deleted. |

## Architecture: the contract mapping

Every agent — BYOK or CLI — implements the existing:

```rust
// manch_protocol
async fn prompt(&self, ctx: Context, tools: &[ToolSchema], sink: &dyn EventSink)
    -> Result<StopReason>;
```

Mapping rules (identical for both paths):

- **Text tokens** → `sink.emit(AgentEvent::Update(SessionUpdate::AgentMessageChunk { ContentBlock::Text })).await`
- **Tool activity** (CLI path only) → `AgentEvent::Update(SessionUpdate::ToolCall | ToolCallUpdate)` forwarded verbatim
- **Errors** → `Err(manch_protocol::Error)` (no more mid-stream `emit(Error)`; the `AgentEvent` enum has no Error variant)
- **End** → `Ok(StopReason)`
- **`tools`** → ignored on both paths for now (host-tool dispatch is a future `manch-core` concern)
- **Prompt** → read from `ctx.blocks` (the last user text block / concatenation); the desktop builds a one-block `Context` until real memory (`manch-memory`) exists

The desktop provides an **async** `EventSink` impl that maps `AgentEvent` → `manch_dto::StreamEvent`
for the frontend (Update→Token/Tool, Done→Done, and a returned `Err`→Error). The ts-rs DTO
and the React frontend are untouched.

## `manch-llm` (BYOK)

```
crates/manch-llm/
  Cargo.toml        features = ["anthropic","gemini","openai"] (default = all three)
  src/
    lib.rs          ensure_crypto_provider(); shared SSE byte-frame splitter
                    (the multibyte-safe \n line buffer, moved verbatim from agent.rs);
                    helper to build AgentEvent text chunks; Provider ids for BYOK
    anthropic.rs    #[cfg(feature="anthropic")] AnthropicAgent + pure fns
    gemini.rs       #[cfg(feature="gemini")]    GeminiAgent + pure fns
    openai.rs       #[cfg(feature="openai")]    OpenAiAgent (Codex BYOK) + pure fns
```

Deps: `reqwest` (rustls-no-provider + stream), `rustls` (ring), `futures-util`, `serde`,
`serde_json`, `async-trait`, `manch-protocol`. **No `std::process`, no fs-write** — enforced
by review rule and noted in the crate docs. `publish = false` for now (see decisions table).

Each provider is the same shape (the Anthropic code is the template):
- pure `request_body(model, prompt) -> Value`
- pure `parse_sse_delta(data) -> Option<String>` (text delta for that dialect)
- pure `parse_sse_error(data) -> Option<String>`
- pure `parse_text(body) -> Result<String>` (non-stream error surfacing)
- pure `parse_models(body) -> Vec<ModelInfo>` (list-models response → catalog)
- one `impl manch_protocol::Agent`

Per-provider data:

| Provider | id | Chat URL | List-models URL | Auth header | Stream | Fallback model |
|----------|----|----------|-----------------|-------------|--------|----------------|
| Anthropic | `anthropic` | `…/v1/messages` | `…/v1/models` | `x-api-key` + `anthropic-version` | SSE | `claude-opus-4-8` (authoritative — do not change) |
| Gemini | `gemini` | `…/v1beta/models/{model}:streamGenerateContent?alt=sse` | `…/v1beta/models` | `x-goog-api-key` | SSE | `gemini-3-flash` |
| OpenAI (Codex BYOK) | `openai` | `…/v1/chat/completions` | `…/v1/models` | `Authorization: Bearer` | SSE (`stream:true`) | `gpt-5-class` (confirm at impl) |

The `model` is a **constructor parameter** on each agent, not a const — the const above is only
a **fallback** used when the fetched catalog is unavailable (no network / invalid key / user has
not chosen). The multibyte-safe byte-buffer SSE framing (splitting on the `\n` byte, decoding
whole lines) is shared in `lib.rs` and reused by all three; only the per-line `parse_sse_delta`
differs.

## Model selection (fetch + user-select)

BYOK model ids are **discovered from the provider**, not hardcoded. `manch-llm` exposes a
BYOK-only capability:

```rust
// manch-llm — implemented by the three BYOK providers only (not the CLI agents)
pub struct ModelInfo { pub id: String, pub display_name: Option<String> }

#[async_trait]
pub trait ModelCatalog {
    async fn list_models(&self, api_key: &str) -> manch_protocol::Result<Vec<ModelInfo>>;
}
```

- Each provider's `list_models` = a `GET` to its list-models URL + the pure `parse_models`.
- On failure it returns the single **fallback** model, so the UI always has ≥1 selectable option.
- **CLI path is unaffected** — claude-code/gemini-cli/codex own their own model selection (BYOC);
  `manch-acp` does not implement `ModelCatalog`.

**Desktop wiring:**
- Tauri command `list_models(provider) -> Vec<ModelInfo>` (fetched on demand, e.g. when the
  model dropdown opens or after a key is saved).
- The user's chosen model id is **persisted per provider** in `db` (new nullable `model` column
  or a small `provider_settings` row; `NULL` → use fallback) and passed into agent construction
  in `send_prompt_stream`.
- Frontend: a model dropdown per BYOK provider, populated from `list_models`.

## `manch-acp` (CLI over ACP)

```
crates/manch-acp/
  Cargo.toml
  src/
    lib.rs   AcpCliAgent { launch: LaunchSpec } impl manch_protocol::Agent
             LaunchSpec { args: Vec<String>, api_key_env: Option<&'static str> }
             builders: claude_code(key), gemini_cli(key), codex(key)
             push_chunk() + tool_status() (moved from agent.rs)
```

Deps: `agent-client-protocol` (unstable), `async-trait`, `manch-protocol`, `tokio`.

Generalizes today's `ClaudeCodeAgent`: the `Client.builder()…connect_with` /
`initialize(V1)` / `new_session` / `prompt` machinery moves in **once**. The only per-CLI
difference is the launch spec (leading `NAME=value` token = subprocess env var):

| Agent | id | Launch args | Key env (optional override) |
|-------|----|-------------|-----------------------------|
| Claude Code | `claude-code` | `npx -y @agentclientprotocol/claude-agent-acp` | `ANTHROPIC_API_KEY` |
| Gemini CLI | `gemini-cli` | `gemini --experimental-acp` (or `npx -y @google/gemini-cli --experimental-acp`) | `GEMINI_API_KEY` |
| Codex | `codex` | `npx -y @zed-industries/codex-acp` | `OPENAI_API_KEY` |

All three are **BYOC** (bring-your-own-CLI): the agent owns its auth; the key is an optional
override, never borrowed from the BYOK `anthropic`/`openai`/`gemini` keys. The notification
handler emits `AgentEvent::Update(session_update)` **directly** — it already receives ACP
`SessionUpdate`s, so no translation to an intermediate type.

### Permission policy (security note)

Today's inline code **auto-approves every `RequestPermissionRequest`** (`options.first()`).
Since `manch-acp` becomes reusable, expose a permission callback on `AcpCliAgent`. Default
behavior **preserves parity** with today (auto-approve) to avoid changing desktop UX in this
extraction, but the hook makes the RCE surface (issue #7) a **conscious call-site choice** for
other consumers. Tightening the default to deny/prompt is a follow-up, not part of this spec.

## Desktop rewiring

- `Cargo.toml`: remove `reqwest`/`rustls`/`futures-util`/`agent-client-protocol` (now in the
  crates); add `manch-llm` (all features) + `manch-acp`.
- Delete `agent.rs`'s `ChatAgent` / `AnthropicAgent` / `ClaudeCodeAgent`. Keep a thin
  `resolve_agent(provider, &Db) -> Result<Box<dyn manch_protocol::Agent>>` factory.
- `Provider` ids expand: BYOK `anthropic|gemini|openai`, CLI `claude-code|gemini-cli|codex`.
  `offerable_providers` continues to force-include the BYOC CLIs.
- `commands.rs::send_prompt_stream`: build a one-block `Context`, pass an async `EventSink`
  that maps `AgentEvent` → `manch_dto::StreamEvent` (and the `Err` return → `StreamEvent::Error`).
- `db.rs`: key storage is already keyed by provider string; `save_api_key`'s id validation
  widens to the new ids. **One additive migration**: persist the per-provider selected model
  (nullable `model` column / `provider_settings` row; `NULL` → fallback).

## Testing

- Move each provider's pure-fn tests into its crate; add Gemini + OpenAI dialect tests
  (request body shape, `parse_sse_delta`, `parse_sse_error`, `parse_models`) mirroring the
  Anthropic ones. `parse_models` is tested against a captured list-models JSON body per provider.
- `LaunchSpec` builder tests (arg shape, env prepending) mirroring today's `claude_code_args` tests.
- Desktop: unit test the `AgentEvent → StreamEvent` mapping.
- Gate: `just ci` green (fmt/clippy/test/build). No live-API tests (no keys in CI).

## Build sequence

1. `manch-llm` skeleton + move Anthropic (proves the `manch_protocol::Agent` mapping) + its
   `ModelCatalog::list_models`.
2. Add `gemini` + `openai` dialects (chat + `list_models`).
3. `manch-acp`: generalize from `ClaudeCodeAgent`; add `gemini-cli` + `codex` launch specs.
4. Rewire desktop onto the crates; delete inline `ChatAgent`; add `list_models` command +
   model persistence + the model dropdown.
5. `just ci`.

## Out of scope (future milestones)

- **Host tools + agentic loop** (Rust file/shell `Tool`s, `prompt→tool→re-prompt`): belongs to
  `manch-core` (issue #1) + a future `manch-tools`. This spec adds no execution to the BYOK path.
- Tightening the ACP permission default from auto-approve to deny/prompt (issue #7).
- Publishing/versioning the new crates — both stay `publish = false` until names/APIs settle;
  `release-plz` handles them when we flip the flag.
```
