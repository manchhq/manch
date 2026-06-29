export function IconRail({
  items,
}: {
  items: { id: string; glyph: string; label: string; onClick: () => void }[];
}) {
  return (
    <nav className="flex flex-col items-center gap-2 py-2">
      {items.map((it) => (
        <button key={it.id} className="btn btn-ghost btn-sm" aria-label={it.label} title={it.label} onClick={it.onClick}>
          {it.glyph}
        </button>
      ))}
    </nav>
  );
}
