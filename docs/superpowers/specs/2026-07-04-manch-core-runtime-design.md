# manch-core: runtime — registries + prompt/tool loop (design)

**Issue:** #1 · **Depends on:** `manch-protocol` (traits exist) · **Blocks:** #5 (first milestone)

## Goal

Build `manch-core`, the framework-free, domain-free runtime crate: it holds the
Agent/Tool/Channel registries, owns the `MemoryStore`, and runs the
`prompt → assemble_context → Agent.prompt → [ToolCall → Tool.call → re-prompt] → Done`
loop by implementing `manch_protocol::PromptHandler`. It is the **seam gate** —
the one rule (no domain noun below the app layer) holds here by construction.

The desktop app already runs this loop inline (`resolve_agent` +
`send_prompt_stream` + `ChannelSink`). This crate extracts and generalizes that
into the shared spine so a CLI (#4/#5) — and later the server (#6) — reuse one
runtime instead of re-implementing it.

## Scope

**In:** the `manch-core` crate — `Manch` + `Manch::builder()`, the three
registries, the `PromptHandler` loop (BYOK host-tool dispatch **and** ACP
pass-through), error handling, and unit tests driven by in-crate mocks.

**Out (separate cycles):** the real `MemoryStore` (SQLite, #3), the CLI channel
(#4), the end-to-end milestone wiring (#5), and refactoring the desktop's inline
loop onto this crate. The builder/registry API is shaped so that later desktop
refactor is clean, but it is not done here.

## Key decisions

### Approach C — plain async orchestrator, kathputli-ready

`Manch` is a plain async struct with `Arc`-based registries; the loop is an async
method implementing `PromptHandler`. **No `kathputli` and no `katha` in
manch-core** for now:

- **katha** (event sourcing) fits **manch-memory (#3)** — core owns a
  `MemoryStore` trait object but delegates all persistence to it. Core has no
  append-only store of its own to event-source.
- **kathputli** (Tokio actors) buys per-session turn serialization and
  message-driven concurrency — real value for the **server (#6)**, not for a
  single-turn CLI milestone. Adding session actors now is YAGNI.

The turn is factored into a self-contained unit (`run_turn`) so a future
`SessionActor` (kathputli) can wrap it without a rewrite when the server needs
concurrency. Issue #1's "use kathputli/katha where they fit" is honored by the
honest reading: they do not yet fit *in core*.

### One interface, two implementations

BYOK and ACP agents are both `Arc<dyn Agent>` in one registry; the loop never
branches on which kind it holds. The BYOK-only host-tool dispatch is triggered by
the event **variant** the agent emits, not by the agent's identity:

- ACP-hosted agents forward their own tool activity as
  `AgentEvent::Update(SessionUpdate::ToolCall | ToolCallUpdate)` — core streams
  these straight through (agent-owned tools).
- BYOK agents that want a host tool emit `AgentEvent::ToolCall(tc)` — core
  intercepts it, dispatches `Tool.call`, feeds the result back, and re-prompts.

This matches the ownership table in `manch-protocol` crate docs exactly.

## Architecture

New crate `crates/manch-core` (added to `[workspace] members`), depending on
`manch-protocol` plus `tokio`, `async-trait` (workspace deps). No web framework,
no domain nouns.

```
manch-core
├── lib.rs        — Manch, Manch::builder(), PromptHandler impl (public surface)
├── builder.rs    — ManchBuilder + build() validation
├── registry.rs   — thin typed wrappers over HashMap<String, Arc<dyn _>>
├── turn.rs       — run_turn: the intercept-sink + dispatch loop (the kathputli seam)
└── tests/…       — in-crate mock Agent/Tool/MemoryStore + loop tests
```

### Builder + registries

```rust
let manch = Manch::builder()
    .agent(Arc::new(some_agent))       // keyed by agent.id()
    .tool(Arc::new(some_tool))         // keyed by tool.schema().name
    .channel(Arc::new(some_channel))   // keyed by channel.id()
    .memory(Arc::new(some_store))
    .build()?;                          // Err if no MemoryStore registered
```

- Registries are `HashMap<String, Arc<dyn _>>`. Duplicate ids: last-registered
  wins (documented) — keeps the builder chainable and predictable.
- `Manch` is `Clone` (all fields `Arc`), so a `Channel` can hold a `Manch` (or an
  `Arc<dyn PromptHandler>` view of it) and drive turns from its ingress loop.
- `build()` returns `Result<Manch>`; the only hard requirement is a `MemoryStore`
  (the loop can't assemble context without one). Empty agent/tool/channel
  registries are legal (a consumer may register lazily or use only some).

### The loop — `PromptHandler::handle`

```rust
async fn handle(
    &self,
    agent_id: &str,
    session_id: &str,
    message: Vec<ContentBlock>,
    sink: Arc<dyn EventSink>,
) -> Result<StopReason>
```

1. Resolve the agent: `agents.get(agent_id)` → `Error::NotFound(agent_id)` if absent.
2. `for block in message { memory.append(session_id, block).await?; }`
3. Loop, bounded by `MAX_TOOL_ITERS` (a small constant, e.g. 8):
   a. `let ctx = memory.assemble_context(session_id).await?;`
   b. Wrap the caller's `sink` in an internal `InterceptSink` that:
      - forwards `AgentEvent::Update(..)` to the real sink live,
      - captures `AgentEvent::ToolCall(tc)` into a per-turn `Vec` (does **not**
        forward — it is a host-side control event, not UI output),
      - swallows `AgentEvent::Done(..)` (core decides when the whole exchange is
        done, not a single sub-turn).
   c. `let stop = agent.prompt(ctx, &tool_schemas, intercept.clone()).await?;`
   d. If the turn captured no tool calls → forward one `AgentEvent::Done(stop)` to
      the real sink and `return Ok(stop)`.
   e. Otherwise, for each captured `tc`: look up the `Tool` by name
      (`Error::NotFound` if missing), `tool.call(tc.args).await`, and append the
      result (as an ACP tool-result `ContentBlock`) to memory. Then loop (re-prompt).
4. If `MAX_TOOL_ITERS` is exceeded → `Error::Other("tool-call loop exceeded N iterations")`.

`tool_schemas` is the list of every registered tool's `schema()`, passed to the
agent each turn so a BYOK model knows what host tools exist. (On the ACP path the
agent ignores it — documented in `manch-protocol`.)

### Error handling

| Condition | Result |
|-----------|--------|
| Unknown `agent_id` | `Error::NotFound` before any streaming |
| Agent `prompt` fails | propagate the agent's error; caller surfaces it |
| Tool name not registered | `Error::NotFound(tool_name)` |
| `Tool.call` fails | propagate as `Error`; stop the loop (do not silently swallow) |
| Iteration cap exceeded | `Error::Other` |

Core never sends its own `Error` event to the sink — the calling `Channel`
decides how to surface a returned `Err` (the desktop's `ChannelSink::send_error`
is the reference). Core only ever emits the pass-through `Update`s and the final
`Done`.

## Testing

In-crate mocks (behind `#[cfg(test)]`):

- **`ScriptAgent`** — constructed from a script of `AgentEvent`s to emit per
  `prompt()` call (so a test can drive: turn 1 emits text + one `ToolCall`; turn 2
  emits text + `Done`).
- **`EchoTool`** — returns a fixed `ToolCallContent`; a `FailTool` for the error path.
- **`VecStore`** — `MemoryStore` backed by a `Mutex<Vec<ContentBlock>>`;
  `assemble_context` returns the full history (the same "dumbest strategy" #3 will ship).
- **`CollectSink`** — an `EventSink` collecting emitted events for assertions.

Tests:
1. `build()` errs without a `MemoryStore`; succeeds with one.
2. Unknown `agent_id` → `Error::NotFound`, nothing emitted.
3. Text-only turn streams `Update`s through and ends with exactly one `Done`.
4. A turn emitting one `ToolCall` dispatches the tool, appends its result, and
   re-prompts; the second turn's `Done` ends the exchange. Assert the tool ran
   once and the caller saw no `ToolCall` event (only `Update` + final `Done`).
5. Unknown tool name → `Error::NotFound(tool_name)`.
6. `FailTool` → the loop stops and the error propagates.
7. An agent that emits a `ToolCall` every turn → hits `MAX_TOOL_ITERS` → `Error::Other`.

Gate: `just ci` green (new crate compiles under clippy `-D warnings`, rustfmt
clean, tests pass).

## Out of scope / follow-ups

- Real SQLite `MemoryStore` + smart context assembly — **#3**.
- CLI channel — **#4**.
- End-to-end milestone wiring against Claude Code — **#5**.
- kathputli `SessionActor` wrapping `run_turn` for per-session concurrency —
  when the server (#6) needs it.
- Refactoring the desktop's inline loop onto `manch-core` — separate follow-up.
