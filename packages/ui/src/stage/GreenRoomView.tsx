import type { JSX } from "react";
import type { ConversationSummary } from "../types";

export function GreenRoomView({
  conversations, activeId, onSelect, onNew, onOpenSettings,
}: {
  conversations: ConversationSummary[];
  activeId: string | null;
  onSelect: (id: string) => void;
  onNew: () => void;
  onOpenSettings: () => void;
}): JSX.Element {
  return (
    <div className="flex h-full flex-col">
      <div className="p-3">
        <button className="btn btn-primary btn-sm w-full font-semibold" onClick={onNew}>+ New</button>
      </div>
      <ul className="menu min-h-0 flex-1 flex-nowrap overflow-y-auto px-2">
        {conversations.map((c) => (
          <li key={c.id} data-active={c.id === activeId}>
            <button className={c.id === activeId ? "active font-semibold" : "font-medium text-base-content/70"} onClick={() => onSelect(c.id)}>
              {c.title}
            </button>
          </li>
        ))}
      </ul>
      <div className="border-t border-base-300 p-3">
        <button className="btn btn-ghost btn-sm w-full justify-start text-base-content/70" onClick={onOpenSettings}>⚙ Keys</button>
      </div>
    </div>
  );
}
