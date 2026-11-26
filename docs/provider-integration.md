---
governs:
  paths: ["crates/manch-llm/**", "crates/manch-protocol/src/lib.rs"]
  shapes: [placement]
  verify: source
---

# Embedding the provider layer

`manch-llm` and `manch-protocol` support one provider invocation at a time.
The host owns tool execution, system prompts, authorization, saved histories,
deadlines, budget decisions and persistence. Neither the streaming call nor its
collector imports the runtime or runs host tools. Sources:
[provider manifest](../crates/manch-llm/Cargo.toml),
[collector](../crates/manch-llm/src/completion.rs),
[host example](../crates/manch-llm/examples/host_round.rs).

## Credentials, transport and deadlines

Construct a provider agent with credentials once and pass it inputs on each
round. The agent keeps its key private. `with_http_client(reqwest::Client)`
accepts a reusable connection pool; the supplied client owns its connect/read
timeouts, redirects and low-level retry policy. Configure that client before
injection. A subsequent `connect_timeout` or `read_timeout` builder creates a
new default-policy client. `max_retries` changes only Manch's retry policy and
keeps the injected pool. Sources: [HTTP policy](../crates/manch-llm/src/http.rs),
[Gemini builder](../crates/manch-llm/src/gemini.rs).

For a strict application retry limit, configure an injected client with
`.retry(reqwest::retry::never())`, as Manch's default client does. Read timeouts
detect stalls; the host sets overall deadlines by wrapping or dropping the
prompt future. Retain an `Arc<CompletionCollector>` and call `snapshot()` after
a timeout to inspect text/usage received before cancellation. Use a new
collector for each invocation. Source:
[collector and collection function](../crates/manch-llm/src/completion.rs).

Provider auth header values are sensitive. Public provider failures expose
category and HTTP status, without arbitrary response messages, URLs or error
sources. `CompletionFailure` retains partial work through its explicit `partial`
field, while its Debug/Display implementations omit that content. Applications
must still avoid logging request/history/result objects themselves. Sources:
[safe errors](../crates/manch-protocol/src/lib.rs),
[HTTP policy](../crates/manch-llm/src/http.rs),
[completion types](../crates/manch-llm/src/completion.rs).

## Generation settings and model policy

`generation_options(GenerationOptions, ModelPolicy)` configures an agent
fallibly. A policy is the host's verified knowledge for an exact provider/model
pair, not a catalog capability inferred by Manch. Optional sampling support
defaults to unknown. Explicitly requested unknown or unsupported options fail;
reasoning controls require a matching allowed kind/level in the policy.
Provider-incompatible controls are rejected even when a host marks them enabled.
Source: [generation policy](../crates/manch-llm/src/generation.rs).

Options cover output ceiling, temperature, top-p/top-k and the implemented
provider reasoning controls. Gemini budget and level are mutually exclusive
typed variants. OpenAI effort levels and Fireworks' integer budget have
different variants. Thinking budget is separate from output ceiling. Sampling
and reasoning support vary per model; the host supplies that knowledge rather
than relying on model-name guesses. Sources:
[generation types](../crates/manch-llm/src/generation.rs),
[Gemini fields](https://ai.google.dev/api/generate-content#ThinkingConfig),
[OpenAI Chat API](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create),
[Fireworks reasoning controls](https://docs.fireworks.ai/api-reference/post-chatcompletions).

An explicit model ID is preserved. Existing constructors keep the legacy
8,192-token fallback when no policy is supplied. A known policy ceiling can
lower that default, and an explicit configured cap cannot exceed the known
ceiling. The host should set a task/model-specific cap, especially for reasoning
models: a context window is input plus output, not an output allowance. A later
`max_output_tokens` override is revalidated against the retained policy before
HTTP. Sources: [generation validation](../crates/manch-llm/src/generation.rs),
[shared default](../crates/manch-llm/src/lib.rs).

The pre-existing `compatible_reasoning_budget_tokens(NonZeroU32)` helper remains
available for compatible endpoints and still has no wire effect on OpenAI.
It can be combined with typed output/sampling policy. Combining it with typed
reasoning controls is an explicit error, in either builder order, rather than a
silent precedence rule. Sources: [OpenAI-compatible builder](../crates/manch-llm/src/openai.rs),
[compatibility fixtures](../crates/manch-llm/tests/host_integration.rs).

## Completion, history and usage

`collect_completion(&agent, context, tools)` collects the same streaming call.
It returns visible text, separate thought text, ordered tool calls, ACP stop
reason, provider metadata and one final per-field usage observation. Keep the
returned `ToolInvocation` including `provider_meta`, add the host's `ToolResult`
with its matching ID, then invoke the same agent again. The example and local
fixtures demonstrate this with saved/reloaded protocol history. Sources:
[example](../crates/manch-llm/examples/host_round.rs),
[acceptance fixtures](../crates/manch-llm/tests/host_integration.rs).

Inspect `stop_reason` before using the result: `EndTurn`, `MaxTokens` and
`Refusal` are different terminal outcomes. MaxTokens may contain empty or
partial text. Missing/unknown finish status, malformed or unterminated SSE events, invalid tool
arguments, interrupted responses and trailing incomplete SSE data fail rather
than becoming normal completion. Explicit provider/model/request/response
identity observations remain optional when absent. Sources:
[stream driver](../crates/manch-llm/src/lib.rs),
[protocol metadata](../crates/manch-protocol/src/lib.rs).

Usage events are cumulative snapshots **per reported field within one
invocation**, never increments. Keep the latest known field; do not sum repeated
frames and do not erase an earlier field when a later frame omits it. Input,
provider output, reasoning, cache read/write and provider total stay independent.
Unknown is `None`, not zero. No total is invented by summing fields. OpenAI's
output count includes reasoning; `visible_output_tokens` derives the difference
only when both counts are known. Gemini's candidate output is visible output;
Anthropic's visible count remains unknown. These are provider observations for
the host to evaluate, not an accounting ledger. Sources:
[Usage](../crates/manch-protocol/src/lib.rs),
[collector](../crates/manch-llm/src/completion.rs).

Gemini thought-marked text is emitted as ACP thought chunks and never joined
to message text. Tool signatures are opaque replay data, not display content.
Native call IDs are retained; missing IDs use a per-round namespace and
stream-lifetime allocation. Duplicate tool names remain separate calls in original issue order.
Sources: [Gemini parser/history mapping](../crates/manch-llm/src/gemini.rs),
[ordered accumulation](../crates/manch-llm/src/lib.rs).

## Content and capability matrix

This matrix describes implemented adapters, **not support for every model**.
Model-specific capabilities not published or supplied by the host stay unknown.

| Representation / feature | Gemini | OpenAI | Anthropic | Fireworks |
|---|---|---|---|---|
| Text, system instructions, host tools/results | Implemented | Implemented | Implemented | Implemented |
| Inline images | Implemented; model support unknown | Implemented; model support unknown | Implemented; model support unknown | Implemented; use catalog flag when known |
| Inline PDF blobs | Implemented | Unsupported by this adapter | Implemented | Unsupported by this adapter |
| Prepared document URI (`ResourceLink`) | Implemented | Unsupported | Unsupported | Unsupported |
| Audio/video and unhandled content | Rejected | Rejected | Rejected | Rejected |
| Typed sampling | Policy-gated temperature/top-p/top-k | Policy-gated temperature/top-p; top-k rejected | Rejected in this initial integration | Policy-gated temperature/top-p/top-k |
| Typed reasoning | Policy-gated budget or level | Policy-gated effort level | Unsupported in this initial integration | Policy-gated effort or integer budget |
| Grounding and retained citation/search metadata | Deferred under #79 | Unsupported | Unsupported | Unsupported |
| Schema-constrained JSON validation | Deferred under #79 | Unsupported | Unsupported | Unsupported |

Sources: [Gemini](../crates/manch-llm/src/gemini.rs),
[OpenAI](../crates/manch-llm/src/openai.rs),
[Anthropic](../crates/manch-llm/src/anthropic.rs),
[Fireworks](../crates/manch-llm/src/fireworks.rs),
[generation validation](../crates/manch-llm/src/generation.rs).
Current Anthropic SDK types no longer expose sampling controls; signed thinking
history also needs a separate parity implementation before this adapter can
promise configurable thinking across tool rounds. Source:
[official SDK message types](https://github.com/anthropics/anthropic-sdk-typescript/blob/main/src/resources/messages/messages.ts).

Represent a prepared Gemini file using ACP
`ResourceLink::new(name, uri).mime_type(mime_string)`. The adapter emits fileData
with URI, MIME and displayName in the original content order. It does not upload,
resolve, authorize or fetch the URI. HTTPS/gs references with image/PDF MIME are
supported representations; whether a specific API endpoint/model accepts the
reference remains host/provider knowledge. System instructions and tool results
currently support text only; unsupported blocks are rejected, not discarded.
Sources: [input validation](../crates/manch-llm/src/lib.rs),
[Gemini fileData](https://ai.google.dev/api/generate-content#FileData).

## Retries and required validation

The default transport refuses redirects so provider keys are not forwarded to
a redirected endpoint. An injected client owns its redirect policy. Manch
retries explicit 429/5xx statuses and connection-establishment failures,
up to `max_retries` after the first attempt. Zero disables its retry loop.
Backoff is bounded and numeric Retry-After is capped at 60 seconds. Other HTTP
4xx, response-header timeouts, response-body failures and partial streams are
not replayed. A header timeout has unknown acceptance; the host decides how to
handle it. No retry can execute a host tool or change provider/model. Source:
[HTTP policy](../crates/manch-llm/src/http.rs).

`just ci` and the provider fixture suites make no paid calls. Live tests remain
ignored. The host example makes real calls only when explicitly run with a key
and `MANCH_MODEL`; no live run is required to accept this change. The consumer
must prove its saved-conversation/tool-checkpoint integration before removing
Rig. Grounding and structured-output specialist paths remain on its working
direct adapter until their parity fixtures land under #79. Sources:
[fixtures](../crates/manch-llm/tests/host_integration.rs),
[example](../crates/manch-llm/examples/host_round.rs),
[issue #79](https://github.com/manchhq/manch/issues/79).

## Reproducible dependency route

`manch-llm` remains unpublished. After reviewing this integration PR, copy its
full reviewed commit ID (`git rev-parse HEAD` in the reviewed checkout) into
**both** dependencies and commit the consumer's Cargo.lock:

```toml
manch-llm = { git = "https://github.com/manchhq/manch", rev = "<reviewed 40-character commit>", default-features = false, features = ["gemini"] }
manch-protocol = { git = "https://github.com/manchhq/manch", rev = "<same reviewed 40-character commit>" }
```

Do not use a moving branch or an absolute local path. `cargo build --locked`
uses the consumer's reviewed dependency resolution. The workspace declares
Rust 1.88; the provider acceptance check is
`cargo +1.88.0 check -p manch-llm --all-targets --locked`, separate from the
desktop workspace's stable-toolchain gate. Sources:
[workspace manifest](../Cargo.toml), [provider manifest](../crates/manch-llm/Cargo.toml).
