# Manch UI — multi-section workbench (design)

**Date:** 2026-06-30
**Scope:** Front-end + a thin Tauri/SQLite data layer returning seeded-but-real
data. Still "basic UI," but interactive and persistent across reload.
**Status:** Approved design, pending implementation plan.
**Builds on:** `2026-06-29-manch-ui-design.md` (the themed 3-pane stage shipped in
PR #14 on branch `feat/manch-ui-stage`). This spec extends that work — the stage
is preserved and re-homed as the Chat section.

## Context

The desktop reference app today is a single themed 3-pane "stage" (Green Room /
Stage / Performance) backed by `mockEngine`/`tauriEngine`, jotai for UI state,
and React Query over three real Tauri commands (`save_api_key`,
`list_configured_providers`, `send_prompt`). Provider keys persist in SQLite
(`apps/desktop/src-tauri/src/db.rs`). The UI hardcodes a single custom daisyUI
theme (`manch-stage`) and the first screen is a testing-oriented
"save a key / pick a provider" page.

This spec grows the app into a multi-section workbench: Chat, Teams, Schedule,
Search, and a proper Settings page — all scoped to a **Workspace**, the new
top-level context. It also makes the UI **theme-agnostic** (any daisyUI theme,
dark by default) and replaces front-end-only mocks for the new sections with
**real Tauri commands returning seeded data**, persisted in the existing SQLite
store.

## Goals

- A workspace-scoped, multi-section app shell (Chat / Teams / Schedule / Search /
  Settings) with a workspace switcher.
- Theme-agnostic components: drop the hardcoded custom theme; user-selectable
  daisyUI themes with `dark` as the default; selection persists.
- Interactive, persistent fake data via real Tauri commands over SQLite (same
  pattern as `provider_keys`), wrapped in React Query; forms via TanStack Form.
- The "AI Teams" concept: user defines a problem and an AI proposes a team
  (auto-compose), or the user hand-builds a team; teams carry a tools/capabilities
  list and can be assigned a task that produces a mock run.
- Multi-AI cross-verification: select one or more configured AIs and get multiple
  reports plus a synthesized comparison.
- Settings replaces the testing key-screen: providers, theme, workspaces.
- Gating: with no provider configured, AI selection for Chat and Teams is disabled
  with a nudge to Settings.
- Keep the package boundary: presentation in `@manch/ui` (story + test each),
  routes + wiring in `apps/desktop`.

## Non-goals (YAGNI for now)

Real ACP streaming in Rust (still mocked behind `StageEngine`); real agent
execution for team runs / cross-verify (mock/computed responses); real
scheduling/cron execution (schedules are stored and listed, not fired); auth,
billing, collaboration; light/dark auto-switching beyond the daisyUI theme
picker. The cross-verify and team-run commands return computed mock data; only
workspaces, teams, and schedules are persisted as CRUD entities.

## Decisions (validated with the user)

1. **Workspace = top-level context.** Chat/Teams/Schedule/Search live inside the
   active workspace (Slack/Notion model), selected from a top-bar switcher.
2. **Real Tauri commands returning fake data**, persisted in the existing SQLite
   `Db`. Not a TS-only mock layer.
3. **Drop the custom `manch-stage` theme**; use daisyUI built-in themes with
   `dark` as the default and a Settings theme picker.
4. **Generate TS types from Rust DTOs with [`ts-rs`](https://github.com/Aleph-Alpha/ts-rs)** —
   no hand-mirrored types. DTOs live in a new **publishable** crate
   `crates/manch-dto` (alongside `manch-protocol`); a generator combines them into
   a single `bindings.ts` consumed by the client.

## Information architecture

```
┌──────────────────────────────────────────────┐
│ [Workspace ▾]   Manch                 (theme) │   top bar
├──┬───────────────────────────────────────────┤
│💬│                                            │   icon rail (left):
│👥│   active section content,                  │   Chat / Teams /
│📅│   scoped to the active workspace           │   Schedule / Search /
│🔍│                                            │   Settings
│⚙ │                                            │
└──┴───────────────────────────────────────────┘
```

- **Top bar:** `WorkspaceSwitcher` (list / switch / + New) + app title + theme
  indicator.
- **Left icon rail:** Chat · Teams · Schedule · Search · Settings.
- **Routing (TanStack Router, file-based):** `/chat`, `/teams`, `/teams/$teamId`,
  `/schedule`, `/search`, `/settings`; `/` redirects to `/chat`. The active
  workspace is a persisted jotai atom (not a route param) — section routes read
  it to scope their queries.

## Sections

### Chat (`/chat`)
The existing 3-pane stage, re-homed. Adds **Compare mode**: the stage header gains
a multi-select of *configured* AIs. With one selected it behaves as today; with
more than one, `send` fans out and the Stage renders N response columns plus a
synthesized **cross-verification** card (`cross_verify` command). Send is disabled
with a "configure a provider in Settings" nudge when no provider is configured.

### Teams (`/teams`, `/teams/$teamId`)
- **List:** team cards for the active workspace (name, member count, capabilities).
- **Create — two paths:**
  - **Auto-compose:** user types a problem statement → `create_team` with an
    `auto` flag returns a proposed team (roles + an assigned AI per role, drawn
    from configured providers). Presented for confirmation/edit before save.
  - **Manual:** name the team, add members (role + pick a configured AI).
- **Detail:** member list, a tools/capabilities list (mock "access to all tools
  and functions"), and an **assign-task** action → `assign_team_task` returns a
  mock run (timeline of per-member steps + a result).
- AI selection (both paths) is disabled when no provider is configured.

### Schedule (`/schedule`)
Agenda-style list of scheduled items for the workspace + a create form
(TanStack Form): target (a team or a chat), cadence (`once` / `daily` / `weekly`),
next-run timestamp. Items are stored and listed; they are **not** fired.

### Search (`/search`)
A query box + typed results across the active workspace's conversations, teams,
and schedules via a `search(workspace_id, query, kinds)` command. Result rows link
to the relevant section; filter chips select kinds.

### Settings (`/settings`) — replaces the testing key-screen
Three sections in one page:
- **Providers:** real `list_configured_providers` (cached query) + `save_api_key`
  (mutation); list configured providers, add a key, remove a key.
- **Theme:** `ThemePicker` over the configured daisyUI themes (dark default).
- **Workspaces:** rename / delete workspaces (create lives in the switcher).

## Theme (theme-agnostic)

- Remove `packages/ui/src/theme/manch-stage.css` and the hardcoded
  `data-theme="manch-stage"` wrapper.
- Configure daisyUI (Tailwind v4 `@plugin "daisyui"`) with a curated built-in set
  — `dark` (default), `light`, `dracula`, `nord`, `cupcake` (final list in the
  plan) — `dark` marked default.
- A `themeAtom` (`atomWithStorage`, default `"dark"`) drives
  `document.documentElement.setAttribute("data-theme", …)` via an effect in the
  shell.
- Audit every `@manch/ui` component to use **only** semantic daisyUI tokens
  (`bg-base-*`, `text-base-content`, `text-primary`, `border-base-300`, …). The
  `Spotlight` radial-gradient must reference a semantic token so it renders under
  every theme. No hardcoded hex/oklch in components.

## Data layer (real Tauri commands, persisted fake data)

Extend the existing SQLite `Db` with tables + CRUD, mirroring the `provider_keys`
pattern. Seed a default workspace (+ one sample team and schedule) on first init
when the tables are empty.

Commands (all `Result<T, String>`, registered in `lib.rs`):

| Command | Shape |
|---|---|
| `list_workspaces()` | `Vec<Workspace>` |
| `create_workspace(name, description)` | `Workspace` |
| `rename_workspace(id, name)` | `Workspace` |
| `delete_workspace(id)` | `()` |
| `list_teams(workspace_id)` | `Vec<Team>` |
| `create_team(input)` | `Team` — `input` carries `auto` + problem, or manual members |
| `get_team(id)` | `Team` |
| `assign_team_task(team_id, task)` | `TeamRun` (mock timeline + result) |
| `list_schedules(workspace_id)` | `Vec<Schedule>` |
| `create_schedule(input)` | `Schedule` |
| `search(workspace_id, query, kinds)` | `Vec<SearchHit>` |
| `cross_verify(providers, text)` | `CrossVerify` (`Vec<Report>` + `summary`) |

Persisted CRUD: workspaces, teams, schedules. Computed/mock (not stored):
`assign_team_task`, `search` (queries stored entities), `cross_verify`. Existing
`save_api_key` / `list_configured_providers` / `send_prompt` are unchanged.

### DTO crate + type generation (`ts-rs`)

All command request/response shapes (`Workspace`, `Team`, `TeamMember`,
`TeamRun`, `Schedule`, `SearchHit`, `CrossVerify`, `Report`, input structs, …)
are defined **once** in a new crate `crates/manch-dto`:

- Each struct derives `serde::{Serialize, Deserialize}` always, and `ts_rs::TS`
  **conditionally** — `#[cfg_attr(feature = "ts", derive(TS))]`. (No `ts(export,
  export_to)` attribute is needed: the generator calls `export_to_string`
  directly rather than the cargo-test auto-export.) The crate is publishable (see
  Versioning), so `ts-rs` is an **optional** dependency behind a `ts` feature; the
  default build is lean serde-only.
- A generator binary `src/bin/gen-types.rs` (`required-features = ["ts"]`) calls
  `TS::export_to_string` for every DTO, strips per-type `import` lines (all types
  land in one file), concatenates them under a "DO NOT EDIT — generated" header,
  and writes a single combined file to `apps/desktop/src/data/bindings.ts`.
- `src-tauri` depends on `manch-dto` (default features) and uses these structs
  directly as command parameter/return types — so the wire contract has one
  source of truth.

`just gen` is extended to run `cargo run -p manch-dto --features ts --bin
gen-types` (after the proto step). The generated `bindings.ts` is **gitignored
and regenerated** (mirrors `@manch/api/src/gen/`); CI's `gen` step (which runs
before `lint`) produces it. The client imports DTO types from `bindings.ts`.

React Query wraps every `invoke`; TanStack Form drives create/edit forms.

DB notes: keep the single `Mutex<Connection>`; never hold the lock across an
`await`. New tables use text primary keys (uuid-like ids generated in Rust). All
new DB methods get unit tests like the existing `db.rs` tests
(`#[cfg(test)]` + `open_in_memory`).

## State management

- **React Query** — every Tauri `invoke` (reads cached; writes are mutations that
  invalidate the relevant query keys).
- **jotai** — UI state: `activeWorkspaceIdAtom` (`atomWithStorage`), `themeAtom`
  (`atomWithStorage`), panel-collapse atoms (existing), the in-flight streaming
  transcript atoms (existing), and a `compareProvidersAtom` (selected provider ids
  for Chat compare mode).
- **TanStack Form** — local form state for create/edit forms; may live inside
  `@manch/ui` form components since it imports no Tauri/jotai/router. The form
  surfaces submission as an `onSubmit(values)` callback so containers own the
  Tauri call and stories/tests render the form with a mock `onSubmit`.

## Package boundary (standing rule)

- **`@manch/ui`** — pure presentation, each component with `.stories.tsx` +
  `.test.tsx`, rendered from mock props. New components: `WorkspaceSwitcher`,
  `NavRail`, `TeamCard`, `TeamList`, `TeamComposer` (create form view, both
  paths), `TeamDetail`/`MemberList`, `ScheduleList`/`ScheduleItem`/`ScheduleForm`,
  `SearchBar`/`SearchResults`, `SettingsView` + `ProviderSettings` + `ThemePicker`
  + `WorkspaceSettings`, `CompareView`, `EmptyState`, and a `Dialog` primitive.
  No imports of `@tauri-apps/api`, the store, or the router.
- **`apps/desktop`** — routes + containers that wire store/queries/engine into the
  `@manch/ui` views; the React Query hooks; the TanStack Form orchestration via
  `onSubmit` callbacks. Imports the generated `data/bindings.ts` (the DTO types)
  for command shapes.
- **`crates/manch-dto`** — the DTO structs (one source of truth for command
  shapes) + the `gen-types` binary. `src-tauri` depends on it for command
  signatures.

`@manch/ui` keeps its own presentational prop types (it must build standalone and
not depend on app-generated bindings); those types structurally match the DTOs.
Only the `apps/desktop` data layer imports `bindings.ts`.

## States to handle

- **No workspace yet:** seeded default means this is rare, but handle an empty
  `list_workspaces` with a first-run "create a workspace" prompt.
- **No provider configured:** Chat send + Team AI selection disabled with a nudge
  routing to Settings.
- **Section empty:** `EmptyState` for empty Teams / Schedule / Search results.
- **Loading / error:** React Query pending + error states surfaced (skeleton or
  spinner; terracotta-equivalent semantic `alert-error`).
- **Compare mode:** 1 AI = normal; >1 = columns + cross-verify card.
- **Panels:** collapse/expand persisted (existing).

## Testing & docs

- Every `@manch/ui` component ships a Storybook story and a Vitest + Testing
  Library test, from mock props, covering its key states.
- App containers, React Query hooks, and the jotai atoms get focused unit tests
  where logic warrants (workspace scoping, compare-mode fan-out, theme persistence).
- New Rust DB methods + commands get `#[cfg(test)]` unit tests over an in-memory DB.

## Verification

- `just ci` green (`gen → fmt-check → clippy → test-rust → lint → test-js →
  build-js`), including the new Rust commands (clippy `-D warnings`, rustfmt).
- `pnpm --filter @manch/ui storybook` renders every new component and state.
- `pnpm --filter @manch/desktop dev` (browser) and `pnpm desktop:dev` (Tauri):
  switch workspaces; create a team both ways; create a schedule; search; switch
  themes (persists across reload); Chat compare mode shows multiple reports + a
  cross-verification card; with no key, AI selection is disabled.

## Build sequence (phased; subagent-driven)

0. Theme-agnostic refactor + app shell/nav rail + workspace switcher + workspace
   context atom + routing skeleton.
1. Settings page (providers + theme picker + workspaces management).
2. `crates/manch-dto` (DTO structs + `ts` feature + `gen-types` bin) + `just gen`
   wiring → Rust data layer in `src-tauri` (tables, CRUD, seed, commands using the
   DTOs) → generated `bindings.ts` + React Query hooks.
3. Teams (list, both create paths, detail, assign-task run).
4. Schedule (list + create form).
5. Search (query + typed results).
6. Chat compare mode + cross-verification.
7. Gating (no-provider / empty states) + first-run + final polish.

Each phase: `@manch/ui` components (test + story) first, then container wiring;
`just ci` green at every phase boundary so the user can stop and review.

## Mapping to existing code

- `apps/desktop/src/routes/__root.tsx` → app shell (top bar + nav rail + Outlet +
  theme effect); `index.tsx` → redirect to `/chat`; new section routes added.
- Existing stage containers/components move under the Chat route unchanged in
  behavior (header gains compare multi-select).
- `packages/ui/src/theme/manch-stage.css` removed; `styles.css` / daisyUI config
  updated for built-in themes.
- `apps/desktop/src/lib/providers.ts` (`ALL_PROVIDERS`) and `lib/api.ts`
  (`PROVIDERS`) unified into one source (resolves a PR #14 follow-up).
- `apps/desktop/src-tauri/src/db.rs` extended; `commands.rs` + `lib.rs` gain the
  new commands (using `manch-dto` structs).
- **New crate** `crates/manch-dto` (publishable) + a row in the AGENTS.md /
  README repository map.
- `Justfile` `gen` recipe extended (proto → dto types); `.gitignore` gains
  `apps/desktop/src/data/bindings.ts`.
- **New dependencies:** `ts-rs` (optional, `crates/manch-dto`, behind `ts`
  feature); `@tanstack/react-form` (app, and/or `@manch/ui` for form components).
  React Query, jotai, react-markdown, remark-gfm already present.

## Versioning & releases

`manch-dto` is `publish = true` and versioned independently by `release-plz` (same
as `manch-protocol`) — it needs the publish metadata (`description`, `license`,
`repository`) in its `Cargo.toml`. Don't hand-bump its version; Conventional
Commit subjects drive it. `ts-rs` being optional keeps the published crate lean
for downstream consumers who only want the serde DTOs.
