# Manch UI — real ACP streaming + visual polish (design)

**Date:** 2026-07-01
**Scope:** Two independent workstreams shipped under one spec: (A) real
token/tool-call streaming from the Rust agents to the stage, replacing the
`mockEngine` seam with a live `tauriEngine`; (B) a theme-agnostic, motion-led
visual polish pass ("theater × kathputli"), plus expanding the daisyUI theme
picker from 5 to all 32 built-in themes.
**Status:** Approved design, pending implementation plan.
**Builds on:** `2026-06-30-manch-ui-multi-section.md` (PR #14, merged) and the
issue #15 follow-ups (PR #16, merged). Closes the two large deferred items
tracked in **GitHub issue #17**.

## Context

The desktop reference app streams its transcript through a `StageEngine` seam
(`apps/desktop/src/engine/StageEngine.ts`) whose event union already models
`token` / `tool` / `done` / `error`. Today the stage is wired to `mockEngine`,
which yields incremental tokens and a fake tool call; `tauriEngine` exists but
collapses a whole reply into a single `send_prompt` call and one `token` event.

Crucially, **the frontend transcript pipeline is already event-driven**:
`useSend` consumes the async iterable, `applyEvent`/`transcript.ts` folds events
into `LiveState`, and the `streamingText` / `liveToolCalls` jotai atoms drive the
live UI. `mockEngine` proves token-by-token rendering works end to end.

On the Rust side, `ClaudeCodeAgent::ask` (`src-tauri/src/agent.rs`) **already
receives** streaming `AgentMessageChunk` notifications over ACP — it buffers them
(`merge_chunk`) and returns one `String`. `AnthropicAgent::ask` makes a single
non-streaming Messages-API call. The `send_prompt` command is `async` and returns
`Result<String, String>`. The `agent.rs` module doc-comment already anticipates
the evolution: *"`ask` becomes a streaming `prompt` through an `EventSink`."*

So "real streaming" is **~70% built**; the missing piece is a transport to push
per-event data Rust→JS, an `EventSink` the agents emit into, and a `tauriEngine`
that bridges the transport into its `AsyncIterable`.

Separately, the app is **theme-agnostic by design** (components use semantic
daisyUI tokens; colors come from the selected theme). Five themes are exposed
today (`dark`, `light`, `dracula`, `nord`, `cupcake`); the intent is to expose
all 32 daisyUI built-ins. The "theater × kathputli" signature look was never
tuned — and because the palette belongs to the theme, that signature must be
carried by **motion and structural nuance**, not color.

## Goals

**Part A — Real ACP streaming**

- Replace the `mockEngine` wiring on the Stage with a live `tauriEngine` that
  streams real tokens and tool calls from the Rust agents.
- Both providers stream **tokens** live; **tool-calls** come from Claude Code
  (Anthropic's request wires no tools, so it has none to stream):
  - **Anthropic** via SSE (`stream: true`, parse `content_block_delta` →
    `text_delta`), emitting `token` events only.
  - **Claude Code** forwards the `AgentMessageChunk` tokens it already receives
    **and** its ACP `ToolCall` / `ToolCallUpdate` notifications as `tool` events
    (running → done), emitting live `ToolCallCard`s.
- Transport: a typed Tauri `Channel<StreamEvent>` passed to a new
  `send_prompt_stream` command (per-invoke, no global event bus).
- `useSend`, the streaming atoms, and `applyEvent` are **untouched** — they are
  already event-driven.

**Part B — Visual polish (theme-agnostic, motion-led)**

- A shared **motion vocabulary** in `@manch/ui`: spotlight focus, stage/curtain
  reveal for section transitions, message + tool-card entrance — all
  `currentColor` and `prefers-reduced-motion`-guarded.
- A **kathputli `PuppetLoader`** (marionette-on-strings SVG, CSS-animated)
  replacing the current busy/loading states (streaming indicator, the
  `EmptyState glyph="⏳"` loading states).
- A **per-component nuance pass** on the hero surfaces (Stage, StageHeader,
  Transcript, GreenRoomView, Spotlight): spacing rhythm, type scale / weight
  contrast, elevation via daisyUI shadow tokens, subtle string/thread ornament
  using semantic tokens only.
- **Theme picker → all 32** daisyUI built-ins, in a scrollable/grouped picker.

## Non-goals (YAGNI for now)

- **Extracting `manch-anthropic`** (or any per-provider crate). The streaming
  Anthropic client stays **inline** in `apps/desktop/src-tauri/src/agent.rs`,
  consistent with the "build inline first, extract crates later" working style.
  The crate extraction — which designs the streaming `Agent` + `EventSink`
  contract in `manch-protocol` — is its own next milestone (filed separately).
  The inline streaming code written here is exactly what moves into the crate
  later; no rework.
- No changes to `manch-protocol` or `manch-dto`'s existing types beyond adding
  the `StreamEvent` wire enum.
- No new JS animation-library dependency (motion is CSS-first).
- No custom bespoke theme — the signature is motion + structure, not a palette.
- Team runs / cross-verify / search stay mock/computed (unchanged from PR #14).
- No streaming cancellation/abort UI (a natural follow-up, not required here).

## Decisions (validated with the user)

1. **One combined spec, two clearly separated workstreams.** Parts A and B share
   no files; the implementation plan phases them so either can land/roll back
   independently.
2. **Streaming fidelity = tokens + live tool-calls**, both providers, first cut.
3. **Transport = typed Tauri `Channel<StreamEvent>`**, not a global `emit`/`listen`
   bus. Per-invoke and typed.
4. **Motion = CSS-first**, no framer-motion or other runtime dep; keeps `@manch/ui`
   dependency-clean. The kathputli loader is an inline animated SVG.
5. **Signature look = small nuances + animation** carried across all 32 themes;
   daisyUI owns color so development isn't spent re-fixing palettes per theme.
6. **Theme expansion (5 → 32) is in this spec.**
7. **Anthropic SSE streaming is in scope now.**
8. **`manch-anthropic` extraction is deferred** to its own milestone (see
   Non-goals); streaming lands inline in `agent.rs`.

## Part A — architecture

### Wire event type (`manch-dto`)

Add a `StreamEvent` enum mirroring the existing `StageEvent` TS union, so
`bindings.ts` gets a typed counterpart:

```
enum StreamEvent {
  Token { text: String },
  Tool  { id: String, name: String, status: String /* running|done|error */, detail: Option<String> },
  Done,
  Error { message: String },
}
```

Serialized to match `StageEvent`'s shape (a `kind` discriminator via serde
`#[serde(tag = "kind", rename_all = "camelCase")]`) so `tauriEngine`'s mapping
is near-identity. Generated to `apps/desktop/src/data/bindings.ts` via
the existing `ts-rs` / `just gen` path.

### `EventSink` + streaming trait (`agent.rs`)

- `EventSink` — a thin wrapper over `tauri::ipc::Channel<StreamEvent>` with
  `token(&str)`, `tool(...)`, `done()`, `error(&str)` helpers. Keeps the agents
  ignorant of Tauri specifics beyond the sink.
- `ChatAgent` gains `async fn stream(&self, prompt: &str, sink: &EventSink) ->
  Result<(), String>`. The existing `ask` can be kept as a thin
  buffer-over-`stream` helper or removed if unused after the switch (decide in
  the plan; nothing outside `send_prompt` calls it).

### `AnthropicAgent::stream` — SSE

- Request body gains `"stream": true`; POST with `reqwest`, consume
  `resp.bytes_stream()`.
- A **pure** `parse_sse_event(data_line: &str) -> Option<AnthropicDelta>` maps a
  single SSE `data: {...}` line to an optional token (handling
  `content_block_delta` / `text_delta`, ignoring `message_start`, `ping`, etc.).
  Unit-testable without a network.
- Emit `sink.token(delta)` per delta; `sink.done()` at `message_stop`; map error
  bodies to `sink.error(...)` (reuse `parse_anthropic_text`'s error branch).

### `ClaudeCodeAgent::stream` — forward existing chunks + tool calls

- Move the emit point into the existing `on_receive_notification` handler:
  `AgentMessageChunk` → `sink.token(...)` (preserving `merge_chunk` dedup so we
  don't double-emit cumulative/repeat chunks — emit only the *newly added*
  delta), and `ToolCall` / `ToolCallUpdate` → `sink.tool(...)` (running → done).
- `sink.done()` on the prompt's stop reason; `sink.error(...)` on failure.

### Command + frontend

- `commands.rs`: `send_prompt_stream(state, provider, text, channel:
  Channel<StreamEvent>)` — resolves keys exactly as `send_prompt` does today,
  builds the agent, calls `agent.stream(&text, &sink)`. Replaces `send_prompt`.
- `tauriEngine.ts`: open a `Channel`, `invoke("send_prompt_stream", { provider,
  text, channel })`, push channel messages into an async queue that the
  `async *send` drains as `StageEvent`s (map `StreamEvent`→`StageEvent`; near
  identity). Stage swaps `mockEngine` → `tauriEngine`.

### Data flow

```
Anthropic SSE  ─┐                          ┌─ token → streamingText atom
Claude Code ACP ┼─ EventSink → Channel ──▶ tauriEngine (AsyncIterable)
notifications  ─┘   (Rust)      (IPC)       └─ tool  → liveToolCalls atom
                                            (useSend / applyEvent unchanged)
```

### Testing (Part A)

- Rust unit tests: `parse_sse_event` (deltas, ping, message_stop, error);
  Claude Code chunk→delta emission via `merge_chunk`; tool-call mapping.
- `tauriEngine` test with a fake `Channel` asserting `StreamEvent`s arrive as
  the right `StageEvent`s (token order preserved, tool status transitions,
  terminal `done`/`error`).
- Existing Stage tests stay green (Stage still accepts an injected engine, so
  tests keep using a mock).

## Part B — architecture

### Motion vocabulary (`@manch/ui`)

- A small set of shared keyframes/utilities (in the ui package's CSS/Tailwind
  layer) for: **spotlight** focus pulse (extend the existing `Spotlight`
  primitive), **stage reveal** (section/route entrance), **message entrance**,
  **tool-card entrance/status transition**. All use `currentColor` and are
  wrapped in `@media (prefers-reduced-motion: reduce)` no-op guards.

### `PuppetLoader` primitive

- New `packages/ui/src/primitives/PuppetLoader.tsx`: an inline SVG marionette
  (kathputli) suspended on strings, CSS-animated (gentle sway / string tug).
  Sizes via props; color via `currentColor`. Storybook story + Vitest render
  test (asserts it renders and respects a `label`/aria).
- Adopt it for: the streaming/busy indicator on the Stage, and the loading
  `EmptyState` states currently using `glyph="⏳"`.

### Per-component nuance pass (hero surfaces)

- Stage / StageHeader / Transcript / GreenRoomView / Spotlight: tune spacing
  rhythm, type scale + weight contrast, elevation via daisyUI `shadow-*` tokens,
  and add restrained string/thread ornament (semantic tokens only). No layout
  rewrites — nuance, not restructure. Each touched component keeps its story
  updated so states are eyeball-reviewable.

### Theme picker → 32

- Expand the `THEMES` array (`apps/desktop/src/store/atoms.ts`) to all 32
  daisyUI built-ins; render the `ThemePicker` as a scrollable/grouped grid so 32
  entries stay usable. Ensure the daisyUI config enables all themes.

### Testing (Part B)

- Story + Vitest render test for `PuppetLoader` and any new primitive.
- Existing component tests stay green (semantic-token changes shouldn't break
  behavior assertions).
- "Done" is subjective: **user sign-off in Storybook**. Objective gates: every
  new/changed component has a story; no hardcoded colors (semantic tokens only,
  verified by eyeballing ≥3 contrasting themes — e.g. dark, cupcake, dracula);
  `prefers-reduced-motion` respected; `just ci` green.

## Build sequence (for the plan)

1. **Part A first** (concrete, testable): DTO `StreamEvent` → `just gen`;
   `EventSink` + `ChatAgent::stream`; Anthropic SSE; Claude Code forward;
   `send_prompt_stream`; `tauriEngine` bridge; Stage swap; tests.
2. **Part B second** (iterative): motion vocabulary; `PuppetLoader` + adoption;
   hero-surface nuance pass; theme picker → 32; stories/tests; Storybook review.

Either part can merge independently; A does not depend on B or vice versa.

## Definition of done

- Real streaming: sending a prompt to Anthropic and to Claude Code renders tokens
  live and (Claude Code) live tool cards; errors surface as `error` events;
  `mockEngine` remains for tests only.
- Visual: `PuppetLoader` + motion vocabulary in place and adopted; hero surfaces
  polished; 32 themes selectable; no hardcoded colors; reduced-motion respected;
  user has signed off in Storybook.
- `just ci` green; Conventional Commits; PR references #17.

## Follow-ups (out of scope, filed separately)

- **`manch-anthropic` crate extraction** — move the inline Anthropic client into
  a published crate and design the streaming `Agent` + `EventSink` contract in
  `manch-protocol`.
- Streaming **cancellation/abort** UI + command.
