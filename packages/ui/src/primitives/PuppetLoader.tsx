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
      {/* handlebar mustache — tips curl up, dip under the nose */}
      <path d="M16 34 Q14 30 18 31 Q22 32 24 35 Q26 32 30 31 Q34 30 32 34 Q29 34 27 33 Q24 35 21 33 Q19 34 16 34 Z" className="fill-neutral" />
      {/* turban + finial jewel */}
      <path d="M15 26 Q24 10 33 26 Z" className="fill-primary" />
      <circle cx="24" cy="17" r="2.4" className="fill-primary" />
      <circle cx="24" cy="17" r="1.1" className="fill-accent" />
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
