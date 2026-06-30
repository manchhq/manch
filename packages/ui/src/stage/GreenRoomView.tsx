import type { ConversationSummary } from "../types";

export function GreenRoomView({
  conversations, activeId, onSelect, onNew, onOpenSettings,
}: {
  conversations: ConversationSummary[];
  activeId: string | null;
  onSelect: (id: string) => void;
  onNew: () => void;
  onOpenSettings: () => void;
}) {
  return (
    <div className="flex h-full flex-col">
      <div className="p-2">
        <button className="btn btn-primary btn-sm w-full" onClick={onNew}>+ New</button>
      </div>
      <ul className="menu min-h-0 flex-1 flex-nowrap overflow-y-auto px-2">
        {conversations.map((c) => (
          <li key={c.id} data-active={c.id === activeId}>
            <button className={c.id === activeId ? "active" : ""} onClick={() => onSelect(c.id)}>
              {c.title}
            </button>
          </li>
        ))}
      </ul>
      <div className="border-t border-base-300 p-2">
        <button className="btn btn-ghost btn-sm w-full justify-start" onClick={onOpenSettings}>⚙ Keys</button>
      </div>
    </div>
  );
}
