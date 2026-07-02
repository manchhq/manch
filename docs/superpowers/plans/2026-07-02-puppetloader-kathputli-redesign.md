# PuppetLoader Kathputli Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Redesign `PuppetLoader` from a flat Western marionette into a recognizable, vibrant Rajasthani/Gujarati kathputli with a `male`/`female` variant, staying theme-agnostic.

**Architecture:** Single React component in `@manch/ui`. Keep the shared wrapper (control bar + strings + motion + a11y); swap only the puppet body by `variant`, drawn with multiple daisyUI semantic tokens (`fill-*`/`stroke-*`) instead of one `currentColor`.

**Tech Stack:** React 19 + TypeScript, Vitest + Testing Library, Storybook, Tailwind + daisyUI (semantic color tokens).

**Spec:** `docs/superpowers/specs/2026-07-02-puppetloader-kathputli-redesign-design.md` · **Issue:** #20

## Global Constraints

- **No hardcoded color values** in the component — no `#hex`, `rgb(`, `hsl(`, `oklch(`. Only daisyUI semantic tokens via Tailwind `fill-*`/`stroke-*`/`text-*` classes.
- **Keep the API + a11y + motion:** props `size` (default 48), `label` (default "Loading"); `role="status"` + `aria-label={label}`; SVG `aria-hidden`; visible `<span>` label; motion via existing `.animate-puppet-sway` (strings) + `.animate-string-tug` (body), already reduced-motion-guarded. Do not add new motion utilities.
- **New prop:** `variant?: "male" | "female"`, default `"male"`.
- Token mapping: turban/veil `fill-primary`; torso/coat/skirt `fill-secondary`; mustache/ornament/motif `fill-accent`; face `fill-warning` with eyes `fill-base-100` whites + `fill-neutral` pupils; control bar + strings `stroke-base-content` low-opacity. (Exact shades are tuned during the Storybook eyeball; the SVG below is the starting point.)
- Gates: `just lint` + `just test-js` green.
- Acceptance: Storybook eyeball across **dark / cupcake / dracula**, both variants (subjective sign-off — not automatable).

## File Structure

- Modify: `packages/ui/src/primitives/PuppetLoader.tsx` — the component + `variant` prop + two internal body sub-components (`MaleBody`/`FemaleBody`, not exported).
- Modify: `packages/ui/src/primitives/PuppetLoader.test.tsx` — variant + a11y assertions.
- Modify: `packages/ui/src/primitives/PuppetLoader.stories.tsx` — `Male`/`Female` stories.
- No barrel change (`packages/ui/src/index.ts` already exports `PuppetLoader`; the new prop is additive).

---

### Task 1: Kathputli redesign with male/female variants

**Files:**
- Modify: `packages/ui/src/primitives/PuppetLoader.tsx`
- Modify: `packages/ui/src/primitives/PuppetLoader.test.tsx`
- Modify: `packages/ui/src/primitives/PuppetLoader.stories.tsx`

**Interfaces:**
- Produces: `PuppetLoader({ size?, label?, variant? }: PuppetLoaderProps)` with `variant?: "male" | "female"` (default `"male"`). Unchanged: `role="status"`, `aria-label`, `aria-hidden` SVG.

- [ ] **Step 1: Write the failing tests**

Replace the body of `packages/ui/src/primitives/PuppetLoader.test.tsx` with (keeps the two existing a11y tests, adds variant + aria-hidden coverage):

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

  it("hides the decorative svg from assistive tech", () => {
    const { container } = render(<PuppetLoader />);
    const svg = container.querySelector("svg");
    expect(svg).toBeTruthy();
    expect(svg?.getAttribute("aria-hidden")).toBe("true");
  });

  it("renders both variants without error and keeps the status role", () => {
    const male = render(<PuppetLoader variant="male" label="M" />);
    expect(male.getByRole("status").getAttribute("aria-label")).toBe("M");
    const female = render(<PuppetLoader variant="female" label="F" />);
    expect(female.getByRole("status").getAttribute("aria-label")).toBe("F");
  });

  it("uses daisyUI semantic color classes (no flat single color)", () => {
    const { container } = render(<PuppetLoader variant="male" />);
    const svgHtml = container.querySelector("svg")?.outerHTML ?? "";
    expect(svgHtml).toContain("fill-primary");
    expect(svgHtml).toContain("fill-secondary");
    expect(svgHtml).toContain("fill-accent");
    // no hardcoded colors
    expect(svgHtml).not.toMatch(/#[0-9a-fA-F]{3,6}\b/);
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `pnpm --filter @manch/ui test -- PuppetLoader`
Expected: FAIL — the new `variant` prop / `aria-hidden="true"` / `fill-primary` assertions don't hold against the current single-`currentColor` component.

- [ ] **Step 3: Rewrite the component**

Replace the entire contents of `packages/ui/src/primitives/PuppetLoader.tsx` with:

```tsx
import type { JSX } from "react";

export interface PuppetLoaderProps {
  /** Pixel size of the puppet glyph (width). Height is 1.5×. Default 48. */
  size?: number;
  /** Accessible label announced to screen readers. Default "Loading". */
  label?: string;
  /** Kathputli persona. Default "male". */
  variant?: "male" | "female";
}

/**
 * Kathputli (Rajasthani/Gujarati marionette) loading state. Theme-agnostic:
 * every part uses a daisyUI semantic token (`fill-*`/`stroke-*`), so the puppet
 * is multi-colored yet adapts across all themes — no hardcoded hex. The shared
 * control bar + strings sway (`.animate-puppet-sway`); the body tugs below
 * (`.animate-string-tug`); both motions are reduced-motion-guarded (see styles.css).
 */
export function PuppetLoader({
  size = 48,
  label = "Loading",
  variant = "male",
}: PuppetLoaderProps): JSX.Element {
  return (
    <div role="status" aria-label={label} className="inline-flex flex-col items-center">
      <svg
        width={size}
        height={size * 1.5}
        viewBox="0 0 48 72"
        fill="none"
        aria-hidden="true"
        className="origin-top"
      >
        {/* control bar */}
        <line x1="8" y1="4" x2="40" y2="4" className="stroke-base-content" strokeWidth="2" strokeLinecap="round" opacity="0.5" />
        <g className="animate-puppet-sway">
          {/* strings from the bar to the shoulders */}
          <line x1="15" y1="4" x2="19" y2="24" className="stroke-base-content" strokeWidth="1" opacity="0.4" />
          <line x1="33" y1="4" x2="29" y2="24" className="stroke-base-content" strokeWidth="1" opacity="0.4" />
          <g className="animate-string-tug">
            {variant === "female" ? <FemaleBody /> : <MaleBody />}
          </g>
        </g>
      </svg>
      <span className="mt-1 text-xs text-base-content/60">{label}</span>
    </div>
  );
}

/** Turban + handlebar mustache + elongated torso + flared dhoti base (no legs). */
function MaleBody(): JSX.Element {
  return (
    <g>
      {/* thin puppet arms */}
      <path d="M21 40 L12 50" className="stroke-primary" strokeWidth="2.5" strokeLinecap="round" />
      <path d="M27 40 L36 50" className="stroke-primary" strokeWidth="2.5" strokeLinecap="round" />
      {/* flared coat/dhoti base — no legs */}
      <path d="M19 50 L29 50 L35 66 L13 66 Z" className="fill-secondary" />
      <circle cx="17" cy="63" r="1.2" className="fill-accent" />
      <circle cx="24" cy="64" r="1.2" className="fill-accent" />
      <circle cx="31" cy="63" r="1.2" className="fill-accent" />
      {/* torso + chest motif */}
      <path d="M20 38 L28 38 L29 51 L19 51 Z" className="fill-secondary" />
      <circle cx="24" cy="44" r="1.6" className="fill-accent" />
      {/* face */}
      <circle cx="24" cy="31" r="7" className="fill-warning" />
      <circle cx="21" cy="30" r="1.6" className="fill-base-100" />
      <circle cx="27" cy="30" r="1.6" className="fill-base-100" />
      <circle cx="21" cy="30" r="0.8" className="fill-neutral" />
      <circle cx="27" cy="30" r="0.8" className="fill-neutral" />
      {/* handlebar mustache */}
      <path d="M18 34 Q24 37 30 34 Q27 33 24 34 Q21 33 18 34 Z" className="fill-accent" />
      {/* turban + jewel */}
      <path d="M15 26 Q24 10 33 26 Z" className="fill-primary" />
      <circle cx="24" cy="14" r="2" className="fill-primary" />
      <circle cx="24" cy="22" r="1.3" className="fill-accent" />
    </g>
  );
}

/** Veil + bindi + small torso + elaborate flared ghagra-lehenga (no legs). */
function FemaleBody(): JSX.Element {
  return (
    <g>
      {/* thin puppet arms */}
      <path d="M21 38 L13 48" className="stroke-primary" strokeWidth="2.5" strokeLinecap="round" />
      <path d="M27 38 L35 48" className="stroke-primary" strokeWidth="2.5" strokeLinecap="round" />
      {/* flared ghagra-lehenga — no legs */}
      <path d="M20 44 L28 44 L40 66 L8 66 Z" className="fill-secondary" />
      <circle cx="13" cy="63" r="1.2" className="fill-accent" />
      <circle cx="19" cy="64" r="1.2" className="fill-accent" />
      <circle cx="24" cy="64.5" r="1.2" className="fill-accent" />
      <circle cx="29" cy="64" r="1.2" className="fill-accent" />
      <circle cx="35" cy="63" r="1.2" className="fill-accent" />
      {/* torso */}
      <path d="M21 34 L27 34 L28 45 L20 45 Z" className="fill-secondary" />
      {/* face */}
      <circle cx="24" cy="29" r="6" className="fill-warning" />
      <circle cx="21.5" cy="28" r="1.4" className="fill-base-100" />
      <circle cx="26.5" cy="28" r="1.4" className="fill-base-100" />
      <circle cx="21.5" cy="28" r="0.7" className="fill-neutral" />
      <circle cx="26.5" cy="28" r="0.7" className="fill-neutral" />
      {/* bindi */}
      <circle cx="24" cy="24" r="1" className="fill-accent" />
      {/* veil / odhni */}
      <path d="M16 24 Q24 8 32 24 Q24 18 16 24 Z" className="fill-primary" />
    </g>
  );
}
```

- [ ] **Step 4: Update the stories**

Replace `packages/ui/src/primitives/PuppetLoader.stories.tsx` with:

```tsx
import type { Meta, StoryObj } from "@storybook/react";
import { PuppetLoader } from "./PuppetLoader";

const meta: Meta<typeof PuppetLoader> = { title: "primitives/PuppetLoader", component: PuppetLoader };
export default meta;
type Story = StoryObj<typeof PuppetLoader>;

export const Male: Story = { args: { variant: "male", label: "Consulting the AIs…" } };
export const Female: Story = { args: { variant: "female", label: "Consulting the AIs…" } };
export const Large: Story = { args: { size: 96, variant: "male", label: "Streaming" } };
export const WithLabel: Story = { args: { label: "Thinking…" } };
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `pnpm --filter @manch/ui test -- PuppetLoader`
Expected: PASS (all 5 tests).

- [ ] **Step 6: Verify no hardcoded colors**

Run: `rg -n "#[0-9a-fA-F]{3,6}\b|rgb\(|hsl\(|oklch\(" packages/ui/src/primitives/PuppetLoader.tsx`
Expected: no matches.

- [ ] **Step 7: Run the full JS gates**

Run: `just lint && just test-js`
Expected: PASS (typecheck clean; all UI + desktop JS tests green).

- [ ] **Step 8: Commit**

```bash
git add packages/ui/src/primitives/PuppetLoader.tsx packages/ui/src/primitives/PuppetLoader.test.tsx packages/ui/src/primitives/PuppetLoader.stories.tsx
git commit -m "feat(ui): redesign PuppetLoader as kathputli with male/female variants (#20)"
```

---

## Self-Review

**Spec coverage:** silhouettes (male turban+mustache / female ghagra) → Step 3 `MaleBody`/`FemaleBody`; `variant` prop default male → Step 3 signature; multi-token color (no hex) → Step 3 classes + Step 6 grep + Step 1 color test; kept API/a11y/motion → Step 3 (props, `role`/`aria-label`/`aria-hidden`, `.animate-puppet-sway`/`.animate-string-tug`); stories for eyeball → Step 4; gates → Step 7. ✅

**Placeholder scan:** none — full component/test/story code inline.

**Type consistency:** `PuppetLoaderProps` with `variant?: "male" | "female"` used consistently in component, tests (`variant="male"`/`"female"`), and stories. `MaleBody`/`FemaleBody` return `JSX.Element`, referenced only inside the component.

**Note (visual):** the SVG path coordinates are a first-cut starting point; the issue's acceptance is a subjective Storybook eyeball across dark/cupcake/dracula. Expect a round of coordinate/token tuning after the first render — that polish is in-scope for this task, not a new plan.
