# Slice 1 — Desktop BYOK Chat Window

**Date:** 2026-06-15
**Status:** Proposed (awaiting review)

## Context & goal

Manch is a domain-free agent substrate; the larger product on top is **NYAYVAANI** (a
closed, BYOK-only, local-first Tauri legal workspace). The first proof is a working chat
window.

**Acceptance:** in the desktop app, the user enters a provider API key, asks
*"What is the capital of India?"*, and gets the answer ("New Delhi") in a chat window.

## Architectural direction (the spine — for context, not all built here)

Two engines, selectable as a feature of `manch-core`:

- **BYOK engine = `rig` in full mode** (agent loop + function-calling + multi-provider).
  rig owns the BYOK loop; we do not hand-write it.
- **CLI engine = ACP**, added later as a **separate crate** (`manch-acp`) only when CLI
  connections are wanted. Not built now.

ACP's *wire protocol* is not needed for BYOK and is deferred. ACP's content/event **types**
(`ContentBlock`, `ToolCall`, `SessionUpdate`, `StopReason`) remain the vocabulary so the UI
renders one shape and a future CLI path drops in cleanly.

**Open question (resolve at Slice 2, when tools land — does not block Slice 1):** whether
consumer tools (file access, NYAYVAANI legal tools) are authored directly as rig tools
(simplest, but couples consumers to rig) or against a thin `manch_protocol::Tool` adapter
that `manch-core` wraps onto rig (recommended — keeps consumers decoupled and BYOK/CLI
interchangeable). Slice 1 registers no tools, so both look identical here.

## Scope of Slice 1

UI-first and **inline in `apps/desktop/src-tauri`** (extract `manch-*` crates later, once
boundaries are clear). **No tools, no server, no ACP, no CLI, no streaming.**

### Components

1. **SQLite store** (inline in `src-tauri`).
   - Table: `provider_keys(provider TEXT PRIMARY KEY, api_key TEXT NOT NULL)`.
   - Crate: `rusqlite` (simplest) or `sqlx` — decide at implementation.
   - Encryption (SQLCipher) deferred but flagged. This DB also becomes the future
     session/memory store.

2. **Settings UI.** A modal/screen to enter & save a key per provider (Anthropic, Gemini).
   - Commands: `save_api_key(provider, key)`, `list_configured_providers() -> [provider]`.

3. **Chat UI** — the home route becomes the chat window.
   - Message list (daisyUI chat bubbles), input box, provider picker showing only
     configured providers.

4. **`prompt` command.** `prompt(provider, session_id, text) -> Result<String, String>`.
   - Loads the key from SQLite → calls the provider via rig (full mode, no tools registered
     ⇒ behaves as a single completion) → returns the answer text.
   - Missing key / HTTP errors surface as clear messages in the chat.
   - Return shape kept ACP-content-compatible so the renderer and future CLI path don't change.

5. **rig integration.** A rig client per provider; model id set via the `claude-api` skill
   (Anthropic) and context7 (Gemini) at implementation. Map rig's output to the UI message
   shape.

### Out of scope (later slices)

- **Slice 2:** extract `manch-core` (engine facade + registry) + first host **Tool** (file
  access) + resolve the rig-tool-vs-`manch_protocol::Tool`-adapter question.
- **Slice 3:** more Channels (Telegram); server wiring (ConnectRPC) reusing the engine.
- **Later:** `manch-acp` crate (CLI path); key encryption (SQLCipher); token streaming.

### Testing

- SQLite store: save/get/list round-trip (unit).
- `prompt` mapping: rig response → UI message shape, behind a small seam so it is testable
  without network.
- **Manual acceptance:** real Anthropic key → "What is the capital of India?" → "New Delhi",
  in the desktop window.

### Risks / notes

- rig API surface, version, and model ids: verify at implementation (context7 + `claude-api`).
- Tauri async commands + rig (tokio): compatible.
- Keys are user-provided via the UI and stored in SQLite (not env vars).
- Honor the product discipline: *"Manch compiling cleanly is not progress toward a paying
  customer"* — keep this slice small.
