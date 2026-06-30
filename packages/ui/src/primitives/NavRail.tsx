export interface NavItem {
  id: string;
  label: string;
  glyph: string;
}

export interface NavRailProps {
  items: NavItem[];
  activeId: string;
  onSelect: (id: string) => void;
}

export function NavRail({ items, activeId, onSelect }: NavRailProps) {
  return (
    <nav role="tablist" aria-orientation="vertical" className="flex h-full flex-col items-center gap-1 bg-base-200 py-3">
      {items.map((item) => {
        const active = item.id === activeId;
        return (
          <button
            key={item.id}
            role="tab"
            aria-current={active ? "page" : undefined}
            aria-label={item.label}
            title={item.label}
            onClick={() => onSelect(item.id)}
            className={`btn btn-square btn-ghost text-xl ${active ? "btn-active text-primary" : "text-base-content/70"}`}
          >
            <span aria-hidden>{item.glyph}</span>
          </button>
        );
      })}
    </nav>
  );
}
