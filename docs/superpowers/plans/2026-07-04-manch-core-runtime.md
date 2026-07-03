# manch-core Runtime Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `manch-core`, the runtime crate that holds the Agent/Tool/Channel registries, owns the `MemoryStore`, and runs the prompt/tool loop by implementing `manch_protocol::PromptHandler`.

**Architecture:** Approach C — a plain async `Manch` struct with `Arc`-based registries; the turn loop is a self-contained unit so a kathputli `SessionActor` can wrap it later. No `kathputli`/`katha` yet (katha→#3, kathputli→#6). BYOK host-tool dispatch and ACP pass-through are distinguished by the `AgentEvent` variant, not the agent's identity.

**Tech Stack:** Rust (edition 2024), `manch-protocol`, `tokio`, `async-trait`, `serde_json`. Tests are inline `#[cfg(test)]` modules driven by in-crate mocks (repo convention — see `manch-llm`/`manch-acp`).

**Spec:** `docs/superpowers/specs/2026-07-04-manch-core-runtime-design.md` · **Issue:** #1

## Global Constraints

- **Domain-free, framework-free:** no domain nouns, no web framework, no Tauri. Depends only on `manch-protocol` + `tokio` + `async-trait` + `serde_json`.
- **No parallel ACP types:** use `manch_protocol::acp::*` (re-exported `agent_client_protocol`). Never define parallel content/event enums.
- **Clippy `-D warnings` + rustfmt enforced.** Return types explicit.
- **Gate:** `just ci` green (adds `manch-core` to the workspace; `gen → fmt-check → clippy → test-rust → lint → test-js → build-js`).
- **Provisional host-tool convention** (BYOK path, unexercised by #5): the tool name is `AgentEvent::ToolCall`'s `acp::ToolCall.title`; args are `raw_input`. Documented as provisional — the protocol may grow a dedicated host-tool-call shape later.

---

### Task 1: Scaffold the crate + in-crate mock harness

**Files:**
- Create: `crates/manch-core/Cargo.toml`
- Create: `crates/manch-core/src/lib.rs`
- Create: `crates/manch-core/src/testing.rs` (the `#[cfg(test)]` mock harness)
- Modify: `Cargo.toml` (workspace `members`)

**Interfaces:**
- Produces: crate `manch-core`; `#[cfg(test)] mod testing` exposing `ScriptAgent`, `EchoTool`, `FailTool`, `VecStore`, `CollectSink` for later tasks' tests.

- [ ] **Step 1: Add the crate to the workspace**

Edit the root `Cargo.toml` `members` array to include the new crate:

```toml
members = ["crates/manch-protocol", "crates/manch-dto", "crates/manch-llm", "crates/manch-acp", "crates/manch-core", "apps/server", "apps/desktop/src-tauri"]
```

- [ ] **Step 2: Create `crates/manch-core/Cargo.toml`**

```toml
[package]
name = "manch-core"
version = "0.0.1"
edition.workspace = true
license.workspace = true
repository.workspace = true
rust-version.workspace = true
description = "Manch runtime: registries + the prompt/tool loop. Framework-free, domain-free."

[dependencies]
manch-protocol = { path = "../manch-protocol" }
async-trait.workspace = true
tokio.workspace = true
serde_json.workspace = true

[dev-dependencies]
tokio = { workspace = true, features = ["macros", "rt", "rt-multi-thread"] }
```

If `tokio` is not yet in `[workspace.dependencies]`, add `tokio = { version = "1", features = ["sync"] }` there first (check with `grep -n '^tokio' Cargo.toml`).

- [ ] **Step 3: Create `crates/manch-core/src/lib.rs` skeleton**

```rust
//! # manch-core
//!
//! The Manch runtime: Agent/Tool/Channel registries + the prompt/tool loop.
//! Framework-free, domain-free — the seam gate. Implements
//! [`manch_protocol::PromptHandler`] over registered [`manch_protocol::Agent`]s,
//! [`manch_protocol::Tool`]s, and a [`manch_protocol::MemoryStore`].

#[cfg(test)]
mod testing;
```

- [ ] **Step 4: Create `crates/manch-core/src/testing.rs` mock harness**

```rust
//! In-crate mocks for the runtime's unit tests. Not compiled outside `cfg(test)`.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use manch_protocol::acp::{ContentBlock, StopReason, TextContent, ToolCallContent, ToolKind};
use manch_protocol::{
    Agent, AgentEvent, Context, EventSink, MemoryStore, Result, Tool, ToolSchema,
};

/// An `Agent` that replays a pre-scripted list of event batches — one batch per
/// `prompt()` call. Each batch is emitted in order; the call returns `EndTurn`.
pub struct ScriptAgent {
    id: &'static str,
    turns: Mutex<std::collections::VecDeque<Vec<AgentEvent>>>,
}

impl ScriptAgent {
    pub fn new(id: &'static str, turns: Vec<Vec<AgentEvent>>) -> Self {
        Self { id, turns: Mutex::new(turns.into_iter().collect()) }
    }
}

#[async_trait]
impl Agent for ScriptAgent {
    fn id(&self) -> &str {
        self.id
    }
    async fn prompt(
        &self,
        _ctx: Context,
        _tools: &[ToolSchema],
        sink: Arc<dyn EventSink>,
    ) -> Result<StopReason> {
        let batch = self.turns.lock().unwrap().pop_front().unwrap_or_default();
        for ev in batch {
            sink.emit(ev).await?;
        }
        Ok(StopReason::EndTurn)
    }
}

/// A `Tool` that echoes a fixed text result and counts its invocations.
pub struct EchoTool {
    name: &'static str,
    pub calls: Arc<Mutex<usize>>,
}

impl EchoTool {
    pub fn new(name: &'static str) -> Self {
        Self { name, calls: Arc::new(Mutex::new(0)) }
    }
}

#[async_trait]
impl Tool for EchoTool {
    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name.to_string(),
            description: "echo".to_string(),
            kind: ToolKind::default(),
            input_schema: serde_json::json!({ "type": "object" }),
        }
    }
    async fn call(&self, _args: serde_json::Value) -> Result<ToolCallContent> {
        *self.calls.lock().unwrap() += 1;
        Ok(ToolCallContent::from(ContentBlock::Text(TextContent::new(
            "echoed".to_string(),
        ))))
    }
}

/// A `Tool` whose `call` always errors — for the failure-path test.
pub struct FailTool {
    name: &'static str,
}

impl FailTool {
    pub fn new(name: &'static str) -> Self {
        Self { name }
    }
}

#[async_trait]
impl Tool for FailTool {
    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name.to_string(),
            description: "always fails".to_string(),
            kind: ToolKind::default(),
            input_schema: serde_json::json!({ "type": "object" }),
        }
    }
    async fn call(&self, _args: serde_json::Value) -> Result<ToolCallContent> {
        Err(manch_protocol::Error::Other("boom".to_string()))
    }
}

/// A `MemoryStore` backed by an in-memory Vec. `assemble_context` returns the
/// full history — the "dumbest strategy" #3 will also ship first.
pub struct VecStore {
    blocks: Mutex<Vec<ContentBlock>>,
}

impl VecStore {
    pub fn new() -> Self {
        Self { blocks: Mutex::new(Vec::new()) }
    }
    pub fn len(&self) -> usize {
        self.blocks.lock().unwrap().len()
    }
}

#[async_trait]
impl MemoryStore for VecStore {
    async fn append(&self, _session_id: &str, block: ContentBlock) -> Result<()> {
        self.blocks.lock().unwrap().push(block);
        Ok(())
    }
    async fn assemble_context(&self, session_id: &str) -> Result<Context> {
        Ok(Context {
            session_id: session_id.to_string(),
            blocks: self.blocks.lock().unwrap().clone(),
        })
    }
}

/// An `EventSink` that records every emitted event for assertions.
#[derive(Clone, Default)]
pub struct CollectSink {
    pub events: Arc<Mutex<Vec<AgentEvent>>>,
}

impl CollectSink {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn events(&self) -> Vec<AgentEvent> {
        self.events.lock().unwrap().clone()
    }
}

#[async_trait]
impl EventSink for CollectSink {
    async fn emit(&self, event: AgentEvent) -> Result<()> {
        self.events.lock().unwrap().push(event);
        Ok(())
    }
}

#[cfg(test)]
mod smoke {
    use super::*;

    #[tokio::test]
    async fn mocks_are_usable() {
        let sink = Arc::new(CollectSink::new());
        let agent = ScriptAgent::new("m", vec![vec![AgentEvent::text_chunk("hi")]]);
        agent.prompt(
            Context { session_id: "s".into(), blocks: vec![] },
            &[],
            sink.clone(),
        )
        .await
        .unwrap();
        assert_eq!(sink.events().len(), 1);
    }
}
```

- [ ] **Step 5: Verify it compiles and the smoke test passes**

Run: `cargo test -p manch-core`
Expected: PASS (1 test: `testing::smoke::mocks_are_usable`).

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/manch-core/Cargo.toml crates/manch-core/src/lib.rs crates/manch-core/src/testing.rs
git commit -m "feat(core): scaffold manch-core crate + mock test harness (#1)"
```

---

### Task 2: Registries + `Manch::builder()`

**Files:**
- Create: `crates/manch-core/src/builder.rs`
- Modify: `crates/manch-core/src/lib.rs` (add `Manch` struct + `mod builder`)

**Interfaces:**
- Consumes: `testing::{ScriptAgent, EchoTool, VecStore}` (Task 1).
- Produces:
  - `struct Manch { agents, tools, channels, memory }` — `Clone`; fields `pub(crate)`.
  - `Manch::builder() -> ManchBuilder`.
  - `ManchBuilder::agent(self, Arc<dyn Agent>) -> Self`, `.tool(self, Arc<dyn Tool>) -> Self`, `.channel(self, Arc<dyn Channel>) -> Self`, `.memory(self, Arc<dyn MemoryStore>) -> Self`, `.build(self) -> Result<Manch>`.

- [ ] **Step 1: Write the failing tests**

Add to the bottom of `crates/manch-core/src/builder.rs` (created in Step 3, but write the test first as a sibling — here we colocate; create the file with just this test module first):

```rust
#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::Manch;
    use crate::testing::{EchoTool, ScriptAgent, VecStore};

    #[test]
    fn build_requires_a_memory_store() {
        let err = Manch::builder()
            .agent(Arc::new(ScriptAgent::new("a", vec![])))
            .build()
            .unwrap_err();
        assert!(matches!(err, manch_protocol::Error::Other(_)));
    }

    #[test]
    fn build_succeeds_and_registers_by_id() {
        let manch = Manch::builder()
            .agent(Arc::new(ScriptAgent::new("a", vec![])))
            .tool(Arc::new(EchoTool::new("echo")))
            .memory(Arc::new(VecStore::new()))
            .build()
            .unwrap();
        assert!(manch.agents.contains_key("a"));
        assert!(manch.tools.contains_key("echo"));
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p manch-core builder`
Expected: FAIL — `Manch`, `builder`, and the fields don't exist yet.

- [ ] **Step 3: Implement `Manch` + `ManchBuilder`**

Prepend to `crates/manch-core/src/builder.rs` (above the test module):

```rust
use std::collections::HashMap;
use std::sync::Arc;

use manch_protocol::{Agent, Channel, Error, MemoryStore, Result, Tool};

use crate::Manch;

/// Fluent builder for [`Manch`]. Registers agents/tools/channels by their id and
/// the sole required dependency, a [`MemoryStore`]. Duplicate ids: last wins.
#[derive(Default)]
pub struct ManchBuilder {
    agents: HashMap<String, Arc<dyn Agent>>,
    tools: HashMap<String, Arc<dyn Tool>>,
    channels: HashMap<String, Arc<dyn Channel>>,
    memory: Option<Arc<dyn MemoryStore>>,
}

impl ManchBuilder {
    pub fn agent(mut self, agent: Arc<dyn Agent>) -> Self {
        self.agents.insert(agent.id().to_string(), agent);
        self
    }
    pub fn tool(mut self, tool: Arc<dyn Tool>) -> Self {
        self.tools.insert(tool.schema().name, tool);
        self
    }
    pub fn channel(mut self, channel: Arc<dyn Channel>) -> Self {
        self.channels.insert(channel.id().to_string(), channel);
        self
    }
    pub fn memory(mut self, memory: Arc<dyn MemoryStore>) -> Self {
        self.memory = Some(memory);
        self
    }
    pub fn build(self) -> Result<Manch> {
        let memory = self
            .memory
            .ok_or_else(|| Error::Other("Manch::builder() requires a MemoryStore".to_string()))?;
        Ok(Manch {
            agents: Arc::new(self.agents),
            tools: Arc::new(self.tools),
            channels: Arc::new(self.channels),
            memory,
        })
    }
}
```

Add to `crates/manch-core/src/lib.rs` (after the `mod testing;` line):

```rust
mod builder;

use std::collections::HashMap;
use std::sync::Arc;

pub use builder::ManchBuilder;
use manch_protocol::{Agent, Channel, MemoryStore, Tool};

/// The Manch runtime. Cheap to clone (every field is `Arc`), so a `Channel` can
/// hold one and drive turns from its ingress loop.
#[derive(Clone)]
pub struct Manch {
    pub(crate) agents: Arc<HashMap<String, Arc<dyn Agent>>>,
    pub(crate) tools: Arc<HashMap<String, Arc<dyn Tool>>>,
    pub(crate) channels: Arc<HashMap<String, Arc<dyn Channel>>>,
    pub(crate) memory: Arc<dyn MemoryStore>,
}

impl Manch {
    /// Start building a runtime. A [`MemoryStore`] is required; agents, tools,
    /// and channels are optional and registered by their id.
    pub fn builder() -> ManchBuilder {
        ManchBuilder::default()
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p manch-core builder`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/manch-core/src/lib.rs crates/manch-core/src/builder.rs
git commit -m "feat(core): Manch registries + builder with MemoryStore requirement (#1)"
```

---

### Task 3: The prompt loop — pass-through (text/Update + final Done)

**Files:**
- Create: `crates/manch-core/src/turn.rs` (the `InterceptSink`)
- Modify: `crates/manch-core/src/lib.rs` (add `mod turn` + `PromptHandler` impl)

**Interfaces:**
- Consumes: `Manch` (Task 2); `testing::{ScriptAgent, VecStore, CollectSink}`.
- Produces:
  - `pub(crate) struct InterceptSink` implementing `EventSink`: forwards `Update`, captures `ToolCall`, swallows `Done`; `InterceptSink::new(Arc<dyn EventSink>)`, `take_calls(&self) -> Vec<acp::ToolCall>`.
  - `impl PromptHandler for Manch` — `handle(agent_id, session_id, message, sink) -> Result<StopReason>` (this task: no tool dispatch yet — a captured tool call is not yet acted on; that is Task 4).

- [ ] **Step 1: Write the failing tests**

Create `crates/manch-core/src/turn.rs` with only its test module for now:

```rust
#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use manch_protocol::PromptHandler;
    use manch_protocol::acp::{ContentBlock, StopReason, TextContent};
    use manch_protocol::{AgentEvent, Error};

    use crate::Manch;
    use crate::testing::{CollectSink, ScriptAgent, VecStore};

    fn user_msg(text: &str) -> Vec<ContentBlock> {
        vec![ContentBlock::Text(TextContent::new(text.to_string()))]
    }

    #[tokio::test]
    async fn unknown_agent_is_not_found() {
        let manch = Manch::builder().memory(Arc::new(VecStore::new())).build().unwrap();
        let sink = Arc::new(CollectSink::new());
        let err = manch.handle("nope", "s", user_msg("hi"), sink.clone()).await.unwrap_err();
        assert!(matches!(err, Error::NotFound(_)));
        assert!(sink.events().is_empty());
    }

    #[tokio::test]
    async fn text_turn_streams_through_and_ends_with_one_done() {
        let agent = ScriptAgent::new(
            "a",
            vec![vec![AgentEvent::text_chunk("hello"), AgentEvent::Done(StopReason::EndTurn)]],
        );
        let store = Arc::new(VecStore::new());
        let manch = Manch::builder().agent(Arc::new(agent)).memory(store.clone()).build().unwrap();
        let sink = Arc::new(CollectSink::new());

        let stop = manch.handle("a", "s", user_msg("hi"), sink.clone()).await.unwrap();

        assert!(matches!(stop, StopReason::EndTurn));
        let evs = sink.events();
        // one text Update forwarded + exactly one final Done (agent's own Done swallowed).
        let updates = evs.iter().filter(|e| matches!(e, AgentEvent::Update(_))).count();
        let dones = evs.iter().filter(|e| matches!(e, AgentEvent::Done(_))).count();
        assert_eq!(updates, 1);
        assert_eq!(dones, 1);
        assert!(matches!(evs.last(), Some(AgentEvent::Done(_))));
        // the inbound user message was appended to memory.
        assert_eq!(store.len(), 1);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p manch-core turn`
Expected: FAIL — `PromptHandler` not implemented for `Manch`; `InterceptSink` absent.

- [ ] **Step 3: Implement `InterceptSink`**

Prepend to `crates/manch-core/src/turn.rs`:

```rust
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use manch_protocol::acp::ToolCall;
use manch_protocol::{AgentEvent, EventSink, Result};

/// Wraps the caller's sink for one sub-turn: streamed `Update`s pass through
/// live, host-tool `ToolCall`s are captured for dispatch (not forwarded — they
/// are host-side control events, not UI output), and the agent's own `Done` is
/// swallowed (the runtime emits a single final `Done` for the whole exchange).
pub(crate) struct InterceptSink {
    inner: Arc<dyn EventSink>,
    captured: Mutex<Vec<ToolCall>>,
}

impl InterceptSink {
    pub(crate) fn new(inner: Arc<dyn EventSink>) -> Self {
        Self { inner, captured: Mutex::new(Vec::new()) }
    }
    /// Drain the tool calls captured during the sub-turn.
    pub(crate) fn take_calls(&self) -> Vec<ToolCall> {
        std::mem::take(&mut self.captured.lock().unwrap())
    }
}

#[async_trait]
impl EventSink for InterceptSink {
    async fn emit(&self, event: AgentEvent) -> Result<()> {
        match event {
            AgentEvent::ToolCall(tc) => {
                self.captured.lock().unwrap().push(tc);
                Ok(())
            }
            AgentEvent::Done(_) => Ok(()),
            update => self.inner.emit(update).await,
        }
    }
}
```

- [ ] **Step 4: Implement `PromptHandler` for `Manch` (pass-through only)**

Add to `crates/manch-core/src/lib.rs`:

```rust
mod turn;

use async_trait::async_trait;
use manch_protocol::acp::{ContentBlock, StopReason};
use manch_protocol::{AgentEvent, Error, EventSink, PromptHandler, Result, ToolSchema};

use turn::InterceptSink;

/// Cap on prompt→tool→re-prompt cycles, guarding against a model that loops on
/// tool calls forever.
const MAX_TOOL_ITERS: usize = 8;

#[async_trait]
impl PromptHandler for Manch {
    async fn handle(
        &self,
        agent_id: &str,
        session_id: &str,
        message: Vec<ContentBlock>,
        sink: Arc<dyn EventSink>,
    ) -> Result<StopReason> {
        let agent = self
            .agents
            .get(agent_id)
            .ok_or_else(|| Error::NotFound(agent_id.to_string()))?
            .clone();

        for block in message {
            self.memory.append(session_id, block).await?;
        }

        let schemas: Vec<ToolSchema> = self.tools.values().map(|t| t.schema()).collect();

        for _ in 0..MAX_TOOL_ITERS {
            let ctx = self.memory.assemble_context(session_id).await?;
            let intercept = Arc::new(InterceptSink::new(sink.clone()));
            let stop = agent.prompt(ctx, &schemas, intercept.clone()).await?;

            let calls = intercept.take_calls();
            if calls.is_empty() {
                sink.emit(AgentEvent::Done(stop)).await?;
                return Ok(stop);
            }

            // Task 4 fills in dispatch here; until then, a captured call would
            // loop with no progress, so treat "no dispatch implemented" as done.
            sink.emit(AgentEvent::Done(stop)).await?;
            return Ok(stop);
        }

        Err(Error::Other(format!(
            "tool-call loop exceeded {MAX_TOOL_ITERS} iterations"
        )))
    }
}
```

Note: `ContentBlock` comes from `manch_protocol::acp` (the protocol re-exports ACP vocabulary there, not at its crate root).

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p manch-core turn`
Expected: PASS (2 tests).

- [ ] **Step 6: Commit**

```bash
git add crates/manch-core/src/lib.rs crates/manch-core/src/turn.rs
git commit -m "feat(core): PromptHandler loop with streaming pass-through + InterceptSink (#1)"
```

---

### Task 4: Host-tool dispatch + re-prompt + error paths

**Files:**
- Modify: `crates/manch-core/src/lib.rs` (fill in the dispatch branch + `tool_result_block`)
- Modify: `crates/manch-core/src/turn.rs` (add dispatch tests)

**Interfaces:**
- Consumes: `Manch::handle`, `InterceptSink` (Task 3); `testing::{ScriptAgent, EchoTool, FailTool, VecStore, CollectSink}`.
- Produces: `fn tool_result_block(tc: &acp::ToolCall, result: acp::ToolCallContent) -> acp::ContentBlock` (module-private in `lib.rs`); full BYOK dispatch loop.

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `crates/manch-core/src/turn.rs`:

```rust
    use manch_protocol::acp::ToolCall;

    /// Build an `AgentEvent::ToolCall` addressed to a registered tool by name.
    fn tool_call(name: &str) -> AgentEvent {
        let mut tc = ToolCall::new(format!("call-{name}"), name.to_string());
        tc.raw_input = Some(serde_json::json!({ "x": 1 }));
        AgentEvent::ToolCall(tc)
    }

    #[tokio::test]
    async fn tool_call_is_dispatched_then_reprompted() {
        use crate::testing::EchoTool;
        let echo = EchoTool::new("echo");
        let calls = echo.calls.clone();
        // turn 1: emit a tool call. turn 2: finish with text + Done.
        let agent = ScriptAgent::new(
            "a",
            vec![
                vec![tool_call("echo")],
                vec![AgentEvent::text_chunk("done"), AgentEvent::Done(StopReason::EndTurn)],
            ],
        );
        let store = Arc::new(VecStore::new());
        let manch = Manch::builder()
            .agent(Arc::new(agent))
            .tool(Arc::new(echo))
            .memory(store.clone())
            .build()
            .unwrap();
        let sink = Arc::new(CollectSink::new());

        manch.handle("a", "s", user_msg("hi"), sink.clone()).await.unwrap();

        assert_eq!(*calls.lock().unwrap(), 1); // tool ran once
        let evs = sink.events();
        // caller never sees a raw ToolCall event; sees the turn-2 text + one Done.
        assert!(!evs.iter().any(|e| matches!(e, AgentEvent::ToolCall(_))));
        assert_eq!(evs.iter().filter(|e| matches!(e, AgentEvent::Done(_))).count(), 1);
        // memory: user msg + tool result appended.
        assert_eq!(store.len(), 2);
    }

    #[tokio::test]
    async fn unknown_tool_is_not_found() {
        let agent = ScriptAgent::new("a", vec![vec![tool_call("ghost")]]);
        let manch = Manch::builder()
            .agent(Arc::new(agent))
            .memory(Arc::new(VecStore::new()))
            .build()
            .unwrap();
        let sink = Arc::new(CollectSink::new());
        let err = manch.handle("a", "s", user_msg("hi"), sink).await.unwrap_err();
        assert!(matches!(err, Error::NotFound(name) if name == "ghost"));
    }

    #[tokio::test]
    async fn failing_tool_propagates_and_stops() {
        use crate::testing::FailTool;
        let agent = ScriptAgent::new("a", vec![vec![tool_call("boom")]]);
        let manch = Manch::builder()
            .agent(Arc::new(agent))
            .tool(Arc::new(FailTool::new("boom")))
            .memory(Arc::new(VecStore::new()))
            .build()
            .unwrap();
        let sink = Arc::new(CollectSink::new());
        let err = manch.handle("a", "s", user_msg("hi"), sink).await.unwrap_err();
        assert!(matches!(err, Error::Other(_)));
    }

    #[tokio::test]
    async fn endless_tool_calls_hit_the_iteration_cap() {
        use crate::testing::EchoTool;
        // every turn emits a tool call → never terminates on its own.
        let turns: Vec<Vec<AgentEvent>> = (0..32).map(|_| vec![tool_call("echo")]).collect();
        let manch = Manch::builder()
            .agent(Arc::new(ScriptAgent::new("a", turns)))
            .tool(Arc::new(EchoTool::new("echo")))
            .memory(Arc::new(VecStore::new()))
            .build()
            .unwrap();
        let sink = Arc::new(CollectSink::new());
        let err = manch.handle("a", "s", user_msg("hi"), sink).await.unwrap_err();
        assert!(matches!(err, Error::Other(msg) if msg.contains("exceeded")));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p manch-core turn`
Expected: FAIL — the dispatch branch is still the Task 3 stub (`tool_call_is_dispatched_then_reprompted` sees 0 tool calls; `unknown_tool`/`failing_tool`/cap don't error).

- [ ] **Step 3: Implement the dispatch branch + `tool_result_block`**

In `crates/manch-core/src/lib.rs`, replace the Task 3 stub comment block:

```rust
            // Task 4 fills in dispatch here; until then, a captured call would
            // loop with no progress, so treat "no dispatch implemented" as done.
            sink.emit(AgentEvent::Done(stop)).await?;
            return Ok(stop);
```

with the real dispatch:

```rust
            for tc in calls {
                // Provisional host-tool convention: the tool name is the ACP
                // ToolCall's `title`, args are `raw_input`. (Unexercised by #5;
                // no BYOK agent emits ToolCall yet. See the spec.)
                let tool = self
                    .tools
                    .get(&tc.title)
                    .ok_or_else(|| Error::NotFound(tc.title.clone()))?;
                let args = tc.raw_input.clone().unwrap_or(serde_json::Value::Null);
                let result = tool.call(args).await?;
                self.memory
                    .append(session_id, tool_result_block(&tc, result))
                    .await?;
            }
```

Then add the helper at the bottom of `lib.rs`:

```rust
/// Turn a tool result into a `ContentBlock` for the next turn's context. A
/// standard content result unwraps to its block; diff/terminal results (which a
/// re-prompt can't consume directly) become a short text placeholder.
fn tool_result_block(
    tc: &manch_protocol::acp::ToolCall,
    result: manch_protocol::acp::ToolCallContent,
) -> ContentBlock {
    use manch_protocol::acp::{TextContent, ToolCallContent};
    match result {
        ToolCallContent::Content(c) => c.content,
        _ => ContentBlock::Text(TextContent::new(format!(
            "[tool {} returned a non-content result]",
            tc.title
        ))),
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p manch-core turn`
Expected: PASS (6 tests total in `turn::tests`).

- [ ] **Step 5: Run the full crate tests + clippy**

Run: `cargo test -p manch-core && cargo clippy -p manch-core --all-targets -- -D warnings`
Expected: PASS, no warnings.

- [ ] **Step 6: Run the full CI gate**

Run: `just ci`
Expected: `✓ CI checks passed` (manch-core now part of the workspace build).

- [ ] **Step 7: Commit**

```bash
git add crates/manch-core/src/lib.rs crates/manch-core/src/turn.rs
git commit -m "feat(core): host-tool dispatch, re-prompt loop, and error paths (#1)"
```

---

## Self-Review

**Spec coverage:**
- Registries + `Manch::builder()` → Task 2. ✅
- Owns the `MemoryStore` (required by `build()`) → Task 2. ✅
- Implements `PromptHandler` loop (assemble→prompt→[dispatch→re-prompt]→Done) → Tasks 3–4. ✅
- BYOK host-tool dispatch only; ACP pass-through by variant → `InterceptSink` (Task 3) + dispatch branch (Task 4). ✅
- Framework-free/domain-free; deps limited to protocol/tokio/async-trait/serde_json → Task 1 Cargo.toml. ✅
- kathputli/katha deferred; turn factored (`turn.rs`) → Tasks 3–4. ✅
- Error handling table (NotFound/tool-fail/cap) → Task 4 tests + branch. ✅
- Testing via in-crate mocks → Task 1 harness, used throughout. ✅

**Placeholder scan:** none — every step has full code. The Task 3 "stub" is intentional, real, compiling code that Task 4 replaces (called out explicitly in both).

**Type consistency:** `Manch { agents, tools, channels, memory }` fields and `pub(crate)` visibility consistent across Tasks 2–4; `InterceptSink::{new, take_calls}` used as defined; `tool_result_block(&acp::ToolCall, acp::ToolCallContent) -> ContentBlock` matches its call site; `ScriptAgent::new(id, Vec<Vec<AgentEvent>>)`, `EchoTool::new/.calls`, `FailTool::new`, `VecStore::{new,len}`, `CollectSink::{new,events}` used consistently. `StopReason` is `Copy` (verified) so the emit-then-return double use is valid.

**Note (provisional):** the host-tool name/args mapping (`title`/`raw_input`) is provisional and unexercised by #5; documented in code and spec. All ACP vocabulary (`ContentBlock`, `ToolCall`, `StopReason`, …) is imported from `manch_protocol::acp`, not the protocol crate root.
