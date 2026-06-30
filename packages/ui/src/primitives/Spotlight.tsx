import type { ReactNode } from "react";

export function Spotlight({ active, children }: { active: boolean; children: ReactNode }) {
  return (
    <div
      data-testid="spotlight"
      data-active={active}
      className={active ? "rounded-box ring-1 ring-primary/20" : "opacity-90"}
      style={active ? { background: "radial-gradient(60% 60% at 50% 0%, var(--color-primary) 0%, transparent 70%)" } : undefined}
    >
      {children}
    </div>
  );
}
