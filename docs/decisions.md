---
governs:
  paths: []
  shapes: [decisions]
  verify: source
---

# Decisions

Append new entries below existing entries. Historical rationale not present in
the cited sources is recorded as **not recorded**.

## 2026-11-26 — Preserve local planning documents

- **Context:** `git check-ignore docs/superpowers` returns `docs/superpowers`,
  and `git ls-files docs/superpowers` has no output. The repository already
  excludes local plans and specs in [.gitignore](../.gitignore).
- **Decision:** Follow the existing exclusion: keep plans and specs under
  `docs/superpowers/` local. Commit the routing baseline in `docs/`.
- **Rejected:** not recorded.

## Existing architectural rationale — recorded 2026-11-26

- **Context:** The protocol distinguishes ACP agent-owned tools from BYOK
  host-registered tools. Source:
  [protocol crate documentation](../crates/manch-protocol/src/lib.rs).
- **Decision:** Re-export ACP vocabulary, with `ToolInvocation` carrying the
  host-dispatch name, arguments and call ID within the documented BYOK
  divergence. Source: the same protocol documentation.
- **Rejected:** A parallel content/event vocabulary and treating ACP's display
  title as a dispatch name. Source: the same protocol documentation.

## 2026-11-26 — Record measured build cost without assuming concurrency safety

- **Context:** During the documentation scaffold, the user requested that build
  constraints be updated in the same commit where needed. The checkout had no
  `target/` directory, allowing a cold local-compilation measurement without
  deleting a cache. Source: the 2026-11-26 conversation and environment probes.
- **Decision:** Record the successful 97.90-second `just ci` baseline, its
  environment and its limits in [architecture.md](architecture.md), preserving
  the [GNU time output](build-baseline.txt). Leave concurrent-build safety as
  not recorded and serialize agent builds because it was not tested.
- **Rejected:** Treating one successful build or its per-process memory maximum
  as proof that two expensive builds are safe. Source: the measurement's scope
  described in [architecture.md](architecture.md).


## Host-owned provider rounds — 2026-11-26

**Decision:** Add a collector over the existing provider stream, with explicit
model policy, safe failures and ACP-native document references.

**Context:** Issue #79 requires an external host to own its tool orchestration
and preserve partial outcomes without depending on Manch's runtime. Source:
[issue #79](https://github.com/manchhq/manch/issues/79).

**Placement:** Provider collection and configuration live in
`crates/manch-llm/src/{completion,generation}.rs`; shared metadata and safe failure
categories extend `manch-protocol`. Prepared files use ACP ResourceLink. Sources:
[collector](../crates/manch-llm/src/completion.rs),
[generation policy](../crates/manch-llm/src/generation.rs),
[protocol](../crates/manch-protocol/src/lib.rs).

**Constraints applied:** ACP owns content/event vocabulary; hosts own tools,
credentials, model choice and billing policy. Usage observations retain the
latest known field rather than accumulating repeated frames. Preserve raw
provider output counts and expose visible counts separately where verifiable.
Timeouts after unknown acceptance and partial streams are never replayed.
Default transport rejects redirects and fails closed if client construction
fails. Sources:
[architecture.md](architecture.md), [HTTP policy](../crates/manch-llm/src/http.rs),
[collector](../crates/manch-llm/src/completion.rs).

**Deferred:** Grounding and schema-constrained generation remain under #79 as
its staged delivery permits. Anthropic sampling and configurable thinking remain
explicitly unsupported in this initial provider contract: current SDK types
remove sampling controls, and signed thinking-history replay needs its own
parity implementation. Consumer Rig removal still requires consumer-owned
integration tests. Sources: [capability matrix](provider-integration.md),
[official Anthropic message types](https://github.com/anthropics/anthropic-sdk-typescript/blob/main/src/resources/messages/messages.ts).


## Existing compatible reasoning API — 2026-11-26

- **Context:** Refreshing `origin/main` revealed #77's existing
  `compatible_reasoning_budget_tokens(NonZeroU32)` API after the starting checkout
  had been inspected. Source: commit `56debe3` and
  [OpenAI-compatible builder](../crates/manch-llm/src/openai.rs).
- **Decision:** Preserve that API when combining typed output policy with a
  legacy compatible budget. Reject simultaneous legacy and typed reasoning
  controls in either builder order; preserve OpenAI's exclusion of the integer
  extension. Sources: [validation](../crates/manch-llm/src/openai.rs),
  [compatibility tests](../crates/manch-llm/tests/host_integration.rs).
- **Rejected:** Silently overriding or discarding a configured reasoning budget.
  Source: the two regression fixtures, which failed before the integration fix.
