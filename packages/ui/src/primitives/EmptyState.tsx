import type { JSX } from "react";
export interface EmptyStateProps {
  glyph?: string;
  title: string;
  description?: string;
  action?: { label: string; onClick: () => void };
}

export function EmptyState({ glyph, title, description, action }: EmptyStateProps): JSX.Element {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 p-8 text-center">
      {glyph && <div className="text-4xl opacity-60" aria-hidden>{glyph}</div>}
      <h2 className="text-lg font-semibold text-base-content">{title}</h2>
      {description && <p className="max-w-sm text-sm text-base-content/70">{description}</p>}
      {action && (
        <button className="btn btn-primary btn-sm mt-2" onClick={action.onClick}>{action.label}</button>
      )}
    </div>
  );
}
