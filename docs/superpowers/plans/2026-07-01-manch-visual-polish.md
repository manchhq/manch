# Visual Polish (theater × kathputli) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the app its "theater × kathputli" signature through motion and structural nuance (a kathputli puppet loader, a small motion vocabulary, hero-surface polish) — all theme-agnostic — and expand the theme picker from 5 to all daisyUI built-ins.

**Architecture:** Signature look is carried by motion + structure, never color: daisyUI themes own the palette (component code uses only semantic tokens and `currentColor`). Motion lives as CSS keyframe utilities in `packages/ui/src/styles.css`, all `prefers-reduced-motion`-guarded. A new `PuppetLoader` primitive replaces busy/loading states. "Done" is subjective — the acceptance gate is user sign-off in Storybook plus objective checks (stories exist, no hardcoded colors, reduced-motion respected, `just ci` green).

**Tech Stack:** React 19, Tailwind v4 + daisyUI v5, Storybook, Vitest (`@manch/ui`).

## Global Constraints

- **No hardcoded colors.** Components use daisyUI semantic tokens (`text-base-content`, `bg-base-200`, `ring-primary`, …) and `currentColor` only. Verified by eyeballing ≥3 contrasting themes (dark, cupcake, dracula).
- **No new JS animation dependency.** Motion is CSS-first (keyframes/utilities). `@manch/ui` stays dependency-clean.
- **All motion `prefers-reduced-motion`-guarded** — each animation utility has a `@media (prefers-reduced-motion: reduce)` no-op.
- Every new/changed component keeps a Storybook story and a Vitest render test.
- Components stay pure presentation (no Tauri/jotai/router/RQ imports) — the package boundary from PR #14.
- All new components declare `: JSX.Element` and `import type { JSX } from "react"` (React 19 dropped the global namespace) — the convention set in PR #16.
- `just ci` green before PR. Conventional Commits. PR references #17.

---

### Task 1: Motion vocabulary (CSS utilities)

**Files:**
- Modify: `packages/ui/src/styles.css` (add keyframes + utilities, reduced-motion guards)

**Interfaces:**
- Produces CSS classes consumed by later tasks: `.animate-puppet-sway`, `.animate-string-tug`, `.stage-reveal`, `.stage-enter`.

- [ ] **Step 1: Add the motion layer.** Append to `packages/ui/src/styles.css`:

```css
/* --- Motion vocabulary (theater × kathputli). Theme-agnostic: currentColor +
   semantic tokens only. All guarded by prefers-reduced-motion. --- */
@keyframes puppet-sway {
  0%, 100% { transform: rotate(-3deg); }
  50% { transform: rotate(3deg); }
}
@keyframes string-tug {
  0%, 100% { transform: translateY(0); }
  50% { transform: translateY(2px); }
}
@keyframes stage-reveal {
  from { opacity: 0; transform: translateY(6px) scale(0.99); }
  to { opacity: 1; transform: translateY(0) scale(1); }
}

.animate-puppet-sway { transform-origin: top center; animation: puppet-sway 2.4s ease-in-out infinite; }
.animate-string-tug { animation: string-tug 1.6s ease-in-out infinite; }
.stage-reveal { animation: stage-reveal 320ms cubic-bezier(0.2, 0.8, 0.2, 1) both; }
.stage-enter { animation: stage-reveal 220ms ease-out both; }

@media (prefers-reduced-motion: reduce) {
  .animate-puppet-sway,
  .animate-string-tug,
  .stage-reveal,
  .stage-enter { animation: none; }
}
```

- [ ] **Step 2: Verify the UI package still builds (Storybook picks up CSS).**

Run: `pnpm --filter @manch/ui build 2>&1 | tail -10`
Expected: builds clean (or no build script — then `just lint` clean).

- [ ] **Step 3: Commit.**

```bash
git add packages/ui/src/styles.css
git commit -m "feat(ui): theme-agnostic motion vocabulary (puppet sway, string tug, stage reveal)"
```

---

### Task 2: `PuppetLoader` primitive

**Files:**
- Create: `packages/ui/src/primitives/PuppetLoader.tsx`
- Create: `packages/ui/src/primitives/PuppetLoader.test.tsx`
- Create: `packages/ui/src/primitives/PuppetLoader.stories.tsx`
- Modify: `packages/ui/src/index.ts` (export)

**Interfaces:**
- Produces: `PuppetLoader({ size?, label? }: { size?: number; label?: string }): JSX.Element` — an accessible (`role="status"`) kathputli marionette on strings, `currentColor`, CSS-animated.

- [ ] **Step 1: Write the failing test.** Create `packages/ui/src/primitives/PuppetLoader.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { PuppetLoader } from "./PuppetLoader";

describe("PuppetLoader", () => {
  it("exposes an accessible status with the given label", () => {
    render(<PuppetLoader label="Thinking…" />);
    const status = screen.getByRole("status");
    expect(status).toBeTruthy();
    expect(status.getAttribute("aria-label")).toBe("Thinking…");
  });

  it("defaults the label when none is given", () => {
    render(<PuppetLoader />);
    expect(screen.getByRole("status").getAttribute("aria-label")).toBe("Loading");
  });
});
```

- [ ] **Step 2: Run test to verify it fails.**

Run: `pnpm --filter @manch/ui test -- --run PuppetLoader 2>&1 | tail -15`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement the component.** Create `packages/ui/src/primitives/PuppetLoader.tsx`:

```tsx
import type { JSX } from "react";

export interface PuppetLoaderProps {
  /** Pixel size of the puppet glyph. Default 48. */
  size?: number;
  /** Accessible label announced to screen readers. Default "Loading". */
  label?: string;
}

/**
 * Kathputli (Rajasthani/Gujarati marionette) loading state. Theme-agnostic:
 * strokes/fills use `currentColor`, motion via CSS utilities (Task 1), all
 * reduced-motion-guarded. The strings sway; the puppet tugs gently below.
 */
export function PuppetLoader({ size = 48, label = "Loading" }: PuppetLoaderProps): JSX.Element {
  return (
    <div role="status" aria-label={label} className="inline-flex flex-col items-center text-primary">
      <svg
        width={size}
        height={size * 1.4}
        viewBox="0 0 48 68"
        fill="none"
        aria-hidden
        className="origin-top"
      >
        {/* control bar */}
        <line x1="8" y1="4" x2="40" y2="4" stroke="currentColor" strokeWidth="2" strokeLinecap="round" opacity="0.6" />
        {/* strings sway from the bar */}
        <g className="animate-puppet-sway">
          <line x1="14" y1="4" x2="18" y2="26" stroke="currentColor" strokeWidth="1" opacity="0.5" />
          <line x1="34" y1="4" x2="30" y2="26" stroke="currentColor" strokeWidth="1" opacity="0.5" />
          {/* puppet body tugs on the strings */}
          <g className="animate-string-tug">
            <circle cx="24" cy="32" r="6" fill="currentColor" />
            <rect x="20" y="38" width="8" height="14" rx="3" fill="currentColor" />
            <line x1="20" y1="42" x2="12" y2="50" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
            <line x1="28" y1="42" x2="36" y2="50" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
            <line x1="22" y1="52" x2="20" y2="62" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
            <line x1="26" y1="52" x2="28" y2="62" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
          </g>
        </g>
      </svg>
      <span className="mt-1 text-xs text-base-content/60">{label}</span>
    </div>
  );
}
```

- [ ] **Step 4: Export it.** In `packages/ui/src/index.ts`, add `export { PuppetLoader } from "./primitives/PuppetLoader";` (match the existing export style in that file).

- [ ] **Step 5: Add the story.** Create `packages/ui/src/primitives/PuppetLoader.stories.tsx` (match the format of a neighboring `*.stories.tsx`, e.g. `StatusDot.stories.tsx`):

```tsx
import type { Meta, StoryObj } from "@storybook/react";
import { PuppetLoader } from "./PuppetLoader";

const meta: Meta<typeof PuppetLoader> = { title: "primitives/PuppetLoader", component: PuppetLoader };
export default meta;
type Story = StoryObj<typeof PuppetLoader>;

export const Default: Story = {};
export const WithLabel: Story = { args: { label: "Consulting the AIs…" } };
export const Large: Story = { args: { size: 96, label: "Streaming" } };
```

- [ ] **Step 6: Run test to verify it passes.**

Run: `pnpm --filter @manch/ui test -- --run PuppetLoader 2>&1 | tail -15`
Expected: PASS (both tests).

- [ ] **Step 7: Commit.**

```bash
git add packages/ui/src/primitives/PuppetLoader.tsx packages/ui/src/primitives/PuppetLoader.test.tsx packages/ui/src/primitives/PuppetLoader.stories.tsx packages/ui/src/index.ts
git commit -m "feat(ui): PuppetLoader kathputli loading primitive"
```

---

### Task 3: Adopt `PuppetLoader` in busy/loading states

**Files:**
- Modify: `apps/desktop/src/containers/Stage.tsx` (streaming indicator)
- Modify: `apps/desktop/src/containers/TeamDetailPage.tsx` (loading `EmptyState glyph="⏳"`)
- Modify: any other `glyph="⏳"` loading states (grep below)

**Interfaces:**
- Consumes: `PuppetLoader` from `@manch/ui`.

- [ ] **Step 1: Find the loading states.**

Run: `grep -rn '⏳\|isStreaming\|streamingText' apps/desktop/src --include=*.tsx | grep -v .test.`
Expected: lists the Stage streaming render and the `EmptyState glyph="⏳"` loading placeholders (e.g. `TeamDetailPage.tsx`, and any `*.isLoading` branches).

- [ ] **Step 2: Swap loading `EmptyState`s.** For each `<EmptyState glyph="⏳" title="Loading…" />`, replace with a centered `PuppetLoader`:

```tsx
// before: return <EmptyState glyph="⏳" title="Loading…" />;
return (
  <div className="flex h-full items-center justify-center">
    <PuppetLoader label="Loading…" />
  </div>
);
```

Add `import { PuppetLoader } from "@manch/ui";` to each touched file (or extend the existing `@manch/ui` import).

- [ ] **Step 3: Use it as the Stage streaming indicator.** In `Stage.tsx`, where `isStreaming` drives the live transcript, render a small `PuppetLoader` near the streaming text (keep the existing `Transcript`; add the loader as the busy affordance). Keep it unobtrusive: `size={32}`.

- [ ] **Step 4: Typecheck + tests.**

Run: `just lint && pnpm --filter @manch/desktop test -- --run 2>&1 | tail -15`
Expected: typecheck clean; desktop tests PASS. If a test asserted the `⏳` glyph or `"Loading…"` title text, update it to assert `getByRole("status")` / the loader label.

- [ ] **Step 5: Commit.**

```bash
git add apps/desktop/src
git commit -m "feat(desktop): adopt PuppetLoader for streaming + loading states"
```

---

### Task 4: Hero-surface nuance pass

**Files:**
- Modify: `packages/ui/src/primitives/Spotlight.tsx` (add reveal motion)
- Modify: `packages/ui/src/stage/StageHeader.tsx`, `Transcript.tsx`, `Message.tsx`, `GreenRoomView.tsx` (entrance motion + spacing/type nuance)

**Interfaces:**
- Consumes: motion utilities from Task 1.

- [ ] **Step 1: Add entrance motion to messages.** In `Message.tsx`, add `stage-enter` to the message wrapper's className (so each message reveals). Verify the existing `Message.test.tsx` still passes (class additions don't change asserted text/role).

- [ ] **Step 2: Reveal the active spotlight.** In `Spotlight.tsx`, when `active`, add `stage-reveal` to the wrapper className alongside the existing ring/gradient. Keep `data-testid="spotlight"` and `data-active` (the test depends on them).

- [ ] **Step 3: Nuance pass (spacing / type / elevation).** On `StageHeader.tsx`, `Transcript.tsx`, `GreenRoomView.tsx`: tune spacing rhythm, weight contrast (e.g. `font-semibold` headers vs `text-base-content/70` secondary), and elevation via daisyUI `shadow-*` tokens. **Semantic tokens only** — no hex/rgb. Update each component's story if it needs a new state to show the change; do not restructure layout.

- [ ] **Step 4: Grep for hardcoded colors (guard the constraint).**

Run: `grep -rnE '#[0-9a-fA-F]{3,6}\b|rgb\(|rgba\(' packages/ui/src --include=*.tsx | grep -v .test. | grep -v .stories.`
Expected: no matches (the pre-existing `var(--color-primary)` gradient in `Spotlight.tsx` uses a token, which is fine; a raw hex is not).

- [ ] **Step 5: Typecheck + tests.**

Run: `just lint && pnpm --filter @manch/ui test -- --run 2>&1 | tail -15`
Expected: typecheck clean; all `@manch/ui` tests PASS.

- [ ] **Step 6: Commit.**

```bash
git add packages/ui/src
git commit -m "feat(ui): motion + structural nuance pass on hero surfaces"
```

---

### Task 5: Expand theme picker to all daisyUI themes

**Files:**
- Modify: `apps/desktop/src/styles.css` (`themes: all`)
- Modify: `apps/desktop/src/store/atoms.ts` (`THEMES` array → all built-ins)
- Modify: `packages/ui/src/settings/ThemePicker.tsx` (scrollable for 35 entries)
- Modify: `apps/desktop/src/containers/SettingsPage.test.tsx` (assert a broader count if it checks theme entries)

**Interfaces:**
- Consumes: `ThemePicker` (unchanged props: `themes: string[]`).

- [ ] **Step 1: Enable all themes.** In `apps/desktop/src/styles.css`, replace the `@plugin "daisyui"` block with:

```css
@plugin "daisyui" {
  themes: all;
}
```

(`dark` stays the effective default: `__root.tsx` sets `data-theme` from `themeAtom`, whose stored default is `"dark"`.)

- [ ] **Step 2: List every theme in `THEMES`.** In `apps/desktop/src/store/atoms.ts`, replace the `THEMES` const with the full daisyUI v5 set (35 themes; `dark` first so it's the picker's leading option):

```ts
export const THEMES = [
  "dark", "light", "cupcake", "bumblebee", "emerald", "corporate", "synthwave",
  "retro", "cyberpunk", "valentine", "halloween", "garden", "forest", "aqua",
  "lofi", "pastel", "fantasy", "wireframe", "black", "luxury", "dracula", "cmyk",
  "autumn", "business", "acid", "lemonade", "night", "coffee", "winter", "dim",
  "nord", "sunset", "caramellatte", "abyss", "silk",
];
```

- [ ] **Step 3: Make the picker scroll.** In `packages/ui/src/settings/ThemePicker.tsx`, wrap the `fieldset` grid so 35 entries stay usable — add `max-h-64 overflow-y-auto pr-1` to the `fieldset` className (keep the `grid grid-cols-2 sm:grid-cols-3 gap-2`). No prop/interface change.

- [ ] **Step 4: Update the theme-switch test if needed.** `SettingsPage.test.tsx` clicks the `dracula` radio — still present, so it passes unchanged. If any test asserted exactly 5 theme radios, update the count. Run:

Run: `pnpm --filter @manch/desktop test -- --run SettingsPage 2>&1 | tail -15`
Expected: PASS (the `dracula` radio still resolves).

- [ ] **Step 5: Typecheck + UI test.**

Run: `just lint && pnpm --filter @manch/ui test -- --run ThemePicker 2>&1 | tail -10`
Expected: typecheck clean; ThemePicker test PASS.

- [ ] **Step 6: Commit.**

```bash
git add apps/desktop/src/styles.css apps/desktop/src/store/atoms.ts packages/ui/src/settings/ThemePicker.tsx apps/desktop/src/containers/SettingsPage.test.tsx
git commit -m "feat(desktop): expose all daisyUI themes in the picker (5 → 35)"
```

---

### Task 6: Storybook review + full CI

- [ ] **Step 1: Full CI gate.**

Run: `just ci 2>&1 | tail -20`
Expected: `✓ CI checks passed`.

- [ ] **Step 2: Storybook eyeball (user sign-off).**

Run: `pnpm --filter @manch/ui storybook`
Review: `PuppetLoader` (Default/WithLabel/Large); hero surfaces (Stage/StageHeader/Transcript/GreenRoom) and `Spotlight` across ≥3 contrasting themes (dark, cupcake, dracula) — confirm nothing looks broken on any theme, motion reads as "puppet/stage", and reduced-motion (OS setting) halts animation. **This is the acceptance gate.**

- [ ] **Step 3: Address sign-off feedback.** Apply any tweaks the review surfaces (commit each as `fix(ui): …` / `style(ui): …`), re-run `just ci`.

- [ ] **Step 4: Push + PR.**

```bash
git push -u origin <branch>
gh pr create --base main --title "feat(ui): theater × kathputli visual polish + 35 themes — #17 part B" --body "…Refs #17"
```

## Self-review notes

- Spec coverage: motion vocabulary (T1); kathputli loader (T2) + adoption (T3); hero-surface nuance (T4); theme expansion 5→35 (T5); Storybook sign-off + CI (T6). All Part-B goals covered.
- Subjectivity is handled by making the objective gates explicit (stories, no hardcoded colors via the T4 grep, reduced-motion, `just ci`) and routing the aesthetic judgment to a single Storybook sign-off step (T6.2).
- daisyUI v5 ships 35 built-in themes; the spec's "32" was approximate — `themes: all` + the full list is authoritative (confirmed via daisyUI docs).
