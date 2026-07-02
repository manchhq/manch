# PuppetLoader: authentic Rajasthani/Gujarati kathputli redesign

- **Date:** 2026-07-02
- **Status:** Approved (design), pending implementation plan
- **Issue:** #20 (follow-up from #17 Part B / #19)
- **Scope:** one component — `packages/ui/src/primitives/PuppetLoader.tsx` (+ its test/stories)

## Motivation

The v1 `PuppetLoader` reads as a **generic Western marionette** (a flat single-`currentColor` line-and-circle figure with head/torso/two arms/two legs). It should be a recognizable **Rajasthani/Gujarati kathputli** — a signature "theatre × kathputli" touch — while staying **theme-agnostic** (daisyUI owns color; no hardcoded hex).

## What stays (do not regress)

- Component API `size` (default 48) and `label` (default "Loading").
- Accessibility: `role="status"` + `aria-label={label}`, SVG `aria-hidden`, visible `<span>` label.
- Motion: `.animate-puppet-sway` (strings) + `.animate-string-tug` (body), already `prefers-reduced-motion`-guarded (from #17 Part B). The shared **control bar + two strings** stay; only the puppet **body** swaps per variant.

## API change

Add one prop:

```ts
export interface PuppetLoaderProps {
  size?: number;            // default 48
  label?: string;           // default "Loading"
  variant?: "male" | "female"; // default "male"
}
```

## Silhouettes (SVG anatomy — legless, identity carried by the silhouette)

Both hang from the shared control bar + strings and tug via `.animate-string-tug`.

- **male:** pagdi/**turban** dome → **large eyes** → prominent handlebar **mustache** → elongated **torso** with a center motif → **flared coat/dhoti base** (no legs).
- **female:** **veil/head** → large eyes → small torso → elaborate flared **ghagra-lehenga** (triangle skirt, no legs) → a row of **dot/paisley ornaments** along the hem.

Both must read at the default 48px size.

## Color: multiple daisyUI semantic tokens (no hex)

v1's single `currentColor` reads flat. Use several theme tokens via Tailwind `fill-*`/`stroke-*` so the puppet is multi-colored yet still derives entirely from the active theme (adapts across all 35). Mapping (exact tokens tuned during the Storybook eyeball):

| Part | Token |
|------|-------|
| Turban / veil | `fill-primary` |
| Torso / coat / skirt (ghagra) | `fill-secondary` |
| Mustache / ornament dots / center motif | `fill-accent` |
| Face | `fill-warning`; eyes = small `fill-base-100` whites + `fill-neutral` pupils |
| Control bar + strings | `stroke-base-content` at low opacity (subtle, as v1) |

**Hard rule:** no fixed `hex`/`rgb`/`hsl`/`oklch` anywhere in the component — only semantic tokens.

## Testing & acceptance

- `PuppetLoader.test.tsx`: renders each variant (`male`, `female`); `role="status"` present with the label; SVG is `aria-hidden`; default variant renders. Assertions verify behavior, not implementation detail.
- `PuppetLoader.stories.tsx`: a story per variant (mirroring the existing story), for the Storybook eyeball.
- **Acceptance:** Storybook eyeball across contrasting themes — **dark / cupcake / dracula** — both variants read as a vibrant kathputli and stay legible. (Subjective sign-off by the user, per the issue.)
- Gates: no hardcoded color values in the component (grep for `#`/`rgb`/`hsl`/`oklch`); `just lint` + `just test-js` green.

## Out of scope

- No new motion vocabulary (reuse existing utilities).
- No changes to other components or the theme set.
- Animation/aesthetic fine-tuning beyond "reads as kathputli, legible across the three themes" is iterative polish, not a blocker.
