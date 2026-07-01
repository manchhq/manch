import type { JSX } from "react";
import { useState } from "react";

export interface WorkspaceSettingsProps {
  workspaces: { id: string; name: string }[];
  onRename: (id: string, name: string) => void;
  onDelete: (id: string) => void;
}

export function WorkspaceSettings({ workspaces, onRename, onDelete }: WorkspaceSettingsProps): JSX.Element {
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
