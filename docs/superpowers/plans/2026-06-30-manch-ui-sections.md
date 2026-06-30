# Manch UI Sections Implementation Plan (Plan 2)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the feature sections on top of the foundation (Plan 1): Teams (both create paths + a mock run), Schedule, Search, Chat multi-AI compare/cross-verify, and the no-provider gating + first-run polish.

**Architecture:** Same split — presentation in `@manch/ui` (story + test each), routes/containers/store/queries in `apps/desktop`. The Rust data layer + DTOs + React Query hooks already exist (Plan 1); this plan consumes them. Each section route (placeholder `EmptyState` today) is replaced by a real container wiring the hooks to new `@manch/ui` views, scoped to `activeWorkspaceIdAtom`.

**Tech Stack:** React 19, TanStack Router (file-based), TanStack Query, TanStack Form, jotai, Tailwind 4 + daisyUI 5 (built-in themes), Vitest + Testing Library, Storybook 8.

## Global Constraints

- **Package boundary:** `@manch/ui` is pure presentation — no imports of `@tauri-apps/api`, jotai, React Query, or the router (TanStack Form IS allowed). Every `@manch/ui` component ships a `.test.tsx` (Vitest + Testing Library) and a `.stories.tsx`, rendered from mock props, and is exported from `packages/ui/src/index.ts`. App wiring lives only in `apps/desktop`.
- **Theme-agnostic:** components use ONLY semantic daisyUI tokens (`bg-base-*`, `text-base-content`, `text-primary`, `bg-warning`, `text-error`, `badge-*`, `alert-*`, …). No hardcoded hex/oklch.
- **DTO types come from the generated bindings** (`apps/desktop/src/data/bindings.ts`, gitignored, produced by `just gen`); never hand-write a TS type mirroring a DTO. `@manch/ui` keeps its own presentational prop types (structurally matching).
- **Workspace scoping:** every section reads `activeWorkspaceIdAtom` and passes it to its hooks; queries are `enabled` only when a workspace is active.
- **Gating:** AI-dependent actions (Chat send, Team AI member selection, Compare) are disabled when `useConfiguredProviders()` returns empty, with a nudge linking to `/settings`.
- **Verification per task:** the scoped test command passes; at each phase boundary `just ci` is green.
- **Conventional Commits.** Branch: continue on `feat/manch-ui-stage`.

## What already exists (Plan 1 — do not rebuild)

- **`@manch/ui`:** `NavRail`, `EmptyState`, `WorkspaceSwitcher`, `ThemePicker`, `ProviderSettings`, `WorkspaceSettings`, `SettingsView`, plus the stage set (`Message`, `ToolCallCard`, `Transcript`, `Composer`, `StageHeader`, `GreenRoomView`, `PerformancePanel`, `SettingsForm`, `Spotlight`, `Panel`, `IconRail`, `StatusDot`, `Badge`).
- **Routes:** `__root.tsx` shell (top bar + `NavRail` + theme effect + Outlet), `/chat` (the 3-pane stage), `/teams` `/schedule` `/search` (EmptyState placeholders — REPLACE these), `/settings` (built).
- **Store:** `activeWorkspaceIdAtom`, `themeAtom`/`THEMES`, the stage atoms (`conversationsAtom`, `activeIdAtom`, `leftCollapsedAtom`, `rightCollapsedAtom`, streaming atoms), `newConversation`.
- **Data layer hooks (`apps/desktop/src/data/queries.ts`):** `useConfiguredProviders`, `useSaveApiKey`, `useWorkspaces`, `useCreateWorkspace`, `useRenameWorkspace`, `useDeleteWorkspace`, `useTeams(workspaceId)`, `useCreateTeam`, `useSchedules(workspaceId)`, `useCreateSchedule`, `useSearch(workspaceId, query, kinds)`, `useCrossVerify`.
- **api.ts wrappers (all present):** incl. `getTeam(id)`, `assignTeamTask(teamId, task)`, `crossVerify(providers, text)`.
- **DTOs (generated `bindings.ts`):** `Team {id, workspace_id, name, problem, members: Array<TeamMember>, capabilities: Array<string>}`, `TeamMember {role, provider}`, `CreateTeam {workspace_id, name, problem, auto, members: Array<TeamMember>}`, `TeamRun {team_id, task, steps: Array<RunStep>, result}`, `RunStep {member_role, detail, status}`, `Schedule {id, workspace_id, target, cadence, next_run}`, `CreateSchedule {workspace_id, target, cadence, next_run}`, `SearchHit {kind, id, title, snippet}`, `Report {provider, text}`, `CrossVerify {reports: Array<Report>, summary}`.
- **`lib/providers.ts`:** `PROVIDERS`, `Provider` union, `ALL_PROVIDERS: ProviderOption[]`, `isProvider(x)` guard.
- **Missing hooks this plan ADDS:** `useTeam(id)`, `useAssignTeamTask()` (api wrappers already exist).

## File Structure

**New in `@manch/ui` (`packages/ui/src/`):**
- `teams/TeamCard.tsx`, `teams/TeamList.tsx`, `teams/TeamComposer.tsx`, `teams/TeamDetail.tsx`
- `schedule/ScheduleList.tsx`, `schedule/ScheduleForm.tsx`
- `search/SearchBar.tsx`, `search/SearchResults.tsx`
- `stage/CompareView.tsx`
- each with sibling `.test.tsx` + `.stories.tsx`; all exported from `index.ts`.

**New/modified in `apps/desktop/src/`:**
- `data/queries.ts` — add `useTeam`, `useAssignTeamTask`.
- `store/atoms.ts` — add `compareProvidersAtom`.
- `containers/Teams.tsx`, `containers/TeamDetailPage.tsx`, `containers/SchedulePage.tsx`, `containers/SearchPage.tsx`.
- `routes/teams.tsx` (replace placeholder), `routes/teams.$teamId.tsx` (new), `routes/schedule.tsx`, `routes/search.tsx` (replace placeholders).
- `containers/Stage.tsx` — compare-mode fan-out; `containers/GreenRoom.tsx` / stage header — provider multi-select + gating.

---

## Phase D — Teams

### Task 1: `TeamCard` + `TeamList`

**Files:** Create `packages/ui/src/teams/TeamCard.tsx`, `TeamList.tsx`, each `.test.tsx` + `.stories.tsx`; modify `index.ts`.

**Interfaces:**
```ts
export interface TeamSummary { id: string; name: string; problem: string; memberCount: number }
export interface TeamCardProps { team: TeamSummary; onOpen: (id: string) => void }
export function TeamCard(props: TeamCardProps): JSX.Element

export interface TeamListProps {
  teams: TeamSummary[];
  onOpen: (id: string) => void;
  onNew: () => void;
}
export function TeamList(props: TeamListProps): JSX.Element
```

- [ ] **Step 1: Write the failing `TeamCard` test**

```tsx
// TeamCard.test.tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { TeamCard } from "./TeamCard";

const team = { id: "tm_1", name: "Discovery", problem: "find precedent", memberCount: 3 };

describe("TeamCard", () => {
  it("shows name, problem, and member count", () => {
    render(<TeamCard team={team} onOpen={() => {}} />);
    expect(screen.getByText("Discovery")).toBeTruthy();
    expect(screen.getByText(/find precedent/)).toBeTruthy();
    expect(screen.getByText(/3/)).toBeTruthy();
  });
  it("opens on click", () => {
    const onOpen = vi.fn();
    render(<TeamCard team={team} onOpen={onOpen} />);
    fireEvent.click(screen.getByRole("button", { name: /Discovery/ }));
    expect(onOpen).toHaveBeenCalledWith("tm_1");
  });
});
```

- [ ] **Step 2: Run to verify fail** — `pnpm --filter @manch/ui exec vitest run src/teams/TeamCard.test.tsx` → FAIL (module missing).

- [ ] **Step 3: Implement `TeamCard`**

```tsx
// TeamCard.tsx
export interface TeamSummary {
  id: string;
  name: string;
  problem: string;
  memberCount: number;
}

export interface TeamCardProps {
  team: TeamSummary;
  onOpen: (id: string) => void;
}

export function TeamCard({ team, onOpen }: TeamCardProps): JSX.Element {
  return (
    <button
      onClick={() => onOpen(team.id)}
      aria-label={team.name}
      className="card w-full bg-base-200 p-4 text-left transition hover:bg-base-300"
    >
      <span className="text-base font-semibold text-base-content">{team.name}</span>
      {team.problem && <span className="mt-1 line-clamp-2 text-sm text-base-content/70">{team.problem}</span>}
      <span className="mt-2 text-xs text-base-content/50">{team.memberCount} member{team.memberCount === 1 ? "" : "s"}</span>
    </button>
  );
}
```

- [ ] **Step 4: Run to verify pass** — PASS.

- [ ] **Step 5: Write the failing `TeamList` test**

```tsx
// TeamList.test.tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { TeamList } from "./TeamList";

const teams = [
  { id: "tm_1", name: "Discovery", problem: "p1", memberCount: 2 },
  { id: "tm_2", name: "Drafting", problem: "p2", memberCount: 1 },
];

describe("TeamList", () => {
  it("renders all teams and fires onNew", () => {
    const onNew = vi.fn();
    render(<TeamList teams={teams} onOpen={() => {}} onNew={onNew} />);
    expect(screen.getByText("Discovery")).toBeTruthy();
    expect(screen.getByText("Drafting")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /New team/ }));
    expect(onNew).toHaveBeenCalledOnce();
  });
});
```

- [ ] **Step 6: Run to verify fail**, then implement `TeamList`:

```tsx
// TeamList.tsx
import { TeamCard, type TeamSummary } from "./TeamCard";

export interface TeamListProps {
  teams: TeamSummary[];
  onOpen: (id: string) => void;
  onNew: () => void;
}

export function TeamList({ teams, onOpen, onNew }: TeamListProps): JSX.Element {
  return (
    <div className="flex h-full flex-col gap-4 p-6">
      <div className="flex items-center justify-between">
        <h1 className="text-xl font-semibold">Teams</h1>
        <button className="btn btn-primary btn-sm" onClick={onNew}>＋ New team</button>
      </div>
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
        {teams.map((t) => <TeamCard key={t.id} team={t} onOpen={onOpen} />)}
      </div>
    </div>
  );
}
```

- [ ] **Step 7: Run to verify pass** — PASS.

- [ ] **Step 8: Stories + exports**

Stories: `TeamCard.stories.tsx` (one card) and `TeamList.stories.tsx` (`Default` with 3 teams, `Empty` with `[]`). Add to `index.ts`:
```ts
export { TeamCard } from "./teams/TeamCard";
export type { TeamCardProps, TeamSummary } from "./teams/TeamCard";
export { TeamList } from "./teams/TeamList";
export type { TeamListProps } from "./teams/TeamList";
```

- [ ] **Step 9: Run full UI suite + lint, commit**

```bash
pnpm --filter @manch/ui test && pnpm --filter @manch/ui lint
git add packages/ui/src/teams packages/ui/src/index.ts
git commit -m "feat(ui): TeamCard + TeamList"
```

---

### Task 2: `TeamComposer` (both create paths, gated AI selection)

**Files:** Create `packages/ui/src/teams/TeamComposer.tsx`, `.test.tsx`, `.stories.tsx`; modify `index.ts`.

**Interfaces:**
```ts
export interface ComposerMember { role: string; provider: string }
export interface TeamComposerValue {
  name: string;
  problem: string;
  auto: boolean;
  members: ComposerMember[];
}
export interface TeamComposerProps {
  providers: { id: string; label: string }[]; // configured providers; empty ⇒ AI selection disabled
  onCreate: (value: TeamComposerValue) => void;
  onConfigureProviders?: () => void; // nudge when no providers
  creating?: boolean;
}
export function TeamComposer(props: TeamComposerProps): JSX.Element
```

Behavior: a mode toggle — **Auto** (a problem textarea; members synthesized server-side, so `members: []`, `auto: true`) and **Manual** (name + add/remove member rows: role text + provider select). Submitting calls `onCreate`. When `providers` is empty: Manual member provider selects are disabled and a nudge ("Add an AI provider in Settings", calling `onConfigureProviders`) shows; Auto mode is still allowed to submit (the backend synthesizes members) but also shows the nudge.

- [ ] **Step 1: Write the failing test**

```tsx
// TeamComposer.test.tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { TeamComposer } from "./TeamComposer";

const providers = [{ id: "anthropic", label: "Anthropic" }];

describe("TeamComposer", () => {
  it("submits an auto-compose team from a problem statement", () => {
    const onCreate = vi.fn();
    render(<TeamComposer providers={providers} onCreate={onCreate} />);
    fireEvent.change(screen.getByLabelText(/problem/i), { target: { value: "find precedent" } });
    fireEvent.click(screen.getByRole("button", { name: /create team/i }));
    expect(onCreate).toHaveBeenCalledWith(expect.objectContaining({ problem: "find precedent", auto: true }));
  });

  it("nudges to settings and disables provider selection when no providers", () => {
    const onConfigureProviders = vi.fn();
    render(<TeamComposer providers={[]} onCreate={() => {}} onConfigureProviders={onConfigureProviders} />);
    fireEvent.click(screen.getByRole("button", { name: /add an ai provider/i }));
    expect(onConfigureProviders).toHaveBeenCalledOnce();
  });
});
```

- [ ] **Step 2: Run to verify fail** — FAIL.

- [ ] **Step 3: Implement `TeamComposer`** (controlled; `useState` for mode/members — TanStack Form optional, controlled is simpler here)

```tsx
// TeamComposer.tsx
import { useState } from "react";

export interface ComposerMember {
  role: string;
  provider: string;
}
export interface TeamComposerValue {
  name: string;
  problem: string;
  auto: boolean;
  members: ComposerMember[];
}
export interface TeamComposerProps {
  providers: { id: string; label: string }[];
  onCreate: (value: TeamComposerValue) => void;
  onConfigureProviders?: () => void;
  creating?: boolean;
}

export function TeamComposer({ providers, onCreate, onConfigureProviders, creating }: TeamComposerProps): JSX.Element {
  const [auto, setAuto] = useState(true);
  const [name, setName] = useState("");
  const [problem, setProblem] = useState("");
  const [members, setMembers] = useState<ComposerMember[]>([]);
  const noProviders = providers.length === 0;

  const submit = () => onCreate({ name: auto ? name || "New team" : name, problem, auto, members: auto ? [] : members });

  return (
    <form className="space-y-4 p-6" onSubmit={(e) => { e.preventDefault(); submit(); }}>
      <h1 className="text-xl font-semibold">New team</h1>

      <div role="tablist" className="tabs tabs-boxed w-fit">
        <button type="button" role="tab" className={`tab ${auto ? "tab-active" : ""}`} onClick={() => setAuto(true)}>Auto-compose</button>
        <button type="button" role="tab" className={`tab ${!auto ? "tab-active" : ""}`} onClick={() => setAuto(false)}>Manual</button>
      </div>

      {noProviders && (
        <div className="alert alert-warning">
          <span>No AI providers configured.</span>
          <button type="button" className="btn btn-sm" onClick={() => onConfigureProviders?.()}>Add an AI provider in Settings</button>
        </div>
      )}

      {auto ? (
        <label className="form-control">
          <span className="label-text">Problem</span>
          <textarea aria-label="problem" className="textarea textarea-bordered" rows={3}
            value={problem} onChange={(e) => setProblem(e.target.value)}
            placeholder="Describe the problem; an AI will compose the team." />
        </label>
      ) : (
        <div className="space-y-3">
          <label className="form-control">
            <span className="label-text">Team name</span>
            <input className="input input-bordered input-sm" value={name} onChange={(e) => setName(e.target.value)} />
          </label>
          <ul className="space-y-2">
            {members.map((m, i) => (
              <li key={i} className="flex gap-2">
                <input aria-label={`role ${i}`} className="input input-bordered input-sm flex-1" placeholder="Role"
                  value={m.role} onChange={(e) => setMembers((ms) => ms.map((x, j) => j === i ? { ...x, role: e.target.value } : x))} />
                <select aria-label={`provider ${i}`} className="select select-bordered select-sm" disabled={noProviders}
                  value={m.provider} onChange={(e) => setMembers((ms) => ms.map((x, j) => j === i ? { ...x, provider: e.target.value } : x))}>
                  {providers.map((p) => <option key={p.id} value={p.id}>{p.label}</option>)}
                </select>
                <button type="button" className="btn btn-ghost btn-sm" onClick={() => setMembers((ms) => ms.filter((_, j) => j !== i))}>✕</button>
              </li>
            ))}
          </ul>
          <button type="button" className="btn btn-sm" disabled={noProviders}
            onClick={() => setMembers((ms) => [...ms, { role: "", provider: providers[0]?.id ?? "" }])}>＋ Add member</button>
        </div>
      )}

      <button type="submit" className="btn btn-primary btn-sm" disabled={creating}>Create team</button>
    </form>
  );
}
```

- [ ] **Step 4: Run to verify pass** — PASS.

- [ ] **Step 5: Story + exports**

`TeamComposer.stories.tsx`: `WithProviders` (providers set) and `NoProviders` (`providers: []`). Export from `index.ts`:
```ts
export { TeamComposer } from "./teams/TeamComposer";
export type { TeamComposerProps, TeamComposerValue, ComposerMember } from "./teams/TeamComposer";
```

- [ ] **Step 6: Full suite + lint + commit**

```bash
git add packages/ui/src/teams/TeamComposer.* packages/ui/src/index.ts
git commit -m "feat(ui): TeamComposer with auto/manual modes + no-provider gating"
```

---

### Task 3: `TeamDetail` + member list + run timeline

**Files:** Create `packages/ui/src/teams/TeamDetail.tsx`, `.test.tsx`, `.stories.tsx`; modify `index.ts`.

**Interfaces:**
```ts
export interface DetailMember { role: string; provider: string }
export interface DetailRunStep { memberRole: string; detail: string; status: "running" | "done" | "error" }
export interface TeamRunView { task: string; steps: DetailRunStep[]; result: string }
export interface TeamDetailProps {
  name: string;
  problem: string;
  members: DetailMember[];
  capabilities: string[];
  run?: TeamRunView | null;
  onAssign: (task: string) => void;
  assigning?: boolean;
}
export function TeamDetail(props: TeamDetailProps): JSX.Element
```

- [ ] **Step 1: Write the failing test**

```tsx
// TeamDetail.test.tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { TeamDetail } from "./TeamDetail";

const base = {
  name: "Discovery", problem: "find precedent",
  members: [{ role: "Researcher", provider: "anthropic" }],
  capabilities: ["read_file", "search"],
};

describe("TeamDetail", () => {
  it("renders members and capabilities", () => {
    render(<TeamDetail {...base} onAssign={() => {}} />);
    expect(screen.getByText("Researcher")).toBeTruthy();
    expect(screen.getByText("read_file")).toBeTruthy();
  });
  it("assigns a task", () => {
    const onAssign = vi.fn();
    render(<TeamDetail {...base} onAssign={onAssign} />);
    fireEvent.change(screen.getByLabelText(/task/i), { target: { value: "summarize case" } });
    fireEvent.click(screen.getByRole("button", { name: /assign/i }));
    expect(onAssign).toHaveBeenCalledWith("summarize case");
  });
  it("renders a run timeline when a run is present", () => {
    const run = { task: "t", steps: [{ memberRole: "Researcher", detail: "did x", status: "done" as const }], result: "synthesized" };
    render(<TeamDetail {...base} run={run} onAssign={() => {}} />);
    expect(screen.getByText(/synthesized/)).toBeTruthy();
    expect(screen.getByText(/did x/)).toBeTruthy();
  });
});
```

- [ ] **Step 2: Run to verify fail** — FAIL.

- [ ] **Step 3: Implement `TeamDetail`** (reuse `StatusDot` for step status)

```tsx
// TeamDetail.tsx
import { useState } from "react";
import { StatusDot } from "../primitives/StatusDot";

export interface DetailMember { role: string; provider: string }
export interface DetailRunStep { memberRole: string; detail: string; status: "running" | "done" | "error" }
export interface TeamRunView { task: string; steps: DetailRunStep[]; result: string }
export interface TeamDetailProps {
  name: string;
  problem: string;
  members: DetailMember[];
  capabilities: string[];
  run?: TeamRunView | null;
  onAssign: (task: string) => void;
  assigning?: boolean;
}

export function TeamDetail({ name, problem, members, capabilities, run, onAssign, assigning }: TeamDetailProps): JSX.Element {
  const [task, setTask] = useState("");
  return (
    <div className="space-y-6 overflow-y-auto p-6">
      <header>
        <h1 className="text-xl font-semibold">{name}</h1>
        {problem && <p className="text-sm text-base-content/70">{problem}</p>}
      </header>

      <section>
        <h2 className="mb-2 text-sm font-medium">Members</h2>
        <ul className="space-y-1">
          {members.map((m, i) => (
            <li key={i} className="flex items-center justify-between rounded-box border border-base-300 px-3 py-2">
              <span className="font-medium">{m.role}</span>
              <span className="badge badge-ghost badge-sm">{m.provider}</span>
            </li>
          ))}
        </ul>
      </section>

      <section>
        <h2 className="mb-2 text-sm font-medium">Capabilities</h2>
        <div className="flex flex-wrap gap-2">
          {capabilities.map((c) => <span key={c} className="badge badge-outline">{c}</span>)}
        </div>
      </section>

      <section className="space-y-2">
        <h2 className="text-sm font-medium">Assign a task</h2>
        <div className="flex gap-2">
          <input aria-label="task" className="input input-bordered input-sm flex-1" value={task} onChange={(e) => setTask(e.target.value)} />
          <button className="btn btn-primary btn-sm" disabled={assigning || !task} onClick={() => onAssign(task)}>Assign</button>
        </div>
      </section>

      {run && (
        <section className="space-y-2">
          <h2 className="text-sm font-medium">Run</h2>
          <ol className="space-y-1">
            {run.steps.map((s, i) => (
              <li key={i} className="flex items-center gap-2 rounded-box border border-base-300 px-3 py-2 text-sm">
                <StatusDot status={s.status} live={false} />
                <span className="font-medium">{s.memberRole}</span>
                <span className="text-base-content/70">{s.detail}</span>
              </li>
            ))}
          </ol>
          <div className="rounded-box bg-base-200 p-3 text-sm">{run.result}</div>
        </section>
      )}
    </div>
  );
}
```

- [ ] **Step 4: Run to verify pass** — PASS.

- [ ] **Step 5: Story + exports** (`NoRun` + `WithRun`). Export `TeamDetail` + types from `index.ts`.

- [ ] **Step 6: Full suite + lint + commit**

```bash
git add packages/ui/src/teams/TeamDetail.* packages/ui/src/index.ts
git commit -m "feat(ui): TeamDetail with members, capabilities, and run timeline"
```

---

### Task 4: Teams container + routes + `useTeam`/`useAssignTeamTask`

**Files:**
- Modify: `apps/desktop/src/data/queries.ts` (add `useTeam`, `useAssignTeamTask`)
- Create: `apps/desktop/src/containers/Teams.tsx`, `apps/desktop/src/containers/TeamDetailPage.tsx`
- Modify: `apps/desktop/src/routes/teams.tsx`; Create: `apps/desktop/src/routes/teams.$teamId.tsx`
- Test: `apps/desktop/src/containers/Teams.test.tsx`

**Interfaces:**
```ts
// queries.ts
export function useTeam(id: string | null): UseQueryResult<Team>
export function useAssignTeamTask(): UseMutationResult<TeamRun, Error, { teamId: string; task: string }>
```

- [ ] **Step 1: Add the hooks to `queries.ts`**

```ts
import type { CreateWorkspace, CreateTeam, CreateSchedule } from "./bindings"; // existing import — extend if needed

export function useTeam(id: string | null) {
  return useQuery({ queryKey: ["team", id], queryFn: () => api.getTeam(id!), enabled: !!id });
}
export function useAssignTeamTask() {
  return useMutation({
    mutationFn: ({ teamId, task }: { teamId: string; task: string }) => api.assignTeamTask(teamId, task),
  });
}
```

- [ ] **Step 2: Write the failing `Teams` container test**

```tsx
// Teams.test.tsx
import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createStore, Provider as JotaiProvider } from "jotai";
import { describe, it, expect, vi, beforeEach } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tanstack/react-router", () => ({ useNavigate: () => () => {} }));
import Teams from "./Teams";
import { activeWorkspaceIdAtom } from "../store/atoms";

function wrap(ui: React.ReactNode) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const store = createStore();
  store.set(activeWorkspaceIdAtom, "w1");
  return <QueryClientProvider client={qc}><JotaiProvider store={store}>{ui}</JotaiProvider></QueryClientProvider>;
}

describe("Teams", () => {
  beforeEach(() => { invoke.mockReset(); localStorage.clear(); });
  it("lists teams for the active workspace", async () => {
    invoke.mockImplementation((cmd: string) =>
      cmd === "list_teams" ? Promise.resolve([{ id: "tm_1", workspace_id: "w1", name: "Discovery", problem: "p", members: [{ role: "R", provider: "anthropic" }], capabilities: [] }]) : Promise.resolve([]));
    render(wrap(<Teams />));
    await waitFor(() => expect(screen.getByText("Discovery")).toBeTruthy());
  });
});
```

- [ ] **Step 3: Run to verify fail** — FAIL (`Teams` missing).

- [ ] **Step 4: Implement `Teams` container** (list + composer toggle)

```tsx
// Teams.tsx
import { useState } from "react";
import { useAtomValue } from "jotai";
import { useNavigate } from "@tanstack/react-router";
import { TeamList, TeamComposer, EmptyState } from "@manch/ui";
import { activeWorkspaceIdAtom } from "../store/atoms";
import { ALL_PROVIDERS } from "../lib/providers";
import { useTeams, useCreateTeam, useConfiguredProviders } from "../data/queries";

export default function Teams() {
  const workspaceId = useAtomValue(activeWorkspaceIdAtom);
  const teams = useTeams(workspaceId);
  const create = useCreateTeam();
  const configured = useConfiguredProviders();
  const navigate = useNavigate();
  const [composing, setComposing] = useState(false);

  const providerOptions = ALL_PROVIDERS.filter((p) => (configured.data ?? []).includes(p.id as never));

  if (!workspaceId) return <EmptyState glyph="🗂" title="No workspace" description="Pick or create a workspace first." />;

  if (composing) {
    return (
      <TeamComposer
        providers={providerOptions}
        creating={create.isPending}
        onConfigureProviders={() => navigate({ to: "/settings" })}
        onCreate={(v) =>
          create.mutate(
            { workspace_id: workspaceId, name: v.name, problem: v.problem, auto: v.auto, members: v.members },
            { onSuccess: (t) => { setComposing(false); navigate({ to: "/teams/$teamId", params: { teamId: t.id } }); } },
          )
        }
      />
    );
  }

  return (
    <TeamList
      teams={(teams.data ?? []).map((t) => ({ id: t.id, name: t.name, problem: t.problem, memberCount: t.members.length }))}
      onOpen={(id) => navigate({ to: "/teams/$teamId", params: { teamId: id } })}
      onNew={() => setComposing(true)}
    />
  );
}
```

- [ ] **Step 5: Implement `TeamDetailPage` container**

```tsx
// TeamDetailPage.tsx
import { useParams } from "@tanstack/react-router";
import { TeamDetail, EmptyState } from "@manch/ui";
import { useTeam, useAssignTeamTask } from "../data/queries";

export default function TeamDetailPage() {
  const { teamId } = useParams({ from: "/teams/$teamId" });
  const team = useTeam(teamId);
  const assign = useAssignTeamTask();

  if (team.isLoading) return <EmptyState glyph="⏳" title="Loading…" />;
  if (!team.data) return <EmptyState glyph="❓" title="Team not found" />;

  const run = assign.data
    ? { task: assign.data.task, result: assign.data.result, steps: assign.data.steps.map((s) => ({ memberRole: s.member_role, detail: s.detail, status: s.status as "running" | "done" | "error" })) }
    : null;

  return (
    <TeamDetail
      name={team.data.name}
      problem={team.data.problem}
      members={team.data.members.map((m) => ({ role: m.role, provider: m.provider }))}
      capabilities={team.data.capabilities}
      run={run}
      assigning={assign.isPending}
      onAssign={(task) => assign.mutate({ teamId, task })}
    />
  );
}
```

- [ ] **Step 6: Wire routes**

`routes/teams.tsx`:
```tsx
import { createFileRoute } from "@tanstack/react-router";
import Teams from "../containers/Teams";
export const Route = createFileRoute("/teams")({ component: Teams });
```
`routes/teams.$teamId.tsx`:
```tsx
import { createFileRoute } from "@tanstack/react-router";
import TeamDetailPage from "../containers/TeamDetailPage";
export const Route = createFileRoute("/teams/$teamId")({ component: TeamDetailPage });
```

- [ ] **Step 7: Regenerate the route tree**

Run: `pnpm --filter @manch/desktop build` (the Vite router plugin regenerates `routeTree.gen.ts`; there is no standalone CLI in this repo — confirmed in Plan 1 Task 6). Then `git add apps/desktop/src/routeTree.gen.ts`.

- [ ] **Step 8: Run the container test + full suite + lint**

```bash
just gen   # ensure bindings.ts exists for tsc
pnpm --filter @manch/desktop exec vitest run src/containers/Teams.test.tsx
pnpm --filter @manch/desktop test && pnpm --filter @manch/desktop lint
```
Expected: PASS.

- [ ] **Step 9: Phase D boundary — full CI + commit**

```bash
just ci
git add apps/desktop/src docs
git commit -m "feat(desktop): Teams section — list, composer, detail, assign-task run"
```

---

## Phase E — Schedule

### Task 5: `ScheduleList` + `ScheduleForm`

**Files:** Create `packages/ui/src/schedule/ScheduleList.tsx`, `ScheduleForm.tsx`, each `.test.tsx` + `.stories.tsx`; modify `index.ts`.

**Interfaces:**
```ts
export interface ScheduleItemView { id: string; target: string; cadence: string; nextRun: string }
export interface ScheduleListProps { schedules: ScheduleItemView[] }
export function ScheduleList(props: ScheduleListProps): JSX.Element

export interface ScheduleFormValue { target: string; cadence: string; nextRun: string }
export interface ScheduleFormProps { onCreate: (value: ScheduleFormValue) => void; creating?: boolean }
export function ScheduleForm(props: ScheduleFormProps): JSX.Element
```

- [ ] **Step 1: Write failing `ScheduleList` test**

```tsx
// ScheduleList.test.tsx
import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { ScheduleList } from "./ScheduleList";

describe("ScheduleList", () => {
  it("renders schedules", () => {
    render(<ScheduleList schedules={[{ id: "s1", target: "Discovery team", cadence: "daily", nextRun: "2026-07-01T09:00:00Z" }]} />);
    expect(screen.getByText("Discovery team")).toBeTruthy();
    expect(screen.getByText(/daily/)).toBeTruthy();
  });
});
```

- [ ] **Step 2: Run to verify fail**, then implement `ScheduleList`:

```tsx
// ScheduleList.tsx
export interface ScheduleItemView { id: string; target: string; cadence: string; nextRun: string }
export interface ScheduleListProps { schedules: ScheduleItemView[] }

export function ScheduleList({ schedules }: ScheduleListProps): JSX.Element {
  return (
    <ul className="space-y-2">
      {schedules.map((s) => (
        <li key={s.id} className="flex items-center justify-between rounded-box border border-base-300 px-3 py-2">
          <div>
            <div className="font-medium text-base-content">{s.target}</div>
            <div className="text-xs text-base-content/60">next: {s.nextRun}</div>
          </div>
          <span className="badge badge-outline">{s.cadence}</span>
        </li>
      ))}
    </ul>
  );
}
```

- [ ] **Step 3: Run to verify pass**.

- [ ] **Step 4: Write failing `ScheduleForm` test**

```tsx
// ScheduleForm.test.tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { ScheduleForm } from "./ScheduleForm";

describe("ScheduleForm", () => {
  it("submits target/cadence/nextRun", () => {
    const onCreate = vi.fn();
    render(<ScheduleForm onCreate={onCreate} />);
    fireEvent.change(screen.getByLabelText(/target/i), { target: { value: "Discovery team" } });
    fireEvent.change(screen.getByLabelText(/next run/i), { target: { value: "2026-07-01T09:00" } });
    fireEvent.click(screen.getByRole("button", { name: /add schedule/i }));
    expect(onCreate).toHaveBeenCalledWith(expect.objectContaining({ target: "Discovery team", cadence: "daily" }));
  });
});
```

- [ ] **Step 5: Run to verify fail**, then implement `ScheduleForm` (cadence defaults `daily`):

```tsx
// ScheduleForm.tsx
import { useState } from "react";
export interface ScheduleFormValue { target: string; cadence: string; nextRun: string }
export interface ScheduleFormProps { onCreate: (value: ScheduleFormValue) => void; creating?: boolean }

export function ScheduleForm({ onCreate, creating }: ScheduleFormProps): JSX.Element {
  const [target, setTarget] = useState("");
  const [cadence, setCadence] = useState("daily");
  const [nextRun, setNextRun] = useState("");
  return (
    <form className="flex flex-wrap items-end gap-2" onSubmit={(e) => { e.preventDefault(); onCreate({ target, cadence, nextRun }); }}>
      <label className="form-control"><span className="label-text">Target</span>
        <input aria-label="target" className="input input-bordered input-sm" value={target} onChange={(e) => setTarget(e.target.value)} /></label>
      <label className="form-control"><span className="label-text">Cadence</span>
        <select aria-label="cadence" className="select select-bordered select-sm" value={cadence} onChange={(e) => setCadence(e.target.value)}>
          <option value="once">once</option><option value="daily">daily</option><option value="weekly">weekly</option>
        </select></label>
      <label className="form-control"><span className="label-text">Next run</span>
        <input aria-label="next run" type="datetime-local" className="input input-bordered input-sm" value={nextRun} onChange={(e) => setNextRun(e.target.value)} /></label>
      <button type="submit" className="btn btn-primary btn-sm" disabled={creating}>Add schedule</button>
    </form>
  );
}
```

- [ ] **Step 6: Run to verify pass; stories + exports; full suite + lint; commit**

Stories for both (`Default`, `Empty` for list). Export both + types from `index.ts`.
```bash
git add packages/ui/src/schedule packages/ui/src/index.ts
git commit -m "feat(ui): ScheduleList + ScheduleForm"
```

---

### Task 6: Schedule container + route

**Files:** Create `apps/desktop/src/containers/SchedulePage.tsx`; modify `apps/desktop/src/routes/schedule.tsx`; test `SchedulePage.test.tsx`.

- [ ] **Step 1: Write the failing container test** (mirrors Teams.test.tsx pattern: mock invoke `list_schedules` → renders the item; set `activeWorkspaceIdAtom`).

```tsx
// SchedulePage.test.tsx — same wrap() helper as Teams.test.tsx
it("lists schedules and creates one", async () => {
  invoke.mockImplementation((cmd: string) =>
    cmd === "list_schedules" ? Promise.resolve([{ id: "s1", workspace_id: "w1", target: "Discovery team", cadence: "daily", next_run: "2026-07-01T09:00:00Z" }]) : Promise.resolve([]));
  render(wrap(<SchedulePage />));
  await waitFor(() => expect(screen.getByText("Discovery team")).toBeTruthy());
});
```

- [ ] **Step 2: Run to verify fail**, then implement `SchedulePage`:

```tsx
// SchedulePage.tsx
import { useAtomValue } from "jotai";
import { ScheduleList, ScheduleForm, EmptyState } from "@manch/ui";
import { activeWorkspaceIdAtom } from "../store/atoms";
import { useSchedules, useCreateSchedule } from "../data/queries";

export default function SchedulePage() {
  const workspaceId = useAtomValue(activeWorkspaceIdAtom);
  const schedules = useSchedules(workspaceId);
  const create = useCreateSchedule();
  if (!workspaceId) return <EmptyState glyph="🗂" title="No workspace" description="Pick or create a workspace first." />;
  const items = (schedules.data ?? []).map((s) => ({ id: s.id, target: s.target, cadence: s.cadence, nextRun: s.next_run }));
  return (
    <div className="space-y-6 p-6">
      <h1 className="text-xl font-semibold">Schedule</h1>
      <ScheduleForm creating={create.isPending}
        onCreate={(v) => create.mutate({ workspace_id: workspaceId, target: v.target, cadence: v.cadence, next_run: v.nextRun })} />
      {items.length === 0
        ? <EmptyState glyph="📅" title="No schedules yet" description="Add one above." />
        : <ScheduleList schedules={items} />}
    </div>
  );
}
```

- [ ] **Step 3: Wire route** (`routes/schedule.tsx` → `SchedulePage`), regenerate route tree (`pnpm --filter @manch/desktop build`), `git add routeTree.gen.ts`.

- [ ] **Step 4: Test + lint + commit**

```bash
just gen && pnpm --filter @manch/desktop test && pnpm --filter @manch/desktop lint
git add apps/desktop/src
git commit -m "feat(desktop): Schedule section wiring"
```

---

## Phase F — Search

### Task 7: `SearchBar` + `SearchResults`

**Files:** Create `packages/ui/src/search/SearchBar.tsx`, `SearchResults.tsx`, each `.test.tsx` + `.stories.tsx`; modify `index.ts`.

**Interfaces:**
```ts
export interface SearchBarProps { value: string; onChange: (q: string) => void; onSubmit: () => void }
export function SearchBar(props: SearchBarProps): JSX.Element

export interface SearchResultView { kind: string; id: string; title: string; snippet: string }
export interface SearchResultsProps { results: SearchResultView[]; onOpen: (kind: string, id: string) => void }
export function SearchResults(props: SearchResultsProps): JSX.Element
```

- [ ] **Step 1: Failing `SearchBar` test**

```tsx
// SearchBar.test.tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { SearchBar } from "./SearchBar";
describe("SearchBar", () => {
  it("changes and submits", () => {
    const onChange = vi.fn(); const onSubmit = vi.fn();
    render(<SearchBar value="" onChange={onChange} onSubmit={onSubmit} />);
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "precedent" } });
    expect(onChange).toHaveBeenCalledWith("precedent");
    fireEvent.submit(screen.getByRole("search"));
    expect(onSubmit).toHaveBeenCalledOnce();
  });
});
```

- [ ] **Step 2: Run to verify fail**, then implement `SearchBar`:

```tsx
// SearchBar.tsx
export interface SearchBarProps { value: string; onChange: (q: string) => void; onSubmit: () => void }
export function SearchBar({ value, onChange, onSubmit }: SearchBarProps): JSX.Element {
  return (
    <form role="search" onSubmit={(e) => { e.preventDefault(); onSubmit(); }} className="flex gap-2">
      <input role="searchbox" aria-label="search" className="input input-bordered input-sm flex-1"
        value={value} onChange={(e) => onChange(e.target.value)} placeholder="Search teams, schedules…" />
      <button type="submit" className="btn btn-primary btn-sm">Search</button>
    </form>
  );
}
```

- [ ] **Step 3: Failing `SearchResults` test**

```tsx
// SearchResults.test.tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { SearchResults } from "./SearchResults";
describe("SearchResults", () => {
  it("renders hits and opens one", () => {
    const onOpen = vi.fn();
    render(<SearchResults results={[{ kind: "team", id: "tm_1", title: "Discovery", snippet: "find precedent" }]} onOpen={onOpen} />);
    fireEvent.click(screen.getByRole("button", { name: /Discovery/ }));
    expect(onOpen).toHaveBeenCalledWith("team", "tm_1");
  });
});
```

- [ ] **Step 4: Run to verify fail**, then implement `SearchResults`:

```tsx
// SearchResults.tsx
export interface SearchResultView { kind: string; id: string; title: string; snippet: string }
export interface SearchResultsProps { results: SearchResultView[]; onOpen: (kind: string, id: string) => void }
export function SearchResults({ results, onOpen }: SearchResultsProps): JSX.Element {
  return (
    <ul className="space-y-2">
      {results.map((r) => (
        <li key={`${r.kind}:${r.id}`}>
          <button aria-label={r.title} onClick={() => onOpen(r.kind, r.id)}
            className="w-full rounded-box border border-base-300 px-3 py-2 text-left hover:bg-base-200">
            <span className="badge badge-ghost badge-sm mr-2">{r.kind}</span>
            <span className="font-medium">{r.title}</span>
            {r.snippet && <span className="block text-sm text-base-content/60">{r.snippet}</span>}
          </button>
        </li>
      ))}
    </ul>
  );
}
```

- [ ] **Step 5: Run to verify pass; stories + exports; full suite + lint; commit**

```bash
git add packages/ui/src/search packages/ui/src/index.ts
git commit -m "feat(ui): SearchBar + SearchResults"
```

---

### Task 8: Search container + route

**Files:** Create `apps/desktop/src/containers/SearchPage.tsx`; modify `apps/desktop/src/routes/search.tsx`; test `SearchPage.test.tsx`.

- [ ] **Step 1: Failing container test** (mock `search` invoke → renders a hit after submitting a query; set active workspace).

- [ ] **Step 2: Run to verify fail**, then implement `SearchPage`:

```tsx
// SearchPage.tsx
import { useState } from "react";
import { useAtomValue } from "jotai";
import { useNavigate } from "@tanstack/react-router";
import { SearchBar, SearchResults, EmptyState } from "@manch/ui";
import { activeWorkspaceIdAtom } from "../store/atoms";
import { useSearch } from "../data/queries";

export default function SearchPage() {
  const workspaceId = useAtomValue(activeWorkspaceIdAtom);
  const [draft, setDraft] = useState("");
  const [query, setQuery] = useState("");
  const navigate = useNavigate();
  const results = useSearch(workspaceId, query, []);
  if (!workspaceId) return <EmptyState glyph="🗂" title="No workspace" description="Pick or create a workspace first." />;
  const hits = results.data ?? [];
  return (
    <div className="space-y-4 p-6">
      <h1 className="text-xl font-semibold">Search</h1>
      <SearchBar value={draft} onChange={setDraft} onSubmit={() => setQuery(draft)} />
      {query && hits.length === 0
        ? <EmptyState glyph="🔍" title="No results" description={`Nothing matched “${query}”.`} />
        : <SearchResults results={hits} onOpen={(kind, id) => { if (kind === "team") navigate({ to: "/teams/$teamId", params: { teamId: id } }); else navigate({ to: "/schedule" }); }} />}
    </div>
  );
}
```

- [ ] **Step 3: Wire route** (`routes/search.tsx` → `SearchPage`), regenerate route tree, `git add routeTree.gen.ts`.

- [ ] **Step 4: Test + lint + commit**

```bash
just gen && pnpm --filter @manch/desktop test && pnpm --filter @manch/desktop lint
git add apps/desktop/src
git commit -m "feat(desktop): Search section wiring"
```

---

## Phase G — Chat compare + cross-verify

### Task 9: `CompareView`

**Files:** Create `packages/ui/src/stage/CompareView.tsx`, `.test.tsx`, `.stories.tsx`; modify `index.ts`.

**Interfaces:**
```ts
export interface CompareReport { provider: string; text: string }
export interface CompareViewProps { reports: CompareReport[]; summary: string }
export function CompareView(props: CompareViewProps): JSX.Element
```
Renders N provider columns (markdown text rendered via the same `react-markdown` + `remark-gfm` the `Message` component uses) + a synthesized cross-verification summary card.

- [ ] **Step 1: Failing test**

```tsx
// CompareView.test.tsx
import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { CompareView } from "./CompareView";
describe("CompareView", () => {
  it("renders one column per report plus the summary", () => {
    render(<CompareView reports={[{ provider: "anthropic", text: "A says" }, { provider: "claude-code", text: "B says" }]} summary="they agree" />);
    expect(screen.getByText(/A says/)).toBeTruthy();
    expect(screen.getByText(/B says/)).toBeTruthy();
    expect(screen.getByText(/they agree/)).toBeTruthy();
    expect(screen.getAllByRole("article")).toHaveLength(2);
  });
});
```

- [ ] **Step 2: Run to verify fail**, then implement `CompareView` (use `react-markdown` + `remark-gfm`, already `@manch/ui` deps):

```tsx
// CompareView.tsx
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

export interface CompareReport { provider: string; text: string }
export interface CompareViewProps { reports: CompareReport[]; summary: string }

export function CompareView({ reports, summary }: CompareViewProps): JSX.Element {
  return (
    <div className="space-y-4 p-4">
      <div className="grid gap-3" style={{ gridTemplateColumns: `repeat(${Math.max(reports.length, 1)}, minmax(0, 1fr))` }}>
        {reports.map((r) => (
          <article key={r.provider} className="rounded-box border border-base-300 bg-base-100 p-3">
            <div className="mb-2 badge badge-primary badge-sm">{r.provider}</div>
            <div className="prose prose-sm max-w-none"><ReactMarkdown remarkPlugins={[remarkGfm]}>{r.text}</ReactMarkdown></div>
          </article>
        ))}
      </div>
      <div className="rounded-box border border-primary/40 bg-base-200 p-3">
        <div className="mb-1 text-sm font-medium text-primary">Cross-verification</div>
        <div className="prose prose-sm max-w-none"><ReactMarkdown remarkPlugins={[remarkGfm]}>{summary}</ReactMarkdown></div>
      </div>
    </div>
  );
}
```

- [ ] **Step 3: Run to verify pass; story (`TwoProviders`) + export; full suite + lint; commit**

```bash
git add packages/ui/src/stage/CompareView.* packages/ui/src/index.ts
git commit -m "feat(ui): CompareView for multi-AI cross-verification"
```

---

### Task 10: Compare-mode wiring (atom + header multi-select + Stage fan-out)

**Files:**
- Modify: `apps/desktop/src/store/atoms.ts` (add `compareProvidersAtom`)
- Modify: `apps/desktop/src/containers/Stage.tsx` (compare mode)
- Test: `apps/desktop/src/containers/Stage.test.tsx` (or a focused compare test)

**Interfaces:**
```ts
export const compareProvidersAtom; // atom<string[]>([])  — selected provider ids for compare mode
```

- [ ] **Step 1: Add the atom**

In `store/atoms.ts`:
```ts
export const compareProvidersAtom = atom<string[]>([]);
```

- [ ] **Step 2: Write the failing compare test**

```tsx
// Stage.test.tsx (compare path) — wrap with QueryClient + jotai store; mock invoke
it("shows a cross-verification view when >1 provider is selected and a prompt is sent", async () => {
  const store = createStore();
  store.set(compareProvidersAtom, ["anthropic", "claude-code"]);
  invoke.mockImplementation((cmd: string) =>
    cmd === "cross_verify"
      ? Promise.resolve({ reports: [{ provider: "anthropic", text: "A" }, { provider: "claude-code", text: "B" }], summary: "agree" })
      : Promise.resolve([]));
  // render Stage in compare mode, submit a prompt via the composer, assert CompareView renders
  // (exact harness mirrors the existing Stage/useSend tests)
});
```

- [ ] **Step 3: Run to verify fail**, then implement compare mode in `Stage.tsx`

Read the current `Stage.tsx` first. Add: read `compareProvidersAtom`; when its length > 1, the composer's submit calls `useCrossVerify().mutate({ providers, text })` instead of the normal single-provider `useSend` path, and the transcript area renders `<CompareView reports={data.reports} summary={data.summary} />` while/after the mutation resolves. When length ≤ 1, behavior is unchanged (existing single-provider streaming via `useSend`). The provider multi-select UI lives in the stage header — extend the header usage (or add a small multi-select control in the Stage container above the composer) bound to `compareProvidersAtom`, options = configured providers from `useConfiguredProviders()`. Keep `@manch/ui` components prop-driven; the multi-select can be a small inline control in the container or a new `@manch/ui` presentational control if cleaner (if added to `@manch/ui`, it needs a test + story).

- [ ] **Step 4: Run to verify pass; full suite + lint**

- [ ] **Step 5: Phase G boundary — full CI + commit**

```bash
just ci
git add apps/desktop/src
git commit -m "feat(desktop): Chat compare mode with multi-AI cross-verification"
```

---

## Phase H — Gating + first-run + polish

### Task 11: No-provider gating (Chat send + nudge)

**Files:** Modify `apps/desktop/src/containers/Stage.tsx` (and/or the stage header control); test the gated state.

- [ ] **Step 1: Write the failing test** — with `useConfiguredProviders` returning `[]`, the composer send is disabled and a nudge to `/settings` is shown.

```tsx
it("disables send and nudges to Settings when no provider is configured", async () => {
  invoke.mockResolvedValue([]); // list_configured_providers → none
  // render Stage; assert the send control is disabled and a "configure a provider" link/button to /settings exists
});
```

- [ ] **Step 2: Run to verify fail**, then implement: in `Stage.tsx`, read `useConfiguredProviders()`; when empty, pass `disabled`/a nudge into the composer area (Composer already takes a `disabled`-style prop for streaming — reuse or add a `blockedReason` prop to the `@manch/ui` `Composer` if needed; if you extend `Composer`, add its test + story). The nudge is a small inline `alert` with a button calling `navigate({ to: "/settings" })`. Team gating already exists in `TeamComposer` (Task 2) — verify it routes to `/settings` via the container's `onConfigureProviders` (wired in Task 4).

- [ ] **Step 3: Run to verify pass; lint; commit**

```bash
git add apps/desktop/src packages/ui/src
git commit -m "feat(desktop): gate Chat send on a configured provider with a Settings nudge"
```

---

### Task 12: First-run on `/chat` + empty states + final CI

**Files:** Modify `apps/desktop/src/containers/Stage.tsx` / `routes/chat.tsx` as needed; verify empty states across sections.

Context (from Plan 1 ledger WATCH): re-homing the stage under `/chat` dropped the old first-run gate. With zero conversations, `/chat` should present an inviting empty stage whose "＋ New" (the existing `GreenRoomView.onNew`) creates a conversation. Ensure this works and the center Stage shows an `EmptyState` ("No conversation selected — start one") when `activeIdAtom` is null.

- [ ] **Step 1: Write the failing test** — render `/chat` (or the `Stage` container) with empty `conversationsAtom`; assert an inviting empty state with a working "new conversation" affordance, and that selecting/creating shows the composer.

- [ ] **Step 2: Run to verify fail**, then implement the empty-state branch in the Stage container (when no active conversation, render `EmptyState` with an action that calls `newConversation()` + sets `activeIdAtom`). Confirm `GreenRoom`'s "＋ New" already does this (Plan 1) and the center mirrors it.

- [ ] **Step 3: Verify empty states across Teams/Schedule/Search** — each already renders `EmptyState` when its list is empty (Tasks 4/6/8); spot-check and add any missing one.

- [ ] **Step 4: Run to verify pass; full suite + lint.**

- [ ] **Step 5: FINAL — full CI + commit**

```bash
just ci   # MUST be green — Plan 2 completion gate
git add apps/desktop/src
git commit -m "feat(desktop): first-run empty stage on /chat + section empty states"
```

---

## Self-Review

**Spec coverage (against `2026-06-30-manch-ui-multi-section.md`):**
- Search section (req 3) → Tasks 7–8. ✓
- Schedule section (req 3) → Tasks 5–6. ✓
- Teams section incl. AI auto-compose + manual + assign-task run (req 3) → Tasks 1–4. ✓
- Chat section + multi-AI cross-verify (req 3, req 6) → Tasks 9–10. ✓
- Gating when no AI (req 5) → Tasks 2 (Team), 11 (Chat). ✓
- Workspace scoping (req 7) → every container reads `activeWorkspaceIdAtom`. ✓
- First-run / empty states → Task 12. ✓
- Theme-agnostic, package boundary, generated DTOs → Global Constraints, enforced per task. ✓

**Placeholder scan:** Task 10's compare-test harness and Task 11/12's tests are described against the existing Stage/useSend test patterns rather than pinned line-for-line, because they depend on the current `Stage.tsx` shape the implementer must read first — flagged explicitly (read the file, mirror the existing harness), not a hidden TODO. All component tasks have complete code.

**Type consistency:** DTO field names used in containers are snake_case from the generated bindings (`workspace_id`, `next_run`, `member_role`) and mapped to camelCase view props at the container boundary (e.g. `nextRun`, `memberRole`) — consistent across Tasks 4/6/8/10. Hook names (`useTeam`, `useAssignTeamTask`, `useTeams`, `useCreateTeam`, `useSchedules`, `useCreateSchedule`, `useSearch`, `useCrossVerify`) match Plan 1 + Task 4 additions. `CreateTeam`/`CreateSchedule` payloads use the exact snake_case DTO fields.

**Open risk to watch:** Task 10 (compare mode) is the most invasive — it branches the Stage send path. Keep the single-provider path untouched (regression risk to Plan 1's streaming) and gate the compare path strictly on `compareProvidersAtom.length > 1`. The `routeTree.gen.ts` regen (Tasks 4/6/8) is via `pnpm --filter @manch/desktop build` (no router CLI in this repo) — commit the plugin-generated file.
