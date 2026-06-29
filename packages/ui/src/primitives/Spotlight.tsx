import type { ReactNode } from "react";

export function Spotlight({ active, children }: { active: boolean; children: ReactNode }) {
  return (
    <div
      data-testid="spotlight"
      data-active={active}
      className={active
        ? "rounded-box bg-[radial-gradient(120%_120%_at_50%_0%,theme(colors.primary/12%),transparent_60%)] ring-1 ring-primary/20"
        : "opacity-90"}
    >
      {children}
    </div>
  );
}
