import type { JSX } from "react";
export interface SearchBarProps { value: string; onChange: (q: string) => void; onSubmit: () => void }
export function SearchBar({ value, onChange, onSubmit }: SearchBarProps): JSX.Element {
  return (
    <form role="search" onSubmit={(e) => { e.preventDefault(); onSubmit(); }} className="flex gap-2">
      <input role="searchbox" aria-label="search" className="input input-bordered input-sm flex-1"
        value={value} onChange={(e) => onChange(e.target.value)} placeholder="Search teams, schedules…" />
      <button type="submit" className="btn btn-primary btn-sm">Search</button>
    </form>
  );
}
