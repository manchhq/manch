import type { JSX } from "react";

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

export function WorkspaceSwitcher({ workspaces, activeId, onSelect, onCreate }: WorkspaceSwitcherProps): JSX.Element {
  const active = workspaces.find((w) => w.id === activeId);
  return (
    <div className="dropdown">
      <button
        tabIndex={0}
        className="btn btn-ghost btn-sm gap-2"
        aria-label={`Current workspace: ${active?.name ?? "Select workspace"}`}
      >
        <span className="font-semibold">{active?.name ?? "Select workspace"}</span>
        <span aria-hidden>▾</span>
      </button>
      <ul tabIndex={0} className="menu dropdown-content z-10 mt-1 w-56 rounded-box bg-base-200 p-2 shadow">
        {workspaces.map((w) => (
          <li key={w.id}>
            <button role="menuitem" className={w.id === activeId ? "active" : ""} onClick={() => onSelect(w.id)}>{w.name}</button>
          </li>
        ))}
        <li className="border-t border-base-300 mt-1 pt-1">
          <button onClick={onCreate}>＋ New workspace</button>
        </li>
      </ul>
    </div>
  );
}
