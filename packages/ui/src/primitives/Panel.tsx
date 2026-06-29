import type { ReactNode } from "react";

export function Panel({
  title, side, collapsed, onToggle, children,
}: {
  title: string;
  side: "left" | "right";
  collapsed: boolean;
  onToggle: () => void;
  children: ReactNode;
}) {
  if (collapsed) {
    return (
      <aside className="flex h-full w-10 flex-col items-center border-base-300 bg-base-200 py-3"
             data-side={side} data-collapsed="true">
        <button className="btn btn-ghost btn-xs" aria-label="toggle panel" onClick={onToggle}>
          {side === "left" ? "»" : "«"}
        </button>
      </aside>
    );
  }
  return (
    <aside className="flex h-full flex-col bg-base-200" data-side={side} data-collapsed="false">
      <header className="flex items-center justify-between border-b border-base-300 px-3 py-2">
        <span className="text-xs font-semibold uppercase tracking-wider text-base-content/60">{title}</span>
        <button className="btn btn-ghost btn-xs" aria-label="collapse panel" onClick={onToggle}>
          {side === "left" ? "«" : "»"}
        </button>
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto">{children}</div>
    </aside>
  );
}
