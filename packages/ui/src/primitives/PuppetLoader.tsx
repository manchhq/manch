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
