import type { JSX } from "react";
export interface SearchResultView { kind: string; id: string; title: string; snippet: string }
export interface SearchResultsProps { results: SearchResultView[]; onOpen: (kind: string, id: string) => void }
export function SearchResults({ results, onOpen }: SearchResultsProps): JSX.Element {
  return (
    <ul className="space-y-2">
      {results.map((r) => (
        <li key={`${r.kind}:${r.id}`}>
          <button aria-label={r.title} onClick={() => onOpen(r.kind, r.id)}
            className="w-full rounded-box border border-base-300 px-3 py-2 text-left hover:bg-base-200">
            <span className="badge badge-ghost badge-sm mr-2">{r.kind}</span>
            <span className="font-medium">{r.title}</span>
            {r.snippet && <span className="block text-sm text-base-content/60">{r.snippet}</span>}
          </button>
        </li>
      ))}
    </ul>
  );
}
