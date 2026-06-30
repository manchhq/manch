# Manch UI Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the single-page stage into a theme-agnostic, workspace-scoped multi-section app shell with a real Tauri/SQLite data layer and a proper Settings page — the foundation the feature sections (Plan 2) build on.

**Architecture:** A 3-region shell (top bar with workspace switcher · left icon nav rail · routed main) in `apps/desktop`, with the existing 3-pane stage re-homed under the `/chat` route. Presentation lives in `@manch/ui` (story + test each); routes/containers/store/queries live in `apps/desktop`. Command DTOs are defined once in a new publishable `crates/manch-dto` crate and generated to a single `bindings.ts` via `ts-rs`. State: React Query over Tauri `invoke`, jotai (`atomWithStorage`) for UI state.

**Tech Stack:** React 19, TanStack Router (file-based), TanStack Query, TanStack Form, jotai, Tailwind 4 + daisyUI 5 (built-in themes), Vitest + Testing Library, Storybook 8; Rust (Tauri 2, rusqlite, serde, ts-rs).

## Global Constraints

- **Package boundary:** `@manch/ui` is pure presentation — no imports of `@tauri-apps/api`, jotai, React Query, or the router. Every `@manch/ui` component ships a `.test.tsx` (Vitest + Testing Library) and a `.stories.tsx` (Storybook 8), rendered from mock props. App wiring (store/queries/engine/router) lives only in `apps/desktop`.
- **Theme-agnostic:** components use ONLY semantic daisyUI tokens (`bg-base-*`, `text-base-content`, `text-primary`, `border-base-300`, `bg-warning`, `text-error`, …). No hardcoded hex/oklch in any component. Default theme is `dark`.
- **One source of truth for command shapes:** DTOs are defined in `crates/manch-dto` and consumed in TS via the generated `apps/desktop/src/data/bindings.ts`. Never hand-write a TS type that mirrors a DTO.
- **`manch-dto` is publishable** (`publish = true`, versioned by `release-plz`); `ts-rs` is an **optional** dependency behind a `ts` feature — the default build is serde-only. Don't hand-bump its version.
- **Rust quality gates:** `cargo fmt` enforced; `cargo clippy --workspace --all-targets -- -D warnings` must be clean. New DB methods get `#[cfg(test)]` unit tests over an in-memory DB. Never hold the SQLite mutex guard across an `await`.
- **Conventional Commits** (`feat:`, `fix:`, `chore:`, `docs:`, `test:`, …). `Cargo.lock` is committed; keep it in sync.
- **Verification per task:** the relevant scoped test command must pass; at each phase boundary `just ci` must be green.
- **Branch:** continue on `feat/manch-ui-stage` (this stacks on PR #14) unless told otherwise.

## File Structure

**New in `@manch/ui` (`packages/ui/src/`):**
- `primitives/NavRail.tsx` — vertical icon nav with active item + onSelect. (replaces use of `IconRail` for top-level nav)
- `primitives/EmptyState.tsx` — centered icon + title + description + optional action.
- `stage/WorkspaceSwitcher.tsx` — dropdown: list workspaces, active, onSelect, onCreate.
- `settings/ThemePicker.tsx` — theme list + active + onSelect.
- `settings/ProviderSettings.tsx` — configured providers list + add-key form (TanStack Form) + remove.
- `settings/WorkspaceSettings.tsx` — rename/delete workspaces.
- `settings/SettingsView.tsx` — composes the three settings sections.
- each with sibling `.test.tsx` + `.stories.tsx`; all exported from `index.ts`.

**New crate `crates/manch-dto/`:**
- `Cargo.toml`, `src/lib.rs` (DTO structs + re-exports), `src/bin/gen-types.rs` (combine → `bindings.ts`).

**Modified in `apps/desktop/`:**
- `src/styles.css` — daisyUI built-in themes; drop manch-stage import.
- `src/routes/__root.tsx` — app shell (top bar + NavRail + theme effect + Outlet).
- `src/routes/index.tsx` — redirect to `/chat`.
- `src/routes/chat.tsx` — the existing 3-pane stage (moved from old index).
- `src/routes/teams.tsx`, `schedule.tsx`, `search.tsx`, `settings.tsx` — section routes (placeholders in this plan, except settings which is built).
- `src/store/atoms.ts` — add `themeAtom`, `activeWorkspaceIdAtom`.
- `src/data/queries.ts` — workspace/data hooks; `src/lib/api.ts` — command wrappers using generated types; unify provider lists.
- `src-tauri/src/db.rs` — workspace/team/schedule tables + CRUD + seed.
- `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs` — new commands + registration.

**Modified repo root:**
- `Cargo.toml` (workspace member), `justfile` (`gen` recipe), `.gitignore` (bindings.ts), `AGENTS.md`/`README.md` repo map row.

**Deleted:** `packages/ui/src/theme/manch-stage.css` (+ its import).

---

## Phase A — Theme-agnostic refactor + UI shell

### Task 1: Make the UI theme-agnostic

**Files:**
- Modify: `apps/desktop/src/styles.css`
- Delete: `packages/ui/src/theme/manch-stage.css`
- Modify: `apps/desktop/src/routes/__root.tsx` (remove hardcoded `data-theme="manch-stage"` — full shell rewrite is Task 6; here just drop the hardcoded theme)
- Audit/Modify: `packages/ui/src/primitives/Spotlight.tsx` and any component using non-semantic colors
- Test: existing `@manch/ui` tests must still pass

**Interfaces:**
- Produces: a daisyUI config exposing themes `dark` (default), `light`, `dracula`, `nord`, `cupcake`; the app no longer hardcodes a theme name.

- [ ] **Step 1: Rewrite `apps/desktop/src/styles.css` to use daisyUI built-in themes**

```css
@import "tailwindcss";
@source "../../../packages/ui/src";
@plugin "daisyui" {
  themes: dark --default, light, dracula, nord, cupcake;
}
```

- [ ] **Step 2: Delete the custom theme file**

```bash
git rm packages/ui/src/theme/manch-stage.css
```

- [ ] **Step 3: Audit `Spotlight` and components for hardcoded colors**

Read `packages/ui/src/primitives/Spotlight.tsx`. If its radial gradient uses a literal color, change it to a semantic token. Expected final gradient line:

```tsx
// inside Spotlight.tsx — use a semantic token so it renders under every theme
style={{ background: "radial-gradient(60% 60% at 50% 0%, var(--color-primary) 0%, transparent 70%)" }}
```

Grep the package for stray literals and replace with semantic tokens:

```bash
rg -n "#[0-9a-fA-F]{3,8}|oklch\(|prose-invert" packages/ui/src
```
Expected after fixes: no matches (or only intentional non-color uses).

- [ ] **Step 4: Run the UI test suite to confirm nothing broke**

Run: `pnpm --filter @manch/ui test`
Expected: PASS (all existing component tests green).

- [ ] **Step 5: Typecheck the desktop app**

Run: `pnpm --filter @manch/desktop lint`
Expected: PASS (no reference to the deleted theme file or `manch-stage`).

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor(ui): drop custom theme; use daisyUI built-in themes (dark default)"
```

---

### Task 2: `NavRail` primitive

**Files:**
- Create: `packages/ui/src/primitives/NavRail.tsx`
- Test: `packages/ui/src/primitives/NavRail.test.tsx`
- Story: `packages/ui/src/primitives/NavRail.stories.tsx`
- Modify: `packages/ui/src/index.ts`

**Interfaces:**
- Produces:
```ts
export interface NavItem { id: string; label: string; glyph: string }
export interface NavRailProps {
  items: NavItem[];
  activeId: string;
  onSelect: (id: string) => void;
}
export function NavRail(props: NavRailProps): JSX.Element
```

- [ ] **Step 1: Write the failing test**

```tsx
// NavRail.test.tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { NavRail } from "./NavRail";

const items = [
  { id: "chat", label: "Chat", glyph: "💬" },
  { id: "teams", label: "Teams", glyph: "👥" },
];

describe("NavRail", () => {
  it("marks the active item with aria-current", () => {
    render(<NavRail items={items} activeId="teams" onSelect={() => {}} />);
    expect(screen.getByRole("tab", { name: /Teams/ }).getAttribute("aria-current")).toBe("page");
    expect(screen.getByRole("tab", { name: /Chat/ }).getAttribute("aria-current")).toBeNull();
  });

  it("calls onSelect with the item id when clicked", () => {
    const onSelect = vi.fn();
    render(<NavRail items={items} activeId="chat" onSelect={onSelect} />);
    fireEvent.click(screen.getByRole("tab", { name: /Teams/ }));
    expect(onSelect).toHaveBeenCalledWith("teams");
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm --filter @manch/ui exec vitest run src/primitives/NavRail.test.tsx`
Expected: FAIL ("Cannot find module './NavRail'").

- [ ] **Step 3: Implement `NavRail`**

```tsx
// NavRail.tsx
export interface NavItem {
  id: string;
  label: string;
  glyph: string;
}

export interface NavRailProps {
  items: NavItem[];
  activeId: string;
  onSelect: (id: string) => void;
}

export function NavRail({ items, activeId, onSelect }: NavRailProps) {
  return (
    <nav role="tablist" aria-orientation="vertical" className="flex h-full flex-col items-center gap-1 bg-base-200 py-3">
      {items.map((item) => {
        const active = item.id === activeId;
        return (
          <button
            key={item.id}
            role="tab"
            aria-current={active ? "page" : undefined}
            aria-label={item.label}
            title={item.label}
            onClick={() => onSelect(item.id)}
            className={`btn btn-square btn-ghost text-xl ${active ? "btn-active text-primary" : "text-base-content/70"}`}
          >
            <span aria-hidden>{item.glyph}</span>
          </button>
        );
      })}
    </nav>
  );
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm --filter @manch/ui exec vitest run src/primitives/NavRail.test.tsx`
Expected: PASS.

- [ ] **Step 5: Write the story + export**

```tsx
// NavRail.stories.tsx
import type { Meta, StoryObj } from "@storybook/react";
import { NavRail } from "./NavRail";

const meta: Meta<typeof NavRail> = { title: "primitives/NavRail", component: NavRail };
export default meta;
type Story = StoryObj<typeof NavRail>;

const items = [
  { id: "chat", label: "Chat", glyph: "💬" },
  { id: "teams", label: "Teams", glyph: "👥" },
  { id: "schedule", label: "Schedule", glyph: "📅" },
  { id: "search", label: "Search", glyph: "🔍" },
  { id: "settings", label: "Settings", glyph: "⚙️" },
];

export const Default: Story = { args: { items, activeId: "chat", onSelect: () => {} } };
```

Add to `packages/ui/src/index.ts`:
```ts
export { NavRail } from "./primitives/NavRail";
export type { NavItem, NavRailProps } from "./primitives/NavRail";
```

- [ ] **Step 6: Commit**

```bash
git add packages/ui/src/primitives/NavRail.* packages/ui/src/index.ts
git commit -m "feat(ui): add NavRail primitive for top-level section navigation"
```

---

### Task 3: `EmptyState` primitive

**Files:**
- Create: `packages/ui/src/primitives/EmptyState.tsx`, `.test.tsx`, `.stories.tsx`
- Modify: `packages/ui/src/index.ts`

**Interfaces:**
- Produces:
```ts
export interface EmptyStateProps {
  glyph?: string;
  title: string;
  description?: string;
  action?: { label: string; onClick: () => void };
}
export function EmptyState(props: EmptyStateProps): JSX.Element
```

- [ ] **Step 1: Write the failing test**

```tsx
// EmptyState.test.tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { EmptyState } from "./EmptyState";

describe("EmptyState", () => {
  it("renders title and description", () => {
    render(<EmptyState title="No teams yet" description="Create one to begin." />);
    expect(screen.getByText("No teams yet")).toBeTruthy();
    expect(screen.getByText("Create one to begin.")).toBeTruthy();
  });

  it("fires the action callback", () => {
    const onClick = vi.fn();
    render(<EmptyState title="Empty" action={{ label: "New", onClick }} />);
    fireEvent.click(screen.getByRole("button", { name: "New" }));
    expect(onClick).toHaveBeenCalledOnce();
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm --filter @manch/ui exec vitest run src/primitives/EmptyState.test.tsx`
Expected: FAIL ("Cannot find module './EmptyState'").

- [ ] **Step 3: Implement `EmptyState`**

```tsx
// EmptyState.tsx
export interface EmptyStateProps {
  glyph?: string;
  title: string;
  description?: string;
  action?: { label: string; onClick: () => void };
}

export function EmptyState({ glyph, title, description, action }: EmptyStateProps) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 p-8 text-center">
      {glyph && <div className="text-4xl opacity-60" aria-hidden>{glyph}</div>}
      <h2 className="text-lg font-semibold text-base-content">{title}</h2>
      {description && <p className="max-w-sm text-sm text-base-content/70">{description}</p>}
      {action && (
        <button className="btn btn-primary btn-sm mt-2" onClick={action.onClick}>{action.label}</button>
      )}
    </div>
  );
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm --filter @manch/ui exec vitest run src/primitives/EmptyState.test.tsx`
Expected: PASS.

- [ ] **Step 5: Story + export**

```tsx
// EmptyState.stories.tsx
import type { Meta, StoryObj } from "@storybook/react";
import { EmptyState } from "./EmptyState";

const meta: Meta<typeof EmptyState> = { title: "primitives/EmptyState", component: EmptyState };
export default meta;
type Story = StoryObj<typeof EmptyState>;

export const Bare: Story = { args: { glyph: "🎭", title: "Nothing here yet" } };
export const WithAction: Story = {
  args: { glyph: "👥", title: "No teams", description: "Spin up a team to get started.", action: { label: "New team", onClick: () => {} } },
};
```

Add to `index.ts`:
```ts
export { EmptyState } from "./primitives/EmptyState";
export type { EmptyStateProps } from "./primitives/EmptyState";
```

- [ ] **Step 6: Commit**

```bash
git add packages/ui/src/primitives/EmptyState.* packages/ui/src/index.ts
git commit -m "feat(ui): add EmptyState primitive"
```

---

### Task 4: `WorkspaceSwitcher` component

**Files:**
- Create: `packages/ui/src/stage/WorkspaceSwitcher.tsx`, `.test.tsx`, `.stories.tsx`
- Modify: `packages/ui/src/index.ts`

**Interfaces:**
- Produces:
```ts
export interface WorkspaceOption { id: string; name: string }
export interface WorkspaceSwitcherProps {
  workspaces: WorkspaceOption[];
  activeId: string | null;
  onSelect: (id: string) => void;
  onCreate: () => void;
}
export function WorkspaceSwitcher(props: WorkspaceSwitcherProps): JSX.Element
```

- [ ] **Step 1: Write the failing test**

```tsx
// WorkspaceSwitcher.test.tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { WorkspaceSwitcher } from "./WorkspaceSwitcher";

const ws = [
  { id: "w1", name: "Legal research" },
  { id: "w2", name: "Health" },
];

describe("WorkspaceSwitcher", () => {
  it("shows the active workspace name on the trigger", () => {
    render(<WorkspaceSwitcher workspaces={ws} activeId="w2" onSelect={() => {}} onCreate={() => {}} />);
    expect(screen.getByRole("button", { name: /Health/ })).toBeTruthy();
  });

  it("selects a workspace", () => {
    const onSelect = vi.fn();
    render(<WorkspaceSwitcher workspaces={ws} activeId="w1" onSelect={onSelect} onCreate={() => {}} />);
    fireEvent.click(screen.getByText("Health"));
    expect(onSelect).toHaveBeenCalledWith("w2");
  });

  it("fires onCreate", () => {
    const onCreate = vi.fn();
    render(<WorkspaceSwitcher workspaces={ws} activeId="w1" onSelect={() => {}} onCreate={onCreate} />);
    fireEvent.click(screen.getByRole("button", { name: /New workspace/ }));
    expect(onCreate).toHaveBeenCalledOnce();
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm --filter @manch/ui exec vitest run src/stage/WorkspaceSwitcher.test.tsx`
Expected: FAIL ("Cannot find module './WorkspaceSwitcher'").

- [ ] **Step 3: Implement `WorkspaceSwitcher`** (daisyUI `dropdown`)

```tsx
// WorkspaceSwitcher.tsx
export interface WorkspaceOption {
  id: string;
  name: string;
}

export interface WorkspaceSwitcherProps {
  workspaces: WorkspaceOption[];
  activeId: string | null;
  onSelect: (id: string) => void;
  onCreate: () => void;
}

export function WorkspaceSwitcher({ workspaces, activeId, onSelect, onCreate }: WorkspaceSwitcherProps) {
  const active = workspaces.find((w) => w.id === activeId);
  return (
    <div className="dropdown">
      <button tabIndex={0} className="btn btn-ghost btn-sm gap-2">
        <span className="font-semibold">{active?.name ?? "Select workspace"}</span>
        <span aria-hidden>▾</span>
      </button>
      <ul tabIndex={0} className="menu dropdown-content z-10 mt-1 w-56 rounded-box bg-base-200 p-2 shadow">
        {workspaces.map((w) => (
          <li key={w.id}>
            <button className={w.id === activeId ? "active" : ""} onClick={() => onSelect(w.id)}>{w.name}</button>
          </li>
        ))}
        <li className="border-t border-base-300 mt-1 pt-1">
          <button onClick={onCreate}>＋ New workspace</button>
        </li>
      </ul>
    </div>
  );
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm --filter @manch/ui exec vitest run src/stage/WorkspaceSwitcher.test.tsx`
Expected: PASS.

- [ ] **Step 5: Story + export**

```tsx
// WorkspaceSwitcher.stories.tsx
import type { Meta, StoryObj } from "@storybook/react";
import { WorkspaceSwitcher } from "./WorkspaceSwitcher";

const meta: Meta<typeof WorkspaceSwitcher> = { title: "stage/WorkspaceSwitcher", component: WorkspaceSwitcher };
export default meta;
type Story = StoryObj<typeof WorkspaceSwitcher>;

export const Default: Story = {
  args: {
    workspaces: [{ id: "w1", name: "Legal research" }, { id: "w2", name: "Health" }],
    activeId: "w1", onSelect: () => {}, onCreate: () => {},
  },
};
export const Empty: Story = { args: { workspaces: [], activeId: null, onSelect: () => {}, onCreate: () => {} } };
```

Add to `index.ts`:
```ts
export { WorkspaceSwitcher } from "./stage/WorkspaceSwitcher";
export type { WorkspaceOption, WorkspaceSwitcherProps } from "./stage/WorkspaceSwitcher";
```

- [ ] **Step 6: Commit**

```bash
git add packages/ui/src/stage/WorkspaceSwitcher.* packages/ui/src/index.ts
git commit -m "feat(ui): add WorkspaceSwitcher dropdown"
```

---

### Task 5: `ThemePicker` component

**Files:**
- Create: `packages/ui/src/settings/ThemePicker.tsx`, `.test.tsx`, `.stories.tsx`
- Modify: `packages/ui/src/index.ts`

**Interfaces:**
- Produces:
```ts
export interface ThemePickerProps {
  themes: string[];
  active: string;
  onSelect: (theme: string) => void;
}
export function ThemePicker(props: ThemePickerProps): JSX.Element
```

- [ ] **Step 1: Write the failing test**

```tsx
// ThemePicker.test.tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { ThemePicker } from "./ThemePicker";

describe("ThemePicker", () => {
  it("marks the active theme as checked", () => {
    render(<ThemePicker themes={["dark", "light"]} active="light" onSelect={() => {}} />);
    const light = screen.getByRole("radio", { name: "light" }) as HTMLInputElement;
    expect(light.checked).toBe(true);
  });

  it("calls onSelect when a theme is chosen", () => {
    const onSelect = vi.fn();
    render(<ThemePicker themes={["dark", "light"]} active="dark" onSelect={onSelect} />);
    fireEvent.click(screen.getByRole("radio", { name: "light" }));
    expect(onSelect).toHaveBeenCalledWith("light");
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm --filter @manch/ui exec vitest run src/settings/ThemePicker.test.tsx`
Expected: FAIL ("Cannot find module './ThemePicker'").

- [ ] **Step 3: Implement `ThemePicker`**

```tsx
// ThemePicker.tsx
export interface ThemePickerProps {
  themes: string[];
  active: string;
  onSelect: (theme: string) => void;
}

export function ThemePicker({ themes, active, onSelect }: ThemePickerProps) {
  return (
    <fieldset className="grid grid-cols-2 gap-2 sm:grid-cols-3">
      <legend className="mb-2 text-sm font-medium text-base-content">Theme</legend>
      {themes.map((theme) => (
        <label key={theme} className="flex items-center gap-2 rounded-box border border-base-300 px-3 py-2">
          <input
            type="radio"
            name="theme"
            aria-label={theme}
            className="radio radio-primary radio-sm"
            checked={theme === active}
            onChange={() => onSelect(theme)}
          />
          <span className="text-sm capitalize">{theme}</span>
        </label>
      ))}
    </fieldset>
  );
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm --filter @manch/ui exec vitest run src/settings/ThemePicker.test.tsx`
Expected: PASS.

- [ ] **Step 5: Story + export**

```tsx
// ThemePicker.stories.tsx
import type { Meta, StoryObj } from "@storybook/react";
import { ThemePicker } from "./ThemePicker";

const meta: Meta<typeof ThemePicker> = { title: "settings/ThemePicker", component: ThemePicker };
export default meta;
type Story = StoryObj<typeof ThemePicker>;

export const Default: Story = {
  args: { themes: ["dark", "light", "dracula", "nord", "cupcake"], active: "dark", onSelect: () => {} },
};
```

Add to `index.ts`:
```ts
export { ThemePicker } from "./settings/ThemePicker";
export type { ThemePickerProps } from "./settings/ThemePicker";
```

- [ ] **Step 6: Commit**

```bash
git add packages/ui/src/settings/ThemePicker.* packages/ui/src/index.ts
git commit -m "feat(ui): add ThemePicker"
```

---

### Task 6: App shell, routing skeleton, theme + workspace atoms

**Files:**
- Modify: `apps/desktop/src/store/atoms.ts` (add `themeAtom`, `activeWorkspaceIdAtom`, `THEMES`)
- Modify: `apps/desktop/src/routes/__root.tsx` (shell)
- Modify: `apps/desktop/src/routes/index.tsx` (redirect)
- Create: `apps/desktop/src/routes/chat.tsx` (the existing stage, moved)
- Create: `apps/desktop/src/routes/teams.tsx`, `schedule.tsx`, `search.tsx` (placeholders)
- Create: `apps/desktop/src/routes/settings.tsx` (placeholder for now; built in Phase C)
- Test: `apps/desktop/src/store/atoms.test.ts` (extend), `apps/desktop/src/routes/-shell.test.tsx`

**Interfaces:**
- Consumes: `NavRail`, `WorkspaceSwitcher`, `EmptyState` from `@manch/ui`.
- Produces:
```ts
// atoms.ts
export const THEMES: string[]; // ["dark","light","dracula","nord","cupcake"]
export const themeAtom; // atomWithStorage<string>("manch.theme", "dark")
export const activeWorkspaceIdAtom; // atomWithStorage<string | null>("manch.activeWorkspace", null)
```

- [ ] **Step 1: Write the failing atoms test**

Append to `apps/desktop/src/store/atoms.test.ts`:
```ts
import { createStore } from "jotai";
import { themeAtom, activeWorkspaceIdAtom, THEMES } from "./atoms";

it("defaults theme to dark and persists a change", () => {
  const store = createStore();
  expect(store.get(themeAtom)).toBe("dark");
  store.set(themeAtom, "dracula");
  expect(store.get(themeAtom)).toBe("dracula");
  expect(localStorage.getItem("manch.theme")).toContain("dracula");
});

it("exposes the configured theme set including dark", () => {
  expect(THEMES).toContain("dark");
});

it("active workspace id defaults to null", () => {
  const store = createStore();
  expect(store.get(activeWorkspaceIdAtom)).toBeNull();
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm --filter @manch/desktop exec vitest run src/store/atoms.test.ts`
Expected: FAIL (`themeAtom` not exported).

- [ ] **Step 3: Add the atoms**

Append to `apps/desktop/src/store/atoms.ts`:
```ts
export const THEMES = ["dark", "light", "dracula", "nord", "cupcake"];
export const themeAtom = atomWithStorage<string>("manch.theme", "dark");
export const activeWorkspaceIdAtom = atomWithStorage<string | null>("manch.activeWorkspace", null);
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm --filter @manch/desktop exec vitest run src/store/atoms.test.ts`
Expected: PASS.

- [ ] **Step 5: Move the existing stage into `routes/chat.tsx`**

Create `apps/desktop/src/routes/chat.tsx` with the 3-pane stage body currently in `routes/index.tsx` (the `grid` branch with GreenRoom/Stage/Performance and the left/right collapse logic). Use route path `/chat`:
```tsx
import { createFileRoute } from "@tanstack/react-router";
import { useAtom } from "jotai";
import { Panel, IconRail } from "@manch/ui";
import GreenRoom from "../containers/GreenRoom";
import Stage from "../containers/Stage";
import Performance from "../containers/Performance";
import { leftCollapsedAtom, rightCollapsedAtom } from "../store/atoms";

export const Route = createFileRoute("/chat")({ component: Chat });

function Chat() {
  const [leftCollapsed, setLeft] = useAtom(leftCollapsedAtom);
  const [rightCollapsed, setRight] = useAtom(rightCollapsedAtom);
  return (
    <div className="grid h-full" style={{ gridTemplateColumns: `${leftCollapsed ? "2.5rem" : "16rem"} 1fr ${rightCollapsed ? "2.5rem" : "20rem"}` }}>
      {leftCollapsed ? (
        <div className="border-r border-base-300 bg-base-200">
          <IconRail items={[{ id: "expand", glyph: "»", label: "Expand", onClick: () => setLeft(false) }]} />
        </div>
      ) : (
        <div className="border-r border-base-300">
          <Panel title="Green Room" side="left" collapsed={false} onToggle={() => setLeft(true)}><GreenRoom /></Panel>
        </div>
      )}
      <main className="min-h-0 bg-base-100"><Stage /></main>
      {rightCollapsed ? (
        <div className="border-l border-base-300 bg-base-200">
          <IconRail items={[{ id: "expand", glyph: "«", label: "Expand", onClick: () => setRight(false) }]} />
        </div>
      ) : (
        <div className="border-l border-base-300">
          <Panel title="Performance" side="right" collapsed={false} onToggle={() => setRight(true)}><Performance /></Panel>
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 6: Add placeholder section routes**

Create `apps/desktop/src/routes/teams.tsx` (and analogous `schedule.tsx`, `search.tsx`, `settings.tsx` — change path, title, glyph, description):
```tsx
import { createFileRoute } from "@tanstack/react-router";
import { EmptyState } from "@manch/ui";

export const Route = createFileRoute("/teams")({ component: Teams });

function Teams() {
  return <EmptyState glyph="👥" title="Teams" description="Coming soon." />;
}
```
For `schedule.tsx`: path `/schedule`, glyph `📅`. `search.tsx`: path `/search`, glyph `🔍`. `settings.tsx`: path `/settings`, glyph `⚙️`, title "Settings" (replaced in Phase C).

- [ ] **Step 7: Make `routes/index.tsx` redirect to `/chat`**

```tsx
import { createFileRoute, redirect } from "@tanstack/react-router";

export const Route = createFileRoute("/")({
  beforeLoad: () => { throw redirect({ to: "/chat" }); },
});
```

- [ ] **Step 8: Rewrite `__root.tsx` as the app shell**

```tsx
import { createRootRoute, Outlet, useRouterState, useNavigate } from "@tanstack/react-router";
import { Provider as JotaiProvider, useAtom, useAtomValue } from "jotai";
import { useEffect } from "react";
import { NavRail } from "@manch/ui";
import { themeAtom } from "../store/atoms";
import WorkspaceBar from "../containers/WorkspaceBar";
import "../styles.css";

export const Route = createRootRoute({ component: RootShell });

const NAV = [
  { id: "/chat", label: "Chat", glyph: "💬" },
  { id: "/teams", label: "Teams", glyph: "👥" },
  { id: "/schedule", label: "Schedule", glyph: "📅" },
  { id: "/search", label: "Search", glyph: "🔍" },
  { id: "/settings", label: "Settings", glyph: "⚙️" },
];

function Shell() {
  const theme = useAtomValue(themeAtom);
  const navigate = useNavigate();
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const activeId = NAV.find((n) => pathname.startsWith(n.id))?.id ?? "/chat";

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme);
  }, [theme]);

  return (
    <div className="flex h-screen flex-col bg-base-100 text-base-content">
      <header className="flex items-center gap-3 border-b border-base-300 px-3 py-2">
        <WorkspaceBar />
        <span className="font-semibold tracking-wide">Manch</span>
      </header>
      <div className="grid min-h-0 flex-1" style={{ gridTemplateColumns: "3.5rem 1fr" }}>
        <NavRail items={NAV} activeId={activeId} onSelect={(id) => navigate({ to: id })} />
        <main className="min-h-0 overflow-hidden"><Outlet /></main>
      </div>
    </div>
  );
}

function RootShell() {
  return (
    <JotaiProvider>
      <Shell />
    </JotaiProvider>
  );
}
```

- [ ] **Step 9: Add a temporary `WorkspaceBar` container stub**

Create `apps/desktop/src/containers/WorkspaceBar.tsx` (real wiring lands in Task 19; stub keeps the shell compiling):
```tsx
import { WorkspaceSwitcher } from "@manch/ui";

export default function WorkspaceBar() {
  return (
    <WorkspaceSwitcher workspaces={[]} activeId={null} onSelect={() => {}} onCreate={() => {}} />
  );
}
```

- [ ] **Step 10: Delete the obsolete `Settings` container usage path & regenerate the route tree**

The old `routes/index.tsx` no longer renders Settings inline. Remove the now-unused `-index.test.tsx` first-run assertions that target the old single-page layout (rewrite minimal shell test below). Regenerate the TanStack route tree:

Run: `pnpm --filter @manch/desktop exec tsr generate`
Expected: `routeTree.gen.ts` updated with `/chat`, `/teams`, `/schedule`, `/search`, `/settings`.

- [ ] **Step 11: Write a minimal shell smoke test**

Create `apps/desktop/src/routes/-shell.test.tsx`:
```tsx
import { render, screen } from "@testing-library/react";
import { describe, it, expect, beforeEach } from "vitest";
import { NavRail } from "@manch/ui";

// The shell's NavRail is the navigation contract; full router rendering is covered e2e.
describe("app shell nav", () => {
  beforeEach(() => localStorage.clear());
  it("renders the five sections", () => {
    const items = [
      { id: "/chat", label: "Chat", glyph: "💬" },
      { id: "/teams", label: "Teams", glyph: "👥" },
      { id: "/schedule", label: "Schedule", glyph: "📅" },
      { id: "/search", label: "Search", glyph: "🔍" },
      { id: "/settings", label: "Settings", glyph: "⚙️" },
    ];
    render(<NavRail items={items} activeId="/chat" onSelect={() => {}} />);
    expect(screen.getAllByRole("tab")).toHaveLength(5);
  });
});
```
Delete `apps/desktop/src/routes/-index.test.tsx` (its assertions targeted the removed single-page layout).

- [ ] **Step 12: Verify**

Run: `pnpm --filter @manch/desktop exec vitest run` then `pnpm --filter @manch/desktop lint`
Expected: PASS both.

- [ ] **Step 13: Commit**

```bash
git add -A
git commit -m "feat(desktop): app shell with nav rail, routed sections, theme atom; re-home stage under /chat"
```

---

## Phase B — `manch-dto` crate + Rust data layer

### Task 7: Scaffold `crates/manch-dto` with the Workspace DTO + `gen-types` bin + `just gen` wiring

**Files:**
- Create: `crates/manch-dto/Cargo.toml`, `crates/manch-dto/src/lib.rs`, `crates/manch-dto/src/bin/gen-types.rs`
- Modify: `Cargo.toml` (workspace members + ts-rs in workspace deps), `justfile` (`gen` recipe), `.gitignore`
- Modify: `AGENTS.md` repo map (add a row)

**Interfaces:**
- Produces (Rust, in `manch_dto`):
```rust
pub struct Workspace { pub id: String, pub name: String, pub description: String }
pub struct CreateWorkspace { pub name: String, pub description: String }
```
- Produces (generated TS): `apps/desktop/src/data/bindings.ts` containing `export type Workspace = {...}` etc.

- [ ] **Step 1: Add `ts-rs` to workspace deps**

In root `Cargo.toml` under `[workspace.dependencies]`:
```toml
ts-rs = "12"
```
And add the member to `[workspace] members`:
```toml
members = ["crates/manch-protocol", "crates/manch-dto", "apps/server", "apps/desktop/src-tauri"]
```

- [ ] **Step 2: Create `crates/manch-dto/Cargo.toml`**

```toml
[package]
name = "manch-dto"
description = "Shared data-transfer objects for the Manch desktop app, with optional TypeScript generation via ts-rs."
version = "0.0.1"
edition.workspace = true
license.workspace = true
repository.workspace = true
rust-version.workspace = true

[features]
ts = ["dep:ts-rs"]

[dependencies]
serde = { workspace = true }
ts-rs = { workspace = true, optional = true }

[[bin]]
name = "gen-types"
required-features = ["ts"]
```

- [ ] **Step 3: Write the DTOs in `src/lib.rs`**

```rust
//! Data-transfer objects shared between the Tauri backend and the TS client.
//! TypeScript is generated by the `gen-types` bin (feature `ts`); never hand-mirror these.

use serde::{Deserialize, Serialize};

#[cfg(feature = "ts")]
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct CreateWorkspace {
    pub name: String,
    pub description: String,
}
```

- [ ] **Step 4: Write the `gen-types` bin**

```rust
// src/bin/gen-types.rs
//! Combines every DTO's TS declaration into one `bindings.ts`.
//! Run via `just gen` (`cargo run -p manch-dto --features ts --bin gen-types`).

use manch_dto::*;
use ts_rs::TS;

/// Every exported DTO, in declaration order. Add new DTOs here.
fn declarations() -> Vec<String> {
    let cfg = ts_rs::Config::default();
    vec![
        Workspace::export_to_string(&cfg).expect("export Workspace"),
        CreateWorkspace::export_to_string(&cfg).expect("export CreateWorkspace"),
    ]
}

fn main() {
    let header = "// AUTO-GENERATED by `manch-dto` gen-types. DO NOT EDIT.\n// Run `just gen` to regenerate.\n\n";
    let body: String = declarations()
        .into_iter()
        .map(|d| {
            // Strip ts-rs import lines; all types live in this single file.
            d.lines()
                .filter(|l| !l.trim_start().starts_with("import "))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let out = "apps/desktop/src/data/bindings.ts";
    std::fs::create_dir_all("apps/desktop/src/data").expect("mkdir data");
    std::fs::write(out, format!("{header}{}\n", body.trim())).expect("write bindings.ts");
    println!("wrote {out}");
}
```

Note: `Config::default()` / `Config::new()` — use whichever the installed `ts-rs` 12 exposes (`ts_rs::Config::default()` is correct for v12). If `export_to_string` needs `&Config`, pass `&cfg` as shown.

- [ ] **Step 5: Build the crate (default features) to prove it's lean**

Run: `cargo build -p manch-dto`
Expected: compiles WITHOUT ts-rs (serde only).

- [ ] **Step 6: Generate the bindings**

Run: `cargo run -p manch-dto --features ts --bin gen-types`
Expected: prints `wrote apps/desktop/src/data/bindings.ts`; file contains `export type Workspace = { id: string, name: string, description: string, };` and `CreateWorkspace`.

- [ ] **Step 7: Wire `just gen` and gitignore**

In `justfile`, change the `gen` recipe:
```make
# Generate protobuf TS bindings (buf) + DTO TS bindings (ts-rs)
gen:
    pnpm generate
    cargo run -p manch-dto --features ts --bin gen-types
```
Append to `.gitignore`:
```
apps/desktop/src/data/bindings.ts
```

- [ ] **Step 8: Add a repo-map row to `AGENTS.md`**

Under the Repository map table, add:
```
| `crates/manch-dto` | `manch-dto` (published lib) | Shared DTOs for the desktop app; generates `bindings.ts` via `ts-rs` (feature `ts`). |
```

- [ ] **Step 9: Verify clippy + fmt on the new crate**

Run: `cargo fmt --all && cargo clippy -p manch-dto --all-targets --all-features -- -D warnings`
Expected: clean.

- [ ] **Step 10: Commit**

```bash
git add Cargo.toml Cargo.lock crates/manch-dto justfile .gitignore AGENTS.md
git commit -m "feat(dto): add manch-dto crate with ts-rs type generation wired into just gen"
```

---

### Task 8: Workspace SQLite schema, CRUD, and seed

**Files:**
- Modify: `apps/desktop/src-tauri/Cargo.toml` (depend on `manch-dto`)
- Modify: `apps/desktop/src-tauri/src/db.rs`
- Test: inline `#[cfg(test)]` in `db.rs`

**Interfaces:**
- Consumes: `manch_dto::{Workspace, CreateWorkspace}`.
- Produces (on `Db`):
```rust
pub fn list_workspaces(&self) -> rusqlite::Result<Vec<Workspace>>
pub fn create_workspace(&self, name: &str, description: &str) -> rusqlite::Result<Workspace>
pub fn rename_workspace(&self, id: &str, name: &str) -> rusqlite::Result<Workspace>
pub fn delete_workspace(&self, id: &str) -> rusqlite::Result<()>
pub fn seed_defaults(&self) -> rusqlite::Result<()> // inserts a default workspace if none exist
fn new_id(prefix: &str) -> String // e.g. "ws_" + counter/random; see Step 3
```

- [ ] **Step 1: Add the dependency**

In `apps/desktop/src-tauri/Cargo.toml` `[dependencies]`:
```toml
manch-dto = { path = "../../../crates/manch-dto" }
```

- [ ] **Step 2: Write the failing CRUD test**

Append to `db.rs` `#[cfg(test)] mod tests`:
```rust
#[test]
fn workspace_crud_roundtrips() {
    let db = Db::open_in_memory().unwrap();
    assert!(db.list_workspaces().unwrap().is_empty());
    let w = db.create_workspace("Legal", "case work").unwrap();
    assert_eq!(w.name, "Legal");
    let all = db.list_workspaces().unwrap();
    assert_eq!(all.len(), 1);
    let renamed = db.rename_workspace(&w.id, "Legal research").unwrap();
    assert_eq!(renamed.name, "Legal research");
    db.delete_workspace(&w.id).unwrap();
    assert!(db.list_workspaces().unwrap().is_empty());
}

#[test]
fn seed_inserts_one_default_workspace_only_when_empty() {
    let db = Db::open_in_memory().unwrap();
    db.seed_defaults().unwrap();
    assert_eq!(db.list_workspaces().unwrap().len(), 1);
    db.seed_defaults().unwrap(); // idempotent
    assert_eq!(db.list_workspaces().unwrap().len(), 1);
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p manch-desktop db::tests::workspace_crud_roundtrips`
Expected: FAIL (method not found).

- [ ] **Step 4: Implement schema + CRUD + id helper**

In `db.rs`, add the table to `init`:
```rust
conn.execute(
    "CREATE TABLE IF NOT EXISTS workspaces (
         id          TEXT PRIMARY KEY,
         name        TEXT NOT NULL,
         description TEXT NOT NULL DEFAULT ''
     )",
    [],
)?;
```
Add an id generator (monotonic-ish, no extra deps) and methods:
```rust
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static SEQ: AtomicU64 = AtomicU64::new(0);

fn new_id(prefix: &str) -> String {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let t = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("{prefix}{t:x}{n:x}")
}

impl Db {
    pub fn list_workspaces(&self) -> rusqlite::Result<Vec<manch_dto::Workspace>> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, name, description FROM workspaces ORDER BY name")?;
        let rows = stmt.query_map([], |r| Ok(manch_dto::Workspace {
            id: r.get(0)?, name: r.get(1)?, description: r.get(2)?,
        }))?;
        rows.collect()
    }

    pub fn create_workspace(&self, name: &str, description: &str) -> rusqlite::Result<manch_dto::Workspace> {
        let id = new_id("ws_");
        let conn = self.0.lock().unwrap();
        conn.execute(
            "INSERT INTO workspaces (id, name, description) VALUES (?1, ?2, ?3)",
            rusqlite::params![id, name, description],
        )?;
        Ok(manch_dto::Workspace { id, name: name.into(), description: description.into() })
    }

    pub fn rename_workspace(&self, id: &str, name: &str) -> rusqlite::Result<manch_dto::Workspace> {
        let conn = self.0.lock().unwrap();
        conn.execute("UPDATE workspaces SET name = ?2 WHERE id = ?1", rusqlite::params![id, name])?;
        conn.query_row("SELECT id, name, description FROM workspaces WHERE id = ?1", [id], |r| {
            Ok(manch_dto::Workspace { id: r.get(0)?, name: r.get(1)?, description: r.get(2)? })
        })
    }

    pub fn delete_workspace(&self, id: &str) -> rusqlite::Result<()> {
        let conn = self.0.lock().unwrap();
        conn.execute("DELETE FROM workspaces WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn seed_defaults(&self) -> rusqlite::Result<()> {
        if self.list_workspaces()?.is_empty() {
            self.create_workspace("My workspace", "Default workspace")?;
        }
        Ok(())
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p manch-desktop db::tests`
Expected: PASS (existing + 2 new).

- [ ] **Step 6: Call `seed_defaults` on startup**

In `lib.rs` `.setup(...)`, after `app.manage(db)` — actually seed before manage:
```rust
db.seed_defaults().expect("seed defaults");
app.manage(db);
```

- [ ] **Step 7: clippy + fmt**

Run: `cargo fmt --all && cargo clippy -p manch-desktop --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 8: Commit**

```bash
git add apps/desktop/src-tauri Cargo.lock
git commit -m "feat(desktop): workspaces SQLite schema, CRUD, and default seed"
```

---

### Task 9: Workspace Tauri commands

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands.rs`, `apps/desktop/src-tauri/src/lib.rs`

**Interfaces:**
- Produces (Tauri commands, all `Result<_, String>`):
```rust
list_workspaces(state) -> Vec<Workspace>
create_workspace(state, input: CreateWorkspace) -> Workspace
rename_workspace(state, id: String, name: String) -> Workspace
delete_workspace(state, id: String) -> ()
```

- [ ] **Step 1: Add commands in `commands.rs`**

```rust
use manch_dto::{CreateWorkspace, Workspace};

#[tauri::command]
pub fn list_workspaces(state: State<Db>) -> Result<Vec<Workspace>, String> {
    state.list_workspaces().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_workspace(state: State<Db>, input: CreateWorkspace) -> Result<Workspace, String> {
    state.create_workspace(&input.name, &input.description).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn rename_workspace(state: State<Db>, id: String, name: String) -> Result<Workspace, String> {
    state.rename_workspace(&id, &name).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_workspace(state: State<Db>, id: String) -> Result<(), String> {
    state.delete_workspace(&id).map_err(|e| e.to_string())
}
```

- [ ] **Step 2: Register in `lib.rs` `generate_handler!`**

```rust
.invoke_handler(tauri::generate_handler![
    commands::save_api_key,
    commands::list_configured_providers,
    commands::send_prompt,
    commands::list_workspaces,
    commands::create_workspace,
    commands::rename_workspace,
    commands::delete_workspace,
])
```

- [ ] **Step 3: Verify it compiles + clippy**

Run: `cargo clippy -p manch-desktop --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add apps/desktop/src-tauri/src
git commit -m "feat(desktop): workspace Tauri commands (list/create/rename/delete)"
```

---

### Task 10: Team / Schedule / Search / CrossVerify DTOs + regen

**Files:**
- Modify: `crates/manch-dto/src/lib.rs`, `crates/manch-dto/src/bin/gen-types.rs`

**Interfaces:**
- Produces (Rust + generated TS):
```rust
pub struct TeamMember { pub role: String, pub provider: String }
pub struct Team { pub id: String, pub workspace_id: String, pub name: String, pub problem: String, pub members: Vec<TeamMember>, pub capabilities: Vec<String> }
pub struct CreateTeam { pub workspace_id: String, pub name: String, pub problem: String, pub auto: bool, pub members: Vec<TeamMember> }
pub struct RunStep { pub member_role: String, pub detail: String, pub status: String } // "running"|"done"|"error"
pub struct TeamRun { pub team_id: String, pub task: String, pub steps: Vec<RunStep>, pub result: String }
pub struct Schedule { pub id: String, pub workspace_id: String, pub target: String, pub cadence: String, pub next_run: String }
pub struct CreateSchedule { pub workspace_id: String, pub target: String, pub cadence: String, pub next_run: String }
pub struct SearchHit { pub kind: String, pub id: String, pub title: String, pub snippet: String }
pub struct Report { pub provider: String, pub text: String }
pub struct CrossVerify { pub reports: Vec<Report>, pub summary: String }
```

- [ ] **Step 1: Add the structs to `lib.rs`**

Add each struct with the same derive pattern:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct TeamMember { pub role: String, pub provider: String }
```
(repeat the `#[derive(...)] #[cfg_attr(...)]` header for every struct above).

- [ ] **Step 2: Register them in `gen-types.rs` `declarations()`**

Add an `export_to_string` line for every new type (nested types like `TeamMember`, `RunStep`, `Report` MUST be listed so they're defined in the file):
```rust
TeamMember::export_to_string(&cfg).expect("export TeamMember"),
Team::export_to_string(&cfg).expect("export Team"),
CreateTeam::export_to_string(&cfg).expect("export CreateTeam"),
RunStep::export_to_string(&cfg).expect("export RunStep"),
TeamRun::export_to_string(&cfg).expect("export TeamRun"),
Schedule::export_to_string(&cfg).expect("export Schedule"),
CreateSchedule::export_to_string(&cfg).expect("export CreateSchedule"),
SearchHit::export_to_string(&cfg).expect("export SearchHit"),
Report::export_to_string(&cfg).expect("export Report"),
CrossVerify::export_to_string(&cfg).expect("export CrossVerify"),
```

- [ ] **Step 3: Regenerate + inspect**

Run: `cargo run -p manch-dto --features ts --bin gen-types`
Expected: `bindings.ts` now contains all the new `export type` declarations, each defined exactly once.

- [ ] **Step 4: clippy + fmt**

Run: `cargo fmt --all && cargo clippy -p manch-dto --all-features --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add crates/manch-dto/src
git commit -m "feat(dto): add team/schedule/search/cross-verify DTOs"
```

---

### Task 11: Teams persistence + commands

**Files:**
- Modify: `apps/desktop/src-tauri/src/db.rs`, `commands.rs`, `lib.rs`

**Interfaces:**
- Produces (on `Db`): `list_teams(workspace_id) -> Vec<Team>`, `create_team(CreateTeam) -> Team`, `get_team(id) -> Option<Team>`.
- Produces (commands): `list_teams`, `create_team`, `get_team`, `assign_team_task(team_id, task) -> TeamRun` (computed, not stored).
- Storage: a `teams` table; `members` and `capabilities` serialized as JSON text columns.

- [ ] **Step 1: Write the failing DB test**

```rust
#[test]
fn team_crud_with_members_roundtrips() {
    let db = Db::open_in_memory().unwrap();
    let ws = db.create_workspace("w", "").unwrap();
    let input = manch_dto::CreateTeam {
        workspace_id: ws.id.clone(),
        name: "Discovery".into(),
        problem: "find precedent".into(),
        auto: false,
        members: vec![manch_dto::TeamMember { role: "researcher".into(), provider: "anthropic".into() }],
    };
    let team = db.create_team(input).unwrap();
    assert_eq!(team.members.len(), 1);
    let got = db.get_team(&team.id).unwrap().unwrap();
    assert_eq!(got.name, "Discovery");
    assert_eq!(db.list_teams(&ws.id).unwrap().len(), 1);
}
```

- [ ] **Step 2: Run to verify fail**

Run: `cargo test -p manch-desktop db::tests::team_crud_with_members_roundtrips`
Expected: FAIL.

- [ ] **Step 3: Implement table + CRUD**

Add to `init`:
```rust
conn.execute(
    "CREATE TABLE IF NOT EXISTS teams (
         id           TEXT PRIMARY KEY,
         workspace_id TEXT NOT NULL,
         name         TEXT NOT NULL,
         problem      TEXT NOT NULL DEFAULT '',
         members      TEXT NOT NULL DEFAULT '[]',
         capabilities TEXT NOT NULL DEFAULT '[]'
     )",
    [],
)?;
```
Add methods (use `serde_json` — add `serde_json = { workspace = true }` to `src-tauri/Cargo.toml` if not present; it is already a dep). When `input.auto` is true, synthesize members + capabilities; otherwise use the provided members:
```rust
pub fn create_team(&self, input: manch_dto::CreateTeam) -> rusqlite::Result<manch_dto::Team> {
    let id = new_id("tm_");
    let members = if input.auto && input.members.is_empty() {
        vec![
            manch_dto::TeamMember { role: "Researcher".into(), provider: "anthropic".into() },
            manch_dto::TeamMember { role: "Analyst".into(), provider: "anthropic".into() },
            manch_dto::TeamMember { role: "Critic".into(), provider: "claude-code".into() },
        ]
    } else {
        input.members
    };
    let capabilities = vec!["read_file".to_string(), "search".to_string(), "write_report".to_string()];
    let members_json = serde_json::to_string(&members).unwrap();
    let caps_json = serde_json::to_string(&capabilities).unwrap();
    let conn = self.0.lock().unwrap();
    conn.execute(
        "INSERT INTO teams (id, workspace_id, name, problem, members, capabilities) VALUES (?1,?2,?3,?4,?5,?6)",
        rusqlite::params![id, input.workspace_id, input.name, input.problem, members_json, caps_json],
    )?;
    Ok(manch_dto::Team { id, workspace_id: input.workspace_id, name: input.name, problem: input.problem, members, capabilities })
}
```
Add `list_teams` and `get_team` that deserialize the JSON columns (factor a `row_to_team` helper). Follow the workspace pattern.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p manch-desktop db::tests::team_crud_with_members_roundtrips`
Expected: PASS.

- [ ] **Step 5: Add the commands (incl. computed `assign_team_task`)**

```rust
use manch_dto::{CreateTeam, RunStep, Team, TeamRun};

#[tauri::command]
pub fn list_teams(state: State<Db>, workspace_id: String) -> Result<Vec<Team>, String> {
    state.list_teams(&workspace_id).map_err(|e| e.to_string())
}
#[tauri::command]
pub fn create_team(state: State<Db>, input: CreateTeam) -> Result<Team, String> {
    state.create_team(input).map_err(|e| e.to_string())
}
#[tauri::command]
pub fn get_team(state: State<Db>, id: String) -> Result<Team, String> {
    state.get_team(&id).map_err(|e| e.to_string())?.ok_or_else(|| "team not found".into())
}
#[tauri::command]
pub fn assign_team_task(state: State<Db>, team_id: String, task: String) -> Result<TeamRun, String> {
    let team = state.get_team(&team_id).map_err(|e| e.to_string())?.ok_or("team not found")?;
    let steps = team.members.iter().map(|m| RunStep {
        member_role: m.role.clone(),
        detail: format!("{} handled part of: {task}", m.role),
        status: "done".into(),
    }).collect();
    Ok(TeamRun { team_id, task, steps, result: "Synthesized result from the team (mock).".into() })
}
```
Register all four in `lib.rs`.

- [ ] **Step 6: clippy + fmt + commit**

Run: `cargo fmt --all && cargo clippy -p manch-desktop --all-targets -- -D warnings`
```bash
git add apps/desktop/src-tauri Cargo.lock
git commit -m "feat(desktop): teams persistence + commands incl. mock assign_team_task"
```

---

### Task 12: Schedules persistence + commands

**Files:** Modify `db.rs`, `commands.rs`, `lib.rs`.

**Interfaces:** `list_schedules(workspace_id) -> Vec<Schedule>`, `create_schedule(CreateSchedule) -> Schedule`; commands `list_schedules`, `create_schedule`.

- [ ] **Step 1: Failing DB test**

```rust
#[test]
fn schedule_crud_roundtrips() {
    let db = Db::open_in_memory().unwrap();
    let ws = db.create_workspace("w", "").unwrap();
    let s = db.create_schedule(manch_dto::CreateSchedule {
        workspace_id: ws.id.clone(), target: "Discovery team".into(),
        cadence: "daily".into(), next_run: "2026-07-01T09:00:00Z".into(),
    }).unwrap();
    assert_eq!(s.cadence, "daily");
    assert_eq!(db.list_schedules(&ws.id).unwrap().len(), 1);
}
```

- [ ] **Step 2: Run to verify fail** — `cargo test -p manch-desktop db::tests::schedule_crud_roundtrips` → FAIL.

- [ ] **Step 3: Implement table + CRUD**

```rust
conn.execute(
    "CREATE TABLE IF NOT EXISTS schedules (
         id           TEXT PRIMARY KEY,
         workspace_id TEXT NOT NULL,
         target       TEXT NOT NULL,
         cadence      TEXT NOT NULL,
         next_run     TEXT NOT NULL
     )",
    [],
)?;
```
Add `create_schedule` / `list_schedules` following the workspace pattern (map columns → `manch_dto::Schedule`).

- [ ] **Step 4: Run to verify pass** — PASS.

- [ ] **Step 5: Commands + register**

```rust
use manch_dto::{CreateSchedule, Schedule};
#[tauri::command]
pub fn list_schedules(state: State<Db>, workspace_id: String) -> Result<Vec<Schedule>, String> {
    state.list_schedules(&workspace_id).map_err(|e| e.to_string())
}
#[tauri::command]
pub fn create_schedule(state: State<Db>, input: CreateSchedule) -> Result<Schedule, String> {
    state.create_schedule(input).map_err(|e| e.to_string())
}
```
Register both in `lib.rs`.

- [ ] **Step 6: clippy + fmt + commit**

```bash
cargo fmt --all && cargo clippy -p manch-desktop --all-targets -- -D warnings
git add apps/desktop/src-tauri Cargo.lock
git commit -m "feat(desktop): schedules persistence + commands"
```

---

### Task 13: `search` + `cross_verify` commands (computed)

**Files:** Modify `commands.rs`, `lib.rs`. (No new tables; `search` queries stored workspaces/teams/schedules.)

**Interfaces:** `search(workspace_id, query, kinds: Vec<String>) -> Vec<SearchHit>`; `cross_verify(providers: Vec<String>, text: String) -> CrossVerify`.

- [ ] **Step 1: Add a `Db::search` helper** (matches name/problem/target against teams + schedules in the workspace)

```rust
pub fn search(&self, workspace_id: &str, query: &str, kinds: &[String]) -> rusqlite::Result<Vec<manch_dto::SearchHit>> {
    let q = query.to_lowercase();
    let mut hits = Vec::new();
    let want = |k: &str| kinds.is_empty() || kinds.iter().any(|x| x == k);
    if want("team") {
        for t in self.list_teams(workspace_id)? {
            if t.name.to_lowercase().contains(&q) || t.problem.to_lowercase().contains(&q) {
                hits.push(manch_dto::SearchHit { kind: "team".into(), id: t.id, title: t.name, snippet: t.problem });
            }
        }
    }
    if want("schedule") {
        for s in self.list_schedules(workspace_id)? {
            if s.target.to_lowercase().contains(&q) {
                hits.push(manch_dto::SearchHit { kind: "schedule".into(), id: s.id, title: s.target, snippet: s.cadence });
            }
        }
    }
    Ok(hits)
}
```
Add a `#[cfg(test)]` test asserting a created team is found by a query substring.

- [ ] **Step 2: Run the search test** — `cargo test -p manch-desktop db::tests` → PASS.

- [ ] **Step 3: Commands**

```rust
use manch_dto::{CrossVerify, Report, SearchHit};
#[tauri::command]
pub fn search(state: State<Db>, workspace_id: String, query: String, kinds: Vec<String>) -> Result<Vec<SearchHit>, String> {
    state.search(&workspace_id, &query, &kinds).map_err(|e| e.to_string())
}
#[tauri::command]
pub fn cross_verify(providers: Vec<String>, text: String) -> Result<CrossVerify, String> {
    if providers.is_empty() { return Err("select at least one provider".into()); }
    let reports = providers.iter().map(|p| Report {
        provider: p.clone(),
        text: format!("**{p}** analysis of “{text}”: (mock) the claim appears consistent."),
    }).collect();
    Ok(CrossVerify { reports, summary: format!("{} providers broadly agree (mock synthesis).", providers.len()) })
}
```
Register both in `lib.rs`.

- [ ] **Step 4: clippy + fmt + commit**

```bash
cargo fmt --all && cargo clippy -p manch-desktop --all-targets -- -D warnings
git add apps/desktop/src-tauri
git commit -m "feat(desktop): search + cross_verify commands (computed mock)"
```

---

### Task 14: TS command wrappers + React Query hooks

**Files:**
- Modify: `apps/desktop/src/lib/api.ts` (wrappers using generated types; unify provider list)
- Modify: `apps/desktop/src/lib/providers.ts` (single source) 
- Modify: `apps/desktop/src/data/queries.ts` (hooks)
- Delete: dead `PROVIDERS` duplication
- Test: `apps/desktop/src/data/queries.test.tsx` (hook smoke test with mocked invoke)

**Interfaces:**
- Consumes: generated `bindings.ts` types; `@tauri-apps/api/core` `invoke`.
- Produces: `useWorkspaces()`, `useCreateWorkspace()`, `useTeams(workspaceId)`, `useCreateTeam()`, `useSchedules(workspaceId)`, `useCreateSchedule()`, `useSearch(...)`, `useCrossVerify()` plus the existing provider hooks.

- [ ] **Step 1: Unify the provider list in `providers.ts`**

```ts
import type { ProviderOption } from "@manch/ui";

export const PROVIDERS = [
  { id: "anthropic", label: "Anthropic (Claude · BYOK)" },
  { id: "claude-code", label: "Claude Code (ACP)" },
] as const;

export type Provider = (typeof PROVIDERS)[number]["id"];
export const ALL_PROVIDERS: ProviderOption[] = PROVIDERS.map((p) => ({ id: p.id, label: p.label }));
```

- [ ] **Step 2: Rewrite `lib/api.ts` to import the unified provider + generated types, drop the `as never` casts**

```ts
import { invoke } from "@tauri-apps/api/core";
import type { Provider } from "./providers";
import type { Workspace, CreateWorkspace, Team, CreateTeam, TeamRun, Schedule, CreateSchedule, SearchHit, CrossVerify } from "../data/bindings";

export const saveApiKey = (provider: Provider, apiKey: string): Promise<void> =>
  invoke("save_api_key", { provider, apiKey });
export const listConfiguredProviders = (): Promise<Provider[]> => invoke("list_configured_providers");
export const sendPrompt = (provider: Provider, text: string): Promise<string> => invoke("send_prompt", { provider, text });

export const listWorkspaces = (): Promise<Workspace[]> => invoke("list_workspaces");
export const createWorkspace = (input: CreateWorkspace): Promise<Workspace> => invoke("create_workspace", { input });
export const renameWorkspace = (id: string, name: string): Promise<Workspace> => invoke("rename_workspace", { id, name });
export const deleteWorkspace = (id: string): Promise<void> => invoke("delete_workspace", { id });

export const listTeams = (workspaceId: string): Promise<Team[]> => invoke("list_teams", { workspaceId });
export const createTeam = (input: CreateTeam): Promise<Team> => invoke("create_team", { input });
export const getTeam = (id: string): Promise<Team> => invoke("get_team", { id });
export const assignTeamTask = (teamId: string, task: string): Promise<TeamRun> => invoke("assign_team_task", { teamId, task });

export const listSchedules = (workspaceId: string): Promise<Schedule[]> => invoke("list_schedules", { workspaceId });
export const createSchedule = (input: CreateSchedule): Promise<Schedule> => invoke("create_schedule", { input });

export const search = (workspaceId: string, query: string, kinds: string[]): Promise<SearchHit[]> =>
  invoke("search", { workspaceId, query, kinds });
export const crossVerify = (providers: string[], text: string): Promise<CrossVerify> =>
  invoke("cross_verify", { providers, text });
```

- [ ] **Step 3: Write the failing hook test**

```tsx
// queries.test.tsx
import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { describe, it, expect, vi, beforeEach } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

import { useWorkspaces } from "./queries";

const wrapper = ({ children }: { children: React.ReactNode }) => {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
};

describe("useWorkspaces", () => {
  beforeEach(() => invoke.mockReset());
  it("returns workspaces from the command", async () => {
    invoke.mockResolvedValueOnce([{ id: "w1", name: "W", description: "" }]);
    const { result } = renderHook(() => useWorkspaces(), { wrapper });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data?.[0].name).toBe("W");
    expect(invoke).toHaveBeenCalledWith("list_workspaces");
  });
});
```

- [ ] **Step 4: Run to verify fail** — `pnpm --filter @manch/desktop exec vitest run src/data/queries.test.tsx` → FAIL (`useWorkspaces` missing).

- [ ] **Step 5: Add hooks to `queries.ts`**

```ts
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import * as api from "../lib/api";
import type { CreateWorkspace, CreateTeam, CreateSchedule } from "./bindings";

export function useConfiguredProviders() {
  return useQuery({ queryKey: ["providers"], queryFn: api.listConfiguredProviders });
}
export function useSaveApiKey() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ provider, apiKey }: { provider: import("../lib/providers").Provider; apiKey: string }) =>
      api.saveApiKey(provider, apiKey),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["providers"] }),
  });
}

export function useWorkspaces() {
  return useQuery({ queryKey: ["workspaces"], queryFn: api.listWorkspaces });
}
export function useCreateWorkspace() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: CreateWorkspace) => api.createWorkspace(input),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["workspaces"] }),
  });
}
export function useTeams(workspaceId: string | null) {
  return useQuery({ queryKey: ["teams", workspaceId], queryFn: () => api.listTeams(workspaceId!), enabled: !!workspaceId });
}
export function useCreateTeam() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: CreateTeam) => api.createTeam(input),
    onSuccess: (_d, input) => qc.invalidateQueries({ queryKey: ["teams", input.workspace_id] }),
  });
}
export function useSchedules(workspaceId: string | null) {
  return useQuery({ queryKey: ["schedules", workspaceId], queryFn: () => api.listSchedules(workspaceId!), enabled: !!workspaceId });
}
export function useCreateSchedule() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: CreateSchedule) => api.createSchedule(input),
    onSuccess: (_d, input) => qc.invalidateQueries({ queryKey: ["schedules", input.workspace_id] }),
  });
}
```
(Add `renameWorkspace`/`deleteWorkspace` mutations, `useSearch`, `useCrossVerify` following the same shape — `useSearch` is a `useMutation` or `useQuery` keyed on `[workspaceId, query, kinds]` with `enabled` on non-empty query; `useCrossVerify` is a mutation.)

- [ ] **Step 6: Run to verify pass** — PASS.

- [ ] **Step 7: Typecheck (confirms generated `bindings.ts` exists & types line up)**

Run: `just gen && pnpm --filter @manch/desktop lint`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add apps/desktop/src
git commit -m "feat(desktop): typed Tauri command wrappers + React Query hooks over generated DTOs"
```

---

## Phase C — Settings page + workspace wiring

### Task 15: `ProviderSettings` component (TanStack Form)

**Files:**
- Modify: `packages/ui/package.json` (add `@tanstack/react-form`)
- Create: `packages/ui/src/settings/ProviderSettings.tsx`, `.test.tsx`, `.stories.tsx`
- Modify: `packages/ui/src/index.ts`

**Interfaces:**
- Produces:
```ts
export interface ProviderSettingsProps {
  all: { id: string; label: string }[];
  configured: string[];
  onSave: (provider: string, apiKey: string) => void;
  onRemove?: (provider: string) => void;
  saving?: boolean;
}
export function ProviderSettings(props: ProviderSettingsProps): JSX.Element
```

- [ ] **Step 1: Add the dependency**

```bash
pnpm --filter @manch/ui add @tanstack/react-form
```

- [ ] **Step 2: Write the failing test**

```tsx
// ProviderSettings.test.tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { ProviderSettings } from "./ProviderSettings";

const all = [{ id: "anthropic", label: "Anthropic" }, { id: "claude-code", label: "Claude Code" }];

describe("ProviderSettings", () => {
  it("lists configured providers", () => {
    render(<ProviderSettings all={all} configured={["anthropic"]} onSave={() => {}} />);
    expect(screen.getByText(/Anthropic/)).toBeTruthy();
    expect(screen.getByText(/configured/i)).toBeTruthy();
  });

  it("submits a provider + key", async () => {
    const onSave = vi.fn();
    render(<ProviderSettings all={all} configured={[]} onSave={onSave} />);
    fireEvent.change(screen.getByLabelText(/api key/i), { target: { value: "sk-test" } });
    fireEvent.click(screen.getByRole("button", { name: /save key/i }));
    await screen.findByRole("button", { name: /save key/i });
    expect(onSave).toHaveBeenCalledWith("anthropic", "sk-test");
  });
});
```

- [ ] **Step 3: Run to verify fail** — `pnpm --filter @manch/ui exec vitest run src/settings/ProviderSettings.test.tsx` → FAIL.

- [ ] **Step 4: Implement with TanStack Form**

```tsx
// ProviderSettings.tsx
import { useForm } from "@tanstack/react-form";

export interface ProviderSettingsProps {
  all: { id: string; label: string }[];
  configured: string[];
  onSave: (provider: string, apiKey: string) => void;
  onRemove?: (provider: string) => void;
  saving?: boolean;
}

export function ProviderSettings({ all, configured, onSave, onRemove, saving }: ProviderSettingsProps) {
  const form = useForm({
    defaultValues: { provider: all[0]?.id ?? "", apiKey: "" },
    onSubmit: ({ value }) => onSave(value.provider, value.apiKey),
  });
  return (
    <section className="space-y-4">
      <h3 className="text-sm font-medium text-base-content">Providers</h3>
      <ul className="space-y-1">
        {all.map((p) => (
          <li key={p.id} className="flex items-center justify-between rounded-box border border-base-300 px-3 py-2">
            <span>{p.label}</span>
            {configured.includes(p.id) ? (
              <span className="flex items-center gap-2">
                <span className="badge badge-success badge-sm">configured</span>
                {onRemove && <button className="btn btn-ghost btn-xs" onClick={() => onRemove(p.id)}>Remove</button>}
              </span>
            ) : (
              <span className="badge badge-ghost badge-sm">not set</span>
            )}
          </li>
        ))}
      </ul>
      <form onSubmit={(e) => { e.preventDefault(); form.handleSubmit(); }} className="flex flex-wrap items-end gap-2">
        <form.Field name="provider">
          {(f) => (
            <label className="form-control">
              <span className="label-text">Provider</span>
              <select className="select select-bordered select-sm" value={f.state.value} onChange={(e) => f.handleChange(e.target.value)}>
                {all.map((p) => <option key={p.id} value={p.id}>{p.label}</option>)}
              </select>
            </label>
          )}
        </form.Field>
        <form.Field name="apiKey">
          {(f) => (
            <label className="form-control">
              <span className="label-text">API key</span>
              <input aria-label="API key" type="password" className="input input-bordered input-sm" value={f.state.value} onChange={(e) => f.handleChange(e.target.value)} />
            </label>
          )}
        </form.Field>
        <button type="submit" className="btn btn-primary btn-sm" disabled={saving}>Save key</button>
      </form>
    </section>
  );
}
```

- [ ] **Step 5: Run to verify pass** — PASS.

- [ ] **Step 6: Story + export**

```tsx
// ProviderSettings.stories.tsx
import type { Meta, StoryObj } from "@storybook/react";
import { ProviderSettings } from "./ProviderSettings";
const meta: Meta<typeof ProviderSettings> = { title: "settings/ProviderSettings", component: ProviderSettings };
export default meta;
type Story = StoryObj<typeof ProviderSettings>;
const all = [{ id: "anthropic", label: "Anthropic" }, { id: "claude-code", label: "Claude Code" }];
export const None: Story = { args: { all, configured: [], onSave: () => {} } };
export const SomeConfigured: Story = { args: { all, configured: ["anthropic"], onSave: () => {}, onRemove: () => {} } };
```
Export `ProviderSettings` + props type from `index.ts`.

- [ ] **Step 7: Commit**

```bash
git add packages/ui/package.json packages/ui/src/settings/ProviderSettings.* packages/ui/src/index.ts pnpm-lock.yaml
git commit -m "feat(ui): ProviderSettings with TanStack Form"
```

---

### Task 16: `WorkspaceSettings` component

**Files:** Create `packages/ui/src/settings/WorkspaceSettings.tsx`, `.test.tsx`, `.stories.tsx`; modify `index.ts`.

**Interfaces:**
```ts
export interface WorkspaceSettingsProps {
  workspaces: { id: string; name: string }[];
  onRename: (id: string, name: string) => void;
  onDelete: (id: string) => void;
}
export function WorkspaceSettings(props: WorkspaceSettingsProps): JSX.Element
```

- [ ] **Step 1: Failing test**

```tsx
// WorkspaceSettings.test.tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { WorkspaceSettings } from "./WorkspaceSettings";

describe("WorkspaceSettings", () => {
  it("renames a workspace", () => {
    const onRename = vi.fn();
    render(<WorkspaceSettings workspaces={[{ id: "w1", name: "Old" }]} onRename={onRename} onDelete={() => {}} />);
    const input = screen.getByDisplayValue("Old");
    fireEvent.change(input, { target: { value: "New" } });
    fireEvent.click(screen.getByRole("button", { name: /save/i }));
    expect(onRename).toHaveBeenCalledWith("w1", "New");
  });
  it("deletes a workspace", () => {
    const onDelete = vi.fn();
    render(<WorkspaceSettings workspaces={[{ id: "w1", name: "Old" }]} onRename={() => {}} onDelete={onDelete} />);
    fireEvent.click(screen.getByRole("button", { name: /delete/i }));
    expect(onDelete).toHaveBeenCalledWith("w1");
  });
});
```

- [ ] **Step 2: Run to verify fail** — FAIL.

- [ ] **Step 3: Implement**

```tsx
// WorkspaceSettings.tsx
import { useState } from "react";

export interface WorkspaceSettingsProps {
  workspaces: { id: string; name: string }[];
  onRename: (id: string, name: string) => void;
  onDelete: (id: string) => void;
}

export function WorkspaceSettings({ workspaces, onRename, onDelete }: WorkspaceSettingsProps) {
  return (
    <section className="space-y-3">
      <h3 className="text-sm font-medium text-base-content">Workspaces</h3>
      <ul className="space-y-2">
        {workspaces.map((w) => <Row key={w.id} w={w} onRename={onRename} onDelete={onDelete} />)}
      </ul>
    </section>
  );
}

function Row({ w, onRename, onDelete }: { w: { id: string; name: string }; onRename: (id: string, n: string) => void; onDelete: (id: string) => void }) {
  const [name, setName] = useState(w.name);
  return (
    <li className="flex items-center gap-2">
      <input className="input input-bordered input-sm flex-1" value={name} onChange={(e) => setName(e.target.value)} />
      <button className="btn btn-sm" onClick={() => onRename(w.id, name)}>Save</button>
      <button className="btn btn-ghost btn-sm text-error" onClick={() => onDelete(w.id)}>Delete</button>
    </li>
  );
}
```

- [ ] **Step 4: Run to verify pass** — PASS.

- [ ] **Step 5: Story + export**, then **Step 6: Commit**

```bash
git add packages/ui/src/settings/WorkspaceSettings.* packages/ui/src/index.ts
git commit -m "feat(ui): WorkspaceSettings (rename/delete)"
```

---

### Task 17: `SettingsView` composition

**Files:** Create `packages/ui/src/settings/SettingsView.tsx`, `.test.tsx`, `.stories.tsx`; modify `index.ts`.

**Interfaces:**
```ts
export interface SettingsViewProps {
  providers: React.ReactNode; // composed ProviderSettings element
  theme: React.ReactNode;     // composed ThemePicker element
  workspaces: React.ReactNode;// composed WorkspaceSettings element
}
export function SettingsView(props: SettingsViewProps): JSX.Element
```
Rationale: `SettingsView` is layout-only (sections + headings); the connected pieces are composed in the container so `@manch/ui` stays prop-driven.

- [ ] **Step 1: Failing test**

```tsx
import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { SettingsView } from "./SettingsView";

describe("SettingsView", () => {
  it("renders all three sections", () => {
    render(<SettingsView providers={<div>P</div>} theme={<div>T</div>} workspaces={<div>W</div>} />);
    expect(screen.getByText("P")).toBeTruthy();
    expect(screen.getByText("T")).toBeTruthy();
    expect(screen.getByText("W")).toBeTruthy();
  });
});
```

- [ ] **Step 2: Run to verify fail** — FAIL.

- [ ] **Step 3: Implement**

```tsx
// SettingsView.tsx
import type { ReactNode } from "react";

export interface SettingsViewProps {
  providers: ReactNode;
  theme: ReactNode;
  workspaces: ReactNode;
}

export function SettingsView({ providers, theme, workspaces }: SettingsViewProps) {
  return (
    <div className="mx-auto max-w-2xl space-y-8 overflow-y-auto p-6">
      <h1 className="text-xl font-semibold">Settings</h1>
      <div className="rounded-box border border-base-300 p-4">{providers}</div>
      <div className="rounded-box border border-base-300 p-4">{theme}</div>
      <div className="rounded-box border border-base-300 p-4">{workspaces}</div>
    </div>
  );
}
```

- [ ] **Step 4: Run to verify pass** — PASS. **Step 5: story + export. Step 6: commit**

```bash
git add packages/ui/src/settings/SettingsView.* packages/ui/src/index.ts
git commit -m "feat(ui): SettingsView layout composition"
```

---

### Task 18: Settings route + container wiring

**Files:**
- Modify: `apps/desktop/src/routes/settings.tsx` (replace placeholder)
- Create: `apps/desktop/src/containers/SettingsPage.tsx`
- Test: `apps/desktop/src/containers/SettingsPage.test.tsx`

**Interfaces:**
- Consumes: `SettingsView`, `ProviderSettings`, `ThemePicker`, `WorkspaceSettings` from `@manch/ui`; hooks from `data/queries`; `themeAtom`, `THEMES`, `activeWorkspaceIdAtom` from store; `ALL_PROVIDERS` from `lib/providers`.

- [ ] **Step 1: Write the container test (provider list + theme switch)**

```tsx
// SettingsPage.test.tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Provider as JotaiProvider } from "jotai";
import { describe, it, expect, vi, beforeEach } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
import SettingsPage from "./SettingsPage";

const wrap = (ui: React.ReactNode) => {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={qc}><JotaiProvider>{ui}</JotaiProvider></QueryClientProvider>;
};

describe("SettingsPage", () => {
  beforeEach(() => { invoke.mockReset(); localStorage.clear(); });
  it("switches theme via the picker", async () => {
    invoke.mockResolvedValue([]); // providers + workspaces queries
    render(wrap(<SettingsPage />));
    const dracula = await screen.findByRole("radio", { name: "dracula" });
    fireEvent.click(dracula);
    expect(document.documentElement.getAttribute("data-theme")).toBe("dracula");
  });
});
```
Note: the theme effect lives in the shell; for this test assert the atom write by reading `localStorage.getItem("manch.theme")` instead if the container doesn't itself set the attribute. Adjust the assertion to: `expect(localStorage.getItem("manch.theme")).toContain("dracula");`.

- [ ] **Step 2: Run to verify fail** — FAIL (`SettingsPage` missing).

- [ ] **Step 3: Implement the container**

```tsx
// SettingsPage.tsx
import { useAtom } from "jotai";
import { SettingsView, ProviderSettings, ThemePicker, WorkspaceSettings } from "@manch/ui";
import { themeAtom, THEMES } from "../store/atoms";
import { ALL_PROVIDERS } from "../lib/providers";
import {
  useConfiguredProviders, useSaveApiKey,
  useWorkspaces, useRenameWorkspace, useDeleteWorkspace,
} from "../data/queries";

export default function SettingsPage() {
  const [theme, setTheme] = useAtom(themeAtom);
  const providers = useConfiguredProviders();
  const save = useSaveApiKey();
  const workspaces = useWorkspaces();
  const rename = useRenameWorkspace();
  const del = useDeleteWorkspace();

  return (
    <SettingsView
      providers={
        <ProviderSettings
          all={ALL_PROVIDERS}
          configured={providers.data ?? []}
          saving={save.isPending}
          onSave={(provider, apiKey) => save.mutate({ provider: provider as never, apiKey })}
        />
      }
      theme={<ThemePicker themes={THEMES} active={theme} onSelect={setTheme} />}
      workspaces={
        <WorkspaceSettings
          workspaces={(workspaces.data ?? []).map((w) => ({ id: w.id, name: w.name }))}
          onRename={(id, name) => rename.mutate({ id, name })}
          onDelete={(id) => del.mutate(id)}
        />
      }
    />
  );
}
```
Add `useRenameWorkspace` / `useDeleteWorkspace` to `queries.ts` (mutations invalidating `["workspaces"]`).

- [ ] **Step 4: Wire the route**

Replace `apps/desktop/src/routes/settings.tsx`:
```tsx
import { createFileRoute } from "@tanstack/react-router";
import SettingsPage from "../containers/SettingsPage";

export const Route = createFileRoute("/settings")({ component: SettingsPage });
```

- [ ] **Step 5: Run to verify pass** — `pnpm --filter @manch/desktop exec vitest run src/containers/SettingsPage.test.tsx` → PASS.

- [ ] **Step 6: Typecheck + commit**

```bash
just gen && pnpm --filter @manch/desktop lint
git add apps/desktop/src
git commit -m "feat(desktop): Settings page wiring (providers, theme, workspaces)"
```

---

### Task 19: WorkspaceSwitcher container wiring (`WorkspaceBar`)

**Files:**
- Modify: `apps/desktop/src/containers/WorkspaceBar.tsx` (replace stub)
- Test: `apps/desktop/src/containers/WorkspaceBar.test.tsx`

**Interfaces:**
- Consumes: `useWorkspaces`, `useCreateWorkspace`; `activeWorkspaceIdAtom`; `WorkspaceSwitcher`.
- Behavior: lists workspaces; selecting sets the active atom; on first load with no active id, defaults to the first workspace; "+ New" creates `"New workspace"` and activates it.

- [ ] **Step 1: Write the test**

```tsx
// WorkspaceBar.test.tsx
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Provider as JotaiProvider } from "jotai";
import { describe, it, expect, vi, beforeEach } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
import WorkspaceBar from "./WorkspaceBar";

const wrap = (ui: React.ReactNode) => {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={qc}><JotaiProvider>{ui}</JotaiProvider></QueryClientProvider>;
};

describe("WorkspaceBar", () => {
  beforeEach(() => { invoke.mockReset(); localStorage.clear(); });
  it("shows workspaces and defaults the active one", async () => {
    invoke.mockResolvedValueOnce([{ id: "w1", name: "Alpha", description: "" }]);
    render(wrap(<WorkspaceBar />));
    await waitFor(() => expect(screen.getByRole("button", { name: /Alpha/ })).toBeTruthy());
  });
});
```

- [ ] **Step 2: Run to verify fail** — FAIL (stub renders empty switcher).

- [ ] **Step 3: Implement `WorkspaceBar`**

```tsx
// WorkspaceBar.tsx
import { useEffect } from "react";
import { useAtom } from "jotai";
import { WorkspaceSwitcher } from "@manch/ui";
import { activeWorkspaceIdAtom } from "../store/atoms";
import { useWorkspaces, useCreateWorkspace } from "../data/queries";

export default function WorkspaceBar() {
  const [activeId, setActiveId] = useAtom(activeWorkspaceIdAtom);
  const workspaces = useWorkspaces();
  const create = useCreateWorkspace();

  const list = workspaces.data ?? [];
  useEffect(() => {
    if (!activeId && list.length > 0) setActiveId(list[0].id);
  }, [activeId, list, setActiveId]);

  return (
    <WorkspaceSwitcher
      workspaces={list.map((w) => ({ id: w.id, name: w.name }))}
      activeId={activeId}
      onSelect={setActiveId}
      onCreate={() =>
        create.mutate(
          { name: "New workspace", description: "" },
          { onSuccess: (w) => setActiveId(w.id) },
        )
      }
    />
  );
}
```

- [ ] **Step 4: Run to verify pass** — PASS.

- [ ] **Step 5: Phase boundary — full CI**

Run: `just ci`
Expected: green (`gen` produces `bindings.ts`, clippy clean, all tests pass, build-js succeeds).

- [ ] **Step 6: Commit**

```bash
git add apps/desktop/src
git commit -m "feat(desktop): wire WorkspaceSwitcher to workspaces query + active atom"
```

---

## Self-Review

**Spec coverage:**
- Theme-agnostic (req 1) → Task 1 (config) + Task 5 (picker) + Task 6 (atom/effect) + Task 18 (wiring). ✓
- Interactive fake data via real Tauri commands (req 2) → Tasks 7–14 (DTO crate, SQLite CRUD, commands, hooks). ✓
- Sections Search/Schedule/Teams/Chat (req 3) → routing skeleton + data layer here (Tasks 6, 11–13); section UIs are **Plan 2**. ✓ (foundation)
- Proper Settings page replacing the test screen (req 4) → Tasks 15–18. ✓
- Gating when no AI (req 5) → data + provider hooks here; gating UI lands with Chat/Teams in **Plan 2**. (foundation ready)
- Multi-AI cross-verify (req 6) → `cross_verify` command + DTO here (Tasks 10, 13); UI in **Plan 2**.
- Workspaces (req 7) → Tasks 4, 6, 8, 9, 16, 18, 19. ✓
- ts-rs generation + manch-dto crate (refinement) → Tasks 7, 10. ✓

**Placeholder scan:** Section routes are intentional placeholders (`EmptyState`) handed off to Plan 2 — flagged explicitly, not hidden TODOs. Repetitive sibling tasks (12, 16) reference the established pattern but include their own full test + key code. No `TBD`.

**Type consistency:** TS command wrappers (Task 14) consume the exact generated names from Tasks 7/10 (`Workspace`, `CreateWorkspace`, `Team`, `CreateTeam`, `TeamRun`, `Schedule`, `CreateSchedule`, `SearchHit`, `CrossVerify`, `Report`). Hook names used in Tasks 18/19 (`useWorkspaces`, `useCreateWorkspace`, `useRenameWorkspace`, `useDeleteWorkspace`, `useConfiguredProviders`, `useSaveApiKey`) are all defined in Task 14/18. `new_id` prefixes (`ws_`, `tm_`) consistent. Provider union unified in Task 14.

**Open risk to watch during execution:** the exact `ts-rs` v12 API surface (`Config::default()` vs `Config::new()`, and whether `export_to_string` emits `import` lines) — Task 7 Step 4 notes the fallback; verify against the installed version when implementing.
