# Manch reference-app UI — design

**Date:** 2026-06-29
**Scope:** Front-end only ("just UI"). No Rust/backend changes.
**Status:** Approved design, pending implementation plan.

## Context

Manch is the *stage* (मंच) where AI agents — the *puppets* (kathputli) — perform,
presented to the audience. `manch-app` is the **thin, disposable Tauri reference
app** whose job is to prove the core stands alone; the first real flow it must
showcase is a single turn streamed through an ACP agent (Claude Code) with one
tool call dispatched.

Today the desktop UI is a functional but bare single page: conditional
`Settings` (save a provider key) → `Chat` (provider dropdown, daisyUI chat
bubbles, non-streaming send). State is local `useState`; styling is default
daisyUI 5 with no custom tokens; `@manch/ui` ships only `Button` + `VersionBadge`
and the app doesn't import them.

This design turns that into a distinctive, themed **agent workbench** — the
stage — while staying thin. It is deliberately front-end only: where the current
backend can deliver (keys, provider list, non-streaming `send_prompt`) the UI is
real; where it can't yet (token streaming, tool-call events, multi-session
history) the UI is driven by a mock behind a typed seam, so the real ACP
streaming backend drops in later without touching components.

## Goals

- A complete, themed front-end shell for the stage, with realistic mock data.
- Embody the brand: theatrical stage rendered in Rajasthani puppet-craft warmth.
- Reusable, tested, documented UI: presentation lives in `@manch/ui` with stories
  and unit tests; the app is routes + wiring only.
- A single swap point (`StageEngine`) so real streaming replaces the mock with a
  one-file change.

## Non-goals (YAGNI for now)

Real ACP streaming in Rust; multi-agent simultaneous sessions; scheduling/cron;
file editing; DB-backed conversation history; a light "lights-up" theme (the
custom theme leaves room for it). These are explicitly out of scope.

## What is real vs mocked

| Capability | Status |
|---|---|
| Save / list provider keys, pick provider | **Real** — Tauri `save_api_key`, `list_configured_providers` |
| Send a prompt, get a reply | **Real** — Tauri `send_prompt(provider, text) → String` (non-streaming) |
| Streamed tokens (typewriter), tool-call cards, agent busy status | **Mocked** via `mockEngine` behind `StageEngine` |
| Multiple conversations / sessions + history | **Mocked** in front-end store (no DB schema change) |

## Architecture: presentational vs connected

The repo-wide rule: **`@manch/ui` is pure presentation; `apps/desktop` is routes +
wiring.** A component that imports `@tauri-apps/api`, the store, or the router is
"connected" and stays in the app; everything that takes data + callbacks as props
is "presentational" and moves to the package (so Storybook/Vitest render it with
mock props, no Tauri/store mocking required).

```
packages/ui/src/                 ← all presentation, each with .stories.tsx + .test.tsx
  primitives/  Spotlight, Panel, IconRail, StatusDot, Badge, Button, VersionBadge
  stage/       GreenRoomView, StageHeader, Transcript, Message, ToolCallCard,
               Composer, PerformancePanel, SettingsForm
  theme/       manch-stage.css   (custom daisyUI 5 theme tokens)
  index.ts

apps/desktop/src/                ← routes + wiring only
  routes/      __root.tsx (3-pane shell), index.tsx
  containers/  GreenRoom, Stage, Performance, Settings  (connect store/engine → @manch/ui)
  engine/      StageEngine.ts, mockEngine.ts, tauriEngine.ts
  store/       atoms.ts (jotai; atomWithStorage for panels + mock sessions)
  data/        queries.ts (React Query hooks over Tauri commands)
```

## Layout & information architecture

One **adaptive 3-pane stage**; both side panels collapse and the choice persists.

- **Left — Green Room** (collapsible): conversation list, "+ New", and
  keys/settings entry at the bottom. Collapses to a thin icon rail.
- **Center — Stage**: the active conversation. Header has the agent/provider
  picker; transcript puts the latest agent message "in the spotlight"; composer
  at the bottom.
- **Right — Performance** (collapsible): the agent's "strings" made visible —
  live tool-call timeline, files touched, agent status. Collapses fully.

Collapse both → focused single chat. Collapse right only → 2-pane workbench.
Open all → full stage.

## Visual system (theater × kathputli)

A bold, distinctive direction; if any element reads as kitsch, fall back toward a
restrained neutral within the same token structure.

- **Canvas:** dark-first — deep stage-black washed with aubergine/indigo
  ("house lights down").
- **Spotlight:** the active message sits in a soft warm radial glow; the rest
  recedes.
- **Accents (kathputli palette):** terracotta, marigold/amber, indigo, used
  sparingly for the signature, statuses, and tool-call states
  (running = amber pulse, done = marigold ✓, error = terracotta).
- **Craft details:** subtle proscenium framing on the stage; a fine "string"
  motif tying the spotlight to the performance panel; a warm, slightly
  characterful type pairing (humanist sans for UI, warm mono for tool I/O).
- **Build:** a custom **daisyUI 5** theme (`manch-stage`) defining these tokens so
  components stay semantic (`bg-base-*`, `text-accent`, …). A tasteful fallback
  theme is one config block away.

## State management

Two kinds of state, two tools, each for its strength:

- **React Query (TanStack Query)** — the data/command layer. Every Tauri `invoke`
  (and later ConnectRPC) goes through it: `list_configured_providers` is a cached
  query; `save_api_key` and `send_prompt` are mutations with built-in
  pending/error state. Already a dependency.
- **jotai** — local UI state: active conversation id, panel collapse (via
  `atomWithStorage` for persistence), composer text, and the in-flight streaming
  transcript assembled from `StageEngine` events. Atomic subscriptions let each
  panel read only the atoms it needs (no selector boilerplate, no over-rendering).

How they compose: `send` is a React Query **mutation** whose `mutationFn` consumes
the `StageEngine` async stream and pushes events into a jotai atom as they arrive;
on `done` it commits the finished message into the conversation's message atom.
React Query owns the request lifecycle; jotai owns the accumulated UI state.

This respects the package split: jotai + React Query live in `apps/desktop`
(connected); no state library is imported by `@manch/ui` (presentation only).

Agent markdown output is rendered with **react-markdown + remark-gfm** inside the
`@manch/ui` `Message` component (a `rehype` syntax highlighter can be added later;
the first cut styles code blocks with the warm mono + theme tokens).

## Data flow & the mock seam

```
Composer.send(text)
  → append user message to the conversation's jotai atom
  → React Query mutation runs StageEngine.send(provider, text):
        mockEngine:  emits token deltas + a scripted read_file tool-call,
                     status running → done, then Done
        tauriEngine: invoke("send_prompt") → one assistant message
  → mutationFn pushes each event into the live-transcript jotai atom
  → on Done, commit the assembled message; panels re-render from their atoms
```

`StageEngine` is the single swap point. `mockEngine` drives the rich demo
(streaming typewriter + a tool-call card performing in the Performance panel);
`tauriEngine` drives the real non-streaming reply today. When ACP streaming lands
in Rust, only `tauriEngine.ts` changes.

```ts
// engine/StageEngine.ts (shape)
type StageEvent =
  | { kind: "token"; text: string }
  | { kind: "tool"; id: string; name: string; status: "running" | "done" | "error"; detail?: string }
  | { kind: "done" }
  | { kind: "error"; message: string };

interface StageEngine {
  send(provider: string, text: string): AsyncIterable<StageEvent>;
}
```

## States to handle

- **Empty (no keys):** a warm "the stage is dark" first-run that routes to Settings.
- **No conversation selected:** inviting empty stage with "+ New".
- **Sending / streaming:** spotlight active, composer disabled, agent ● busy.
- **Tool call:** running / done / error variants in transcript + Performance.
- **Send error:** terracotta inline alert.
- **Panels:** collapsed / expanded, persisted.

## Testing & docs

- Every `@manch/ui` component ships a `.stories.tsx` (Storybook 8) and a
  `.test.tsx` (Vitest + Testing Library), rendered purely from mock props.
- Stories double as the visual review surface for the theme and each state
  (e.g. ToolCallCard: running/done/error; Transcript: empty/one-turn/streaming).
- The app's containers and the `mockEngine` get focused unit tests where logic
  warrants (e.g. token accumulation, panel persistence).

## Verification

- `just lint` (tsc), `just test-js` (Vitest across `@manch/ui` + app), `just build-js`
  all green; `just ci` overall green.
- `pnpm --filter @manch/ui storybook` renders every component and every state.
- `pnpm tauri dev` (or `just`'s desktop run): the stage loads themed; with a saved
  Anthropic key a real `send_prompt` round-trips; the mock engine demonstrates a
  streamed turn with a tool-call card animating in the Performance panel;
  collapsing/expanding panels persists across reload.

## Mapping to existing code

- Reworks `apps/desktop/src/components/{Chat,Settings}.tsx` into the container +
  `@manch/ui` view split; removes the single-page conditional in favor of the
  3-pane shell in `routes/__root.tsx`.
- Keeps `apps/desktop/src/lib/api.ts` (Tauri wrappers) as the `tauriEngine`'s
  dependency.
- Promotes/extends `@manch/ui` beyond `Button` + `VersionBadge`.
- **New dependencies:** `jotai`, `react-markdown`, `remark-gfm` (React Query
  already present). `react-markdown` + `remark-gfm` are `@manch/ui` deps; `jotai`
  is an `apps/desktop` dep.
