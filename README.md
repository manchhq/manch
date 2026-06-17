# Manch

> *Manch* (मंच) — the stage. The place where the puppets perform and the
> audience connects. An open, framework-agnostic substrate for hosting AI agents.

Manch is reusable plumbing for hosting AI agents. It exists so the products built
on top of it spend their effort on domain logic, not infrastructure. It is
**domain-free by rule**: nothing in Manch knows what a contract, a case, or a
patient is.

> **Status: design + skeleton.** Core APIs are not yet stabilised. See
> [Status & first milestone](#status--first-milestone).

---

## What Manch is (and is not)

**Is:**
- A set of Rust crates you embed as a **library** to host AI agents.
- An extension surface: implement a trait, register it, done — no forking.
- A bridge to external agents over **ACP** (Agent Client Protocol) and tools over **MCP**.
- Optionally, a thin self-hostable **server** (chat + Telegram engine) built on the same core.

**Is not:**
- A web service you must run (the server is one optional consumer, not the center).
- A model reseller or billing system.
- A product with domain knowledge (legal/health logic lives in *consumers*, never here).

### Design stance

- **Library-centric, not server-centric.** The core is headless and framework-free.
  A Tauri app, a CLI, a server, and any domain product all embed the same core.
  The application is a thin, disposable consumer.
- **Functional / DI-free leaning.** Capabilities are passed explicitly and composed,
  not injected through a runtime container. `dyn Trait` is used only at genuine
  extension boundaries (the rim), not throughout (the core). *FP core, thin polymorphic rim.*
- **Follow standards, own the ergonomics.** ACP is already an open standard (authored
  by Zed, Apache-licensed, community-governed). Manch does **not** reinvent it — it
  builds the reusable, framework-agnostic *host* that the ecosystem lacks.

---

## The one rule that keeps Manch reusable

> **Nothing below the application layer may name a domain.**
> No "legal", no "health", no "contract", no "patient".

Domain products are **consumers** that implement Manch's traits in their own
repositories and register them at startup. The test:

> *Can a consumer depend on `manch-core` without pulling in Tauri or anything
> domain-specific?* If yes, it's a library. If no, logic has leaked into the
> shell — pull it back down.

The moment a domain noun appears inside a `manch-*` crate, it has stopped being substrate.

---

## Layered architecture

```
┌─────────────────────────────────────────────────────────────┐
│  CONSUMERS  (own repos · domain-specific)                     │
│                                                               │
│   domain products    manch-server      manch-app             │
│   (own Tools)        (open, self-       (Tauri reference)     │
│                       hostable)                               │
└───────────────┬───────────────────────────────────┬──────────┘
                │  register Agents / Tools / Channels via builder()
                ▼                                     ▼
┌─────────────────────────────────────────────────────────────┐
│  manch-core   — runtime: registries + prompt/tool loop        │
│                 framework-free · domain-free · the seam gate  │
└───────┬───────────────┬────────────────┬───────────────┬─────┘
        │ implements     │                │               │
        ▼                ▼                ▼               ▼
┌───────────────┐ ┌────────────┐ ┌──────────────┐ ┌──────────────┐
│ manch-protocol│ │ manch-acp  │ │manch-channels│ │ manch-memory │
│ THE CONTRACTS │ │ ACP host   │ │ CLI/Telegram │ │ SQLite store │
│ Agent  Tool   │ │ (on official│ │ /webhook     │ │ + context    │
│ Channel       │ │  acp crate)│ │ (Channel)    │ │  assembly    │
│ MemoryStore   │ │            │ │              │ │  (the seam)  │
└───────────────┘ └────────────┘ └──────────────┘ └──────────────┘
        │                                                │
        └───────────────────┬────────────────────────────┘
                            ▼
            ┌────────────────────────────────┐
            │ kathputli  (actors over tokio)  │  ← already published,
            │ katha      (event sourcing)     │     now with a home
            └────────────────────────────────┘
```

**Dependencies flow strictly downward.** Lower layers never depend on upper layers.
`katha` and `kathputli` have zero internal dependencies.

---

## Crate responsibilities

### Foundation (already published)

| Crate | Responsibility | crates.io |
|-------|----------------|-----------|
| [`katha`](https://crates.io/crates/katha) | Event sourcing — the append-only *story* of what happened. | v0.1.1 |
| [`kathputli`](https://crates.io/crates/kathputli) | Thin actor framework over Tokio — addressable, message-driven puppets. | v0.1.1 |

### Protocol & runtime

| Crate | Responsibility |
|-------|----------------|
| `manch-protocol` | The contracts: `Agent`, `Tool`, `Channel`, `MemoryStore` traits + shared message/event types. The single source of truth for how you extend Manch. |
| `manch-core` | The runtime. Holds the agent/tool/channel registries, owns the `MemoryStore`, and runs the `prompt → agent → tool → stream` loop. **No web framework, no domain.** |

### Capabilities

| Crate | Responsibility |
|-------|----------------|
| `manch-acp` | Reusable, framework-agnostic **ACP host**. Wraps an external ACP agent (Claude Code, Gemini CLI, Codex) as a Manch `Agent`. Built on the official [`agent-client-protocol`](https://crates.io/crates/agent-client-protocol) crate for wire types. |
| `manch-channels` | Inbound/outbound surfaces behind the `Channel` trait: CLI, Telegram, webhook. Each channel is an **opt-in Cargo feature *and* a separately published crate**, so a consumer (e.g. a future domain product) can depend on just `manch-channel-telegram` and wire it in as-is — no fork, no all-or-nothing dependency. |
| `manch-memory` | Default `MemoryStore` (SQLite, local-first). **Context assembly lives here** — the hard retrieval/summarisation/compaction problem is isolated behind one method so it can be iterated without touching anything else. |

### Consumers (not in this workspace)

| Consumer | Responsibility |
|----------|----------------|
| `manch-app` | Thin Tauri reference app. One consumer among many; proves the core stands alone. |
| `manch-server` | **Optional, open, domain-free** self-hostable server: chat + Telegram engine (personal-agent surface). Secure-by-default (see [Security model](#security-model-for-manch-server)). |
| Domain products | Implement `Tool` (and friends) with their own logic, in their own repos. |

---

## The four extension points

Everything an external developer extends is a trait in `manch-protocol`.

| Trait | What it abstracts | Example impls |
|-------|-------------------|---------------|
| `Agent` | How a model/agent is invoked and streams events back. | BYOK provider (Claude/GPT/Gemini), ACP child process, local Ollama model. |
| `Tool` | What an agent can *do*. **This is where domain products plug in.** | docx generation, a workflow, an external API call. |
| `Channel` | How the outside world reaches an agent. | CLI, Telegram, webhook. |
| `MemoryStore` | How sessions persist and how context is assembled. | SQLite default; swap for Postgres or a retrieval-backed strategy. |

### Two ways to "build on it"

1. **In-process (Rust):** implement a trait, register with `Manch::builder()`.
   Typed, fast, compile-time. Best for Rust contributors and for domain products.
2. **Out-of-process (any language):** speak **ACP**; `manch-acp` hosts you. No recompile,
   no source access. This is the primary, language-agnostic "build on it" path.

---

## Choosing an agent: BYOK, or bring your own CLI

Manch follows [Zed's external-agents model](https://zed.dev/docs/ai/external-agents):
a session runs against **either** a key you bring **or** an agent CLI already on
your machine. Either way it's just a registered `Agent` — **one interface, two
implementations.** A BYOK provider and an external CLI agent both implement the
same [`Agent`] trait and both stream back ACP's own event vocabulary, so the core
and the UI never branch on which kind they're talking to. (Zed proves this works:
its native agent and its external ACP agents share one connection abstraction.)

1. **BYOK (bring your own key)** — a direct provider connection (Claude / GPT /
   Gemini / local Ollama). `manch-core` owns the `prompt → tool → re-prompt` loop
   and supplies host-registered `Tool`s. *You* hold the key; Manch never resells.
2. **BYOC (bring your own CLI)** — an external agent (Claude Code, Gemini CLI,
   Codex, …) launched as a subprocess and driven over **ACP** by `manch-acp`. The
   external agent owns its own auth, model selection, and tools; Manch is the ACP
   *client* and streams its events through. No recompile, no source access.

### The BYOK completion layer: thin hand-rolled clients, not a framework

Manch's BYOK path does **not** sit on a Rust LLM framework (e.g. `rig`). Every
serious agent host we surveyed — Zed, ZeroClaw, and AionUi's `aionrs` engine —
hand-rolls thin per-provider clients over `reqwest`, and Manch does the same. The
reason is *control*: the provider features that matter (prompt-caching betas,
OAuth / subscription auth, reasoning params, the Responses API, streaming quirks)
land in a framework — if ever — long after they ship in the raw API, and auth
schemes like Anthropic OAuth, Bedrock SigV4, or Vertex service accounts don't fit
a one-size abstraction at all.

The work is smaller than it looks, because providers cluster into a few **wire
dialects**, not N bespoke integrations:

| Wire dialect | Build cost | Covers |
|--------------|-----------|--------|
| Anthropic Messages | one client | Claude |
| OpenAI Chat / Responses | one client | OpenAI |
| OpenAI-compatible (same client, different `base_url` + key) | ~zero extra | xAI/Grok, Groq, Together, Fireworks, DeepSeek, vLLM, LiteLLM, Ollama's `/v1` |
| Gemini `generateContent` | one client | Gemini |
| Bedrock (official `aws-sdk`) | later | Claude/others on AWS |

So ~three hand-rolled clients plus one OpenAI-compatible config layer cover the
entire list. Per-provider quirks (max-tokens field name, base URL, schema
sanitisation) are **data**, not branches.

**Provider roadmap:** Anthropic → OpenAI → OpenAI-compatible (lights up
xAI / Groq / Together / Fireworks / vLLM / LiteLLM at once) → Gemini, Ollama →
Bedrock / Vertex on demand.

> `rig` is not banned. If it ever saves real work for one provider it can be
> dropped in as a single `impl` behind the same trait — never as the load-bearing
> interface.

How a CLI agent gets discovered (a superset of Zed — registry + config + detection):

| Source | How |
|--------|-----|
| **Built-in catalog** | Known agents ship with default launch commands; offered when their binary resolves. |
| **PATH detection** | If a known agent binary (e.g. `claude-code`, `gemini`, `codex`) is found on `PATH`, it's offered automatically — *this is the "whatever CLI is installed" path.* |
| **Explicit config** | A Zed-style `agent_servers` block — `{ command, args, env }` — registers any custom ACP agent the catalog doesn't know. |

```jsonc
// Zed-compatible agent config shape
{
  "agent_servers": {
    "my-agent": { "command": "node", "args": ["~/agent/index.js", "--acp"], "env": {} }
  }
}
```

---

## Core flows

**Prompt flow**

```
Channel/UI → manch-core.prompt(agent_id, session, message, sink)
           → MemoryStore.assemble_context(...)        // pluggable strategy
           → Agent.prompt(context, tool_schemas, sink)
           → [stream] AgentEvent::ToolCall
               → manch-core dispatches Tool.call(args)
               → feeds ToolResult back, re-prompts
           → AgentEvent::Done
           → events stream back out to the originating Channel/UI
```

**Extension flow**

```
A domain product NEVER edits Manch source. It:
  1. implements Tool (and/or Agent/Channel/MemoryStore) in its OWN repo
  2. registers via Manch::builder().tool(Arc::new(MyDomainTool))
  3. ships its own binary that embeds manch-core
```

That single `.tool(...)` call is the entire open-substrate / closed-moat boundary,
expressed in code.

---

## Security model (for `manch-server`)

An internet-reachable agent with shell/file tools and a Telegram command path is a
**real RCE surface**. The self-hostable server ships **secure-by-default**:

- Auth **on** by default (token-based); bound to `localhost` unless explicitly opened.
- Telegram bot tokens scoped; inbound commands authenticated to a known user.
- Shell / file-system tools **off or sandboxed** unless explicitly opted in.
- Loud README warning about the exposure footgun.
- Secrets never hardcoded; user-supplied API keys encrypted at rest, never logged.

---

## What Manch deliberately excludes

- **Model billing / gateway** — a separate project. End-users connect **directly** to
  Gemini / Claude / OpenAI with their own credentials and pay the provider. Keeping it
  out of Manch avoids the reseller-liability and account-coupling traps.
- **Domain logic** — see [The one rule](#the-one-rule-that-keeps-manch-reusable).
- **A mandatory server** — the server is optional; the library is the product.

---

## Naming family

| Name | Meaning | Role |
|------|---------|------|
| Katha (कथा) | story / narrative | event sourcing |
| Kathputli (कठपुतली) | puppet on strings | actor framework |
| Manch (मंच) | the stage / platform | the agent substrate |

Each name maps to its function. *Katha records, Kathputli performs, Manch presents.*

---

## Development

```bash
just setup     # install JS deps, generate proto bindings, install git hooks
just ci        # run everything CI runs: fmt, clippy, tests, JS lint/test/build
just test      # all tests (Rust + JS)
just --list    # see all recipes
```

Git hooks (via Lefthook) run `fmt` + `lint` on commit and `clippy` + tests on push.
`manch-protocol` is published to crates.io by release-plz on merge to `main`.
Tagging `vX.Y.Z` builds desktop installers and the server Docker image
(`ghcr.io/manchhq/manch-server`).

---

## Status & first milestone

**Status:** design + skeleton. Core APIs not yet stabilised.

**First milestone (proves the whole thesis):**
> One prompt — *"What is the capital of India?"* — answered correctly through
> **both** agent paths behind the single [`Agent`] interface:
> 1. **BYOK Anthropic**, via Manch's own hand-rolled Messages-API client (no `rig`).
> 2. **Claude Code**, launched as a subprocess and driven over **ACP**.
>
> Same question, same interface, two implementations. No streaming, no tools yet —
> just proof that the unified seam holds across a raw provider and an external CLI.

If that works, every other consumer (the server, any domain product) and every
other provider is just a different `Agent`/`Tool` registered on the same spine.
